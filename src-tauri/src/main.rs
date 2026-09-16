#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod events;
mod tray;

use std::sync::Arc;

pub use dbnest_core::Manager as CoreManager;

use tauri::Manager as _;

/// Disimpan sebagai state supaya `on_window_event` tahu apakah tray
/// berhasil dibuat. Kalau tidak (mis. GNOME tanpa ekstensi AppIndicator,
/// atau tidak ada session bus sama sekali), menutup jendela harus benar-
/// benar keluar dari aplikasi — bukan sembunyi ke tray yang tidak ada,
/// yang membuat aplikasi jadi tidak bisa ditutup sama sekali (§13.3,
/// §21 "Tray tidak tampil di GNOME").
struct TrayAvailable(bool);

fn main() {
    tracing_subscriber::fmt::init();

    let manager = Arc::new(CoreManager::new().expect("gagal inisialisasi dbnest-core"));

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .manage(manager.clone())
        .invoke_handler(tauri::generate_handler![
            commands::list_engines,
            commands::list_instances,
            commands::create_instance,
            commands::update_instance,
            commands::delete_instance,
            commands::start_instance,
            commands::stop_instance,
            commands::restart_instance,
            commands::get_logs,
            commands::run_preflight,
            commands::open_terminal,
            commands::open_data_folder,
            commands::suggest_port,
            commands::list_installed_versions,
            commands::uninstall_version,
            commands::get_settings,
            commands::update_settings,
        ])
        .setup(move |app| {
            let tray_available = match tray::setup_tray(app, manager.clone()) {
                Ok(()) => true,
                Err(e) => {
                    tracing::warn!(
                        "tray tidak tersedia ({e}), aplikasi berjalan sebagai jendela biasa"
                    );
                    false
                }
            };
            app.manage(TrayAvailable(tray_available));
            events::spawn_status_poller(app.handle().clone(), manager.clone());
            Ok(())
        })
        .on_window_event(|window, event| {
            // Menutup jendela hanya menyembunyikannya ke tray (§13.3), dan
            // hanya kalau tray-nya benar-benar ada — kalau tidak, sembunyi
            // ke tray yang tidak ada berarti aplikasi tidak bisa ditutup
            // sama sekali. Quit sungguhan lewat menu tray "Quit", yang
            // tidak menghentikan server karena backend saat ini adalah
            // DirectBackend/systemd, bukan child process aplikasi.
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let tray_available = window.state::<TrayAvailable>().0;
                if tray_available {
                    let _ = window.hide();
                    api.prevent_close();
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error saat menjalankan aplikasi tauri");
}
