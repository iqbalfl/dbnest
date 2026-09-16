//! Symlink kompatibilitas library sistem, tanpa sudo (DESIGN.md §8.2).

use std::path::{Path, PathBuf};

use crate::error::Result;
use crate::paths::Paths;

/// Direktori triplet tempat library sistem dicari.
const TRIPLET_DIRS: &[&str] = &["/usr/lib/x86_64-linux-gnu", "/usr/lib/aarch64-linux-gnu"];

/// Ubuntu 24.04+ hanya menyediakan `libaio.so.1t64`, sementara MySQL
/// mencari `libaio.so.1`. Kalau `libaio.so.1` tidak ada di sistem tapi
/// `libaio.so.1t64` ada, buat symlink di `compat-lib/` milik aplikasi
/// (tanpa sudo) lalu tambahkan `compat-lib/` ke `LD_LIBRARY_PATH` MySQL.
///
/// Mengembalikan path symlink jika dibuat/sudah ada, atau `None` jika tidak
/// diperlukan (library asli sudah tersedia, atau `libaio.so.1t64` pun tidak
/// ditemukan sehingga tidak ada yang bisa disymlink).
pub fn ensure_libaio_compat_symlink(paths: &Paths) -> Result<Option<PathBuf>> {
    ensure_libaio_compat_symlink_in(paths, TRIPLET_DIRS)
}

fn ensure_libaio_compat_symlink_in(
    paths: &Paths,
    triplet_dirs: &[&str],
) -> Result<Option<PathBuf>> {
    let link_path = paths.compat_lib_dir().join("libaio.so.1");
    if link_path.exists() {
        return Ok(Some(link_path));
    }
    if find_in_dirs(triplet_dirs, "libaio.so.1").is_some() {
        // Sudah tersedia secara native, tidak perlu symlink.
        return Ok(None);
    }
    let Some(real_path) = find_in_dirs(triplet_dirs, "libaio.so.1t64") else {
        return Ok(None);
    };

    std::fs::create_dir_all(paths.compat_lib_dir())?;
    std::os::unix::fs::symlink(&real_path, &link_path)?;
    Ok(Some(link_path))
}

fn find_in_dirs(dirs: &[&str], file_name: &str) -> Option<PathBuf> {
    dirs.iter()
        .map(|dir| Path::new(dir).join(file_name))
        .find(|p| p.exists())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_symlink_when_only_t64_variant_exists() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths::under_root(tmp.path());
        let triplet_dir = tmp.path().join("triplet");
        std::fs::create_dir_all(&triplet_dir).unwrap();
        std::fs::write(triplet_dir.join("libaio.so.1t64"), b"fake").unwrap();
        let triplet_str = triplet_dir.to_str().unwrap();

        let link = ensure_libaio_compat_symlink_in(&paths, &[triplet_str])
            .unwrap()
            .expect("symlink harus dibuat");

        assert_eq!(link, paths.compat_lib_dir().join("libaio.so.1"));
        assert!(link.exists());
        let target = std::fs::read_link(&link).unwrap();
        assert_eq!(target, triplet_dir.join("libaio.so.1t64"));
    }

    #[test]
    fn skips_when_native_library_already_present() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths::under_root(tmp.path());
        let triplet_dir = tmp.path().join("triplet");
        std::fs::create_dir_all(&triplet_dir).unwrap();
        std::fs::write(triplet_dir.join("libaio.so.1"), b"fake").unwrap();
        let triplet_str = triplet_dir.to_str().unwrap();

        let result = ensure_libaio_compat_symlink_in(&paths, &[triplet_str]).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn skips_when_neither_variant_found() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths::under_root(tmp.path());
        let empty_dir = tmp.path().join("empty");
        std::fs::create_dir_all(&empty_dir).unwrap();
        let empty_str = empty_dir.to_str().unwrap();

        let result = ensure_libaio_compat_symlink_in(&paths, &[empty_str]).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn is_idempotent_when_symlink_already_exists() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths::under_root(tmp.path());
        let triplet_dir = tmp.path().join("triplet");
        std::fs::create_dir_all(&triplet_dir).unwrap();
        std::fs::write(triplet_dir.join("libaio.so.1t64"), b"fake").unwrap();
        let triplet_str = triplet_dir.to_str().unwrap();

        let first = ensure_libaio_compat_symlink_in(&paths, &[triplet_str]).unwrap();
        let second = ensure_libaio_compat_symlink_in(&paths, &[triplet_str]).unwrap();
        assert_eq!(first, second);
    }
}
