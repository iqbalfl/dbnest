use std::process::ExitCode;
use std::str::FromStr;

use clap::{Parser, Subcommand};
use dbnest_core::manager::{CreateInstanceRequest, Manager};
use dbnest_core::model::{EngineKind, InstallEvent, Instance, InstanceStatus, ProgressEvent};
use dbnest_core::Error;
use serde::Serialize;

#[derive(Parser)]
#[command(
    name = "dbnest",
    version,
    about = "DBnest — server database lokal untuk Linux, tanpa Docker dan tanpa root"
)]
struct Cli {
    /// Keluarkan hasil sebagai JSON, untuk keperluan skrip.
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Daftar engine dan versi yang tersedia di manifest.
    Engines,
    /// Daftar instance beserta status dan port.
    List,
    /// Buat instance baru.
    Create {
        engine: String,
        version: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        port: Option<u16>,
        #[arg(long)]
        autostart: bool,
    },
    /// Jalankan instance (install + init otomatis bila perlu).
    Start { id: String },
    /// Hentikan instance.
    Stop { id: String },
    /// Jalankan ulang instance (stop lalu start).
    Restart { id: String },
    /// Tampilkan status instance.
    Status { id: String },
    /// Tampilkan log instance.
    Logs {
        id: String,
        #[arg(short = 'n', long, default_value_t = 100)]
        lines: usize,
    },
    /// Tampilkan info koneksi instance.
    Info { id: String },
    /// Hapus instance.
    Delete {
        id: String,
        #[arg(long)]
        keep_data: bool,
    },
    /// Buka $SHELL dengan PATH/env yang mengarah ke engine instance ini.
    Shell { id: String },
    /// Cetak baris `export ...` untuk di-eval di shell saat ini.
    Env { id: String },
    /// Daftar versi di manifest, atau hanya yang sudah terpasang.
    Versions {
        #[arg(long)]
        installed: bool,
        /// Unduh ulang manifest dari `manifest_url` sebelum menampilkan.
        #[arg(long)]
        refresh: bool,
    },
    /// Hapus versi engine yang sudah terpasang.
    Uninstall { engine: String, version: String },
    /// Jalankan preflight untuk semua instance + info sistem.
    Doctor,
}

#[derive(Serialize)]
struct JsonError {
    code: String,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    hint: Option<String>,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("gagal membuat runtime async: {e}");
            return ExitCode::from(1);
        }
    };

    match runtime.block_on(run(cli)) {
        Ok(()) => ExitCode::from(0),
        Err(e) => {
            let json = std::env::args().any(|a| a == "--json");
            if json {
                let payload = JsonError {
                    code: e.code().to_string(),
                    message: e.to_string(),
                    hint: e.hint(),
                };
                println!(
                    "{}",
                    serde_json::to_string_pretty(&payload).unwrap_or_default()
                );
            } else {
                eprintln!("error: {e}");
                if let Some(hint) = e.hint() {
                    eprintln!("hint: {hint}");
                }
            }
            let code = match e {
                Error::PreflightFailed(_) => 3,
                _ => 1,
            };
            ExitCode::from(code)
        }
    }
}

async fn run(cli: Cli) -> dbnest_core::Result<()> {
    let manager = Manager::new()?;
    match cli.command {
        Commands::Engines => cmd_engines(&manager, cli.json),
        Commands::List => cmd_list(&manager, cli.json).await,
        Commands::Create {
            engine,
            version,
            name,
            port,
            autostart,
        } => cmd_create(&manager, cli.json, &engine, &version, name, port, autostart).await,
        Commands::Start { id } => cmd_start(&manager, cli.json, &id).await,
        Commands::Stop { id } => cmd_stop(&manager, cli.json, &id).await,
        Commands::Restart { id } => cmd_restart(&manager, cli.json, &id).await,
        Commands::Status { id } => cmd_status(&manager, cli.json, &id).await,
        Commands::Logs { id, lines } => cmd_logs(&manager, cli.json, &id, lines),
        Commands::Info { id } => cmd_info(&manager, cli.json, &id),
        Commands::Delete { id, keep_data } => cmd_delete(&manager, cli.json, &id, keep_data).await,
        Commands::Shell { id } => cmd_shell(&manager, &id),
        Commands::Env { id } => cmd_env(&manager, cli.json, &id),
        Commands::Versions { installed, refresh } => {
            cmd_versions(&manager, cli.json, installed, refresh).await
        }
        Commands::Uninstall { engine, version } => {
            cmd_uninstall(&manager, cli.json, &engine, &version)
        }
        Commands::Doctor => cmd_doctor(&manager, cli.json).await,
    }
}

