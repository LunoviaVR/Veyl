//! Frame capture through Intel PresentMon's console application (MIT licensed,
//! https://github.com/GameTechDev/PresentMon). PresentMon is not bundled: the
//! user installs it and selects the executable in Settings, or places it in the
//! application's `tools` folder.
//!
//! PresentMon reads real presentation events from ETW (DXGI, D3D9, and the
//! kernel graphics events that also cover Vulkan and OpenGL presents). No code
//! is injected into the game. Starting an ETW session requires administrator
//! rights or membership in the Performance Log Users group.
//!
//! The CSV header is parsed by column name, so both the v1 metric columns
//! (`MsBetweenPresents`, `TimeInSeconds`) and v2 ones (`FrameTime`,
//! `CPUStartTime`) are accepted.

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};

use pch_core::frames::{FrameMetricsProvider, FramePoll, FrameRecord};
use pch_core::{Availability, Status};

pub const SESSION_NAME: &str = "PCHealthConsole";
/// Frames buffered between polls; older frames are dropped (and counted) beyond this.
const BUFFER_CAP: usize = 200_000;
/// windows.h CREATE_NO_WINDOW: do not flash a console window.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Column indices resolved from the CSV header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CsvLayout {
    application: Option<usize>,
    pid: usize,
    swap_chain: Option<usize>,
    runtime: Option<usize>,
    present_mode: Option<usize>,
    time: usize,
    time_is_ms: bool,
    frametime: usize,
    display: Option<usize>,
    dropped: Option<usize>,
    width: usize,
}

impl CsvLayout {
    pub fn from_header(header: &str) -> Result<Self, String> {
        let cols: Vec<String> = header.split(',').map(|c| c.trim().to_owned()).collect();
        let find = |names: &[&str]| {
            names
                .iter()
                .find_map(|n| cols.iter().position(|c| c.eq_ignore_ascii_case(n)))
        };
        let pid = find(&["ProcessID"]).ok_or("PresentMon output lacks a ProcessID column")?;
        let frametime = find(&["MsBetweenPresents", "FrameTime"])
            .ok_or("PresentMon output lacks MsBetweenPresents/FrameTime")?;
        let (time, time_is_ms) = match find(&["TimeInSeconds"]) {
            Some(i) => (i, false),
            None => (
                find(&["CPUStartTime", "TimeInMs"])
                    .ok_or("PresentMon output lacks a time column")?,
                true,
            ),
        };
        Ok(Self {
            application: find(&["Application"]),
            pid,
            swap_chain: find(&["SwapChainAddress"]),
            runtime: find(&["Runtime", "PresentRuntime"]),
            present_mode: find(&["PresentMode"]),
            time,
            time_is_ms,
            frametime,
            display: find(&["MsBetweenDisplayChange", "DisplayedTime"]),
            dropped: find(&["Dropped"]),
            width: cols.len(),
        })
    }

    /// Parses one data row; `None` for rows with missing or non-numeric required fields.
    pub fn parse(&self, line: &str) -> Option<FrameRecord> {
        let f: Vec<&str> = line.split(',').map(str::trim).collect();
        if f.len() < self.width {
            return None;
        }
        let num = |i: usize| {
            f.get(i)
                .and_then(|v| v.parse::<f64>().ok())
                .filter(|v| v.is_finite())
        };
        let text = |i: Option<usize>| {
            i.and_then(|i| f.get(i))
                .map(|s| (*s).to_owned())
                .unwrap_or_default()
        };
        let time = num(self.time)?;
        Some(FrameRecord {
            pid: f.get(self.pid)?.parse().ok()?,
            application: text(self.application),
            swap_chain: text(self.swap_chain),
            runtime: text(self.runtime),
            present_mode: text(self.present_mode),
            time_secs: if self.time_is_ms { time / 1000.0 } else { time },
            frametime_ms: num(self.frametime)?,
            display_ms: self.display.and_then(num).filter(|v| *v > 0.0),
            displayed: self.dropped.and_then(|i| f.get(i)).and_then(|v| match *v {
                "0" => Some(true),
                "1" => Some(false),
                _ => None,
            }),
        })
    }
}

