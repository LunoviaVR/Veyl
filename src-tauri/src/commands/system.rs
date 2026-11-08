use std::sync::atomic::Ordering;

use serde::Serialize;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_opener::OpenerExt;
use veyl_core::specs::{parse_smbios, sections, SpecSection, SpecsInput};
use veyl_core::{SystemInfo, TelemetryFrame};
use veyl_storage::DbStats;
use veyl_windows::{cpu, display, gpu, system_info};

use super::CmdResult;
use crate::state::{AppState, ChannelStatus};

/// The spec sheet shown on the PC Specs page and written by its exports.
pub(crate) fn pc_specs(state: &AppState) -> Vec<SpecSection> {
    let system = state.system_info.read().clone();
    let (drives, adapters) = {
        let live = state.live.read();
        let adapters = live
            .frame
            .as_ref()
            .map(|f| f.network.adapters.clone())
            .unwrap_or_default();
        (live.drives.clone(), adapters)
    };
    let smbios = system_info::smbios_table()
        .map(|t| parse_smbios(&t))
        .unwrap_or_default();
    sections(&SpecsInput {
        system: &system,
        smbios: &smbios,
        caches: cpu::cache_sizes(),
        gpu_drivers: &gpu::driver_versions(),
        displays: &display::displays(),
        drives: &drives,
        adapters: &adapters,
    })
}

#[tauri::command(async)]
pub fn get_pc_specs(state: State<'_, AppState>) -> Vec<SpecSection> {
    pc_specs(&state)
}

#[tauri::command]
pub fn get_system_info(state: State<'_, AppState>) -> SystemInfo {
    state.system_info.read().clone()
}

#[tauri::command]
pub fn get_latest(state: State<'_, AppState>) -> Option<TelemetryFrame> {
    state.live.read().frame.as_deref().cloned()
}

/// The UI reports whether it is visible so live events are only emitted when someone is looking.
#[tauri::command]
pub fn set_ui_visible(state: State<'_, AppState>, visible: bool) {
    state.ui_visible.store(visible, Ordering::Relaxed);
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeStatus {
    pub version: String,
    pub elevated: bool,
    pub data_dir: String,
    pub log_dir: String,
    pub tools_dir: String,
    pub db_error: Option<String>,
    pub db_stats: Option<DbStats>,
    pub dropped_write_batches: u64,
    pub event_channels: Vec<ChannelStatus>,
    pub started_ms: i64,
}

#[tauri::command(async)]
pub fn get_runtime_status(app: AppHandle, state: State<'_, AppState>) -> RuntimeStatus {
    RuntimeStatus {
        version: app.package_info().version.to_string(),
        elevated: state.system_info.read().elevated,
        data_dir: state.paths.data.display().to_string(),
        log_dir: state.paths.logs.display().to_string(),
        tools_dir: state.paths.tools.display().to_string(),
        db_error: state.db_error.read().clone(),
        db_stats: state.with_db(|db| db.stats(Some(&state.paths.db))).ok(),
        dropped_write_batches: crate::writer::DROPPED_BATCHES.load(Ordering::Relaxed),
        event_channels: state.event_channels.read().clone(),
        started_ms: state.started_ms,
    }
}

/// Restarts the application elevated after an explicit user request (UAC prompt).
#[tauri::command(async)]
pub fn restart_as_admin(app: AppHandle) -> CmdResult<()> {
    veyl_windows::elevation::relaunch_elevated()?;
    // The single-instance lock is released when this instance exits.
    app.exit(0);
    Ok(())
}

/// Opens one of the application's own folders in Explorer.
#[tauri::command]
pub fn open_app_folder(app: AppHandle, state: State<'_, AppState>, which: String) -> CmdResult<()> {
    let p = match which.as_str() {
        "data" => &state.paths.data,
        "logs" => &state.paths.logs,
        "tools" => &state.paths.tools,
        _ => return Err("Unknown folder".into()),
    };
    std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
    app.opener()
        .open_path(p.display().to_string(), None::<&str>)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_update_status(app: AppHandle) -> crate::updater::UpdateStatus {
    let mut s = app.state::<crate::updater::Updates>().0.lock().clone();
    s.current_version = app.package_info().version.to_string();
    s
}

/// Contacts the GitHub release endpoint now (explicit user action).
#[tauri::command]
pub async fn check_for_update(app: AppHandle) -> Result<crate::updater::UpdateStatus, String> {
    Ok(crate::updater::check(&app).await)
}

/// Downloads, verifies the signature of, and installs the available update, then restarts.
#[tauri::command]
pub async fn install_update(app: AppHandle) -> CmdResult<()> {
    crate::updater::install(&app).await
}
