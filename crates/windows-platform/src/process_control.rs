//! Process management: details, end process / tree, priority.
//!
//! Safeguards, in order, for every action:
//! 1. Refuse the idle and System pseudo-processes and this application itself.
//! 2. Refuse processes Windows marks critical (`IsProcessCritical`) and known
//!    session-critical system processes by name.
//! 3. Re-open the process and compare its creation time with the one the user
//!    saw in the table; a mismatch means the PID was reused, so refuse.
//!
//! The UI additionally asks for confirmation before any destructive action.

use serde::{Deserialize, Serialize};
use windows::Wdk::System::Threading::{NtQueryInformationProcess, ProcessCommandLineInformation};
use windows::Win32::Foundation::{ERROR_ACCESS_DENIED, FILETIME, UNICODE_STRING};
use windows::Win32::System::Threading::{
    GetCurrentProcessId, GetPriorityClass, GetProcessTimes, IsProcessCritical, OpenProcess,
    SetPriorityClass, TerminateProcess, ABOVE_NORMAL_PRIORITY_CLASS, BELOW_NORMAL_PRIORITY_CLASS,
    HIGH_PRIORITY_CLASS, IDLE_PRIORITY_CLASS, NORMAL_PRIORITY_CLASS, PROCESS_ACCESS_RIGHTS,
    PROCESS_CREATION_FLAGS, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SET_INFORMATION,
    PROCESS_TERMINATE, REALTIME_PRIORITY_CLASS,
};

use crate::handle::OwnedHandle;
use crate::processes::image_path;
use crate::version_info::{file_version_info, FileVersionInfo};

/// Processes whose termination logs the user off or crashes Windows.
const PROTECTED_NAMES: [&str; 10] = [
    "system",
    "registry",
    "memory compression",
    "smss.exe",
    "csrss.exe",
    "wininit.exe",
    "winlogon.exe",
    "services.exe",
    "lsass.exe",
    "secure system",
];

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Priority {
    Idle,
    BelowNormal,
    Normal,
    AboveNormal,
    High,
    Realtime,
}

impl Priority {
    fn flags(self) -> PROCESS_CREATION_FLAGS {
        match self {
            Priority::Idle => IDLE_PRIORITY_CLASS,
            Priority::BelowNormal => BELOW_NORMAL_PRIORITY_CLASS,
            Priority::Normal => NORMAL_PRIORITY_CLASS,
            Priority::AboveNormal => ABOVE_NORMAL_PRIORITY_CLASS,
            Priority::High => HIGH_PRIORITY_CLASS,
            Priority::Realtime => REALTIME_PRIORITY_CLASS,
        }
    }
    fn from_flags(v: u32) -> Option<Self> {
        [
            Priority::Idle,
            Priority::BelowNormal,
            Priority::Normal,
            Priority::AboveNormal,
            Priority::High,
            Priority::Realtime,
        ]
        .into_iter()
        .find(|p| p.flags().0 == v)
    }
}

#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum ControlError {
    #[error("{0}")]
    Refused(String),
    #[error("Access denied. The process is protected or runs with higher privileges; restarting Veyl as administrator may allow this.")]
    AccessDenied,
    #[error("The process has already exited.")]
    Exited,
    #[error(
        "PID {0} now belongs to a different process (the original exited). Nothing was changed."
    )]
    PidReused(u32),
    #[error("{0}")]
    Os(String),
}

/// Serialized as its user-facing message.
impl Serialize for ControlError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProcessDetails {
    pub pid: u32,
    pub path: Option<String>,
    pub command_line: Option<String>,
    pub priority: Option<Priority>,
    pub critical: Option<bool>,
    pub protected_by_policy: bool,
    pub file: Option<FileVersionInfo>,
    pub access_note: Option<String>,
}

fn open(pid: u32, access: PROCESS_ACCESS_RIGHTS) -> Result<OwnedHandle, ControlError> {
    // SAFETY: plain OpenProcess; handle owned by the returned wrapper.
    match unsafe { OpenProcess(access, false, pid) } {
        Ok(h) => Ok(OwnedHandle(h)),
        Err(e) if e.code() == ERROR_ACCESS_DENIED.to_hresult() => Err(ControlError::AccessDenied),
        // ERROR_INVALID_PARAMETER: no such PID.
        Err(e) if e.code().0 as u32 == 0x8007_0057 => Err(ControlError::Exited),
        Err(e) => Err(ControlError::Os(format!("OpenProcess failed: {e}"))),
    }
}

