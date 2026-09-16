//! Pemeriksaan sebelum start: root, binary/library, port, panjang path
//! socket, dan ruang disk (DESIGN.md §8.1).

use std::collections::HashMap;
use std::path::Path;

use serde::Serialize;

use crate::engines::{EngineAdapter, InstanceCtx};
use crate::install;
use crate::model::{Instance, Issue, IssueSeverity};
use crate::paths::Paths;
use crate::ports;

const MAX_SOCKET_PATH_BYTES: usize = 100;
const MIN_FREE_DISK_BYTES: u64 = 1024 * 1024 * 1024; // 1 GB

/// Ringkasan sistem yang ditampilkan `dbnest doctor`.
#[derive(Serialize, Clone, Debug)]
pub struct SystemInfo {
    pub arch: String,
    pub os_id: String,
    pub os_version: String,
    pub running_as_root: bool,
}

pub fn system_info() -> SystemInfo {
    let os_release = std::fs::read_to_string("/etc/os-release").unwrap_or_default();
    let fields = parse_os_release(&os_release);
    SystemInfo {
        arch: std::env::consts::ARCH.to_string(),
        os_id: fields
            .get("ID")
            .cloned()
            .unwrap_or_else(|| "unknown".to_string()),
        os_version: fields
            .get("VERSION_ID")
            .cloned()
            .unwrap_or_else(|| "unknown".to_string()),
        running_as_root: running_as_root(),
    }
}

/// `already_running` harus `true` kalau instance ini sendiri yang sedang
/// memegang portnya saat ini (mis. dipanggil dari `dbnest doctor` pada
/// instance yang sedang jalan), supaya port yang dipakai diri sendiri tidak
/// dilaporkan sebagai error.
pub fn check(
    paths: &Paths,
    instance: &Instance,
    ctx: &InstanceCtx,
    adapter: &dyn EngineAdapter,
    already_running: bool,
) -> Vec<Issue> {
    let mut issues = Vec::new();

    if running_as_root() {
        issues.push(Issue {
            severity: IssueSeverity::Error,
            message: "dbnest tidak boleh dijalankan sebagai root".to_string(),
            fix_hint: Some("Jalankan dbnest sebagai user biasa, bukan root/sudo.".to_string()),
        });
    }

    if !install::is_installed(paths, instance.engine, &instance.version) {
        issues.push(Issue {
            severity: IssueSeverity::Warning,
            message: format!(
                "binary {} {} belum terpasang",
                instance.engine, instance.version
            ),
            fix_hint: Some("jalankan `dbnest start` untuk memasang otomatis".to_string()),
        });
    } else if let Ok(spec) = adapter.launch_spec(ctx) {
        if let Some(issue) = check_libraries(&spec.program, &spec.env) {
            issues.push(issue);
        }
    }

    if !already_running && !ports::is_port_free(instance.port) {
        issues.push(Issue {
            severity: IssueSeverity::Error,
            message: format!("port {} sudah dipakai", instance.port),
            fix_hint: Some("pilih port lain atau hentikan proses yang memakainya".to_string()),
        });
    }

    let socket_len = ctx.run_dir.to_string_lossy().len();
    if socket_len > MAX_SOCKET_PATH_BYTES {
        issues.push(Issue {
            severity: IssueSeverity::Error,
            message: format!(
                "path socket terlalu panjang ({socket_len} byte, maksimal {MAX_SOCKET_PATH_BYTES})"
            ),
            fix_hint: Some(
                "pindahkan XDG_DATA_HOME/XDG_RUNTIME_DIR ke path yang lebih pendek".to_string(),
            ),
        });
    }

    if let Some(issue) = check_disk_space(paths) {
        issues.push(issue);
    }

    issues
}

fn running_as_root() -> bool {
    nix::unistd::Uid::effective().is_root()
}

fn check_libraries(program: &Path, env: &[(String, String)]) -> Option<Issue> {
    if !program.exists() {
        return None;
    }
    let mut cmd = std::process::Command::new("ldd");
    cmd.arg(program);
    for (key, value) in env {
        cmd.env(key, value);
    }
    let output = cmd.output().ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let missing = parse_missing_libraries(&stdout);
    if missing.is_empty() {
        return None;
    }

    let hints: Vec<String> = missing.iter().map(|lib| package_hint(lib)).collect();
    Some(Issue {
        severity: IssueSeverity::Error,
        message: format!("library hilang: {missing:?}"),
        fix_hint: Some(format!("pasang paket sistem berikut: {}", hints.join("; "))),
    })
}

