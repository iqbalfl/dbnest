use std::path::{Path, PathBuf};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;

use crate::engines::{EngineAdapter, InstanceCtx, LaunchSpec, StopSignal};
use crate::error::Result;
use crate::model::{ConnectionInfo, EngineKind};

pub struct RedisAdapter;

impl RedisAdapter {
    fn conf_path(ctx: &InstanceCtx) -> PathBuf {
        ctx.conf_dir.join("redis.conf")
    }
}

#[async_trait::async_trait]
impl EngineAdapter for RedisAdapter {
    fn kind(&self) -> EngineKind {
        EngineKind::Redis
    }

    fn default_port(&self) -> u16 {
        6379
    }

    fn client_bin_dirs(&self, bin_dir: &Path) -> Vec<PathBuf> {
        vec![bin_dir.join("bin")]
    }

    fn main_binary(&self, bin_dir: &Path) -> PathBuf {
        bin_dir.join("bin").join("redis-server")
    }

    fn is_initialized(&self, ctx: &InstanceCtx) -> bool {
        Self::conf_path(ctx).exists()
    }

    async fn init(&self, ctx: &InstanceCtx) -> Result<()> {
        tokio::fs::create_dir_all(&ctx.data_dir).await?;
        tokio::fs::create_dir_all(&ctx.conf_dir).await?;
        tokio::fs::create_dir_all(&ctx.run_dir).await?;
        let conf = render_redis_conf(ctx.instance.port, &ctx.data_dir);
        tokio::fs::write(Self::conf_path(ctx), conf).await?;
        Ok(())
    }

    fn launch_spec(&self, ctx: &InstanceCtx) -> Result<LaunchSpec> {
        Ok(LaunchSpec {
            program: self.main_binary(&ctx.bin_dir),
            args: vec![Self::conf_path(ctx).to_string_lossy().to_string()],
            env: vec![],
            working_dir: ctx.data_dir.clone(),
            stop_signal: StopSignal::Term,
            stop_timeout: Duration::from_secs(30),
        })
    }

    async fn health_check(&self, ctx: &InstanceCtx) -> Result<bool> {
        Ok(ping(ctx.instance.port).await.unwrap_or(false))
    }

    fn connection_info(&self, ctx: &InstanceCtx) -> ConnectionInfo {
        let port = ctx.instance.port;
        ConnectionInfo {
            host: "127.0.0.1".to_string(),
            port,
            username: None,
            password: None,
            socket: None,
            url: format!("redis://127.0.0.1:{port}"),
        }
    }
}

async fn ping(port: u16) -> std::io::Result<bool> {
    let mut stream = timeout(
        Duration::from_secs(2),
        TcpStream::connect(("127.0.0.1", port)),
    )
    .await??;
    stream.write_all(b"PING\r\n").await?;
    let mut buf = [0u8; 32];
    let n = timeout(Duration::from_secs(2), stream.read(&mut buf)).await??;
    Ok(buf[..n].starts_with(b"+PONG"))
}

/// Render isi `redis.conf` persis seperti spesifikasi DESIGN.md §7.2.
pub fn render_redis_conf(port: u16, data_dir: &Path) -> String {
    format!(
        "port {port}\n\
         bind 127.0.0.1\n\
         protected-mode yes\n\
         dir {dir}\n\
         daemonize no\n\
         logfile \"\"\n\
         save 3600 1 300 100 60 10000\n",
        port = port,
        dir = data_dir.display(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_redis_conf_matches_spec() {
        let conf = render_redis_conf(
            6380,
            Path::new("/home/u/.local/share/dbnest/instances/rds-1/data"),
        );
        assert!(conf.contains("port 6380\n"));
        assert!(conf.contains("bind 127.0.0.1\n"));
        assert!(conf.contains("protected-mode yes\n"));
        assert!(conf.contains("dir /home/u/.local/share/dbnest/instances/rds-1/data\n"));
        assert!(conf.contains("daemonize no\n"));
        assert!(conf.contains("logfile \"\"\n"));
        assert!(conf.contains("save 3600 1 300 100 60 10000\n"));
    }
}
