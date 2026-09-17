//! Backend `systemctl --user` (DESIGN.md §9.1).

use std::collections::HashMap;
use std::path::Path;

use crate::engines::{LaunchSpec, StopSignal};
use crate::error::{Error, Result};
use crate::paths::Paths;
use crate::process::{ProcState, ProcessBackend};

pub struct SystemdUserBackend {
    paths: Paths,
}

impl SystemdUserBackend {
    pub fn new(paths: Paths) -> Self {
        Self { paths }
    }

    fn unit_name(id: &str) -> String {
        format!("dbnest-{id}.service")
    }

    /// Tulis (atau timpa) unit file lalu `daemon-reload`, supaya perubahan
    /// pada `LaunchSpec` (mis. port berubah) langsung diikuti systemd.
    async fn write_unit(&self, id: &str, spec: &LaunchSpec, log_file: &Path) -> Result<()> {
        // `StandardOutput=append:` hanya membuat *file*-nya; direktori
        // induknya harus sudah ada. Kalau tidak, systemd gagal menyiapkan
        // stdio, prosesnya tidak pernah jalan, log tetap kosong, dan
        // `Restart=on-failure` membuat unit itu bolak-balik hidup-mati.
        // DirectBackend sudah melakukan ini di `spawn_detached`; systemd
        // tidak punya padanannya, jadi harus dikerjakan di sini.
        prepare_runtime_dirs(spec, log_file).await?;

        let unit_path = self.paths.systemd_unit_file(id);
        if let Some(dir) = unit_path.parent() {
            tokio::fs::create_dir_all(dir).await?;
        }
        let contents = render_unit(id, spec, log_file);
        tokio::fs::write(&unit_path, contents).await?;
        self.daemon_reload().await
    }

    async fn daemon_reload(&self) -> Result<()> {
        run_systemctl_ok(&["--user", "daemon-reload"]).await
    }
}

#[async_trait::async_trait]
impl ProcessBackend for SystemdUserBackend {
    async fn start(&self, id: &str, spec: &LaunchSpec, log_file: &Path) -> Result<()> {
        self.write_unit(id, spec, log_file).await?;
        run_systemctl_ok(&["--user", "start", &Self::unit_name(id)]).await
    }

    async fn stop(&self, id: &str, _spec: &LaunchSpec) -> Result<()> {
        run_systemctl_ok(&["--user", "stop", &Self::unit_name(id)]).await
    }

    async fn status(&self, id: &str) -> Result<ProcState> {
        let unit = Self::unit_name(id);
        let output = run_systemctl(&[
            "--user",
            "show",
            &unit,
            "-p",
            "ActiveState,SubState,MainPID,Result,ExecMainStatus",
        ])
        .await?;
        if !output.status.success() {
            return Ok(ProcState::Stopped);
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        let props = parse_show_output(&stdout);
        let active_state = props.get("ActiveState").map(String::as_str).unwrap_or("");
        let main_pid: u32 = props
            .get("MainPID")
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);

        // Pemetaan status per §9.1: active -> Running, activating ->
        // Starting, deactivating -> Stopping, failed -> Failed, selain itu
        // -> Stopped. `ProcState` sendiri hanya punya tiga varian
        // (Running/Stopped/Failed); Starting/Stopping diratakan ke Stopped
        // di level ini — Manager yang memutuskan makna Starting/Stopping
        // lewat alur start()/stop()-nya sendiri.
        match active_state {
            "active" => Ok(ProcState::Running { pid: main_pid }),
            "failed" => Ok(ProcState::Failed {
                message: failure_message(&props),
            }),
            _ => Ok(ProcState::Stopped),
        }
    }

    async fn set_autostart(
        &self,
        id: &str,
        spec: &LaunchSpec,
        log_file: &Path,
        on: bool,
    ) -> Result<()> {
        self.write_unit(id, spec, log_file).await?;
        let unit = Self::unit_name(id);
        if on {
            run_systemctl_ok(&["--user", "enable", &unit]).await
        } else {
            run_systemctl_ok(&["--user", "disable", &unit]).await
        }
    }

