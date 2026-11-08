//! Rule-based diagnostic engine.
//!
//! Each rule looks for one measurable pattern in recorded evidence (Windows
//! events, stored telemetry, drive health, game sessions) and, when it finds
//! it, returns a [`Finding`] that lists every piece of evidence it used. Rules
//! never claim a root cause: titles state what was observed, explanations say
//! what the pattern is commonly associated with, and `caveat` says what the
//! evidence cannot show. All thresholds live in [`Thresholds`].

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::codes;
use crate::events::{
    group_shutdown_incidents, EventDetail, EventKind, IncidentKind, Severity, StabilityEvent,
};
use crate::model::{DriveHealth, VolumeSnapshot};

const DAY_MS: i64 = 24 * 60 * 60 * 1000;
const MAX_EVIDENCE: usize = 12;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Evidence {
    pub time_ms: Option<i64>,
    pub text: String,
    /// `channel:recordId` of a Windows event, when the evidence is one.
    pub event_ref: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    /// Stable id: rule plus subject, so the UI can track a finding across refreshes.
    pub id: String,
    pub rule: String,
    pub severity: Severity,
    pub title: String,
    pub explanation: String,
    pub caveat: Option<String>,
    pub suggestions: Vec<String>,
    pub evidence: Vec<Evidence>,
    pub evidence_total: usize,
    pub first_ms: Option<i64>,
    pub last_ms: Option<i64>,
}

/// One-minute (or coarser) aggregated points of a stored metric.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MetricSeries {
    pub key: String,
    pub label: String,
    pub bucket_ms: i64,
    /// `(bucket start ms, avg, max)`, ascending by time.
    pub points: Vec<(i64, f64, f64)>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionDigest {
    pub id: i64,
    pub process_name: String,
    pub started_ms: i64,
    pub frames: u64,
    pub stutter_time_pct: Option<f64>,
    pub low_1pct_fps: Option<f64>,
    pub avg_fps: Option<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Thresholds {
    pub crash_repeat_count: usize,
    pub crash_window_days: i64,
    pub module_window_days: i64,
    pub restart_count: usize,
    pub restart_window_days: i64,
    pub long_window_days: i64,
    pub whea_corrected_count: usize,
    pub gpu_reset_count: usize,
    pub service_failure_count: usize,
    pub memory_pressure_pct: f64,
    pub memory_pressure_minutes: usize,
    pub gpu_temp_limit_c: f64,
    pub cpu_temp_limit_c: f64,
    pub thermal_minutes: usize,
    pub disk_free_warn_pct: f64,
    pub disk_free_critical_pct: f64,
    pub nvme_wear_warn_pct: u8,
    pub stutter_pct: f64,
    pub stutter_sessions: usize,
    pub min_session_frames: u64,
}

impl Default for Thresholds {
    fn default() -> Self {
        Self {
            crash_repeat_count: 3,
            crash_window_days: 7,
            module_window_days: 14,
            restart_count: 2,
            restart_window_days: 7,
            long_window_days: 30,
            whea_corrected_count: 3,
            gpu_reset_count: 2,
            service_failure_count: 3,
            memory_pressure_pct: 90.0,
            memory_pressure_minutes: 10,
            gpu_temp_limit_c: 90.0,
            cpu_temp_limit_c: 95.0,
            thermal_minutes: 5,
            disk_free_warn_pct: 10.0,
            disk_free_critical_pct: 5.0,
            nvme_wear_warn_pct: 90,
            stutter_pct: 5.0,
            stutter_sessions: 3,
            min_session_frames: 1000,
        }
    }
}

pub struct DiagnosticInput<'a> {
    pub now_ms: i64,
    pub events: &'a [StabilityEvent],
    pub series: &'a [MetricSeries],
    pub drives: &'a [DriveHealth],
    pub volumes: &'a [VolumeSnapshot],
    pub sessions: &'a [SessionDigest],
}

/// Runs every rule. Findings are sorted most severe first, then newest first.
pub fn run(input: &DiagnosticInput<'_>, t: &Thresholds) -> Vec<Finding> {
    let mut out = Vec::new();
    repeated_app_crashes(input, t, &mut out);
    repeated_module_crashes(input, t, &mut out);
    unexpected_restarts(input, t, &mut out);
    bugchecks(input, t, &mut out);
    whea(input, t, &mut out);
    gpu_driver_resets(input, t, &mut out);
    storage_events(input, t, &mut out);
    drive_health(input, t, &mut out);
    service_failures(input, t, &mut out);
    memory_pressure(input, t, &mut out);
    thermal(input, t, &mut out);
    low_disk_space(input, t, &mut out);
    gaming_stutter(input, t, &mut out);
    out.sort_by(|a, b| b.severity.cmp(&a.severity).then(b.last_ms.cmp(&a.last_ms)));
    out
}

