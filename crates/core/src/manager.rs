use std::time::Duration;

use tokio::time::Instant;

use crate::config::{ConfigStore, ProcessBackendKind, SettingsFile};
use crate::engines::{self, InstanceCtx};
use crate::error::{Error, Result};
use crate::install::{self, Installer};
use crate::manifest::{self, Manifest, ManifestSource};
use crate::model::{
    ConnectionInfo, EngineKind, InstalledVersion, Instance, InstanceStatus, Issue, IssueSeverity,
    ProgressEvent,
};
use crate::paths::Paths;
use crate::ports;
use crate::preflight;
use crate::process::direct::DirectBackend;
use crate::process::systemd::SystemdUserBackend;
use crate::process::{self, ProcState, ProcessBackend};
use crate::terminal;

pub struct CreateInstanceRequest {
    pub engine: EngineKind,
    pub version: String,
    pub name: Option<String>,
    pub port: Option<u16>,
    pub autostart: bool,
}

#[derive(Default)]
pub struct UpdateInstance {
    pub name: Option<String>,
    pub port: Option<u16>,
    pub autostart: Option<bool>,
}

/// Facade tingkat tinggi yang dipakai CLI dan (nanti) Tauri commands. Semua
/// logika bisnis hidup di sini; lapisan di atasnya hanya pembungkus tipis.
pub struct Manager {
    paths: Paths,
    config: ConfigStore,
    backend: Box<dyn ProcessBackend>,
    backend_is_systemd: bool,
}

impl Manager {
    pub fn new() -> Result<Self> {
        let paths = Paths::new()?;
        Self::with_paths(paths)
    }

    /// Dipakai oleh tes integrasi: semua path XDG diarahkan ke direktori
    /// sementara, bukan home asli.
    pub fn with_paths(paths: Paths) -> Result<Self> {
        paths.ensure_base_dirs()?;
        let config = ConfigStore::new(paths.clone());
        let (backend, backend_is_systemd) = select_backend(&config, &paths)?;
        Ok(Self {
            paths,
            config,
            backend,
            backend_is_systemd,
        })
    }

