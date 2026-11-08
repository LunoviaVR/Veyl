# Veyl engineering guide

Veyl is a Windows desktop monitor in one window:
- **Task Manager**: process list (CPU, RAM, I/O, GPU, VRAM per process), End task, CPU/RAM/disk/network totals.
- **HWiNFO**: hardware summary (CPU, RAM, GPUs, driver) and a sensor table with current/min/max/avg that keeps recording while the window is minimized.
- **Gaming**: FPS, frame times, 1% and 0.1% lows and stutters for the game in front, plus that process's CPU, RAM, GPU and VRAM.

Stack: Tauri 2, Rust backend, static vanilla-JS frontend. Windows 10 2004+ only (it is the only OS with the GPU counters and present events we read). No cross-platform shims.

Veyl runs **asInvoker** (the tauri-build default manifest). Everything works unelevated except FPS capture: a real-time ETW session needs administrator or membership in Performance Log Users. Unelevated, the gaming status reads `FPS needs Veyl run as administrator (or Performance Log Users); close Veyl first`.

## 1. Layout

```
.gitignore                src-tauri/target/, src-tauri/gen/ (tauri-build writes gen/schemas)
check.sh                  the gate: cargo fmt --check, clippy -D warnings, cargo test, node --test
GUIDE.md
ui/                       Tauri frontendDist: static ES modules, no bundler, no npm
  index.html style.css app.js   DOM, polling, rendering
  stats.js  stats.test.js       pure helpers, tested with node:test
src-tauri/
  Cargo.toml Cargo.lock build.rs tauri.conf.json icons/
  src/main.rs             the windows_subsystem cfg_attr line + fn main() { veyl_lib::run() }
  src/lib.rs              mod list, AppState, sampler thread, #[tauri::command]s, Builder, RunEvent::Exit
  src/system.rs           sysinfo: CPU, memory, processes, disk/net rates, ACPI temperatures; kill_process
  src/gpu.rs              DXGI adapters, PDH GPU counters, D3DKMT perf data (temperature, fan, power)
  src/sensors.rs          pure min/max/avg tracker for the sensor table
  src/frame_stats.rs      pure frame-time math
  src/present.rs          ETW present capture -> per-(pid, swapchain) frame times; game selection
```

`Cargo.lock` is committed and changes only together with `Cargo.toml`. Generated files under `src-tauri/gen/` are ignored and exempt from the unit file rule.

## 2. How the house patterns map here

Nexus (`D:\GitHub\Nexus`) is the house Tauri app. Veyl keeps its shape and drops what a small app does not need.

| Nexus | Veyl |
|---|---|
| `main.rs` only calls `nexus_lib::run()` | same, `veyl_lib::run()`, plus the `windows_subsystem` cfg_attr |
| `tauri-plugin-single-instance` registered first | same: a second launch focuses the existing window |
| `state.rs`: `AppState` with `parking_lot` locks, `app.manage()` in `setup` | `AppState` lives in `lib.rs` and holds only the latest snapshot and the sampler thread handle (section 4) |
| `commands.rs`: `#[tauri::command]` fns returning `Result<T>` | commands live in `lib.rs`, are sync `fn`s marked `#[tauri::command(async)]`, return `T` or `Result<T, String>` |
| `error.rs`: thiserror enum serialized as a string | no error type. A failure the user must see is a `String`. A sampler that cannot start reports `Option<String>` status in the snapshot and the app keeps running |
| React + Vite + TS | vanilla ES modules, no build step. `withGlobalTauri: true`, call `window.__TAURI__.core.invoke` |
| one-line "why" comments | same |

## 3. Naming

