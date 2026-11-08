use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
use veyl_storage::SessionRow;

use super::CmdResult;
use crate::gaming::{GamingCommand, GamingLive};
use crate::state::AppState;
use crate::writer::{self, WriteMsg};

#[tauri::command]
pub fn get_gaming_status(state: State<'_, AppState>) -> GamingLive {
    state.gaming.read().clone()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub pid: u32,
    pub name: String,
    pub path: Option<String>,
    pub gpu_pct: Option<f32>,
    pub gpu_dedicated_bytes: Option<u64>,
    pub foreground: bool,
}

/// Processes that are using the GPU right now, plus the foreground app. A
/// suggestion list for choosing what to capture; it does not claim a process is a game.
#[tauri::command]
pub fn get_game_candidates(state: State<'_, AppState>) -> Vec<Candidate> {
    const IGNORE: [&str; 8] = [
        "dwm.exe",
        "csrss.exe",
        "explorer.exe",
        "system",
        "searchhost.exe",
        "startmenuexperiencehost.exe",
        "textinputhost.exe",
        "veyl.exe",
    ];
    let fg = veyl_windows::elevation::foreground_pid();
    let own = std::process::id();
    let table = state.live.read().processes.clone();
    let mut list: Vec<Candidate> = table
        .rows
        .iter()
        .filter(|r| {
            r.pid != own && r.pid > 4 && !IGNORE.contains(&r.name.to_ascii_lowercase().as_str())
        })
        .filter(|r| Some(r.pid) == fg || r.gpu_pct.unwrap_or(0.0) >= 1.0)
        .map(|r| Candidate {
            pid: r.pid,
            name: r.name.clone(),
            path: r.path.clone(),
            gpu_pct: r.gpu_pct,
            gpu_dedicated_bytes: r.gpu_dedicated_bytes,
            foreground: Some(r.pid) == fg,
        })
        .collect();
    list.sort_by(|a, b| {
        b.foreground.cmp(&a.foreground).then(
            b.gpu_pct
                .unwrap_or(0.0)
                .total_cmp(&a.gpu_pct.unwrap_or(0.0)),
        )
    });
    list.truncate(25);
    list
}

#[tauri::command]
pub fn start_capture(state: State<'_, AppState>, pid: u32) -> CmdResult<()> {
    if pid <= 4 || pid == std::process::id() {
        return Err("This process cannot be captured".into());
    }
    state
        .gaming_tx
        .try_send(GamingCommand::Start { pid })
        .map_err(|_| "The capture service is busy; try again".to_string())
}

#[tauri::command]
pub fn stop_capture(state: State<'_, AppState>) -> CmdResult<()> {
    state
        .gaming_tx
        .try_send(GamingCommand::Stop)
        .map_err(|_| "The capture service is busy; try again".to_string())
}

#[tauri::command(async)]
pub fn get_game_sessions(
    state: State<'_, AppState>,
    limit: Option<u32>,
) -> CmdResult<Vec<SessionRow>> {
    state.with_db(|db| db.sessions(limit.unwrap_or(200).clamp(1, 2000)))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionDetail {
    pub session: SessionRow,
    pub frametimes: Vec<f32>,
}

#[tauri::command(async)]
pub fn get_game_session(state: State<'_, AppState>, id: i64) -> CmdResult<SessionDetail> {
    let (session, frametimes) = state
        .with_db(|db| db.session(id))?
        .ok_or("Session not found")?;
    Ok(SessionDetail {
        session,
        frametimes,
    })
}

#[tauri::command]
pub fn delete_game_session(state: State<'_, AppState>, id: i64) {
    writer::send(&state.writer, WriteMsg::DeleteSession(id));
}

#[tauri::command(async)]
pub fn set_overlay_enabled(
    app: AppHandle,
    state: State<'_, AppState>,
    enabled: bool,
) -> CmdResult<()> {
    let mut s = state.settings.write();
    s.overlay.enabled = enabled;
    let snapshot = s.clone();
    drop(s);
    snapshot
        .save(&state.paths.settings)
        .map_err(|e| e.to_string())?;
    crate::overlay::apply(&app, &state, &snapshot.overlay)?;
    let _ = app.emit("settings-changed", &snapshot); // windows may not be listening
    Ok(())
}
