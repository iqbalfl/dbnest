//! Pembungkus tipis `#[tauri::command]` di atas `dbnest_core::Manager`
//! (DESIGN.md §13.1). Semua logika bisnis tetap di `dbnest-core`.

use std::sync::Arc;

use dbnest_core::config::SettingsFile;
use dbnest_core::manager::{CreateInstanceRequest, UpdateInstance};
use dbnest_core::manifest::Manifest;
use dbnest_core::model::{
    EngineKind, InstalledVersion, Instance, InstanceView, Issue, ProgressEvent,
};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use crate::CoreManager;

#[derive(Serialize)]
pub struct CommandError {
    code: String,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    hint: Option<String>,
}

impl From<dbnest_core::Error> for CommandError {
    fn from(e: dbnest_core::Error) -> Self {
        Self {
            code: e.code().to_string(),
            message: e.to_string(),
            hint: e.hint(),
        }
    }
}

type CmdResult<T> = Result<T, CommandError>;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateInstanceArgs {
    pub engine: EngineKind,
    pub version: String,
    pub name: Option<String>,
    pub port: Option<u16>,
    pub autostart: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInstanceArgs {
    pub name: Option<String>,
    pub port: Option<u16>,
    pub autostart: Option<bool>,
}

#[derive(Clone, Serialize)]
struct ProgressPayload<'a> {
    id: &'a str,
    event: ProgressEvent,
}

#[tauri::command]
pub fn list_engines(manager: State<'_, Arc<CoreManager>>) -> CmdResult<Manifest> {
    Ok(manager.manifest()?)
}

#[tauri::command]
pub async fn list_instances(manager: State<'_, Arc<CoreManager>>) -> CmdResult<Vec<InstanceView>> {
    let instances = manager.list_instances()?;
    let mut views = Vec::with_capacity(instances.len());
    for instance in instances {
        let status = manager.status(&instance.id).await?;
        let connection = manager.connection_info(&instance.id)?;
        views.push(InstanceView {
            instance,
            status,
            connection,
        });
    }
    Ok(views)
}

#[tauri::command]
pub fn create_instance(
    manager: State<'_, Arc<CoreManager>>,
    payload: CreateInstanceArgs,
) -> CmdResult<Instance> {
    let instance = manager.create_instance(CreateInstanceRequest {
        engine: payload.engine,
        version: payload.version,
        name: payload.name,
        port: payload.port,
        autostart: payload.autostart,
    })?;
    Ok(instance)
}

#[tauri::command]
pub async fn update_instance(
    manager: State<'_, Arc<CoreManager>>,
    id: String,
    payload: UpdateInstanceArgs,
) -> CmdResult<Instance> {
    let instance = manager
        .update_instance(
            &id,
            UpdateInstance {
                name: payload.name,
                port: payload.port,
                autostart: payload.autostart,
            },
        )
        .await?;
    Ok(instance)
}

#[tauri::command]
pub async fn delete_instance(
    manager: State<'_, Arc<CoreManager>>,
    app: AppHandle,
    id: String,
    delete_data: bool,
) -> CmdResult<()> {
    manager.delete_instance(&id, delete_data).await?;
    let _ = app.emit("instances://changed", ());
    Ok(())
}

#[tauri::command]
pub async fn start_instance(
    manager: State<'_, Arc<CoreManager>>,
    app: AppHandle,
    id: String,
) -> CmdResult<()> {
    let result = manager
        .start(&id, |event| {
            let _ = app.emit("instance://progress", ProgressPayload { id: &id, event });
        })
        .await;
    let _ = app.emit("instances://changed", ());
    result.map_err(CommandError::from)
}

#[tauri::command]
pub async fn stop_instance(
    manager: State<'_, Arc<CoreManager>>,
    app: AppHandle,
    id: String,
) -> CmdResult<()> {
    manager.stop(&id).await?;
    let _ = app.emit("instances://changed", ());
    Ok(())
}

#[tauri::command]
pub async fn restart_instance(
    manager: State<'_, Arc<CoreManager>>,
    app: AppHandle,
    id: String,
) -> CmdResult<()> {
    manager.stop(&id).await?;
    let result = manager
        .start(&id, |event| {
            let _ = app.emit("instance://progress", ProgressPayload { id: &id, event });
        })
        .await;
    let _ = app.emit("instances://changed", ());
    result.map_err(CommandError::from)
}

#[tauri::command]
pub fn get_logs(
    manager: State<'_, Arc<CoreManager>>,
    id: String,
    lines: usize,
) -> CmdResult<Vec<String>> {
    Ok(manager.tail_logs(&id, lines)?)
}

#[tauri::command]
pub async fn run_preflight(
    manager: State<'_, Arc<CoreManager>>,
    id: String,
) -> CmdResult<Vec<Issue>> {
    Ok(manager.preflight(&id).await?)
}

#[tauri::command]
pub fn open_terminal(
    manager: State<'_, Arc<CoreManager>>,
    id: String,
) -> CmdResult<Option<String>> {
    Ok(manager.open_terminal(&id)?)
}

#[tauri::command]
pub fn open_data_folder(manager: State<'_, Arc<CoreManager>>, id: String) -> CmdResult<()> {
    manager.open_data_folder(&id)?;
    Ok(())
}

#[tauri::command]
pub fn suggest_port(manager: State<'_, Arc<CoreManager>>, engine: EngineKind) -> CmdResult<u16> {
    Ok(manager.suggest_port(engine)?)
}

#[tauri::command]
pub fn list_installed_versions(
    manager: State<'_, Arc<CoreManager>>,
) -> CmdResult<Vec<InstalledVersion>> {
    Ok(manager.installed_versions()?)
}

#[tauri::command]
pub fn uninstall_version(
    manager: State<'_, Arc<CoreManager>>,
    engine: EngineKind,
    version: String,
) -> CmdResult<()> {
    manager.uninstall_version(engine, &version)?;
    Ok(())
}

#[tauri::command]
pub fn get_settings(manager: State<'_, Arc<CoreManager>>) -> CmdResult<SettingsFile> {
    Ok(manager.get_settings()?)
}

#[tauri::command]
pub fn update_settings(
    manager: State<'_, Arc<CoreManager>>,
    settings: SettingsFile,
) -> CmdResult<SettingsFile> {
    Ok(manager.update_settings(settings)?)
}