- Modules are snake_case nouns. A sampler is `struct <Thing>Sampler` with `new()` and `sample(&mut self) -> <Thing>Snapshot`.
- Anything serialized derives `Serialize` with `#[serde(rename_all = "camelCase")]` and ends in `Snapshot`, `Info`, `Summary` or `Stats`.
- Field names carry their unit: `_bytes`, `_bps` (bytes per second), `_ms`, `_mhz`, `_c` (degrees Celsius), `_pct` (0..=100), `_rpm`, `_secs`. Never a bare `memory`, `temp`, `usage`. JS sees `usedBytes`, `readBps`, `cpuPct`. Nothing in this stack reports watts: there is no `_mw`.
- Percent is always an `f32` in 0..=100.
- Process CPU % is a machine share (sysinfo per-process value / logical CPU count, clamped), time-based. It reads lower than Task Manager's frequency-scaled column under turbo. That is expected.
- Per-process disk numbers from sysinfo are all I/O (file, network, device): name them `io_read_bps` / `io_write_bps` and the column "I/O". Disk totals (`disk_read_bps`, `disk_write_bps`) come from sysinfo `Disks`.
- Commands are `verb_noun`: `get_snapshot`, `get_hardware`, `kill_process`, `reset_sensors`. JS invokes the same names.
- JS: camelCase functions, `const` by default, DOM ids kebab-case.

## 4. Ownership and threading

- **One sampler thread**, spawned in `setup`, creates, owns, samples and drops every sampler (`SystemSampler`, `GpuSampler`, `PresentCapture`, the sensor tracker) at 1 Hz. Samplers never cross threads, so there is no `unsafe impl Send` or `Sync` anywhere.
- The thread waits on a stop channel with `recv_timeout(1 s)`; a message or a disconnect ends the loop and drops the samplers.
- Each tick it builds a complete `Snapshot`, updates sensor min/max/avg, prunes histories, and swaps the snapshot into `AppState.latest: Mutex<Snapshot>`. `get_snapshot` returns a clone. Sampling keeps going while the window is minimized (WebView2 throttles hidden timers and Tauri cannot turn that off on Windows), so min/max/avg cover a whole gaming session.
- `AppState` also holds `Mutex<Option<(Sender<()>, JoinHandle<()>)>>` for the sampler thread. Requests to the thread (reset sensors) are an `AtomicBool` or a channel message, never a shared sampler.
- **Lock rule**: a lock is held only to copy, push, take or swap. No code path holds two locks. Never sample, join, send/recv, or call the OS while holding a lock.
- Commands are sync `fn`s marked `#[tauri::command(async)]`. A plain sync command runs on the main (UI) thread (tauri-macros `lib.rs`: "runs the command on the async runtime instead of blocking the main thread").
- The ETW consumer thread is the only other thread. It shares an `Arc<parking_lot::Mutex<_>>` with `PresentCapture`. The raw pointer handed to ETW as callback context points into an allocation that `PresentCapture` keeps alive until after that thread is joined.
- Rates of cumulative counters (bytes/s) are delta / measured elapsed seconds between two samples (`Instant`). Never assume 1 s. The first sample reports 0. Prefer sysinfo's per-refresh deltas. Any subtraction of two cumulative readings uses `saturating_sub` (drivers reset counters on a TDR). Frame metrics never use `Instant`: see section 7.

## 5. Cleanup

- Every OS handle lives in a struct whose `Drop` releases it. Release explicitly in `Drop`; do not rely on field declaration order.
  - PDH query: `PdhCloseQuery`.
  - D3DKMT adapter: `D3DKMTCloseAdapter`.
  - COM interfaces from the `windows` crate release on drop. Never call `Release` by hand.