fn ev_ref(e: &StabilityEvent) -> Evidence {
    Evidence {
        time_ms: Some(e.time_ms),
        text: e.title.clone(),
        event_ref: Some(format!("{}:{}", e.channel, e.record_id)),
    }
}

fn since<'a>(
    input: &'a DiagnosticInput<'a>,
    days: i64,
    kind: EventKind,
) -> impl Iterator<Item = &'a StabilityEvent> + 'a {
    let from = input.now_ms - days * DAY_MS;
    input
        .events
        .iter()
        .filter(move |e| e.kind == kind && e.time_ms >= from)
}

#[allow(clippy::too_many_arguments)]
fn finding(
    rule: &str,
    subject: &str,
    severity: Severity,
    title: String,
    explanation: &str,
    caveat: Option<&str>,
    suggestions: &[&str],
    evidence: Vec<Evidence>,
) -> Finding {
    // Summary lines (no time) lead and are never truncated; only events are counted.
    let (mut evidence, mut events): (Vec<_>, Vec<_>) =
        evidence.into_iter().partition(|e| e.time_ms.is_none());
    events.sort_by_key(|e| std::cmp::Reverse(e.time_ms));
    let total = events.len();
    let first_ms = events.iter().filter_map(|e| e.time_ms).min();
    let last_ms = events.iter().filter_map(|e| e.time_ms).max();
    events.truncate(MAX_EVIDENCE);
    evidence.append(&mut events);
    Finding {
        id: format!("{rule}:{}", subject.to_ascii_lowercase()),
        rule: rule.to_owned(),
        severity,
        title,
        explanation: explanation.to_owned(),
        caveat: caveat.map(str::to_owned),
        suggestions: suggestions.iter().map(|s| (*s).to_owned()).collect(),
        evidence,
        evidence_total: total,
        first_ms,
        last_ms,
    }
}

fn group_by<'a>(
    events: impl Iterator<Item = &'a StabilityEvent>,
    key: impl Fn(&StabilityEvent) -> Option<String>,
) -> BTreeMap<String, (String, Vec<&'a StabilityEvent>)> {
    let mut m: BTreeMap<String, (String, Vec<&StabilityEvent>)> = BTreeMap::new();
    for e in events {
        if let Some(k) = key(e) {
            m.entry(k.to_ascii_lowercase())
                .or_insert_with(|| (k.clone(), Vec::new()))
                .1
                .push(e);
        }
    }
    m
}

fn repeated_app_crashes(input: &DiagnosticInput<'_>, t: &Thresholds, out: &mut Vec<Finding>) {
    let crashes = since(input, t.crash_window_days, EventKind::AppCrash).chain(since(
        input,
        t.crash_window_days,
        EventKind::AppHang,
    ));
    for (_, (app, evs)) in group_by(crashes, |e| e.subject.clone()) {
        if evs.len() < t.crash_repeat_count {
            continue;
        }
        let mut modules: BTreeMap<String, usize> = BTreeMap::new();
        for e in &evs {
            if let EventDetail::AppCrash(d) = &e.detail {
                if let Some(m) = &d.module_name {
                    *modules.entry(m.clone()).or_insert(0) += 1;
                }
            }
        }
        let mut evidence: Vec<Evidence> = evs.iter().map(|e| ev_ref(e)).collect();
        if !modules.is_empty() {
            let list: Vec<String> = modules.iter().map(|(m, n)| format!("{m} ×{n}")).collect();
            evidence.push(Evidence {
                time_ms: None,
                text: format!("Faulting modules: {}", list.join(", ")),
                event_ref: None,
            });
        }
        out.push(finding(
            "repeated-app-crash",
            &app,
            Severity::Warning,
            format!("{app} failed {} times in the last {} days", evs.len(), t.crash_window_days),
            "The same application crashed or stopped responding repeatedly. Recurring failures of one program usually point to that program, its plug-ins or data, or a component it depends on.",
            Some("Crash records show where the failure surfaced, not necessarily what caused it."),
            &["Check for updates to the application and its runtime libraries.", "Compare the crash times with the History view to see what the system was doing.", "Use the application's own repair or file verification option if it has one."],
            evidence,
        ));
    }
}

