use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::Path;

use fs4::fs_std::FileExt;
use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::model::Instance;
use crate::paths::Paths;

const CURRENT_SCHEMA_VERSION: u32 = 1;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct InstancesFile {
    pub schema_version: u32,
    pub instances: Vec<Instance>,
}

impl Default for InstancesFile {
    fn default() -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            instances: Vec::new(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ProcessBackendKind {
    Auto,
    Systemd,
    Direct,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SettingsFile {
    pub schema_version: u32,
    pub manifest_url: Option<String>,
    pub process_backend: ProcessBackendKind,
    pub terminal_command: Option<String>,
    pub start_minimized_to_tray: bool,
}

impl Default for SettingsFile {
    fn default() -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            manifest_url: None,
            process_backend: ProcessBackendKind::Auto,
            terminal_command: None,
            start_minimized_to_tray: false,
        }
    }
}

/// Migrasi `instances.json` berdasarkan `schema_version`. Saat ini hanya
/// versi 1 yang ada, jadi migrasi ini adalah no-op sampai skema berubah.
fn migrate_instances(mut file: InstancesFile) -> InstancesFile {
    if file.schema_version == 0 {
        file.schema_version = 1;
    }
    file
}

fn migrate_settings(mut file: SettingsFile) -> SettingsFile {
    if file.schema_version == 0 {
        file.schema_version = 1;
    }
    file
}

/// Tulis `bytes` ke `path` secara atomik: tulis ke file sementara di
/// direktori yang sama, `fsync`, lalu `rename`.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(dir)?;
    let tmp_path = dir.join(format!(
        ".{}.tmp-{}",
        path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("dbnest"),
        std::process::id()
    ));
    {
        let mut tmp_file = File::create(&tmp_path)?;
        tmp_file.write_all(bytes)?;
        tmp_file.sync_all()?;
    }
    fs::rename(&tmp_path, path)?;
    Ok(())
}

/// Kunci eksklusif pada `<path>.lock` selama nilai ini hidup. Dipakai untuk
/// melindungi baca-ubah-tulis `instances.json`/`settings.json` karena GUI
/// dan CLI bisa berjalan bersamaan.
pub struct FileLock {
    _file: File,
}

impl FileLock {
    /// Kunci eksklusif pada `path` persis seperti yang diberikan. Pemanggil
    /// bertanggung jawab memberi path lock file yang tepat (lihat
    /// [`lock_path_for`] untuk kasus "kunci file JSON ini").
    pub fn acquire(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(path)?;
        FileExt::lock_exclusive(&file)?;
        Ok(Self { _file: file })
    }
}

/// Path lock file (`<nama>.lock`) yang bersebelahan dengan `path`.
pub fn lock_path_for(path: &Path) -> std::path::PathBuf {
    let mut lock_path = path.to_path_buf();
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("dbnest");
    lock_path.set_file_name(format!("{file_name}.lock"));
    lock_path
}

pub struct ConfigStore {
    paths: Paths,
}

impl ConfigStore {
    pub fn new(paths: Paths) -> Self {
        Self { paths }
    }

    fn read_json_or_default<T>(&self, path: &Path) -> Result<T>
    where
        T: Default + for<'de> Deserialize<'de>,
    {
        if !path.exists() {
            return Ok(T::default());
        }
        let bytes = fs::read(path)?;
        if bytes.is_empty() {
            return Ok(T::default());
        }
        Ok(serde_json::from_slice(&bytes)?)
    }

    pub fn load_instances(&self) -> Result<InstancesFile> {
        let _lock = FileLock::acquire(&lock_path_for(&self.paths.instances_file()))?;
        let file: InstancesFile = self.read_json_or_default(&self.paths.instances_file())?;
        Ok(migrate_instances(file))
    }

    pub fn save_instances(&self, file: &InstancesFile) -> Result<()> {
        let _lock = FileLock::acquire(&lock_path_for(&self.paths.instances_file()))?;
        let bytes = serde_json::to_vec_pretty(file)?;
        atomic_write(&self.paths.instances_file(), &bytes)
    }

    /// Baca, biarkan pemanggil mengubah, lalu tulis kembali di bawah kunci yang sama.
    pub fn with_instances<F, R>(&self, f: F) -> Result<R>
    where
        F: FnOnce(&mut InstancesFile) -> Result<R>,
    {
        let _lock = FileLock::acquire(&lock_path_for(&self.paths.instances_file()))?;
        let mut file: InstancesFile = self.read_json_or_default(&self.paths.instances_file())?;
        file = migrate_instances(file);
        let result = f(&mut file)?;
        let bytes = serde_json::to_vec_pretty(&file)?;
        atomic_write(&self.paths.instances_file(), &bytes)?;
        Ok(result)
    }

    pub fn load_settings(&self) -> Result<SettingsFile> {
        let _lock = FileLock::acquire(&lock_path_for(&self.paths.settings_file()))?;
        let file: SettingsFile = self.read_json_or_default(&self.paths.settings_file())?;
        Ok(migrate_settings(file))
    }

    pub fn save_settings(&self, file: &SettingsFile) -> Result<()> {
        let _lock = FileLock::acquire(&lock_path_for(&self.paths.settings_file()))?;
        let bytes = serde_json::to_vec_pretty(file)?;
        atomic_write(&self.paths.settings_file(), &bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::EngineKind;

    fn store_in_tmp() -> (tempfile::TempDir, ConfigStore) {
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths::under_root(tmp.path());
        (tmp, ConfigStore::new(paths))
    }

    #[test]
    fn load_instances_defaults_when_missing() {
        let (_tmp, store) = store_in_tmp();
        let file = store.load_instances().unwrap();
        assert_eq!(file.schema_version, CURRENT_SCHEMA_VERSION);
        assert!(file.instances.is_empty());
    }

    #[test]
    fn save_then_load_roundtrips() {
        let (_tmp, store) = store_in_tmp();
        let instance = Instance {
            id: "rds-abc123".into(),
            name: "Redis dev".into(),
            engine: EngineKind::Redis,
            version: "7.4.0".into(),
            port: 6379,
            autostart: false,
            created_at: "2026-09-16T10:00:00Z".into(),
            extra_args: vec![],
        };
        store
            .with_instances(|f| {
                f.instances.push(instance.clone());
                Ok(())
            })
            .unwrap();

        let loaded = store.load_instances().unwrap();
        assert_eq!(loaded.instances.len(), 1);
        assert_eq!(loaded.instances[0].id, "rds-abc123");
    }

    #[test]
    fn atomic_write_never_leaves_tmp_file_behind() {
        let (_tmp, store) = store_in_tmp();
        store
            .with_instances(|f| {
                f.instances.clear();
                Ok(())
            })
            .unwrap();
        let dir = store.paths.instances_file().parent().unwrap().to_path_buf();
        let leftover_tmp = fs::read_dir(dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .any(|e| e.file_name().to_string_lossy().contains(".tmp-"));
        assert!(!leftover_tmp);
    }

    #[test]
    fn settings_roundtrip() {
        let (_tmp, store) = store_in_tmp();
        let mut settings = store.load_settings().unwrap();
        settings.process_backend = ProcessBackendKind::Direct;
        store.save_settings(&settings).unwrap();
        let loaded = store.load_settings().unwrap();
        assert_eq!(loaded.process_backend, ProcessBackendKind::Direct);
    }
}
