//! Game capture sessions: drives the frame provider, keeps the session's frame
//! times, computes live statistics, samples system context once per second, and
//! stores the finished session.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::Ordering;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use veyl_core::frame_stats::{compute, FrameStats, FrameStatsConfig};
use veyl_core::frames::{FrameMetricsProvider, FrameRecord};
use veyl_core::{Availability, Status};
use veyl_storage::NewSession;
use veyl_windows::games::Games;
use veyl_windows::presentmon::{find_presentmon, PresentMonCli};

use crate::state::{now_ms, AppState};
use crate::writer::WriteMsg;

pub enum GamingCommand {
    Start {
        pid: u32,
    },
    Stop,
    /// Re-check whether PresentMon is available (after it was installed).
    Refresh,
}

/// Window used for live statistics.
const LIVE_WINDOW_MS: f64 = 30_000.0;
const RECENT_FRAMES: usize = 600;
/// Hard cap on frames kept per session (about 4.6 h at 240 FPS).
const MAX_SESSION_FRAMES: usize = 4_000_000;
const MAX_CONTEXT_SECONDS: usize = 6 * 3600;
const MIN_FRAMES_TO_SAVE: usize = 30;

#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct ContextPoint {
    pub t: i64,
    pub cpu_pct: Option<f32>,
    pub gpu_pct: Option<f32>,
    pub gpu_temp_c: Option<f32>,
    pub cpu_temp_c: Option<f32>,
    pub vram_bytes: Option<u64>,
    pub ram_used_bytes: Option<u64>,
    pub process_cpu_pct: Option<f32>,
    pub process_gpu_pct: Option<f32>,
}

#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct GamingLive {
    pub provider: String,
    pub provider_status: Status,
    pub capturing: bool,
    pub pid: Option<u32>,
    pub process_name: Option<String>,
    pub exe_path: Option<String>,
    pub started_ms: Option<i64>,
    pub duration_secs: f64,
    pub frames: u64,
    pub other_swap_chain_frames: u64,
    pub lost_frames: u64,
    pub runtime: Option<String>,
    pub present_mode: Option<String>,
    pub live: FrameStats,
    pub recent_frametimes: Vec<f32>,
    pub context: Option<ContextPoint>,
    pub last_saved_session: Option<i64>,
    /// The capture was started automatically for a detected game.
    pub auto_started: bool,
}

#[derive(Default)]
struct Session {
    pid: u32,
    /// (pid, process start time): identifies the process across PID reuse.
    key: (u32, Option<i64>),
    name: String,
    path: Option<String>,
    started_ms: i64,
    chains: HashMap<String, Vec<(f64, f32)>>,
    meta: HashMap<String, (String, String)>,
    total: usize,
    lost: u64,
    context: Vec<ContextPoint>,
}

impl Session {
    fn leader(&self) -> Option<&String> {
        self.chains
            .iter()
            .max_by(|a, b| a.1.len().cmp(&b.1.len()).then(b.0.cmp(a.0)))
            .map(|(k, _)| k)
    }

    fn push(&mut self, f: FrameRecord) {
        if self.total >= MAX_SESSION_FRAMES {
            self.lost += 1;
            return;
        }
        self.total += 1;
        self.meta
            .entry(f.swap_chain.clone())
            .or_insert((f.runtime.clone(), f.present_mode.clone()));
        self.chains
            .entry(f.swap_chain)
            .or_default()
            .push((f.time_secs, f.frametime_ms as f32));
    }
}

fn context_point(state: &AppState, pid: u32) -> Option<ContextPoint> {
    let live = state.live.read();
    let f = live.frame.as_ref()?;
    let gpu = f.gpus.adapters.iter().max_by(|a, b| {
        a.util_pct
            .unwrap_or(0.0)
            .total_cmp(&b.util_pct.unwrap_or(0.0))
    });
    let proc_row = live.processes.rows.iter().find(|r| r.pid == pid);
    Some(ContextPoint {
        t: f.timestamp_ms,
        cpu_pct: f.cpu.total_pct,
        gpu_pct: gpu.and_then(|g| g.util_pct),
        gpu_temp_c: gpu.and_then(|g| g.temperature_c),
        cpu_temp_c: f.cpu.package_temp_c,
        vram_bytes: gpu.and_then(|g| g.dedicated_used_bytes),
        ram_used_bytes: f.memory.used_bytes,
        process_cpu_pct: proc_row.and_then(|r| r.cpu_pct),
        process_gpu_pct: proc_row.and_then(|r| r.gpu_pct),
    })
}

