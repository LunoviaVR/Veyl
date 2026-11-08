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
    /// Windows lists it as a game, or it runs from a game store's library.
    pub is_game: bool,
}

/// Running games first, then processes using the GPU
/// and the foreground app: a suggestion list for choosing what to capture.
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
    let games = veyl_windows::games::Games::current();
    let game = |r: &veyl_core::ProcessRow| {
        r.path
            .as_deref()
            .is_some_and(|p| games.is_game(p, table.app_pids.binary_search(&r.pid).is_ok()))
    };
    let mut list: Vec<Candidate> = table
        .rows
        .iter()
        .filter(|r| {
            r.pid != own && r.pid > 4 && !IGNORE.contains(&r.name.to_ascii_lowercase().as_str())
        })
        .filter(|r| Some(r.pid) == fg || r.gpu_pct.unwrap_or(0.0) >= 1.0 || game(r))
        .map(|r| Candidate {
            pid: r.pid,
            name: r.name.clone(),
            path: r.path.clone(),
            gpu_pct: r.gpu_pct,
            gpu_dedicated_bytes: r.gpu_dedicated_bytes,
            foreground: Some(r.pid) == fg,
            is_game: game(r),
        })
        .collect();
    list.sort_by(|a, b| {
        b.is_game
            .cmp(&a.is_game)
            .then(b.foreground.cmp(&a.foreground))
            .then(
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

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PresentMonRelease {
    pub version: &'static str,
    pub size_bytes: u64,
    pub url: &'static str,
}

/// What `install_presentmon` would download, for the button text.
#[tauri::command]
pub fn get_presentmon_release() -> PresentMonRelease {
    use veyl_windows::presentmon::{PINNED_SIZE, PINNED_URL, PINNED_VERSION};
    PresentMonRelease {
        version: PINNED_VERSION,
        size_bytes: PINNED_SIZE,
        url: PINNED_URL,
    }
}

/// Downloads the pinned PresentMon release into the tools folder (explicit user action).
#[tauri::command(async)]
pub fn install_presentmon(state: State<'_, AppState>) -> CmdResult<String> {
    let path = veyl_windows::presentmon::install_pinned(&state.paths.tools)?;
    tracing::info!("installed PresentMon to {}", path.display());
    // A full queue only delays the status update until the next capture command.
    let _ = state.gaming_tx.try_send(GamingCommand::Refresh);
    Ok(path.display().to_string())
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

/// Picks a bundled overlay design and theme (Gaming page).
#[tauri::command]
pub fn set_overlay_style(
    app: AppHandle,
    state: State<'_, AppState>,
    design: String,
    theme: String,
) -> CmdResult<()> {
    use crate::settings::{OVERLAY_DESIGNS, OVERLAY_THEMES};
    if !OVERLAY_DESIGNS.contains(&design.as_str()) || !OVERLAY_THEMES.contains(&theme.as_str()) {
        return Err("Unknown overlay design or theme".into());
    }
    let snapshot = {
        let mut s = state.settings.write();
        s.overlay.design = design;
        s.overlay.theme = theme;
        s.clone()
    };
    snapshot
        .save(&state.paths.settings)
        .map_err(|e| e.to_string())?;
    let _ = app.emit("settings-changed", &snapshot); // the overlay window reloads its layout
    Ok(())
}

/// Where the overlay sits on the game's window (Gaming page).
#[tauri::command(async)]
pub fn set_overlay_position(
    app: AppHandle,
    state: State<'_, AppState>,
    position: String,
) -> CmdResult<()> {
    if !crate::settings::OVERLAY_POSITIONS.contains(&position.as_str()) {
        return Err("Unknown overlay position".into());
    }
    let snapshot = {
        let mut s = state.settings.write();
        s.overlay.position = position;
        s.clone()
    };
    snapshot
        .save(&state.paths.settings)
        .map_err(|e| e.to_string())?;
    crate::overlay::apply(&app, &state, &snapshot.overlay)?; // moves the window now
    let _ = app.emit("settings-changed", &snapshot); // the overlay re-anchors its content
    Ok(())
}

#[tauri::command]
pub fn set_auto_capture_games(
    app: AppHandle,
    state: State<'_, AppState>,
    enabled: bool,
) -> CmdResult<()> {
    let snapshot = {
        let mut s = state.settings.write();
        s.auto_capture_games = enabled;
        s.clone()
    };
    snapshot
        .save(&state.paths.settings)
        .map_err(|e| e.to_string())?;
    let _ = app.emit("settings-changed", &snapshot); // windows may not be listening
    Ok(())
}