- **ETW**. `PresentCapture` holds each resource in an `Option`. `new()` builds the struct before acquiring anything, so an early `return Err` drops whatever was already acquired. `Drop` is the only teardown path and releases what is present in exactly this order: `ControlTraceW(.., EVENT_TRACE_CONTROL_STOP)`, `CloseTrace`, join the consumer thread, free the callback context. (Joining first would hang: a real-time `ProcessTrace` returns only after the session stops or the trace is closed.)
- **Exit**. `RunEvent::Exit` takes the sampler `(Sender, JoinHandle)` out of its `Mutex`, releases the lock, sends stop, and joins. The capture drops when the sampler thread exits. Tauri does not promise to drop managed state, so this path is required.
- **Single instance**. `tauri-plugin-single-instance` is the first plugin registered. ETW sessions are system-wide and outlive a crashed process: `StartTraceW` returning `ERROR_ALREADY_EXISTS` means a leftover `Veyl-Present`; stop it and retry once. Carry `// shortcut: the plugin mutex is per logon session; another Windows user's elevated Veyl can still stop ours, use per-pid session names if that matters`.
- If `ProcessTrace` returns on its own (session stopped from outside), the consumer thread stores its return code in shared state and the status reads `FPS capture stopped (code N)`.
- `Drop` never panics. Log close errors with `eprintln!` and continue.

## 6. Errors and unsafe

- No `unwrap`/`expect` on OS results or external data. `expect("why it cannot fail")` only for invariants of our own code.
- A sampler that fails degrades: its part of the snapshot is empty plus a status string. The app keeps running.
- `unsafe` blocks are as small as possible and each carries a one-line `// SAFETY:` comment naming the invariant.
- Check every Win32 return code (`HRESULT`, `NTSTATUS`, PDH status, ETW `WIN32_ERROR`). An ignored code is a bug. These non-success codes are expected and mean what is listed:

| Call | Code | Meaning |
|---|---|---|
| `ControlTraceW` STOP | `ERROR_SUCCESS`, `ERROR_MORE_DATA`, `ERROR_WMI_INSTANCE_NOT_FOUND` (4201) | stopped / nothing to stop |
| `CloseTrace` | `ERROR_SUCCESS`, `ERROR_CTX_CLOSE_PENDING` (7007) | OK |
| `StartTraceW` | `ERROR_ALREADY_EXISTS` | stop the leftover session, retry once |
| `StartTraceW` | `ERROR_ACCESS_DENIED` | set the asInvoker status above |
| `OpenTraceW` | returns `INVALID_PROCESSTRACE_HANDLE` (`Value == u64::MAX`) | failure; read the code with `GetLastError` |
| PDH size call | `PDH_MORE_DATA` | expected; allocate and call again |
| PDH read before the second collect | `PDH_INVALID_DATA` | no data yet, not a failure |
| PDH array items | `CStatus` not `PDH_CSTATUS_VALID_DATA` / `PDH_CSTATUS_NEW_DATA` | skip that item |

- Buffer sizes come from the API's size query (the two-call pattern), or from the documented layout where the API has none.
  - PDH array buffers are `Vec<u64>` of `ceil(bytes / 8)`: `PDH_FMT_COUNTERVALUE_ITEM_W` needs 8-byte alignment. Keep the buffer alive while reading `szName`, which points into it.
  - `EVENT_TRACE_PROPERTIES` has no size query: size = `size_of::<EVENT_TRACE_PROPERTIES>() + (name UTF-16 length + 1) * 2`, `LoggerNameOffset = size_of::<EVENT_TRACE_PROPERTIES>()`, buffer 8-aligned.
- Every GUID, event id, keyword, unit conversion and magic constant carries a one-line source comment (header file:line, crate file:line, or doc URL). A reviewer rejects one that has none.

## 7. Data

