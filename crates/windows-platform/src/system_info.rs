//! Static system identification: computer name, Windows version, boot time.

use veyl_core::{CpuInfo, SystemInfo};
use windows::Wdk::System::SystemServices::RtlGetVersion;
use windows::Win32::System::SystemInformation::{
    ComputerNamePhysicalDnsHostname, GetComputerNameExW, GetTickCount64, OSVERSIONINFOW,
};

use crate::handle::from_wide;
use crate::registry::{read_dword, read_string, HKEY_LOCAL_MACHINE};

const NT_CURRENT_VERSION: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion";

pub fn computer_name() -> Option<String> {
    let mut buf = [0u16; 256];
    let mut len = buf.len() as u32;
    // SAFETY: buffer and its length.
    unsafe {
        GetComputerNameExW(
            ComputerNamePhysicalDnsHostname,
            Some(windows::core::PWSTR(buf.as_mut_ptr())),
            &mut len,
        )
    }
    .ok()?;
    Some(from_wide(&buf[..len as usize]))
}

/// "Windows 11 Home 25H2" style name and "26200.1234" build string.
pub fn os_version() -> (Option<String>, Option<String>) {
    let mut v = OSVERSIONINFOW {
        dwOSVersionInfoSize: std::mem::size_of::<OSVERSIONINFOW>() as u32,
        ..Default::default()
    };
    // SAFETY: size field set; out-pointer to a local. RtlGetVersion is not subject to compatibility shims.
    let ok = unsafe { RtlGetVersion(&mut v) }.0 >= 0;
    let edition = read_string(HKEY_LOCAL_MACHINE, NT_CURRENT_VERSION, "EditionID");
    let display = read_string(HKEY_LOCAL_MACHINE, NT_CURRENT_VERSION, "DisplayVersion");
    let ubr = read_dword(HKEY_LOCAL_MACHINE, NT_CURRENT_VERSION, "UBR");
    if !ok {
        return (None, None);
    }
    // Windows 11 kept major version 10; build 22000 is the first Windows 11 build.
    let family = if v.dwMajorVersion == 10 && v.dwBuildNumber >= 22_000 {
        "Windows 11"
    } else {
        "Windows 10"
    };
    let mut name = family.to_owned();
    if let Some(e) = edition {
        name.push(' ');
        name.push_str(match e.as_str() {
            "Core" => "Home",
            "Professional" => "Pro",
            other => other,
        });
    }
    if let Some(d) = display {
        name.push(' ');
        name.push_str(&d);
    }
    let build = match ubr {
        Some(u) => format!("{}.{u}", v.dwBuildNumber),
        None => v.dwBuildNumber.to_string(),
    };
    (Some(name), Some(build))
}

pub fn boot_time_ms(now_ms: i64) -> i64 {
    // SAFETY: no preconditions.
    now_ms - unsafe { GetTickCount64() } as i64
}

pub fn collect(now_ms: i64, cpu: CpuInfo, cpu_status: Vec<String>) -> SystemInfo {
    let (os_name, os_build) = os_version();
    SystemInfo {
        computer_name: computer_name(),
        os_name,
        os_build,
        boot_time_ms: Some(boot_time_ms(now_ms)),
        cpu,
        installed_memory_bytes: crate::memory::total_physical_bytes(),
        gpus: Vec::new(),
        elevated: crate::elevation::is_elevated(),
        status: cpu_status,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn identifies_windows() {
        let (name, build) = super::os_version();
        assert!(name.as_deref().is_some_and(|n| n.starts_with("Windows")));
        assert!(build.is_some());
        assert!(super::computer_name().is_some());
    }
}
