# Implementation status

**Done** = implemented and verified (tests and/or on real hardware). **Partial** = works
with the stated limits. **Unsupported** = not implemented; reason given.
Verification machine: Windows 11 Home 26300, i5-13600KF, Radeon RX 9060 XT, 4 NVMe + 1 SATA drive.

## Foundation

| Item | State | Notes |
|---|---|---|
| Tauri 2 app, Rust workspace (core / platform / storage / app) | Done | 160 Rust tests, clippy `-D warnings` clean |
| React + TS (strict) + Vite + Tailwind 4, Radix primitives, Lucide, uPlot, TanStack Virtual | Done | 17 frontend tests |
| Manual QA pass of every page on the release build | Done (2026-11-07) | Unelevated; elevated-only sensors verified separately with `examples/probe.rs` |
| Settings (validated, atomic save), structured logging (daily rotation, 7 files) | Done | |
| Single instance, tray icon, close/minimize to tray, window state | Done | Minimize-to-tray verified |
| Elevated start | Done | Release builds embed `requireAdministrator`; debug builds `asInvoker` |
| Signed auto-update from GitHub Releases | Done (needs a published repo) | Endpoint `LunoviaVR/Veyl`; key in `%USERPROFILE%\.tauri`; release workflow included |
| NSIS installer, icon, version info, uninstaller | Done | Per-machine install (Program Files); the elevated app only runs helper programs from admin-only folders |

## Monitoring

| Item | State | Notes |
|---|---|---|
| CPU total, utility, per-core, base/effective/per-core clocks, counts, hybrid topology | Done | |
| CPU package / per-core temperatures | Done (Intel, needs PawnIO + admin) | PawnIO IntelMSR; verified on i5-13600KF |
| Motherboard fans, voltages, temperatures | Done (Nuvoton NCT6796–6799, needs PawnIO + admin) | PawnIO LpcIO; verified on NCT6798D |
| CPU package / cores power | Done | RAPL via Windows Energy Meter, unelevated |
| Memory: used/available/commit/cache/pools/hard faults | Done | |
| Disks: per physical disk read/write/active/queue; volume capacity | Done | |
| Network: per adapter rates, link speed; connections per process | Done | Per-process throughput unsupported (needs kernel ETW) |
| GPU: utilization per engine, VRAM, temperature, fan, power %, core & memory clock, rated max | Done | Vendor-neutral WDDM; what the driver reports varies (RX 9060 XT: no power) |
| GPU hotspot, board power, voltage (AMD) | Done | ADLX; verified on RX 9060 XT |
| GPU temperature, power, fan, clocks (NVIDIA) | Done (not verified on hardware) | NVML |
| Processes: full table, apps grouped first in the default order, tree, CPU/mem/commit/I/O/GPU/VRAM/threads/handles/status/start/path | Done | 700+ processes, virtualized |
| End task / tree, priority, properties, open location, copy PID | Done | PID-reuse check, critical-process refusal, confirmation dialogs |
| Services, startup apps (read-only) | Done | |
| ACPI thermal zones incl. passive limit / throttle reasons | Done | |
| NVMe SMART/health, drive temperature, identity | Done | Unelevated on the test machine |
| SATA SMART | Unsupported | Needs ATA pass-through |
| Intel Arc sensors (IGCL), AMD Ryzen temps, ITE/Fintek Super I/O | Not implemented | Possible with PawnIO modules (RyzenSMU/AMDFamily17, LpcIO) |

## Gaming

| Item | State | Notes |
|---|---|---|
| PresentMon CLI provider (v1 and v2 CSV columns) | Done (not exercised end-to-end: PresentMon is not installed on the test machine) | Parser tested on both formats |
| FPS, 1%/0.1% lows, P50/P95/P99, stutter, pacing, live stats | Done | Definitions in docs/METRICS.md |
| Candidate detection (GPU users + foreground), manual selection | Done | |
| Sessions: storage, context (CPU/GPU/temps/VRAM/RAM), comparison, CSV export, delete | Done | |
| Overlay (transparent, click-through, configurable) | Done | Not over exclusive fullscreen |
| PresentMon service API (PresentMonAPI2) | Unsupported | CLI chosen for ABI stability across versions |

## Diagnostics

| Item | State | Notes |
|---|---|---|
| Event Log back-fill + push subscriptions (Application, System) | Done | Provider **and** ID validated |
| Application crashes/hangs, WER link by report ID, module info, related events, telemetry context | Done | |
| Crash grouping by app/exception/module, recurrence | Done | |
| BSOD incidents (WER 1001 + Kernel-Power 41 + EventLog 6008), stop-code table, parameters | Done | Verified with a real 0x13A on the test machine |
| Dump discovery | Done | `LiveKernelReports` needs elevation |
| Dump analysis via cdb `!analyze -v` | Done (not exercised: Debugging Tools not installed) | Output parser tested |
| Diagnostic rules (13) with evidence | Done | |
| Stability timeline, daily chart, "around this time" view | Done | |
| Alerts: thresholds with duration/hysteresis/cooldown, event alerts, notifications, clear active list (history kept) | Done | |
| History: 5 s + 1 min tiers, retention, category cards, select-all, shaded no-data periods, custom ranges, CSV | Done | |
| PC Specs: SMBIOS board/BIOS/RAM modules, CPU caches, GPU driver, displays, drives, network; TXT and CSV export (no serials) | Done | Verified on this PC, TXT export clicked through |
| Network: live packet capture (Windows NDIS capture over ETW, needs admin), dissectors (Ethernet, ARP, IPv4/6, TCP, UDP, ICMP, DNS/mDNS/LLMNR, HTTP/1.x, TLS SNI, SMB2, QUIC, DHCP), display filters, hex view; memory only | Done | Verified on this PC: capture, filters, presets, field filter, hex highlight, stop/clear, external stop; 1.5M random frames fuzzed |
| Exports: history/session/events CSV, JSON report, redaction | Done | |

## Not yet done

- Overhead measured for idle monitoring (see PERFORMANCE.md); long-run (24 h+) leak test and capture-time overhead pending.
- UI integration tests for full workflows (only unit tests for UI logic so far).
- Windows 10 and multi-GPU verification on real hardware.
- Code signing of the installer (no certificate configured; updates are signed with the updater key).
