//! Version resource of an executable or DLL (company, description, version).

use serde::Serialize;
use windows::Win32::Storage::FileSystem::{
    GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW,
};

use crate::handle::{from_pwstr, wide};

#[derive(Serialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FileVersionInfo {
    pub company: Option<String>,
    pub description: Option<String>,
    pub product: Option<String>,
    pub file_version: Option<String>,
    pub size_bytes: Option<u64>,
}

/// Reads the version resource; `None` if the file has none or cannot be read.
pub fn file_version_info(path: &str) -> Option<FileVersionInfo> {
    let w = wide(path);
    let size_bytes = std::fs::metadata(path).ok().map(|m| m.len());
    // SAFETY: NUL-terminated path.
    let len = unsafe { GetFileVersionInfoSizeW(windows::core::PCWSTR(w.as_ptr()), None) };
    if len == 0 {
        return size_bytes.map(|s| FileVersionInfo {
            size_bytes: Some(s),
            ..Default::default()
        });
    }
    let mut block = vec![0u8; len as usize];
    // SAFETY: buffer of `len` bytes.
    unsafe {
        GetFileVersionInfoW(
            windows::core::PCWSTR(w.as_ptr()),
            None,
            len,
            block.as_mut_ptr().cast(),
        )
    }
    .ok()?;

    // First language/codepage pair from \VarFileInfo\Translation.
    let translation = query(&block, r"\VarFileInfo\Translation").and_then(|(p, n)| {
        if n < 4 {
            return None;
        }
        // SAFETY: VerQueryValueW returned a pointer to at least 4 bytes inside `block`.
        let b = unsafe { std::slice::from_raw_parts(p.cast::<u8>(), 4) };
        Some(format!(
            "{:04x}{:04x}",
            u16::from_le_bytes([b[0], b[1]]),
            u16::from_le_bytes([b[2], b[3]])
        ))
    });
    let lang = translation.unwrap_or_else(|| "040904b0".into());
    let string = |name: &str| {
        query(&block, &format!(r"\StringFileInfo\{lang}\{name}")).and_then(|(p, n)| {
            if n == 0 {
                return None;
            }
            // SAFETY: string values are NUL-terminated UTF-16 inside `block`.
            let s = unsafe { from_pwstr(p.cast()) };
            let s = s.trim().to_owned();
            (!s.is_empty()).then_some(s)
        })
    };
    Some(FileVersionInfo {
        company: string("CompanyName"),
        description: string("FileDescription"),
        product: string("ProductName"),
        file_version: string("FileVersion"),
        size_bytes,
    })
}

fn query(block: &[u8], sub: &str) -> Option<(*const core::ffi::c_void, u32)> {
    let w = wide(sub);
    let mut p: *mut core::ffi::c_void = std::ptr::null_mut();
    let mut n = 0u32;
    // SAFETY: `block` is a version resource returned by GetFileVersionInfoW and outlives the returned pointer's use.
    let ok = unsafe {
        VerQueryValueW(
            block.as_ptr().cast(),
            windows::core::PCWSTR(w.as_ptr()),
            &mut p,
            &mut n,
        )
    };
    (ok.as_bool() && !p.is_null()).then_some((p as *const _, n))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_kernel32_version() {
        let root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
        let info =
            file_version_info(&format!(r"{root}\System32\kernel32.dll")).expect("version info");
        assert!(info
            .company
            .as_deref()
            .is_some_and(|c| c.contains("Microsoft")));
        assert!(info.file_version.is_some());
        assert!(file_version_info(r"C:\definitely\missing.dll").is_none());
    }
}
