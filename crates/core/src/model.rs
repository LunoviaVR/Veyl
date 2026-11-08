//! Typed telemetry models shared by collectors, storage and the UI.
//!
//! Conventions:
//! - Every value that a source can fail to provide is an `Option`; `None` means
//!   "not measured", never zero. A section that can be unavailable carries a
//!   [`Status`] explaining why.
//! - Field names carry their unit: `_bytes`, `_bps` (bytes per second), `_pct`
//!   (0..=100), `_mhz`, `_c` (degrees Celsius), `_rpm`, `_ms`, `_secs`.
//! - Timestamps are wall-clock Unix milliseconds (`_ms` suffix on a time field).
//!   Rates and intervals are computed from monotonic clocks by the collectors.

use serde::{Deserialize, Serialize};

/// Availability of a value, a provider or a whole section.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub enum Availability {
    Available,
    /// The hardware, driver or Windows version does not expose this.
    Unsupported,
    /// Exposed, but needs administrator rights (or another permission) we do not have.
    PermissionRequired,
    /// An optional external provider (vendor GPU library, PresentMon, Debugging Tools) is not present or not running.
    ProviderUnavailable,
    /// The last reading is older than expected.
    Stale,
    /// The source works but needs more than one sample before it can report.
    WaitingForSamples,
    /// The source failed; see the message.
    Error,
}

/// Availability plus a human-readable explanation.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub state: Availability,
    pub message: Option<String>,
}

impl Status {
    pub fn available() -> Self {
        Self {
            state: Availability::Available,
            message: None,
        }
    }
    pub fn new(state: Availability, message: impl Into<String>) -> Self {
        Self {
            state,
            message: Some(message.into()),
        }
    }
    pub fn waiting(message: impl Into<String>) -> Self {
        Self::new(Availability::WaitingForSamples, message)
    }
    pub fn error(message: impl Into<String>) -> Self {
        Self::new(Availability::Error, message)
    }
    pub fn is_available(&self) -> bool {
        self.state == Availability::Available
    }
}

impl Default for Status {
    fn default() -> Self {
        Self::waiting("Waiting for the first sample")
    }
}

/// Canonical unit symbols used by sensor readings and history metrics.
pub mod unit {
    pub const PERCENT: &str = "%";
    pub const CELSIUS: &str = "°C";
    pub const RPM: &str = "RPM";
    pub const MHZ: &str = "MHz";
    pub const VOLT: &str = "V";
    pub const WATT: &str = "W";
    pub const AMPERE: &str = "A";
    pub const BYTES: &str = "B";
    pub const BYTES_PER_SEC: &str = "B/s";
    pub const MILLISECONDS: &str = "ms";
    pub const FPS: &str = "FPS";
    pub const COUNT: &str = "";
    pub const HOURS: &str = "h";
}