fn repeated_module_crashes(input: &DiagnosticInput<'_>, t: &Thresholds, out: &mut Vec<Finding>) {
    let crashes = since(input, t.module_window_days, EventKind::AppCrash);
    let modules = group_by(crashes, |e| match &e.detail {
        EventDetail::AppCrash(d) => d.module_name.clone(),
        _ => None,
    });
    for (_, (module, evs)) in modules {
        let apps: BTreeMap<String, usize> =
            evs.iter()
                .filter_map(|e| e.subject.as_deref())
                .fold(BTreeMap::new(), |mut m, a| {
                    *m.entry(a.to_ascii_lowercase()).or_insert(0) += 1;
                    m
                });
        // Only interesting when the module is not simply the crashing program itself.
        let is_app_itself = apps.len() == 1
            && apps
                .keys()
                .next()
                .is_some_and(|a| a.eq_ignore_ascii_case(&module));
        if evs.len() < t.crash_repeat_count || is_app_itself {
            continue;
        }
        let generic = codes::is_generic_system_module(&module);
        let mut evidence: Vec<Evidence> = evs.iter().map(|e| ev_ref(e)).collect();
        let list: Vec<String> = apps.iter().map(|(a, n)| format!("{a} ×{n}")).collect();
        evidence.push(Evidence {
            time_ms: None,
            text: format!("Affected applications: {}", list.join(", ")),
            event_ref: None,
        });
        let caveat = if generic {
            "This is a core Windows or runtime library. Faults are often reported here because it is where an error was detected or an exception raised; the defect is frequently in the calling program."
        } else {
            "The faulting module is where the exception occurred. Another component may have corrupted memory earlier."
        };
        out.push(finding(
            "repeated-module-crash",
            &module,
            if apps.len() > 1 { Severity::Warning } else { Severity::Info },
            format!("{} crashes faulted in {module} ({} application{})", evs.len(), apps.len(), if apps.len() == 1 { "" } else { "s" }),
            "Several crashes in the last two weeks reported the same faulting module. When it spans different applications, a shared component such as a driver user-mode library, overlay or injected tool is a common factor.",
            Some(caveat),
            &["Identify which product installed the module (its path is shown in each crash).", "Update or temporarily disable that product (overlays, recording tools, GPU driver) and see whether the crashes stop."],
            evidence,
        ));
    }
}

fn unexpected_restarts(input: &DiagnosticInput<'_>, t: &Thresholds, out: &mut Vec<Finding>) {
    let from = input.now_ms - t.restart_window_days * DAY_MS;
    let incidents: Vec<_> = group_shutdown_incidents(input.events)
        .into_iter()
        .filter(|i| i.time_ms >= from)
        .collect();
    if incidents.len() < t.restart_count {
        return;
    }
    let bugchecks = incidents
        .iter()
        .filter(|i| i.kind == IncidentKind::ConfirmedBugcheck)
        .count();
    let evidence = incidents
        .iter()
        .map(|i| Evidence {
            time_ms: Some(i.time_ms),
            text: match (&i.kind, &i.bugcheck) {
                (IncidentKind::ConfirmedBugcheck, Some(b)) => format!(
                    "Restart after stop code 0x{:08X}{}",
                    b.code.unwrap_or(0),
                    b.name
                        .as_deref()
                        .map(|n| format!(" {n}"))
                        .unwrap_or_default()
                ),
                _ if i.power_button_pressed => {
                    "Unexpected restart; the power button was used".into()
                }
                _ => "Unexpected restart without a recorded stop code".into(),
            },
            event_ref: i
                .evidence
                .first()
                .map(|r| format!("{}:{}", r.channel, r.record_id)),
        })
        .collect();
    out.push(finding(
        "unexpected-restarts",
        "system",
        Severity::Error,
        format!("{} unexpected restarts in the last {} days ({} with a stop code)", incidents.len(), t.restart_window_days, bugchecks),
        "Windows restarted without a clean shutdown more than once. Restarts with a stop code are blue screens; restarts without one are consistent with power loss, a forced power-off, a hard hang or a power-delivery problem.",
        Some("When no stop code is recorded, the logs cannot say why power was lost."),
        &["Open Blue Screen Analyzer for the stop codes and dump files.", "If no stop code was recorded, check power delivery (PSU, cables, power strip) and whether the PC was frozen beforehand.", "Look for WHEA hardware errors around the same times."],
        evidence,
    ));
}