/// Validates a user-selected PresentMon executable path.
pub fn valid_presentmon_path(p: &Path) -> bool {
    p.is_absolute()
        && p.is_file()
        && p.file_name()
            .map(|n| n.to_string_lossy().to_ascii_lowercase())
            .is_some_and(|n| n.starts_with("presentmon") && n.ends_with(".exe"))
}

/// Locates PresentMon: the configured path, then `PresentMon*.exe` in `tools_dir`.
pub fn find_presentmon(configured: Option<&str>, tools_dir: &Path) -> Option<PathBuf> {
    if let Some(p) = configured.filter(|p| !p.trim().is_empty()) {
        let p = PathBuf::from(p.trim());
        return valid_presentmon_path(&p).then_some(p);
    }
    let mut found: Vec<PathBuf> = std::fs::read_dir(tools_dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| valid_presentmon_path(p))
        .collect();
    found.sort();
    found.pop()
}

#[derive(Default)]
struct Shared {
    frames: VecDeque<FrameRecord>,
    lost: u64,
    error: Option<String>,
    header_seen: bool,
    ended: bool,
}

pub struct PresentMonCli {
    exe: PathBuf,
    child: Option<Child>,
    shared: Arc<Mutex<Shared>>,
}

impl PresentMonCli {
    pub fn new(exe: PathBuf) -> Self {
        Self {
            exe,
            child: None,
            shared: Arc::new(Mutex::new(Shared::default())),
        }
    }

    fn command(&self) -> Command {
        use std::os::windows::process::CommandExt;
        let mut c = Command::new(&self.exe);
        c.creation_flags(CREATE_NO_WINDOW).stdin(Stdio::null());
        c
    }

    /// Stops a session left behind by a previous run (PresentMon's own cleanup flag).
    fn terminate_session(&self) {
        let r = self
            .command()
            .args([
                "--terminate_existing_session",
                "--session_name",
                SESSION_NAME,
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        if let Err(e) = r {
            tracing::warn!("PresentMon session cleanup failed: {e}");
        }
    }
}

fn lock(shared: &Arc<Mutex<Shared>>) -> std::sync::MutexGuard<'_, Shared> {
    // A poisoned lock only means a reader thread panicked mid-push; the data is still usable.
    shared.lock().unwrap_or_else(|p| p.into_inner())
}

impl FrameMetricsProvider for PresentMonCli {
    fn name(&self) -> &'static str {
        "PresentMon"
    }