fn parse_engine(s: &str) -> dbnest_core::Result<EngineKind> {
    EngineKind::from_str(s).map_err(Error::Other)
}

fn cmd_engines(manager: &Manager, json: bool) -> dbnest_core::Result<()> {
    let manifest = manager.manifest()?;
    if json {
        println!("{}", serde_json::to_string_pretty(&manifest)?);
        return Ok(());
    }
    let mut engines: Vec<_> = manifest.engines.iter().collect();
    engines.sort_by_key(|(k, _)| (*k).clone());
    for (key, catalog) in engines {
        println!("{} ({})", catalog.display_name, key);
        for v in &catalog.versions {
            let mark = if v.verified {
                ""
            } else {
                " [belum diverifikasi]"
            };
            println!("  - {}{}", v.version, mark);
        }
    }
    Ok(())
}

#[derive(Serialize)]
struct InstanceRow {
    #[serde(flatten)]
    instance: Instance,
    status: InstanceStatus,
}

async fn cmd_list(manager: &Manager, json: bool) -> dbnest_core::Result<()> {
    let instances = manager.list_instances()?;
    let mut rows = Vec::with_capacity(instances.len());
    for instance in instances {
        let status = manager.status(&instance.id).await?;
        rows.push(InstanceRow { instance, status });
    }

    if json {
        println!("{}", serde_json::to_string_pretty(&rows)?);
        return Ok(());
    }

    if rows.is_empty() {
        println!("Belum ada instance. Buat satu dengan `dbnest create <engine> <version>`.");
        return Ok(());
    }

    println!(
        "{:<12} {:<20} {:<10} {:<8} {:<6} STATUS",
        "ID", "NAME", "ENGINE", "VERSION", "PORT"
    );
    for row in rows {
        println!(
            "{:<12} {:<20} {:<10} {:<8} {:<6} {}",
            row.instance.id,
            row.instance.name,
            row.instance.engine.as_str(),
            row.instance.version,
            row.instance.port,
            status_label(&row.status),
        );
    }
    Ok(())
}

fn status_label(status: &InstanceStatus) -> String {
    match status {
        InstanceStatus::NotInitialized => "not_initialized".to_string(),
        InstanceStatus::Stopped => "stopped".to_string(),
        InstanceStatus::Starting => "starting".to_string(),
        InstanceStatus::Running { pid } => match pid {
            Some(pid) => format!("running (pid {pid})"),
            None => "running".to_string(),
        },
        InstanceStatus::Stopping => "stopping".to_string(),
        InstanceStatus::Failed { message } => format!("failed: {message}"),
    }
}

async fn cmd_create(
    manager: &Manager,
    json: bool,
    engine: &str,
    version: &str,
    name: Option<String>,
    port: Option<u16>,
    autostart: bool,
) -> dbnest_core::Result<()> {
    let engine = parse_engine(engine)?;
    let instance = manager
        .create_instance(CreateInstanceRequest {
            engine,
            version: version.to_string(),
            name,
            port,
            autostart,
        })
        .await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&instance)?);
    } else {
        println!(
            "Instance dibuat: {} ({} {}) di port {}",
            instance.id, instance.engine, instance.version, instance.port
        );
    }
    Ok(())
}

