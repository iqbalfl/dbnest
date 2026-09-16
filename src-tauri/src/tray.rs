//! Ikon tray dan menu per-instance (DESIGN.md §13.3).

use std::sync::Arc;

use dbnest_core::model::InstanceStatus;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, SubmenuBuilder};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager as _, Wry};

use crate::CoreManager;

const TRAY_ID: &str = "dbnest-tray";

pub fn setup_tray(app: &tauri::App, manager: Arc<CoreManager>) -> tauri::Result<()> {
    let handle = app.handle().clone();
    let menu = tauri::async_runtime::block_on(build_menu(&handle, &manager))?;

    let icon = app
        .default_window_icon()
        .cloned()
        .expect("ikon default aplikasi harus ada, cek src-tauri/icons/");

    TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon)
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(move |app, event| handle_menu_event(app, &manager, event.id().as_ref()))
        .build(app)?;

    Ok(())
}

/// Bangun ulang menu tray (dipanggil dari poller saat status berubah).
pub async fn refresh_tray_menu(app: &AppHandle, manager: &Arc<CoreManager>) {
    let Ok(menu) = build_menu(app, manager).await else {
        return;
    };
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let _ = tray.set_menu(Some(menu));
    }
}

async fn build_menu(app: &AppHandle, manager: &Arc<CoreManager>) -> tauri::Result<Menu<Wry>> {
    let menu = Menu::new(app)?;

    let instances = manager.list_instances().unwrap_or_default();
    for instance in &instances {
        let status = manager
            .status(&instance.id)
            .await
            .unwrap_or(InstanceStatus::Stopped);
        let dot = if matches!(status, InstanceStatus::Running { .. }) {
            "\u{25cf}"
        } else {
            "\u{25cb}"
        };
        let label = format!(
            "{dot} {} ({} {}:{})",
            instance.name, instance.engine, instance.version, instance.port
        );
        let submenu = SubmenuBuilder::new(app, label)
            .text(format!("start:{}", instance.id), "Start")
            .text(format!("stop:{}", instance.id), "Stop")
            .text(format!("terminal:{}", instance.id), "Open Terminal")
            .text(format!("copy:{}", instance.id), "Copy Connection URL")
            .build()?;
        menu.append(&submenu)?;
    }

    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&MenuItem::with_id(
        app,
        "show",
        "Show Window",
        true,
        None::<&str>,
    )?)?;
    menu.append(&MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?)?;

    Ok(menu)
}

fn handle_menu_event(app: &AppHandle, manager: &Arc<CoreManager>, id: &str) {
    match id {
        "show" => {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }
        "quit" => app.exit(0),
        other => dispatch_instance_action(app, manager, other),
    }
}

fn dispatch_instance_action(app: &AppHandle, manager: &Arc<CoreManager>, id: &str) {
    if let Some(instance_id) = id.strip_prefix("start:") {
        let manager = manager.clone();
        let instance_id = instance_id.to_string();
        tauri::async_runtime::spawn(async move {
            let _ = manager.start(&instance_id, |_| {}).await;
        });
    } else if let Some(instance_id) = id.strip_prefix("stop:") {
        let manager = manager.clone();
        let instance_id = instance_id.to_string();
        tauri::async_runtime::spawn(async move {
            let _ = manager.stop(&instance_id).await;
        });
    } else if let Some(instance_id) = id.strip_prefix("terminal:") {
        let _ = manager.open_terminal(instance_id);
    } else if let Some(instance_id) = id.strip_prefix("copy:") {
        if let (Ok(info), Some(window)) = (
            manager.connection_info(instance_id),
            app.get_webview_window("main"),
        ) {
            // Clipboard ditulis dari frontend biasanya; di tray kita cukup
            // munculkan jendela dan biarkan pengguna memakai tombol Copy di
            // detail instance (clipboard-write butuh plugin tersendiri).
            let _ = window.show();
            let _ = window.set_focus();
            let _ = info;
        }
    }
}