fn bugchecks(input: &DiagnosticInput<'_>, t: &Thresholds, out: &mut Vec<Finding>) {
    let from = input.now_ms - t.long_window_days * DAY_MS;
    let incidents: Vec<_> = group_shutdown_incidents(input.events)
        .into_iter()
        .filter(|i| i.time_ms >= from && i.kind == IncidentKind::ConfirmedBugcheck)
        .collect();
    let mut by_code: BTreeMap<u32, Vec<_>> = BTreeMap::new();
    for i in &incidents {
        if let Some(code) = i.bugcheck.as_ref().and_then(|b| b.code) {
            by_code.entry(code).or_default().push(i);
        }
    }
    for (code, list) in by_code {
        let info = codes::bugcheck(code);
        let name = info.map(|i| i.name).unwrap_or("unknown stop code");
        let evidence = list
            .iter()
            .map(|i| Evidence {
                time_ms: Some(i.time_ms),
                text: format!(
                    "Stop code 0x{code:08X}{}",
                    i.bugcheck
                        .as_ref()
                        .and_then(|b| b.dump_path.as_deref())
                        .map(|p| format!(", dump {p}"))
                        .unwrap_or_default()
                ),
                event_ref: i
                    .evidence
                    .first()
                    .map(|r| format!("{}:{}", r.channel, r.record_id)),
            })
            .collect();
        let explanation = info.map(|i| i.description).unwrap_or("Windows stopped with this code. Its meaning is listed in Microsoft's bug check reference.");
        out.push(finding(
            "bugcheck",
            &format!("{code:x}"),
            if list.len() > 1 { Severity::Critical } else { Severity::Error },
            if list.len() > 1 {
                format!("Recurring blue screen 0x{code:08X} {name} ({} times in {} days)", list.len(), t.long_window_days)
            } else {
                format!("Blue screen 0x{code:08X} {name}")
            },
            explanation,
            Some("The stop code identifies the kind of failure, not the faulty component. Dump analysis can name the module that was executing."),
            &["Analyze the dump in Blue Screen Analyzer (requires Debugging Tools for Windows).", "Update the drivers named by the analysis, chipset and GPU drivers first."],
            evidence,
        ));
    }
}

fn whea(input: &DiagnosticInput<'_>, t: &Thresholds, out: &mut Vec<Finding>) {
    let errors: Vec<_> = since(input, t.long_window_days, EventKind::WheaError).collect();
    if !errors.is_empty() {
        out.push(finding(
            "whea-error",
            "uncorrected",
            Severity::Critical,
            format!("{} uncorrected hardware error{} reported in {} days", errors.len(), if errors.len() == 1 { "" } else { "s" }, t.long_window_days),
            "The Windows Hardware Error Architecture logged a hardware error that could not be corrected. These come from the CPU, memory, PCIe devices or the platform firmware.",
            Some("The event's error source and details identify the reporting component; the underlying cause can include unstable overclocks or undervolts."),
            &["Return CPU, memory (XMP/EXPO) and GPU settings to stock and check whether the errors stop.", "Run a memory test (Windows Memory Diagnostic or MemTest86).", "Update the motherboard BIOS."],
            errors.iter().map(|e| ev_ref(e)).collect(),
        ));
    }
    let corrected: Vec<_> = since(input, t.crash_window_days, EventKind::WheaCorrected).collect();
    if corrected.len() >= t.whea_corrected_count {
        out.push(finding(
            "whea-corrected",
            "corrected",
            Severity::Warning,
            format!("{} corrected hardware errors in {} days", corrected.len(), t.crash_window_days),
            "Hardware reported errors that it corrected. Occasional corrected errors are tolerated by design; a rising count can precede uncorrectable failures.",
            Some("Corrected PCIe errors are commonly caused by link power management or signal quality and may be harmless."),
            &["Note the error source in each event (PCIe device, memory, CPU cache).", "Check whether the count rises over time in Stability."],
            corrected.iter().map(|e| ev_ref(e)).collect(),
        ));
    }
}

fn gpu_driver_resets(input: &DiagnosticInput<'_>, t: &Thresholds, out: &mut Vec<Finding>) {
    let resets: Vec<_> = since(input, t.long_window_days, EventKind::GpuDriverReset).collect();
    if resets.len() >= t.gpu_reset_count {
        out.push(finding(
            "gpu-driver-reset",
            "gpu",
            Severity::Warning,
            format!("Graphics driver stopped responding {} times in {} days", resets.len(), t.long_window_days),
            "Windows reset the graphics driver after the GPU did not respond within the timeout (TDR). Repeated resets are associated with driver issues, unstable GPU clocks or voltages, overheating or power delivery.",
            None,
            &["Install the current GPU driver (a clean install if problems persist).", "Remove GPU overclocks or undervolts.", "Compare reset times with GPU temperature in History."],
            resets.iter().map(|e| ev_ref(e)).collect(),
        ));
    }
}

fn storage_events(input: &DiagnosticInput<'_>, t: &Thresholds, out: &mut Vec<Finding>) {
    let evs: Vec<_> = since(input, t.long_window_days, EventKind::StorageError).collect();
    if evs.is_empty() {
        return;
    }
    let plural = if evs.len() == 1 { "" } else { "s" };
    let mut f = finding(
        "storage-events",
        "storage",
        if evs.iter().any(|e| e.severity >= Severity::Error) { Severity::Error } else { Severity::Warning },
        format!("{} storage warning{plural} or error{plural} logged in {} days", evs.len(), t.long_window_days),
        "The disk, file system or storage controller driver logged problems such as bad blocks, controller errors, retried I/O or file system corruption.",
        Some("Some events (for example I/O retries on removable drives) can be caused by cables or USB enclosures rather than the drive itself."),
        &["Back up important data.", "Check the drive's health in Hardware → Storage.", "Run chkdsk on the affected volume."],
        evs.iter().map(|e| ev_ref(e)).collect(),
    );
    let mut by_source: BTreeMap<String, usize> = BTreeMap::new();
    for e in &evs {
        let subject = e
            .subject
            .as_deref()
            .map(|s| format!(" ({s})"))
            .unwrap_or_default();
        *by_source
            .entry(format!("{} {}{subject}", e.provider, e.event_id))
            .or_insert(0) += 1;
    }
    let summary: Vec<String> = by_source.iter().map(|(k, n)| format!("{k} ×{n}")).collect();
    f.evidence.insert(
        0,
        Evidence {
            time_ms: None,
            text: format!("By source: {}", summary.join(", ")),
            event_ref: None,
        },
    );
    out.push(f);
}

