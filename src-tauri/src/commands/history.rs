use tauri::{AppHandle, Emitter, State};
use veyl_core::alerts::AlertSignal;
use veyl_storage::{AlertRow, MetricInfo, Series};

use super::{check_range, CmdResult};
use crate::state::{now_ms, AppState};

pub(crate) fn check_keys(keys: &[String]) -> CmdResult<()> {
    if keys.is_empty() || keys.len() > 128 || keys.iter().any(|k| k.is_empty() || k.len() > 200) {
        return Err("Select between 1 and 128 metrics".into());
    }
    Ok(())
}

#[tauri::command(async)]
pub fn get_metric_catalog(state: State<'_, AppState>) -> CmdResult<Vec<MetricInfo>> {
    state.with_db(|db| db.metric_catalog())
}

#[tauri::command(async)]
pub fn get_history(
    state: State<'_, AppState>,
    keys: Vec<String>,
    from_ms: i64,
    to_ms: i64,
    max_points: Option<u32>,
) -> CmdResult<Vec<Series>> {
    check_keys(&keys)?;
    check_range(from_ms, to_ms)?;
    let raw_hours = state.settings.read().retention.raw_hours;
    let points = max_points.unwrap_or(600).clamp(10, 4000);
    state.with_db(|db| db.series(&keys, from_ms, to_ms, points, now_ms(), raw_hours))
}

#[tauri::command(async)]
pub fn get_alert_history(
    state: State<'_, AppState>,
    limit: Option<u32>,
) -> CmdResult<Vec<AlertRow>> {
    state.with_db(|db| db.alerts(limit.unwrap_or(500).clamp(1, 10_000)))
}

#[tauri::command]
pub fn get_active_alerts(state: State<'_, AppState>) -> Vec<AlertSignal> {
    state.live.read().active_alerts.clone()
}

/// Clears the active-alert list only; alert history stays. The engine keeps the
/// rule triggered, so a cleared alert returns only after it recovers and fires anew.
#[tauri::command]
pub fn clear_active_alerts(app: AppHandle, state: State<'_, AppState>) {
    state.live.write().active_alerts.clear();
    let _ = app.emit("alert", ()); // every view showing active alerts refetches on this
}
