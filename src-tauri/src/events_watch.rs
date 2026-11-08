//! Event-log ingestion thread: back-fills recent history from the Application
//! and System logs, then follows new events through push subscriptions.
//! Events are stored raw (deduplicated); new BSODs, unexpected restarts and
//! repeated crashes raise alerts once, when first stored.

use std::sync::atomic::Ordering;
use std::time::Duration;

use pch_core::events::{
    classify, xpath_for_channel, EventDetail, EventKind, RawEvent, Severity, CHANNELS,
};
use pch_core::{Availability, Status};
use pch_windows::eventlog::{self, EventLogError, Subscription};
use tauri::{AppHandle, Emitter, Manager};

use crate::state::{now_ms, AppState, ChannelStatus};
use crate::writer::WriteMsg;

/// Events read per channel when back-filling at startup (newest first).
const BACKFILL_MAX: usize = 5_000;
/// Only events newer than this raise notifications (older ones are history).
const ALERT_HORIZON_MS: i64 = 24 * 3_600_000;

fn status_for(e: &EventLogError) -> Status {
    match e {
        EventLogError::AccessDenied(c) => Status::new(
            Availability::PermissionRequired,
            format!("Reading the {c} log was denied"),
        ),
        EventLogError::NotFound(c) => Status::new(
            Availability::Unsupported,
            format!("The {c} log does not exist"),
        ),
        other => Status::error(other.to_string()),
    }
}

fn store(state: &AppState, events: Vec<RawEvent>) -> Vec<RawEvent> {
    if events.is_empty() {
        return Vec::new();
    }
    let (tx, rx) = std::sync::mpsc::channel();
    if state.writer.send(WriteMsg::Events(events, tx)).is_err() {
        return Vec::new();
    }
    rx.recv_timeout(Duration::from_secs(30)).unwrap_or_default()
}

fn alert_on_fresh(app: &AppHandle, state: &AppState, fresh: &[RawEvent]) {
    let settings = state.settings();
    let horizon = now_ms() - ALERT_HORIZON_MS;
    for raw in fresh.iter().filter(|e| e.time_ms >= horizon) {
        let Some(ev) = classify(raw) else { continue };
        match ev.kind {
            EventKind::Bugcheck if settings.event_alerts.new_bsod => crate::notify::event_alert(
                app,
                state,
                "event:bsod",
                "Blue screen recorded",
                &ev.title,
                Severity::Critical,
                ev.time_ms,
            ),
            EventKind::KernelPowerUnexpected if settings.event_alerts.unexpected_restart => {
                crate::notify::event_alert(
                    app,
                    state,
                    "event:unexpected-restart",
                    "Unexpected restart detected",
                    &ev.title,
                    Severity::Error,
                    ev.time_ms,
                )
            }
            EventKind::AppCrash if settings.event_alerts.repeated_crashes => {
                let Some(app_name) = ev.subject.clone() else {
                    continue;
                };
                let window = i64::from(settings.event_alerts.crash_window_min) * 60_000;
                let recent = state
                    .with_db(|db| db.events(ev.time_ms - window, ev.time_ms, 1000))
                    .unwrap_or_default()
                    .iter()
                    .filter_map(classify)
                    .filter(|e| {
                        e.kind == EventKind::AppCrash
                            && e.subject
                                .as_deref()
                                .is_some_and(|s| s.eq_ignore_ascii_case(&app_name))
                    })
                    .count();
                // Alert exactly when the threshold is reached, not on every later crash.
                if recent == settings.event_alerts.crash_count as usize {
                    let module = match &ev.detail {
                        EventDetail::AppCrash(d) => d
                            .module_name
                            .clone()
                            .map(|m| format!(" (latest faulting module: {m})"))
                            .unwrap_or_default(),
                        _ => String::new(),
                    };
                    crate::notify::event_alert(
                        app,
                        state,
                        "event:repeated-crash",
                        &format!("{app_name} keeps crashing"),
                        &format!(
                            "{recent} crashes in {} minutes{module}",
                            settings.event_alerts.crash_window_min
                        ),
                        Severity::Warning,
                        ev.time_ms,
                    );
                }
            }
            _ => {}
        }
    }
}

pub fn run(app: AppHandle) {
    let state = app.state::<AppState>();
    let mut subs: Vec<Subscription> = Vec::new();
    let mut statuses = Vec::new();
    for ch in CHANNELS {
        let xpath = xpath_for_channel(ch);
        // Subscribe first so nothing logged during the back-fill is missed (duplicates are ignored on insert).
        let sub = Subscription::new(ch, &xpath);
        let status = match eventlog::query(ch, &xpath, true, BACKFILL_MAX) {
            Ok((events, bad)) => {
                let total = events.len();
                let fresh = store(&state, events);
                alert_on_fresh(&app, &state, &fresh);
                if bad > 0 {
                    tracing::warn!("{bad} events in {ch} could not be parsed");
                }
                ChannelStatus {
                    channel: ch.into(),
                    status: Status::available(),
                    stored_events: total,
                }
            }
            Err(e) => ChannelStatus {
                channel: ch.into(),
                status: status_for(&e),
                stored_events: 0,
            },
        };
        match sub {
            Ok(s) => subs.push(s),
            Err(e) => tracing::warn!("subscription to {ch} failed: {e}"),
        }
        statuses.push(status);
    }
    *state.event_channels.write() = statuses;
    let _ = app.emit("stability-changed", ()); // the UI may not be listening yet

    while !state.shutdown.load(Ordering::Relaxed) {
        let mut any = false;
        for s in &mut subs {
            if s.wait(250) {
                let fresh = store(&state, s.take());
                if !fresh.is_empty() {
                    tracing::info!("{} new events from {}", fresh.len(), s.channel());
                    alert_on_fresh(&app, &state, &fresh);
                    any = true;
                }
            }
        }
        if any {
            let _ = app.emit("stability-changed", ());
        }
        if subs.is_empty() {
            std::thread::sleep(Duration::from_millis(500));
        }
    }
    tracing::info!("event watcher stopped");
}
