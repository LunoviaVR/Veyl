//! Hardware sensors through PawnIO (https://pawnio.eu), an open-source (GPL-2.0),
//! Microsoft-signed, HVCI-compatible driver that only executes signed modules.
//!
//! Veyl's installer puts the official signed PawnIO setup in `tools`; it runs only when
//! the user clicks Install PawnIO ([`install`]). PawnIO may also come from pawnio.eu or tools
//! such as FanControl and LibreHardwareMonitor.
//! The app talks to it through PawnIOLib.dll (LGPL-2.1) from the PawnIO install
//! folder and loads the official signed modules embedded here (LGPL-2.1, see
//! `pawnio/COPYING`, release 0.2.11, SHA-256 of the release zip
//! 43608cb89bc84247fef1368a139013f7d043e17db6d6c8dfc9b46bf0905a81f4).
//!
//! Opening the driver requires administrator rights.

use veyl_core::{
    Availability, HardwareSensorProvider, ProviderPoll, SensorCategory, SensorKind, SensorReading,
    Status,
};
use windows::core::{s, HRESULT, PCSTR};
use windows::Win32::Foundation::{FreeLibrary, HANDLE, HMODULE};
use windows::Win32::System::LibraryLoader::{
    GetProcAddress, LoadLibraryExW, LOAD_WITH_ALTERED_SEARCH_PATH,
};
use windows::Win32::System::SystemInformation::GROUP_AFFINITY;
use windows::Win32::System::Threading::{GetCurrentThread, SetThreadGroupAffinity};

use crate::handle::wide;
use crate::nvml::fn_cast;
use crate::registry::{read_string, HKEY_LOCAL_MACHINE};

pub const INTEL_MSR: &[u8] = include_bytes!("../pawnio/IntelMSR.bin");
pub const LPC_IO: &[u8] = include_bytes!("../pawnio/LpcIO.bin");

type OpenFn = unsafe extern "system" fn(*mut HANDLE) -> HRESULT;
type LoadFn = unsafe extern "system" fn(HANDLE, *const u8, usize) -> HRESULT;
type ExecFn = unsafe extern "system" fn(
    HANDLE,
    PCSTR,
    *const u64,
    usize,
    *mut u64,
    usize,
    *mut usize,
) -> HRESULT;
type CloseFn = unsafe extern "system" fn(HANDLE) -> HRESULT;

/// Locates PawnIOLib.dll: the PawnIO uninstall entry's InstallLocation, else %ProgramFiles%\PawnIO.
pub fn library_path() -> Option<std::path::PathBuf> {
    let from_registry = read_string(
        HKEY_LOCAL_MACHINE,
        r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\PawnIO",
        "InstallLocation",
    )
    .map(|d| std::path::PathBuf::from(d.trim_matches('"')).join("PawnIOLib.dll"));
    let default = std::env::var_os("ProgramFiles")
        .map(|p| std::path::PathBuf::from(p).join(r"PawnIO\PawnIOLib.dll"));
    [from_registry, default]
        .into_iter()
        .flatten()
        .find(|p| p.is_file())
}

/// The PawnIO setup the installer puts in Veyl's `tools` folder (release 2.2.0).
pub const SETUP_EXE: &str = "PawnIO_setup.exe";

/// Installs the PawnIO driver with the bundled setup (explicit user action).
pub fn install(tools: &std::path::Path) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    if library_path().is_some() {
        return Err("PawnIO is already installed.".into());
    }
    let setup = tools.join(SETUP_EXE);
    if !setup.is_file() {
        return Err(format!(
            "{SETUP_EXE} is missing from {}. Reinstall Veyl to restore it.",
            tools.display()
        ));
    }
    if !crate::elevation::may_execute(&setup) {
        return Err(format!("Veyl runs as administrator, so it only starts programs from folders that only administrators can change, and {} is not one. Reinstall Veyl into Program Files (the default).", tools.display()));
    }
    let status = std::process::Command::new(&setup)
        .args(["-install", "-silent"])
        .creation_flags(crate::presentmon::CREATE_NO_WINDOW)
        .status()
        .map_err(|e| format!("Could not start the PawnIO setup: {e}"))?;
    match status.code() {
        // 3010 = ERROR_SUCCESS_REBOOT_REQUIRED.
        Some(0) => Ok(()),
        Some(3010) => Err("PawnIO is installed. Restart Windows to finish.".into()),
        code => Err(format!(
            "The PawnIO setup failed (error {}).",
            code.unwrap_or(-1)
        )),
    }
}

/// One PawnIO executor with one module loaded.
pub struct Executor {
    module: HMODULE,
    handle: HANDLE,
    exec: ExecFn,
    close: CloseFn,
}

// SAFETY: the executor handle is a kernel handle usable from any thread; PawnIOLib functions are thread-safe
// per the PawnIO design (each call is one DeviceIoControl). It is moved to the sampler thread once.
unsafe impl Send for Executor {}

