//! Crash dump discovery and optional analysis with Microsoft's console debugger
//! (cdb.exe from Debugging Tools for Windows). The debugger is never downloaded
//! or installed by this application; the user installs it (Windows SDK) or
//! points to it in Settings.
//!
//! The debugger runs with a fixed argument list built here (no shell), a dump
//! path that has been validated, and a symbol path we control: a local cache
//! only, unless the user explicitly enabled the Microsoft symbol server.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::Serialize;

const ANALYSIS_TIMEOUT: Duration = Duration::from_secs(240);
const MAX_OUTPUT_BYTES: usize = 4 * 1024 * 1024;
const MS_SYMBOL_SERVER: &str = "https://msdl.microsoft.com/download/symbols";

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DumpFile {
    pub path: String,
    pub size_bytes: u64,
    pub modified_ms: Option<i64>,
    pub kind: &'static str,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DumpListing {
    pub files: Vec<DumpFile>,
    /// Folders that exist but could not be listed (administrator rights needed).
    pub inaccessible: Vec<String>,
}

fn system_root() -> PathBuf {
    std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"))
}

fn file_entry(path: &Path, kind: &'static str) -> Option<DumpFile> {
    let meta = std::fs::metadata(path).ok()?;
    let modified_ms = meta
        .modified()
        .ok()
        .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64);
    Some(DumpFile {
        path: path.to_string_lossy().into_owned(),
        size_bytes: meta.len(),
        modified_ms,
        kind,
    })
}