fn drive_health(input: &DiagnosticInput<'_>, t: &Thresholds, out: &mut Vec<Finding>) {
    for d in input.drives {
        let Some(n) = &d.nvme else { continue };
        let name = d
            .model
            .clone()
            .unwrap_or_else(|| format!("Disk {}", d.index));
        let flags = n.critical_warning_flags();
        if !flags.is_empty() {
            out.push(finding(
                "nvme-critical-warning",
                &name,
                Severity::Critical,
                format!("{name} reports a critical warning: {}", flags.join(", ")),
                "The drive itself set one or more Critical Warning bits in its SMART / Health log.",
                None,
                &[
                    "Back up the drive now.",
                    "Check the manufacturer's tool and warranty status.",
                ],
                vec![Evidence {
                    time_ms: None,
                    text: format!("Critical Warning = 0x{:02X}", n.critical_warning),
                    event_ref: None,
                }],
            ));
        }
        if n.percentage_used >= t.nvme_wear_warn_pct
            || n.available_spare_pct < n.available_spare_threshold_pct
        {
            out.push(finding(
                "nvme-wear",
                &name,
                Severity::Warning,
                format!("{name} endurance: {} % used, {} % spare remaining", n.percentage_used, n.available_spare_pct),
                "The drive's own estimate of consumed write endurance is high or its spare blocks are below the manufacturer threshold.",
                Some("Percentage Used is the vendor's estimate and can exceed 100 % while the drive still works."),
                &["Back up the drive and plan a replacement."],
                vec![Evidence { time_ms: None, text: format!("Percentage Used {} %, Available Spare {} % (threshold {} %)", n.percentage_used, n.available_spare_pct, n.available_spare_threshold_pct), event_ref: None }],
            ));
        }
        if n.media_errors > 0 {
            out.push(finding(
                "nvme-media-errors",
                &name,
                Severity::Warning,
                format!("{name} has recorded {} media/data integrity errors", n.media_errors),
                "The controller detected unrecovered data integrity errors over the drive's lifetime.",
                Some("This is a lifetime counter; check whether it increases."),
                &["Back up the drive.", "Re-check this value over the next days."],
                vec![Evidence { time_ms: None, text: format!("Media and Data Integrity Errors = {}", n.media_errors), event_ref: None }],
            ));
        }
    }
}

fn service_failures(input: &DiagnosticInput<'_>, t: &Thresholds, out: &mut Vec<Finding>) {
    let evs = since(input, t.crash_window_days, EventKind::ServiceFailure);
    for (_, (svc, list)) in group_by(evs, |e| e.subject.clone()) {
        if list.len() >= t.service_failure_count {
            out.push(finding(
                "service-failure",
                &svc,
                Severity::Info,
                format!("Service {svc} failed {} times in {} days", list.len(), t.crash_window_days),
                "The Service Control Manager recorded repeated failures of the same service.",
                None,
                &["Check whether the software that installed the service is up to date or still needed."],
                list.iter().map(|e| ev_ref(e)).collect(),
            ));
        }
    }
}

/// Longest run of consecutive buckets satisfying `pred`, with its start time.
fn longest_run(
    points: &[(i64, f64, f64)],
    bucket_ms: i64,
    pred: impl Fn(f64, f64) -> bool,
) -> (usize, Option<i64>) {
    let mut best = (0, None);
    let mut run = 0usize;
    let mut run_start = None;
    let mut prev_ts: Option<i64> = None;
    for &(ts, avg, max) in points {
        let contiguous = prev_ts.is_some_and(|p| ts - p <= bucket_ms);
        if pred(avg, max) {
            if !contiguous || run == 0 {
                run = 0;
                run_start = Some(ts);
            }
            run += 1;
            if run > best.0 {
                best = (run, run_start);
            }
        } else {
            run = 0;
        }
        prev_ts = Some(ts);
    }
    best
}

