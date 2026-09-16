use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[serde(rename_all = "lowercase")]
pub enum EngineKind {
    Postgres,
    Mysql,
    Mariadb,
    Redis,
    // TODO(milestone 5): belum ada adapter/CLI support untuk MongoDB, lihat DESIGN.md §20.
    Mongodb,
}

impl EngineKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            EngineKind::Postgres => "postgres",
            EngineKind::Mysql => "mysql",
            EngineKind::Mariadb => "mariadb",
            EngineKind::Redis => "redis",
            EngineKind::Mongodb => "mongodb",
        }
    }

    /// Prefiks singkat dipakai untuk id instance, mis. "pg-7f3a2c".
    pub fn id_prefix(&self) -> &'static str {
        match self {
            EngineKind::Postgres => "pg",
            EngineKind::Mysql => "my",
            EngineKind::Mariadb => "mdb",
            EngineKind::Redis => "rds",
            EngineKind::Mongodb => "mongo",
        }
    }

    pub fn all() -> &'static [EngineKind] {
        &[
            EngineKind::Postgres,
            EngineKind::Mysql,
            EngineKind::Mariadb,
            EngineKind::Redis,
            EngineKind::Mongodb,
        ]
    }
}

impl std::str::FromStr for EngineKind {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "postgres" | "postgresql" | "pg" => Ok(EngineKind::Postgres),
            "mysql" => Ok(EngineKind::Mysql),
            "mariadb" => Ok(EngineKind::Mariadb),
            "redis" => Ok(EngineKind::Redis),
            "mongodb" | "mongo" => Ok(EngineKind::Mongodb),
            other => Err(format!("engine tidak dikenal: {other}")),
        }
    }
}

impl std::fmt::Display for EngineKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Instance {
    pub id: String,
    pub name: String,
    pub engine: EngineKind,
    pub version: String,
    pub port: u16,
    pub autostart: bool,
    pub created_at: String,
    #[serde(default)]
    pub extra_args: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum InstanceStatus {
    NotInitialized,
    Stopped,
    Starting,
    Running { pid: Option<u32> },
    Stopping,
    Failed { message: String },
}

#[derive(Serialize, Clone, Debug)]
pub struct ConnectionInfo {
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<String>,
    pub socket: Option<PathBuf>,
    pub url: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct InstanceView {
    #[serde(flatten)]
    pub instance: Instance,
    pub status: InstanceStatus,
    pub connection: ConnectionInfo,
}

#[derive(Serialize, Clone, Debug)]
pub enum InstallEvent {
    Downloading { downloaded: u64, total: Option<u64> },
    Verifying,
    Extracting,
    Done,
    Failed { message: String },
}

#[derive(Serialize, Clone, Debug)]
pub enum ProgressEvent {
    Install(InstallEvent),
    Initializing,
    Starting,
    HealthCheck,
    Ready,
    Failed { message: String },
}

#[derive(Serialize, Clone, Debug)]
pub struct Issue {
    pub severity: IssueSeverity,
    pub message: String,
    pub fix_hint: Option<String>,
}

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum IssueSeverity {
    Error,
    Warning,
    Info,
}

#[derive(Serialize, Clone, Debug)]
pub struct InstalledVersion {
    pub engine: EngineKind,
    pub version: String,
    pub size_bytes: u64,
}