// ---------------------------------------------------------------------------
// Static system information
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct SystemInfo {
    pub computer_name: Option<String>,
    pub os_name: Option<String>,
    pub os_build: Option<String>,
    pub boot_time_ms: Option<i64>,
    pub cpu: CpuInfo,
    pub installed_memory_bytes: Option<u64>,
    pub gpus: Vec<GpuInfo>,
    pub elevated: bool,
    pub status: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct CpuInfo {
    pub brand: Option<String>,
    pub vendor: Option<String>,
    pub physical_cores: Option<u32>,
    pub logical_processors: Option<u32>,
    pub packages: Option<u32>,
    /// Cores reported with efficiency class > 0 (hybrid "P-cores"); None when not hybrid or unknown.
    pub performance_cores: Option<u32>,
    pub efficiency_cores: Option<u32>,
    pub base_mhz: Option<f32>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GpuInfo {
    /// Adapter LUID as `0xHIGH_0xLOW`, the key used by PDH GPU counters.
    pub luid: String,
    pub name: String,
    pub vendor_id: u32,
    pub device_id: u32,
    pub dedicated_memory_bytes: u64,
    pub shared_memory_bytes: u64,
    pub is_software: bool,
}

// ---------------------------------------------------------------------------
// Live telemetry frame (one per sampling tick)
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct TelemetryFrame {
    pub seq: u64,
    pub timestamp_ms: i64,
    pub cpu: CpuSnapshot,
    pub memory: MemorySnapshot,
    pub disks: DisksSnapshot,
    pub network: NetworkSnapshot,
    pub gpus: GpusSnapshot,
    pub self_usage: SelfUsage,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct CpuSnapshot {
    pub status: Status,
    /// Time-based utilization of all logical processors (`% Processor Time`).
    pub total_pct: Option<f32>,
    /// Frequency-scaled utilization (`% Processor Utility`), what Task Manager shows. Can exceed 100 under boost.
    pub utility_pct: Option<f32>,
    /// One slot per logical processor in (group, index) order; `None` when Windows
    /// had no valid value for that processor this tick.
    pub per_core_pct: Vec<Option<f32>>,
    /// Nominal (base) frequency reported by Windows.
    pub base_mhz: Option<f32>,
    /// Effective frequency: base × `% Processor Performance` / 100, averaged over the sample interval.
    pub effective_mhz: Option<f32>,
    pub per_core_mhz: Vec<Option<f32>>,
    /// Package temperature from a hardware sensor provider, if one supplies it.
    pub package_temp_c: Option<f32>,
    /// Package power from RAPL (Windows Energy Meter), watts.
    pub package_power_w: Option<f32>,
    pub package_temp_source: Option<String>,
    pub process_count: Option<u32>,
    pub thread_count: Option<u32>,
    pub handle_count: Option<u32>,
    pub uptime_secs: Option<u64>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct MemorySnapshot {
    pub status: Status,
    pub total_bytes: Option<u64>,
    pub available_bytes: Option<u64>,
    pub used_bytes: Option<u64>,
    pub load_pct: Option<f32>,
    pub commit_bytes: Option<u64>,
    pub commit_limit_bytes: Option<u64>,
    pub cached_bytes: Option<u64>,
    pub paged_pool_bytes: Option<u64>,
    pub nonpaged_pool_bytes: Option<u64>,
    /// Hard page faults per second (`\Memory\Pages Input/sec`), a memory-pressure indicator.
    pub pages_input_per_sec: Option<f32>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct DisksSnapshot {
    pub status: Status,
    pub volumes: Vec<VolumeSnapshot>,
    pub physical: Vec<PhysicalDiskActivity>,
    pub total_read_bps: Option<f64>,
    pub total_write_bps: Option<f64>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VolumeSnapshot {
    pub mount: String,
    pub label: Option<String>,
    pub file_system: Option<String>,
    pub drive_type: String,
    pub total_bytes: Option<u64>,
    pub free_bytes: Option<u64>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PhysicalDiskActivity {
    /// PDH instance name, e.g. `0 C: D:`.
    pub name: String,
    pub index: Option<u32>,
    pub read_bps: Option<f64>,
    pub write_bps: Option<f64>,
    /// 100 − `% Idle Time`, clamped to 0..=100.
    pub active_pct: Option<f32>,
    pub queue_length: Option<f32>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct NetworkSnapshot {
    pub status: Status,
    pub adapters: Vec<NetworkAdapterSnapshot>,
    pub total_rx_bps: Option<f64>,
    pub total_tx_bps: Option<f64>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NetworkAdapterSnapshot {
    pub luid: u64,
    /// Windows interface index (NET_IFINDEX); identifies the adapter in packet captures.
    pub if_index: u32,
    pub alias: String,
    pub description: String,
    pub media: String,
    pub connected: bool,
    pub link_speed_bps: Option<u64>,
    pub rx_bps: Option<f64>,
    pub tx_bps: Option<f64>,
    pub rx_total_bytes: u64,
    pub tx_total_bytes: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct GpusSnapshot {
    pub status: Status,
    pub adapters: Vec<GpuSnapshot>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GpuSnapshot {
    pub luid: String,
    pub name: String,
    pub vendor_id: u32,
    /// Busiest engine's utilization, summed over processes (Task Manager's rule).
    pub util_pct: Option<f32>,
    pub engines: Vec<GpuEngineUsage>,
    pub dedicated_total_bytes: u64,
    pub dedicated_used_bytes: Option<u64>,
    pub shared_used_bytes: Option<u64>,
    pub temperature_c: Option<f32>,
    pub temperature_max_c: Option<f32>,
    pub hotspot_c: Option<f32>,
    pub fan_rpm: Option<u32>,
    pub fan_pct: Option<f32>,
    /// Driver-reported power draw as a percentage of the board limit (WDDM perf data).
    pub power_pct: Option<f32>,
    pub power_w: Option<f32>,
    pub core_clock_mhz: Option<f32>,
    pub memory_clock_mhz: Option<f32>,
    pub counters_status: Status,
    pub sensors_status: Status,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GpuEngineUsage {
    pub engine_type: String,
    pub util_pct: f32,
}

/// Resource use of this application's own process (overhead self-check).
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct SelfUsage {
    pub cpu_pct: Option<f32>,
    pub working_set_bytes: Option<u64>,
    pub private_bytes: Option<u64>,
    /// Wall time the last sampling tick took, in milliseconds.
    pub tick_cost_ms: Option<f32>,
}

// ---------------------------------------------------------------------------
// Processes
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProcessTable {
    pub status: Status,
    pub timestamp_ms: i64,
    pub logical_processors: u32,
    pub rows: Vec<ProcessRow>,
    /// Processes owning a visible app window (Task Manager's "Apps" group).
    pub app_pids: Vec<u32>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProcessRow {
    pub pid: u32,
    pub parent_pid: u32,
    /// Creation time as Unix ms; with `pid` it identifies a process across PID reuse.
    pub start_time_ms: Option<i64>,
    pub name: String,
    pub path: Option<String>,
    /// Share of total CPU time across all logical processors (Task Manager "Details" semantics).
    pub cpu_pct: Option<f32>,
    pub working_set_bytes: u64,
    /// Private working set: what Task Manager's Processes tab shows as "Memory".
    pub private_working_set_bytes: u64,
    /// Private committed bytes (commit charge).
    pub private_bytes: u64,
    /// All I/O (file, network, device) transfer rates from the kernel's per-process counters.
    pub io_read_bps: Option<f64>,
    pub io_write_bps: Option<f64>,
    pub gpu_pct: Option<f32>,
    pub gpu_dedicated_bytes: Option<u64>,
    pub threads: u32,
    pub handles: u32,
    pub session_id: u32,
    pub suspended: bool,
}

// ---------------------------------------------------------------------------
// Hardware sensors
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub enum SensorCategory {
    Cpu,
    Gpu,
    Motherboard,
    Memory,
    Storage,
    System,
    Other,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub enum SensorKind {
    Temperature,
    Voltage,
    Fan,
    Current,
    Power,
    Clock,
    Usage,
    Throttle,
    Health,
    Other,
}

/// One normalized sensor measurement.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SensorReading {
    /// Stable identifier: `<provider>/<device>/<sensor>`.
    pub key: String,
    pub provider: String,
    pub device: String,
    pub category: SensorCategory,
    pub kind: SensorKind,
    pub label: String,
    pub unit: String,
    pub value: Option<f64>,
    pub timestamp_ms: i64,
}

/// Status of one sensor provider (native counters, vendor libraries, WDDM, storage).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderStatus {
    pub id: String,
    pub name: String,
    pub status: Status,
    pub last_update_ms: Option<i64>,
    pub reading_count: usize,
}

/// The result of polling one hardware sensor provider.
#[derive(Clone, Debug, PartialEq)]
pub struct ProviderPoll {
    pub status: Status,
    pub readings: Vec<SensorReading>,
}

/// A source of hardware sensor readings. Implementations live in the platform
/// crate; the runtime polls each on its own interval and isolates failures, so
/// one provider failing never affects the others.
pub trait HardwareSensorProvider {
    /// Stable machine id (`thermal-zone`, `adlx`, ...).
    fn id(&self) -> &'static str;
    /// Display name.
    fn name(&self) -> &'static str;
    /// Take one reading of every sensor the provider exposes.
    fn poll(&mut self, now_ms: i64) -> ProviderPoll;
}

/// Running min/max/avg for a sensor since the last reset.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SensorStats {
    pub min: f64,
    pub max: f64,
    pub avg: f64,
    pub samples: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SensorRow {
    #[serde(flatten)]
    pub reading: SensorReading,
    pub stats: Option<SensorStats>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SensorsView {
    pub providers: Vec<ProviderStatus>,
    pub rows: Vec<SensorRow>,
    pub stats_since_ms: i64,
}

// ---------------------------------------------------------------------------
// Storage health
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DriveHealth {
    pub index: u32,
    pub model: Option<String>,
    pub serial: Option<String>,
    pub firmware: Option<String>,
    pub bus_type: Option<String>,
    pub size_bytes: Option<u64>,
    pub temperature_c: Option<f32>,
    pub temperature_warning_c: Option<f32>,
    pub temperature_critical_c: Option<f32>,
    pub nvme: Option<NvmeHealth>,
    pub status: Status,
    pub health_status: Status,
}

/// NVMe SMART / Health Information log page (log identifier 02h), NVMe Base Specification 1.4 §5.14.1.2.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NvmeHealth {
    pub critical_warning: u8,
    pub composite_temperature_c: Option<f32>,
    pub available_spare_pct: u8,
    pub available_spare_threshold_pct: u8,
    pub percentage_used: u8,
    pub data_units_read: u128,
    pub data_units_written: u128,
    pub power_cycles: u128,
    pub power_on_hours: u128,
    pub unsafe_shutdowns: u128,
    pub media_errors: u128,
    pub error_log_entries: u128,
    pub warning_temp_minutes: u32,
    pub critical_temp_minutes: u32,
}

impl NvmeHealth {
    /// Human-readable meanings of the set Critical Warning bits (spec Figure 194, bits 0-5).
    pub fn critical_warning_flags(&self) -> Vec<&'static str> {
        const FLAGS: [&str; 6] = [
            "available spare below threshold",
            "temperature outside threshold",
            "NVM subsystem reliability degraded",
            "media placed in read-only mode",
            "volatile memory backup failed",
            "persistent memory region read-only or unreliable",
        ];
        FLAGS
            .iter()
            .enumerate()
            .filter(|(bit, _)| self.critical_warning & (1 << bit) != 0)
            .map(|(_, s)| *s)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nvme_warning_flags_decode_each_bit() {
        let h = NvmeHealth {
            critical_warning: 0b0000_0101,
            ..Default::default()
        };
        assert_eq!(
            h.critical_warning_flags(),
            vec![
                "available spare below threshold",
                "NVM subsystem reliability degraded"
            ]
        );
        assert!(NvmeHealth::default().critical_warning_flags().is_empty());
    }

    #[test]
    fn status_serializes_camel_case() {
        let s = serde_json::to_string(&Status::new(Availability::PermissionRequired, "x"))
            .expect("serialize");
        assert_eq!(s, r#"{"state":"permissionRequired","message":"x"}"#);
    }
}

/// Running min/max/average per sensor key since the last reset.
#[derive(Debug, Default)]
pub struct SensorStatsTracker {
    stats: std::collections::HashMap<String, SensorStats>,
}

impl SensorStatsTracker {
    pub fn update(&mut self, readings: &[SensorReading]) {
        for r in readings {
            let Some(v) = r.value.filter(|v| v.is_finite()) else {
                continue;
            };
            self.stats
                .entry(r.key.clone())
                .and_modify(|s| {
                    s.samples += 1;
                    s.min = s.min.min(v);
                    s.max = s.max.max(v);
                    s.avg += (v - s.avg) / s.samples as f64;
                })
                .or_insert(SensorStats {
                    min: v,
                    max: v,
                    avg: v,
                    samples: 1,
                });
        }
    }
    pub fn get(&self, key: &str) -> Option<&SensorStats> {
        self.stats.get(key)
    }
    pub fn reset(&mut self) {
        self.stats.clear();
    }
    /// Drops keys no longer reported so the map cannot grow without bound.
    pub fn retain_keys(&mut self, live: &std::collections::HashSet<&str>) {
        self.stats.retain(|k, _| live.contains(k.as_str()));
    }
}

#[cfg(test)]
mod tracker_tests {
    use super::*;

    fn reading(v: Option<f64>) -> SensorReading {
        SensorReading {
            key: "k".into(),
            provider: "p".into(),
            device: "d".into(),
            category: SensorCategory::Cpu,
            kind: SensorKind::Temperature,
            label: "l".into(),
            unit: "°C".into(),
            value: v,
            timestamp_ms: 0,
        }
    }

    #[test]
    fn tracks_min_max_avg_and_ignores_missing() {
        let mut t = SensorStatsTracker::default();
        for v in [Some(10.0), None, Some(30.0), Some(f64::NAN), Some(20.0)] {
            t.update(&[reading(v)]);
        }
        let s = t.get("k").expect("stats");
        assert_eq!((s.min, s.max, s.avg, s.samples), (10.0, 30.0, 20.0, 3));
        t.retain_keys(&std::collections::HashSet::new());
        assert!(t.get("k").is_none());
    }
}
