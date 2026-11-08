//! Alert recording and Windows notifications.

use tauri::{AppHandle, Emitter};
use tauri_plugin_notification::NotificationExt;
use veyl_core::alerts::{AlertSignal, AlertTransition};
use veyl_core::events::Severity;
use veyl_storage::AlertRow;

use crate::state::AppState;
use crate::writer::{self, WriteMsg};

fn severity_name(s: Severity) -> &'static str {
    match s {
        Severity::Info => "info",
        Severity::Warning => "warning",
        Severity::Error => "error",
        Severity::Critical => "critical",
    }
}

pub fn toast(app: &AppHandle, title: &str, body: &str) {
    if let Err(e) = app.notification().builder().title(title).body(body).show() {
        tracing::warn!("notification failed: {e}");
    }
}

/// Stores a threshold alert transition, tells the UI, and notifies if enabled.
pub fn record_alert(app: &AppHandle, state: &AppState, s: &AlertSignal, notifications: bool) {
    let row = AlertRow {
        id: 0,
        time_ms: s.time_ms,
        rule_id: s.rule_id.clone(),
        rule_name: s.rule_name.clone(),
        metric: s.metric.clone(),
        severity: severity_name(s.severity).into(),
        transition: match s.transition {
            AlertTransition::Fired => "fired".into(),
            AlertTransition::Resolved => "resolved".into(),
        },
        value: s.value,
        threshold: s.threshold,
        since_ms: s.since_ms,
    };
    writer::send(&state.writer, WriteMsg::Alert(row));
    let _ = app.emit("alert", s); // the UI may not be listening
    if notifications && s.notify && s.transition == AlertTransition::Fired {
        toast(
            app,
            &s.rule_name,
            &format!(
                "{} = {:.1} (threshold {:.1})",
                s.metric, s.value, s.threshold
            ),
        );
    }
}

/// Records an alert raised by a Windows event (BSOD, unexpected restart, repeated crashes).
pub fn event_alert(
    app: &AppHandle,
    state: &AppState,
    rule_id: &str,
    title: &str,
    body: &str,
    severity: Severity,
    time_ms: i64,
) {
    let row = AlertRow {
        id: 0,
        time_ms,
        rule_id: rule_id.into(),
        rule_name: title.into(),
        metric: body.into(),
        severity: severity_name(severity).into(),
        transition: "fired".into(),
        value: 0.0,
        threshold: 0.0,
        since_ms: time_ms,
    };
    writer::send(&state.writer, WriteMsg::Alert(row));
    let _ = app.emit("event-alert", (rule_id, title, body));
    if state.settings.read().notifications_enabled {
        toast(app, title, body);
    }
}
