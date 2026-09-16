use std::os::unix::process::CommandExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use nix::sys::signal::{self, Signal};
use nix::unistd::Pid;

use crate::engines::{LaunchSpec, StopSignal};
use crate::error::{Error, Result};
use crate::paths::Paths;
use crate::process::{ProcState, ProcessBackend};

/// Backend fallback yang men-spawn proses langsung (bukan lewat systemd),
/// dengan `setsid` agar server tetap hidup walau aplikasi ditutup. Dipakai
/// saat systemd --user tidak tersedia (§9.2).
pub struct DirectBackend {
    paths: Paths,
}

impl DirectBackend {
    pub fn new(paths: Paths) -> Self {
        Self { paths }
    }

    fn pid_file(&self, id: &str) -> PathBuf {
        self.paths.instance_run_dir(id).join("dbnest.pid")
    }

    fn read_pid(&self, id: &str) -> Option<u32> {
        std::fs::read_to_string(self.pid_file(id))
            .ok()
            .and_then(|s| s.trim().parse::<u32>().ok())
    }

    /// PID dianggap valid hanya jika `/proc/<pid>` ada **dan**
    /// `/proc/<pid>/exe` menunjuk ke `program`, supaya tidak salah membaca
    /// PID yang sudah dipakai proses lain.
    fn pid_is_our_process(pid: u32, program: &Path) -> bool {
        let exe_link = format!("/proc/{pid}/exe");
        match std::fs::read_link(&exe_link) {
            Ok(target) => {
                let canon_program =
                    std::fs::canonicalize(program).unwrap_or_else(|_| program.to_path_buf());
                target == canon_program || target == program
            }
            Err(_) => false,
        }
    }
}

fn spawn_detached(spec: &LaunchSpec, log_file: &Path) -> Result<u32> {
    if let Some(parent) = log_file.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::create_dir_all(&spec.working_dir)?;

    let stdout_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_file)?;
    let stderr_file = stdout_file.try_clone()?;

    let mut cmd = Command::new(&spec.program);
    cmd.args(&spec.args)
        .envs(spec.env.iter().cloned())
        .current_dir(&spec.working_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout_file))
        .stderr(Stdio::from(stderr_file));

    unsafe {
        cmd.pre_exec(|| {
            nix::unistd::setsid()
                .map(|_| ())
                .map_err(|errno| std::io::Error::from_raw_os_error(errno as i32))
        });
    }

    let child = cmd.spawn()?;
    Ok(child.id())
}

fn process_alive(pid: u32) -> bool {
    Path::new(&format!("/proc/{pid}")).exists()
}

#[async_trait::async_trait]
impl ProcessBackend for DirectBackend {
    async fn start(&self, id: &str, spec: &LaunchSpec, log_file: &Path) -> Result<()> {
        let run_dir = self.paths.instance_run_dir(id);
        std::fs::create_dir_all(&run_dir)?;

        let spec = spec.clone();
        let log_file = log_file.to_path_buf();
        let pid = tokio::task::spawn_blocking(move || spawn_detached(&spec, &log_file))
            .await
            .map_err(|e| Error::Process(e.to_string()))??;

        std::fs::write(self.pid_file(id), pid.to_string())?;
        Ok(())
    }

    async fn stop(&self, id: &str, spec: &LaunchSpec) -> Result<()> {
        let Some(pid) = self.read_pid(id) else {
            return Ok(());
        };
        if !Self::pid_is_our_process(pid, &spec.program) {
            let _ = std::fs::remove_file(self.pid_file(id));
            return Ok(());
        }

        let signal = match spec.stop_signal {
            StopSignal::Term => Signal::SIGTERM,
            StopSignal::Int => Signal::SIGINT,
        };
        let _ = signal::kill(Pid::from_raw(pid as i32), signal);

        let deadline = tokio::time::Instant::now() + spec.stop_timeout;
        while tokio::time::Instant::now() < deadline {
            if !process_alive(pid) {
                let _ = std::fs::remove_file(self.pid_file(id));
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }

        tracing::warn!(
            id,
            pid,
            "proses tidak berhenti tepat waktu, mengirim SIGKILL"
        );
        let _ = signal::kill(Pid::from_raw(pid as i32), Signal::SIGKILL);
        tokio::time::sleep(Duration::from_millis(200)).await;
        let _ = std::fs::remove_file(self.pid_file(id));
        Ok(())
    }

    async fn status(&self, id: &str) -> Result<ProcState> {
        let Some(pid) = self.read_pid(id) else {
            return Ok(ProcState::Stopped);
        };
        if process_alive(pid) {
            Ok(ProcState::Running { pid })
        } else {
            let _ = std::fs::remove_file(self.pid_file(id));
            Ok(ProcState::Stopped)
        }
    }

    async fn set_autostart(
        &self,
        _id: &str,
        _spec: &LaunchSpec,
        _log_file: &Path,
        _on: bool,
    ) -> Result<()> {
        // DirectBackend tidak punya mekanisme autostart OS-level; instance
        // dengan autostart=true dimulai lewat Manager::autostart_all() saat
        // aplikasi/tray dibuka (§9.2).
        Ok(())
    }

    async fn remove(&self, id: &str) -> Result<()> {
        let _ = std::fs::remove_file(self.pid_file(id));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths_in_tmp() -> (tempfile::TempDir, Paths) {
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths::under_root(tmp.path());
        (tmp, paths)
    }

    #[tokio::test]
    async fn status_is_stopped_when_no_pid_file() {
        let (_tmp, paths) = paths_in_tmp();
        let backend = DirectBackend::new(paths);
        let status = backend.status("rds-000001").await.unwrap();
        assert_eq!(status, ProcState::Stopped);
    }

    #[tokio::test]
    async fn start_stop_true_process() {
        let (_tmp, paths) = paths_in_tmp();
        let id = "rds-000001";
        let log_file = paths.instance_log_file(id);
        let backend = DirectBackend::new(paths.clone());

        let spec = LaunchSpec {
            program: PathBuf::from("/bin/sleep"),
            args: vec!["30".to_string()],
            env: vec![],
            working_dir: paths.instance_data_dir(id),
            stop_signal: StopSignal::Term,
            stop_timeout: Duration::from_secs(5),
        };

        backend.start(id, &spec, &log_file).await.unwrap();
        let status = backend.status(id).await.unwrap();
        assert!(matches!(status, ProcState::Running { .. }));

        backend.stop(id, &spec).await.unwrap();
        let status = backend.status(id).await.unwrap();
        assert_eq!(status, ProcState::Stopped);
    }
}