fn summarize(ctx: &[ContextPoint]) -> serde_json::Value {
    fn stat(v: Vec<f64>) -> serde_json::Value {
        if v.is_empty() {
            return serde_json::Value::Null;
        }
        let avg = v.iter().sum::<f64>() / v.len() as f64;
        let max = v.iter().copied().fold(f64::MIN, f64::max);
        serde_json::json!({ "avg": avg, "max": max, "samples": v.len() })
    }
    serde_json::json!({
        "cpuPct": stat(ctx.iter().filter_map(|c| c.cpu_pct.map(f64::from)).collect()),
        "gpuPct": stat(ctx.iter().filter_map(|c| c.gpu_pct.map(f64::from)).collect()),
        "gpuTempC": stat(ctx.iter().filter_map(|c| c.gpu_temp_c.map(f64::from)).collect()),
        "cpuTempC": stat(ctx.iter().filter_map(|c| c.cpu_temp_c.map(f64::from)).collect()),
        "vramBytes": stat(ctx.iter().filter_map(|c| c.vram_bytes.map(|v| v as f64)).collect()),
        "ramUsedBytes": stat(ctx.iter().filter_map(|c| c.ram_used_bytes.map(|v| v as f64)).collect()),
        "processCpuPct": stat(ctx.iter().filter_map(|c| c.process_cpu_pct.map(f64::from)).collect()),
        "processGpuPct": stat(ctx.iter().filter_map(|c| c.process_gpu_pct.map(f64::from)).collect()),
        "timeline": ctx,
    })
}

fn finalize(app: &AppHandle, state: &AppState, s: Session, cfg: &FrameStatsConfig) -> Option<i64> {
    let leader = s.leader().cloned()?;
    let frames = s.chains.get(&leader)?;
    if frames.len() < MIN_FRAMES_TO_SAVE {
        return None;
    }
    let ft: Vec<f64> = frames.iter().map(|(_, f)| f64::from(*f)).collect();
    let stats = compute(&ft, cfg);
    let (runtime, mode) = s.meta.get(&leader).cloned().unwrap_or_default();
    let mut stats_json = serde_json::to_value(&stats).unwrap_or_default();
    if let Some(o) = stats_json.as_object_mut() {
        o.insert("runtime".into(), runtime.into());
        o.insert("presentMode".into(), mode.into());
        o.insert("swapChain".into(), leader.clone().into());
        o.insert(
            "otherSwapChainFrames".into(),
            (s.total - frames.len()).into(),
        );
        o.insert("stutterFactor".into(), cfg.stutter_factor.into());
    }
    let session = NewSession {
        process_name: s.name,
        exe_path: s.path,
        pid: s.pid,
        started_ms: s.started_ms,
        ended_ms: now_ms(),
        provider: "PresentMon".into(),
        stats: stats_json,
        context: summarize(&s.context),
        frametimes_ms: frames.iter().map(|(_, f)| *f).collect(),
    };
    let (tx, rx) = std::sync::mpsc::channel();
    state
        .writer
        .send(WriteMsg::Session(Box::new(session), tx))
        .ok()?;
    match rx.recv_timeout(Duration::from_secs(30)) {
        Ok(Ok(id)) => {
            let _ = app.emit("gaming-session-saved", id);
            Some(id)
        }
        Ok(Err(e)) => {
            tracing::error!("saving game session failed: {e}");
            None
        }
        Err(_) => None,
    }
}

fn provider_status(state: &AppState) -> (Option<std::path::PathBuf>, Status) {
    let configured = state.settings.read().presentmon_path.clone();
    match find_presentmon(configured.as_deref(), &state.paths.tools) {
        Some(p) => (Some(p), Status::available()),
        None if configured.is_some() => (None, Status::new(Availability::ProviderUnavailable, "The configured PresentMon path is not a PresentMon*.exe file")),
        None => (
            None,
            Status::new(
                Availability::ProviderUnavailable,
                format!("PresentMon not found. Install Intel PresentMon (free, MIT) and select PresentMon*.exe in Settings, or place it in {}", state.paths.tools.display()),
            ),
        ),
    }
}

