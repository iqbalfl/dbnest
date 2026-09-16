//! Helper yang dipakai bersama oleh adapter MySQL dan MariaDB, karena
//! keduanya berasal dari codebase yang sama dan berbagi banyak perilaku
//! (§7.2).

use std::path::{Path, PathBuf};

/// Cari executable pertama yang ada di antara `candidates` (relatif
/// terhadap `bin_dir`). Kalau tidak ada yang ditemukan, kembalikan
/// kandidat pertama saja (dipakai sebagai path default sebelum instalasi).
pub fn find_first_existing(bin_dir: &Path, candidates: &[&str]) -> PathBuf {
    for candidate in candidates {
        let path = bin_dir.join(candidate);
        if path.exists() {
            return path;
        }
    }
    bin_dir.join(candidates[0])
}

/// Jalankan `<admin_bin> --no-defaults -h 127.0.0.1 -P <port> -u root ping`.
pub async fn admin_ping(admin_bin: &Path, port: u16) -> bool {
    if !admin_bin.exists() {
        return false;
    }
    tokio::process::Command::new(admin_bin)
        .args([
            "--no-defaults",
            "-h",
            "127.0.0.1",
            "-P",
            &port.to_string(),
            "-u",
            "root",
            "ping",
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await
        .map(|status| status.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_first_existing_prefers_existing_candidate() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("mariadbd"), b"").unwrap();
        let found = find_first_existing(tmp.path(), &["mysqld", "mariadbd"]);
        assert_eq!(found, tmp.path().join("mariadbd"));
    }

    #[test]
    fn find_first_existing_falls_back_to_first_candidate() {
        let tmp = tempfile::tempdir().unwrap();
        let found = find_first_existing(tmp.path(), &["mariadbd", "mysqld"]);
        assert_eq!(found, tmp.path().join("mariadbd"));
    }
}