fn creation_ms(h: &OwnedHandle) -> Result<Option<i64>, ControlError> {
    let (mut c, mut e, mut k, mut u) = (
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
    );
    // SAFETY: valid handle with query access; out-pointers to locals.
    unsafe { GetProcessTimes(h.raw(), &mut c, &mut e, &mut k, &mut u) }
        .map_err(|e| ControlError::Os(format!("GetProcessTimes failed: {e}")))?;
    let ft = (u64::from(c.dwHighDateTime) << 32) | u64::from(c.dwLowDateTime);
    Ok(veyl_core::time::filetime_to_unix_ms(ft))
}

fn is_critical(h: &OwnedHandle) -> Option<bool> {
    let mut b = windows::core::BOOL(0);
    // SAFETY: valid handle; out-pointer to a local.
    unsafe { IsProcessCritical(h.raw(), &mut b) }
        .ok()
        .map(|_| b.as_bool())
}

/// Policy check that needs no OS access.
pub fn refusal_reason(pid: u32, name: &str) -> Option<String> {
    // SAFETY: no preconditions.
    let own = unsafe { GetCurrentProcessId() };
    if pid == 0 || pid == 4 {
        return Some("The System and System Idle processes cannot be ended.".into());
    }
    if pid == own {
        return Some("Use Exit from the tray menu to close Veyl.".into());
    }
    if PROTECTED_NAMES.contains(&name.to_ascii_lowercase().as_str()) {
        return Some(format!("{name} is a critical Windows process; ending it would crash Windows or end your session."));
    }
    None
}

/// Opens `pid` and verifies it is the same process the caller saw.
fn open_verified(
    pid: u32,
    start_time_ms: Option<i64>,
    access: PROCESS_ACCESS_RIGHTS,
) -> Result<OwnedHandle, ControlError> {
    let h = open(pid, access | PROCESS_QUERY_LIMITED_INFORMATION)?;
    let actual = creation_ms(&h)?;
    if start_time_ms.is_none() || actual != start_time_ms {
        return Err(ControlError::PidReused(pid));
    }
    Ok(h)
}

pub fn end_process(pid: u32, name: &str, start_time_ms: Option<i64>) -> Result<(), ControlError> {
    if let Some(r) = refusal_reason(pid, name) {
        return Err(ControlError::Refused(r));
    }
    let h = open_verified(pid, start_time_ms, PROCESS_TERMINATE)?;
    if is_critical(&h) == Some(true) {
        return Err(ControlError::Refused(format!(
            "{name} is marked critical by Windows; ending it would stop the system."
        )));
    }
    // SAFETY: handle opened with PROCESS_TERMINATE and verified identity.
    unsafe { TerminateProcess(h.raw(), 1) }.map_err(|e| {
        if e.code() == ERROR_ACCESS_DENIED.to_hresult() {
            ControlError::AccessDenied
        } else {
            ControlError::Os(format!("TerminateProcess failed: {e}"))
        }
    })
}

pub fn set_priority(
    pid: u32,
    name: &str,
    start_time_ms: Option<i64>,
    priority: Priority,
) -> Result<(), ControlError> {
    if let Some(r) = refusal_reason(pid, name) {
        return Err(ControlError::Refused(r));
    }
    if priority == Priority::Realtime {
        return Err(ControlError::Refused(
            "Realtime priority can starve input and disk drivers and is not offered.".into(),
        ));
    }
    let h = open_verified(pid, start_time_ms, PROCESS_SET_INFORMATION)?;
    // SAFETY: handle has PROCESS_SET_INFORMATION; identity verified.
    unsafe { SetPriorityClass(h.raw(), priority.flags()) }.map_err(|e| {
        if e.code() == ERROR_ACCESS_DENIED.to_hresult() {
            ControlError::AccessDenied
        } else {
            ControlError::Os(format!("SetPriorityClass failed: {e}"))
        }
    })
}