fn memory_pressure(input: &DiagnosticInput<'_>, t: &Thresholds, out: &mut Vec<Finding>) {
    let from = input.now_ms - DAY_MS;
    for s in input
        .series
        .iter()
        .filter(|s| s.key == "memory.load_pct" || s.key == "memory.commit_pct")
    {
        let pts: Vec<_> = s.points.iter().copied().filter(|p| p.0 >= from).collect();
        let (minutes, start) =
            longest_run(&pts, s.bucket_ms, |avg, _| avg >= t.memory_pressure_pct);
        let minutes = minutes * (s.bucket_ms / 60_000).max(1) as usize;
        if minutes >= t.memory_pressure_minutes {
            let commit = s.key == "memory.commit_pct";
            out.push(finding(
                "memory-pressure",
                &s.key,
                Severity::Warning,
                format!("{} stayed at or above {:.0} % for {minutes} minutes in the last 24 hours", s.label, t.memory_pressure_pct),
                if commit {
                    "Committed memory approached the commit limit (RAM plus page file). When it is reached, allocations fail and applications can crash."
                } else {
                    "Physical memory was almost fully used for a sustained period, which forces Windows to page and slows the system."
                },
                None,
                &["Open Processes and sort by memory to find the largest consumers.", "If this happens during normal use, consider closing background applications or adding RAM."],
                vec![Evidence { time_ms: start, text: format!("Longest continuous period: {minutes} min of {}-second averages ≥ {:.0} %", s.bucket_ms / 1000, t.memory_pressure_pct), event_ref: None }],
            ));
        }
    }
}

fn thermal(input: &DiagnosticInput<'_>, t: &Thresholds, out: &mut Vec<Finding>) {
    let from = input.now_ms - DAY_MS;
    for s in input.series {
        let limit = if s.key.starts_with("gpu.") && s.key.ends_with(".temp_c") {
            t.gpu_temp_limit_c
        } else if s.key == "cpu.package_temp_c" {
            t.cpu_temp_limit_c
        } else {
            continue;
        };
        let hot: Vec<_> = s
            .points
            .iter()
            .filter(|p| p.0 >= from && p.2 >= limit)
            .collect();
        let minutes = hot.len() * (s.bucket_ms / 60_000).max(1) as usize;
        if minutes < t.thermal_minutes {
            continue;
        }
        let peak = hot.iter().map(|p| p.2).fold(f64::MIN, f64::max);
        out.push(finding(
            "thermal-saturation",
            &s.key,
            Severity::Warning,
            format!("{} reached {limit:.0} °C or more during {minutes} minutes in the last 24 hours", s.label),
            "Sustained temperatures near the component's limit make it reduce clocks to protect itself, lowering performance. It is commonly associated with dust, fan curves, airflow or thermal paste.",
            Some("Safe limits differ between models; check the manufacturer's specification."),
            &["Check fan operation and case airflow.", "Compare with Gaming sessions to see whether performance dropped at the same time."],
            vec![Evidence { time_ms: hot.first().map(|p| p.0), text: format!("Peak {peak:.1} °C; {minutes} minute(s) at or above {limit:.0} °C"), event_ref: None }],
        ));
    }
}

fn low_disk_space(input: &DiagnosticInput<'_>, t: &Thresholds, out: &mut Vec<Finding>) {
    for v in input.volumes {
        let (Some(total), Some(free)) = (v.total_bytes, v.free_bytes) else {
            continue;
        };
        if total == 0 {
            continue;
        }
        let pct = free as f64 / total as f64 * 100.0;
        if pct >= t.disk_free_warn_pct {
            continue;
        }
        out.push(finding(
            "low-disk-space",
            &v.mount,
            if pct < t.disk_free_critical_pct { Severity::Error } else { Severity::Warning },
            format!("{} has {pct:.1} % free space", v.mount),
            "Very little free space slows updates and can make Windows Update, page file growth and crash dump writing fail.",
            None,
            &["Remove unneeded files or use Storage Sense.", "Move large game libraries to another drive."],
            vec![Evidence { time_ms: Some(input.now_ms), text: format!("{free} of {total} bytes free"), event_ref: None }],
        ));
    }
}

