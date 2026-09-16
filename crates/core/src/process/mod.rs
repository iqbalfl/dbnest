pub mod direct;

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
