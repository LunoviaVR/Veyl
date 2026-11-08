//! NVIDIA GPU sensors through NVML (`nvml.dll`, installed by the NVIDIA driver in
//! System32). Function signatures and enum values from NVIDIA's `nvml.h`.
//! Not verified on NVIDIA hardware (the development machine has an AMD GPU).

use std::ffi::{c_char, c_void, CStr};

use pch_core::{
    Availability, HardwareSensorProvider, ProviderPoll, SensorCategory, SensorKind, SensorReading,
    Status,
};
use windows::core::{s, w, PCSTR};
use windows::Win32::Foundation::{FreeLibrary, HMODULE};
use windows::Win32::System::LibraryLoader::{
    GetProcAddress, LoadLibraryExW, LOAD_LIBRARY_SEARCH_SYSTEM32,
};

type Ret = u32; // nvmlReturn_t; NVML_SUCCESS = 0
type Dev = *mut c_void;
const NVML_TEMPERATURE_GPU: u32 = 0;
const NVML_CLOCK_GRAPHICS: u32 = 0;
const NVML_CLOCK_MEM: u32 = 2;

struct Api {
    module: HMODULE,
    shutdown: unsafe extern "C" fn() -> Ret,
    temperature: unsafe extern "C" fn(Dev, u32, *mut u32) -> Ret,
    power: unsafe extern "C" fn(Dev, *mut u32) -> Ret,
    power_limit: unsafe extern "C" fn(Dev, *mut u32) -> Ret,
    fan: unsafe extern "C" fn(Dev, *mut u32) -> Ret,
    clock: unsafe extern "C" fn(Dev, u32, *mut u32) -> Ret,
    devices: Vec<(Dev, String)>,
}

impl Drop for Api {
    fn drop(&mut self) {
        // SAFETY: nvmlInit_v2 succeeded; shut down once, then unload.
        unsafe {
            (self.shutdown)();
            let _ = FreeLibrary(self.module); // nothing to recover if unloading fails
        }
    }
}

/// Reinterprets an exported function pointer as its documented signature.
///
/// # Safety
/// `T` must be a function-pointer type matching the export's real signature.
pub(crate) unsafe fn fn_cast<T: Copy>(f: unsafe extern "system" fn() -> isize) -> T {
    const { assert!(std::mem::size_of::<T>() == std::mem::size_of::<usize>()) };
    // SAFETY: same size (checked above); caller guarantees the signature.
    unsafe { std::mem::transmute_copy(&f) }
}

fn sym(m: HMODULE, name: PCSTR) -> Option<unsafe extern "system" fn() -> isize> {
    // SAFETY: symbol lookup in a module we loaded.
    unsafe { GetProcAddress(m, name) }
}

fn open() -> Result<Api, Status> {
    // SAFETY: loads the driver-installed DLL from System32 only.
    let module = unsafe { LoadLibraryExW(w!("nvml.dll"), None, LOAD_LIBRARY_SEARCH_SYSTEM32) }
        .map_err(|_| {
            Status::new(
                Availability::ProviderUnavailable,
                "No NVIDIA driver (nvml.dll) is installed",
            )
        })?;
    macro_rules! get {
        ($name:literal) => {
            match sym(module, s!($name)) {
                // SAFETY: the exported function has the nvml.h signature it is transmuted to.
                Some(f) => unsafe { fn_cast(f) },
                None => {
                    // SAFETY: unloading the module we loaded.
                    let _ = unsafe { FreeLibrary(module) };
                    return Err(Status::error(concat!("nvml.dll lacks ", $name)));
                }
            }
        };
    }
    let init: unsafe extern "C" fn() -> Ret = get!("nvmlInit_v2");
    let shutdown: unsafe extern "C" fn() -> Ret = get!("nvmlShutdown");
    let count: unsafe extern "C" fn(*mut u32) -> Ret = get!("nvmlDeviceGetCount_v2");
    let handle: unsafe extern "C" fn(u32, *mut Dev) -> Ret = get!("nvmlDeviceGetHandleByIndex_v2");
    let name: unsafe extern "C" fn(Dev, *mut c_char, u32) -> Ret = get!("nvmlDeviceGetName");
    let temperature = get!("nvmlDeviceGetTemperature");
    let power = get!("nvmlDeviceGetPowerUsage");
    let power_limit = get!("nvmlDeviceGetEnforcedPowerLimit");
    let fan = get!("nvmlDeviceGetFanSpeed");
    let clock = get!("nvmlDeviceGetClockInfo");
    // SAFETY: documented initialization call.
    let r = unsafe { init() };
    if r != 0 {
        // SAFETY: unloading the module we loaded.
        let _ = unsafe { FreeLibrary(module) };
        return Err(Status::new(
            Availability::Unsupported,
            format!("nvmlInit failed ({r})"),
        ));
    }
    let mut api = Api {
        module,
        shutdown,
        temperature,
        power,
        power_limit,
        fan,
        clock,
        devices: Vec::new(),
    };
    let mut n = 0u32;
    // SAFETY: out-pointers to locals; NVML initialized.
    unsafe {
        if count(&mut n) == 0 {
            for i in 0..n {
                let mut d: Dev = std::ptr::null_mut();
                if handle(i, &mut d) == 0 {
                    let mut buf = [0 as c_char; 96];
                    let label = if name(d, buf.as_mut_ptr(), buf.len() as u32) == 0 {
                        CStr::from_ptr(buf.as_ptr()).to_string_lossy().into_owned()
                    } else {
                        format!("NVIDIA GPU {i}")
                    };
                    api.devices.push((d, label));
                }
            }
        }
    }
    if api.devices.is_empty() {
        return Err(Status::new(
            Availability::Unsupported,
            "NVML found no NVIDIA GPU",
        ));
    }
    Ok(api)
}