impl Executor {
    /// Opens the driver and loads `blob`. Errors carry a user-facing status.
    pub fn open(blob: &[u8]) -> Result<Self, Status> {
        let path = library_path().ok_or_else(|| {
            Status::new(Availability::ProviderUnavailable, "PawnIO is not installed. Install it on the Hardware page to add CPU temperatures, motherboard fans and voltages.")
        })?;
        let w = wide(&path.to_string_lossy());
        // SAFETY: absolute path to the DLL in PawnIO's (admin-writable only) install folder.
        let module = unsafe {
            LoadLibraryExW(
                windows::core::PCWSTR(w.as_ptr()),
                None,
                LOAD_WITH_ALTERED_SEARCH_PATH,
            )
        }
        .map_err(|e| Status::error(format!("Could not load PawnIOLib.dll: {e}")))?;
        let get = |name: PCSTR| {
            // SAFETY: symbol lookup in the module loaded above.
            unsafe { GetProcAddress(module, name) }
        };
        let (Some(open), Some(load), Some(exec), Some(close)) = (
            get(s!("pawnio_open")),
            get(s!("pawnio_load")),
            get(s!("pawnio_execute")),
            get(s!("pawnio_close")),
        ) else {
            // SAFETY: unloading the module we loaded.
            let _ = unsafe { FreeLibrary(module) };
            return Err(Status::error(
                "PawnIOLib.dll does not export the expected functions",
            ));
        };
        // SAFETY: signatures from PawnIOLib.h (STDAPICALLTYPE = system ABI on x64).
        let (open, load, exec, close): (OpenFn, LoadFn, ExecFn, CloseFn) =
            unsafe { (fn_cast(open), fn_cast(load), fn_cast(exec), fn_cast(close)) };
        let mut handle = HANDLE::default();
        // SAFETY: out-pointer to a local.
        let hr = unsafe { open(&mut handle) };
        if hr.is_err() {
            // SAFETY: unloading the module we loaded.
            let _ = unsafe { FreeLibrary(module) };
            return Err(if hr.0 as u32 == 0x8007_0005 {
                Status::new(
                    Availability::PermissionRequired,
                    "PawnIO needs administrator rights (run the installed, elevated build)",
                )
            } else {
                Status::error(format!(
                    "pawnio_open failed (0x{:08X}); is the PawnIO driver installed and running?",
                    hr.0 as u32
                ))
            });
        }
        let ex = Self {
            module,
            handle,
            exec,
            close,
        };
        // SAFETY: blob pointer/length from a static slice.
        let hr = unsafe { load(handle, blob.as_ptr(), blob.len()) };
        if hr.is_err() {
            return Err(Status::error(format!(
                "pawnio_load failed (0x{:08X}); the module may be newer than the installed driver",
                hr.0 as u32
            )));
        }
        Ok(ex)
    }

    /// Calls a module ioctl; returns its output values.
    pub fn execute(&self, name: PCSTR, input: &[u64], out_len: usize) -> Result<Vec<u64>, HRESULT> {
        let mut out = vec![0u64; out_len];
        let mut written = 0usize;
        // SAFETY: buffers sized as passed; handle owned by self.
        let hr = unsafe {
            (self.exec)(
                self.handle,
                name,
                input.as_ptr(),
                input.len(),
                out.as_mut_ptr(),
                out.len(),
                &mut written,
            )
        };
        if hr.is_err() {
            return Err(hr);
        }
        out.truncate(written.min(out_len));
        Ok(out)
    }
}

impl Drop for Executor {
    fn drop(&mut self) {
        // SAFETY: closing the executor and unloading the library we loaded, once.
        unsafe {
            let _ = (self.close)(self.handle);
            if let Err(e) = FreeLibrary(self.module) {
                tracing::warn!("FreeLibrary(PawnIOLib) failed: {e}");
            }
        }
    }
}

/// Runs `f` with the current thread pinned to one logical processor, then restores the affinity.
fn on_cpu<T>(group: u16, index: u32, f: impl FnOnce() -> T) -> Option<T> {
    let target = GROUP_AFFINITY {
        Mask: 1usize << index,
        Group: group,
        Reserved: [0; 3],
    };
    let mut previous = GROUP_AFFINITY::default();
    // SAFETY: pseudo-handle of the current thread; affinity structs are locals.
    if !unsafe { SetThreadGroupAffinity(GetCurrentThread(), &target, Some(&mut previous)) }
        .as_bool()
    {
        return None;
    }
    let r = f();
    // SAFETY: restoring the affinity saved above.
    let _ = unsafe { SetThreadGroupAffinity(GetCurrentThread(), &previous, None) };
    Some(r)
}

/// Digital thermal sensor decoding (Intel SDM Vol. 3B §15.8): temperature = TjMax − readout,
/// readout in bits 22:16. IA32_THERM_STATUS additionally has a "reading valid" bit 31.
pub fn decode_dts(tj_max: u64, status: u64, needs_valid_bit: bool) -> Option<f64> {
    if needs_valid_bit && status & (1 << 31) == 0 {
        return None;
    }
    let readout = (status >> 16) & 0x7F;
    (tj_max > readout && tj_max <= 150).then(|| (tj_max - readout) as f64)
}

