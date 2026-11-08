//! Read-only list of Windows services (state, PID, start type, binary path).

use serde::Serialize;
use windows::Win32::System::Services::{
    CloseServiceHandle, EnumServicesStatusExW, OpenSCManagerW, OpenServiceW, QueryServiceConfigW,
    ENUM_SERVICE_STATUS_PROCESSW, QUERY_SERVICE_CONFIGW, SC_ENUM_PROCESS_INFO, SC_HANDLE,
    SC_MANAGER_CONNECT, SC_MANAGER_ENUMERATE_SERVICE, SERVICE_QUERY_CONFIG, SERVICE_STATE_ALL,
    SERVICE_WIN32,
};

use crate::handle::{from_pwstr, wide};

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ServiceInfo {
    pub name: String,
    pub display_name: String,
    pub state: &'static str,
    pub pid: Option<u32>,
    pub start_type: Option<&'static str>,
    pub binary_path: Option<String>,
    pub account: Option<String>,
}

struct Sc(SC_HANDLE);
impl Drop for Sc {
    fn drop(&mut self) {
        // SAFETY: closing a handle we own.
        let _ = unsafe { CloseServiceHandle(self.0) }; // a close failure leaves nothing to recover
    }
}

/// winsvc.h SERVICE_* current state values.
fn state_name(s: u32) -> &'static str {
    match s {
        1 => "stopped",
        2 => "starting",
        3 => "stopping",
        4 => "running",
        5 => "resuming",
        6 => "pausing",
        7 => "paused",
        _ => "unknown",
    }
}

/// winnt.h SERVICE_*_START values.
fn start_name(s: u32) -> &'static str {
    match s {
        0 => "boot",
        1 => "system",
        2 => "automatic",
        3 => "manual",
        4 => "disabled",
        _ => "unknown",
    }
}

pub fn list() -> Result<Vec<ServiceInfo>, String> {
    // SAFETY: local SCM, enumerate access only.
    let scm = Sc(unsafe {
        OpenSCManagerW(
            windows::core::PCWSTR::null(),
            windows::core::PCWSTR::null(),
            SC_MANAGER_CONNECT | SC_MANAGER_ENUMERATE_SERVICE,
        )
    }
    .map_err(|e| format!("OpenSCManagerW failed: {e}"))?);
    let mut needed = 0u32;
    let mut count = 0u32;
    let mut resume = 0u32;
    let mut buf: Vec<u8> = Vec::new();
    let mut out = Vec::new();
    loop {
        // SAFETY: buffer slice and out-pointers valid; ERROR_MORE_DATA handled by resizing.
        let r = unsafe {
            EnumServicesStatusExW(
                scm.0,
                SC_ENUM_PROCESS_INFO,
                SERVICE_WIN32,
                SERVICE_STATE_ALL,
                (!buf.is_empty()).then_some(buf.as_mut_slice()),
                &mut needed,
                &mut count,
                Some(&mut resume),
                windows::core::PCWSTR::null(),
            )
        };
        let more = r
            .as_ref()
            .err()
            .is_some_and(|e| e.code().0 as u32 == 0x8007_00EA); // ERROR_MORE_DATA
        if r.is_err() && !more {
            return Err(format!(
                "EnumServicesStatusExW failed: {}",
                r.err().map(|e| e.to_string()).unwrap_or_default()
            ));
        }
        let items = buf.as_ptr().cast::<ENUM_SERVICE_STATUS_PROCESSW>();
        for i in 0..count as usize {
            // SAFETY: the API wrote `count` entries at the start of `buf`; strings point into `buf`.
            let e = unsafe { &*items.add(i) };
            let st = e.ServiceStatusProcess;
            // SAFETY: NUL-terminated strings inside `buf`.
            let (name, display_name) =
                unsafe { (from_pwstr(e.lpServiceName.0), from_pwstr(e.lpDisplayName.0)) };
            let (start_type, binary_path, account) = config(&scm, &name);
            out.push(ServiceInfo {
                name,
                display_name,
                state: state_name(st.dwCurrentState.0),
                pid: (st.dwProcessId != 0).then_some(st.dwProcessId),
                start_type,
                binary_path,
                account,
            });
        }
        if !more {
            break;
        }
        buf = vec![0u8; needed as usize + 4096];
        count = 0;
    }
    out.sort_by_cached_key(|s| s.display_name.to_lowercase());
    Ok(out)
}

fn config(scm: &Sc, name: &str) -> (Option<&'static str>, Option<String>, Option<String>) {
    let w = wide(name);
    // SAFETY: NUL-terminated name; query-config access only.
    let Ok(h) = (unsafe {
        OpenServiceW(
            scm.0,
            windows::core::PCWSTR(w.as_ptr()),
            SERVICE_QUERY_CONFIG,
        )
    }) else {
        return (None, None, None);
    };
    let svc = Sc(h);
    let mut needed = 0u32;
    // SAFETY: size query.
    let _ = unsafe { QueryServiceConfigW(svc.0, None, 0, &mut needed) };
    if needed == 0 {
        return (None, None, None);
    }
    let mut buf = vec![0u64; (needed as usize).div_ceil(8)];
    // SAFETY: 8-aligned buffer of at least `needed` bytes.
    if unsafe { QueryServiceConfigW(svc.0, Some(buf.as_mut_ptr().cast()), needed, &mut needed) }
        .is_err()
    {
        return (None, None, None);
    }
    // SAFETY: the API filled a QUERY_SERVICE_CONFIGW whose strings point into `buf`.
    let c = unsafe { &*buf.as_ptr().cast::<QUERY_SERVICE_CONFIGW>() };
    // SAFETY: strings are NUL-terminated inside `buf` (or null).
    let path = unsafe { from_pwstr(c.lpBinaryPathName.0) };
    // SAFETY: as above.
    let account = unsafe { from_pwstr(c.lpServiceStartName.0) };
    (
        Some(start_name(c.dwStartType.0)),
        (!path.is_empty()).then_some(path),
        (!account.is_empty()).then_some(account),
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn lists_services() {
        let s = super::list().expect("SCM readable");
        assert!(s.len() > 50);
        assert!(s
            .iter()
            .any(|s| s.name.eq_ignore_ascii_case("EventLog") && s.state == "running"));
    }
}
