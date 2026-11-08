//! Read-only registry helpers. Nothing in this application writes the registry.

use windows::Win32::Foundation::{ERROR_MORE_DATA, ERROR_NO_MORE_ITEMS, ERROR_SUCCESS};
use windows::Win32::System::Registry::{
    RegCloseKey, RegEnumValueW, RegGetValueW, RegOpenKeyExW, HKEY, KEY_READ, REG_SAM_FLAGS,
    RRF_RT_REG_DWORD, RRF_RT_REG_SZ,
};

use crate::handle::{from_wide, wide};

pub use windows::Win32::System::Registry::{
    HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_WOW64_32KEY, KEY_WOW64_64KEY,
};

/// `REG_SZ` / `REG_EXPAND_SZ` value as a string (unexpanded), or `None` if absent.
pub fn read_string(root: HKEY, subkey: &str, value: &str) -> Option<String> {
    let (sk, v) = (wide(subkey), wide(value));
    let mut bytes = 0u32;
    // SAFETY: size query with NUL-terminated name buffers.
    let rc = unsafe {
        RegGetValueW(
            root,
            windows::core::PCWSTR(sk.as_ptr()),
            windows::core::PCWSTR(v.as_ptr()),
            RRF_RT_REG_SZ,
            None,
            None,
            Some(&mut bytes),
        )
    };
    if rc != ERROR_SUCCESS || bytes == 0 {
        return None;
    }
    let mut buf = vec![0u16; (bytes as usize).div_ceil(2)];
    // SAFETY: buffer holds `bytes` bytes.
    let rc = unsafe {
        RegGetValueW(
            root,
            windows::core::PCWSTR(sk.as_ptr()),
            windows::core::PCWSTR(v.as_ptr()),
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr().cast()),
            Some(&mut bytes),
        )
    };
    (rc == ERROR_SUCCESS).then(|| from_wide(&buf))
}

pub fn read_dword(root: HKEY, subkey: &str, value: &str) -> Option<u32> {
    let (sk, v) = (wide(subkey), wide(value));
    let mut data = 0u32;
    let mut bytes = 4u32;
    // SAFETY: 4-byte output buffer for a REG_DWORD.
    let rc = unsafe {
        RegGetValueW(
            root,
            windows::core::PCWSTR(sk.as_ptr()),
            windows::core::PCWSTR(v.as_ptr()),
            RRF_RT_REG_DWORD,
            None,
            Some((&mut data as *mut u32).cast()),
            Some(&mut bytes),
        )
    };
    (rc == ERROR_SUCCESS).then_some(data)
}

/// One value of a key: name, registry type, raw data.
pub struct RegValue {
    pub name: String,
    pub kind: u32,
    pub data: Vec<u8>,
}

impl RegValue {
    /// Data interpreted as a NUL-terminated UTF-16 string.
    pub fn as_string(&self) -> String {
        let words: Vec<u16> = self
            .data
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        from_wide(&words)
    }
}

/// Enumerates all values of `subkey`. Missing key → `Ok(empty)`; other failures → `Err(code)`.
pub fn enum_values(root: HKEY, subkey: &str, extra: REG_SAM_FLAGS) -> Result<Vec<RegValue>, u32> {
    let sk = wide(subkey);
    let mut key = HKEY::default();
    // SAFETY: out-pointer to a local; key closed below.
    let rc = unsafe {
        RegOpenKeyExW(
            root,
            windows::core::PCWSTR(sk.as_ptr()),
            None,
            REG_SAM_FLAGS(KEY_READ.0 | extra.0),
            &mut key,
        )
    };
    if rc.0 == 2 {
        // ERROR_FILE_NOT_FOUND: the key does not exist.
        return Ok(Vec::new());
    }
    if rc != ERROR_SUCCESS {
        return Err(rc.0);
    }
    let mut out = Vec::new();
    let mut index = 0u32;
    let mut name = vec![0u16; 16_384];
    let mut data = vec![0u8; 4096];
    loop {
        let mut name_len = name.len() as u32;
        let mut data_len = data.len() as u32;
        let mut kind = 0u32;
        // SAFETY: buffers sized as passed; key is open.
        let rc = unsafe {
            RegEnumValueW(
                key,
                index,
                Some(windows::core::PWSTR(name.as_mut_ptr())),
                &mut name_len,
                None,
                Some(&mut kind),
                Some(data.as_mut_ptr()),
                Some(&mut data_len),
            )
        };
        if rc == ERROR_NO_MORE_ITEMS {
            break;
        }
        if rc == ERROR_MORE_DATA {
            data.resize(data_len as usize + 2, 0);
            continue;
        }
        if rc != ERROR_SUCCESS {
            break;
        }
        out.push(RegValue {
            name: String::from_utf16_lossy(&name[..name_len as usize]),
            kind,
            data: data[..data_len as usize].to_vec(),
        });
        index += 1;
    }
    // SAFETY: closing the key we opened.
    unsafe {
        let _ = RegCloseKey(key); // a close failure leaves nothing to recover
    }
    Ok(out)
}