pub struct NvmlProvider {
    api: Result<Api, Status>,
}

impl NvmlProvider {
    pub fn new() -> Self {
        Self { api: open() }
    }
}

impl Default for NvmlProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl HardwareSensorProvider for NvmlProvider {
    fn id(&self) -> &'static str {
        "nvml"
    }
    fn name(&self) -> &'static str {
        "NVIDIA GPU sensors (NVML)"
    }

    fn poll(&mut self, now_ms: i64) -> ProviderPoll {
        let api = match &self.api {
            Ok(a) => a,
            Err(s) => {
                return ProviderPoll {
                    status: s.clone(),
                    readings: vec![],
                }
            }
        };
        let mut readings = Vec::new();
        for (d, name) in &api.devices {
            let mut push = |label: &str, kind, unit: &str, v: Option<f64>| {
                if v.is_some() {
                    readings.push(SensorReading {
                        key: format!("nvml/{name}/{label}"),
                        provider: "nvml".into(),
                        device: name.clone(),
                        category: SensorCategory::Gpu,
                        kind,
                        label: label.into(),
                        unit: unit.into(),
                        value: v,
                        timestamp_ms: now_ms,
                    })
                }
            };
            let read = |f: &dyn Fn(*mut u32) -> Ret| {
                let mut v = 0u32;
                (f(&mut v) == 0).then_some(f64::from(v))
            };
            use pch_core::unit::*;
            // SAFETY (all calls below): live device handle, out-pointer to a local.
            push(
                "GPU temperature",
                SensorKind::Temperature,
                CELSIUS,
                read(&|p| unsafe { (api.temperature)(*d, NVML_TEMPERATURE_GPU, p) }),
            );
            // nvml.h: power values are milliwatts.
            push(
                "Board power",
                SensorKind::Power,
                WATT,
                read(&|p| unsafe { (api.power)(*d, p) }).map(|v| v / 1000.0),
            );
            push(
                "Power limit",
                SensorKind::Power,
                WATT,
                read(&|p| unsafe { (api.power_limit)(*d, p) }).map(|v| v / 1000.0),
            );
            push(
                "Fan",
                SensorKind::Fan,
                PERCENT,
                read(&|p| unsafe { (api.fan)(*d, p) }),
            );
            push(
                "GPU clock",
                SensorKind::Clock,
                MHZ,
                read(&|p| unsafe { (api.clock)(*d, NVML_CLOCK_GRAPHICS, p) }),
            );
            push(
                "Memory clock",
                SensorKind::Clock,
                MHZ,
                read(&|p| unsafe { (api.clock)(*d, NVML_CLOCK_MEM, p) }),
            );
        }
        ProviderPoll {
            status: Status::available(),
            readings,
        }
    }
}