- Sizes are `u64` bytes. Formatting belongs to JS.
- **sysinfo** is built with `default-features = false, features = ["system", "disk", "network", "component"]`. Its `gpu` feature is wrong for us (usage sums every engine past 100 %, used memory is Veyl's own VRAM). All GPU numbers come from `gpu.rs`.
- **Temperatures from sysinfo** are ACPI thermal zones (WMI `MSAcpi_ThermalZoneTemperature`): usually admin-only and often absent. Show them under their reported label, never as "CPU". Refresh them only on the sampler thread (sysinfo initialises COM as MTA, which fails on the STA main thread). Empty means a status line. A CPU package temperature needs a kernel driver and is out of scope.
- **GPU (PDH)**. Add counters only with `PdhAddEnglishCounterW` (counter names are localized). An engine is `(luid, phys, eng)` parsed from the instance name. Process GPU % = max over that pid's engines. Adapter GPU % = for each engine, sum over processes, then max over engines. Clamp both to 0..=100 (Task Manager's busiest-engine rule; engines of the same type are separate hardware, so never sum by engtype). Adapter VRAM used = `\GPU Adapter Memory(*)\Dedicated Usage` per LUID; total = DXGI `DedicatedVideoMemory`. Process VRAM = `\GPU Process Memory(*)\Dedicated Usage` summed per pid.
- **GPU (D3DKMT_ADAPTER_PERFDATA)**, units from `d3dkmthk.h`: `temp_c = Temperature / 10` (deci-°C), `power_pct = Power / 10` (tenths of a percent), `fan_rpm = FanRPM`, MemoryFrequency is Hz (divide by 1e6 for `_mhz`). A reading of 0 is `None`.
- **ETW present capture**.
  - Session `Veyl-Present`: `Wnode.ClientContext = 1` (QPC clock), `WNODE_FLAG_TRACED_GUID`, `EVENT_TRACE_REAL_TIME_MODE`, `FlushTimer = 1` (0 delivers buffers only when full, which freezes the panel for seconds).
  - Providers, verified with `wevtutil gp <provider> /ge /gm`: Microsoft-Windows-DXGI `{CA11C036-0102-4A2D-A6AD-F03CFED5D3C9}` Present_Start id 42 version 0; Microsoft-Windows-D3D9 `{783ACA0A-790E-4D7F-8451-AA850511C6B9}` Present_Start id 1 version 0. Enable each with `EnableTraceEx2`, `TRACE_LEVEL_INFORMATION`, `MatchAnyKeyword = 0x8000000000000002`.
  - Consumer: `ProcessTraceMode = PROCESS_TRACE_MODE_REAL_TIME | PROCESS_TRACE_MODE_EVENT_RECORD | PROCESS_TRACE_MODE_RAW_TIMESTAMP`. Without `RAW_TIMESTAMP`, `TimeStamp` arrives as FILETIME, not QPC. Convert ticks with `QueryPerformanceFrequency`. Any "now" compared with event timestamps comes from `QueryPerformanceCounter`, never `Instant`.
  - **The callback never panics** (a panic in an `extern "system"` fn aborts the app and orphans the session). Match `ProviderId`, `Id` and `Version` first. Check `UserDataLength`, then read with `get()` / `read_unaligned` only. Pointer-sized payload fields are 4 bytes when `EventHeader.Flags` has `EVENT_HEADER_FLAG_32_BIT_HEADER` (0x20). No indexing, no unchecked arithmetic. A timestamp <= the key's last one is dropped and the last stays unchanged (real-time buffers arrive out of order across CPUs). The callback only records; it does nothing else.
  - Frames are keyed by `(pid, swapchain)`, swapchain = Present_Start payload field 0 (pointer-sized). Cite the payload layout source. Keying by pid alone silently doubles FPS when one process presents on two swapchains.
- **Frame statistics** (`frame_stats.rs`, pure):
  - `frame_ms` = Δ QPC ticks between consecutive presents of one key × 1000 / QPF.
  - Window = frames with timestamp >= newest − 10 s, capped at 20 000 frames.
  - `fps` = 1000 / mean frame time of the frames with timestamp >= newest − 1 s.
  - N % low = 1000 / mean of the slowest `ceil(N / 100 × n)` frame times of the window (1 % and 0.1 %).
  - stutter = a frame time > 2 × the window median.
  - No frame times -> `None`. Never report 0 for "no data".
