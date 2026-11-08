use std::path::PathBuf;

use pch_core::diagnostics::{
    self, DiagnosticInput, Finding, MetricSeries, SessionDigest, Thresholds,
};
use pch_core::events::{
    classify, group_shutdown_incidents, EventDetail, ShutdownIncident, StabilityEvent,
};
use pch_storage::Series;
use pch_windows::debugger::{self, DumpAnalysis, DumpListing};
use pch_windows::version_info::{file_version_info, FileVersionInfo};
use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use super::{blocking, check_range, CmdResult};
use crate::state::{now_ms, AppState, ChannelStatus};

const DAY_MS: i64 = 24 * 3_600_000;

#[tauri::command(async)]
pub fn get_stability_events(
    state: State<'_, AppState>,
    from_ms: i64,
    to_ms: i64,
    limit: Option<u32>,
) -> CmdResult<Vec<StabilityEvent>> {
    check_range(from_ms, to_ms)?;
    let limit = limit.unwrap_or(5000).clamp(1, 20_000);
    let raw = state.with_db(|db| db.events(from_ms, to_ms, limit))?;
    Ok(raw.iter().filter_map(classify).collect())
}

#[tauri::command]
pub fn get_event_channels(state: State<'_, AppState>) -> Vec<ChannelStatus> {
    state.event_channels.read().clone()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TelemetryContext {
    pub from_ms: i64,
    pub to_ms: i64,
    pub series: Vec<Series>,
    /// True when no telemetry was recorded in the window (the app was not running, or history was off).
    pub empty: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EventDetailView {
    pub event: StabilityEvent,
    pub message: Option<String>,
    pub related: Vec<StabilityEvent>,
    pub linked_report: Option<StabilityEvent>,
    pub module_file: Option<FileVersionInfo>,
    pub app_file: Option<FileVersionInfo>,
    pub same_app_count_30d: usize,
    pub same_module_count_30d: usize,
    pub context: TelemetryContext,
}

pub const CONTEXT_KEYS: [&str; 7] = [
    "cpu.total_pct",
    "memory.load_pct",
    "gpu.0.util_pct",
    "gpu.0.temp_c",
    "cpu.package_temp_c",
    "gpu.0.vram_used_bytes",
    "memory.commit_pct",
];

pub fn context(state: &AppState, center_ms: i64, half_window_ms: i64) -> TelemetryContext {
    let (from, to) = (center_ms - half_window_ms, center_ms + half_window_ms);
    let raw_hours = state.settings.read().retention.raw_hours;
    let keys: Vec<String> = CONTEXT_KEYS.iter().map(|s| (*s).to_owned()).collect();
    let series = state
        .with_db(|db| db.series(&keys, from, to, 300, now_ms(), raw_hours))
        .unwrap_or_default();
    let series: Vec<Series> = series.into_iter().filter(|s| !s.t.is_empty()).collect();
    TelemetryContext {
        from_ms: from,
        to_ms: to,
        empty: series.is_empty(),
        series,
    }
}

#[tauri::command(async)]
pub fn get_event_detail(
    state: State<'_, AppState>,
    channel: String,
    record_id: u64,
) -> CmdResult<EventDetailView> {
    if !pch_core::events::CHANNELS.contains(&channel.as_str()) {
        return Err("Unknown channel".into());
    }
    let raw = state
        .with_db(|db| db.event(&channel, record_id))?
        .ok_or("Event not found")?;
    let event = classify(&raw).ok_or("Event is not a recognized stability event")?;
    let window = 10 * 60_000;
    let nearby: Vec<StabilityEvent> = state
        .with_db(|db| db.events(raw.time_ms - window, raw.time_ms + window, 500))?
        .iter()
        .filter_map(classify)
        .filter(|e| !(e.channel == event.channel && e.record_id == event.record_id))
        .collect();
    let month: Vec<StabilityEvent> = state
        .with_db(|db| db.events(raw.time_ms - 30 * DAY_MS, raw.time_ms + DAY_MS, 20_000))?
        .iter()
        .filter_map(classify)
        .collect();

    let (mut linked_report, mut module_file, mut app_file) = (None, None, None);
    let (mut same_app, mut same_module) = (0, 0);
    if let EventDetail::AppCrash(d) = &event.detail {
        // WER links its report to the crash through the report id; only an exact match is shown.
        linked_report = d.report_id.as_ref().and_then(|id| {
            nearby.iter().find(|e| matches!(&e.detail, EventDetail::WerReport(w) if w.report_id.as_deref() == Some(id.as_str()))).cloned()
        });
        module_file = d.module_path.as_deref().and_then(file_version_info);
        app_file = d.app_path.as_deref().and_then(file_version_info);
        for e in &month {
            if let EventDetail::AppCrash(o) = &e.detail {
                if o.app_name.as_deref().map(str::to_ascii_lowercase)
                    == d.app_name.as_deref().map(str::to_ascii_lowercase)
                {
                    same_app += 1;
                }
                if o.module_name.is_some()
                    && o.module_name.as_deref().map(str::to_ascii_lowercase)
                        == d.module_name.as_deref().map(str::to_ascii_lowercase)
                {
                    same_module += 1;
                }
            }
        }
    }
    let message =
        pch_windows::eventlog::format_message(&event.channel, &event.provider, event.record_id);
    Ok(EventDetailView {
        context: context(&state, event.time_ms, 15 * 60_000),
        event,
        message,
        related: nearby,
        linked_report,
        module_file,
        app_file,
        same_app_count_30d: same_app,
        same_module_count_30d: same_module,
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BsodOverview {
    pub incidents: Vec<ShutdownIncident>,
    pub dumps: DumpListing,
    pub debugger_path: Option<String>,
    pub symbol_server_enabled: bool,
    pub elevated: bool,
}

#[tauri::command(async)]
pub fn get_bsod_overview(state: State<'_, AppState>) -> CmdResult<BsodOverview> {
    let now = now_ms();
    let events: Vec<StabilityEvent> = state
        .with_db(|db| db.events(0, now + DAY_MS, 20_000))?
        .iter()
        .filter_map(classify)
        .collect();
    let settings = state.settings();
    Ok(BsodOverview {
        incidents: group_shutdown_incidents(&events),
        dumps: debugger::list_dumps(),
        debugger_path: debugger::find_cdb(settings.debugger_path.as_deref())
            .map(|p| p.display().to_string()),
        symbol_server_enabled: settings.symbol_server_enabled,
        elevated: state.system_info.read().elevated,
    })
}

/// Analyzes a kernel dump with cdb. The path must be one of the dumps Windows
/// wrote (as listed by `get_bsod_overview`), never an arbitrary file.
#[tauri::command]
pub async fn analyze_dump(app: AppHandle, path: String) -> CmdResult<DumpAnalysis> {
    let listed = debugger::list_dumps();
    if !listed
        .files
        .iter()
        .any(|f| f.path.eq_ignore_ascii_case(&path))
    {
        return Err("Only dump files found in the Windows dump locations can be analyzed".into());
    }
    let state = app.state::<AppState>();
    let settings = state.settings();
    let cdb = debugger::find_cdb(settings.debugger_path.as_deref())
        .ok_or("Debugging Tools for Windows (cdb.exe) were not found. Install them from the Windows SDK, or select cdb.exe in Settings.")?;
    let cache = state.paths.symbols.clone();
    let symbols = settings.symbol_server_enabled;
    tracing::info!("analyzing dump {path} (symbol server: {symbols})");
    blocking(move || debugger::analyze(&cdb, &PathBuf::from(path), &cache, symbols)).await?
}

#[tauri::command(async)]
pub fn get_diagnostics(state: State<'_, AppState>) -> CmdResult<Vec<Finding>> {
    let now = now_ms();
    let events: Vec<StabilityEvent> = state
        .with_db(|db| db.events(now - 31 * DAY_MS, now + DAY_MS, 20_000))?
        .iter()
        .filter_map(classify)
        .collect();
    let mut series = Vec::new();
    for like in ["memory.%", "gpu.%.temp_c", "cpu.package_temp_c"] {
        for (key, points) in state.with_db(|db| db.minute_points(like, now - DAY_MS))? {
            let label = match key.as_str() {
                "memory.load_pct" => "Memory load".to_owned(),
                "memory.commit_pct" => "Committed memory".to_owned(),
                "cpu.package_temp_c" => "CPU package temperature".to_owned(),
                k if k.starts_with("gpu.") => {
                    let idx: usize = k
                        .split('.')
                        .nth(1)
                        .and_then(|i| i.parse().ok())
                        .unwrap_or(0);
                    let name = state
                        .system_info
                        .read()
                        .gpus
                        .get(idx)
                        .map(|g| g.name.clone())
                        .unwrap_or_else(|| format!("GPU {idx}"));
                    format!("{name} temperature")
                }
                _ => continue,
            };
            series.push(MetricSeries {
                key,
                label,
                bucket_ms: pch_storage::MINUTE_MS,
                points,
            });
        }
    }
    let sessions: Vec<SessionDigest> = state
        .with_db(|db| db.sessions(200))?
        .into_iter()
        .map(|s| SessionDigest {
            id: s.id,
            process_name: s.process_name,
            started_ms: s.started_ms,
            frames: s.stats["frames"].as_u64().unwrap_or(0),
            stutter_time_pct: s.stats["stutterTimePct"].as_f64(),
            low_1pct_fps: s.stats["low1pctFps"].as_f64(),
            avg_fps: s.stats["avgFps"].as_f64(),
        })
        .collect();
    let live = state.live.read();
    let volumes = live
        .frame
        .as_ref()
        .map(|f| f.disks.volumes.clone())
        .unwrap_or_default();
    let drives = live.drives.clone();
    drop(live);
    let input = DiagnosticInput {
        now_ms: now,
        events: &events,
        series: &series,
        drives: &drives,
        volumes: &volumes,
        sessions: &sessions,
    };
    Ok(diagnostics::run(&input, &Thresholds::default()))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelineView {
    pub events: Vec<StabilityEvent>,
    pub context: TelemetryContext,
}

#[tauri::command(async)]
pub fn get_timeline(
    state: State<'_, AppState>,
    center_ms: i64,
    half_window_ms: i64,
) -> CmdResult<TimelineView> {
    let half = half_window_ms.clamp(60_000, 7 * DAY_MS);
    check_range(center_ms - half, center_ms + half)?;
    let events = state
        .with_db(|db| db.events(center_ms - half, center_ms + half, 2000))?
        .iter()
        .filter_map(classify)
        .collect();
    Ok(TimelineView {
        events,
        context: context(&state, center_ms, half),
    })
}