fn command_line(h: &OwnedHandle) -> Option<String> {
    let mut buf = vec![0u64; 8192];
    let mut needed = 0u32;
    for _ in 0..2 {
        // SAFETY: buffer of the given size, 8-byte aligned; handle has query-limited access (sufficient since Windows 8.1).
        let st = unsafe {
            NtQueryInformationProcess(
                h.raw(),
                ProcessCommandLineInformation,
                buf.as_mut_ptr().cast(),
                (buf.len() * 8) as u32,
                &mut needed,
            )
        };
        if st.0 >= 0 {
            // SAFETY: on success the buffer starts with a UNICODE_STRING whose Buffer points inside it.
            let us = unsafe { &*buf.as_ptr().cast::<UNICODE_STRING>() };
            let start = (us.Buffer.0 as usize).wrapping_sub(buf.as_ptr() as usize);
            let len = usize::from(us.Length) / 2;
            if us.Buffer.is_null() || start + len * 2 > buf.len() * 8 {
                return None;
            }
            // SAFETY: bounds checked against the buffer just above.
            let s = unsafe { std::slice::from_raw_parts(us.Buffer.0, len) };
            return Some(String::from_utf16_lossy(s));
        }
        buf = vec![0u64; (needed as usize).div_ceil(8) + 1];
    }
    None
}

pub fn details(pid: u32, name: &str) -> ProcessDetails {
    let path = image_path(pid);
    let mut d = ProcessDetails {
        pid,
        file: path.as_deref().and_then(file_version_info),
        path,
        command_line: None,
        priority: None,
        critical: None,
        protected_by_policy: refusal_reason(pid, name).is_some(),
        access_note: None,
    };
    match open(pid, PROCESS_QUERY_LIMITED_INFORMATION) {
        Ok(h) => {
            d.command_line = command_line(&h);
            // SAFETY: valid handle.
            d.priority = Priority::from_flags(unsafe { GetPriorityClass(h.raw()) });
            d.critical = is_critical(&h);
        }
        Err(e) => d.access_note = Some(e.to_string()),
    }
    d
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_system_self_and_critical_names() {
        assert!(refusal_reason(0, "System Idle Process").is_some());
        assert!(refusal_reason(4, "System").is_some());
        assert!(refusal_reason(std::process::id(), "x.exe").is_some());
        assert!(refusal_reason(9999, "CSRSS.EXE").is_some());
        assert!(refusal_reason(9999, "notepad.exe").is_none());
    }

    #[test]
    fn pid_reuse_is_detected() {
        let own = std::process::id();
        assert_eq!(
            open_verified(own, Some(1), PROCESS_QUERY_LIMITED_INFORMATION).err(),
            Some(ControlError::PidReused(own))
        );
        assert_eq!(
            open_verified(own, None, PROCESS_QUERY_LIMITED_INFORMATION).err(),
            Some(ControlError::PidReused(own))
        );
    }

    #[test]
    fn details_of_own_process() {
        let d = details(std::process::id(), "test.exe");
        assert!(d.command_line.is_some());
        assert_eq!(d.critical, Some(false));
        assert!(d.priority.is_some());
    }

    #[test]
    fn end_and_priority_on_a_child_process() {
        let mut child = std::process::Command::new("cmd.exe")
            .args(["/c", "ping -n 30 127.0.0.1 >nul"])
            .spawn()
            .expect("spawn");
        let pid = child.id();
        let h = open(pid, PROCESS_QUERY_LIMITED_INFORMATION).expect("open");
        let start = creation_ms(&h).expect("times");
        drop(h);
        set_priority(pid, "cmd.exe", start, Priority::BelowNormal).expect("priority");
        assert_eq!(
            details(pid, "cmd.exe").priority,
            Some(Priority::BelowNormal)
        );
        assert!(set_priority(pid, "cmd.exe", start, Priority::Realtime).is_err());
        end_process(pid, "cmd.exe", start).expect("end");
        let status = child.wait().expect("wait");
        assert_eq!(status.code(), Some(1));
    }
}
