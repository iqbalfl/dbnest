//! Polling status ringan tiap 2 detik (DESIGN.md §13.2). Event hanya
//! dikirim ke frontend kalau status instance benar-benar berubah.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use dbnest_core::model::InstanceStatus;
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::{tray, CoreManager};

#[derive(Clone, Serialize)]
struct StatusPayload<'a> {
    id: &'a str,
    status: InstanceStatus,
}

pub fn spawn_status_poller(app: AppHandle, manager: Arc<CoreManager>) {
    tauri::async_runtime::spawn(async move {
        let mut last: HashMap<String, InstanceStatus> = HashMap::new();
        loop {
            let mut any_changed = false;
            if let Ok(instances) = manager.list_instances() {
                let mut seen = std::collections::HashSet::new();
                for instance in instances {
                    seen.insert(instance.id.clone());
                    if let Ok(status) = manager.status(&instance.id).await {
                        if last.get(&instance.id) != Some(&status) {
                            let _ = app.emit(
                                "instance://status",
                                StatusPayload {
                                    id: &instance.id,
                                    status: status.clone(),
                                },
                            );
                            last.insert(instance.id.clone(), status);
                            any_changed = true;
                        }
                    }
                }
                let before = last.len();
                last.retain(|id, _| seen.contains(id));
                if last.len() != before {
                    any_changed = true;
                }
            }
            if any_changed {
                tray::refresh_tray_menu(&app, &manager).await;
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    });
}
