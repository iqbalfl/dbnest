use std::collections::HashMap;
use std::str::FromStr;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::config::atomic_write;
use crate::error::{Error, Result};
use crate::model::EngineKind;
use crate::paths::Paths;

/// Salinan `manifest/manifest.json` yang di-embed ke dalam binary, dipakai
/// sebagai fallback offline terakhir (§5.3).
pub const EMBEDDED_MANIFEST_JSON: &str = include_str!("../../../manifest/manifest.json");

/// Timeout pengambilan manifest remote (§5.3).
const FETCH_TIMEOUT: Duration = Duration::from_secs(10);

/// Skema manifest tertinggi yang dimengerti versi aplikasi ini. Manifest
/// remote dengan skema lebih baru ditolak supaya tidak salah tafsir.
const SUPPORTED_SCHEMA_VERSION: u32 = 1;

/// Dari mana manifest yang sedang dipakai berasal.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ManifestSource {
    /// Hasil unduhan terakhir dari `manifest_url`, tersimpan di cache XDG.
    Cache,
    /// Salinan bawaan binary, dipakai kalau belum pernah berhasil mengunduh.
    Embedded,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Manifest {
    pub schema_version: u32,
    pub generated_at: String,
    pub engines: HashMap<String, EngineCatalog>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct EngineCatalog {
    pub display_name: String,
    pub default_port: u16,
    pub versions: Vec<VersionEntry>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct VersionEntry {
    pub version: String,
    pub channel: String,
    /// `false` berarti URL/sha256 di bawah masih placeholder `"TODO"` dan
    /// belum diverifikasi manusia. Lihat aturan di CLAUDE.md: jangan
    /// mengarang URL unduhan atau sha256.
    #[serde(default = "default_true")]
    pub verified: bool,
    pub artifacts: Option<HashMap<String, Artifact>>,
    #[serde(default)]
    pub variants_by_distro: Option<Vec<DistroVariant>>,
}

fn default_true() -> bool {
    true
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Artifact {
    pub url: String,
    pub sha256: String,
    pub format: String,
    #[serde(default)]
    pub strip_components: u32,
    #[serde(default)]
    pub min_glibc: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DistroVariant {
    #[serde(rename = "match")]
    pub distro_match: DistroMatch,
    #[serde(flatten)]
    pub artifacts_by_arch: HashMap<String, Artifact>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DistroMatch {
    pub id: Vec<String>,
    #[serde(default)]
    pub version_id_min: Option<String>,
}

impl Manifest {
    pub fn parse(json: &str) -> Result<Self> {
        Ok(serde_json::from_str(json)?)
    }

    pub fn embedded() -> Result<Self> {
        Self::parse(EMBEDDED_MANIFEST_JSON)
    }

    pub fn engine_catalog(&self, engine: EngineKind) -> Option<&EngineCatalog> {
        self.engines.get(engine.as_str())
    }

    pub fn version_entry(&self, engine: EngineKind, version: &str) -> Option<&VersionEntry> {
        self.engine_catalog(engine)?
            .versions
            .iter()
            .find(|v| v.version == version)
    }
}

/// Manifest yang dipakai aplikasi sekarang: cache hasil unduhan terakhir
/// kalau ada dan masih bisa dibaca, selain itu salinan bawaan (§5.3).
/// Tidak pernah menyentuh jaringan — pemanggilnya ada di jalur panas.
pub fn load(paths: &Paths) -> Result<(Manifest, ManifestSource)> {
    let cache_path = paths.manifest_cache_file();
    if let Ok(contents) = std::fs::read_to_string(&cache_path) {
        match parse_validated(&contents) {
            Ok(manifest) => return Ok((manifest, ManifestSource::Cache)),
            Err(e) => {
                // Cache rusak/terlalu baru bukan alasan untuk gagal total;
                // cukup jatuh ke embedded dan biarkan refresh berikutnya
                // menimpanya.
                tracing::warn!("cache manifest diabaikan ({e}), memakai manifest bawaan");
            }
        }
    }
    Ok((Manifest::embedded()?, ManifestSource::Embedded))
}

/// Unduh manifest dari `url` (timeout 10 detik), validasi, lalu simpan ke
/// cache XDG secara atomik. Error dikembalikan apa adanya supaya UI bisa
/// memberi tahu pengguna kalau "Refresh versions" gagal — aplikasi sendiri
/// tetap jalan dengan cache/embedded yang lama.
pub async fn refresh(paths: &Paths, url: &str) -> Result<Manifest> {
    let client = reqwest::Client::new();
    let response = client
        .get(url)
        .timeout(FETCH_TIMEOUT)
        .send()
        .await?
        .error_for_status()?;
    let body = response.text().await?;

    // Validasi dulu, baru tulis cache: respons yang tidak valid tidak boleh
    // meracuni cache yang sebelumnya baik.
    let manifest = parse_validated(&body)?;
    atomic_write(&paths.manifest_cache_file(), body.as_bytes())?;
    Ok(manifest)
}

fn parse_validated(json: &str) -> Result<Manifest> {
    let manifest = Manifest::parse(json)?;
    if manifest.schema_version > SUPPORTED_SCHEMA_VERSION {
        return Err(Error::Other(format!(
            "manifest schema_version {} lebih baru dari yang didukung aplikasi ini ({SUPPORTED_SCHEMA_VERSION}); perbarui DBnest",
            manifest.schema_version
        )));
    }
    Ok(manifest)
}

/// Pilih artefak untuk `engine`/`version` berdasarkan arsitektur saat ini
/// (`std::env::consts::ARCH`), menolak entri yang belum diverifikasi.
pub fn select_artifact<'m>(
    manifest: &'m Manifest,
    engine: EngineKind,
    version: &str,
) -> Result<&'m Artifact> {
    select_artifact_for_arch(manifest, engine, version, std::env::consts::ARCH)
}

pub fn select_artifact_for_arch<'m>(
    manifest: &'m Manifest,
    engine: EngineKind,
    version: &str,
    arch: &str,
) -> Result<&'m Artifact> {
    let entry =
        manifest
            .version_entry(engine, version)
            .ok_or_else(|| Error::VersionUnavailable {
                engine,
                version: version.to_string(),
            })?;

    if !entry.verified {
        return Err(Error::Other(format!(
            "versi {engine} {version} belum diverifikasi (URL/sha256 masih TODO di manifest.json); \
             lengkapi manifest sebelum instalasi",
        )));
    }

    let artifacts = entry
        .artifacts
        .as_ref()
        .ok_or_else(|| Error::VersionUnavailable {
            engine,
            version: version.to_string(),
        })?;

    artifacts
        .get(arch)
        .ok_or_else(|| Error::VersionUnavailable {
            engine,
            version: version.to_string(),
        })
}

pub fn parse_engine_kind(s: &str) -> Result<EngineKind> {
    EngineKind::from_str(s).map_err(Error::Other)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_cache(paths: &Paths, contents: &str) {
        let cache = paths.manifest_cache_file();
        std::fs::create_dir_all(cache.parent().unwrap()).unwrap();
        std::fs::write(cache, contents).unwrap();
    }

    fn minimal_manifest_json(schema_version: u32, redis_version: &str) -> String {
        format!(
            r#"{{
                "schema_version": {schema_version},
                "generated_at": "2026-01-01T00:00:00Z",
                "engines": {{
                    "redis": {{
                        "display_name": "Redis",
                        "default_port": 6379,
                        "versions": [{{
                            "version": "{redis_version}",
                            "channel": "stable",
                            "verified": true,
                            "artifacts": {{
                                "x86_64": {{"url": "https://example.com/r.tar.gz", "sha256": "abc", "format": "tar.gz", "strip_components": 1}}
                            }}
                        }}]
                    }}
                }}
            }}"#
        )
    }

    #[test]
    fn load_falls_back_to_embedded_without_cache() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths::under_root(tmp.path());
        let (manifest, source) = load(&paths).unwrap();
        assert_eq!(source, ManifestSource::Embedded);
        assert!(manifest.engine_catalog(EngineKind::Redis).is_some());
    }

    #[test]
    fn load_prefers_cache_over_embedded() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths::under_root(tmp.path());
        write_cache(&paths, &minimal_manifest_json(1, "9.9.9-from-cache"));

        let (manifest, source) = load(&paths).unwrap();
        assert_eq!(source, ManifestSource::Cache);
        assert!(manifest
            .version_entry(EngineKind::Redis, "9.9.9-from-cache")
            .is_some());
    }

    #[test]
    fn load_ignores_corrupt_cache_and_uses_embedded() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths::under_root(tmp.path());
        write_cache(&paths, "{ this is not valid json");

        let (manifest, source) = load(&paths).unwrap();
        assert_eq!(source, ManifestSource::Embedded);
        assert!(manifest.engine_catalog(EngineKind::Redis).is_some());
    }

    #[test]
    fn load_ignores_cache_with_newer_schema_version() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths::under_root(tmp.path());
        write_cache(&paths, &minimal_manifest_json(999, "1.0.0"));

        let (_, source) = load(&paths).unwrap();
        assert_eq!(source, ManifestSource::Embedded);
    }

    #[test]
    fn parse_validated_rejects_future_schema_version() {
        let err = parse_validated(&minimal_manifest_json(2, "1.0.0")).unwrap_err();
        assert!(err.to_string().contains("schema_version"));
    }

    #[test]
    fn embedded_manifest_parses() {
        let manifest = Manifest::embedded().unwrap();
        assert_eq!(manifest.schema_version, 1);
        assert!(manifest.engine_catalog(EngineKind::Redis).is_some());
        assert!(manifest.engine_catalog(EngineKind::Postgres).is_some());
    }

    #[test]
    fn unverified_entries_are_rejected() {
        // Sengaja memakai fixture sendiri, bukan manifest bawaan: yang diuji
        // adalah gerbang `verified`, bukan keadaan data yang kebetulan sedang
        // dikirim. Sebelumnya tes ini bergantung pada entri bawaan yang masih
        // TODO, jadi ia ikut gagal begitu manifest diisi data sungguhan.
        let json = r#"{
            "schema_version": 1,
            "generated_at": "2026-01-01T00:00:00Z",
            "engines": {
                "redis": {
                    "display_name": "Redis",
                    "default_port": 6379,
                    "versions": [{
                        "version": "7.4.0",
                        "channel": "stable",
                        "verified": false,
                        "artifacts": {
                            "x86_64": {"url": "TODO", "sha256": "TODO", "format": "tar.gz", "strip_components": 1}
                        }
                    }]
                }
            }
        }"#;
        let manifest = Manifest::parse(json).unwrap();
        let err = select_artifact(&manifest, EngineKind::Redis, "7.4.0").unwrap_err();
        assert!(err.to_string().contains("belum diverifikasi"));
    }

    #[test]
    fn embedded_manifest_entries_are_all_usable() {
        // Kebalikannya: apa pun isi manifest bawaan saat ini, entri yang
        // ditandai `verified` harus benar-benar punya url dan sha256 yang
        // terisi — supaya data hasil build-engines.yml tidak pernah masuk
        // setengah jadi.
        let manifest = Manifest::embedded().unwrap();
        for engine in [
            EngineKind::Postgres,
            EngineKind::Mysql,
            EngineKind::Mariadb,
            EngineKind::Redis,
        ] {
            let Some(catalog) = manifest.engine_catalog(engine) else {
                continue;
            };
            for entry in &catalog.versions {
                if !entry.verified {
                    continue;
                }
                let artifacts = entry.artifacts.as_ref().unwrap_or_else(|| {
                    panic!("{engine} {} verified tanpa artifacts", entry.version)
                });
                assert!(
                    !artifacts.is_empty(),
                    "{engine} {} verified tapi tidak punya artefak",
                    entry.version
                );
                for (arch, artifact) in artifacts {
                    assert!(
                        artifact.url.starts_with("https://"),
                        "{engine} {} {arch}: url bukan https ({})",
                        entry.version,
                        artifact.url
                    );
                    assert!(
                        artifact.sha256.len() == 64
                            && artifact.sha256.chars().all(|c| c.is_ascii_hexdigit()),
                        "{engine} {} {arch}: sha256 tidak valid ({})",
                        entry.version,
                        artifact.sha256
                    );
                }
            }
        }
    }

    #[test]
    fn select_artifact_picks_arch() {
        let json = r#"{
            "schema_version": 1,
            "generated_at": "2026-01-01T00:00:00Z",
            "engines": {
                "redis": {
                    "display_name": "Redis",
                    "default_port": 6379,
                    "versions": [{
                        "version": "7.4.0",
                        "channel": "stable",
                        "verified": true,
                        "artifacts": {
                            "x86_64": {"url": "https://example.com/redis-x86_64.tar.gz", "sha256": "abc", "format": "tar.gz", "strip_components": 1},
                            "aarch64": {"url": "https://example.com/redis-aarch64.tar.gz", "sha256": "def", "format": "tar.gz", "strip_components": 1}
                        }
                    }]
                }
            }
        }"#;
        let manifest = Manifest::parse(json).unwrap();
        let artifact =
            select_artifact_for_arch(&manifest, EngineKind::Redis, "7.4.0", "aarch64").unwrap();
        assert_eq!(artifact.sha256, "def");
    }

    #[test]
    fn unknown_version_errors() {
        let manifest = Manifest::embedded().unwrap();
        let err = select_artifact(&manifest, EngineKind::Redis, "999.0").unwrap_err();
        assert_eq!(err.code(), "version_unavailable");
    }
}
