pub mod mariadb;
pub mod mysql;
mod mysql_family;
pub mod postgres;
pub mod redis;

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::error::{Error, Result};
use crate::model::{ConnectionInfo, EngineKind, Instance};
use crate::paths::Paths;

pub struct InstanceCtx<'a> {
    pub instance: &'a Instance,
    pub bin_dir: PathBuf,
    pub data_dir: PathBuf,
    pub run_dir: PathBuf,
    pub log_file: PathBuf,
    pub conf_dir: PathBuf,
    pub lib_path: Vec<PathBuf>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StopSignal {
    Term,
    Int,
}

#[derive(Clone, Debug)]
pub struct LaunchSpec {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub working_dir: PathBuf,
    pub stop_signal: StopSignal,
    pub stop_timeout: Duration,
}

#[async_trait::async_trait]
pub trait EngineAdapter: Send + Sync {
    fn kind(&self) -> EngineKind;
    fn default_port(&self) -> u16;
    /// Nama executable yang ditambahkan ke PATH saat "Open Terminal".
    fn client_bin_dirs(&self, bin_dir: &Path) -> Vec<PathBuf>;
    fn main_binary(&self, bin_dir: &Path) -> PathBuf;

    fn is_initialized(&self, ctx: &InstanceCtx) -> bool;
    async fn init(&self, ctx: &InstanceCtx) -> Result<()>;
    fn launch_spec(&self, ctx: &InstanceCtx) -> Result<LaunchSpec>;
    async fn health_check(&self, ctx: &InstanceCtx) -> Result<bool>;
    fn connection_info(&self, ctx: &InstanceCtx) -> ConnectionInfo;

    /// Dipanggil setelah binary diekstrak. `paths` diberikan karena
    /// beberapa engine (mis. MySQL) perlu menyiapkan sesuatu di luar
    /// `bin_dir`, seperti symlink kompatibilitas library (§8.2).
    fn post_install(&self, _bin_dir: &Path, _paths: &Paths) -> Result<()> {
        Ok(())
    }
}

/// Gabungkan beberapa direktori jadi satu nilai `LD_LIBRARY_PATH`.
pub fn join_lib_path(dirs: &[PathBuf]) -> String {
    dirs.iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join(":")
}

/// Registry adapter per engine. Engine yang belum diimplementasikan
/// (MongoDB di Milestone 5) mengembalikan error yang jelas alih-alih panic.
pub fn adapter(kind: EngineKind) -> Result<&'static dyn EngineAdapter> {
    static REDIS: redis::RedisAdapter = redis::RedisAdapter;
    static POSTGRES: postgres::PostgresAdapter = postgres::PostgresAdapter;
    static MYSQL: mysql::MysqlAdapter = mysql::MysqlAdapter;
    static MARIADB: mariadb::MariadbAdapter = mariadb::MariadbAdapter;
    match kind {
        EngineKind::Redis => Ok(&REDIS),
        EngineKind::Postgres => Ok(&POSTGRES),
        EngineKind::Mysql => Ok(&MYSQL),
        EngineKind::Mariadb => Ok(&MARIADB),
        EngineKind::Mongodb => Err(Error::Other(format!(
            "engine {kind} belum didukung pada milestone ini"
        ))),
    }
}
