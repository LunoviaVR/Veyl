# Architecture

Veyl is a Tauri 2 application. Rust owns every system interaction,
calculation, rule and persistence step; the React/TypeScript frontend only renders
data and sends user intent back through typed commands.

## Workspace

```
crates/
  core/               veyl-core    Pure Rust, no OS calls. Models, statistics, event interpretation,
                                   alert engine, diagnostic rules, redaction, aggregation.
  windows-platform/   veyl-windows Every Windows API call: collectors, process control, event log,
                                   storage IOCTLs, external tools (PresentMon, cdb), elevation.
  history-storage/    veyl-storage SQLite: migrations, batched writes, rollups, retention, queries.
src-tauri/            app shell    Threads, scheduling, shared state, IPC commands, tray, overlay,
                                   notifications, exports, settings.
src/                  React UI     Pages, components, typed IPC wrappers, live store.
```

### Module map (spec module → location)

| Responsibility | Location |
|---|---|
| core | `crates/core/src/model.rs` |
| windows-platform | `crates/windows-platform/src/{handle,pdh,registry,elevation,version_info}.rs` |
| system-telemetry | `veyl-windows::{cpu,memory,disk,network,system_info}` |
| process-monitor | `veyl-windows::{processes,process_control,services,startup,connections}` |
| hardware-monitor / sensor-providers | `veyl_core::HardwareSensorProvider`, `veyl-windows::{thermal,energy,adlx,nvml,storage_health}`, `veyl_core::sensor_map` |
| gpu-monitor | `veyl-windows::gpu` |
| gaming-monitor / frame-capture | `veyl_core::{frames,frame_stats}`, `veyl-windows::presentmon`, `src-tauri/src/gaming.rs` |
| crash-monitor / event-log-reader | `veyl-windows::eventlog`, `veyl_core::events`, `src-tauri/src/events_watch.rs` |
| bsod-analyzer | `veyl_core::{events::group_shutdown_incidents, codes}`, `veyl-windows::debugger` |
| packet-capture | `veyl-windows::packet_capture` (Windows NDIS capture via ETW, no driver installed), `veyl_core::{packet,packet_filter}`, `src-tauri/src/packets.rs` |
| pc-specs | `veyl_core::specs` (SMBIOS), `veyl-windows::{display,app_windows}` |
| diagnostic-engine | `veyl_core::diagnostics` |
| alert-engine | `veyl_core::alerts`, `src-tauri/src/notify.rs` |
| history-storage | `crates/history-storage` |
| export-service | `src-tauri/src/export.rs`, `src-tauri/src/commands/export.rs`, `veyl_core::redact` |
| settings | `src-tauri/src/settings.rs` |

## Data flow

```
 Windows APIs (PDH, NtQuerySystemInformation, DXGI, D3DKMT, IP Helper, IOCTL, wevtapi)
 Vendor libs (ADLX, NVML), Energy Meter    PresentMon CLI (optional)        Event Log service
            │                                       │                             │
            ▼                                       ▼                             ▼
 ┌─────────────────────┐                 ┌───────────────────┐        ┌───────────────────┐
 │  sampler thread     │                 │  gaming thread    │        │  event-log thread │
 │  collectors → frame │                 │  frames → stats   │        │  back-fill + push │
 │  metrics, alerts    │                 │  context, session │        │  subscription     │
 └──┬──────┬───────┬───┘                 └───┬──────────┬────┘        └──┬────────────┬───┘
    │      │       │ try_send (bounded)       │          │ send            │ send        │
    │      │       ▼                          │          ▼                 ▼             │
    │      │   ┌──────────────────────── db-writer thread ─────────────────────┐          │
    │      │   │ batched transactions · minute rollups · hourly retention      │          │
    │      │   └──────────────────────────────┬────────────────────────────────┘          │
    │      │                                   ▼                                         │
    │      │                            SQLite (WAL) ◄── read connection (commands)       │
    │      ▼                                                                             │
    │  AppState.live (Arc snapshots, swapped under a short lock)                         │
    ▼                                                                                    ▼
 Tauri events: telemetry (1/tick, only while a window is visible) · gaming · alert · stability-changed
            │
            ▼
 React: live store (bounded 300-frame ring) → pages; commands poll page-specific data
```

### Threads and lifecycle

| Thread | Owns | Wakes | Stops |
|---|---|---|---|
| `sampler` | All collectors and sensor providers, bucket aggregator, alert engine | Every `sample_interval_ms` (500–5000) via `recv_timeout` on a stop channel | Stop sender dropped at exit → immediate |
| `db-writer` | The write connection | Messages; 2 s idle tick for rollups/retention | Drains its queue when `shutdown` is set |
| `event-log` | Two `EvtSubscribe` subscriptions | Win32 event signalled by the Event Log service | Checks `shutdown` every 250 ms |
| `gaming` | `PresentMonCli` (child process + reader thread) | 250 ms poll / commands | Checks `shutdown`; stops PresentMon and its ETW session |

`RunEvent::Exit` sets `shutdown`, drops the sampler's stop sender, then joins every
thread. Collectors release OS handles in `Drop` (PDH queries, D3DKMT adapters, event
log handles, mapped views, process handles).

### Backpressure and isolation

- The sampler never blocks on storage: it uses `try_send` on a 256-message queue;
  a full queue drops the batch and increments a visible counter.
- UI slowness cannot delay sampling: events are fire-and-forget; the frontend keeps
  only the newest 300 frames.
- Each sensor provider runs inside `catch_unwind`; a panic or error becomes that
  provider's status. Every snapshot section carries its own `Status`.
- Events are emitted only while the main window is visible or the overlay is open.

### Identity and timing

- A process is `(pid, creation time)`; rates are computed only against the same
  identity, and process actions re-verify the creation time before acting.
- Rates use `Instant` deltas; wall-clock milliseconds are used only for timestamps.
- GPUs are keyed by LUID internally and by enumeration index (`gpu.0`) in history.

## Data model rules

- Any value a source can fail to provide is `Option`; `None` is rendered as "—".
  Zero is shown only when zero was measured.
- Field names carry units (`_bytes`, `_bps`, `_pct`, `_mhz`, `_c`, `_ms`).
- Windows events are stored raw (channel, record id, provider, id, level, data) and
  classified on read by `veyl_core::events::classify`, which validates channel,
  provider **and** event ID. Improving the classifier needs no data migration.
- The XPath queries sent to the Event Log service are generated from the same rule
  table the classifier uses (`xpath_for_channel`).

## Security boundaries

- The app runs `asInvoker`. Elevation happens only through an explicit
  "Restart as administrator" action (UAC prompt).
- Tauri capability: `core:default` only. No filesystem, shell or HTTP plugins are
  exposed to the webview; file dialogs, opening paths and notifications are done in
  Rust after validation.
- Every command validates its arguments (ranges, list sizes, string lengths, paths).
  `reveal_path` accepts only existing absolute paths; `analyze_dump` accepts only a
  dump listed from the Windows dump folders.
- External tools run with fixed argument lists (`std::process::Command`, no shell).
  `cdb.exe` gets a controlled symbol path; inherited `_NT_SYMBOL_PATH` is removed.
- CSP: `default-src 'self'`; IPC only.
