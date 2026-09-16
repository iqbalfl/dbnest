use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::compat;
use crate::engines::mysql_family::admin_ping;
use crate::engines::{join_lib_path, EngineAdapter, InstanceCtx, LaunchSpec, StopSignal};
use crate::error::{Error, Result};
use crate::model::{ConnectionInfo, EngineKind};
use crate::paths::Paths;

pub struct MysqlAdapter;

impl MysqlAdapter {
    fn admin_binary(bin_dir: &Path) -> PathBuf {
        bin_dir.join("bin").join("mysqladmin")
    }

    /// `<compat-lib>:B/lib/private` (§7.2). `compat-lib` datang dari
    /// `ctx.lib_path`, diisi Manager dari `Paths::compat_lib_dir()`.
    fn lib_dirs(ctx: &InstanceCtx) -> Vec<PathBuf> {
        let mut dirs = ctx.lib_path.clone();
        dirs.push(ctx.bin_dir.join("lib").join("private"));
        dirs
    }

    fn socket_path(ctx: &InstanceCtx) -> PathBuf {
        ctx.run_dir.join("mysql.sock")
    }
}

#[async_trait::async_trait]
impl EngineAdapter for MysqlAdapter {
    fn kind(&self) -> EngineKind {
        EngineKind::Mysql
    }

    fn default_port(&self) -> u16 {
        3306
    }

    fn client_bin_dirs(&self, bin_dir: &Path) -> Vec<PathBuf> {
        vec![bin_dir.join("bin")]
    }

    fn main_binary(&self, bin_dir: &Path) -> PathBuf {
        bin_dir.join("bin").join("mysqld")
    }

    fn is_initialized(&self, ctx: &InstanceCtx) -> bool {
        ctx.data_dir.join("mysql").is_dir()
    }

    async fn init(&self, ctx: &InstanceCtx) -> Result<()> {
        tokio::fs::create_dir_all(&ctx.data_dir).await?;
        tokio::fs::create_dir_all(&ctx.run_dir).await?;

        let mysqld = self.main_binary(&ctx.bin_dir);
        let ld_library_path = join_lib_path(&Self::lib_dirs(ctx));

        // "--no-defaults" harus argumen pertama, supaya /etc/mysql/my.cnf
        // tidak ikut terbaca.
        let output = tokio::process::Command::new(&mysqld)
            .arg("--no-defaults")
            .arg("--initialize-insecure")
            .arg(format!("--basedir={}", ctx.bin_dir.display()))
            .arg(format!("--datadir={}", ctx.data_dir.display()))
            .env("LD_LIBRARY_PATH", &ld_library_path)
            .output()
            .await?;

        if !output.status.success() {
            return Err(Error::Other(format!(
                "mysqld --initialize-insecure gagal: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }
        Ok(())
    }

    fn launch_spec(&self, ctx: &InstanceCtx) -> Result<LaunchSpec> {
        let pid_file = ctx.run_dir.join("mysqld.pid");
        let args = vec![
            "--no-defaults".to_string(),
            format!("--basedir={}", ctx.bin_dir.display()),
            format!("--datadir={}", ctx.data_dir.display()),
            format!("--port={}", ctx.instance.port),
            "--bind-address=127.0.0.1".to_string(),
            format!("--socket={}", Self::socket_path(ctx).display()),
            format!("--pid-file={}", pid_file.display()),
            "--mysqlx=OFF".to_string(),
            format!("--log-error={}", ctx.log_file.display()),
        ];
        Ok(LaunchSpec {
            program: self.main_binary(&ctx.bin_dir),
            args,
            env: vec![(
                "LD_LIBRARY_PATH".to_string(),
                join_lib_path(&Self::lib_dirs(ctx)),
            )],
            working_dir: ctx.data_dir.clone(),
            stop_signal: StopSignal::Term,
            stop_timeout: Duration::from_secs(60),
        })
    }

    async fn health_check(&self, ctx: &InstanceCtx) -> Result<bool> {
        if tokio::net::TcpStream::connect(("127.0.0.1", ctx.instance.port))
            .await
            .is_err()
        {
            return Ok(false);
        }
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

    fn post_install(&self, _bin_dir: &Path, paths: &Paths) -> Result<()> {
        // Ubuntu 24.04+: buat symlink compat libaio.so.1 -> libaio.so.1t64
        // tanpa sudo, lalu compat-lib/ otomatis masuk LD_LIBRARY_PATH lewat
        // ctx.lib_path (§8.2).
        compat::ensure_libaio_compat_symlink(paths)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Instance;
    use std::path::PathBuf;

    fn ctx(instance: &Instance) -> InstanceCtx<'_> {
        InstanceCtx {
            instance,
            bin_dir: PathBuf::from("/data/binaries/mysql/8.4.3"),
            data_dir: PathBuf::from("/data/instances/my-1/data"),
            run_dir: PathBuf::from("/data/instances/my-1/run"),
            log_file: PathBuf::from("/data/instances/my-1/logs/engine.log"),
            conf_dir: PathBuf::from("/data/instances/my-1/conf"),
            lib_path: vec![PathBuf::from("/data/compat-lib")],
        }
    }

    fn instance() -> Instance {
        Instance {
            id: "my-abc123".into(),
            name: "mysql dev".into(),
            engine: EngineKind::Mysql,
            version: "8.4.3".into(),
            port: 3307,
            autostart: false,
            created_at: "2026-09-16T10:00:00Z".into(),
            extra_args: vec![],
        }
    }

    #[test]
    fn launch_spec_puts_no_defaults_first_and_disables_mysqlx() {
        let instance = instance();
        let ctx = ctx(&instance);
        let spec = MysqlAdapter.launch_spec(&ctx).unwrap();

        assert_eq!(spec.args.first(), Some(&"--no-defaults".to_string()));
        assert!(spec.args.contains(&"--mysqlx=OFF".to_string()));
        assert!(spec.args.contains(&"--port=3307".to_string()));
        assert!(spec.args.contains(&"--bind-address=127.0.0.1".to_string()));
        assert_eq!(spec.stop_signal, StopSignal::Term);
        assert_eq!(spec.stop_timeout, Duration::from_secs(60));
    }

    #[test]
    fn launch_spec_ld_library_path_includes_compat_and_private_lib() {
        let instance = instance();
        let ctx = ctx(&instance);
        let spec = MysqlAdapter.launch_spec(&ctx).unwrap();

        let (key, value) = &spec.env[0];
        assert_eq!(key, "LD_LIBRARY_PATH");
        assert_eq!(
            value,
            "/data/compat-lib:/data/binaries/mysql/8.4.3/lib/private"
        );
    }

    #[test]
    fn connection_info_has_no_password_and_root_user() {
        let instance = instance();
        let ctx = ctx(&instance);
        let info = MysqlAdapter.connection_info(&ctx);

        assert_eq!(info.username.as_deref(), Some("root"));
        assert!(info.password.is_none());
        assert_eq!(info.url, "mysql://root@127.0.0.1:3307");
        assert_eq!(
            info.socket,
            Some(PathBuf::from("/data/instances/my-1/run/mysql.sock"))
        );
    }
}