fn print_progress_event(event: &ProgressEvent) {
    match event {
        ProgressEvent::Install(InstallEvent::Downloading { downloaded, .. })
            if *downloaded == 0 =>
        {
            println!("mengunduh binary...");
        }
        ProgressEvent::Install(InstallEvent::Downloading { .. }) => {}
        ProgressEvent::Install(InstallEvent::Verifying) => println!("memverifikasi checksum..."),
        ProgressEvent::Install(InstallEvent::Extracting) => println!("mengekstrak binary..."),
        ProgressEvent::Install(InstallEvent::Done) => println!("instalasi selesai"),
        ProgressEvent::Install(InstallEvent::Failed { message }) => {
            eprintln!("instalasi gagal: {message}")
        }
        ProgressEvent::Preflight => println!("menjalankan preflight..."),
        ProgressEvent::Initializing => println!("menginisialisasi data directory..."),
        ProgressEvent::Starting => println!("menjalankan proses..."),
        ProgressEvent::HealthCheck => println!("menunggu server siap..."),
        ProgressEvent::Ready => println!("server siap"),
        ProgressEvent::Failed { message } => eprintln!("gagal: {message}"),
    }
}

async fn cmd_start(manager: &Manager, json: bool, id: &str) -> dbnest_core::Result<()> {
    manager
        .start(id, |event| {
            if !json {
                print_progress_event(&event);
            }
        })
        .await?;

    if json {
        let status = manager.status(id).await?;
        println!("{}", serde_json::to_string_pretty(&status)?);
    } else {
        println!("{id}: berjalan");
    }
    Ok(())
}

async fn cmd_stop(manager: &Manager, json: bool, id: &str) -> dbnest_core::Result<()> {
    manager.stop(id).await?;
    if json {
        let status = manager.status(id).await?;
        println!("{}", serde_json::to_string_pretty(&status)?);
    } else {
        println!("{id}: dihentikan");
    }
    Ok(())
}

async fn cmd_restart(manager: &Manager, json: bool, id: &str) -> dbnest_core::Result<()> {
    manager.stop(id).await?;
    manager
        .start(id, |event| {
            if !json {
                print_progress_event(&event);
            }
        })
        .await?;
    if json {
        let status = manager.status(id).await?;
        println!("{}", serde_json::to_string_pretty(&status)?);
    } else {
        println!("{id}: dijalankan ulang");
    }
    Ok(())
}

async fn cmd_status(manager: &Manager, json: bool, id: &str) -> dbnest_core::Result<()> {
    let status = manager.status(id).await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&status)?);
    } else {
        println!("{id}: {}", status_label(&status));
    }
    Ok(())
}

fn cmd_logs(manager: &Manager, json: bool, id: &str, lines: usize) -> dbnest_core::Result<()> {
    let logs = manager.tail_logs(id, lines)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&logs)?);
    } else {
        for line in logs {
            println!("{line}");
        }
    }
    Ok(())
}

fn cmd_info(manager: &Manager, json: bool, id: &str) -> dbnest_core::Result<()> {
    let instance = manager.find_instance(id)?;
    let connection = manager.connection_info(id)?;
    if json {
        #[derive(Serialize)]
        struct Info {
            instance: Instance,
            connection: dbnest_core::model::ConnectionInfo,
        }
        println!(
            "{}",
            serde_json::to_string_pretty(&Info {
                instance,
                connection
            })?
        );
    } else {
        println!("id       : {}", instance.id);
        println!("name     : {}", instance.name);
        println!("engine   : {} {}", instance.engine, instance.version);
        println!("host     : {}", connection.host);
        println!("port     : {}", connection.port);
        if let Some(user) = &connection.username {
            println!("username : {user}");
        }
        println!("url      : {}", connection.url);
    }
    Ok(())
}

async fn cmd_delete(
    manager: &Manager,
    json: bool,
    id: &str,
    keep_data: bool,
) -> dbnest_core::Result<()> {
    manager.delete_instance(id, !keep_data).await?;
    if !json {
        println!("{id}: dihapus");
    }
    Ok(())
}

