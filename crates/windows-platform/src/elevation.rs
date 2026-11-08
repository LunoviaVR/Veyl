//! Privilege state and explicit, user-initiated elevation.

use windows::Win32::Foundation::HANDLE;
use windows::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
use windows::Win32::System::Threading::{
    GetCurrentProcess, GetCurrentProcessId, OpenProcess, OpenProcessToken, WaitForSingleObject,
    PROCESS_SYNCHRONIZE,
};
use windows::Win32::UI::Shell::{ShellExecuteExW, SHELLEXECUTEINFOW};
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowThreadProcessId, SW_SHOWNORMAL,
};

use std::path::{Path, PathBuf};

use crate::handle::{wide, OwnedHandle};
use crate::registry::{read_string, HKEY_LOCAL_MACHINE};

/// Program Files (both) and the Windows folder, read from HKLM because the user
/// can override the matching environment variables.
fn admin_only_roots() -> Vec<PathBuf> {
    let cv = r"SOFTWARE\Microsoft\Windows\CurrentVersion";
    let mut roots: Vec<PathBuf> = ["ProgramFilesDir", "ProgramFilesDir (x86)"]
        .iter()
        .filter_map(|v| read_string(HKEY_LOCAL_MACHINE, cv, v))
        .map(PathBuf::from)
        .collect();
    roots.extend(
        read_string(
            HKEY_LOCAL_MACHINE,
            r"SOFTWARE\Microsoft\Windows NT\CurrentVersion",
            "SystemRoot",
        )
        .map(PathBuf::from),
    );
    roots
}

/// True when `p` lies in a folder that only administrators can change.
// shortcut: a Program Files subfolder whose installer opened it to users still passes; check the ACL if that matters.
fn in_admin_only_folder(p: &Path) -> bool {
    let Ok(real) = std::fs::canonicalize(p) else {
        return false;
    };
    admin_only_roots()
        .iter()
        .filter_map(|r| std::fs::canonicalize(r).ok())
        .any(|r| real.starts_with(r))
}

/// Whether this process may run the program at `p`. Elevated, it only runs programs
/// from admin-only folders: anywhere else, any program running as the user could
/// replace the file (or plant a DLL beside it) and gain administrator rights.
pub fn may_execute(p: &Path) -> bool {
    !is_elevated() || in_admin_only_folder(p)
}

/// Command-line flag `relaunch_elevated` passes to the new instance.
pub const WAIT_PID_ARG: &str = "--wait-pid";

/// Blocks until process `pid` has exited, at most `timeout_ms`.
pub fn wait_for_exit(pid: u32, timeout_ms: u32) {
    // SAFETY: SYNCHRONIZE-only handle, owned below; a PID that is already gone just fails to open.
    if let Ok(h) = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, false, pid) } {
        let h = OwnedHandle(h);
        // SAFETY: valid process handle.
        unsafe { WaitForSingleObject(h.raw(), timeout_ms) };
    }
}

/// True when this process runs with an elevated (administrator) token.
pub fn is_elevated() -> bool {
    let mut token = HANDLE::default();
    // SAFETY: pseudo-handle of the current process; out-pointer to a local.
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) }.is_err() {
        return false;
    }
    let token = OwnedHandle(token);
    let mut e = TOKEN_ELEVATION::default();
    let mut len = 0u32;
    // SAFETY: buffer is a TOKEN_ELEVATION of the passed size.
    let ok = unsafe {
        GetTokenInformation(
            token.raw(),
            TokenElevation,
            Some((&mut e as *mut TOKEN_ELEVATION).cast()),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut len,
        )
    };
    ok.is_ok() && e.TokenIsElevated != 0
}

/// Starts a new elevated instance of this executable through the UAC prompt.
/// The caller exits the current instance after this returns `Ok`.
/// `arg` is a fixed flag chosen by the caller, never user input.
pub fn relaunch_elevated() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| format!("cannot locate executable: {e}"))?;
    // The new instance waits for this one to exit; otherwise the single-instance check
    // would find this one still shutting down, hand over to it and quit.
    // SAFETY: no preconditions.
    let arg = format!("{WAIT_PID_ARG} {}", unsafe { GetCurrentProcessId() });
    let (verb, file, params) = (wide("runas"), wide(&exe.to_string_lossy()), wide(&arg));
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        lpVerb: windows::core::PCWSTR(verb.as_ptr()),
        lpFile: windows::core::PCWSTR(file.as_ptr()),
        lpParameters: windows::core::PCWSTR(params.as_ptr()),
        nShow: SW_SHOWNORMAL.0,
        ..Default::default()
    };
    // SAFETY: struct initialized with valid NUL-terminated strings that outlive the call.
    unsafe { ShellExecuteExW(&mut info) }.map_err(|e| {
        if e.code().0 as u32 == 0x8007_04C7 {
            "The administrator prompt was cancelled.".to_owned()
        } else {
            format!("Could not start an elevated instance: {e}")
        }
    })
}

/// PID owning the foreground window, if any.
pub fn foreground_pid() -> Option<u32> {
    // SAFETY: no preconditions.
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.is_invalid() {
        return None;
    }
    let mut pid = 0u32;
    // SAFETY: valid window handle; out-pointer to a local.
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    (pid != 0).then_some(pid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waits_for_a_process_to_exit() {
        let mut child = std::process::Command::new("cmd.exe")
            .args(["/c", "ping -n 2 127.0.0.1 >nul"])
            .spawn()
            .expect("spawn");
        let t = std::time::Instant::now();
        wait_for_exit(child.id(), 10_000);
        assert!(
            t.elapsed() < std::time::Duration::from_secs(9),
            "returned when the process exited"
        );
        assert!(
            child.try_wait().expect("status").is_some(),
            "the process has exited"
        );
        wait_for_exit(u32::MAX - 3, 10_000); // no such process: returns at once
    }

    #[test]
    fn admin_only_folders() {
        assert!(in_admin_only_folder(Path::new(
            r"C:\Windows\System32\cmd.exe"
        )));
        let user_file = std::env::temp_dir().join(format!("veyl-trust-{}.exe", std::process::id()));
        std::fs::write(&user_file, b"x").expect("write");
        assert!(
            !in_admin_only_folder(&user_file),
            "the user's temp folder is not admin-only"
        );
        assert!(!in_admin_only_folder(Path::new(r"C:\nope\missing.exe")));
        let _ = std::fs::remove_file(&user_file);
    }
}