/// Lists kernel dumps: `%SystemRoot%\Minidump\*.dmp`, `%SystemRoot%\MEMORY.DMP`
/// and live kernel reports (GPU timeouts and similar). Never modifies them.
pub fn list_dumps() -> DumpListing {
    let root = system_root();
    let mut listing = DumpListing {
        files: Vec::new(),
        inaccessible: Vec::new(),
    };
    for (dir, kind) in [
        (root.join("Minidump"), "minidump"),
        (root.join("LiveKernelReports"), "live kernel report"),
    ] {
        match std::fs::read_dir(&dir) {
            Ok(entries) => {
                let mut stack: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
                while let Some(p) = stack.pop() {
                    if p.is_dir() {
                        if let Ok(sub) = std::fs::read_dir(&p) {
                            stack.extend(sub.flatten().map(|e| e.path()));
                        }
                    } else if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("dmp")) {
                        listing.files.extend(file_entry(&p, kind));
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => listing
                .inaccessible
                .push(dir.to_string_lossy().into_owned()),
            Err(_) => {}
        }
    }
    let full = root.join("MEMORY.DMP");
    match std::fs::metadata(&full) {
        Ok(_) => listing
            .files
            .extend(file_entry(&full, "complete/kernel dump")),
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => listing
            .inaccessible
            .push(full.to_string_lossy().into_owned()),
        Err(_) => {}
    }
    listing
        .files
        .sort_by_key(|f| std::cmp::Reverse(f.modified_ms));
    listing
}

/// Finds cdb.exe: an explicit user setting first, then the Windows SDK locations.
pub fn find_cdb(configured: Option<&str>) -> Option<PathBuf> {
    if let Some(p) = configured.filter(|p| !p.trim().is_empty()) {
        let p = PathBuf::from(p.trim());
        return valid_cdb_path(&p).then_some(p);
    }
    let mut candidates = Vec::new();
    for var in ["ProgramFiles(x86)", "ProgramFiles"] {
        if let Some(base) = std::env::var_os(var) {
            candidates.push(PathBuf::from(base).join(r"Windows Kits\10\Debuggers\x64\cdb.exe"));
        }
    }
    candidates.into_iter().find(|p| valid_cdb_path(p))
}

/// A configured debugger must be an absolute path to an existing file named cdb.exe.
pub fn valid_cdb_path(p: &Path) -> bool {
    p.is_absolute()
        && p.file_name()
            .is_some_and(|n| n.eq_ignore_ascii_case("cdb.exe"))
        && p.is_file()
}

/// A dump to analyze must be an absolute path to an existing `.dmp` file.
pub fn valid_dump_path(p: &Path) -> bool {
    p.is_absolute()
        && p.extension().is_some_and(|e| e.eq_ignore_ascii_case("dmp"))
        && p.is_file()
        && !p.to_string_lossy().contains('"')
}

#[derive(Serialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DumpAnalysis {
    pub dump_path: String,
    pub debugger_path: String,
    pub symbol_server_used: bool,
    pub bugcheck_code: Option<u32>,
    pub bugcheck_parameters: Vec<u64>,
    pub module_name: Option<String>,
    pub image_name: Option<String>,
    pub image_version: Option<String>,
    pub process_name: Option<String>,
    pub symbol_name: Option<String>,
    pub failure_bucket: Option<String>,
    pub stack: Vec<String>,
    pub symbols_resolved: bool,
    pub warnings: Vec<String>,
    pub fields: Vec<(String, String)>,
    pub raw_output: String,
    pub duration_ms: u64,
}

/// Parses `!analyze -v` output: `KEY:  value` fields, the STACK_TEXT block and warnings.
pub fn parse_analyze_output(out: &str) -> DumpAnalysis {
    let mut a = DumpAnalysis::default();
    let mut in_stack = false;
    for line in out.lines() {
        let t = line.trim_end();
        if in_stack {
            if t.trim().is_empty() {
                in_stack = false;
            } else if a.stack.len() < 64 {
                a.stack.push(t.trim().to_owned());
            }
            continue;
        }
        if t.starts_with("STACK_TEXT:") {
            in_stack = true;
            continue;
        }
        let lower = t.to_ascii_lowercase();
        if (lower.contains("symbols can not be loaded")
            || lower.contains("unable to load image")
            || lower.contains("symbol loading may be unreliable")
            || lower.contains("wrong symbols"))
            && a.warnings.len() < 20
            && !a.warnings.iter().any(|w| w == t.trim())
        {
            a.warnings.push(t.trim().to_owned());
        }
        let Some((k, v)) = t.split_once(':') else {
            continue;
        };
        if k.is_empty()
            || !k
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
            || k.len() < 3
        {
            continue;
        }
        let v = v.trim().to_owned();
        if v.is_empty() {
            continue;
        }
        let hex = |s: &str| {
            u64::from_str_radix(
                s.trim_start_matches("0x")
                    .trim_start_matches("0X")
                    .replace('`', "")
                    .as_str(),
                16,
            )
            .ok()
        };
        match k {
            "BUGCHECK_CODE" => a.bugcheck_code = hex(&v).and_then(|c| u32::try_from(c).ok()),
            "BUGCHECK_P1" | "BUGCHECK_P2" | "BUGCHECK_P3" | "BUGCHECK_P4" => {
                a.bugcheck_parameters.extend(hex(&v))
            }
            "MODULE_NAME" => a.module_name = Some(v.clone()),
            "IMAGE_NAME" => a.image_name = Some(v.clone()),
            "IMAGE_VERSION" => a.image_version = Some(v.clone()),
            "PROCESS_NAME" => a.process_name = Some(v.clone()),
            "SYMBOL_NAME" => a.symbol_name = Some(v.clone()),
            "FAILURE_BUCKET_ID" => a.failure_bucket = Some(v.clone()),
            _ => {}
        }
        if a.fields.len() < 200 {
            a.fields.push((k.to_owned(), v));
        }
    }
    a.symbols_resolved = a.symbol_name.is_some()
        && !a
            .warnings
            .iter()
            .any(|w| w.to_ascii_lowercase().contains("symbols can not be loaded"));
    // Without the matching Windows symbols, !analyze substitutes placeholders
    // ("ntoskrnl.wrong.symbols.exe", WRONG_SYMBOLS_…) that name no real module.
    if [
        &a.failure_bucket,
        &a.image_name,
        &a.module_name,
        &a.symbol_name,
    ]
    .into_iter()
    .any(is_wrong_symbols)
    {
        for f in [
            &mut a.image_name,
            &mut a.module_name,
            &mut a.symbol_name,
            &mut a.process_name,
        ] {
            if is_wrong_symbols(f) {
                *f = None;
            }
        }
        a.symbols_resolved = false;
        a.warnings.insert(0, WRONG_SYMBOLS_NOTE.into());
    }
    a
}

const WRONG_SYMBOLS_NOTE: &str = "Windows symbol files for this build were not available, so the debugger could not name the faulting module. Turn on the Microsoft symbol server in Settings and analyze again.";

fn is_wrong_symbols(v: &Option<String>) -> bool {
    v.as_deref().is_some_and(|s| {
        let l = s.to_ascii_lowercase();
        l.contains("wrong_symbols") || l.contains("wrong.symbols")
    })
}

/// Runs `cdb -z <dump> -c "!analyze -v; q"` and parses the result.
pub fn analyze(
    cdb: &Path,
    dump: &Path,
    symbol_cache: &Path,
    use_symbol_server: bool,
) -> Result<DumpAnalysis, String> {
    if !valid_cdb_path(cdb) {
        return Err("The configured debugger is not a valid cdb.exe path.".into());
    }
    if !valid_dump_path(dump) {
        return Err("The dump path is not an existing .dmp file.".into());
    }
    if let Err(e) = std::fs::File::open(dump) {
        return Err(if e.kind() == std::io::ErrorKind::PermissionDenied {
            "Reading this dump requires administrator rights. Restart Veyl as administrator to analyze it.".into()
        } else {
            format!("Cannot open the dump: {e}")
        });
    }
    std::fs::create_dir_all(symbol_cache)
        .map_err(|e| format!("Cannot create the symbol cache folder: {e}"))?;
    let cache = symbol_cache.to_string_lossy();
    let sympath = if use_symbol_server {
        format!("srv*{cache}*{MS_SYMBOL_SERVER}")
    } else {
        format!("cache*{cache}")
    };

    let started = Instant::now();
    let mut child = Command::new(cdb)
        .arg("-z")
        .arg(dump)
        .arg("-y")
        .arg(&sympath)
        .arg("-c")
        .arg("!analyze -v; q")
        // No inherited symbol settings: the user's choice above is the only source.
        .env_remove("_NT_SYMBOL_PATH")
        .env_remove("_NT_ALT_SYMBOL_PATH")
        .env_remove("_NT_SYMBOL_PROXY")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("Could not start cdb.exe: {e}"))?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or("cdb.exe produced no output pipe")?;
    let reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stdout
            .by_ref()
            .take(MAX_OUTPUT_BYTES as u64)
            .read_to_end(&mut buf);
        buf
    });
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if started.elapsed() > ANALYSIS_TIMEOUT => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "Analysis did not finish within {} s and was stopped.",
                    ANALYSIS_TIMEOUT.as_secs()
                ));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(200)),
            Err(e) => return Err(format!("Waiting for cdb.exe failed: {e}")),
        }
    }
    let bytes = reader
        .join()
        .map_err(|_| "cdb output reader failed".to_owned())?;
    let text = String::from_utf8_lossy(&bytes).into_owned();
    let mut a = parse_analyze_output(&text);
    a.dump_path = dump.to_string_lossy().into_owned();
    a.debugger_path = cdb.to_string_lossy().into_owned();
    a.symbol_server_used = use_symbol_server;
    a.duration_ms = started.elapsed().as_millis() as u64;
    a.raw_output = text;
    if a.bugcheck_code.is_none() && a.fields.is_empty() {
        a.warnings
            .push("The debugger produced no !analyze fields; see the raw output.".into());
    }
    Ok(a)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What cdb printed for a real minidump on this PC without the symbol server.
    #[test]
    fn wrong_symbols_are_not_reported_as_the_cause() {
        let out = "BUGCHECK_CODE:  13a
BUGCHECK_P1: 11
PROCESS_NAME:  ntoskrnl.wrong.symbols.exe
SYMBOL_NAME:  nt_wrong_symbols!AFF6531F1450000
MODULE_NAME: nt_wrong_symbols
IMAGE_NAME:  ntoskrnl.wrong.symbols.exe
FAILURE_BUCKET_ID:  WRONG_SYMBOLS_X64_10.0.26100.9711_nt_wrong_symbols!AFF6531F1450000
";
        let a = parse_analyze_output(out);
        assert_eq!(a.bugcheck_code, Some(0x13a));
        assert_eq!(
            (a.image_name, a.module_name, a.symbol_name, a.process_name),
            (None, None, None, None)
        );
        assert!(!a.symbols_resolved);
        assert_eq!(
            a.warnings.first().map(String::as_str),
            Some(WRONG_SYMBOLS_NOTE)
        );
        assert!(
            a.failure_bucket.is_some(),
            "the raw bucket stays visible in the full output"
        );
    }

    const SAMPLE: &str = "\
*******************************************************************************
*                        Bugcheck Analysis                                    *
*******************************************************************************

