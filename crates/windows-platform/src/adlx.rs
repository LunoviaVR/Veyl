//! AMD GPU sensors through AMD's ADLX SDK runtime (`amdadlx64.dll`, installed by
//! the Adrenalin driver; SDK is MIT-licensed). Adds what WDDM does not expose:
//! hotspot, board/ASIC power, core voltage, intake temperature and fan RPM.
//!
//! ADLX exposes C-compatible interfaces as `struct { const Vtbl* pVtbl; }`. The
//! vtable structs below mirror the method order of ADLX SDK 2.0
//! (`SDK/Include/ISystem.h`, `IPerformanceMonitoring.h`); only the prefix up to the
//! last method used is declared, which keeps every used offset correct. On x64,
//! `__stdcall` and `__cdecl` are the same calling convention as `extern "system"`.

use std::ffi::{c_char, CStr};

use veyl_core::{
    Availability, HardwareSensorProvider, ProviderPoll, SensorCategory, SensorKind, SensorReading,
    Status,
};
use windows::core::{s, w};
use windows::Win32::Foundation::{FreeLibrary, HMODULE};
use windows::Win32::System::LibraryLoader::{
    GetProcAddress, LoadLibraryExW, LOAD_LIBRARY_SEARCH_SYSTEM32,
};

type AdlxResult = i32;
/// ADLXDefines.h: ADLX_OK = 0, ADLX_ALREADY_ENABLED = 1, ADLX_ALREADY_INITIALIZED = 2 count as success.
fn ok(r: AdlxResult) -> bool {
    (0..=2).contains(&r)
}
/// ADLXVersion.h 2.0.0.125, packed as ADLX_MAKE_FULL_VER (major<<48 | minor<<32 | release<<16 | build).
const ADLX_FULL_VERSION: u64 = (2u64 << 48) | 125;

#[repr(C)]
struct System {
    vtbl: *const SystemVtbl,
}
#[repr(C)]
struct SystemVtbl {
    get_hybrid_graphics_type: usize,
    get_gpus: unsafe extern "system" fn(*mut System, *mut *mut GpuList) -> AdlxResult,
    query_interface: usize,
    get_displays_services: usize,
    get_desktops_services: usize,
    get_gpus_changed_handling: usize,
    enable_log: usize,
    get_3d_settings_services: usize,
    get_gpu_tuning_services: usize,
    get_performance_monitoring_services:
        unsafe extern "system" fn(*mut System, *mut *mut PerfServices) -> AdlxResult,
}

#[repr(C)]
struct GpuList {
    vtbl: *const GpuListVtbl,
}
#[repr(C)]
struct GpuListVtbl {
    acquire: usize,
    release: unsafe extern "system" fn(*mut GpuList) -> i32,
    query_interface: usize,
    size: unsafe extern "system" fn(*mut GpuList) -> u32,
    empty: usize,
    begin: usize,
    end: usize,
    at: usize,
    clear: usize,
    remove_back: usize,
    add_back: usize,
    at_gpu_list: unsafe extern "system" fn(*mut GpuList, u32, *mut *mut Gpu) -> AdlxResult,
}

#[repr(C)]
struct Gpu {
    vtbl: *const GpuVtbl,
}
#[repr(C)]
struct GpuVtbl {
    acquire: usize,
    release: unsafe extern "system" fn(*mut Gpu) -> i32,
    query_interface: usize,
    vendor_id: usize,
    asic_family_type: usize,
    gpu_type: usize,
    is_external: usize,
    name: unsafe extern "system" fn(*mut Gpu, *mut *const c_char) -> AdlxResult,
}