- **The game**: the most recent foreground pid other than Veyl's own (sampled each tick) that has a key with a frame in the last 10 s; its stats come from its key with the most frames in the window. If the newest frame is more than 2 s older than QPC now, the game is not presenting. Otherwise every frame stat is `null` with a status: `no DXGI/D3D9 presents from <name> (Vulkan/OpenGL not captured)` or `no game in front`. Carry `// shortcut: UWP windows report ApplicationFrameHost's pid`.
- External strings (PDH instance names, process names) are parsed by pure functions with unit tests built from real-format examples. A malformed value is skipped, never a panic.
- Every history is bounded: time-pruned `VecDeque` plus a hard length cap. Per-key maps evict keys idle for more than 10 s.
- Verify a crate or Win32 API by reading its source in `~/.cargo/registry/src` or the SDK headers. Do not guess signatures, units or ids.

## 8. Process control

`kill_process(pid, start_time_secs)`:
- Refuse pid 0, pid 4 and Veyl's own pid.
- `OpenProcess(PROCESS_TERMINATE | PROCESS_QUERY_LIMITED_INFORMATION)`.
- Creation time from `GetProcessTimes`, converted the way sysinfo does (`FILETIME / 10^7 − 11 644 473 600`), must equal `start_time_secs` from the snapshot. A mismatch means the pid was reused: refuse.
- Refuse if `IsProcessCritical`.
- `TerminateProcess` on that same handle. The handle closes on drop.
- Never sysinfo's `kill`: on Windows it spawns `taskkill.exe /F` and waits on it.

## 9. Frontend

- `stats.js` is pure (no DOM) and tested with `node:test`. `app.js` imports from it.
- Never put backend data into `innerHTML`: process names are controlled by whoever started the process. Use `textContent` and `createElement`.
- One timer drives polling. A tick still in flight makes the next tick skip; ticks never overlap.
- End task asks `confirm()` naming the process and pid first, and passes the snapshot's start time.
- CSP in `tauri.conf.json` is exactly `default-src 'self'; connect-src ipc: http://ipc.localhost`. Never `null`. No inline scripts, no `eval`, no remote resources.

## 10. Tests

- `bash check.sh` is the gate. It is green before a unit goes to review and after every fix.
- Pure logic (math, parsing, aggregation, formatting) gets unit tests with concrete inputs, including empty, single element, zero, huge and malformed inputs.
- Code that calls the OS is not unit-tested. Pull its parsing and aggregation into pure functions, and test those.
- Never weaken, `#[ignore]`, `.skip`, or delete a test to get green.

## 11. Units, review, commits

- A unit lists its files. Touch nothing else. If the work needs another file, stop and say which and why.
- A reviewer rejects a change that: stubs or skips real logic to make errors disappear; needs a paragraph-long comment to justify a workaround; weakens, skips or deletes a test; touches files outside the unit.
- A known limit gets one comment: `// shortcut: <the limit>, <when to upgrade>`.
- Commit messages: `feat(<unit>): <what>`, `fix(<unit>): <what>`. Commit only the unit's files.
- Never run `git stash`, `git reset`, `git checkout -- <path>`, `git clean`, or any other destructive git command.

## 12. Learned rules

Rules added when the same kind of bug showed up twice. Each one names the bug it prevents.

- **wrong-units-or-rate**: Every rate divides its delta by that delta's own elapsed: an `Instant` taken right next to the refresh that produced it (one for disks+networks, one just before `refresh_processes_specifics`), never a tick-wide `now`; the first sample of each I/O rate is 0, never a lifetime total.
  CPU % is never published unprimed: `SystemSampler::new()` refreshes CPUs and processes (with CPU), sleeps `sysinfo::MINIMUM_CPU_UPDATE_INTERVAL`, refreshes processes (with CPU) again, and the loop waits one tick before its first `sample()`.
  A process first seen after `new()` publishes `cpu_pct: None` (`Option<f32>`, never 0) until its third refresh, keyed by `(pid, start_time)` in a set pruned of gone processes, gated by a pure unit-tested helper. Prevents: disk/net/process I/O rates skewed by how long the other refreshes took, and a first CPU figure (machine-wide at startup, or for a process started later) that is 0 or a since-boot average.
