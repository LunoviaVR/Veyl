//! Optional in-game overlay: a separate transparent, click-through,
//! always-on-top window that renders live metrics in a corner of the game's
//! window while the game is in front. It is an ordinary desktop window composed
//! by Windows (DWM): it appears over borderless/windowed games but not over
//! exclusive-fullscreen ones, and it never touches the game process.

use std::sync::atomic::Ordering;

use tauri::{
    AppHandle, Manager, PhysicalPosition, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
};
use veyl_windows::elevation::ScreenRect;

use crate::settings::OverlaySettings;
use crate::state::AppState;

pub const LABEL: &str = "overlay";
/// Window size at scale 1: fits the largest bundled design (750 wide, 544 tall). The design
/// sits in the window's corner nearest the game's corner; the rest is transparent and click-through.
const SIZE: (f64, f64) = (760.0, 560.0);
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
    let (w, h) = (SIZE.0 * cfg.scale, SIZE.1 * cfg.scale);
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
            .visible(false)
            .inner_size(w, h)
            .build()
            .map_err(|e| format!("Could not create the overlay window: {e}"))?,
    };
    win.set_size(tauri::LogicalSize::new(w, h))
        .map_err(|e| e.to_string())?;
    win.set_ignore_cursor_events(true)
        .map_err(|e| e.to_string())?;
    follow_game(&win, cfg, *state.game_window.read());
    state.overlay_open.store(true, Ordering::Relaxed);
    Ok(())
}

/// Top-left point that puts a `w` x `h` overlay at the chosen spot of the game's drawing area
/// (see `OVERLAY_POSITIONS`): edges keep a margin, centre and middle are centred.
fn corner(game: ScreenRect, w: i32, h: i32, position: &str) -> PhysicalPosition<i32> {
    let x = if position.ends_with("Left") {
        game.x + MARGIN
    } else if position.ends_with("Right") {
        game.x + game.width - w - MARGIN
    } else {
        game.x + (game.width - w) / 2
    };
    let y = if position.starts_with("top") {
        game.y + MARGIN
    } else if position.starts_with("bottom") {
        game.y + game.height - h - MARGIN
    } else {
        game.y + (game.height - h) / 2
    };
    PhysicalPosition::new(x, y)
}

/// Shows the overlay in a corner of the game in front, or hides it when no game is in front:
/// it never appears over other applications.
pub fn follow_game(win: &WebviewWindow, cfg: &OverlaySettings, game: Option<ScreenRect>) {
    let r = match game {
        None => win.hide(),
        Some(g) => win.outer_size().and_then(|s| {
            win.set_position(corner(g, s.width as i32, s.height as i32, &cfg.position))?;
            win.show()
        }),
    };
    if let Err(e) = r {
        tracing::debug!("overlay placement: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlay_sits_inside_the_chosen_corner_of_the_game() {
        // A 1280x720 window on a second monitor to the left of the primary one.
        let g = ScreenRect {
            x: -1600,
            y: 100,
            width: 1280,
            height: 720,
        };
        assert_eq!(
            corner(g, 200, 100, "topLeft"),
            PhysicalPosition::new(-1584, 116)
        );
        assert_eq!(
            corner(g, 200, 100, "topRight"),
            PhysicalPosition::new(-536, 116)
        );
        assert_eq!(
            corner(g, 200, 100, "bottomLeft"),
            PhysicalPosition::new(-1584, 704)
        );
        assert_eq!(
            corner(g, 200, 100, "bottomRight"),
            PhysicalPosition::new(-536, 704)
        );
        assert_eq!(
            corner(g, 200, 100, "topCenter"),
            PhysicalPosition::new(-1060, 116)
        );
        assert_eq!(
            corner(g, 200, 100, "center"),
            PhysicalPosition::new(-1060, 410)
        );
        assert_eq!(
            corner(g, 200, 100, "middleLeft"),
            PhysicalPosition::new(-1584, 410)
        );
        assert_eq!(
            corner(g, 200, 100, "bottomCenter"),
            PhysicalPosition::new(-1060, 704)
        );
    }
}