#[repr(C)]
struct PerfServices {
    vtbl: *const PerfServicesVtbl,
}
#[repr(C)]
struct PerfServicesVtbl {
    acquire: usize,
    release: unsafe extern "system" fn(*mut PerfServices) -> i32,
    query_interface: usize,
    get_sampling_interval_range: usize,
    set_sampling_interval: usize,
    get_sampling_interval: usize,
    get_max_history_size_range: usize,
    set_max_history_size: usize,
    get_max_history_size: usize,
    clear_history: usize,
    get_current_history_size: usize,
    start_tracking: usize,
    stop_tracking: usize,
    get_all_metrics_history: usize,
    get_gpu_metrics_history: usize,
    get_system_metrics_history: usize,
    get_fps_history: usize,
    get_current_all_metrics: usize,
    get_current_gpu_metrics:
        unsafe extern "system" fn(*mut PerfServices, *mut Gpu, *mut *mut Metrics) -> AdlxResult,
}

#[repr(C)]
struct Metrics {
    vtbl: *const MetricsVtbl,
}
#[repr(C)]
struct MetricsVtbl {
    acquire: usize,
    release: unsafe extern "system" fn(*mut Metrics) -> i32,
    query_interface: usize,
    time_stamp: usize,
    gpu_usage: unsafe extern "system" fn(*mut Metrics, *mut f64) -> AdlxResult,
    gpu_clock_speed: unsafe extern "system" fn(*mut Metrics, *mut i32) -> AdlxResult,
    gpu_vram_clock_speed: unsafe extern "system" fn(*mut Metrics, *mut i32) -> AdlxResult,
    gpu_temperature: unsafe extern "system" fn(*mut Metrics, *mut f64) -> AdlxResult,
    gpu_hotspot_temperature: unsafe extern "system" fn(*mut Metrics, *mut f64) -> AdlxResult,
    gpu_power: unsafe extern "system" fn(*mut Metrics, *mut f64) -> AdlxResult,
    gpu_total_board_power: unsafe extern "system" fn(*mut Metrics, *mut f64) -> AdlxResult,
    gpu_fan_speed: unsafe extern "system" fn(*mut Metrics, *mut i32) -> AdlxResult,
    gpu_vram: unsafe extern "system" fn(*mut Metrics, *mut i32) -> AdlxResult,
    gpu_voltage: unsafe extern "system" fn(*mut Metrics, *mut i32) -> AdlxResult,
    gpu_intake_temperature: unsafe extern "system" fn(*mut Metrics, *mut f64) -> AdlxResult,
}

type InitFn = unsafe extern "C" fn(u64, *mut *mut System) -> AdlxResult;
type TermFn = unsafe extern "C" fn() -> AdlxResult;

struct Session {
    module: HMODULE,
    terminate: TermFn,
    perf: *mut PerfServices,
    gpus: Vec<(*mut Gpu, String)>,
}

impl Drop for Session {
    fn drop(&mut self) {
        // SAFETY: each object was obtained from ADLX and is released exactly once, before ADLXTerminate.
        unsafe {
            for (g, _) in self.gpus.drain(..) {
                ((*(*g).vtbl).release)(g);
            }
            if !self.perf.is_null() {
                ((*(*self.perf).vtbl).release)(self.perf);
            }
            let r = (self.terminate)();
            if !ok(r) {
                tracing::warn!("ADLXTerminate returned {r}");
            }
            if let Err(e) = FreeLibrary(self.module) {
                tracing::warn!("FreeLibrary(amdadlx64) failed: {e}");
            }
        }
    }
}

