# Performance

## Measured overhead

Release build 0.1.0, default settings (1 s system sampling, 2 s sensors), fresh data
folder, measured with `scripts/measure-overhead.ps1` on 2026-11-07.
Machine: Intel Core i5-13600KF (20 logical CPUs), 64 GB RAM, Radeon RX 9060 XT,
Windows 11 26300. About 700 processes running. Each scenario is a 120 s window after a
30 s warm-up. "Total" includes the application process and its WebView2 processes.

| Metric | Window visible (Overview) | Minimized to tray |
|---|---|---|
| Total CPU, % of the whole machine | **0.12 %** | **0.06 %** |
| Total CPU, % of one core | 2.3 % | 1.3 % |
| Backend (Rust) CPU, % of one core | 1.1 % | 1.1 % |
| Backend working set | 76 MB | 76 MB |
| Total working set (incl. WebView2, shared pages counted per process) | 484 MB | 482 MB |
| Total private memory | 251 MB | 233 MB |
| Disk writes, all I/O (database, logs, WebView2 cache) | 39 KB/s | 21 KB/s |

Per-tick sampling cost (shown live on Overview and in Settings): about 14 ms in debug
builds; the release build's backend CPU above includes all sampling, PDH queries,
process enumeration, GPU queries, ADLX and history writes.

History database growth: roughly 40 metrics × one 5-second row = ~0.7 MB/hour of
5-second data, pruned after 48 h by default; 1-minute rollups ~0.25 MB/day, pruned
after 30 days. Event copies are kept 365 days.

## Design choices that keep overhead low

- One sampling thread; one `NtQuerySystemInformation` call per tick for all processes;
  executable paths cached per `(pid, creation time)`.
- PDH queries are opened once and collected per tick; rate counters use PDH's own
  interval math.
- Sensor providers poll on their own (slower) interval; drive health every 30 min.
- Event logs use push subscriptions (no polling).
- History is aggregated in memory and written in 5-second buckets through a bounded
  queue to a single writer thread (batched transactions, WAL mode).
- Telemetry events are emitted only while a window is visible or the overlay is open;
  the UI keeps a fixed 300-frame ring; charts redraw on animation frames.
- Tables are virtualized (only visible rows exist in the DOM).

## Methodology

`scripts/measure-overhead.ps1`:

1. Starts the release executable unelevated (`__COMPAT_LAYER=RunAsInvoker`) with
   `VEYL_DATA_DIR` pointing to a new temporary folder.
2. Waits for warm-up, then samples `TotalProcessorTime`, working set and private bytes
   of the application process and every descendant process (WebView2), plus
   `Win32_Process.WriteTransferCount`, at the start and end of each window.
3. Minimizes the window (it hides to the tray) and repeats.
4. Prints the results and the database size, then stops the application.

Elevation changes nothing measured here except which data is available.

## Not yet measured

- 24-hour and multi-day runs (memory growth / leak check). All in-memory collections
  are bounded (sample ring, bucket aggregator, per-key maps pruned to live keys, frame
  buffers capped), but a long-run measurement is still pending.
- Overhead during an active PresentMon frame capture.
- Lower-end hardware.
