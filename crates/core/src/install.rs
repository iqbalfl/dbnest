use std::io::Read;
use std::path::{Component, Path, PathBuf};

use crate::config::FileLock;
use crate::download::{download_with_progress, verify_sha256};
use crate::error::{Error, Result};
use crate::manifest::Artifact;
use crate::model::{EngineKind, InstallEvent};
use crate::paths::Paths;

pub struct Installer<'a> {
    paths: &'a Paths,
    client: reqwest::Client,
}

impl<'a> Installer<'a> {
    pub fn new(paths: &'a Paths) -> Self {
        Self {
            paths,
            client: reqwest::Client::new(),
        }
    }

    pub fn is_installed(&self, engine: EngineKind, version: &str) -> bool {
        is_installed(self.paths, engine, version)
    }

    /// Jalankan alur instalasi lengkap dari §6: lock → cek `.installed` →
    /// download → verifikasi sha256 → ekstrak (dengan proteksi path
    /// traversal) → `post_install` → rename atomik → tulis `.installed`.
    pub async fn ensure_installed(
        &self,
        engine: EngineKind,
        version: &str,
        artifact: &Artifact,
        post_install: impl Fn(&Path) -> Result<()>,
        mut on_event: impl FnMut(InstallEvent),
    ) -> Result<PathBuf> {
        let engine_dir = self.paths.engine_binaries_dir(engine.as_str());
        std::fs::create_dir_all(&engine_dir)?;
        let _lock = FileLock::acquire(&engine_dir.join(".lock"))?;

        let version_dir = self.paths.version_dir(engine.as_str(), version);
        if let Some(recorded_sha) = read_installed_sha(&version_dir) {
            if recorded_sha.eq_ignore_ascii_case(&artifact.sha256) {
                on_event(InstallEvent::Done);
                return Ok(version_dir);
            }
        }

        let file_name = artifact
            .url
            .rsplit('/')
            .next()
            .filter(|s| !s.is_empty())
            .unwrap_or("archive");
        std::fs::create_dir_all(self.paths.downloads_dir())?;
        let dest_part = self.paths.downloads_dir().join(format!("{file_name}.part"));

        let downloaded_sha =
            download_with_progress(&self.client, &artifact.url, &dest_part, &mut on_event).await?;

        on_event(InstallEvent::Verifying);
        if let Err(e) = verify_sha256(&downloaded_sha, &artifact.sha256) {
            let _ = std::fs::remove_file(&dest_part);
            on_event(InstallEvent::Failed {
                message: e.to_string(),
            });
            return Err(e);
        }
        let downloaded_path = self.paths.downloads_dir().join(file_name);
        std::fs::rename(&dest_part, &downloaded_path)?;

        on_event(InstallEvent::Extracting);
        std::fs::create_dir_all(self.paths.tmp_dir())?;
        let extract_tmp = self.paths.tmp_dir().join(uuid::Uuid::new_v4().to_string());
        let extract_result = extract_archive(
            &downloaded_path,
            &extract_tmp,
            &artifact.format,
            artifact.strip_components,
        )
        .and_then(|()| post_install(&extract_tmp));

        if let Err(e) = extract_result {
            let _ = std::fs::remove_dir_all(&extract_tmp);
            on_event(InstallEvent::Failed {
                message: e.to_string(),
            });
            return Err(e);
        }

        if let Some(parent) = version_dir.parent() {
            std::fs::create_dir_all(parent)?;
        }
        if version_dir.exists() {
            std::fs::remove_dir_all(&version_dir)?;
        }
        std::fs::rename(&extract_tmp, &version_dir)?;

        write_installed_marker(&version_dir, &artifact.sha256)?;
        on_event(InstallEvent::Done);
        Ok(version_dir)
    }
}

pub fn is_installed(paths: &Paths, engine: EngineKind, version: &str) -> bool {
    let version_dir = paths.version_dir(engine.as_str(), version);
    read_installed_sha(&version_dir).is_some()
}

fn read_installed_sha(version_dir: &Path) -> Option<String> {
    let marker = version_dir.join(".installed");
    let contents = std::fs::read_to_string(marker).ok()?;
    contents
        .lines()
        .find_map(|line| line.strip_prefix("sha256="))
        .map(|s| s.trim().to_string())
}

fn write_installed_marker(version_dir: &Path, sha256: &str) -> Result<()> {
    let installed_at = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default();
    let contents = format!("sha256={sha256}\ninstalled_at={installed_at}\n");
    let marker = version_dir.join(".installed");
    let tmp = version_dir.join(".installed.tmp");
    std::fs::write(&tmp, contents)?;
    std::fs::rename(&tmp, &marker)?;
    Ok(())
}