async fn cmd_doctor(manager: &Manager, json: bool) -> dbnest_core::Result<()> {
    let system = dbnest_core::preflight::system_info();
    let instances = manager.list_instances()?;

    let mut report = Vec::with_capacity(instances.len());
    for instance in &instances {
        let issues = manager.preflight(&instance.id).await?;
        report.push((instance.clone(), issues));
    }

    if json {
        #[derive(Serialize)]
        struct DoctorReport {
            system: dbnest_core::preflight::SystemInfo,
            process_backend: &'static str,
            instances: Vec<InstanceIssues>,
        }
        #[derive(Serialize)]
        struct InstanceIssues {
            instance: Instance,
            issues: Vec<dbnest_core::model::Issue>,
        }
        let payload = DoctorReport {
            system,
            process_backend: manager.backend_label(),
            instances: report
                .into_iter()
                .map(|(instance, issues)| InstanceIssues { instance, issues })
                .collect(),
        };
        println!("{}", serde_json::to_string_pretty(&payload)?);
        return Ok(());
    }

    println!("sistem   : {} ({})", system.os_id, system.os_version);
    println!("arsitektur: {}", system.arch);
    println!("backend  : {}", manager.backend_label());
    println!(
        "root?    : {}",
        if system.running_as_root {
            "YA (masalah!)"
        } else {
            "tidak"
        }
    );
    println!();

    if report.is_empty() {
        println!("Belum ada instance untuk diperiksa.");
        return Ok(());
    }

    for (instance, issues) in report {
        println!(
            "{} ({} {}, port {})",
            instance.id, instance.engine, instance.version, instance.port
        );
        if issues.is_empty() {
            println!("  OK, tidak ada masalah");
        }
        for issue in issues {
            let tag = match issue.severity {
                dbnest_core::model::IssueSeverity::Error => "ERROR",
                dbnest_core::model::IssueSeverity::Warning => "WARN ",
                dbnest_core::model::IssueSeverity::Info => "INFO ",
            };
            println!("  [{tag}] {}", issue.message);
            if let Some(hint) = &issue.fix_hint {
                println!("         hint: {hint}");
            }
        }
    }
    Ok(())
}

/// Bungkus nilai untuk baris `export` yang aman di-eval shell: kutip
/// tunggal, dengan kutip tunggal di dalamnya dipecah jadi `'\''`.
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

fn cmd_env(manager: &Manager, json: bool, id: &str) -> dbnest_core::Result<()> {
    let env = manager.instance_env(id)?;
    if json {
        let map: std::collections::BTreeMap<_, _> = env.into_iter().collect();
        println!("{}", serde_json::to_string_pretty(&map)?);
        return Ok(());
    }
    for (key, value) in env {
        println!("export {key}={}", shell_quote(&value));
    }
    if let Some(hint) = manager.connection_hint(id)? {
        println!("# petunjuk koneksi: {hint}");
    }
    Ok(())
}

fn cmd_shell(manager: &Manager, id: &str) -> dbnest_core::Result<()> {
    use std::os::unix::process::CommandExt as _;

    let env = manager.instance_env(id)?;
    let shell = manager.user_shell();
    if let Some(hint) = manager.connection_hint(id)? {
        println!("dbnest: {hint}");
    }

    // `exec` menggantikan proses ini dengan shell-nya, jadi tidak ada
    // lapisan proses dbnest yang menganggur selama sesi berlangsung.
    // Baris berikutnya hanya tercapai kalau exec gagal.
    let err = std::process::Command::new(&shell).envs(env).exec();
    Err(dbnest_core::Error::Process(format!(
        "gagal menjalankan shell {shell}: {err}"
    )))
}

#[derive(Serialize)]
struct VersionRow {
    engine: String,
    version: String,
    installed: bool,
    verified: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    size_bytes: Option<u64>,
}

