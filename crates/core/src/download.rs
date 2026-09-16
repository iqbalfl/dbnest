use std::path::Path;

use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

use crate::error::{Error, Result};
use crate::model::InstallEvent;

/// Unduh `url` ke `dest_part` secara streaming sambil menghitung sha256 dan
/// mengirim event progress. Mengembalikan sha256 hex dari isi yang diunduh.
pub async fn download_with_progress(
    client: &reqwest::Client,
    url: &str,
    dest_part: &Path,
    mut on_event: impl FnMut(InstallEvent),
) -> Result<String> {
    if let Some(dir) = dest_part.parent() {
        tokio::fs::create_dir_all(dir).await?;
    }

    let response = client
        .get(url)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await?
        .error_for_status()?;
    let total = response.content_length();

    let mut file = tokio::fs::File::create(dest_part).await?;
    let mut hasher = Sha256::new();
    let mut downloaded: u64 = 0;
    let mut stream = response.bytes_stream();

    on_event(InstallEvent::Downloading {
        downloaded: 0,
        total,
    });

    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        hasher.update(&chunk);
        file.write_all(&chunk).await?;
        downloaded += chunk.len() as u64;
        on_event(InstallEvent::Downloading { downloaded, total });
    }
    file.flush().await?;
    file.sync_all().await?;

    Ok(hex::encode(hasher.finalize()))
}

/// Verifikasi sha256 dari sebuah file yang sudah ada di disk (dipakai untuk
/// mengecek ulang `.installed` di §6 langkah 2).
pub fn sha256_of_file(path: &Path) -> Result<String> {
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

pub fn verify_sha256(actual: &str, expected: &str) -> Result<()> {
    if actual.eq_ignore_ascii_case(expected) {
        Ok(())
    } else {
        Err(Error::ChecksumMismatch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verify_sha256_accepts_case_insensitive_match() {
        assert!(verify_sha256("AbCd", "abcd").is_ok());
    }

    #[test]
    fn verify_sha256_rejects_mismatch() {
        let err = verify_sha256("abcd", "efgh").unwrap_err();
        assert_eq!(err.code(), "checksum_mismatch");
    }

    #[test]
    fn sha256_of_file_matches_known_value() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), b"hello world").unwrap();
        let digest = sha256_of_file(tmp.path()).unwrap();
        assert_eq!(
            digest,
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
    }
}