    async fn remove(&self, id: &str) -> Result<()> {
        let unit = Self::unit_name(id);
        let _ = run_systemctl_ok(&["--user", "stop", &unit]).await;
        let _ = run_systemctl_ok(&["--user", "disable", &unit]).await;
        let unit_path = self.paths.systemd_unit_file(id);
        if unit_path.exists() {
            tokio::fs::remove_file(&unit_path).await?;
        }
        self.daemon_reload().await
    }
}

/// Rangkai pesan gagal dari properti `systemctl show`. `Result` saja
/// ("exit-code") tidak memberi tahu apa-apa, jadi status keluar prosesnya
/// ikut disebut kalau ada.
fn failure_message(props: &HashMap<String, String>) -> String {
    let result = props
        .get("Result")
        .map(String::as_str)
        .filter(|s| !s.is_empty() && *s != "success")
        .unwrap_or("gagal");
    match props
        .get("ExecMainStatus")
        .and_then(|s| s.parse::<i32>().ok())
        .filter(|code| *code != 0)
    {
        Some(code) => format!("{result} (status keluar {code})"),
        None => result.to_string(),
    }
}

/// Siapkan direktori yang systemd sendiri tidak mau buatkan: folder induk
/// file log dan `WorkingDirectory=`. Keduanya bikin unit gagal start kalau
/// belum ada.
async fn prepare_runtime_dirs(spec: &LaunchSpec, log_file: &Path) -> Result<()> {
    if let Some(dir) = log_file.parent() {
        tokio::fs::create_dir_all(dir).await?;
    }
    tokio::fs::create_dir_all(&spec.working_dir).await?;
    Ok(())
}

async fn run_systemctl(args: &[&str]) -> Result<std::process::Output> {
    tokio::process::Command::new("systemctl")
        .args(args)
        .output()
        .await
        .map_err(|e| Error::Systemd(format!("gagal menjalankan systemctl: {e}")))
}

