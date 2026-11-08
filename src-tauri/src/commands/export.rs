//! User-initiated exports. Nothing is written until the user picks a target
//! file in the save dialog; nothing is ever sent anywhere.

use std::path::PathBuf;

use pch_core::events::classify;
use pch_core::redact::redact;
use serde::Serialize;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_dialog::DialogExt;

use super::{check_range, CmdResult};
use crate::export::{events_csv, frametimes_csv, redaction_context, series_csv};
use crate::state::{now_ms, AppState};

fn ask_path(app: &AppHandle, default_name: &str, ext: &str, label: &str) -> Option<PathBuf> {
    app.dialog()
        .file()
        .set_file_name(default_name)
        .add_filter(label, &[ext])
        .blocking_save_file()
        .and_then(|p| p.into_path().ok())
}

fn write(path: PathBuf, text: String) -> CmdResult<Option<String>> {
    std::fs::write(&path, text).map_err(|e| format!("Could not write {}: {e}", path.display()))?;
    Ok(Some(path.display().to_string()))
}

#[tauri::command(async)]
pub fn export_history_csv(
    app: AppHandle,
    state: State<'_, AppState>,
    keys: Vec<String>,
    from_ms: i64,
    to_ms: i64,
) -> CmdResult<Option<String>> {
    super::history::check_keys(&keys)?;
    check_range(from_ms, to_ms)?;
    let raw_hours = state.settings.read().retention.raw_hours;
    let series = state.with_db(|db| db.series(&keys, from_ms, to_ms, 4000, now_ms(), raw_hours))?;
    let Some(path) = ask_path(&app, "history.csv", "csv", "CSV") else {
        return Ok(None);
    };
    write(path, series_csv(&series))
}

#[tauri::command(async)]
pub fn export_session_csv(
    app: AppHandle,
    state: State<'_, AppState>,
    id: i64,
) -> CmdResult<Option<String>> {
    let (s, ft) = state
        .with_db(|db| db.session(id))?
        .ok_or("Session not found")?;
    let name = format!(
        "{}-{}.csv",
        s.process_name.trim_end_matches(".exe"),
        s.started_ms
    );
    let Some(path) = ask_path(&app, &name, "csv", "CSV") else {
        return Ok(None);
    };
    write(path, frametimes_csv(&ft))
}

#[tauri::command(async)]
pub fn export_events_csv(
    app: AppHandle,
    state: State<'_, AppState>,
    from_ms: i64,
    to_ms: i64,
    redact: bool,
) -> CmdResult<Option<String>> {
    check_range(from_ms, to_ms)?;
    let events: Vec<_> = state
        .with_db(|db| db.events(from_ms, to_ms, 20_000))?
        .iter()
        .filter_map(classify)
        .collect();
    let ctx = redaction_context(state.system_info.read().computer_name.as_deref());
    let Some(path) = ask_path(&app, "stability-events.csv", "csv", "CSV") else {
        return Ok(None);
    };
    write(path, events_csv(&events, redact.then_some(&ctx)))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Report {
    generated_ms: i64,
    application_version: String,
    redacted: bool,
    system: pch_core::SystemInfo,
    findings: Vec<pch_core::diagnostics::Finding>,
    incidents: Vec<pch_core::events::ShutdownIncident>,
    events_30d: Vec<pch_core::events::StabilityEvent>,
    drives: Vec<pch_core::DriveHealth>,
}

/// A JSON diagnostic report (system summary, findings, incidents, events, drives).
#[tauri::command(async)]
pub fn export_report(app: AppHandle, redact_personal: bool) -> CmdResult<Option<String>> {
    let state = app.state::<AppState>();
    let now = now_ms();
    let events: Vec<_> = state
        .with_db(|db| db.events(now - 30 * 86_400_000, now, 20_000))?
        .iter()
        .filter_map(classify)
        .collect();
    let report = Report {
        generated_ms: now,
        application_version: app.package_info().version.to_string(),
        redacted: redact_personal,
        system: state.system_info.read().clone(),
        findings: super::stability::get_diagnostics(state.clone()).unwrap_or_default(),
        incidents: pch_core::events::group_shutdown_incidents(&events),
        events_30d: events,
        drives: state.live.read().drives.as_ref().clone(),
    };
    let mut text = serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?;
    if redact_personal {
        let mut ctx = redaction_context(report.system.computer_name.as_deref());
        // Paths appear JSON-escaped (doubled backslashes) in the serialized text.
        ctx.user_profile = ctx.user_profile.replace('\\', "\\\\");
        text = redact(&text, &ctx);
        // Serial numbers identify hardware.
        if let Ok(mut v) = serde_json::from_str::<serde_json::Value>(&text) {
            if let Some(d) = v.get_mut("drives").and_then(|d| d.as_array_mut()) {
                for drive in d {
                    drive["serial"] = serde_json::Value::Null;
                }
            }
            text = serde_json::to_string_pretty(&v).map_err(|e| e.to_string())?;
        }
    }
    let Some(path) = ask_path(&app, "pc-health-report.json", "json", "JSON") else {
        return Ok(None);
    };
    write(path, text)
}