/// The running game to auto-capture: the one in front first, otherwise any, skipping
/// processes the user stopped or whose capture failed or ended.
fn pick_game<'a>(
    running: &[&'a veyl_core::ProcessRow],
    fg: Option<u32>,
    skip: &HashSet<(u32, Option<i64>)>,
) -> Option<&'a veyl_core::ProcessRow> {
    running
        .iter()
        .copied()
        .filter(|r| !skip.contains(&(r.pid, r.start_time_ms)))
        .max_by_key(|r| Some(r.pid) == fg)
}

/// Whether the captured process (pid, start time) is still in the process list.
/// An empty list means no sample yet, not that everything exited.
fn still_running(rows: &[veyl_core::ProcessRow], key: (u32, Option<i64>)) -> bool {
    rows.is_empty()
        || rows
            .iter()
            .any(|r| r.pid == key.0 && (key.1.is_none() || r.start_time_ms == key.1))
}

pub fn run(app: AppHandle, rx: Receiver<GamingCommand>) {
    let state = app.state::<AppState>();
    let mut provider: Option<PresentMonCli> = None;
    let mut session: Option<Session> = None;
    let mut last_publish = Instant::now();
    let mut last_context = Instant::now();
    let mut last_saved = None;
    let mut status = provider_status(&state).1;
    let mut last_scan = Instant::now();
    // Processes not to auto-capture again: stopped by the user, or the capture failed or ended.
    let mut no_auto: HashSet<(u32, Option<i64>)> = HashSet::new();
    let mut auto_started = false;
    let mut game_pids: Vec<u32> = Vec::new();

    loop {
        let mut cmd = match rx.recv_timeout(Duration::from_millis(250)) {
            Ok(c) => Some(c),
            Err(RecvTimeoutError::Timeout) => None,
            Err(RecvTimeoutError::Disconnected) => break,
        };
        if state.shutdown.load(Ordering::Relaxed) {
            break;
        }
        if cmd.is_some() {
            auto_started = false;
        }
        let mut target_closed = false;
        let fg_window = veyl_windows::elevation::foreground_window();
        let fg = fg_window.map(|(pid, _)| pid);
        if last_scan.elapsed() >= Duration::from_secs(1) {
            last_scan = Instant::now();
            let games = Games::current();
            let table = state.live.read().processes.clone();
            let running: Vec<&veyl_core::ProcessRow> = table
                .rows
                .iter()
                .filter(|r| {
                    r.path.as_deref().is_some_and(|p| {
                        games.is_game(p, table.app_pids.binary_search(&r.pid).is_ok())
                    })
                })
                .collect();
            no_auto.retain(|k| table.rows.iter().any(|r| (r.pid, r.start_time_ms) == *k));
            // PresentMon's own exit-on-close is not reliable, so watch the process too.
            target_closed = session
                .as_ref()
                .is_some_and(|s| !still_running(&table.rows, s.key));
            game_pids = running.iter().map(|r| r.pid).collect();

            let usable = status.state != Availability::ProviderUnavailable;
            // An automatic capture follows the game in front: a launcher that stays open
            // in the game's folder must not keep the game itself from being captured.
            let switch = auto_started
                && session
                    .as_ref()
                    .is_some_and(|s| fg.is_some_and(|f| f != s.pid && game_pids.contains(&f)));
            if cmd.is_none()
                && (session.is_none() || switch)
                && usable
                && state.settings.read().auto_capture_games
            {
                if let Some(s) = session.as_ref().filter(|_| switch) {
                    no_auto.insert(s.key); // do not switch back to it
                }
                if let Some(r) = pick_game(&running, fg, &no_auto) {
                    tracing::info!("auto-capturing {} ({})", r.name, r.pid);
                    cmd = Some(GamingCommand::Start { pid: r.pid });
                    auto_started = true;
                }
            }
        }
        // Checked every pass (not every scan) so the overlay follows a moving game window.
        let game_window = fg_window
            .filter(|(pid, _)| game_pids.contains(pid))
            .map(|(_, r)| r);
        if *state.game_window.read() != game_window {
            *state.game_window.write() = game_window;
            if let Some(w) = app.get_webview_window(crate::overlay::LABEL) {
                let cfg = state.settings.read().overlay.clone();
                crate::overlay::follow_game(&w, &cfg, game_window);
            }
        }
        let cfg = FrameStatsConfig {
            stutter_factor: state.settings.read().stutter_factor,
            ..Default::default()
        };
        let had_cmd = cmd.is_some();
        match cmd {
            Some(GamingCommand::Start { pid }) => {
                if let (Some(p), Some(s)) = (provider.as_mut(), session.take()) {
                    p.stop();
                    last_saved = finalize(&app, &state, s, &cfg).or(last_saved);
                }
                let (exe, st) = provider_status(&state);
                status = st;
                if let Some(exe) = exe {
                    let (name, path, started) = {
                        let live = state.live.read();
                        let row = live.processes.rows.iter().find(|r| r.pid == pid);
                        (
                            row.map(|r| r.name.clone())
                                .unwrap_or_else(|| format!("PID {pid}")),
                            row.and_then(|r| r.path.clone()),
                            row.and_then(|r| r.start_time_ms),
                        )
                    };
                    let mut p = PresentMonCli::new(exe);
                    match p.start(pid) {
                        Ok(()) => {
                            session = Some(Session {
                                pid,
                                key: (pid, started),
                                name,
                                path,
                                started_ms: now_ms(),
                                ..Default::default()
                            });
                            provider = Some(p);
                            status = Status::waiting("Starting PresentMon…");
                        }
                        Err(e) => {
                            status = Status::error(e);
                            no_auto.insert((pid, started)); // never retry a failed start in a loop
                        }
                    }
                }
            }
            Some(GamingCommand::Refresh) if session.is_none() => status = provider_status(&state).1,
            Some(GamingCommand::Refresh) => {}
            Some(GamingCommand::Stop) => {
                if let Some(mut p) = provider.take() {
                    p.stop();
                }
                if let Some(s) = session.take() {
                    no_auto.insert(s.key); // the user stopped it: do not start it again
                    last_saved = finalize(&app, &state, s, &cfg).or(last_saved);
                }
                status = provider_status(&state).1;
            }
            None => {}
        }

        if let (Some(p), Some(s)) = (provider.as_mut(), session.as_mut()) {
            let poll = p.poll();
            s.lost += poll.lost;
            for f in poll.frames {
                if f.pid == s.pid {
                    s.push(f);
                }
            }
            let ended = target_closed
                || matches!(
                    poll.status.state,
                    Availability::Stale | Availability::Error | Availability::PermissionRequired
                );
            status = if target_closed {
                Status::new(
                    Availability::Stale,
                    format!("Capture ended: {} closed", s.name),
                )
            } else {
                poll.status
            };
            if last_context.elapsed() >= Duration::from_secs(1)
                && s.context.len() < MAX_CONTEXT_SECONDS
            {
                last_context = Instant::now();
                if let Some(c) = context_point(&state, s.pid) {
                    s.context.push(c);
                }
            }
            if ended {
                provider = None;
                if let Some(s) = session.take() {
                    tracing::info!("capture of {} ended ({} frames)", s.name, s.total);
                    no_auto.insert(s.key); // forgotten once the process exits
                    last_saved = finalize(&app, &state, s, &cfg).or(last_saved);
                }
            }
        }

        if last_publish.elapsed() >= Duration::from_millis(500) || had_cmd {
            last_publish = Instant::now();
            let mut live = GamingLive {
                provider: "PresentMon".into(),
                provider_status: status.clone(),
                last_saved_session: last_saved,
                ..Default::default()
            };
            if let Some(s) = &session {
                live.capturing = true;
                live.auto_started = auto_started;
                live.pid = Some(s.pid);
                live.process_name = Some(s.name.clone());
                live.exe_path = s.path.clone();
                live.started_ms = Some(s.started_ms);
                live.duration_secs = (now_ms() - s.started_ms) as f64 / 1000.0;
                live.lost_frames = s.lost;
                live.context = s.context.last().cloned();
                if let Some(leader) = s.leader() {
                    let frames = &s.chains[leader];
                    live.frames = frames.len() as u64;
                    live.other_swap_chain_frames = (s.total - frames.len()) as u64;
                    if let Some((rt, pm)) = s.meta.get(leader) {
                        live.runtime = Some(rt.clone());
                        live.present_mode = Some(pm.clone());
                    }
                    // Newest frames covering the live window.
                    let mut span = 0.0;
                    let mut start = frames.len();
                    while start > 0 && span < LIVE_WINDOW_MS {
                        start -= 1;
                        span += f64::from(frames[start].1);
                    }
                    let window: Vec<f64> =
                        frames[start..].iter().map(|(_, f)| f64::from(*f)).collect();
                    live.live = compute(&window, &cfg);
                    live.recent_frametimes = frames[frames.len().saturating_sub(RECENT_FRAMES)..]
                        .iter()
                        .map(|(_, f)| *f)
                        .collect();
                }
            }
            *state.gaming.write() = live.clone();
            if state.ui_visible.load(Ordering::Relaxed)
                || state.overlay_open.load(Ordering::Relaxed)
            {
                let _ = app.emit("gaming", &live);
            }
        }
    }
    if let Some(mut p) = provider.take() {
        p.stop();
    }
    if let Some(s) = session.take() {
        let cfg = FrameStatsConfig {
            stutter_factor: state.settings.read().stutter_factor,
            ..Default::default()
        };
        finalize(&app, &state, s, &cfg);
    }
    tracing::info!("gaming manager stopped");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_capture_prefers_the_game_in_front_and_skips_stopped_ones() {
        let row = |pid: u32| veyl_core::ProcessRow {
            pid,
            start_time_ms: Some(i64::from(pid)),
            ..Default::default()
        };
        let (a, b) = (row(10), row(20));
        let running = [&a, &b];
        let none = HashSet::new();
        assert_eq!(
            pick_game(&running, Some(10), &none).map(|r| r.pid),
            Some(10)
        );
        assert_eq!(
            pick_game(&running, Some(20), &none).map(|r| r.pid),
            Some(20)
        );
        assert!(
            pick_game(&running, None, &none).is_some(),
            "a background game still counts"
        );
        let stopped: HashSet<_> = [(20, Some(20))].into();
        assert_eq!(
            pick_game(&running, Some(20), &stopped).map(|r| r.pid),
            Some(10),
            "a stopped game is not restarted"
        );
        let both: HashSet<_> = [(10, Some(10)), (20, Some(20))].into();
        assert!(pick_game(&running, Some(20), &both).is_none());
        assert!(pick_game(&[], Some(1), &none).is_none());
    }

    #[test]
    fn capture_target_is_gone_only_when_missing_from_a_real_list() {
        let row = veyl_core::ProcessRow {
            pid: 10,
            start_time_ms: Some(5),
            ..Default::default()
        };
        let rows = [row];
        assert!(still_running(&rows, (10, Some(5))));
        assert!(still_running(&rows, (10, None)), "start time unknown");
        assert!(
            !still_running(&rows, (10, Some(6))),
            "PID reused by a new process"
        );
        assert!(!still_running(&rows, (11, Some(5))));
        assert!(still_running(&[], (11, Some(5))), "no sample yet");
    }

    fn rec(chain: &str, ft: f64) -> FrameRecord {
        FrameRecord {
            pid: 1,
            application: "g".into(),
            swap_chain: chain.into(),
            runtime: "DXGI".into(),
            present_mode: "Hardware: Independent Flip".into(),
            time_secs: 0.0,
            frametime_ms: ft,
            display_ms: None,
            displayed: None,
        }
    }

    #[test]
    fn session_tracks_leading_swap_chain() {
        let mut s = Session::default();
        s.push(rec("overlay", 100.0));
        for _ in 0..5 {
            s.push(rec("game", 16.0));
        }
        assert_eq!(s.leader().map(String::as_str), Some("game"));
        assert_eq!(s.total, 6);
    }

    #[test]
    fn context_summary_handles_missing_values() {
        let ctx = vec![
            ContextPoint {
                cpu_pct: Some(50.0),
                ..Default::default()
            },
            ContextPoint {
                cpu_pct: Some(70.0),
                gpu_temp_c: None,
                ..Default::default()
            },
        ];
        let v = summarize(&ctx);
        assert_eq!(v["cpuPct"]["avg"], 60.0);
        assert!(v["gpuTempC"].is_null(), "no fabricated temperature");
    }
}