KERNEL_MODE_HEAP_CORRUPTION (13a)
Arguments:
Arg1: 0000000000000017, Type of corruption detected

*** WARNING: Unable to verify timestamp for amdkmdag.sys
BUGCHECK_CODE:  13a

BUGCHECK_P1: 17

BUGCHECK_P2: ffffdc85`48000340

PROCESS_NAME:  System

STACK_TEXT:
ffff8a0c`1 fffff803`2 : nt!KeBugCheckEx
ffff8a0c`3 fffff803`4 : nt!RtlpHeapHandleError+0x40

SYMBOL_NAME:  amdkmdag+1a2b3c

MODULE_NAME: amdkmdag

IMAGE_NAME:  amdkmdag.sys

FAILURE_BUCKET_ID:  0x13a_17_amdkmdag!unknown_function
";

    #[test]
    fn parses_analyze_fields() {
        let a = parse_analyze_output(SAMPLE);
        assert_eq!(a.bugcheck_code, Some(0x13a));
        assert_eq!(a.bugcheck_parameters, vec![0x17, 0xffff_dc85_4800_0340]);
        assert_eq!(a.module_name.as_deref(), Some("amdkmdag"));
        assert_eq!(a.image_name.as_deref(), Some("amdkmdag.sys"));
        assert_eq!(a.process_name.as_deref(), Some("System"));
        assert_eq!(a.stack.len(), 2);
        assert!(a.symbols_resolved);
        assert!(a
            .failure_bucket
            .as_deref()
            .is_some_and(|f| f.starts_with("0x13a")));
    }

    #[test]
    fn empty_output_parses_to_nothing() {
        let a = parse_analyze_output("");
        assert!(a.bugcheck_code.is_none() && a.fields.is_empty() && !a.symbols_resolved);
    }

    #[test]
    fn path_validation() {
        assert!(!valid_cdb_path(Path::new("cdb.exe")), "relative");
        assert!(
            !valid_cdb_path(Path::new(r"C:\Windows\notepad.exe")),
            "wrong name"
        );
        assert!(!valid_dump_path(Path::new(r"C:\Windows\win.ini")));
        assert!(find_cdb(Some(r"C:\nope\cdb.exe")).is_none());
    }

    #[test]
    fn listing_never_fails() {
        let l = list_dumps();
        assert!(l
            .files
            .iter()
            .all(|f| f.path.to_ascii_lowercase().ends_with(".dmp")));
    }
}
