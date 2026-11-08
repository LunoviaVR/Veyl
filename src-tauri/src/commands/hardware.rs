use std::sync::atomic::Ordering;
use std::sync::Arc;

use serde::Serialize;
use tauri::State;
use veyl_core::{DriveHealth, SensorsView};

use crate::state::AppState;

#[tauri::command]
pub fn get_sensors(state: State<'_, AppState>) -> Arc<SensorsView> {
    state.live.read().sensors.clone()
}

#[tauri::command]
pub fn reset_sensor_stats(state: State<'_, AppState>) {
    state.reset_sensor_stats.store(true, Ordering::Relaxed);
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DriveHealthView {
    pub drives: Arc<Vec<DriveHealth>>,
    pub updated_ms: Option<i64>,
}

#[tauri::command]
pub fn get_drive_health(state: State<'_, AppState>) -> DriveHealthView {
    let live = state.live.read();
    DriveHealthView {
        drives: live.drives.clone(),
        updated_ms: live.drives_updated_ms,
    }
}

/// Asks the sampler to re-read drive health on its next tick.
#[tauri::command]
pub fn refresh_drive_health(state: State<'_, AppState>) {
    state.refresh_drives.store(true, Ordering::Relaxed);
}
