//! Veyl application shell: wires collectors, storage and the UI
//! together. Domain logic lives in `veyl-core`; Windows access in `veyl-windows`;
//! persistence in `veyl-storage`.

mod commands;
mod events_watch;
mod export;
mod gaming;
mod metrics;
mod notify;
mod overlay;
mod packets;
mod sampler;
mod settings;
mod state;
mod tray;
mod updater;
mod writer;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, sync_channel, Sender};
use std::sync::Arc;

use parking_lot::{Mutex, RwLock};
use tauri::{App, AppHandle, Manager, RunEvent, WindowEvent};
use veyl_storage::Db;

use crate::settings::Settings;
use crate::state::{now_ms, AppState, Live, Paths};

/// Environment variable that overrides the data directory (portable or managed installs).
pub const DATA_DIR_ENV: &str = "VEYL_DATA_DIR";

/// Keeps the non-blocking log writer alive; flushed when dropped at exit.
struct LogGuard(#[allow(dead_code)] tracing_appender::non_blocking::WorkerGuard);

/// Stops the sampler when dropped.
struct SamplerStop(Mutex<Option<Sender<()>>>);

fn resolve_paths(app: &App) -> Result<Paths, String> {
    let data = match std::env::var_os(DATA_DIR_ENV) {
        Some(p) if !p.is_empty() => PathBuf::from(p),
        _ => app
            .path()
            .app_local_data_dir()
            .map_err(|e| format!("no local data directory: {e}"))?,
    };
    let p = Paths {
        db: data.join("history.db"),
        settings: data.join("settings.json"),
        logs: data.join("logs"),
        // Next to the program: per-machine installs put it in Program Files, where only
        // administrators can add executables for the elevated app to run.
        tools: std::env::current_exe()
            .ok()
            .and_then(|e| e.parent().map(|d| d.join("tools")))
            .unwrap_or_else(|| data.join("tools")),
        symbols: data.join("symbols"),
        data,
    };
    for d in [&p.data, &p.logs] {
        std::fs::create_dir_all(d).map_err(|e| format!("cannot create {}: {e}", d.display()))?;
    }
    Ok(p)
}

fn init_logging(dir: &std::path::Path) -> Option<tracing_appender::non_blocking::WorkerGuard> {
    use tracing_subscriber::prelude::*;
    let appender = tracing_appender::rolling::Builder::new()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix("veyl")
        .filename_suffix("log")
        .max_log_files(7)
        .build(dir)
        .ok()?;
    let (writer, guard) = tracing_appender::non_blocking(appender);
    let filter = tracing_subscriber::EnvFilter::try_from_env("VEYL_LOG")
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(writer)
                .with_ansi(false),
        )
        .try_init();
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        tracing::error!("panic: {info}");
        default_hook(info);
    }));
    Some(guard)
}

fn spawn(state: &AppState, name: &'static str, f: impl FnOnce() + Send + 'static) {
    match std::thread::Builder::new().name(name.into()).spawn(f) {
        Ok(h) => state.threads.lock().push((name, h)),
        Err(e) => tracing::error!("could not start thread {name}: {e}"),
    }
}

fn setup(app: &mut App) -> Result<(), Box<dyn std::error::Error>> {
    let paths = resolve_paths(app)?;
    if let Some(g) = init_logging(&paths.logs) {
        app.manage(LogGuard(g));
    }
    tracing::info!(
        "starting {} {} (data: {})",
        app.package_info().name,
        app.package_info().version,
        paths.data.display()
    );

    let settings = Settings::load(&paths.settings);
    if !paths.settings.exists() {
        if let Err(e) = settings.save(&paths.settings) {
            tracing::warn!("could not write default settings: {e}");
        }
    }

    let (writer_tx, writer_rx) = sync_channel(writer::QUEUE_LEN);
    let (gaming_tx, gaming_rx) = sync_channel(16);
    let (stop_tx, stop_rx) = channel::<()>();
    let shutdown = Arc::new(AtomicBool::new(false));

    let (write_db, read_db, db_error) = match (Db::open(&paths.db), Db::open(&paths.db)) {
        (Ok(w), Ok(r)) => (Some(w), Some(r), None),
        (Err(e), _) | (_, Err(e)) => {
            tracing::error!("history database unavailable: {e}");
            (
                None,
                None,
                Some(format!(
                    "History database could not be opened ({e}). Live monitoring still works."
                )),
            )
        }
    };

    let (cpu, cpu_status) = veyl_windows::cpu::cpu_info();
    let system_info = veyl_windows::system_info::collect(now_ms(), cpu, cpu_status);
    let retention = settings.retention.clone();
    let overlay_cfg = settings.overlay.clone();
    let start_minimized = settings.start_minimized;

    app.manage(AppState {
        paths,
        settings: RwLock::new(settings),
        live: RwLock::new(Live::default()),
        system_info: RwLock::new(system_info),
        reader: Mutex::new(read_db),
        db_error: RwLock::new(db_error),
        writer: writer_tx,
        gaming_tx,
        gaming: RwLock::new(Default::default()),
        event_channels: RwLock::new(Vec::new()),
        ui_visible: AtomicBool::new(!start_minimized),
        overlay_open: AtomicBool::new(false),
        game_window: parking_lot::RwLock::new(None),
        reset_sensor_stats: AtomicBool::new(false),
        refresh_drives: AtomicBool::new(false),
        shutdown: shutdown.clone(),
        threads: Mutex::new(Vec::new()),
        started_ms: now_ms(),
    });
    app.manage(SamplerStop(Mutex::new(Some(stop_tx))));
    app.manage(packets::Capture::default());
    // A capture session left by a run that did not exit cleanly keeps logging; end it.
    veyl_windows::packet_capture::stop_stale_session();
    app.manage(updater::Updates::default());

    let handle = app.handle().clone();
    let state = app.state::<AppState>();
    if let Some(db) = write_db {
        let sd = shutdown.clone();
        spawn(&state, "db-writer", move || {
            writer::run(db, writer_rx, retention, sd)
        });
    }
    let h = handle.clone();
    spawn(&state, "sampler", move || sampler::run(h, stop_rx));
    let h = handle.clone();
    spawn(&state, "event-log", move || events_watch::run(h));
    let h = handle.clone();
    spawn(&state, "gaming", move || gaming::run(h, gaming_rx));
    let h = handle.clone();
    spawn(&state, "updater", move || updater::run(h));

    tray::create(&handle)?;
    if overlay_cfg.enabled {
        // Created after the main window has loaded, off the main thread.
        overlay::apply_later(&handle, overlay_cfg, 1500);
    }
    if let Some(w) = app.get_webview_window("main") {
        if start_minimized {
            let _ = w.hide();
        } else {
            let _ = w.show();
        }
    }
    Ok(())
}

