use std::time::Duration;

use crate::model::EngineKind;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("instance tidak ditemukan: {0}")]
    InstanceNotFound(String),
    #[error("nama instance sudah dipakai: {0}")]
    DuplicateName(String),
    #[error("port {0} sudah dipakai")]
    PortInUse(u16),
    #[error("port {0} tidak valid, gunakan port >= 1024")]
    InvalidPort(u16),
    #[error("versi {engine:?} {version} tidak tersedia untuk arsitektur ini")]
    VersionUnavailable { engine: EngineKind, version: String },
    #[error("checksum tidak cocok")]
    ChecksumMismatch,
    #[error("library hilang: {0:?}")]
    MissingLibraries(Vec<String>),
    #[error("tidak boleh dijalankan sebagai root")]
    RunningAsRoot,
    #[error("server gagal siap dalam {0:?}")]
    StartTimeout(Duration),
    #[error("systemd: {0}")]
    Systemd(String),
    #[error("proses: {0}")]
    Process(String),
    #[error("preflight gagal: {0}")]
    PreflightFailed(String),
    #[error("instance masih dipakai oleh versi ini")]
    VersionInUse,
    #[error("{0}")]
    Other(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Http(#[from] reqwest::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

impl Error {
    /// Kode stabil untuk konsumsi UI/CLI (mis. `--json`).
    pub fn code(&self) -> &'static str {
        match self {
            Error::InstanceNotFound(_) => "instance_not_found",
            Error::DuplicateName(_) => "duplicate_name",
            Error::PortInUse(_) => "port_in_use",
            Error::InvalidPort(_) => "invalid_port",
            Error::VersionUnavailable { .. } => "version_unavailable",
            Error::ChecksumMismatch => "checksum_mismatch",
            Error::MissingLibraries(_) => "missing_libraries",
            Error::RunningAsRoot => "running_as_root",
            Error::StartTimeout(_) => "start_timeout",
            Error::Systemd(_) => "systemd_error",
            Error::Process(_) => "process_error",
            Error::PreflightFailed(_) => "preflight_failed",
            Error::VersionInUse => "version_in_use",
            Error::Other(_) => "other",
            Error::Io(_) => "io_error",
            Error::Http(_) => "http_error",
            Error::Json(_) => "json_error",
        }
    }

    pub fn hint(&self) -> Option<String> {
        match self {
            Error::PortInUse(p) => Some(format!(
                "Pilih port lain selain {p}, atau hentikan proses yang memakainya."
            )),
            Error::RunningAsRoot => {
                Some("Jalankan dbnest sebagai user biasa, bukan root/sudo.".to_string())
            }
            Error::MissingLibraries(libs) => Some(format!(
                "Pasang paket sistem yang menyediakan: {}",
                libs.join(", ")
            )),
            _ => None,
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;