/// Ekstrak arsip `archive_path` (format `tar.gz` atau `tar.xz`) ke
/// `dest_dir`, menerapkan `strip_components` dan menolak entri path absolut
/// atau yang mengandung `..` (§6 langkah 6).
pub fn extract_archive(
    archive_path: &Path,
    dest_dir: &Path,
    format: &str,
    strip_components: u32,
) -> Result<()> {
    std::fs::create_dir_all(dest_dir)?;
    let file = std::fs::File::open(archive_path)?;
    match format {
        "tar.gz" | "tgz" => {
            let decoder = flate2::read::GzDecoder::new(file);
            extract_tar(decoder, dest_dir, strip_components)
        }
        "tar.xz" => {
            let decoder = xz2::read::XzDecoder::new(file);
            extract_tar(decoder, dest_dir, strip_components)
        }
        other => Err(Error::Other(format!(
            "format arsip tidak didukung: {other}"
        ))),
    }
}

fn extract_tar<R: Read>(reader: R, dest_dir: &Path, strip_components: u32) -> Result<()> {
    let mut archive = tar::Archive::new(reader);
    for entry in archive.entries()? {
        let mut entry = entry?;
        let raw_path = entry.path()?.into_owned();
        let safe_relative = safe_relative_path(&raw_path, strip_components)?;
        let Some(relative) = safe_relative else {
            continue; // entri di dalam strip_components yang dihapus (mis. folder root tarball)
        };

        let dest_path = dest_dir.join(&relative);
        if !dest_path.starts_with(dest_dir) {
            return Err(Error::Other(format!(
                "arsip berisi path yang keluar dari tujuan ekstraksi: {}",
                raw_path.display()
            )));
        }

        if entry.header().entry_type().is_dir() {
            std::fs::create_dir_all(&dest_path)?;
        } else if entry.header().entry_type() == tar::EntryType::Link {
            // Hard link: nama target di header relatif terhadap akar arsip,
            // sedangkan `Entry::unpack` menafsirkannya relatif terhadap
            // direktori kerja proses — jadi target diselesaikan sendiri, dengan
            // strip_components dan validasi yang sama seperti path entri.
            // PostgreSQL memakai hard link besar-besaran di share/timezone,
            // jadi tanpa ini tarball-nya gagal diekstrak.
            let raw_link = entry
                .link_name()?
                .ok_or_else(|| {
                    Error::Other(format!(
                        "hard link tanpa target di arsip: {}",
                        raw_path.display()
                    ))
                })?
                .into_owned();
            let link_relative =
                safe_relative_path(&raw_link, strip_components)?.ok_or_else(|| {
                    Error::Other(format!(
                        "hard link menunjuk ke luar arsip: {}",
                        raw_link.display()
                    ))
                })?;
            let link_target = dest_dir.join(&link_relative);
            if !link_target.starts_with(dest_dir) {
                return Err(Error::Other(format!(
                    "hard link menunjuk ke luar tujuan ekstraksi: {}",
                    raw_link.display()
                )));
            }
            if let Some(parent) = dest_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::hard_link(&link_target, &dest_path)?;
        } else {
            if let Some(parent) = dest_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            entry.unpack(&dest_path)?;
        }
    }
    Ok(())
}

