//! Windows event interpretation: XML parsing, provider+ID validated
//! classification, and grouping of unexpected-shutdown evidence into incidents.
//!
//! The table [`RULES`] is the single source of truth for which events the
//! application reads: the event-log reader builds its XPath queries from it and
//! [`classify`] accepts only events whose channel, provider *and* ID match it.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::codes;
use crate::time::parse_iso8601_utc_ms;

/// One event as read from the log, before interpretation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawEvent {
    pub channel: String,
    pub provider: String,
    pub event_id: u32,
    pub level: u8,
    pub record_id: u64,
    pub time_ms: i64,
    /// `EventData`/`UserData` fields in document order. Unnamed `<Data>` elements are named `#0`, `#1`, ...
    pub data: Vec<(String, String)>,
}

impl RawEvent {
    pub fn field(&self, name: &str) -> Option<&str> {
        self.data
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }
    /// A field that is present and not empty / "NULL".
    fn value(&self, name: &str) -> Option<String> {
        self.field(name)
            .map(str::trim)
            .filter(|v| !v.is_empty() && *v != "NULL")
            .map(str::to_owned)
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum EventParseError {
    #[error("event XML is malformed: {0}")]
    Xml(String),
    #[error("event XML lacks {0}")]
    Missing(&'static str),
}

/// Parses the XML rendered by `EvtRender(EvtRenderEventXml)`.
pub fn parse_event_xml(xml: &str) -> Result<RawEvent, EventParseError> {
    let doc = roxmltree::Document::parse(xml).map_err(|e| EventParseError::Xml(e.to_string()))?;
    let root = doc.root_element();
    let system = child(root, "System").ok_or(EventParseError::Missing("System"))?;
    let provider = child(system, "Provider")
        .and_then(|p| p.attribute("Name"))
        .ok_or(EventParseError::Missing("Provider/@Name"))?;
    let event_id = child(system, "EventID")
        .and_then(|e| e.text())
        .and_then(|t| t.trim().parse::<u32>().ok())
        .ok_or(EventParseError::Missing("EventID"))?;
    let level = child(system, "Level")
        .and_then(|e| e.text())
        .and_then(|t| t.trim().parse().ok())
        .unwrap_or(0);
    let record_id = child(system, "EventRecordID")
        .and_then(|e| e.text())
        .and_then(|t| t.trim().parse().ok())
        .ok_or(EventParseError::Missing("EventRecordID"))?;
    let time_ms = child(system, "TimeCreated")
        .and_then(|e| e.attribute("SystemTime"))
        .and_then(parse_iso8601_utc_ms)
        .ok_or(EventParseError::Missing("TimeCreated/@SystemTime"))?;
    let channel = child(system, "Channel")
        .and_then(|e| e.text())
        .unwrap_or_default()
        .to_owned();

    let mut data = Vec::new();
    if let Some(ed) = child(root, "EventData") {
        for (i, d) in ed
            .children()
            .filter(|n| n.is_element() && n.tag_name().name() == "Data")
            .enumerate()
        {
            let name = d
                .attribute("Name")
                .map(str::to_owned)
                .unwrap_or_else(|| format!("#{i}"));
            data.push((name, d.text().unwrap_or_default().to_owned()));
        }
    } else if let Some(ud) = child(root, "UserData") {
        // UserData wraps one provider-defined element whose children are the fields.
        for wrapper in ud.children().filter(|n| n.is_element()) {
            for f in wrapper.children().filter(|n| n.is_element()) {
                data.push((
                    f.tag_name().name().to_owned(),
                    f.text().unwrap_or_default().to_owned(),
                ));
            }
        }
    }
    Ok(RawEvent {
        channel,
        provider: provider.to_owned(),
        event_id,
        level,
        record_id,
        time_ms,
        data,
    })
}

fn child<'a, 'i>(node: roxmltree::Node<'a, 'i>, name: &str) -> Option<roxmltree::Node<'a, 'i>> {
    node.children()
        .find(|n| n.is_element() && n.tag_name().name() == name)
}

// ---------------------------------------------------------------------------
// Classification
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub enum EventKind {
    AppCrash,
    AppHang,
    WerReport,
    Bugcheck,
    KernelPowerUnexpected,
    UnexpectedShutdown,
    WheaError,
    WheaCorrected,
    GpuDriverReset,
    StorageError,
    ServiceFailure,
    SystemStart,
    SystemShutdown,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub enum Severity {
    Info,
    Warning,
    Error,
    Critical,
}

impl Severity {
    /// Windows event level: 1 critical, 2 error, 3 warning, 4/0/5 informational.
    pub fn from_level(level: u8) -> Self {
        match level {
            1 => Severity::Critical,
            2 => Severity::Error,
            3 => Severity::Warning,
            _ => Severity::Info,
        }
    }
}

struct Rule {
    channel: &'static str,
    provider: &'static str,
    /// Empty = every event ID of the provider.
    ids: &'static [u32],
    kind: EventKind,
}

impl Rule {
    /// Storage drivers log healthy-volume notices under the same IDs (Ntfs 98 at
    /// Information level: "Volume ... is healthy. No action is needed."), so only
    /// warning, error and critical levels count as storage problems.
    fn problems_only(&self) -> bool {
        self.kind == EventKind::StorageError
    }
}

/// Provider names and IDs as they appear in the `System/Provider/@Name` and
/// `System/EventID` elements on Windows 10/11 (verified with `wevtutil qe` on a
/// Windows 11 26300 system for the application/WER/Kernel-Power/EventLog entries).
static RULES: &[Rule] = &[
    Rule {
        channel: "Application",
        provider: "Application Error",
        ids: &[1000],
        kind: EventKind::AppCrash,
    },
    Rule {
        channel: "Application",
        provider: "Application Hang",
        ids: &[1002],
        kind: EventKind::AppHang,
    },
    Rule {
        channel: "Application",
        provider: "Windows Error Reporting",
        ids: &[1001],
        kind: EventKind::WerReport,
    },
    Rule {
        channel: "System",
        provider: "Microsoft-Windows-WER-SystemErrorReporting",
        ids: &[1001],
        kind: EventKind::Bugcheck,
    },
    Rule {
        channel: "System",
        provider: "Microsoft-Windows-Kernel-Power",
        ids: &[41],
        kind: EventKind::KernelPowerUnexpected,
    },
    Rule {
        channel: "System",
        provider: "EventLog",
        ids: &[6008],
        kind: EventKind::UnexpectedShutdown,
    },
    Rule {
        channel: "System",
        provider: "Microsoft-Windows-WHEA-Logger",
        ids: &[],
        kind: EventKind::WheaError,
    },
    Rule {
        channel: "System",
        provider: "Display",
        ids: &[4101],
        kind: EventKind::GpuDriverReset,
    },
    Rule {
        channel: "System",
        provider: "nvlddmkm",
        ids: &[13, 14, 153],
        kind: EventKind::GpuDriverReset,
    },
    Rule {
        channel: "System",
        provider: "disk",
        ids: &[7, 11, 51, 153],
        kind: EventKind::StorageError,
    },
    Rule {
        channel: "System",
        provider: "Ntfs",
        ids: &[55, 98, 137, 140],
        kind: EventKind::StorageError,
    },
    Rule {
        channel: "System",
        provider: "Microsoft-Windows-Ntfs",
        ids: &[55, 98, 137, 140],
        kind: EventKind::StorageError,
    },
    Rule {
        channel: "System",
        provider: "stornvme",
        ids: &[11, 129],
        kind: EventKind::StorageError,
    },
    Rule {
        channel: "System",
        provider: "storahci",
        ids: &[129],
        kind: EventKind::StorageError,
    },
    Rule {
        channel: "System",
        provider: "Service Control Manager",
        ids: &[7000, 7001, 7009, 7011, 7022, 7023, 7024, 7031, 7034],
        kind: EventKind::ServiceFailure,
    },
    Rule {
        channel: "System",
        provider: "Microsoft-Windows-Kernel-General",
        ids: &[12],
        kind: EventKind::SystemStart,
    },
    Rule {
        channel: "System",
        provider: "Microsoft-Windows-Kernel-General",
        ids: &[13],
        kind: EventKind::SystemShutdown,
    },
];

/// Channels the reader queries.
pub const CHANNELS: [&str; 2] = ["Application", "System"];

/// Structured XPath query for `EvtQuery`/`EvtSubscribe` selecting exactly the
/// events [`classify`] accepts on `channel`.
pub fn xpath_for_channel(channel: &str) -> String {
    let clauses: Vec<String> = RULES
        .iter()
        .filter(|r| r.channel == channel)
        .map(|r| {
            let ids = if r.ids.is_empty() {
                String::new()
            } else {
                let ids: Vec<String> = r.ids.iter().map(|i| format!("EventID={i}")).collect();
                format!(" and ({})", ids.join(" or "))
            };
            let level = if r.problems_only() {
                " and (Level=1 or Level=2 or Level=3)"
            } else {
                ""
            };
            format!("(Provider[@Name='{}']{ids}{level})", r.provider)
        })
        .collect();
    format!("*[System[{}]]", clauses.join(" or "))
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AppCrash {
    pub app_name: Option<String>,
    pub app_version: Option<String>,
    pub app_path: Option<String>,
    pub module_name: Option<String>,
    pub module_version: Option<String>,
    pub module_path: Option<String>,
    pub exception_code: Option<u32>,
    pub exception_name: Option<String>,
    pub exception_description: Option<String>,
    pub fault_offset: Option<String>,
    pub process_id: Option<u32>,
    pub report_id: Option<String>,
    pub package_full_name: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AppHang {
    pub app_name: Option<String>,
    pub app_version: Option<String>,
    pub app_path: Option<String>,
    pub process_id: Option<u32>,
    pub report_id: Option<String>,
    pub hang_type: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WerReport {
    pub event_name: Option<String>,
    pub bucket: Option<String>,
    pub report_id: Option<String>,
    pub parameters: Vec<String>,
    pub attached_files: Vec<String>,
    pub store_path: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BugcheckInfo {
    pub code: Option<u32>,
    pub name: Option<String>,
    pub description: Option<String>,
    pub area: Option<String>,
    pub parameters: Vec<u64>,
    pub dump_path: Option<String>,
    pub report_id: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct KernelPowerInfo {
    /// Stop code recorded by Kernel-Power; `None` when 0 (no bugcheck recorded).
    pub bugcheck: Option<BugcheckInfo>,
    pub power_button_pressed: bool,
    pub sleep_in_progress: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum EventDetail {
    AppCrash(AppCrash),
    AppHang(AppHang),
    WerReport(WerReport),
    Bugcheck(BugcheckInfo),
    KernelPower(KernelPowerInfo),
    Generic,
}

/// An event that matched a classification rule.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StabilityEvent {
    pub channel: String,
    pub record_id: u64,
    pub time_ms: i64,
    pub provider: String,
    pub event_id: u32,
    pub kind: EventKind,
    pub severity: Severity,
    pub title: String,
    /// What the event is about, for grouping: application, stop code, service or device.
    pub subject: Option<String>,
    pub detail: EventDetail,
    pub data: Vec<(String, String)>,
}

pub fn classify(raw: &RawEvent) -> Option<StabilityEvent> {
    let rule = RULES.iter().find(|r| {
        r.channel.eq_ignore_ascii_case(&raw.channel)
            && r.provider == raw.provider
            && (r.ids.is_empty() || r.ids.contains(&raw.event_id))
            && (!r.problems_only() || (1..=3).contains(&raw.level))
    })?;
    let mut kind = rule.kind;
    let mut severity = Severity::from_level(raw.level);
    let mut subject = None;
    let (title, detail) = match kind {
        EventKind::AppCrash => {
            let d = app_crash(raw);
            subject = d.app_name.clone();
            let app = d
                .app_name
                .clone()
                .unwrap_or_else(|| "An application".into());
            (format!("{app} crashed"), EventDetail::AppCrash(d))
        }
        EventKind::AppHang => {
            let d = AppHang {
                app_name: raw.value("AppName").or_else(|| raw.value("#0")),
                app_version: raw.value("AppVersion").or_else(|| raw.value("#1")),
                app_path: raw.value("ExeFileName"),
                process_id: raw
                    .value("ProcessId")
                    .and_then(|v| codes::parse_code(&v, true))
                    .and_then(|v| u32::try_from(v).ok()),
                report_id: raw.value("ReportId"),
                hang_type: raw.value("HangType"),
            };
            subject = d.app_name.clone();
            let app = d
                .app_name
                .clone()
                .unwrap_or_else(|| "An application".into());
            (
                format!("{app} stopped responding and was closed"),
                EventDetail::AppHang(d),
            )
        }
        EventKind::WerReport => {
            let d = wer_report(raw);
            subject = d.parameters.first().cloned();
            severity = match d.event_name.as_deref() {
                Some("LiveKernelEvent") => Severity::Warning,
                _ => Severity::Info,
            };
            let name = d.event_name.clone().unwrap_or_else(|| "Unknown".into());
            let title = match &subject {
                Some(s) => format!("Error report {name}: {s}"),
                None => format!("Error report {name}"),
            };
            (title, EventDetail::WerReport(d))
        }
        EventKind::Bugcheck => {
            let d = bugcheck_from_wer(raw);
            severity = Severity::Critical;
            subject = d.code.map(|c| format!("0x{c:08X}"));
            (bugcheck_title(&d), EventDetail::Bugcheck(d))
        }
        EventKind::KernelPowerUnexpected => {
            let code = raw
                .value("BugcheckCode")
                .and_then(|v| codes::parse_code(&v, true))
                .and_then(|v| u32::try_from(v).ok());
            let bugcheck = code.filter(|c| *c != 0).map(|c| {
                let params = (1..=4)
                    .filter_map(|i| {
                        raw.value(&format!("BugcheckParameter{i}"))
                            .and_then(|v| codes::parse_code(&v, true))
                    })
                    .collect();
                bugcheck_info(c, params, None, None)
            });
            let d = KernelPowerInfo {
                power_button_pressed: raw.value("PowerButtonTimestamp").is_some_and(|v| v != "0"),
                sleep_in_progress: raw.value("SleepInProgress").is_some_and(|v| v != "0"),
                bugcheck,
            };
            severity = Severity::Critical;
            let title = match &d.bugcheck {
                Some(b) => {
                    subject = b.code.map(|c| format!("0x{c:08X}"));
                    format!("Unexpected restart after stop code {}", code_label(b))
                }
                None if d.power_button_pressed => {
                    "Unexpected restart (power button was used)".into()
                }
                None => "Unexpected restart without a recorded stop code".into(),
            };
            (title, EventDetail::KernelPower(d))
        }
        EventKind::UnexpectedShutdown => {
            let when = [raw.value("#0"), raw.value("#1")]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" ");
            let when = strip_bidi_marks(&when);
            let title = if when.is_empty() {
                "Previous shutdown was unexpected".into()
            } else {
                format!("Previous shutdown at {when} was unexpected")
            };
            (title, EventDetail::Generic)
        }
        EventKind::WheaError | EventKind::WheaCorrected => {
            if severity <= Severity::Warning {
                kind = EventKind::WheaCorrected;
            }
            subject = raw.value("ErrorSource").or_else(|| raw.value("ErrorType"));
            let title = if kind == EventKind::WheaCorrected {
                "Corrected hardware error reported (WHEA)".into()
            } else {
                "Hardware error reported (WHEA)".into()
            };
            (title, EventDetail::Generic)
        }
        EventKind::GpuDriverReset => {
            subject = raw.value("#0").or_else(|| Some(raw.provider.clone()));
            let title = if raw.provider == "Display" {
                format!(
                    "Display driver {} stopped responding and recovered",
                    raw.value("#0").unwrap_or_default()
                )
                .replace("  ", " ")
            } else {
                format!("GPU driver error ({} event {})", raw.provider, raw.event_id)
            };
            (title, EventDetail::Generic)
        }
        EventKind::StorageError => {
            subject = raw
                .value("#0")
                .or_else(|| raw.value("DeviceName"))
                .or_else(|| Some(raw.provider.clone()));
            (
                format!(
                    "Storage warning from {} (event {})",
                    raw.provider, raw.event_id
                ),
                EventDetail::Generic,
            )
        }
        EventKind::ServiceFailure => {
            subject = raw.value("param1").or_else(|| raw.value("#0"));
            let svc = subject.clone().unwrap_or_else(|| "A service".into());
            let what = match raw.event_id {
                7031 | 7034 => "terminated unexpectedly",
                7000 | 7009 | 7011 | 7022 => "failed to start or respond",
                7023 | 7024 => "stopped with an error",
                _ => "failed",
            };
            (format!("Service {svc} {what}"), EventDetail::Generic)
        }
        EventKind::SystemStart => ("Windows started".into(), EventDetail::Generic),
        EventKind::SystemShutdown => ("Windows shut down".into(), EventDetail::Generic),
    };
    Some(StabilityEvent {
        channel: raw.channel.clone(),
        record_id: raw.record_id,
        time_ms: raw.time_ms,
        provider: raw.provider.clone(),
        event_id: raw.event_id,
        kind,
        severity,
        title,
        subject,
        detail,
        data: raw.data.clone(),
    })
}

fn app_crash(raw: &RawEvent) -> AppCrash {
    // Named fields since Windows 10; positional order matches the event manifest for older logs.
    let f = |name: &str, pos: usize| raw.value(name).or_else(|| raw.value(&format!("#{pos}")));
    let exception_code = f("ExceptionCode", 6)
        .and_then(|v| codes::parse_code(&v, false))
        .and_then(|v| u32::try_from(v).ok());
    let info = exception_code.and_then(codes::exception);
    AppCrash {
        app_name: f("AppName", 0),
        app_version: f("AppVersion", 1),
        module_name: f("ModuleName", 3),
        module_version: f("ModuleVersion", 4),
        exception_code,
        exception_name: info.map(|i| i.name.to_owned()),
        exception_description: info.map(|i| i.description.to_owned()),
        fault_offset: f("FaultingOffset", 7),
        process_id: f("ProcessId", 8)
            .and_then(|v| codes::parse_code(&v, true))
            .and_then(|v| u32::try_from(v).ok()),
        app_path: f("AppPath", 10),
        module_path: f("ModulePath", 11),
        report_id: f("IntegratorReportId", 12),
        package_full_name: f("PackageFullName", 13),
    }
}

fn wer_report(raw: &RawEvent) -> WerReport {
    let parameters = (1..=10)
        .filter_map(|i| raw.value(&format!("P{i}")))
        .collect();
    let attached_files = raw
        .value("AttachedFiles")
        .map(|v| {
            v.lines()
                .map(|l| l.trim().trim_start_matches(r"\\?\").to_owned())
                .filter(|l| !l.is_empty())
                .collect()
        })
        .unwrap_or_default();
    WerReport {
        event_name: raw.value("EventName"),
        bucket: raw.value("Bucket"),
        report_id: raw.value("ReportId"),
        parameters,
        attached_files,
        store_path: raw.value("StorePath"),
    }
}

fn bugcheck_from_wer(raw: &RawEvent) -> BugcheckInfo {
    // param1: "0x0000013a (0x0000000000000017, 0xffffdc8548000340, ...)"
    let p1 = raw
        .value("param1")
        .or_else(|| raw.value("#0"))
        .unwrap_or_default();
    let (code, params) = parse_bugcheck_string(&p1);
    let dump = raw.value("param2").or_else(|| raw.value("#1"));
    let report = raw.value("param3").or_else(|| raw.value("#2"));
    match code {
        Some(c) => bugcheck_info(c, params, dump, report),
        None => BugcheckInfo {
            code: None,
            name: None,
            description: None,
            area: None,
            parameters: params,
            dump_path: dump,
            report_id: report,
        },
    }
}

/// Parses `0x0000013a (0x17, 0xffff..., 0x0, 0x0)` into the code and parameters.
pub fn parse_bugcheck_string(s: &str) -> (Option<u32>, Vec<u64>) {
    let s = s.trim();
    let (code_part, rest) = match s.split_once('(') {
        Some((a, b)) => (a, b.trim_end_matches(')')),
        None => (s, ""),
    };
    let code = codes::parse_code(code_part, false).and_then(|v| u32::try_from(v).ok());
    let params = rest
        .split(',')
        .filter_map(|p| codes::parse_code(p, false))
        .collect();
    (code, params)
}

fn bugcheck_info(
    code: u32,
    parameters: Vec<u64>,
    dump_path: Option<String>,
    report_id: Option<String>,
) -> BugcheckInfo {
    let info = codes::bugcheck(code);
    BugcheckInfo {
        code: Some(code),
        name: info.map(|i| i.name.to_owned()),
        description: info.map(|i| i.description.to_owned()),
        area: info.map(|i| i.area.to_owned()),
        parameters,
        dump_path,
        report_id,
    }
}

fn code_label(b: &BugcheckInfo) -> String {
    match (b.code, &b.name) {
        (Some(c), Some(n)) => format!("0x{c:08X} {n}"),
        (Some(c), None) => format!("0x{c:08X}"),
        _ => "unknown".into(),
    }
}

fn bugcheck_title(b: &BugcheckInfo) -> String {
    format!("Blue screen: {}", code_label(b))
}

/// The event log embeds U+200E/U+200F direction marks in localized dates.
fn strip_bidi_marks(s: &str) -> String {
    s.chars()
        .filter(|c| !matches!(c, '\u{200E}' | '\u{200F}'))
        .collect()
}

// ---------------------------------------------------------------------------
// Unexpected shutdown incidents
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum IncidentKind {
    /// A stop code was recorded (WER bugcheck report or Kernel-Power BugcheckCode ≠ 0).
    ConfirmedBugcheck,
    /// Windows restarted without a clean shutdown and no stop code was recorded:
    /// power loss, forced power-off, hard reset or a hang. The cause is not in the logs.
    UnexpectedShutdown,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EventRef {
    pub channel: String,
    pub record_id: u64,
    pub time_ms: i64,
    pub kind: EventKind,
    pub title: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ShutdownIncident {
    /// Time Windows logged the evidence (after the restart).
    pub time_ms: i64,
    pub kind: IncidentKind,
    pub bugcheck: Option<BugcheckInfo>,
    pub power_button_pressed: bool,
    pub previous_shutdown_text: Option<String>,
    pub evidence: Vec<EventRef>,
}

/// Window within which Kernel-Power 41, EventLog 6008 and the WER bugcheck
/// report written during the same boot are treated as one incident.
pub const INCIDENT_WINDOW_MS: i64 = 15 * 60 * 1000;

/// Groups bugcheck, Kernel-Power 41 and EventLog 6008 events into incidents.
/// Events are matched when they occur within [`INCIDENT_WINDOW_MS`] of the
/// incident's first event. Output is newest first.
pub fn group_shutdown_incidents(events: &[StabilityEvent]) -> Vec<ShutdownIncident> {
    let mut relevant: Vec<&StabilityEvent> = events
        .iter()
        .filter(|e| {
            matches!(
                e.kind,
                EventKind::Bugcheck
                    | EventKind::KernelPowerUnexpected
                    | EventKind::UnexpectedShutdown
            )
        })
        .collect();
    relevant.sort_by_key(|e| (e.time_ms, e.record_id));

    let mut out: Vec<ShutdownIncident> = Vec::new();
    for e in relevant {
        let joins = out.last().is_some_and(|inc| {
            e.time_ms - inc.time_ms <= INCIDENT_WINDOW_MS && !has_kind(inc, e.kind)
        });
        if !joins {
            out.push(ShutdownIncident {
                time_ms: e.time_ms,
                kind: IncidentKind::UnexpectedShutdown,
                bugcheck: None,
                power_button_pressed: false,
                previous_shutdown_text: None,
                evidence: Vec::new(),
            });
        }
        let Some(inc) = out.last_mut() else { continue };
        inc.evidence.push(EventRef {
            channel: e.channel.clone(),
            record_id: e.record_id,
            time_ms: e.time_ms,
            kind: e.kind,
            title: e.title.clone(),
        });
        match &e.detail {
            // The WER report carries the dump path and report id, so it wins over Kernel-Power's copy.
            EventDetail::Bugcheck(b) if b.code.is_some() => inc.bugcheck = Some(b.clone()),
            EventDetail::KernelPower(k) => {
                inc.power_button_pressed |= k.power_button_pressed;
                if inc.bugcheck.is_none() {
                    inc.bugcheck = k.bugcheck.clone();
                }
            }
            _ if e.kind == EventKind::UnexpectedShutdown => {
                inc.previous_shutdown_text = Some(e.title.clone())
            }
            _ => {}
        }
        if inc.bugcheck.is_some() {
            inc.kind = IncidentKind::ConfirmedBugcheck;
        }
    }
    out.reverse();
    out
}

fn has_kind(inc: &ShutdownIncident, kind: EventKind) -> bool {
    inc.evidence.iter().any(|r| r.kind == kind)
}

/// Counts events per subject (case-insensitive) for grouping and recurrence detection.
pub fn count_by_subject<'a>(
    events: impl IntoIterator<Item = &'a StabilityEvent>,
) -> BTreeMap<String, usize> {
    let mut m = BTreeMap::new();
    for e in events {
        if let Some(s) = &e.subject {
            *m.entry(s.to_ascii_lowercase()).or_insert(0) += 1;
        }
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    const APP_ERROR: &str = r"<Event xmlns='http://schemas.microsoft.com/win/2004/08/events/event'><System><Provider Name='Application Error' Guid='{a0e9b465-b939-57d7-b27d-95d8e925ff57}'/><EventID>1000</EventID><Version>0</Version><Level>2</Level><Task>100</Task><Opcode>0</Opcode><Keywords>0x8000000000000000</Keywords><TimeCreated SystemTime='2026-11-06T14:27:23.2194678Z'/><EventRecordID>117703</EventRecordID><Correlation/><Execution ProcessID='33188' ThreadID='22376'/><Channel>Application</Channel><Computer>PC</Computer><Security UserID='S-1-5-21-0-0-0-1001'/></System><EventData><Data Name='AppName'>Minecraft.exe</Data><Data Name='AppVersion'>1.0.1.0</Data><Data Name='AppTimeStamp'>69e9a09c</Data><Data Name='ModuleName'>libcef.dll</Data><Data Name='ModuleVersion'>127.3.5.0</Data><Data Name='ModuleTimeStamp'>66ba83b2</Data><Data Name='ExceptionCode'>80000003</Data><Data Name='FaultingOffset'>0000000005a8bed0</Data><Data Name='ProcessId'>0x57c4</Data><Data Name='ProcessCreationTime'>0x1dd6dfae6fd0d2c</Data><Data Name='AppPath'>C:\Program Files\WindowsApps\Game\Minecraft.exe</Data><Data Name='ModulePath'>C:\Users\user\AppData\Local\game\libcef.dll</Data><Data Name='IntegratorReportId'>c788bda7-9c65-45e5-912d-abac1b45114f</Data><Data Name='PackageFullName'>Microsoft.Game_2.6.2.0_x64</Data><Data Name='PackageRelativeAppId'>Minecraft</Data></EventData></Event>";

    const KP41: &str = r"<Event xmlns='http://schemas.microsoft.com/win/2004/08/events/event'><System><Provider Name='Microsoft-Windows-Kernel-Power' Guid='{331c3b3a-2005-44c2-ac5e-77220c37d6b4}'/><EventID>41</EventID><Version>10</Version><Level>1</Level><Task>63</Task><Opcode>0</Opcode><Keywords>0x8000400000000002</Keywords><TimeCreated SystemTime='2026-10-15T03:32:55.3515908Z'/><EventRecordID>125199</EventRecordID><Correlation/><Execution ProcessID='4' ThreadID='8'/><Channel>System</Channel><Computer>PC</Computer><Security UserID='S-1-5-18'/></System><EventData><Data Name='BugcheckCode'>314</Data><Data Name='BugcheckParameter1'>0x17</Data><Data Name='BugcheckParameter2'>0xffffdc8548000340</Data><Data Name='BugcheckParameter3'>0xffffdc8558f770b0</Data><Data Name='BugcheckParameter4'>0x0</Data><Data Name='SleepInProgress'>0</Data><Data Name='PowerButtonTimestamp'>0</Data></EventData></Event>";

    const WER_BUGCHECK: &str = r"<Event xmlns='http://schemas.microsoft.com/win/2004/08/events/event'><System><Provider Name='Microsoft-Windows-WER-SystemErrorReporting' Guid='{abce23e7-de45-4366-8631-84fa6c525952}'/><EventID>1001</EventID><Version>1</Version><Level>2</Level><Task>0</Task><Opcode>0</Opcode><Keywords>0x8000000000000000</Keywords><TimeCreated SystemTime='2026-10-15T03:33:07.9494118Z'/><EventRecordID>125306</EventRecordID><Correlation/><Execution ProcessID='1952' ThreadID='1956'/><Channel>System</Channel><Computer>PC</Computer><Security UserID='S-1-5-18'/></System><EventData><Data Name='param1'>0x0000013a (0x0000000000000017, 0xffffdc8548000340, 0xffffdc8558f770b0, 0x0000000000000000)</Data><Data Name='param2'>C:\WINDOWS\Minidump\101426-16484-01.dmp</Data><Data Name='param3'>0280bb5c-ae7a-45fb-995d-02c772745ce3</Data></EventData></Event>";

    const EVENTLOG_6008: &str = "<Event xmlns='http://schemas.microsoft.com/win/2004/08/events/event'><System><Provider Name='EventLog'/><EventID Qualifiers='32768'>6008</EventID><Version>0</Version><Level>2</Level><Task>0</Task><Opcode>0</Opcode><Keywords>0x80000000000000</Keywords><TimeCreated SystemTime='2026-10-15T03:33:07.1623210Z'/><EventRecordID>125180</EventRecordID><Correlation/><Execution ProcessID='3448' ThreadID='3548'/><Channel>System</Channel><Computer>PC</Computer><Security/></System><EventData><Data>8:32:23 PM</Data><Data>\u{200e}10/\u{200e}14/\u{200e}2026</Data><Data></Data><Data></Data><Data>21</Data></EventData></Event>";

    const WER_REPORT: &str = r"<Event xmlns='http://schemas.microsoft.com/win/2004/08/events/event'><System><Provider Name='Windows Error Reporting' Guid='{0ead09bd-2157-539a-8d6d-c87f95b64d70}'/><EventID>1001</EventID><Version>0</Version><Level>4</Level><Task>0</Task><Opcode>0</Opcode><Keywords>0x8000000000000000</Keywords><TimeCreated SystemTime='2026-11-07T09:33:14.5091545Z'/><EventRecordID>117958</EventRecordID><Channel>Application</Channel><Computer>PC</Computer></System><EventData><Data Name='Bucket'>1897126794448572759</Data><Data Name='BucketType'>5</Data><Data Name='EventName'>APPCRASH</Data><Data Name='Response'>Not available</Data><Data Name='CabId'>0</Data><Data Name='P1'>Game.exe</Data><Data Name='P2'>1.10.0.0</Data><Data Name='P3'></Data><Data Name='AttachedFiles'>
\\?\C:\ProgramData\Microsoft\Windows\WER\Temp\WER.1.tmp.xml
\\?\C:\ProgramData\Microsoft\Windows\WER\Temp\WER.2.tmp.csv</Data><Data Name='StorePath'>NULL</Data><Data Name='ReportId'>89c598f7-ca30-4669-abb7-e9cef071fc98</Data></EventData></Event>";

    fn parse(xml: &str) -> RawEvent {
        parse_event_xml(xml).expect("fixture parses")
    }

    #[test]
    fn parses_app_crash_fields() {
        let raw = parse(APP_ERROR);
        assert_eq!(raw.provider, "Application Error");
        assert_eq!(raw.event_id, 1000);
        assert_eq!(raw.record_id, 117_703);
        assert_eq!(raw.channel, "Application");
        let ev = classify(&raw).expect("classified");
        assert_eq!(ev.kind, EventKind::AppCrash);
        assert_eq!(ev.severity, Severity::Error);
        assert_eq!(ev.subject.as_deref(), Some("Minecraft.exe"));
        let EventDetail::AppCrash(d) = ev.detail else {
            panic!("detail")
        };
        assert_eq!(d.module_name.as_deref(), Some("libcef.dll"));
        assert_eq!(d.exception_code, Some(0x8000_0003));
        assert_eq!(d.exception_name.as_deref(), Some("STATUS_BREAKPOINT"));
        assert_eq!(d.process_id, Some(0x57c4));
        assert_eq!(
            d.report_id.as_deref(),
            Some("c788bda7-9c65-45e5-912d-abac1b45114f")
        );
    }

    #[test]
    fn provider_and_id_must_both_match() {
        let mut raw = parse(APP_ERROR);
        raw.event_id = 1001;
        assert!(
            classify(&raw).is_none(),
            "Application Error 1001 is not a crash"
        );
        let mut raw = parse(APP_ERROR);
        raw.provider = "Some Other Provider".into();
        assert!(classify(&raw).is_none());
        let mut raw = parse(APP_ERROR);
        raw.channel = "System".into();
        assert!(classify(&raw).is_none(), "channel is validated too");
    }

    #[test]
    fn positional_app_crash_fallback() {
        let raw = RawEvent {
            channel: "Application".into(),
            provider: "Application Error".into(),
            event_id: 1000,
            level: 2,
            record_id: 1,
            time_ms: 0,
            data: [
                "old.exe", "1.0", "x", "bad.dll", "2.0", "y", "c0000005", "0000beef", "0x10",
            ]
            .iter()
            .enumerate()
            .map(|(i, v)| (format!("#{i}"), (*v).to_owned()))
            .collect(),
        };
        let EventDetail::AppCrash(d) = classify(&raw).expect("classified").detail else {
            panic!()
        };
        assert_eq!(d.app_name.as_deref(), Some("old.exe"));
        assert_eq!(d.module_name.as_deref(), Some("bad.dll"));
        assert_eq!(d.exception_code, Some(0xC000_0005));
        assert_eq!(d.process_id, Some(16));
    }

    #[test]
    fn kernel_power_41_with_and_without_bugcheck() {
        let ev = classify(&parse(KP41)).expect("classified");
        assert_eq!(ev.kind, EventKind::KernelPowerUnexpected);
        let EventDetail::KernelPower(k) = &ev.detail else {
            panic!()
        };
        let b = k.bugcheck.as_ref().expect("bugcheck 314 = 0x13A");
        assert_eq!(b.code, Some(0x13A));
        assert_eq!(b.parameters.len(), 4);
        assert!(ev.title.contains("KERNEL_MODE_HEAP_CORRUPTION"));

        let zero = KP41.replace(
            "<Data Name='BugcheckCode'>314</Data>",
            "<Data Name='BugcheckCode'>0</Data>",
        );
        let ev = classify(&parse(&zero)).expect("classified");
        let EventDetail::KernelPower(k) = &ev.detail else {
            panic!()
        };
        assert!(
            k.bugcheck.is_none(),
            "Kernel-Power 41 with code 0 is not a BSOD"
        );
        assert_eq!(ev.title, "Unexpected restart without a recorded stop code");
    }

    #[test]
    fn wer_bugcheck_report() {
        let ev = classify(&parse(WER_BUGCHECK)).expect("classified");
        assert_eq!(ev.kind, EventKind::Bugcheck);
        let EventDetail::Bugcheck(b) = &ev.detail else {
            panic!()
        };
        assert_eq!(b.code, Some(0x13A));
        assert_eq!(
            b.parameters,
            vec![0x17, 0xffff_dc85_4800_0340, 0xffff_dc85_58f7_70b0, 0]
        );
        assert_eq!(
            b.dump_path.as_deref(),
            Some(r"C:\WINDOWS\Minidump\101426-16484-01.dmp")
        );
        assert_eq!(
            ev.title,
            "Blue screen: 0x0000013A KERNEL_MODE_HEAP_CORRUPTION"
        );
    }

    #[test]
    fn unnamed_data_and_bidi_marks() {
        let ev = classify(&parse(EVENTLOG_6008)).expect("classified");
        assert_eq!(ev.kind, EventKind::UnexpectedShutdown);
        assert_eq!(
            ev.title,
            "Previous shutdown at 8:32:23 PM 10/14/2026 was unexpected"
        );
    }

    #[test]
    fn wer_report_files_and_null_store() {
        let ev = classify(&parse(WER_REPORT)).expect("classified");
        let EventDetail::WerReport(w) = &ev.detail else {
            panic!()
        };
        assert_eq!(w.event_name.as_deref(), Some("APPCRASH"));
        assert_eq!(
            w.parameters,
            vec!["Game.exe".to_string(), "1.10.0.0".to_string()]
        );
        assert_eq!(w.store_path, None);
        assert_eq!(w.attached_files.len(), 2);
        assert!(w.attached_files[0].starts_with(r"C:\ProgramData"));
        assert_eq!(ev.severity, Severity::Info);
    }

    #[test]
    fn malformed_xml_is_an_error() {
        assert!(matches!(
            parse_event_xml("<Event>"),
            Err(EventParseError::Xml(_))
        ));
        assert_eq!(
            parse_event_xml("<Event><System/></Event>"),
            Err(EventParseError::Missing("Provider/@Name"))
        );
    }

    #[test]
    fn groups_one_incident_from_three_events() {
        let events: Vec<StabilityEvent> = [KP41, WER_BUGCHECK, EVENTLOG_6008]
            .iter()
            .filter_map(|x| classify(&parse(x)))
            .collect();
        let incidents = group_shutdown_incidents(&events);
        assert_eq!(incidents.len(), 1);
        let inc = &incidents[0];
        assert_eq!(inc.kind, IncidentKind::ConfirmedBugcheck);
        assert_eq!(inc.evidence.len(), 3);
        let b = inc.bugcheck.as_ref().expect("bugcheck");
        assert!(
            b.dump_path.is_some(),
            "WER report details win over Kernel-Power's"
        );
    }

    #[test]
    fn separate_incidents_and_unexpected_without_code() {
        let zero = KP41.replace(
            "<Data Name='BugcheckCode'>314</Data>",
            "<Data Name='BugcheckCode'>0</Data>",
        );
        let mut a = classify(&parse(&zero)).expect("a");
        let mut b = classify(&parse(&zero)).expect("b");
        a.time_ms = 0;
        b.time_ms = INCIDENT_WINDOW_MS + 1;
        b.record_id += 1;
        let incidents = group_shutdown_incidents(&[a, b]);
        assert_eq!(incidents.len(), 2);
        assert!(incidents
            .iter()
            .all(|i| i.kind == IncidentKind::UnexpectedShutdown));
        assert!(incidents[0].time_ms > incidents[1].time_ms, "newest first");
    }

    #[test]
    fn two_kernel_power_events_close_together_are_two_incidents() {
        let mut a = classify(&parse(KP41)).expect("a");
        let mut b = a.clone();
        a.time_ms = 0;
        b.time_ms = 60_000;
        b.record_id += 1;
        assert_eq!(group_shutdown_incidents(&[a, b]).len(), 2);
    }

    #[test]
    fn xpath_lists_each_provider() {
        let x = xpath_for_channel("Application");
        assert!(x.starts_with("*[System[("));
        assert!(x.contains("(Provider[@Name='Application Error'] and (EventID=1000))"));
        let s = xpath_for_channel("System");
        assert!(s.contains("(Provider[@Name='Microsoft-Windows-WHEA-Logger'])"));
        assert!(s.contains("EventID=7031 or EventID=7034"));
    }

    #[test]
    fn whea_level_selects_corrected() {
        let raw = RawEvent {
            channel: "System".into(),
            provider: "Microsoft-Windows-WHEA-Logger".into(),
            event_id: 19,
            level: 3,
            record_id: 5,
            time_ms: 1,
            data: vec![("ErrorSource".into(), "3".into())],
        };
        assert_eq!(
            classify(&raw).map(|e| e.kind),
            Some(EventKind::WheaCorrected)
        );
        let raw = RawEvent {
            level: 2,
            event_id: 18,
            ..raw
        };
        assert_eq!(classify(&raw).map(|e| e.kind), Some(EventKind::WheaError));
    }

    #[test]
    fn healthy_volume_notice_is_not_a_storage_problem() {
        // Ntfs 98 at Information level is "Volume ... is healthy. No action is needed."
        let raw = RawEvent {
            channel: "System".into(),
            provider: "Microsoft-Windows-Ntfs".into(),
            event_id: 98,
            level: 4,
            record_id: 7,
            time_ms: 1,
            data: vec![],
        };
        assert!(classify(&raw).is_none());
        let raw = RawEvent { level: 2, ..raw };
        assert_eq!(
            classify(&raw).map(|e| e.kind),
            Some(EventKind::StorageError)
        );
        assert!(xpath_for_channel("System")
            .contains("(Provider[@Name='Ntfs'] and (EventID=55 or EventID=98 or EventID=137 or EventID=140) and (Level=1 or Level=2 or Level=3))"));
    }
}