async fn cmd_versions(
    manager: &Manager,
    json: bool,
    installed_only: bool,
    refresh: bool,
) -> dbnest_core::Result<()> {
    if refresh {
        manager.refresh_manifest().await?;
    }
    let installed = manager.installed_versions()?;
    let is_installed = |engine: &str, version: &str| {
        installed
            .iter()
            .any(|i| i.engine.as_str() == engine && i.version == version)
    };
    let size_of = |engine: &str, version: &str| {
        installed
            .iter()
            .find(|i| i.engine.as_str() == engine && i.version == version)
            .map(|i| i.size_bytes)
    };

    let manifest = manager.manifest()?;
    let mut rows: Vec<VersionRow> = Vec::new();
    let mut engines: Vec<_> = manifest.engines.iter().collect();
    engines.sort_by_key(|(k, _)| (*k).clone());
    for (engine, catalog) in engines {
        for entry in &catalog.versions {
            let installed_here = is_installed(engine, &entry.version);
            if installed_only && !installed_here {
                continue;
            }
            rows.push(VersionRow {
                engine: engine.clone(),
                version: entry.version.clone(),
                installed: installed_here,
                verified: entry.verified,
                size_bytes: size_of(engine, &entry.version),
            });
        }
    }

    // Versi terpasang yang sudah tidak ada di manifest tetap perlu terlihat,
    // supaya bisa di-uninstall.
    for item in &installed {
        let known = rows
            .iter()
            .any(|r| r.engine == item.engine.as_str() && r.version == item.version);
        if !known {
            rows.push(VersionRow {
                engine: item.engine.as_str().to_string(),
                version: item.version.clone(),
                installed: true,
                verified: false,
                size_bytes: Some(item.size_bytes),
            });
        }
    }

    if json {
        println!("{}", serde_json::to_string_pretty(&rows)?);
        return Ok(());
    }

    if rows.is_empty() {
        println!("Tidak ada versi untuk ditampilkan.");
        return Ok(());
    }

    println!(
        "{:<10} {:<10} {:<10} CATATAN",
        "ENGINE", "VERSION", "INSTALLED"
    );
    for row in rows {
        let note = match (row.installed, row.verified) {
            (true, _) => row
                .size_bytes
                .map(format_bytes)
                .unwrap_or_else(|| "terpasang".to_string()),
            (false, false) => "belum diverifikasi di manifest".to_string(),
            (false, true) => String::new(),
        };
        println!(
            "{:<10} {:<10} {:<10} {}",
            row.engine,
            row.version,
            if row.installed { "ya" } else { "tidak" },
            note
        );
    }
    Ok(())
}

fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["KB", "MB", "GB", "TB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64 / 1024.0;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}

fn cmd_uninstall(
    manager: &Manager,
    json: bool,
    engine: &str,
    version: &str,
) -> dbnest_core::Result<()> {
    let engine = parse_engine(engine)?;
    manager.uninstall_version(engine, version)?;
    if !json {
        println!("{engine} {version}: dihapus");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_quote_wraps_plain_value() {
        assert_eq!(shell_quote("/usr/bin"), "'/usr/bin'");
    }

    #[test]
    fn shell_quote_survives_spaces() {
        assert_eq!(shell_quote("/home/a b/bin"), "'/home/a b/bin'");
    }

    /// Path dengan kutip tunggal harus tetap aman di-`eval`: potong kutip,
    /// sisipkan kutip yang di-escape, lalu buka kutip lagi.
    #[test]
    fn shell_quote_escapes_single_quote() {
        assert_eq!(shell_quote("it's"), r"'it'\''s'");
    }

    #[test]
    fn shell_quote_does_not_expand_variables() {
        assert_eq!(shell_quote("$HOME/`whoami`"), "'$HOME/`whoami`'");
    }

    #[test]
    fn format_bytes_scales_units() {
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(2048), "2.0 KB");
        assert_eq!(format_bytes(5 * 1024 * 1024), "5.0 MB");
    }
}
