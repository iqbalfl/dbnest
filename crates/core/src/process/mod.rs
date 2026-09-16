pub mod direct;
pub mod systemd;

use crate::engines::LaunchSpec;
use crate::error::Result;
use std::path::Path;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProcState {
    Running { pid: u32 },
    Stopped,
    Failed { message: String },
}

#[async_trait::async_trait]
pub trait ProcessBackend: Send + Sync {
    async fn start(&self, id: &str, spec: &LaunchSpec, log_file: &Path) -> Result<()>;
    async fn stop(&self, id: &str, spec: &LaunchSpec) -> Result<()>;
    async fn status(&self, id: &str) -> Result<ProcState>;
    async fn set_autostart(
        &self,
        id: &str,
        spec: &LaunchSpec,
        log_file: &Path,
        on: bool,
    ) -> Result<()>;
    async fn remove(&self, id: &str) -> Result<()>;
}

/// Aturan pemilihan backend `auto` (§9): pakai `SystemdUserBackend` kalau
/// `systemctl --user is-system-running` bisa dijalankan dan hasilnya bukan
/// `offline` (atau gagal terhubung sama sekali), selain itu `DirectBackend`.
pub fn systemd_user_available() -> bool {
    let Ok(output) = std::process::Command::new("systemctl")
        .args(["--user", "is-system-running"])
        .output()
    else {
        return false;
    };
    let state = String::from_utf8_lossy(&output.stdout).trim().to_string();
    matches!(
        state.as_str(),
        "running" | "degraded" | "maintenance" | "starting" | "stopping"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn systemd_user_available_does_not_panic() {
        // Tidak mengasumsikan hasil (tergantung environment CI), cuma
        // memastikan tidak panic saat systemctl tidak ada / gagal jalan.
        let _ = systemd_user_available();
    }
}
