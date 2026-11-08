//! Signed auto-update from GitHub Releases (tauri-plugin-updater).
//!
//! The endpoint and public key are in `tauri.conf.json → plugins.updater`. The
//! release workflow publishes `latest.json` plus installers signed with the
//! matching private key; an update whose signature does not verify is refused by
//! the plugin. Checks run only when "Check for updates automatically" is on, or
//! when the user presses "Check now". Installing is always a user action.

use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::UpdaterExt;

use crate::state::{now_ms, AppState};

const FIRST_CHECK_DELAY: Duration = Duration::from_secs(20);
const CHECK_EVERY: Duration = Duration::from_secs(12 * 3600);

#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub version: String,
    pub notes: Option<String>,
    pub date: Option<String>,
}

#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    pub current_version: String,
    pub available: Option<UpdateInfo>,
    pub last_check_ms: Option<i64>,
    pub last_error: Option<String>,
    pub installing: bool,
}

/// Shared update state (managed by Tauri).
#[derive(Default)]
pub struct Updates(pub Mutex<UpdateStatus>);

/// Checks the release endpoint once and records the result.
pub async fn check(app: &AppHandle) -> UpdateStatus {
    let result = match app.updater() {
        Ok(u) => u.check().await.map_err(|e| match e {
            // Also what a missing latest.json (no release published yet) looks like.
            tauri_plugin_updater::Error::ReleaseNotFound => {
                "No release information was found on GitHub, or GitHub could not be reached".into()
            }
            e => e.to_string(),
        }),
        Err(e) => Err(e.to_string()),
    };
    let updates = app.state::<Updates>();
    let mut s = updates.0.lock();
    s.current_version = app.package_info().version.to_string();
    s.last_check_ms = Some(now_ms());
    match result {
        Ok(Some(u)) => {
            let fresh = s
                .available
                .as_ref()
                .map(|a| a.version != u.version)
                .unwrap_or(true);
            s.available = Some(UpdateInfo {
                version: u.version.clone(),
                notes: u.body.clone(),
                date: u.date.map(|d| d.to_string()),
            });
            s.last_error = None;
            if fresh {
                let _ = app.emit("update-available", &s.available); // the UI may not be listening
                if app
                    .state::<AppState>()
                    .settings
                    .read()
                    .notifications_enabled
                {
                    crate::notify::toast(
                        app,
                        "Update available",
                        &format!("PC Health Console {} is ready to install.", u.version),
                    );
                }
            }
        }
        Ok(None) => {
            s.available = None;
            s.last_error = None;
        }
        Err(e) => {
            tracing::info!("update check failed: {e}");
            s.last_error = Some(e);
        }
    }
    s.clone()
}

/// Downloads, verifies and installs the available update, then restarts.
pub async fn install(app: &AppHandle) -> Result<(), String> {
    let updates = app.state::<Updates>();
    {
        let mut s = updates.0.lock();
        if s.installing {
            return Err("An update is already being installed".into());
        }
        s.installing = true;
    }
    let result: Result<(), String> = async {
        let updater = app.updater().map_err(|e| e.to_string())?;
        let update = updater
            .check()
            .await
            .map_err(|e| e.to_string())?
            .ok_or("No update is available")?;
        tracing::info!("installing update {}", update.version);
        update
            .download_and_install(|_, _| {}, || {})
            .await
            .map_err(|e| format!("Update failed: {e}"))
    }
    .await;
    updates.0.lock().installing = false;
    result?;
    // The NSIS installer has replaced the files; restart into the new version.
    app.restart();
}

/// Background thread: first check shortly after start, then every 12 hours,
/// only while automatic checks are enabled. Exits promptly on shutdown.
pub fn run(app: AppHandle) {
    let state = app.state::<AppState>();
    let started = Instant::now();
    let mut last: Option<Instant> = None;
    while !state.shutdown.load(Ordering::Relaxed) {
        std::thread::sleep(Duration::from_secs(1));
        let due = match last {
            None => started.elapsed() >= FIRST_CHECK_DELAY,
            Some(t) => t.elapsed() >= CHECK_EVERY,
        };
        if due && state.settings.read().auto_update_check {
            last = Some(Instant::now());
            tauri::async_runtime::block_on(check(&app));
        } else if due {
            last = Some(Instant::now());
        }
    }
}