    fn start(&mut self, pid: u32) -> Result<(), String> {
        self.stop();
        *lock(&self.shared) = Shared::default();
        let pid_s = pid.to_string();
        let mut child = self
            .command()
            .args([
                "--process_id",
                pid_s.as_str(),
                "--output_stdout",
                "--stop_existing_session",
                "--session_name",
                SESSION_NAME,
                "--terminate_on_proc_exit",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Could not start PresentMon: {e}"))?;
        let stdout = child.stdout.take().ok_or("PresentMon has no stdout")?;
        let stderr = child.stderr.take();
        let shared = self.shared.clone();
        std::thread::Builder::new()
            .name("presentmon-reader".into())
            .spawn(move || {
                let mut layout: Option<CsvLayout> = None;
                for line in BufReader::new(stdout).lines() {
                    let Ok(line) = line else { break };
                    match &layout {
                        None => match CsvLayout::from_header(&line) {
                            Ok(l) => {
                                layout = Some(l);
                                lock(&shared).header_seen = true;
                            }
                            // PresentMon prints status lines before the header; keep the last one for errors.
                            Err(_) => {
                                lock(&shared).error =
                                    Some(line.trim().to_owned()).filter(|s| !s.is_empty())
                            }
                        },
                        Some(l) => {
                            if let Some(rec) = l.parse(&line) {
                                let mut s = lock(&shared);
                                if s.frames.len() >= BUFFER_CAP {
                                    s.frames.pop_front();
                                    s.lost += 1;
                                }
                                s.frames.push_back(rec);
                            }
                        }
                    }
                }
                let mut err = String::new();
                if let Some(mut e) = stderr {
                    let _ = e.by_ref().take(8192).read_to_string(&mut err);
                }
                let mut s = lock(&shared);
                s.ended = true;
                if !err.trim().is_empty() {
                    s.error = Some(err.trim().to_owned());
                }
            })
            .map_err(|e| format!("Could not start the PresentMon reader thread: {e}"))?;
        self.child = Some(child);
        Ok(())
    }

    fn poll(&mut self) -> FramePoll {
        let mut s = lock(&self.shared);
        let frames: Vec<FrameRecord> = s.frames.drain(..).collect();
        let lost = std::mem::take(&mut s.lost);
        let status = if s.ended && !s.header_seen {
            let msg = s
                .error
                .clone()
                .unwrap_or_else(|| "PresentMon exited without output".into());
            let denied = msg.to_ascii_lowercase().contains("access")
                || msg.to_ascii_lowercase().contains("administrator")
                || msg.to_ascii_lowercase().contains("elevat");
            if denied {
                Status::new(
                    Availability::PermissionRequired,
                    format!("PresentMon needs administrator rights or membership in Performance Log Users ({msg})"),
                )
            } else {
                Status::error(format!("PresentMon stopped: {msg}"))
            }
        } else if s.ended {
            Status::new(
                Availability::Stale,
                "Capture ended (the process exited or PresentMon stopped)",
            )
        } else if !s.header_seen {
            Status::waiting("Starting PresentMon…")
        } else {
            Status::available()
        };
        FramePoll {
            status,
            frames,
            lost,
        }
    }

    fn stop(&mut self) {
        if let Some(mut c) = self.child.take() {
            if let Err(e) = c.kill() {
                tracing::debug!("PresentMon already exited: {e}");
            }
            let _ = c.wait(); // reap; exit status is irrelevant after kill
            self.terminate_session();
        }
    }

    fn is_running(&self) -> bool {
        self.child.is_some() && !lock(&self.shared).ended
    }
}

impl Drop for PresentMonCli {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const V1: &str = "Application,ProcessID,SwapChainAddress,Runtime,SyncInterval,PresentFlags,Dropped,TimeInSeconds,MsInPresentAPI,MsBetweenPresents,AllowsTearing,PresentMode,MsUntilRenderComplete,MsUntilDisplayed,MsBetweenDisplayChange";
    const V2: &str = "Application,ProcessID,SwapChainAddress,PresentRuntime,SyncInterval,PresentFlags,AllowsTearing,PresentMode,CPUStartTime,FrameTime,CPUBusy,CPUWait,GPULatency,GPUTime,GPUBusy,GPUWait,DisplayLatency,DisplayedTime";

    #[test]
    fn parses_v1_rows() {
        let l = CsvLayout::from_header(V1).expect("header");
        let r = l.parse("game.exe,1234,0x000001A2B3C4D5E0,DXGI,0,0,0,12.5,0.2,16.67,1,Hardware: Independent Flip,1.0,2.0,16.68").expect("row");
        assert_eq!(r.pid, 1234);
        assert_eq!(r.frametime_ms, 16.67);
        assert_eq!(r.time_secs, 12.5);
        assert_eq!(r.display_ms, Some(16.68));
        assert_eq!(r.displayed, Some(true));
        assert_eq!(r.present_mode, "Hardware: Independent Flip");
    }

    #[test]
    fn parses_v2_rows_with_ms_time_and_na() {
        let l = CsvLayout::from_header(V2).expect("header");
        let r = l
            .parse("game.exe,42,0x1,DXGI,0,0,1,Composed: Flip,1500.0,8.3,7,1,0.5,6,6,2,10,NA")
            .expect("row");
        assert_eq!(r.time_secs, 1.5);
        assert_eq!(r.frametime_ms, 8.3);
        assert_eq!(r.display_ms, None, "NA is not a value");
        assert!(
            l.parse("game.exe,42,0x1,DXGI,0,0,1,Composed: Flip,1500.0,NA,7,1,0.5,6,6,2,10,8")
                .is_none(),
            "missing frame time"
        );
        assert!(l.parse("too,short").is_none());
    }

    #[test]
    fn rejects_unrelated_lines_as_header() {
        assert!(CsvLayout::from_header("Started recording...").is_err());
        assert!(CsvLayout::from_header("").is_err());
    }

    #[test]
    fn path_validation() {
        assert!(!valid_presentmon_path(Path::new("PresentMon.exe")));
        assert!(!valid_presentmon_path(Path::new(r"C:\Windows\notepad.exe")));
        assert!(find_presentmon(Some(r"C:\missing\PresentMon.exe"), Path::new(r"C:\")).is_none());
    }
}