async fn run_systemctl_ok(args: &[&str]) -> Result<()> {
    let output = run_systemctl(args).await?;
    if !output.status.success() {
        return Err(Error::Systemd(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }
    Ok(())
}

fn parse_show_output(output: &str) -> HashMap<String, String> {
    output
        .lines()
        .filter_map(|line| line.split_once('='))
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

/// Render isi unit file persis mengikuti template di §9.1. **Jangan**
/// menambahkan `After=default.target`: karena unit ini `WantedBy=
/// default.target`, target tersebut sudah otomatis terurut setelah unit
/// ini, jadi baris itu akan menimbulkan ordering cycle.
fn render_unit(id: &str, spec: &LaunchSpec, log_file: &Path) -> String {
    let exec_start = std::iter::once(spec.program.to_string_lossy().to_string())
        .chain(spec.args.iter().cloned())
        .map(|arg| systemd_escape_arg(&arg))
        .collect::<Vec<_>>()
        .join(" ");

    let env_lines: String = spec
        .env
        .iter()
        .map(|(k, v)| {
            format!(
                "Environment=\"{}={}\"\n",
                k,
                v.replace('\\', "\\\\").replace('"', "\\\"")
            )
        })
        .collect();

    let kill_signal = match spec.stop_signal {
        StopSignal::Term => "SIGTERM",
        StopSignal::Int => "SIGINT",
    };

    format!(
        "[Unit]\n\
         Description=DBnest instance {id}\n\
         StartLimitIntervalSec=60\n\
         StartLimitBurst=3\n\
         \n\
         [Service]\n\
         Type=simple\n\
         WorkingDirectory={working_dir}\n\
         {env_lines}\
         ExecStart={exec_start}\n\
         KillSignal={kill_signal}\n\
         TimeoutStopSec={timeout_secs}\n\
         Restart=on-failure\n\
         RestartSec=3\n\
         StandardOutput=append:{log}\n\
         StandardError=append:{log}\n\
         \n\
         [Install]\n\
         WantedBy=default.target\n",
        working_dir = spec.working_dir.display(),
        timeout_secs = spec.stop_timeout.as_secs(),
        log = log_file.display(),
    )
}

/// Quote satu argumen untuk baris `ExecStart=`, sesuai aturan quoting
/// systemd (systemd.service(5)/systemd.syntax(7)): bungkus dengan tanda
/// kutip ganda kalau mengandung whitespace atau karakter khusus, escape
/// backslash dan kutip ganda, dan gandakan `%` supaya tidak dianggap
/// specifier. Path bisa mengandung spasi, jadi ini wajib.
pub fn systemd_escape_arg(arg: &str) -> String {
    let needs_quoting = arg.is_empty()
        || arg
            .chars()
            .any(|c| c.is_whitespace() || c == '"' || c == '\\');
    let mut escaped = String::with_capacity(arg.len());
    for ch in arg.chars() {
        match ch {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '%' => escaped.push_str("%%"),
            other => escaped.push(other),
        }
    }
    if needs_quoting {
        format!("\"{escaped}\"")
    } else {
        escaped
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::Duration;

    #[test]
    fn escape_plain_arg_needs_no_quoting() {
        assert_eq!(systemd_escape_arg("--port=5432"), "--port=5432");
    }

    #[test]
    fn escape_arg_with_space_gets_quoted() {
        assert_eq!(
            systemd_escape_arg("/home/user/my data/bin/postgres"),
            "\"/home/user/my data/bin/postgres\""
        );
    }

    #[test]
    fn escape_arg_with_quote_and_backslash() {
        assert_eq!(
            systemd_escape_arg(r#"weird"arg\here"#),
            r#""weird\"arg\\here""#
        );
    }

    #[test]
    fn escape_arg_doubles_percent_sign() {
        assert_eq!(systemd_escape_arg("100%done"), "100%%done");
    }

    #[test]
    fn escape_empty_arg_becomes_empty_quotes() {
        assert_eq!(systemd_escape_arg(""), "\"\"");
    }

    #[test]
    fn render_unit_has_no_after_default_target() {
        let spec = LaunchSpec {
            program: PathBuf::from("/opt/dbnest/bin/redis-server"),
            args: vec!["/tmp/redis.conf".to_string()],
            env: vec![],
            working_dir: PathBuf::from("/tmp/data"),
            stop_signal: StopSignal::Term,
            stop_timeout: Duration::from_secs(30),
        };
        let unit = render_unit("rds-abc123", &spec, Path::new("/tmp/log/engine.log"));
        assert!(!unit.contains("After=default.target"));
        assert!(unit.contains("WantedBy=default.target"));
        assert!(unit.contains("ExecStart=/opt/dbnest/bin/redis-server /tmp/redis.conf"));
        assert!(unit.contains("KillSignal=SIGTERM"));
        assert!(unit.contains("TimeoutStopSec=30"));
        assert!(unit.contains("StandardOutput=append:/tmp/log/engine.log"));
    }

    #[test]
    fn render_unit_quotes_paths_with_spaces_in_exec_start() {
        let spec = LaunchSpec {
            program: PathBuf::from("/opt/dbnest bin/postgres"),
            args: vec!["-D".to_string(), "/tmp/my data".to_string()],
            env: vec![],
            working_dir: PathBuf::from("/tmp"),
            stop_signal: StopSignal::Int,
            stop_timeout: Duration::from_secs(30),
        };
        let unit = render_unit("pg-abc123", &spec, Path::new("/tmp/log/engine.log"));
        assert!(unit.contains("ExecStart=\"/opt/dbnest bin/postgres\" -D \"/tmp/my data\""));
        assert!(unit.contains("KillSignal=SIGINT"));
    }

    #[test]
    fn render_unit_includes_env_vars() {
        let spec = LaunchSpec {
            program: PathBuf::from("/opt/dbnest/bin/mysqld"),
            args: vec![],
            env: vec![("LD_LIBRARY_PATH".to_string(), "/opt/dbnest/lib".to_string())],
            working_dir: PathBuf::from("/tmp"),
            stop_signal: StopSignal::Term,
            stop_timeout: Duration::from_secs(60),
        };
        let unit = render_unit("my-abc123", &spec, Path::new("/tmp/log/engine.log"));
        assert!(unit.contains("Environment=\"LD_LIBRARY_PATH=/opt/dbnest/lib\"\n"));
    }

    fn sample_spec(working_dir: PathBuf) -> LaunchSpec {
        LaunchSpec {
            program: PathBuf::from("/opt/dbnest/bin/postgres"),
            args: vec!["-D".to_string(), "/tmp/data".to_string()],
            env: vec![],
            working_dir,
            stop_signal: StopSignal::Int,
            stop_timeout: Duration::from_secs(30),
        }
    }

    #[tokio::test]
    async fn prepare_runtime_dirs_creates_log_dir_and_working_dir() {
        // systemd hanya membuat *file* log, bukan direktorinya. Kalau
        // direktori itu tidak ada, unit gagal start tanpa menulis apa pun ke
        // log — persis gejala "status hidup-mati terus, log kosong".
        let tmp = tempfile::tempdir().unwrap();
        let log_file = tmp.path().join("instances/pg-abc123/logs/engine.log");
        let working_dir = tmp.path().join("instances/pg-abc123/data");
        assert!(!log_file.parent().unwrap().exists());

        prepare_runtime_dirs(&sample_spec(working_dir.clone()), &log_file)
            .await
            .unwrap();

        assert!(log_file.parent().unwrap().is_dir());
        assert!(working_dir.is_dir());
    }

    #[tokio::test]
    async fn prepare_runtime_dirs_is_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        let log_file = tmp.path().join("logs/engine.log");
        let spec = sample_spec(tmp.path().join("data"));
        prepare_runtime_dirs(&spec, &log_file).await.unwrap();
        prepare_runtime_dirs(&spec, &log_file).await.unwrap();
        assert!(log_file.parent().unwrap().is_dir());
    }

    #[test]
    fn render_unit_limits_auto_restart() {
        // Tanpa batas ini, `Restart=on-failure` mengulang selamanya tiap 3
        // detik, sehingga UI melihat status berkedip alih-alih satu
        // kegagalan yang jelas.
        let unit = render_unit(
            "pg-abc123",
            &sample_spec(PathBuf::from("/tmp/data")),
            Path::new("/tmp/log/engine.log"),
        );
        assert!(unit.contains("StartLimitIntervalSec=60"));
        assert!(unit.contains("StartLimitBurst=3"));
        // Batas start ada di [Unit], bukan [Service].
        let unit_section = unit.split("[Service]").next().unwrap();
        assert!(unit_section.contains("StartLimitBurst=3"));
    }

    #[test]
    fn failure_message_mentions_exit_status() {
        let props = parse_show_output("Result=exit-code\nExecMainStatus=1\n");
        assert_eq!(failure_message(&props), "exit-code (status keluar 1)");
    }

    #[test]
    fn failure_message_without_exit_status_falls_back_to_result() {
        let props = parse_show_output("Result=timeout\nExecMainStatus=0\n");
        assert_eq!(failure_message(&props), "timeout");
    }

    #[test]
    fn failure_message_without_any_detail_is_still_readable() {
        let props = parse_show_output("ActiveState=failed\n");
        assert_eq!(failure_message(&props), "gagal");
    }

    #[test]
    fn parse_show_output_extracts_key_value_pairs() {
        let output = "ActiveState=active\nSubState=running\nMainPID=1234\nResult=success\n";
        let props = parse_show_output(output);
        assert_eq!(props.get("ActiveState").map(String::as_str), Some("active"));
        assert_eq!(props.get("MainPID").map(String::as_str), Some("1234"));
    }
}