fn shutdown(app: &AppHandle) {
    let state = app.state::<AppState>();
    if state.shutdown.swap(true, Ordering::SeqCst) {
        return;
    }
    tracing::info!("shutting down");
    // Dropping the sender wakes the sampler immediately.
    app.state::<SamplerStop>().0.lock().take();
    app.state::<packets::Capture>().stop();
    let threads: Vec<_> = state.threads.lock().drain(..).collect();
    for (name, h) in threads {
        if h.join().is_err() {
            tracing::error!("thread {name} panicked during shutdown");
        }
    }
    tracing::info!("stopped cleanly");
}

pub fn run() {
    // Started by "Restart as administrator": let the old instance finish exiting first.
    let mut args = std::env::args().skip_while(|a| a != veyl_windows::elevation::WAIT_PID_ARG);
    if let Some(pid) = args.nth(1).and_then(|p| p.parse().ok()) {
        veyl_windows::elevation::wait_for_exit(pid, 15_000);
    }
    let app = tauri::Builder::default()
        // Must be first: a second launch focuses the running instance instead of starting another monitor.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            tray::show_main(app)
        }))
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_denylist(&[overlay::LABEL])
                .build(),
        )
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| setup(app))
        .on_window_event(|window, event| {
            if window.label() != "main" {
                return;
            }
            let state = window.state::<AppState>();
            match event {
                WindowEvent::CloseRequested { api, .. } => {
                    if state.settings.read().close_to_tray {
                        api.prevent_close();
                        let _ = window.hide();
                        state.ui_visible.store(false, Ordering::Relaxed);
                    } else {
                        window.app_handle().exit(0);
                    }
                }
                // Minimizing hides the window to the notification area; monitoring continues.
                WindowEvent::Resized(_)
                    if state.settings.read().minimize_to_tray
                        && window.is_minimized().unwrap_or(false) =>
                {
                    let _ = window.hide();
                    state.ui_visible.store(false, Ordering::Relaxed);
                }
                _ => {}
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::system::get_system_info,
            commands::system::get_pc_specs,
            commands::system::get_latest,
            commands::system::set_ui_visible,
            commands::system::get_runtime_status,
            commands::system::restart_as_admin,
            commands::system::open_app_folder,
            commands::system::get_update_status,
            commands::system::check_for_update,
            commands::system::install_update,
            commands::processes::get_processes,
            commands::processes::get_process_details,
            commands::processes::end_process,
            commands::processes::end_process_tree,
            commands::processes::set_process_priority,
            commands::processes::reveal_path,
            commands::processes::get_services,
            commands::processes::get_startup_items,
            commands::processes::get_connections,
            commands::hardware::get_sensors,
            commands::hardware::reset_sensor_stats,
            commands::hardware::get_drive_health,
            commands::hardware::refresh_drive_health,
            commands::stability::get_stability_events,
            commands::stability::get_event_channels,
            commands::stability::get_event_detail,
            commands::stability::get_bsod_overview,
            commands::stability::analyze_dump,
            commands::stability::install_debugger,
            commands::stability::get_diagnostics,
            commands::stability::get_timeline,
            commands::gaming::get_gaming_status,
            commands::gaming::get_game_candidates,
            commands::gaming::start_capture,
            commands::gaming::stop_capture,
            commands::gaming::get_game_sessions,
            commands::gaming::get_game_session,
            commands::gaming::delete_game_session,
            commands::gaming::set_overlay_enabled,
            commands::gaming::set_auto_capture_games,
            commands::gaming::set_overlay_style,
            commands::gaming::set_overlay_position,
            commands::network::get_capture_interfaces,
            commands::network::start_packet_capture,
            commands::network::stop_packet_capture,
            commands::network::clear_packets,
            commands::network::get_packets,
            commands::network::get_packet_detail,
            commands::network::check_packet_filter,
            commands::history::get_metric_catalog,
            commands::history::get_history,
            commands::history::get_alert_history,
            commands::history::get_active_alerts,
            commands::history::clear_active_alerts,
            commands::settings::get_settings,
            commands::settings::save_settings,
            commands::settings::reset_alert_rules,
            commands::export::export_history_csv,
            commands::export::export_pc_specs,
            commands::export::export_session_csv,
            commands::export::export_events_csv,
            commands::export::export_report,
        ])
        .build(tauri::generate_context!());
    match app {
        Ok(app) => app.run(|handle, event| {
            if let RunEvent::Exit = event {
                shutdown(handle);
            }
        }),
        Err(e) => {
            eprintln!("Veyl failed to start: {e}");
            std::process::exit(1);
        }
    }
}
