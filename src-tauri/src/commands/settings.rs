use tauri::{AppHandle, Emitter, State};

use super::CmdResult;
use crate::settings::{default_rules, Settings};
use crate::state::AppState;
use crate::writer::{self, WriteMsg};

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Settings {
    state.settings()
}

/// Validates, persists and applies settings; returns what was actually stored.
#[tauri::command(async)]
pub fn save_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: Settings,
) -> CmdResult<Settings> {
    let validated = settings.validated();
    validated
        .save(&state.paths.settings)
        .map_err(|e| format!("Could not save settings: {e}"))?;
    let previous = std::mem::replace(&mut *state.settings.write(), validated.clone());
    if previous.retention != validated.retention {
        writer::send(
            &state.writer,
            WriteMsg::Retention(validated.retention.clone()),
        );
    }
    if previous.overlay != validated.overlay {
        crate::overlay::apply(&app, &state, &validated.overlay)?;
    }
    let _ = app.emit("settings-changed", &validated); // windows may not be listening
    Ok(validated)
}

#[tauri::command(async)]
pub fn reset_alert_rules(app: AppHandle, state: State<'_, AppState>) -> CmdResult<Settings> {
    let mut s = state.settings();
    s.alert_rules = default_rules();
    save_settings(app, state, s)
}
