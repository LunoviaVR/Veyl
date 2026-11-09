//! Signed auto-update from GitHub Releases (tauri-plugin-updater).
//!
//! The endpoint and public key are in `tauri.conf.json → plugins.updater`. The
//! release workflow publishes `latest.json` plus installers signed with the
//! matching private key; an update whose signature does not verify is refused by
//! the plugin. Checks run only when "Check for updates automatically" is on, or
//! when the user presses "Check now". Installing is always a user action.
//!
//! The updater is one state machine ([`UpdateState`]); every change is emitted as
//! `update-status`, so the banner and Settings always show the same thing.

use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::UpdaterExt;

use crate::state::{now_ms, AppState};

const FIRST_CHECK_DELAY: Duration = Duration::from_secs(20);
const CHECK_EVERY: Duration = Duration::from_secs(12 * 3600);

#[derive(Serialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub version: String,
    pub notes: Option<String>,
    pub date: Option<String>,
}

/// ```text
/// NotChecked ─check→ Checking ─→ UpToDate | Available | Failed
/// Available ─install→ Installing ─→ (restart) | Failed{update}
/// Failed{update: Some} can be installed again; any state but Installing can be re-checked.
/// ```
#[derive(Serialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase", tag = "state")]
pub enum UpdateState {
    #[default]
    NotChecked,
    Checking,
    UpToDate,
    Available {
        update: UpdateInfo,
    },
    Installing {
        update: UpdateInfo,
    },
    /// A check or install failed. A known update is kept so it can still be installed.
    Failed {
        error: String,
        update: Option<UpdateInfo>,
    },
}

impl UpdateState {
    /// The update this state knows about, if any.
    pub fn update(&self) -> Option<&UpdateInfo> {
        match self {
            Self::Available { update } | Self::Installing { update } => Some(update),
            Self::Failed { update, .. } => update.as_ref(),
            _ => None,
        }
    }
}

#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    pub current_version: String,
    pub last_check_ms: Option<i64>,
    #[serde(flatten)]
    pub state: UpdateState,
}

/// Shared update state (managed by Tauri).
#[derive(Default)]
pub struct Updates(pub Mutex<UpdateStatus>);

/// Sets the state and tells every view.
fn set(app: &AppHandle, f: impl FnOnce(&mut UpdateStatus)) -> UpdateStatus {
    let updates = app.state::<Updates>();
    let snapshot = {
        let mut s = updates.0.lock();
        f(&mut s);
        s.current_version = app.package_info().version.to_string();
        s.clone()
    };
    let _ = app.emit("update-status", &snapshot); // the UI may not be listening
    snapshot
}

/// Checks the release endpoint once and records the result. Skipped while installing.
pub async fn check(app: &AppHandle) -> UpdateStatus {
    let mut known = None;
    let mut busy = false;
    set(app, |s| match &s.state {
        UpdateState::Installing { .. } | UpdateState::Checking => busy = true,
        other => {
            known = other.update().cloned();
            s.state = UpdateState::Checking;
        }
    });
    if busy {
        return app.state::<Updates>().0.lock().clone();
    }
    let result = match app.updater() {
        Ok(u) => u.check().await.map_err(|e| match e {
            // Any non-success status: no release yet, offline, or GitHub still serving the old
            // "latest" release (it errors for a few minutes after each publish).
            tauri_plugin_updater::Error::ReleaseNotFound => {
                "GitHub did not return the update information. Right after a new release is published this can take a few minutes, so try again shortly; otherwise check the internet connection.".into()
            }
            e => e.to_string(),
        }),
        Err(e) => Err(e.to_string()),
    };
    let next = match result {
        Ok(Some(u)) => UpdateState::Available {
            update: UpdateInfo {
                version: u.version.clone(),
                notes: u.body.clone(),
                date: u.date.map(|d| d.to_string()),
            },
        },
        Ok(None) => UpdateState::UpToDate,
        Err(error) => {
            tracing::info!("update check failed: {error}");
            UpdateState::Failed {
                error,
                update: known.clone(),
            }
        }
    };
    let fresh = matches!(&next, UpdateState::Available { update } if known.as_ref().map(|k| &k.version) != Some(&update.version));
    let status = set(app, |s| {
        s.last_check_ms = Some(now_ms());
        s.state = next;
    });
    if fresh
        && app
            .state::<AppState>()
            .settings
            .read()
            .notifications_enabled
    {
        if let Some(u) = status.state.update() {
            crate::notify::toast(
                app,
                "Update available",
                &format!("Veyl {} is ready to install.", u.version),
            );
        }
    }
    status
}

/// Downloads, verifies and installs the known update, then restarts.
pub async fn install(app: &AppHandle) -> Result<(), String> {
    let mut chosen = None;
    let mut refusal = None;
    set(app, |s| match &s.state {
        UpdateState::Installing { .. } => refusal = Some("An update is already being installed"),
        other => match other.update().cloned() {
            Some(update) => {
                chosen = Some(update.clone());
                s.state = UpdateState::Installing { update };
            }
            None => refusal = Some("No update is available"),
        },
    });
    if let Some(r) = refusal {
        return Err(r.into());
    }
    let update_info = chosen.unwrap_or_default();
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
    if let Err(error) = result {
        tracing::warn!("update install failed: {error}");
        set(app, |s| {
            s.state = UpdateState::Failed {
                error: error.clone(),
                update: Some(update_info),
            }
        });
        return Err(error);
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn info(v: &str) -> UpdateInfo {
        UpdateInfo {
            version: v.into(),
            ..Default::default()
        }
    }

    #[test]
    fn known_update_survives_failures_and_serializes_flat() {
        assert_eq!(
            UpdateState::Available {
                update: info("0.2.0")
            }
            .update(),
            Some(&info("0.2.0"))
        );
        assert_eq!(
            UpdateState::Failed {
                error: "offline".into(),
                update: Some(info("0.2.0"))
            }
            .update(),
            Some(&info("0.2.0"))
        );
        assert_eq!(UpdateState::UpToDate.update(), None);
        let json = serde_json::to_value(UpdateStatus {
            current_version: "0.1.0".into(),
            last_check_ms: Some(5),
            state: UpdateState::Installing {
                update: info("0.2.0"),
            },
        })
        .expect("serialize");
        assert_eq!(json["state"], "installing");
        assert_eq!(json["update"]["version"], "0.2.0");
        assert_eq!(json["currentVersion"], "0.1.0");
    }
}