    /// `"systemd"` atau `"direct"` — backend proses yang sedang dipakai
    /// (§9). Dipakai `dbnest doctor` dan GUI (mis. untuk memutuskan apakah
    /// perlu menawarkan "hentikan semua server?" saat Quit, karena hanya
    /// `DirectBackend` yang tidak diawasi systemd secara independen).
    pub fn backend_label(&self) -> &'static str {
        if self.backend_is_systemd {
            "systemd"
        } else {
            "direct"
        }
    }

    pub fn is_direct_backend(&self) -> bool {
        !self.backend_is_systemd
    }

    /// Manifest yang dipakai sekarang: cache hasil unduhan terakhir, atau
    /// salinan bawaan binary (§5.3). Tidak menyentuh jaringan.
    pub fn manifest(&self) -> Result<Manifest> {
        Ok(manifest::load(&self.paths)?.0)
    }

    /// Dari mana manifest saat ini berasal — supaya pengguna bisa tahu
    /// apakah daftar versi yang dilihat sudah datang dari `manifest_url`
    /// atau masih bawaan aplikasi.
    pub fn manifest_source(&self) -> Result<ManifestSource> {
        Ok(manifest::load(&self.paths)?.1)
    }

    /// Unduh ulang manifest dari `settings.manifest_url` dan simpan ke
    /// cache. Gagal kalau URL belum diatur atau unduhannya gagal — supaya
    /// UI bisa bilang apa adanya saat pengguna menekan "Refresh versions".
    pub async fn refresh_manifest(&self) -> Result<Manifest> {
        let url = self.get_settings()?.manifest_url.ok_or_else(|| {
            Error::Other(
                "manifest_url belum diatur di settings; isi dulu supaya bisa mengambil daftar versi terbaru"
                    .to_string(),
            )
        })?;
        manifest::refresh(&self.paths, &url).await
    }

    pub fn list_instances(&self) -> Result<Vec<Instance>> {
        Ok(self.config.load_instances()?.instances)
    }

    pub fn find_instance(&self, id_or_name: &str) -> Result<Instance> {
        let file = self.config.load_instances()?;
        file.instances
            .into_iter()
            .find(|i| i.id == id_or_name || i.name.eq_ignore_ascii_case(id_or_name))
            .ok_or_else(|| Error::InstanceNotFound(id_or_name.to_string()))
    }

    pub async fn create_instance(&self, req: CreateInstanceRequest) -> Result<Instance> {
        let manifest = self.manifest()?;
        if manifest.version_entry(req.engine, &req.version).is_none() {
            return Err(Error::VersionUnavailable {
                engine: req.engine,
                version: req.version,
            });
        }
        let adapter = engines::adapter(req.engine)?;

        let instance = self.config.with_instances(|file| {
            let port = match req.port {
                Some(p) => {
                    if p < 1024 {
                        return Err(Error::InvalidPort(p));
                    }
                    if file.instances.iter().any(|i| i.port == p) {
                        return Err(Error::PortInUse(p));
                    }
                    p
                }
                None => ports::suggest_port(adapter.default_port(), &file.instances)
                    .ok_or_else(|| Error::Other("tidak ada port kosong tersedia".to_string()))?,
            };

            let name = match req.name {
                Some(n) => {
                    if file
                        .instances
                        .iter()
                        .any(|i| i.name.eq_ignore_ascii_case(&n))
                    {
                        return Err(Error::DuplicateName(n));
                    }
                    n
                }
                None => unique_default_name(req.engine, &manifest, &file.instances),
            };

            let id = generate_id(req.engine, &file.instances);
            let instance = Instance {
                id,
                name,
                engine: req.engine,
                version: req.version.clone(),
                port,
                autostart: req.autostart,
                created_at: now_rfc3339(),
                extra_args: Vec::new(),
            };
            file.instances.push(instance.clone());
            Ok(instance)
        })?;

        // Kalau autostart dicentang, unit systemd (kalau backend-nya
        // systemd) harus langsung dibuat & di-enable sekarang juga, bukan
        // menunggu instance ini pernah di-start manual sekali — supaya
        // "logout lalu login lagi" langsung menjalankannya (§20 M4).
        if instance.autostart {
            self.sync_autostart(&instance).await?;
        }

        Ok(instance)
    }

    /// Alur start lengkap dari §9.3: preflight (disederhanakan di Milestone
    /// 1), install bila perlu, init sekali, start proses, lalu poll health
    /// check.
    pub async fn start(
        &self,
        id_or_name: &str,
        mut on_event: impl FnMut(ProgressEvent),
    ) -> Result<()> {
        let instance = self.find_instance(id_or_name)?;
        let adapter = engines::adapter(instance.engine)?;
        let manifest = self.manifest()?;

        if !install::is_installed(&self.paths, instance.engine, &instance.version) {
            let artifact =
                manifest::select_artifact(&manifest, instance.engine, &instance.version)?;
            let installer = Installer::new(&self.paths);
            installer
                .ensure_installed(
                    instance.engine,
                    &instance.version,
                    artifact,
                    |dir| adapter.post_install(dir, &self.paths),
                    |e| on_event(ProgressEvent::Install(e)),
                )
                .await?;
        }

        let ctx = self.ctx_for(&instance);

        on_event(ProgressEvent::Preflight);
        let already_running = matches!(
            self.backend.status(&instance.id).await?,
            ProcState::Running { .. }
        );
        let issues = preflight::check(&self.paths, &instance, &ctx, adapter, already_running);
        if let Some(blocking) = issues.iter().find(|i| i.severity == IssueSeverity::Error) {
            let message = blocking.message.clone();
            on_event(ProgressEvent::Failed {
                message: message.clone(),
            });
            return Err(Error::PreflightFailed(message));
        }

        if !adapter.is_initialized(&ctx) {
            on_event(ProgressEvent::Initializing);
            if let Err(e) = adapter.init(&ctx).await {
                let _ = std::fs::remove_dir_all(&ctx.data_dir);
                on_event(ProgressEvent::Failed {
                    message: e.to_string(),
                });
                return Err(e);
            }
        }

        on_event(ProgressEvent::Starting);
        let spec = adapter.launch_spec(&ctx)?;
        self.backend
            .start(&instance.id, &spec, &ctx.log_file)
            .await?;

        on_event(ProgressEvent::HealthCheck);
        let health_timeout = if instance.engine == EngineKind::Mysql {
            Duration::from_secs(60)
        } else {
            Duration::from_secs(30)
        };
        let deadline = Instant::now() + health_timeout;
        loop {
            if adapter.health_check(&ctx).await.unwrap_or(false) {
                on_event(ProgressEvent::Ready);
                return Ok(());
            }
            if Instant::now() >= deadline {
                let tail = self.tail_logs(id_or_name, 20).unwrap_or_default();
                let message = format!(
                    "server gagal siap dalam {health_timeout:?}, log terakhir:\n{}",
                    tail.join("\n")
                );
                on_event(ProgressEvent::Failed { message });
                return Err(Error::StartTimeout(health_timeout));
            }
            tokio::time::sleep(Duration::from_millis(300)).await;
        }
    }

    pub async fn stop(&self, id_or_name: &str) -> Result<()> {
        let instance = self.find_instance(id_or_name)?;
        let adapter = engines::adapter(instance.engine)?;
        let ctx = self.ctx_for(&instance);
        let spec = adapter.launch_spec(&ctx)?;
        self.backend.stop(&instance.id, &spec).await
    }

    pub async fn status(&self, id_or_name: &str) -> Result<InstanceStatus> {
        let instance = self.find_instance(id_or_name)?;
        let adapter = engines::adapter(instance.engine)?;
        let ctx = self.ctx_for(&instance);

        match self.backend.status(&instance.id).await? {
            ProcState::Running { pid } => Ok(InstanceStatus::Running { pid: Some(pid) }),
            ProcState::Failed { message } => Ok(InstanceStatus::Failed { message }),
            ProcState::Stopped => {
                if !install::is_installed(&self.paths, instance.engine, &instance.version)
                    || !adapter.is_initialized(&ctx)
                {
                    Ok(InstanceStatus::NotInitialized)
                } else {
                    Ok(InstanceStatus::Stopped)
                }
            }
        }
    }

    pub fn connection_info(&self, id_or_name: &str) -> Result<ConnectionInfo> {
        let instance = self.find_instance(id_or_name)?;
        let adapter = engines::adapter(instance.engine)?;
        let ctx = self.ctx_for(&instance);
        Ok(adapter.connection_info(&ctx))
    }

    pub fn tail_logs(&self, id_or_name: &str, lines: usize) -> Result<Vec<String>> {
        let instance = self.find_instance(id_or_name)?;
        let log_file = self.paths.instance_log_file(&instance.id);
        if !log_file.exists() {
            return Ok(Vec::new());
        }
        let content = std::fs::read_to_string(&log_file)?;
        let all: Vec<String> = content.lines().map(str::to_string).collect();
        let start = all.len().saturating_sub(lines);
        Ok(all[start..].to_vec())
    }

    pub async fn delete_instance(&self, id_or_name: &str, delete_data: bool) -> Result<()> {
        let instance = self.find_instance(id_or_name)?;
        let adapter = engines::adapter(instance.engine)?;
        let ctx = self.ctx_for(&instance);
        if let Ok(spec) = adapter.launch_spec(&ctx) {
            let _ = self.backend.stop(&instance.id, &spec).await;
        }
        self.backend.remove(&instance.id).await?;

        self.config.with_instances(|file| {
            file.instances.retain(|i| i.id != instance.id);
            Ok(())
        })?;

        if delete_data {
            let dir = self.paths.instance_dir(&instance.id);
            if dir.exists() {
                std::fs::remove_dir_all(&dir)?;
            }
        }
        Ok(())
    }

    pub fn suggest_port(&self, engine: EngineKind) -> Result<u16> {
        let adapter = engines::adapter(engine)?;
        let instances = self.list_instances()?;
        ports::suggest_port(adapter.default_port(), &instances)
            .ok_or_else(|| Error::Other("tidak ada port kosong tersedia".to_string()))
    }

    /// Jalankan preflight (§8.1) untuk instance ini tanpa menginstall atau
    /// menjalankannya. Dipakai oleh `dbnest doctor` dan banner UI.
    pub async fn preflight(&self, id_or_name: &str) -> Result<Vec<Issue>> {
        let instance = self.find_instance(id_or_name)?;
        let adapter = engines::adapter(instance.engine)?;
        let ctx = self.ctx_for(&instance);
        let already_running = matches!(
            self.backend.status(&instance.id).await?,
            ProcState::Running { .. }
        );
        Ok(preflight::check(
            &self.paths,
            &instance,
            &ctx,
            adapter,
            already_running,
        ))
    }

    /// Ubah nama/port/autostart. Kalau instance sedang jalan, port berubah
    /// artinya stop → update config → start lagi (§12).
    pub async fn update_instance(
        &self,
        id_or_name: &str,
        patch: UpdateInstance,
    ) -> Result<Instance> {
        let instance = self.find_instance(id_or_name)?;
        let was_running = matches!(
            self.backend.status(&instance.id).await?,
            ProcState::Running { .. }
        );

        if was_running && (patch.port.is_some()) {
            self.stop(&instance.id).await?;
        }

        let updated = self.config.with_instances(|file| {
            if !file.instances.iter().any(|i| i.id == instance.id) {
                return Err(Error::InstanceNotFound(instance.id.clone()));
            }
            if let Some(name) = &patch.name {
                let taken = file
                    .instances
                    .iter()
                    .any(|i| i.id != instance.id && i.name.eq_ignore_ascii_case(name));
                if taken {
                    return Err(Error::DuplicateName(name.clone()));
                }
            }
            if let Some(port) = patch.port {
                if port < 1024 {
                    return Err(Error::InvalidPort(port));
                }
                let taken = file
                    .instances
                    .iter()
                    .any(|i| i.id != instance.id && i.port == port);
                if taken {
                    return Err(Error::PortInUse(port));
                }
            }

            let target = file
                .instances
                .iter_mut()
                .find(|i| i.id == instance.id)
                .expect("sudah divalidasi ada di atas");
            if let Some(name) = patch.name {
                target.name = name;
            }
            if let Some(port) = patch.port {
                target.port = port;
            }
            if let Some(autostart) = patch.autostart {
                target.autostart = autostart;
            }
            Ok(target.clone())
        })?;

        // Selaraskan unit systemd (kalau ada) dengan config terbaru —
        // nama/port yang berubah harus ikut tertulis ulang di ExecStart,
        // dan status enabled harus ikut autostart yang baru (§12: "tulis
        // ulang unit systemd").
        self.sync_autostart(&updated).await?;

        if was_running && patch.port.is_some() {
            self.start(&instance.id, |_| {}).await?;
        }

        Ok(updated)
    }

    /// Jalankan semua instance dengan `autostart = true`. Dipakai saat
    /// aplikasi/tray dibuka, supaya `DirectBackend` (yang tidak punya
    /// mekanisme autostart level-OS) tetap menepati flag ini (§9.2).
    /// Untuk `SystemdUserBackend`, autostart sesungguhnya sudah ditangani
    /// systemd sendiri lewat unit yang di-enable; memanggil ini lagi cukup
    /// aman karena `start()` idempoten.
    pub async fn autostart_all(&self) -> Result<()> {
        for instance in self.list_instances()? {
            if instance.autostart {
                let _ = self.start(&instance.id, |_| {}).await;
            }
        }
        Ok(())
    }

    /// Tulis ulang (atau buat) unit systemd instance ini dan set status
    /// enabled-nya sesuai `instance.autostart`. Untuk backend selain
    /// systemd ini adalah no-op (lihat `DirectBackend::set_autostart`).
    async fn sync_autostart(&self, instance: &Instance) -> Result<()> {
        let adapter = engines::adapter(instance.engine)?;
        let ctx = self.ctx_for(instance);
        let spec = adapter.launch_spec(&ctx)?;
        self.backend
            .set_autostart(&instance.id, &spec, &ctx.log_file, instance.autostart)
            .await
    }

    /// Semua versi engine yang sudah terpasang di disk, dengan ukurannya.
    pub fn installed_versions(&self) -> Result<Vec<InstalledVersion>> {
        let mut result = Vec::new();
        let binaries_dir = self.paths.binaries_dir();
        let Ok(engine_dirs) = std::fs::read_dir(&binaries_dir) else {
            return Ok(result);
        };
        for engine_entry in engine_dirs.flatten() {
            let Ok(engine) = engine_entry.file_name().into_string() else {
                continue;
            };
            let Ok(engine) = engine.parse::<EngineKind>() else {
                continue;
            };
            let Ok(version_dirs) = std::fs::read_dir(engine_entry.path()) else {
                continue;
            };
            for version_entry in version_dirs.flatten() {
                let version_dir = version_entry.path();
                if !version_dir.join(".installed").exists() {
                    continue;
                }
                let Ok(version) = version_entry.file_name().into_string() else {
                    continue;
                };
                let size_bytes = dir_size(&version_dir);
                result.push(InstalledVersion {
                    engine,
                    version,
                    size_bytes,
                });
            }
        }
        Ok(result)
    }

    /// Hapus versi yang terpasang. Ditolak kalau masih dipakai instance
    /// manapun (§6).
    pub fn uninstall_version(&self, engine: EngineKind, version: &str) -> Result<()> {
        let in_use = self
            .list_instances()?
            .iter()
            .any(|i| i.engine == engine && i.version == version);
        if in_use {
            return Err(Error::VersionInUse);
        }
        let dir = self.paths.version_dir(engine.as_str(), version);
        if dir.exists() {
            std::fs::remove_dir_all(&dir)?;
        }
        Ok(())
    }

    pub fn get_settings(&self) -> Result<SettingsFile> {
        self.config.load_settings()
    }

    pub fn update_settings(&self, settings: SettingsFile) -> Result<SettingsFile> {
        self.config.save_settings(&settings)?;
        Ok(settings)
    }

    /// Buka terminal emulator dengan PATH/env sudah mengarah ke versi
    /// engine instance ini (§11.1). Mengembalikan petunjuk tambahan yang
    /// perlu ditampilkan ke pengguna (mis. `redis-cli -p P` untuk Redis).
    pub fn open_terminal(&self, id_or_name: &str) -> Result<Option<String>> {
        let instance = self.find_instance(id_or_name)?;
        let adapter = engines::adapter(instance.engine)?;
        let ctx = self.ctx_for(&instance);
        let settings = self.get_settings()?;

        let terminal_cmd = terminal::find_terminal(settings.terminal_command.as_deref())
            .ok_or_else(|| {
                Error::Other("tidak menemukan terminal emulator di sistem".to_string())
            })?;
        let launch = terminal::build_launch(&terminal_cmd, adapter, &ctx)?;

        std::process::Command::new(&launch.program)
            .args(&launch.args)
            .envs(launch.env.iter().cloned())
            .spawn()?;

        Ok(terminal::connection_hint(instance.engine, instance.port))
    }

    /// Environment untuk bekerja dengan instance ini dari shell: PATH sudah
    /// diawali direktori client engine-nya, plus variabel koneksi per
    /// engine (§11.1). Dipakai `dbnest shell` dan `dbnest env`.
    pub fn instance_env(&self, id_or_name: &str) -> Result<Vec<(String, String)>> {
        let instance = self.find_instance(id_or_name)?;
        let adapter = engines::adapter(instance.engine)?;
        let ctx = self.ctx_for(&instance);
        Ok(terminal::instance_env(adapter, &ctx))
    }

    /// Shell login pengguna, program yang dijalankan `dbnest shell`.
    pub fn user_shell(&self) -> String {
        terminal::user_shell()
    }

    /// Petunjuk tambahan yang perlu ditampilkan sebelum masuk shell, untuk
    /// engine yang tidak punya variabel env koneksi (Redis).
    pub fn connection_hint(&self, id_or_name: &str) -> Result<Option<String>> {
        let instance = self.find_instance(id_or_name)?;
        Ok(terminal::connection_hint(instance.engine, instance.port))
    }

    /// Path folder data instance, dipakai UI untuk "Open data folder".
    pub fn instance_data_folder(&self, id_or_name: &str) -> Result<std::path::PathBuf> {
        let instance = self.find_instance(id_or_name)?;
        Ok(self.paths.instance_dir(&instance.id))
    }

    pub fn open_data_folder(&self, id_or_name: &str) -> Result<()> {
        let dir = self.instance_data_folder(id_or_name)?;
        std::fs::create_dir_all(&dir)?;
        std::process::Command::new("xdg-open").arg(&dir).spawn()?;
        Ok(())
    }

    fn ctx_for<'a>(&self, instance: &'a Instance) -> InstanceCtx<'a> {
        let bin_dir = self
            .paths
            .version_dir(instance.engine.as_str(), &instance.version);
        // `compat-lib/` (§8.2) adalah satu-satunya bagian dari
        // LD_LIBRARY_PATH yang bersifat lintas-engine; sisanya (mis.
        // `bin/lib` Postgres atau `bin/lib/private` MySQL) dihitung sendiri
        // oleh masing-masing adapter.
        let lib_path = vec![self.paths.compat_lib_dir()];
        InstanceCtx {
            instance,
            bin_dir,
            data_dir: self.paths.instance_data_dir(&instance.id),
            run_dir: self.paths.instance_run_dir(&instance.id),
            log_file: self.paths.instance_log_file(&instance.id),
            conf_dir: self.paths.instance_conf_dir(&instance.id),
            lib_path,
        }
    }
}

