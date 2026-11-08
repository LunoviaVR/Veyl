//! Notification-area icon: open the window, toggle the overlay, exit.

use std::sync::atomic::Ordering;

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager};

use crate::state::AppState;

pub fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
        // Hiding stops live events; WebView2 never tells the page it was hidden, so the page can't resume them.
        app.state::<AppState>()
            .ui_visible
            .store(true, Ordering::Relaxed);
    }
}

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open Veyl", true, None::<&str>)?;
    let overlay = MenuItem::with_id(app, "overlay", "Toggle game overlay", true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "Exit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &overlay, &sep, &quit])?;
    let mut builder = TrayIconBuilder::with_id("main")
        .tooltip("Veyl")
        .menu(&menu)
        .show_menu_on_left_click(false);
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder
        .on_menu_event(|app, e| match e.id.as_ref() {
            "open" => show_main(app),
            "overlay" => {
                let state = app.state::<AppState>();
                let snapshot = {
                    let mut s = state.settings.write();
                    s.overlay.enabled = !s.overlay.enabled;
                    s.clone()
                };
                if let Err(e) = snapshot.save(&state.paths.settings) {
                    tracing::warn!("saving settings failed: {e}");
                }
                crate::overlay::apply_later(app, snapshot.overlay.clone(), 0);
                let _ = app.emit("settings-changed", &snapshot); // windows may not be listening
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, e| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = e
            {
                show_main(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}
