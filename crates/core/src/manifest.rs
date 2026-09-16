use std::collections::HashMap;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::model::EngineKind;

/// Salinan `manifest/manifest.json` yang di-embed ke dalam binary, dipakai
/// sebagai fallback offline (§5.3) dan, untuk Milestone 1, satu-satunya
/// sumber manifest (belum ada fetch remote).
pub const EMBEDDED_MANIFEST_JSON: &str = include_str!("../../../manifest/manifest.json");

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

    #[test]
    fn embedded_manifest_parses() {
        let manifest = Manifest::embedded().unwrap();
        assert_eq!(manifest.schema_version, 1);
        assert!(manifest.engine_catalog(EngineKind::Redis).is_some());
        assert!(manifest.engine_catalog(EngineKind::Postgres).is_some());
    }

    #[test]
    fn unverified_entries_are_rejected() {
        let manifest = Manifest::embedded().unwrap();
        let err = select_artifact(&manifest, EngineKind::Redis, "7.4.0").unwrap_err();
        assert!(err.to_string().contains("belum diverifikasi"));
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