/// Pilih `ProcessBackend` sesuai `settings.process_backend` (§9): `auto`
/// memakai systemd kalau `systemctl --user` benar-benar tersedia, selain
/// itu `direct`.
fn select_backend(config: &ConfigStore, paths: &Paths) -> Result<(Box<dyn ProcessBackend>, bool)> {
    let kind = config.load_settings()?.process_backend;
    let use_systemd = match kind {
        ProcessBackendKind::Direct => false,
        ProcessBackendKind::Systemd => true,
        ProcessBackendKind::Auto => process::systemd_user_available(),
    };
    if use_systemd {
        Ok((Box::new(SystemdUserBackend::new(paths.clone())), true))
    } else {
        Ok((Box::new(DirectBackend::new(paths.clone())), false))
    }
}

fn dir_size(dir: &std::path::Path) -> u64 {
    let mut total = 0u64;
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    for entry in entries.flatten() {
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if metadata.is_dir() {
            total += dir_size(&entry.path());
        } else {
            total += metadata.len();
        }
    }
    total
}

fn generate_id(engine: EngineKind, existing: &[Instance]) -> String {
    loop {
        let suffix = &uuid::Uuid::new_v4().simple().to_string()[..6];
        let id = format!("{}-{}", engine.id_prefix(), suffix);
        if !existing.iter().any(|i| i.id == id) {
            return id;
        }
    }
}

