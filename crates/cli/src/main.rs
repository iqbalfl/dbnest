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
        } => cmd_create(&manager, cli.json, &engine, &version, name, port, autostart),
        Commands::Start { id } => cmd_start(&manager, cli.json, &id).await,
        Commands::Stop { id } => cmd_stop(&manager, cli.json, &id).await,
        Commands::Restart { id } => cmd_restart(&manager, cli.json, &id).await,
        Commands::Status { id } => cmd_status(&manager, cli.json, &id).await,
        Commands::Logs { id, lines } => cmd_logs(&manager, cli.json, &id, lines),
        Commands::Info { id } => cmd_info(&manager, cli.json, &id),
        Commands::Delete { id, keep_data } => cmd_delete(&manager, cli.json, &id, keep_data).await,
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

fn cmd_create(
    manager: &Manager,
    json: bool,
    engine: &str,
    version: &str,
    name: Option<String>,
    port: Option<u16>,
    autostart: bool,
) -> dbnest_core::Result<()> {
    let engine = parse_engine(engine)?;
    let instance = manager.create_instance(CreateInstanceRequest {
        engine,
        version: version.to_string(),
        name,
        port,
        autostart,
    })?;

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
