use std::path::{Path, PathBuf};

use directories::{BaseDirs, ProjectDirs};

use crate::error::{Error, Result};

const QUALIFIER: &str = "";
const ORG: &str = "";
const APP: &str = "dbnest";

/// Lokasi XDG milik aplikasi. Semua path di bawah ditentukan mengikuti
/// `$XDG_DATA_HOME`, `$XDG_CONFIG_HOME`, `$XDG_CACHE_HOME`, `$XDG_STATE_HOME`.
#[derive(Clone, Debug)]
pub struct Paths {
    data_dir: PathBuf,
    config_dir: PathBuf,
    cache_dir: PathBuf,
    state_dir: PathBuf,
    runtime_dir: Option<PathBuf>,
}

impl Paths {
    pub fn new() -> Result<Self> {
        let proj = ProjectDirs::from(QUALIFIER, ORG, APP)
            .ok_or_else(|| Error::Other("tidak bisa menentukan direktori home".to_string()))?;
        let base = BaseDirs::new()
            .ok_or_else(|| Error::Other("tidak bisa menentukan direktori home".to_string()))?;
        let state_dir = proj
            .state_dir()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| proj.data_dir().join("state"));
        Ok(Self {
            data_dir: proj.data_dir().to_path_buf(),
            config_dir: proj.config_dir().to_path_buf(),
            cache_dir: proj.cache_dir().to_path_buf(),
            state_dir,
            runtime_dir: base.runtime_dir().map(|p| p.join(APP)),
        })
    }

    /// Dipakai oleh tes: bikin `Paths` yang semuanya mengarah ke sub-folder
    /// direktori sementara, supaya tidak pernah menyentuh home asli.
    pub fn under_root(root: &Path) -> Self {
        Self {
            data_dir: root.join("data"),
            config_dir: root.join("config"),
            cache_dir: root.join("cache"),
            state_dir: root.join("state"),
            runtime_dir: Some(root.join("runtime")),
        }
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub fn config_dir(&self) -> &Path {
        &self.config_dir
    }

    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }

    pub fn state_dir(&self) -> &Path {
        &self.state_dir
    }

    pub fn binaries_dir(&self) -> PathBuf {
        self.data_dir.join("binaries")
    }

    pub fn engine_binaries_dir(&self, engine: &str) -> PathBuf {
        self.binaries_dir().join(engine)
    }

    pub fn version_dir(&self, engine: &str, version: &str) -> PathBuf {
        self.engine_binaries_dir(engine).join(version)
    }

    pub fn compat_lib_dir(&self) -> PathBuf {
        self.data_dir.join("compat-lib")
    }

    pub fn instances_dir(&self) -> PathBuf {
        self.data_dir.join("instances")
    }

    pub fn instance_dir(&self, id: &str) -> PathBuf {
        self.instances_dir().join(id)
    }

    pub fn instance_data_dir(&self, id: &str) -> PathBuf {
        self.instance_dir(id).join("data")
    }

    pub fn instance_run_dir(&self, id: &str) -> PathBuf {
        self.instance_dir(id).join("run")
    }

    pub fn instance_log_file(&self, id: &str) -> PathBuf {
        self.instance_dir(id).join("logs").join("engine.log")
    }

    pub fn instance_conf_dir(&self, id: &str) -> PathBuf {
        self.instance_dir(id).join("conf")
    }

    pub fn tmp_dir(&self) -> PathBuf {
        self.data_dir.join("tmp")
    }

    pub fn downloads_dir(&self) -> PathBuf {
        self.cache_dir.join("downloads")
    }

    pub fn manifest_cache_file(&self) -> PathBuf {
        self.cache_dir.join("manifest.json")
    }

    pub fn instances_file(&self) -> PathBuf {
        self.config_dir.join("instances.json")
    }

    pub fn settings_file(&self) -> PathBuf {
        self.config_dir.join("settings.json")
    }

    pub fn app_log_file(&self) -> PathBuf {
        self.state_dir.join("app.log")
    }

    pub fn systemd_user_dir(&self) -> PathBuf {
        self.config_dir_for_systemd()
    }

    fn config_dir_for_systemd(&self) -> PathBuf {
        // ~/.config/systemd/user, bukan ~/.config/dbnest/systemd/user.
        self.config_dir
            .parent()
            .map(|p| p.join("systemd").join("user"))
            .unwrap_or_else(|| self.config_dir.join("systemd").join("user"))
    }

    pub fn systemd_unit_file(&self, id: &str) -> PathBuf {
        self.systemd_user_dir().join(format!("dbnest-{id}.service"))
    }

    /// Fallback run dir jika path socket di `instances/<id>/run` terlalu panjang.
    pub fn runtime_fallback_run_dir(&self, id: &str) -> Option<PathBuf> {
        self.runtime_dir.as_ref().map(|p| p.join(id))
    }

    /// Buat semua direktori dasar yang dibutuhkan aplikasi.
    pub fn ensure_base_dirs(&self) -> Result<()> {
        for dir in [
            &self.data_dir,
            &self.config_dir,
            &self.cache_dir,
            &self.state_dir,
            &self.binaries_dir(),
            &self.instances_dir(),
            &self.tmp_dir(),
            &self.downloads_dir(),
            &self.compat_lib_dir(),
        ] {
            std::fs::create_dir_all(dir)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn under_root_keeps_everything_inside_root() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths::under_root(tmp.path());
        assert!(paths.data_dir().starts_with(tmp.path()));
        assert!(paths.config_dir().starts_with(tmp.path()));
        assert!(paths.cache_dir().starts_with(tmp.path()));
        assert!(paths.instance_data_dir("pg-abc123").starts_with(tmp.path()));
    }

    #[test]
    fn ensure_base_dirs_creates_directories() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths::under_root(tmp.path());
        paths.ensure_base_dirs().unwrap();
        assert!(paths.instances_dir().is_dir());
        assert!(paths.downloads_dir().is_dir());
    }
}