fn unique_default_name(engine: EngineKind, manifest: &Manifest, existing: &[Instance]) -> String {
    let display_name = manifest
        .engine_catalog(engine)
        .map(|c| c.display_name.clone())
        .unwrap_or_else(|| engine.as_str().to_string());
    let base = display_name;
    let mut candidate = base.clone();
    let mut n = 2;
    while existing
        .iter()
        .any(|i| i.name.eq_ignore_ascii_case(&candidate))
    {
        candidate = format!("{base} {n}");
        n += 1;
    }
    candidate
}

fn now_rfc3339() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_id_has_engine_prefix() {
        let id = generate_id(EngineKind::Redis, &[]);
        assert!(id.starts_with("rds-"));
        assert_eq!(id.len(), "rds-".len() + 6);
    }

    #[test]
    fn unique_default_name_dedupes() {
        let manifest = Manifest::embedded().unwrap();
        let existing = vec![Instance {
            id: "rds-000001".into(),
            name: "Redis".into(),
            engine: EngineKind::Redis,
            version: "7.4.0".into(),
            port: 6379,
            autostart: false,
            created_at: now_rfc3339(),
            extra_args: vec![],
        }];
        let name = unique_default_name(EngineKind::Redis, &manifest, &existing);
        assert_eq!(name, "Redis 2");
    }
}