- **unchecked-os-error**: No OS or sysinfo failure becomes a value: an `HRESULT`/`NTSTATUS`/PDH status, a sysinfo `Err` (`host_name`, `boot_time`, `uptime`, `physical_core_count`, `long_os_version`, `kernel_version`), or a `refresh_processes_specifics` count of 0 (a good refresh always counts Idle and System). A diff applying `.ok()`, `.is_ok().then_some(..)`, `if let Ok`, `unwrap_or_default`, `unwrap_or(0)` or `let _ =` to one is rejected; a wrapper returns `Result<_, code>`, never `Option`.
  The field it feeds becomes `None` (make it an `Option`; never `""` or `0` as a stand-in) and the failure goes into that struct's `status` (`GpuSnapshot`, `SystemSnapshot`, `SystemInfo`) every tick it lasts, naming call, item and code: `"D3DKMTQueryAdapterInfo(ADAPTERPERFDATA) failed for {name} (0x{status:08X}): no temperature, fan or power"`. A process refresh counting 0 publishes no rows and resets `last_process_refresh` to `None`.
  A section 6 table code is exempt only while its condition is tracked in code: `PDH_INVALID_DATA` only while `PdhQuery`'s own count of successful `PdhCollectQueryData` calls is below 2, after that `Err("reading <counter> failed (0xC0000BC6)")`. Prevents: sensor fields, `driver_version`, VRAM used, host name, boot time or the process table silently `None`/`0`/`""`/empty with no status (`D3DKMTQueryAdapterInfo` or `CheckInterfaceSupport` failing, `PDH_INVALID_DATA` after a driver reset, a failed Toolhelp snapshot followed by lifetime I/O published as a rate).
- **edge-case-math**: Before summing or deduplicating OS items, name the physical thing a value measures, and key by an id that is exactly one-to-one with it. sysinfo returns one `Disk` per mount path, and `Disk::id()` is a 32-bit volume serial that clones share. So disk I/O is summed once per volume GUID path from `GetVolumeNameForVolumeMountPointW`, resolved when the disk list changes, never per tick. A disk whose path fails to resolve counts on its own and is never merged. `disk_io` has a test for two mount paths of one volume (counted once) and one for two volumes with the same serial (both counted).
  Prevents: `disk_read_bps`/`disk_write_bps` doubled for a volume mounted at `E:\` and `C:\Data\`, or a 400 MB/s write to a sector-cloned `D:` dropped because it shares `C:`'s serial.
- **guide-violation**: `AppState` has exactly two fields, `latest` and `sampler`. Other shared data such as `HardwareInfo` is a `#[serde(skip)] Option` inside the swapped `Snapshot`. It is stored before the CPU-priming wait and rebuilt only when the GPU adapters are enumerated again. A `OnceLock`, a third `Mutex` or a static in a diff is rejected.
  Nothing unmeasured reaches the frontend as `0` or as a section with no `status`. `get_snapshot` returns `Option<Snapshot>` and stays `None` until the first `sample()` result is stored. A GPU value with no successful PDH reading this tick is `None`, never `GpuCounters::default()`: this covers a PDH query that never opened, `read_all` returning `Err`, and no instance for the adapter's LUID. That makes `util_pct`, `dedicated_used_bytes` and `shared_used_bytes` `Option`s. Only a successful engine read with no engine instances means idle and publishes `0`.
  A sampler whose `new()` fails never ends the thread with only an `eprintln!`. The other samplers keep running, and the failed sampler's section is empty with the error as its `status`. Prevents: a third `AppState` field (`OnceLock<HardwareInfo>`) that cannot be refreshed; `GPU 0 %, VRAM 0 B / 8 GB` shown when PDH failed; and, during startup or after `SystemSampler::new()` fails, CPU 0 %, RAM 0 of 0 and a GPU section with no status served forever.
