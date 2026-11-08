//! Owned Win32 handles and UTF-16 helpers.

use windows::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};

/// A kernel handle closed on drop.
#[derive(Debug)]
pub struct OwnedHandle(pub HANDLE);

impl OwnedHandle {
    pub fn raw(&self) -> HANDLE {
        self.0
    }
}

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        if !self.0.is_invalid() && self.0 != INVALID_HANDLE_VALUE {
            // SAFETY: we own this handle and close it exactly once.
            if let Err(e) = unsafe { CloseHandle(self.0) } {
                tracing::warn!("CloseHandle failed: {e}");
            }
        }
    }
}

/// NUL-terminated UTF-16 copy of `s`.
pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// String from a UTF-16 buffer, stopping at the first NUL.
pub fn from_wide(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

/// Reads a NUL-terminated UTF-16 string.
///
/// # Safety
/// `p` must be null or point to a valid NUL-terminated UTF-16 string.
pub unsafe fn from_pwstr(p: *const u16) -> String {
    if p.is_null() {
        return String::new();
    }
    let mut len = 0usize;
    // SAFETY: caller guarantees a NUL terminator exists.
    while unsafe { *p.add(len) } != 0 {
        len += 1;
    }
    // SAFETY: `len` elements were just read successfully.
    String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(p, len) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wide_round_trip() {
        let w = wide("C:\\é");
        assert_eq!(*w.last().expect("nul"), 0);
        assert_eq!(from_wide(&w), "C:\\é");
        assert_eq!(from_wide(&[0x41, 0, 0x42]), "A");
        // SAFETY: `w` is NUL-terminated.
        assert_eq!(unsafe { from_pwstr(w.as_ptr()) }, "C:\\é");
        // SAFETY: null is allowed.
        assert_eq!(unsafe { from_pwstr(std::ptr::null()) }, "");
    }
}