fn open() -> Result<Session, Status> {
    // SAFETY: loads the driver-installed DLL from System32 only (no search-path hijacking).
    let module = unsafe { LoadLibraryExW(w!("amdadlx64.dll"), None, LOAD_LIBRARY_SEARCH_SYSTEM32) }
        .map_err(|_| {
            Status::new(
                Availability::ProviderUnavailable,
                "No AMD driver with ADLX is installed",
            )
        })?;
    let unload = |m: HMODULE| {
        // SAFETY: unloading the module we loaded above.
        let _ = unsafe { FreeLibrary(m) };
    };
    // SAFETY: symbol lookup in the loaded module; signatures per ADLXDefines.h ADLXInitialize_Fn / ADLXTerminate_Fn.
    let (init, term) = unsafe {
        (
            GetProcAddress(module, s!("ADLXInitialize")),
            GetProcAddress(module, s!("ADLXTerminate")),
        )
    };
    let (Some(init), Some(term)) = (init, term) else {
        unload(module);
        return Err(Status::error(
            "amdadlx64.dll does not export ADLXInitialize/ADLXTerminate",
        ));
    };
    // SAFETY: transmuting exported function pointers to their documented signatures.
    let (init, terminate): (InitFn, TermFn) =
        unsafe { (crate::nvml::fn_cast(init), crate::nvml::fn_cast(term)) };
    let mut system: *mut System = std::ptr::null_mut();
    // SAFETY: out-pointer to a local; version from the SDK headers.
    let r = unsafe { init(ADLX_FULL_VERSION, &mut system) };
    if !ok(r) || system.is_null() {
        unload(module);
        return Err(Status::new(
            Availability::Unsupported,
            format!("ADLXInitialize failed ({r}); the driver may be too old"),
        ));
    }
    let mut session = Session {
        module,
        terminate,
        perf: std::ptr::null_mut(),
        gpus: Vec::new(),
    };
    // SAFETY: `system` is the singleton returned by ADLXInitialize; vtable layout per ISystem.h.
    unsafe {
        let r = ((*(*system).vtbl).get_performance_monitoring_services)(system, &mut session.perf);
        if !ok(r) || session.perf.is_null() {
            return Err(Status::new(
                Availability::Unsupported,
                format!("ADLX performance monitoring unavailable ({r})"),
            ));
        }
        let mut list: *mut GpuList = std::ptr::null_mut();
        let r = ((*(*system).vtbl).get_gpus)(system, &mut list);
        if !ok(r) || list.is_null() {
            return Err(Status::new(
                Availability::Unsupported,
                format!("ADLX found no AMD GPU ({r})"),
            ));
        }
        let n = ((*(*list).vtbl).size)(list);
        for i in 0..n {
            let mut gpu: *mut Gpu = std::ptr::null_mut();
            if ok(((*(*list).vtbl).at_gpu_list)(list, i, &mut gpu)) && !gpu.is_null() {
                let mut name: *const c_char = std::ptr::null();
                let label = if ok(((*(*gpu).vtbl).name)(gpu, &mut name)) && !name.is_null() {
                    CStr::from_ptr(name).to_string_lossy().trim().to_owned()
                } else {
                    format!("AMD GPU {i}")
                };
                session.gpus.push((gpu, label));
            }
        }
        ((*(*list).vtbl).release)(list);
    }
    if session.gpus.is_empty() {
        return Err(Status::new(
            Availability::Unsupported,
            "ADLX found no AMD GPU",
        ));
    }
    Ok(session)
}

pub struct AdlxProvider {
    session: Option<Session>,
    init_status: Option<Status>,
}

impl AdlxProvider {
    pub fn new() -> Self {
        match open() {
            Ok(s) => Self {
                session: Some(s),
                init_status: None,
            },
            Err(st) => Self {
                session: None,
                init_status: Some(st),
            },
        }
    }
}

