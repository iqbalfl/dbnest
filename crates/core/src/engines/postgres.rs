use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::engines::{EngineAdapter, InstanceCtx, LaunchSpec, StopSignal};
use crate::error::{Error, Result};
use crate::model::{ConnectionInfo, EngineKind};

pub struct PostgresAdapter;

impl PostgresAdapter {
    fn lib_dir(bin_dir: &Path) -> PathBuf {
        bin_dir.join("lib")
    }
}

#[async_trait::async_trait]
impl EngineAdapter for PostgresAdapter {
    fn kind(&self) -> EngineKind {
        EngineKind::Postgres
    }

    fn default_port(&self) -> u16 {
        5432
    }

    fn client_bin_dirs(&self, bin_dir: &Path) -> Vec<PathBuf> {
        vec![bin_dir.join("bin")]
    }

    fn main_binary(&self, bin_dir: &Path) -> PathBuf {
        bin_dir.join("bin").join("postgres")
    }

    fn is_initialized(&self, ctx: &InstanceCtx) -> bool {
        ctx.data_dir.join("PG_VERSION").exists()
    }

    async fn init(&self, ctx: &InstanceCtx) -> Result<()> {
        tokio::fs::create_dir_all(&ctx.data_dir).await?;
        tokio::fs::create_dir_all(&ctx.run_dir).await?;

        let initdb = ctx.bin_dir.join("bin").join("initdb");
        let lib_dir = Self::lib_dir(&ctx.bin_dir);
        let data_dir_str = ctx.data_dir.to_string_lossy().to_string();

        let base_args = vec![
            "-D".to_string(),
            data_dir_str,
            "-U".to_string(),
            "postgres".to_string(),
            "--auth=trust".to_string(),
            "--encoding=UTF8".to_string(),
        ];

        let mut with_locale = base_args.clone();
        with_locale.push("--locale=C.UTF-8".to_string());

        let output = tokio::process::Command::new(&initdb)
            .args(&with_locale)
            .env("LD_LIBRARY_PATH", &lib_dir)
            .output()
            .await?;

        if !output.status.success() {
            let mut no_locale = base_args;
            no_locale.push("--no-locale".to_string());
            let retry = tokio::process::Command::new(&initdb)
                .args(&no_locale)
                .env("LD_LIBRARY_PATH", &lib_dir)
                .output()
                .await?;
            if !retry.status.success() {
                return Err(Error::Other(format!(
                    "initdb gagal: {}",
                    String::from_utf8_lossy(&retry.stderr)
                )));
            }
        }
        Ok(())
    }

    fn launch_spec(&self, ctx: &InstanceCtx) -> Result<LaunchSpec> {
        let port = ctx.instance.port.to_string();
        let args = vec![
            "-D".to_string(),
            ctx.data_dir.to_string_lossy().to_string(),
            "-p".to_string(),
            port,
            "-k".to_string(),
            ctx.run_dir.to_string_lossy().to_string(),
            "-c".to_string(),
            "listen_addresses=127.0.0.1".to_string(),
            "-c".to_string(),
            "logging_collector=off".to_string(),
        ];
        Ok(LaunchSpec {
            program: self.main_binary(&ctx.bin_dir),
            args,
            env: vec![(
                "LD_LIBRARY_PATH".to_string(),
                Self::lib_dir(&ctx.bin_dir).to_string_lossy().to_string(),
            )],
            working_dir: ctx.data_dir.clone(),
            stop_signal: StopSignal::Int,
            stop_timeout: Duration::from_secs(30),
        })
    }

    async fn health_check(&self, ctx: &InstanceCtx) -> Result<bool> {
        let pg_isready = ctx.bin_dir.join("bin").join("pg_isready");
        if pg_isready.exists() {
            let status = tokio::process::Command::new(&pg_isready)
                .args(["-h", "127.0.0.1", "-p", &ctx.instance.port.to_string()])
                .env("LD_LIBRARY_PATH", Self::lib_dir(&ctx.bin_dir))
                .status()
                .await?;
            return Ok(status.success());
        }
        Ok(
            tokio::net::TcpStream::connect(("127.0.0.1", ctx.instance.port))
                .await
                .is_ok(),
        )
    }

    fn connection_info(&self, ctx: &InstanceCtx) -> ConnectionInfo {
        let port = ctx.instance.port;
        ConnectionInfo {
            host: "127.0.0.1".to_string(),
            port,
            username: Some("postgres".to_string()),
            password: None,
            socket: Some(ctx.run_dir.clone()),
            url: format!("postgresql://postgres@127.0.0.1:{port}/postgres"),
        }
    }
}
