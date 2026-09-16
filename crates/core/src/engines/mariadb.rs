use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::engines::mysql_family::{admin_ping, find_first_existing};
use crate::engines::{EngineAdapter, InstanceCtx, LaunchSpec, StopSignal};
use crate::error::{Error, Result};
use crate::model::{ConnectionInfo, EngineKind};

pub struct MariadbAdapter;

impl MariadbAdapter {
    /// Versi lama memakai nama `mysqld`, versi baru `mariadbd` (catatan
    /// §7.2). Adapter mencari kedua nama.
    fn daemon_binary(bin_dir: &Path) -> PathBuf {
        find_first_existing(bin_dir, &["bin/mariadbd", "bin/mysqld"])
    }

    fn install_db_script(bin_dir: &Path) -> PathBuf {
        find_first_existing(
            bin_dir,
            &[
                "scripts/mariadb-install-db",
                "scripts/mysql_install_db",
                "bin/mariadb-install-db",
                "bin/mysql_install_db",
            ],
        )
    }

    fn admin_binary(bin_dir: &Path) -> PathBuf {
        find_first_existing(bin_dir, &["bin/mariadb-admin", "bin/mysqladmin"])
    }

    fn socket_path(ctx: &InstanceCtx) -> PathBuf {
        ctx.run_dir.join("mysql.sock")
    }
}

#[async_trait::async_trait]
impl EngineAdapter for MariadbAdapter {
    fn kind(&self) -> EngineKind {
        EngineKind::Mariadb
    }

    fn default_port(&self) -> u16 {
        3306
    }

    fn client_bin_dirs(&self, bin_dir: &Path) -> Vec<PathBuf> {
        vec![bin_dir.join("bin")]
    }

    fn main_binary(&self, bin_dir: &Path) -> PathBuf {
        Self::daemon_binary(bin_dir)
    }

    fn is_initialized(&self, ctx: &InstanceCtx) -> bool {
        ctx.data_dir.join("mysql").is_dir()
    }

    async fn init(&self, ctx: &InstanceCtx) -> Result<()> {
        tokio::fs::create_dir_all(&ctx.data_dir).await?;
        tokio::fs::create_dir_all(&ctx.run_dir).await?;

        let install_db = Self::install_db_script(&ctx.bin_dir);
        let output = tokio::process::Command::new(&install_db)
            .arg("--no-defaults")
            .arg(format!("--basedir={}", ctx.bin_dir.display()))
            .arg(format!("--datadir={}", ctx.data_dir.display()))
            .arg("--auth-root-authentication-method=normal")
            .arg("--skip-test-db")
            .output()
            .await?;

        if !output.status.success() {
            return Err(Error::Other(format!(
                "mariadb-install-db gagal: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }
        Ok(())
    }

    fn launch_spec(&self, ctx: &InstanceCtx) -> Result<LaunchSpec> {
        let pid_file = ctx.run_dir.join("mariadbd.pid");
        let args = vec![
            "--no-defaults".to_string(),
            format!("--basedir={}", ctx.bin_dir.display()),
            format!("--datadir={}", ctx.data_dir.display()),
            format!("--port={}", ctx.instance.port),
            "--bind-address=127.0.0.1".to_string(),
            format!("--socket={}", Self::socket_path(ctx).display()),
            format!("--pid-file={}", pid_file.display()),
            format!("--log-error={}", ctx.log_file.display()),
        ];
        Ok(LaunchSpec {
            program: Self::daemon_binary(&ctx.bin_dir),
            args,
            env: vec![],
            working_dir: ctx.data_dir.clone(),
            stop_signal: StopSignal::Term,
            stop_timeout: Duration::from_secs(60),
        })
    }

    async fn health_check(&self, ctx: &InstanceCtx) -> Result<bool> {
        Ok(admin_ping(&Self::admin_binary(&ctx.bin_dir), ctx.instance.port).await)
    }

    fn connection_info(&self, ctx: &InstanceCtx) -> ConnectionInfo {
        let port = ctx.instance.port;
        ConnectionInfo {
            host: "127.0.0.1".to_string(),
            port,
            username: Some("root".to_string()),
            password: None,
            socket: Some(Self::socket_path(ctx)),
            url: format!("mysql://root@127.0.0.1:{port}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Instance;

    fn instance() -> Instance {
        Instance {
            id: "mdb-abc123".into(),
            name: "mariadb dev".into(),
            engine: EngineKind::Mariadb,
            version: "11.4.4".into(),
            port: 3308,
            autostart: false,
            created_at: "2026-09-16T10:00:00Z".into(),
            extra_args: vec![],
        }
    }

    fn ctx(instance: &Instance) -> InstanceCtx<'_> {
        InstanceCtx {
            instance,
            bin_dir: PathBuf::from("/data/binaries/mariadb/11.4.4"),
            data_dir: PathBuf::from("/data/instances/mdb-1/data"),
            run_dir: PathBuf::from("/data/instances/mdb-1/run"),
            log_file: PathBuf::from("/data/instances/mdb-1/logs/engine.log"),
            conf_dir: PathBuf::from("/data/instances/mdb-1/conf"),
            lib_path: vec![],
        }
    }

    #[test]
    fn launch_spec_puts_no_defaults_first_and_has_no_mysqlx_flag() {
        let instance = instance();
        let ctx = ctx(&instance);
        let spec = MariadbAdapter.launch_spec(&ctx).unwrap();

        assert_eq!(spec.args.first(), Some(&"--no-defaults".to_string()));
        assert!(!spec.args.iter().any(|a| a.contains("mysqlx")));
        assert!(spec.args.contains(&"--port=3308".to_string()));
        assert!(spec.env.is_empty());
        assert_eq!(spec.stop_timeout, Duration::from_secs(60));
    }

    #[test]
    fn daemon_binary_falls_back_to_mysqld_name_when_mariadbd_missing() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("bin")).unwrap();
        std::fs::write(tmp.path().join("bin").join("mysqld"), b"").unwrap();
        assert_eq!(
            MariadbAdapter::daemon_binary(tmp.path()),
            tmp.path().join("bin/mysqld")
        );
    }

    #[test]
    fn connection_info_uses_mysql_url_scheme() {
        let instance = instance();
        let ctx = ctx(&instance);
        let info = MariadbAdapter.connection_info(&ctx);
        assert_eq!(info.url, "mysql://root@127.0.0.1:3308");
    }
}