fn parse_missing_libraries(ldd_output: &str) -> Vec<String> {
    ldd_output
        .lines()
        .filter(|line| line.contains("=> not found"))
        .filter_map(|line| line.split_whitespace().next())
        .map(str::to_string)
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DistroFamily {
    Debian,
    Fedora,
    Arch,
    Unknown,
}

fn detect_distro_family() -> DistroFamily {
    let Ok(contents) = std::fs::read_to_string("/etc/os-release") else {
        return DistroFamily::Unknown;
    };
    detect_distro_family_from(&contents)
}

fn detect_distro_family_from(os_release: &str) -> DistroFamily {
    let fields = parse_os_release(os_release);
    let id = fields.get("ID").map(String::as_str).unwrap_or("");
    let id_like = fields.get("ID_LIKE").map(String::as_str).unwrap_or("");
    let haystack = format!("{id} {id_like}");

    if haystack.contains("debian") || haystack.contains("ubuntu") {
        DistroFamily::Debian
    } else if haystack.contains("fedora") || haystack.contains("rhel") {
        DistroFamily::Fedora
    } else if haystack.contains("arch") {
        DistroFamily::Arch
    } else {
        DistroFamily::Unknown
    }
}

fn parse_os_release(contents: &str) -> HashMap<String, String> {
    contents
        .lines()
        .filter_map(|line| line.split_once('='))
        .map(|(k, v)| (k.trim().to_string(), v.trim().trim_matches('"').to_string()))
        .collect()
}

/// Pemetaan library yang hilang ke nama paket sistem (§8.2).
fn package_hint(lib_name: &str) -> String {
    let family = detect_distro_family();
    let package = match (lib_name, family) {
        ("libaio.so.1", DistroFamily::Debian) => {
            "libaio1 (Ubuntu <=23.10) atau libaio1t64 (Ubuntu >=24.04)"
        }
        ("libaio.so.1", DistroFamily::Fedora) => "libaio",
        ("libaio.so.1", DistroFamily::Arch) => "libaio",
        ("libncurses.so.6" | "libtinfo.so.6", DistroFamily::Debian) => "libncurses6",
        ("libncurses.so.6" | "libtinfo.so.6", DistroFamily::Fedora) => "ncurses-libs",
        ("libncurses.so.6" | "libtinfo.so.6", DistroFamily::Arch) => "ncurses",
        ("libnuma.so.1", DistroFamily::Debian) => "libnuma1",
        ("libnuma.so.1", DistroFamily::Fedora) => "numactl-libs",
        ("libnuma.so.1", DistroFamily::Arch) => "numactl",
        ("libssl.so.3", DistroFamily::Debian) => "libssl3 (atau libssl3t64)",
        ("libssl.so.3", DistroFamily::Fedora) => "openssl-libs",
        ("libssl.so.3", DistroFamily::Arch) => "openssl",
        (_, _) => "(paket tidak diketahui, cari manual untuk library ini)",
    };
    format!("{lib_name} -> {package}")
}

fn check_disk_space(paths: &Paths) -> Option<Issue> {
    let available = fs4::available_space(paths.data_dir()).ok()?;
    if available < MIN_FREE_DISK_BYTES {
        Some(Issue {
            severity: IssueSeverity::Warning,
            message: format!(
                "ruang disk tersisa di {} kurang dari 1 GB ({} byte)",
                paths.data_dir().display(),
                available
            ),
            fix_hint: Some("kosongkan ruang disk sebelum memasang binary baru".to_string()),
        })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_missing_libraries_extracts_names() {
        let output = "\tlibfoo.so.1 => not found\n\tlibc.so.6 => /lib/libc.so.6 (0x0)\n";
        let missing = parse_missing_libraries(output);
        assert_eq!(missing, vec!["libfoo.so.1".to_string()]);
    }

    #[test]
    fn parse_missing_libraries_empty_when_all_resolved() {
        let output = "\tlibc.so.6 => /lib/libc.so.6 (0x0)\n";
        assert!(parse_missing_libraries(output).is_empty());
    }

    #[test]
    fn detects_debian_family() {
        let os_release = "ID=ubuntu\nID_LIKE=debian\nVERSION_ID=\"24.04\"\n";
        assert_eq!(detect_distro_family_from(os_release), DistroFamily::Debian);
    }

    #[test]
    fn detects_fedora_family() {
        let os_release = "ID=fedora\n";
        assert_eq!(detect_distro_family_from(os_release), DistroFamily::Fedora);
    }

    #[test]
    fn detects_arch_family() {
        let os_release = "ID=arch\n";
        assert_eq!(detect_distro_family_from(os_release), DistroFamily::Arch);
    }

    #[test]
    fn unknown_distro_falls_back() {
        let os_release = "ID=solaris\n";
        assert_eq!(detect_distro_family_from(os_release), DistroFamily::Unknown);
    }

    #[test]
    fn package_hint_mentions_library_name() {
        let hint = package_hint("libnuma.so.1");
        assert!(hint.starts_with("libnuma.so.1 ->"));
    }
}
