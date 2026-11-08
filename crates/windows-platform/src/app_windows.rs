//! Which processes are "apps" in Task Manager's sense: they own a window the
//! user can see and switch to.

use windows::core::BOOL;
use windows::Win32::Foundation::{HWND, LPARAM, TRUE};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumChildWindows, EnumWindows, GetClassNameW, GetWindow, GetWindowLongW, GetWindowTextLengthW,
    GetWindowThreadProcessId, IsWindowVisible, GWL_EXSTYLE, GW_OWNER, WS_EX_TOOLWINDOW,
};

/// PIDs owning a visible, unowned, titled, non-tool, non-cloaked top-level window.
/// Store-app frames (ApplicationFrameHost) are credited to the app hosted inside.
/// Windows on other virtual desktops are cloaked, so they are not counted.
pub fn app_pids() -> Vec<u32> {
    let mut pids: Vec<u32> = Vec::new();
    // SAFETY: the callback runs only during this call and receives a pointer to `pids`.
    let _ = unsafe { EnumWindows(Some(on_top_level), LPARAM(&raw mut pids as isize)) };
    pids.retain(|&p| p != 0);
    pids.sort_unstable();
    pids.dedup();
    pids
}

fn window_pid(hwnd: HWND) -> u32 {
    let mut pid = 0u32;
    // SAFETY: window handle from the enumeration; out-pointer to a local.
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    pid
}

fn is_app_window(hwnd: HWND) -> bool {
    // SAFETY: window handle from the enumeration; all calls only read window state.
    unsafe {
        if !IsWindowVisible(hwnd).as_bool()
            || GetWindow(hwnd, GW_OWNER).is_ok_and(|o| !o.is_invalid())
            || GetWindowLongW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TOOLWINDOW.0 != 0
            || GetWindowTextLengthW(hwnd) == 0
        {
            return false;
        }
        let mut cloaked = 0u32;
        let read = DwmGetWindowAttribute(hwnd, DWMWA_CLOAKED, (&raw mut cloaked).cast(), 4);
        read.is_err() || cloaked == 0
    }
}

fn class_name(hwnd: HWND) -> String {
    let mut buf = [0u16; 64];
    // SAFETY: window handle from the enumeration; buffer with its length.
    let n = unsafe { GetClassNameW(hwnd, &mut buf) }.max(0) as usize;
    String::from_utf16_lossy(&buf[..n])
}

unsafe extern "system" fn on_top_level(hwnd: HWND, lparam: LPARAM) -> BOOL {
    // SAFETY: `lparam` is the `&mut Vec<u32>` passed by `app_pids` for this enumeration.
    let pids = unsafe { &mut *(lparam.0 as *mut Vec<u32>) };
    if !is_app_window(hwnd) {
        return TRUE;
    }
    match class_name(hwnd).as_str() {
        "Progman" | "WorkerW" => {} // the desktop, not an app
        "ApplicationFrameWindow" => {
            // The frame belongs to ApplicationFrameHost; the app is the child window's process.
            let frame = window_pid(hwnd);
            let mut kids: Vec<u32> = Vec::new();
            // SAFETY: as above, for the child enumeration.
            let _ = unsafe {
                EnumChildWindows(Some(hwnd), Some(on_child), LPARAM(&raw mut kids as isize))
            };
            pids.extend(kids.into_iter().filter(|&p| p != frame));
        }
        _ => pids.push(window_pid(hwnd)),
    }
    TRUE
}

unsafe extern "system" fn on_child(hwnd: HWND, lparam: LPARAM) -> BOOL {
    // SAFETY: `lparam` is the `&mut Vec<u32>` passed for this enumeration.
    let pids = unsafe { &mut *(lparam.0 as *mut Vec<u32>) };
    pids.push(window_pid(hwnd));
    TRUE
}

#[cfg(test)]
mod tests {
    #[test]
    fn lists_without_failing() {
        let pids = super::app_pids();
        assert!(pids.windows(2).all(|w| w[0] < w[1]), "sorted and unique");
    }
}
