//! Optional in-game overlay: a separate transparent, click-through,
//! always-on-top window that renders live metrics. It is an ordinary desktop
//! window composed by Windows (DWM): it appears over borderless/windowed games
//! but not over exclusive-fullscreen ones, and it never touches the game process.

use std::sync::atomic::Ordering;

use tauri::{AppHandle, Manager, PhysicalPosition, WebviewUrl, WebviewWindowBuilder};

use crate::settings::OverlaySettings;
use crate::state::AppState;

pub const LABEL: &str = "overlay";
const BASE_W: f64 = 230.0;
const BASE_H: f64 = 210.0;
const MARGIN: i32 = 16;

/// Applies overlay settings from a background thread. Creating a webview window on
/// the main thread (during setup, in a menu handler or a synchronous command)
/// stalls IPC for the main window on Windows (wry#583), so this is the only entry
/// point used outside async commands.
pub fn apply_later(app: &AppHandle, cfg: OverlaySettings, delay_ms: u64) {
    let app = app.clone();
    let spawned = std::thread::Builder::new()
        .name("overlay".into())
        .spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(delay_ms));
            let state = app.state::<AppState>();
            if let Err(e) = apply(&app, &state, &cfg) {
                tracing::warn!("overlay: {e}");
            }
        });
    if let Err(e) = spawned {
        tracing::warn!("could not start overlay thread: {e}");
    }
}

/// Creates, updates or closes the overlay. Must not run on the main thread.
pub fn apply(app: &AppHandle, state: &AppState, cfg: &OverlaySettings) -> Result<(), String> {
    let existing = app.get_webview_window(LABEL);
    if !cfg.enabled {
        if let Some(w) = existing {
            w.close().map_err(|e| e.to_string())?;
        }
        state.overlay_open.store(false, Ordering::Relaxed);
        return Ok(());
    }
    let (w, h) = if cfg.layout == "horizontal" {
        (BASE_W * 2.6 * cfg.scale, 64.0 * cfg.scale)
    } else {
        (BASE_W * cfg.scale, BASE_H * cfg.scale)
    };
    let win = match existing {
        Some(win) => win,
        None => WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("overlay.html".into()))
            .title("Veyl overlay")
            .decorations(false)
            .transparent(true)
            .shadow(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .resizable(false)
            .focused(false)
            .focusable(false)
            .inner_size(w, h)
            .build()
            .map_err(|e| format!("Could not create the overlay window: {e}"))?,
    };
    win.set_size(tauri::LogicalSize::new(w, h))
        .map_err(|e| e.to_string())?;
    win.set_ignore_cursor_events(true)
        .map_err(|e| e.to_string())?;
    if let Some(m) = win.primary_monitor().map_err(|e| e.to_string())? {
        let (ms, mp) = (m.size(), m.position());
        let size = win.outer_size().map_err(|e| e.to_string())?;
        let x = if cfg.position.ends_with("Right") {
            mp.x + ms.width as i32 - size.width as i32 - MARGIN
        } else {
            mp.x + MARGIN
        };
        let y = if cfg.position.starts_with("bottom") {
            mp.y + ms.height as i32 - size.height as i32 - MARGIN * 3
        } else {
            mp.y + MARGIN
        };
        win.set_position(PhysicalPosition::new(x, y))
            .map_err(|e| e.to_string())?;
    }
    state.overlay_open.store(true, Ordering::Relaxed);
    Ok(())
}