impl Default for AdlxProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl HardwareSensorProvider for AdlxProvider {
    fn id(&self) -> &'static str {
        "adlx"
    }
    fn name(&self) -> &'static str {
        "AMD GPU sensors (ADLX)"
    }

    fn poll(&mut self, now_ms: i64) -> ProviderPoll {
        let Some(s) = &self.session else {
            return ProviderPoll {
                status: self.init_status.clone().unwrap_or_default(),
                readings: vec![],
            };
        };
        let mut readings = Vec::new();
        let mut failures = 0;
        for (gpu, name) in &s.gpus {
            let mut m: *mut Metrics = std::ptr::null_mut();
            // SAFETY: perf services and GPU are live ADLX objects owned by the session.
            let r = unsafe { ((*(*s.perf).vtbl).get_current_gpu_metrics)(s.perf, *gpu, &mut m) };
            if !ok(r) || m.is_null() {
                failures += 1;
                continue;
            }
            let mut push = |label: &str, kind: SensorKind, unit: &str, v: Option<f64>| {
                readings.push(SensorReading {
                    key: format!("adlx/{name}/{label}"),
                    provider: "adlx".into(),
                    device: name.clone(),
                    category: SensorCategory::Gpu,
                    kind,
                    label: label.into(),
                    unit: unit.into(),
                    value: v,
                    timestamp_ms: now_ms,
                });
            };
            // SAFETY: `m` is a live metrics object; each getter writes to a local. Unsupported metrics return
            // ADLX_NOT_SUPPORTED and are skipped rather than reported as zero.
            unsafe {
                let vt = &*(*m).vtbl;
                let f = |g: unsafe extern "system" fn(*mut Metrics, *mut f64) -> AdlxResult| {
                    let mut v = 0.0;
                    ok(g(m, &mut v)).then_some(v)
                };
                let i = |g: unsafe extern "system" fn(*mut Metrics, *mut i32) -> AdlxResult| {
                    let mut v = 0;
                    ok(g(m, &mut v)).then_some(f64::from(v))
                };
                use veyl_core::unit::*;
                if let Some(v) = f(vt.gpu_temperature) {
                    push("GPU temperature", SensorKind::Temperature, CELSIUS, Some(v));
                }
                if let Some(v) = f(vt.gpu_hotspot_temperature) {
                    push(
                        "GPU hotspot temperature",
                        SensorKind::Temperature,
                        CELSIUS,
                        Some(v),
                    );
                }
                if let Some(v) = f(vt.gpu_intake_temperature) {
                    push(
                        "Intake temperature",
                        SensorKind::Temperature,
                        CELSIUS,
                        Some(v),
                    );
                }
                if let Some(v) = f(vt.gpu_total_board_power) {
                    push("Total board power", SensorKind::Power, WATT, Some(v));
                }
                if let Some(v) = f(vt.gpu_power) {
                    push("GPU (ASIC) power", SensorKind::Power, WATT, Some(v));
                }
                // ADLX reports voltage in millivolts.
                if let Some(v) = i(vt.gpu_voltage) {
                    push("Core voltage", SensorKind::Voltage, VOLT, Some(v / 1000.0));
                }
                if let Some(v) = i(vt.gpu_fan_speed) {
                    push("Fan", SensorKind::Fan, RPM, Some(v));
                }
                if let Some(v) = i(vt.gpu_clock_speed) {
                    push("GPU clock", SensorKind::Clock, MHZ, Some(v));
                }
                if let Some(v) = i(vt.gpu_vram_clock_speed) {
                    push("Memory clock", SensorKind::Clock, MHZ, Some(v));
                }
                if let Some(v) = f(vt.gpu_usage) {
                    push("GPU usage", SensorKind::Usage, PERCENT, Some(v));
                }
                // ADLX reports VRAM in MB.
                if let Some(v) = i(vt.gpu_vram) {
                    push(
                        "VRAM used",
                        SensorKind::Other,
                        BYTES,
                        Some(v * 1024.0 * 1024.0),
                    );
                }
                (vt.release)(m);
            }
        }
        let status = if failures == s.gpus.len() {
            Status::error("ADLX did not return GPU metrics")
        } else {
            Status::available()
        };
        ProviderPoll { status, readings }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_packing() {
        assert_eq!(ADLX_FULL_VERSION >> 48, 2);
        assert_eq!(ADLX_FULL_VERSION & 0xFFFF, 125);
    }

    #[test]
    fn provider_never_errors_without_amd_and_reads_with_amd() {
        let mut p = AdlxProvider::new();
        let poll = p.poll(0);
        match poll.status.state {
            Availability::Available => assert!(!poll.readings.is_empty()),
            Availability::ProviderUnavailable | Availability::Unsupported => {}
            other => panic!("unexpected {other:?}: {:?}", poll.status.message),
        }
    }
}
