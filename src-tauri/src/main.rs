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

/// `true` kalau backend proses saat ini `DirectBackend` — dipakai untuk
/// memutuskan apakah Quit perlu menawarkan "hentikan semua server?"
/// (§13.3): hanya relevan untuk DirectBackend, karena server yang dikelola
/// systemd tetap hidup lepas dari proses aplikasi ini.
pub struct BackendIsDirect(pub bool);

fn main() {
    tracing_subscriber::fmt::init();

    let manager = Arc::new(CoreManager::new().expect("gagal inisialisasi dbnest-core"));
    let backend_is_direct = manager.is_direct_backend();

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
            commands::active_backend,
            commands::quit_app,
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
            app.manage(BackendIsDirect(backend_is_direct));
            events::spawn_status_poller(app.handle().clone(), manager.clone());

            // Jalankan instance autostart saat aplikasi/tray dibuka (§9.2).
            // Untuk SystemdUserBackend ini sebagian besar no-op (systemd
            // sendiri sudah menjalankannya saat login lewat unit yang
            // di-enable), tapi tetap aman dipanggil karena start() idempoten.
            let manager_for_autostart = manager.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = manager_for_autostart.autostart_all().await {
                    tracing::warn!("autostart_all gagal: {e}");
                }
            });

            Ok(())
        })
        .on_window_event(|window, event| {
            // Menutup jendela hanya menyembunyikannya ke tray (§13.3), dan
            // hanya kalau tray-nya benar-benar ada — kalau tidak, sembunyi
            // ke tray yang tidak ada berarti aplikasi tidak bisa ditutup
            // sama sekali, jadi lewatkan ke alur quit yang sama dengan
            // tray "Quit".
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let tray_available = window.state::<TrayAvailable>().0;
                if tray_available {
                    let _ = window.hide();
                    api.prevent_close();
                } else {
                    api.prevent_close();
                    request_quit(window.app_handle());
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error saat menjalankan aplikasi tauri");
}

/// Titik keluar tunggal untuk Quit (tray atau, kalau tray tidak ada, tombol
/// tutup jendela). Backend systemd tidak menghentikan server lepas dari
/// proses aplikasi, jadi langsung keluar; DirectBackend tidak diawasi
/// siapa pun kalau aplikasi ditutup, jadi tanya dulu lewat frontend
/// (`app://confirm-quit` → `window.confirm()` → command `quit_app`).
pub(crate) fn request_quit(app: &tauri::AppHandle) {
    use tauri::{Emitter, Manager as _};
    let backend_is_direct = app.state::<BackendIsDirect>().0;
    if backend_is_direct {
        let _ = app.emit("app://confirm-quit", ());
    } else {
        app.exit(0);
    }
}
