//! Downloads a file the user asked for and checks it against a pinned SHA-256,
//! using Windows' own downloader (URLMon: proxy settings, redirects, TLS) and hash
//! (CNG). Nothing here runs the file.

use std::path::Path;

use windows::core::PCWSTR;
use windows::Win32::Security::Cryptography::{BCryptHash, BCRYPT_SHA256_ALG_HANDLE};
use windows::Win32::System::Com::Urlmon::URLDownloadToFileW;
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED};

use crate::handle::wide;

/// Lowercase hex SHA-256 of `data`.
pub fn sha256_hex(data: &[u8]) -> Option<String> {
    let mut out = [0u8; 32];
    // SAFETY: CNG pseudo-handle for SHA-256 (Windows 10+); input and output slices sized by the call.
    let st = unsafe { BCryptHash(BCRYPT_SHA256_ALG_HANDLE, None, data, &mut out) };
    (st.0 >= 0).then(|| out.iter().map(|b| format!("{b:02x}")).collect())
}

/// Downloads `url` to `dest`, replacing it.
pub fn to_file(url: &str, dest: &Path) -> Result<(), String> {
    // SAFETY: URLMon is a COM API; balanced with CoUninitialize when this call initialized COM.
    let com = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    let (u, d) = (wide(url), wide(&dest.to_string_lossy()));
    // SAFETY: NUL-terminated strings that outlive the call; no callback object.
    let r = unsafe { URLDownloadToFileW(None, PCWSTR(u.as_ptr()), PCWSTR(d.as_ptr()), 0, None) };
    if com.is_ok() {
        // SAFETY: pairs the successful CoInitializeEx above.
        unsafe { CoUninitialize() };
    }
    r.map_err(|e| {
        format!("The download failed ({e}). Check the internet connection and try again.")
    })
}

/// Downloads `url` to `dest` and keeps it only if its size and SHA-256 match.
/// The bytes land in `dest`'s folder first, so callers can pass an admin-only folder.
pub fn verified(url: &str, dest: &Path, size: u64, sha256: &str) -> Result<(), String> {
    let partial = dest.with_extension("partial");
    let _ = std::fs::remove_file(&partial);
    to_file(url, &partial)?;
    let ok = std::fs::read(&partial)
        .map(|b| b.len() as u64 == size && sha256_hex(&b).as_deref() == Some(sha256))
        .unwrap_or(false);
    if !ok {
        let _ = std::fs::remove_file(&partial);
        return Err(
            "The downloaded file did not match the expected release, so it was deleted.".into(),
        );
    }
    std::fs::rename(&partial, dest).map_err(|e| {
        let _ = std::fs::remove_file(&partial);
        format!("Could not save {}: {e}", dest.display())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_matches_known_vectors() {
        assert_eq!(
            sha256_hex(b"abc").as_deref(),
            Some("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
        );
        assert_eq!(
            sha256_hex(b"").as_deref(),
            Some("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855")
        );
    }

    #[test]
    fn a_file_that_does_not_match_is_deleted() {
        let dir = std::env::temp_dir().join(format!("veyl-download-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let src = dir.join("source.bin");
        std::fs::write(&src, b"not the release").expect("write");
        let url = format!("file:///{}", src.display().to_string().replace('\\', "/"));
        let dest = dir.join("Tool.exe");
        let err = verified(&url, &dest, 15, "00").expect_err("wrong hash must be refused");
        assert!(err.contains("did not match"), "{err}");
        assert!(
            !dest.exists() && !dest.with_extension("partial").exists(),
            "nothing is left behind"
        );
        let good = sha256_hex(b"not the release").expect("hash");
        verified(&url, &dest, 15, &good).expect("matching file is kept");
        assert_eq!(std::fs::read(&dest).expect("read"), b"not the release");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