/// Validasi sebuah entri arsip lalu terapkan `strip_components`.
/// Mengembalikan `None` jika entri habis dihapus oleh `strip_components`
/// (mis. entri untuk folder root itu sendiri).
fn safe_relative_path(path: &Path, strip_components: u32) -> Result<Option<PathBuf>> {
    if path.is_absolute() {
        return Err(Error::Other(format!(
            "arsip berisi path absolut: {}",
            path.display()
        )));
    }
    if path.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(Error::Other(format!(
            "arsip berisi path traversal: {}",
            path.display()
        )));
    }
    let stripped: PathBuf = path.components().skip(strip_components as usize).collect();
    if stripped.as_os_str().is_empty() {
        Ok(None)
    } else {
        Ok(Some(stripped))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Bangun tar.gz untuk tes. Path ditulis langsung ke field mentah header
    /// (bukan lewat `append_data`), supaya tes traversal di bawah bisa
    /// menyisipkan `..` walau `tar` crate sendiri menolaknya di API level
    /// tinggi. Ini mensimulasikan arsip yang dibuat tool lain / diserang.
    fn build_tar_gz(entries: &[(&str, &str)]) -> Vec<u8> {
        let mut tar_bytes = Vec::new();
        {
            let mut builder = tar::Builder::new(&mut tar_bytes);
            for (path, contents) in entries {
                let mut header = tar::Header::new_gnu();
                header.set_size(contents.len() as u64);
                header.set_mode(0o644);
                let name_bytes = path.as_bytes();
                header.as_gnu_mut().unwrap().name[..name_bytes.len()].copy_from_slice(name_bytes);
                header.set_cksum();
                builder.append(&header, contents.as_bytes()).unwrap();
            }
            builder.finish().unwrap();
        }
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(&tar_bytes).unwrap();
        encoder.finish().unwrap()
    }

    #[test]
    fn extract_archive_applies_strip_components() {
        let tmp = tempfile::tempdir().unwrap();
        let archive_path = tmp.path().join("archive.tar.gz");
        std::fs::write(
            &archive_path,
            build_tar_gz(&[("redis-7.4.0/bin/redis-server", "fake-binary")]),
        )
        .unwrap();

        let dest = tmp.path().join("dest");
        extract_archive(&archive_path, &dest, "tar.gz", 1).unwrap();

        assert!(dest.join("bin/redis-server").exists());
    }

    /// Membuat arsip berisi satu berkas biasa dan satu hard link ke berkas itu,
    /// dengan nama target relatif terhadap akar arsip — persis seperti tarball
    /// PostgreSQL di `share/timezone`.
    fn build_tar_gz_with_hard_link(
        file_path: &str,
        contents: &str,
        link_path: &str,
        link_target: &str,
    ) -> Vec<u8> {
        let mut tar_bytes = Vec::new();
        {
            let mut builder = tar::Builder::new(&mut tar_bytes);

            let mut header = tar::Header::new_gnu();
            header.set_size(contents.len() as u64);
            header.set_mode(0o644);
            header.set_entry_type(tar::EntryType::Regular);
            builder
                .append_data(&mut header, file_path, contents.as_bytes())
                .unwrap();

            let mut link_header = tar::Header::new_gnu();
            link_header.set_size(0);
            link_header.set_mode(0o644);
            link_header.set_entry_type(tar::EntryType::Link);
            link_header.set_link_name(link_target).unwrap();
            builder
                .append_data(&mut link_header, link_path, std::io::empty())
                .unwrap();

            builder.finish().unwrap();
        }
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(&tar_bytes).unwrap();
        encoder.finish().unwrap()
    }

    #[test]
    fn extract_archive_resolves_hard_links_after_strip_components() {
        let tmp = tempfile::tempdir().unwrap();
        let archive_path = tmp.path().join("archive.tar.gz");
        std::fs::write(
            &archive_path,
            build_tar_gz_with_hard_link(
                "postgres-16.4/share/timezone/Pacific/Wake",
                "tzdata",
                "postgres-16.4/share/timezone/Pacific/Wallis",
                "postgres-16.4/share/timezone/Pacific/Wake",
            ),
        )
        .unwrap();

        let dest = tmp.path().join("dest");
        extract_archive(&archive_path, &dest, "tar.gz", 1).unwrap();

        let linked = dest.join("share/timezone/Pacific/Wallis");
        assert!(linked.exists(), "hard link tidak dibuat");
        assert_eq!(std::fs::read_to_string(linked).unwrap(), "tzdata");
    }

    #[test]
    fn extract_archive_rejects_hard_link_outside_archive() {
        let tmp = tempfile::tempdir().unwrap();
        let archive_path = tmp.path().join("archive.tar.gz");
        std::fs::write(
            &archive_path,
            build_tar_gz_with_hard_link("root/file", "data", "root/evil", "../../etc/passwd"),
        )
        .unwrap();

        let dest = tmp.path().join("dest");
        let err = extract_archive(&archive_path, &dest, "tar.gz", 1).unwrap_err();
        assert!(
            err.to_string().contains("path traversal"),
            "pesan tak terduga: {err}"
        );
    }

    #[test]
    fn extract_archive_rejects_parent_dir_traversal() {
        let tmp = tempfile::tempdir().unwrap();
        let archive_path = tmp.path().join("archive.tar.gz");
        std::fs::write(&archive_path, build_tar_gz(&[("../evil", "pwned")])).unwrap();

        let dest = tmp.path().join("dest");
        let err = extract_archive(&archive_path, &dest, "tar.gz", 0).unwrap_err();
        assert!(err.to_string().contains("traversal"));
    }

    #[test]
    fn is_installed_false_when_no_marker() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths::under_root(tmp.path());
        assert!(!is_installed(&paths, EngineKind::Redis, "7.4.0"));
    }

    #[test]
    fn write_and_read_installed_marker_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path()).unwrap();
        write_installed_marker(tmp.path(), "deadbeef").unwrap();
        assert_eq!(read_installed_sha(tmp.path()), Some("deadbeef".to_string()));
    }
}
