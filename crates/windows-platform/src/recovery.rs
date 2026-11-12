//! Restarting into Safe Mode or the Windows Recovery Environment, on an explicit user request.
//! Uses Windows' own bcdedit.exe and shutdown.exe from System32; changing the boot entry needs
//! administrator rights.

use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use windows::core::PCWSTR;
use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
use windows::Win32::System::Registry::{RegDeleteKeyValueW, RegSetKeyValueW, REG_SZ};
use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CLEANBOOT};

use crate::handle::wide;
use crate::presentmon::CREATE_NO_WINDOW;
use crate::registry::{read_string, HKEY_LOCAL_MACHINE};

/// Where Windows starts after the restart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// Safe Mode (minimal drivers), for one boot.
    SafeMode,
    /// Safe Mode with networking, for one boot.
    SafeModeNetworking,
    /// The Windows Recovery Environment: Startup Settings, System Restore, Startup Repair, …
    Recovery,
    /// Normal Windows: leaves Safe Mode.
    Normal,
}

/// How the running Windows was started.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootMode {
    Normal,
    SafeMode,
    SafeModeNetworking,
}

const RUN_ONCE: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\RunOnce";
/// The `*` makes Windows run it in Safe Mode too: at the first logon there it clears the Safe Mode
/// flag, so the restart after that boots normally.
const LEAVE_SAFE_MODE: &str = "*VeylLeaveSafeMode";

pub fn boot_mode() -> BootMode {
    // SAFETY: no preconditions.
    match unsafe { GetSystemMetrics(SM_CLEANBOOT) } {
        1 => BootMode::SafeMode,
        2 => BootMode::SafeModeNetworking,
        _ => BootMode::Normal,
    }
}

/// True when Veyl set the next boot to Safe Mode and Windows has not restarted yet
/// (for example, the restart was cancelled on the "apps are open" screen).
pub fn safe_mode_pending() -> bool {
    boot_mode() == BootMode::Normal
        && read_string(HKEY_LOCAL_MACHINE, RUN_ONCE, LEAVE_SAFE_MODE).is_some()
}

/// Restarts Windows into `target` now. Apps with unsaved work can still hold up or cancel it.
pub fn restart_into(target: Target) -> Result<(), String> {
    match target {
        Target::Recovery => restart(&["/r", "/o", "/t", "0"]),
        Target::Normal => {
            // Fails when Safe Mode was picked from the boot menu, which sets no flag: nothing to clear.
            let _ = cancel_safe_mode();
            restart(&["/r", "/t", "0"])
        }
        Target::SafeMode | Target::SafeModeNetworking => {
            let kind = if target == Target::SafeMode {
                "minimal"
            } else {
                "network"
            };
            bcdedit(&["/set", "{current}", "safeboot", kind])?;
            set_leave_safe_mode(true)
                .and_then(|()| restart(&["/r", "/t", "0"]))
                .inspect_err(|_| {
                    let _ = cancel_safe_mode();
                })
        }
    }
}

/// Clears a Safe Mode boot set by Veyl that has not happened yet.
pub fn cancel_safe_mode() -> Result<(), String> {
    bcdedit(&["/deletevalue", "{current}", "safeboot"])?;
    set_leave_safe_mode(false)
}

fn system32() -> Result<PathBuf, String> {
    read_string(
        HKEY_LOCAL_MACHINE,
        r"SOFTWARE\Microsoft\Windows NT\CurrentVersion",
        "SystemRoot",
    )
    .map(|r| PathBuf::from(r).join("System32"))
    .ok_or_else(|| "Cannot find the Windows folder".to_owned())
}

fn bcdedit(args: &[&str]) -> Result<(), String> {
    run("bcdedit.exe", args)
}

/// No /f: Windows asks apps with unsaved work to close, so nothing is lost silently.
fn restart(args: &[&str]) -> Result<(), String> {
    run("shutdown.exe", args)
}

/// Runs a System32 tool; the error carries the tool's own message.
fn run(tool: &str, args: &[&str]) -> Result<(), String> {
    let out = Command::new(system32()?.join(tool))
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("Could not run {tool}: {e}"))?;
    if out.status.success() {
        return Ok(());
    }
    // shortcut: the tools print in the OEM code page, so non-ASCII letters come out garbled; decode with MultiByteToWideChar if that matters.
    let text = if out.stderr.is_empty() {
        &out.stdout
    } else {
        &out.stderr
    };
    Err(format!(
        "{tool} failed: {}",
        String::from_utf8_lossy(text).trim()
    ))
}

fn set_leave_safe_mode(on: bool) -> Result<(), String> {
    let (key, name) = (wide(RUN_ONCE), wide(LEAVE_SAFE_MODE));
    let rc = if on {
        let bcd = system32()?.join("bcdedit.exe");
        let cmd = wide(&format!(
            "\"{}\" /deletevalue {{current}} safeboot",
            bcd.display()
        ));
        // SAFETY: NUL-terminated strings; `cmd` holds `cmd.len() * 2` bytes including its NUL.
        unsafe {
            RegSetKeyValueW(
                HKEY_LOCAL_MACHINE,
                PCWSTR(key.as_ptr()),
                PCWSTR(name.as_ptr()),
                REG_SZ.0,
                Some(cmd.as_ptr().cast()),
                (cmd.len() * 2) as u32,
            )
        }
    } else {
        // SAFETY: NUL-terminated strings.
        unsafe {
            RegDeleteKeyValueW(
                HKEY_LOCAL_MACHINE,
                PCWSTR(key.as_ptr()),
                PCWSTR(name.as_ptr()),
            )
        }
    };
    if rc == ERROR_SUCCESS || (!on && rc == ERROR_FILE_NOT_FOUND) {
        Ok(())
    } else {
        Err(format!(
            "Could not update the Safe Mode exit entry (error {})",
            rc.0
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_boot_state() {
        // The test machine is booted normally; the probes must not fail or change anything.
        assert_eq!(boot_mode(), BootMode::Normal);
        let _ = safe_mode_pending();
        assert!(system32()
            .expect("windows folder")
            .join("bcdedit.exe")
            .exists());
    }
}