fn gaming_stutter(input: &DiagnosticInput<'_>, t: &Thresholds, out: &mut Vec<Finding>) {
    let from = input.now_ms - t.long_window_days * DAY_MS;
    let bad: Vec<_> = input
        .sessions
        .iter()
        .filter(|s| {
            s.started_ms >= from
                && s.frames >= t.min_session_frames
                && s.stutter_time_pct.is_some_and(|p| p > t.stutter_pct)
        })
        .collect();
    if bad.is_empty() {
        return;
    }
    let evidence = bad
        .iter()
        .map(|s| Evidence {
            time_ms: Some(s.started_ms),
            text: format!(
                "{}: {:.1} % of time in stutter frames, avg {:.0} FPS, 1 % low {:.0} FPS",
                s.process_name,
                s.stutter_time_pct.unwrap_or(0.0),
                s.avg_fps.unwrap_or(0.0),
                s.low_1pct_fps.unwrap_or(0.0)
            ),
            event_ref: Some(format!("session:{}", s.id)),
        })
        .collect();
    out.push(finding(
        "gaming-stutter",
        "sessions",
        if bad.len() >= t.stutter_sessions { Severity::Warning } else { Severity::Info },
        format!("{} recorded game session{} spent more than {:.0} % of their time in stutter frames", bad.len(), if bad.len() == 1 { "" } else { "s" }, t.stutter_pct),
        "Stutter frames are frames that took more than 2.5× the session's median frame time. Frequent spikes are associated with shader compilation, asset streaming, background activity, VRAM exhaustion or thermal throttling.",
        Some("Some games stutter by design during loading or scene changes."),
        &["Compare CPU, GPU and VRAM usage in the session details.", "Check background processes during play."],
        evidence,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{AppCrash, KernelPowerInfo};
    use crate::model::NvmeHealth;

    const NOW: i64 = 100 * DAY_MS;

    fn crash(app: &str, module: &str, t: i64, rec: u64) -> StabilityEvent {
        StabilityEvent {
            channel: "Application".into(),
            record_id: rec,
            time_ms: t,
            provider: "Application Error".into(),
            event_id: 1000,
            kind: EventKind::AppCrash,
            severity: Severity::Error,
            title: format!("{app} crashed"),
            subject: Some(app.into()),
            detail: EventDetail::AppCrash(AppCrash {
                app_name: Some(app.into()),
                app_version: None,
                app_path: None,
                module_name: Some(module.into()),
                module_version: None,
                module_path: None,
                exception_code: Some(0xC000_0005),
                exception_name: None,
                exception_description: None,
                fault_offset: None,
                process_id: None,
                report_id: None,
                package_full_name: None,
            }),
            data: vec![],
        }
    }

    fn kp41(t: i64, rec: u64) -> StabilityEvent {
        StabilityEvent {
            channel: "System".into(),
            record_id: rec,
            time_ms: t,
            provider: "Microsoft-Windows-Kernel-Power".into(),
            event_id: 41,
            kind: EventKind::KernelPowerUnexpected,
            severity: Severity::Critical,
            title: "Unexpected restart without a recorded stop code".into(),
            subject: None,
            detail: EventDetail::KernelPower(KernelPowerInfo {
                bugcheck: None,
                power_button_pressed: false,
                sleep_in_progress: false,
            }),
            data: vec![],
        }
    }

    fn input<'a>(
        events: &'a [StabilityEvent],
        series: &'a [MetricSeries],
        drives: &'a [DriveHealth],
        volumes: &'a [VolumeSnapshot],
        sessions: &'a [SessionDigest],
    ) -> DiagnosticInput<'a> {
        DiagnosticInput {
            now_ms: NOW,
            events,
            series,
            drives,
            volumes,
            sessions,
        }
    }

    #[test]
    fn nothing_recorded_means_no_findings() {
        assert!(run(&input(&[], &[], &[], &[], &[]), &Thresholds::default()).is_empty());
    }

    #[test]
    fn repeated_crashes_need_threshold_and_window() {
        let evs = vec![
            crash("Game.exe", "a.dll", NOW - 1000, 1),
            crash("Game.exe", "b.dll", NOW - 2000, 2),
        ];
        assert!(run(&input(&evs, &[], &[], &[], &[]), &Thresholds::default()).is_empty());
        let mut evs = evs;
        evs.push(crash("game.exe", "a.dll", NOW - 8 * DAY_MS, 3)); // outside 7 days
        assert!(
            run(&input(&evs, &[], &[], &[], &[]), &Thresholds::default())
                .iter()
                .all(|f| f.rule != "repeated-app-crash")
        );
        evs.push(crash("GAME.EXE", "a.dll", NOW - 3000, 4));
        let f = run(&input(&evs, &[], &[], &[], &[]), &Thresholds::default());
        let app = f
            .iter()
            .find(|f| f.rule == "repeated-app-crash")
            .expect("finding");
        assert_eq!(
            app.evidence_total, 3,
            "summary lines are not counted as events"
        );
        assert!(
            app.evidence[0].time_ms.is_none()
                && app.evidence[0].text.starts_with("Faulting modules"),
            "the module summary leads"
        );
        assert!(app.title.contains("3 times"));
    }

    #[test]
    fn module_across_apps_is_warning_with_generic_caveat() {
        let evs = vec![
            crash("A.exe", "ntdll.dll", NOW - 1, 1),
            crash("B.exe", "ntdll.dll", NOW - 2, 2),
            crash("C.exe", "NTDLL.dll", NOW - 3, 3),
        ];
        let f = run(&input(&evs, &[], &[], &[], &[]), &Thresholds::default());
        let m = f
            .iter()
            .find(|f| f.rule == "repeated-module-crash")
            .expect("finding");
        assert_eq!(m.severity, Severity::Warning);
        assert!(m
            .caveat
            .as_deref()
            .is_some_and(|c| c.contains("core Windows")));
    }

    #[test]
    fn module_equal_to_app_is_not_reported_as_module_finding() {
        let evs: Vec<_> = (0..3)
            .map(|i| crash("tool.exe", "tool.exe", NOW - i, i as u64))
            .collect();
        let f = run(&input(&evs, &[], &[], &[], &[]), &Thresholds::default());
        assert!(f.iter().all(|f| f.rule != "repeated-module-crash"));
    }

    #[test]
    fn restarts_counted_as_incidents() {
        let evs = vec![kp41(NOW - DAY_MS, 1), kp41(NOW - 2 * DAY_MS, 2)];
        let f = run(&input(&evs, &[], &[], &[], &[]), &Thresholds::default());
        let r = f
            .iter()
            .find(|f| f.rule == "unexpected-restarts")
            .expect("finding");
        assert!(r.title.starts_with("2 unexpected restarts"));
        assert!(
            f.iter().all(|f| f.rule != "bugcheck"),
            "no stop code, no bugcheck finding"
        );
    }

    #[test]
    fn memory_pressure_needs_contiguous_minutes() {
        let mk = |n: usize, gap_at: Option<usize>| MetricSeries {
            key: "memory.load_pct".into(),
            label: "Memory load".into(),
            bucket_ms: 60_000,
            points: (0..n)
                .map(|i| {
                    let skip = gap_at.is_some_and(|g| i >= g) as i64 * 5 * 60_000;
                    (NOW - DAY_MS / 2 + i as i64 * 60_000 + skip, 95.0, 97.0)
                })
                .collect(),
        };
        let s = [mk(10, None)];
        assert_eq!(
            run(&input(&[], &s, &[], &[], &[]), &Thresholds::default()).len(),
            1
        );
        let s = [mk(10, Some(5))];
        assert!(
            run(&input(&[], &s, &[], &[], &[]), &Thresholds::default()).is_empty(),
            "a gap in data breaks the run"
        );
    }

    #[test]
    fn thermal_uses_max_of_bucket() {
        let s = [MetricSeries {
            key: "gpu.0.temp_c".into(),
            label: "GPU temperature".into(),
            bucket_ms: 60_000,
            points: (0..5)
                .map(|i| (NOW - 1000 - i * 60_000, 80.0, 91.0))
                .collect(),
        }];
        let f = run(&input(&[], &s, &[], &[], &[]), &Thresholds::default());
        assert_eq!(f[0].rule, "thermal-saturation");
    }

    #[test]
    fn drive_health_flags() {
        let d = [DriveHealth {
            model: Some("NVMe X".into()),
            nvme: Some(NvmeHealth {
                critical_warning: 1,
                percentage_used: 95,
                available_spare_pct: 100,
                available_spare_threshold_pct: 10,
                media_errors: 2,
                ..Default::default()
            }),
            ..Default::default()
        }];
        let f = run(&input(&[], &[], &d, &[], &[]), &Thresholds::default());
        let rules: Vec<_> = f.iter().map(|f| f.rule.as_str()).collect();
        assert_eq!(rules[0], "nvme-critical-warning");
        assert!(rules.contains(&"nvme-wear") && rules.contains(&"nvme-media-errors"));
    }

    #[test]
    fn low_disk_space_levels() {
        let v = [
            VolumeSnapshot {
                mount: "C:\\".into(),
                total_bytes: Some(100),
                free_bytes: Some(4),
                ..Default::default()
            },
            VolumeSnapshot {
                mount: "D:\\".into(),
                total_bytes: Some(100),
                free_bytes: Some(50),
                ..Default::default()
            },
            VolumeSnapshot {
                mount: "E:\\".into(),
                total_bytes: Some(0),
                free_bytes: Some(0),
                ..Default::default()
            },
        ];
        let f = run(&input(&[], &[], &[], &v, &[]), &Thresholds::default());
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].severity, Severity::Error);
    }

    #[test]
    fn stutter_sessions() {
        let s = [SessionDigest {
            id: 1,
            process_name: "g.exe".into(),
            started_ms: NOW - 1,
            frames: 5000,
            stutter_time_pct: Some(9.0),
            low_1pct_fps: Some(30.0),
            avg_fps: Some(90.0),
        }];
        let f = run(&input(&[], &[], &[], &[], &s), &Thresholds::default());
        assert_eq!(f[0].severity, Severity::Info);
        assert_eq!(f[0].evidence[0].event_ref.as_deref(), Some("session:1"));
    }
}
