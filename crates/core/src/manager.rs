use std::time::Duration;

use tokio::time::Instant;

use crate::config::ConfigStore;
use crate::engines::{self, InstanceCtx};
use crate::error::{Error, Result};
use crate::install::{self, Installer};
use crate::manifest::{self, Manifest};
use crate::model::{ConnectionInfo, EngineKind, Instance, InstanceStatus, ProgressEvent};
use crate::paths::Paths;
use crate::ports;
use crate::process::direct::DirectBackend;
use crate::process::{ProcState, ProcessBackend};

pub struct CreateInstanceRequest {
    pub engine: EngineKind,
    pub version: String,
    pub name: Option<String>,
    pub port: Option<u16>,
    pub autostart: bool,
}

/// Facade tingkat tinggi yang dipakai CLI dan (nanti) Tauri commands. Semua
/// logika bisnis hidup di sini; lapisan di atasnya hanya pembungkus tipis.
pub struct Manager {
    paths: Paths,
    config: ConfigStore,
    backend: Box<dyn ProcessBackend>,
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
        let backend: Box<dyn ProcessBackend> = Box::new(DirectBackend::new(paths.clone()));
        Ok(Self {
            paths,
            config,
            backend,
        })
    }

    pub fn manifest(&self) -> Result<Manifest> {
        Manifest::embedded()
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

    pub fn create_instance(&self, req: CreateInstanceRequest) -> Result<Instance> {
        let manifest = self.manifest()?;
        if manifest.version_entry(req.engine, &req.version).is_none() {
            return Err(Error::VersionUnavailable {
                engine: req.engine,
                version: req.version,
            });
        }
        let adapter = engines::adapter(req.engine)?;

        self.config.with_instances(|file| {
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
        })
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
                    |dir| adapter.post_install(dir),
                    |e| on_event(ProgressEvent::Install(e)),
                )
                .await?;
        }

        let ctx = self.ctx_for(&instance);

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

    fn ctx_for<'a>(&self, instance: &'a Instance) -> InstanceCtx<'a> {
        let bin_dir = self
            .paths
            .version_dir(instance.engine.as_str(), &instance.version);
        let lib_path = vec![bin_dir.join("lib")];
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