pub struct IntelCpuTempProvider {
    executor: Result<Executor, Status>,
    tj_max: Option<u64>,
    cores: Vec<(u16, u32, String)>,
}

impl IntelCpuTempProvider {
    pub fn new() -> Self {
        let vendor = crate::cpu::cpu_info().0.vendor.unwrap_or_default();
        if vendor != "GenuineIntel" {
            return Self {
                executor: Err(Status::new(
                    Availability::Unsupported,
                    format!("Intel CPU temperature module does not apply to {vendor}"),
                )),
                tj_max: None,
                cores: vec![],
            };
        }
        let executor = Executor::open(INTEL_MSR);
        let mut tj_max = None;
        if let Ok(ex) = &executor {
            // MSR_TEMPERATURE_TARGET (0x1A2) bits 23:16 = TjMax in °C.
            tj_max = ex
                .execute(s!("ioctl_read_msr"), &[0x1A2], 1)
                .ok()
                .and_then(|v| v.first().copied())
                .map(|v| (v >> 16) & 0xFF)
                .filter(|t| (60..=150).contains(t));
        }
        let cores = crate::cpu::core_list()
            .unwrap_or_default()
            .into_iter()
            .enumerate()
            .map(|(i, c)| {
                let label = match c.kind {
                    crate::cpu::CoreKind::Performance => format!("P-core #{i}"),
                    crate::cpu::CoreKind::Efficiency => format!("E-core #{i}"),
                    crate::cpu::CoreKind::Uniform => format!("Core #{i}"),
                };
                (c.group, c.first_index, label)
            })
            .collect();
        Self {
            executor,
            tj_max,
            cores,
        }
    }
}

impl Default for IntelCpuTempProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl HardwareSensorProvider for IntelCpuTempProvider {
    fn id(&self) -> &'static str {
        "pawnio-cpu"
    }
    fn name(&self) -> &'static str {
        "CPU temperatures (PawnIO · IntelMSR)"
    }

    fn poll(&mut self, now_ms: i64) -> ProviderPoll {
        let ex = match &self.executor {
            Ok(e) => e,
            Err(s) => {
                return ProviderPoll {
                    status: s.clone(),
                    readings: vec![],
                }
            }
        };
        let Some(tj) = self.tj_max else {
            return ProviderPoll {
                status: Status::error("Could not read TjMax (MSR 0x1A2)"),
                readings: vec![],
            };
        };
        let read = |msr: u64| {
            ex.execute(s!("ioctl_read_msr"), &[msr], 1)
                .ok()
                .and_then(|v| v.first().copied())
        };
        let mut readings = Vec::new();
        let mut push = |label: &str, v: Option<f64>| {
            readings.push(SensorReading {
                key: format!("pawnio-cpu/{label}"),
                provider: "pawnio-cpu".into(),
                device: "CPU (digital thermal sensors)".into(),
                category: SensorCategory::Cpu,
                kind: SensorKind::Temperature,
                label: label.into(),
                unit: veyl_core::unit::CELSIUS.into(),
                value: v,
                timestamp_ms: now_ms,
            })
        };
        // Package scope: any core.
        push(
            "CPU Package",
            read(0x1B1).and_then(|s| decode_dts(tj, s, false)),
        );
        let mut core_vals = Vec::new();
        for (group, index, label) in &self.cores {
            let v = on_cpu(*group, *index, || read(0x19C))
                .flatten()
                .and_then(|s| decode_dts(tj, s, true));
            if let Some(v) = v {
                core_vals.push(v);
            }
            push(label, v);
        }
        if !core_vals.is_empty() {
            push("Core max", core_vals.iter().copied().reduce(f64::max));
            push(
                "Core average",
                Some(core_vals.iter().sum::<f64>() / core_vals.len() as f64),
            );
        }
        push("TjMax", Some(tj as f64));
        ProviderPoll {
            status: Status::available(),
            readings,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dts_decoding() {
        // TjMax 100, readout 45 -> 55 °C; valid bit required for core status.
        let core = (1u64 << 31) | (45 << 16);
        assert_eq!(decode_dts(100, core, true), Some(55.0));
        assert_eq!(decode_dts(100, 45 << 16, true), None, "invalid reading");
        assert_eq!(
            decode_dts(100, 45 << 16, false),
            Some(55.0),
            "package status has no valid bit"
        );
        assert_eq!(decode_dts(0, core, true), None);
    }

    #[test]
    fn provider_degrades_gracefully() {
        let mut p = IntelCpuTempProvider::new();
        let poll = p.poll(0);
        // Unelevated test runs report PermissionRequired / ProviderUnavailable; elevated runs report readings.
        if poll.status.is_available() {
            assert!(poll
                .readings
                .iter()
                .any(|r| r.label == "CPU Package" && r.value.is_some()));
        }
    }
}
