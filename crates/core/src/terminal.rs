//! Deteksi & siapkan proses untuk "Open Terminal" (DESIGN.md §11.1).

use std::path::{Path, PathBuf};

use crate::engines::{join_lib_path, EngineAdapter, InstanceCtx};
use crate::error::Result;
use crate::model::EngineKind;

const FALLBACK_CANDIDATES: &[&str] = &[
    "x-terminal-emulator",
    "gnome-terminal",
    "ptyxis",
    "konsole",
    "xfce4-terminal",
    "kitty",
    "alacritty",
    "wezterm",
    "xterm",
];

pub struct TerminalLaunch {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

/// Cari terminal emulator sesuai urutan §11.1: `settings.terminal_command`
/// → `$TERMINAL` → `x-terminal-emulator` → daftar emulator populer.
pub fn find_terminal(settings_terminal_command: Option<&str>) -> Option<String> {
    if let Some(cmd) = settings_terminal_command {
        if !cmd.trim().is_empty() {
            return Some(cmd.to_string());
        }
    }
    if let Ok(term) = std::env::var("TERMINAL") {
        if !term.trim().is_empty() {
            return Some(term);
        }
    }
    FALLBACK_CANDIDATES
        .iter()
        .find(|candidate| is_on_path(candidate))
        .map(|s| s.to_string())
}

fn is_on_path(bin: &str) -> bool {
    let Some(path_var) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path_var).any(|dir| dir.join(bin).is_file())
}

/// Argumen untuk menjalankan `shell` di dalam terminal, berbeda tiap
/// emulator (`--`, `-e`, `-x`).
fn shell_invocation_args(terminal_cmd: &str, shell: &str) -> Vec<String> {
    let name = Path::new(terminal_cmd)
        .file_name()
        .and_then(|f| f.to_str())
        .unwrap_or(terminal_cmd);
    match name {
        "gnome-terminal" | "ptyxis" => vec!["--".to_string(), shell.to_string()],
        "konsole" | "alacritty" | "xterm" | "x-terminal-emulator" => {
            vec!["-e".to_string(), shell.to_string()]
        }
        "xfce4-terminal" => vec!["-x".to_string(), shell.to_string()],
        "wezterm" => vec!["start".to_string(), "--".to_string(), shell.to_string()],
        "kitty" => vec![shell.to_string()],
        _ => vec!["-e".to_string(), shell.to_string()],
    }
}

/// Petunjuk yang perlu ditampilkan ke pengguna sebelum membuka terminal,
/// untuk engine yang tidak punya variabel env koneksi (Redis: §11.1).
pub fn connection_hint(engine: EngineKind, port: u16) -> Option<String> {
    match engine {
        EngineKind::Redis => Some(format!("redis-cli -p {port}")),
        _ => None,
    }
}

/// Environment untuk bekerja dengan satu instance: PATH sudah diawali
/// direktori client engine-nya, plus variabel koneksi per engine (§11.1).
/// Dipakai "Open Terminal" di GUI maupun `dbnest shell` / `dbnest env`.
pub fn instance_env(adapter: &dyn EngineAdapter, ctx: &InstanceCtx) -> Vec<(String, String)> {
    let bin_dirs = adapter.client_bin_dirs(&ctx.bin_dir);
    let existing_path = std::env::var("PATH").unwrap_or_default();

    // Warisi env dari launch_spec (mis. LD_LIBRARY_PATH) supaya client CLI
    // memakai library yang sama dengan server.
    let mut env = adapter
        .launch_spec(ctx)
        .map(|spec| spec.env)
        .unwrap_or_default();
    env.push((
        "PATH".to_string(),
        format!("{}:{existing_path}", join_lib_path(&bin_dirs)),
    ));

    let port = ctx.instance.port;
    match adapter.kind() {
        EngineKind::Postgres => {
            env.push(("PGHOST".to_string(), "127.0.0.1".to_string()));
            env.push(("PGPORT".to_string(), port.to_string()));
            env.push(("PGUSER".to_string(), "postgres".to_string()));
        }
        EngineKind::Mysql | EngineKind::Mariadb => {
            env.push(("MYSQL_HOST".to_string(), "127.0.0.1".to_string()));
            env.push(("MYSQL_TCP_PORT".to_string(), port.to_string()));
        }
        EngineKind::Redis | EngineKind::Mongodb => {}
    }
    env
}

/// Shell login pengguna, untuk `dbnest shell` dan sebagai program yang
/// dijalankan di dalam terminal emulator.
pub fn user_shell() -> String {
    std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string())
}

/// Siapkan program, argumen, dan environment untuk membuka `terminal_cmd`
/// dengan PATH/env yang sudah mengarah ke versi engine yang benar.
pub fn build_launch(
    terminal_cmd: &str,
    adapter: &dyn EngineAdapter,
    ctx: &InstanceCtx,
) -> Result<TerminalLaunch> {
    let shell = user_shell();
    let env = instance_env(adapter, ctx);
    let args = shell_invocation_args(terminal_cmd, &shell);
    Ok(TerminalLaunch {
        program: PathBuf::from(terminal_cmd),
        args,
        env,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_terminal_prefers_settings_over_env() {
        let found = find_terminal(Some("my-custom-term"));
        assert_eq!(found.as_deref(), Some("my-custom-term"));
    }

    #[test]
    fn find_terminal_ignores_blank_settings() {
        std::env::remove_var("TERMINAL");
        let found = find_terminal(Some("   "));
        // Falls through to $TERMINAL (unset) then PATH scan; either None or
        // a real emulator found on this machine's PATH is acceptable, but
        // it must not be the blank string.
        assert_ne!(found.as_deref(), Some("   "));
    }

    #[test]
    fn shell_invocation_args_uses_dashdash_for_gnome_terminal() {
        let args = shell_invocation_args("gnome-terminal", "/bin/bash");
        assert_eq!(args, vec!["--".to_string(), "/bin/bash".to_string()]);
    }

    #[test]
    fn shell_invocation_args_uses_dash_x_for_xfce4() {
        let args = shell_invocation_args("xfce4-terminal", "/bin/bash");
        assert_eq!(args, vec!["-x".to_string(), "/bin/bash".to_string()]);
    }

    #[test]
    fn connection_hint_only_for_redis() {
        assert!(connection_hint(EngineKind::Redis, 6379).is_some());
        assert!(connection_hint(EngineKind::Postgres, 5432).is_none());
        assert!(connection_hint(EngineKind::Mysql, 3306).is_none());
    }
}
