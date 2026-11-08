# Veyl

A Windows desktop application for monitoring, troubleshooting and understanding PC
health: live system and process monitoring, hardware sensors, frame-time analysis for
games, application crash and blue-screen investigation, a stability timeline,
historical charts and alerts — in one window, using real Windows data only.

> Working name; the product name is set in `src-tauri/tauri.conf.json` (`productName`).

## What it does

| Area | What you get | Source |
|---|---|---|
| Overview | CPU, GPU, memory, storage, network and stability at a glance | Live telemetry + event history |
| Processes | Virtualized process table/tree (CPU, memory, I/O, GPU, VRAM, threads, handles), end task / tree with safeguards, priority, properties, file location; services, startup apps, network connections | `NtQuerySystemInformation`, PDH, SCM, registry, IP Helper |
| Performance | Live charts per device, per-core utilization and clocks | PDH counters |
| Hardware | Sensor table with min/max/avg, provider status, NVMe SMART/health, drive temperatures, hardware identification | In-house providers: ACPI thermal zones, Windows Energy Meter (CPU RAPL power), AMD ADLX and NVIDIA NVML GPU sensors, WDDM perf data, storage IOCTLs |
| Gaming | FPS, 1%/0.1% lows, P95/P99 frame times, stutter and pacing, live context, session history, comparison, CSV export, optional overlay | Intel PresentMon (user-installed) |
| Stability | Diagnostic findings with evidence, daily event chart, unified timeline with "what happened around this time" | Windows Event Log + recorded telemetry |
| Crash Reports | Application crashes/hangs with exception, faulting module, WER report link, related events, recorded system state | Application Error 1000, Application Hang 1002, WER 1001 |
| Blue Screen Analyzer | Bugcheck vs. unexpected-shutdown incidents, stop-code reference, dump discovery, `!analyze -v` | WER-SystemErrorReporting 1001, Kernel-Power 41, EventLog 6008, Debugging Tools |
| History | 5-second and 1-minute telemetry, 15 min – 30 days or custom range, CSV export | Local SQLite |
| Alerts | Threshold rules with duration, hysteresis, cooldown; BSOD / restart / repeated-crash alerts; Windows notifications | Telemetry + events |

Nothing is ever estimated or simulated: a value Windows (or an optional provider) does
not report is shown as "—" with an explanation, never as 0.

## System requirements

- Windows 11 x64 (primary) or Windows 10 x64 21H2+
- WebView2 runtime (included with Windows 11; installed automatically on Windows 10 if missing)
- About 80 MB RAM; sampling overhead is shown on the Overview page and in Settings

## Install

Run `Veyl_x.y.z_x64-setup.exe` from the release page. It installs for all users into
Program Files (Windows asks for administrator permission). The application itself
starts as administrator (Windows shows a UAC prompt), which enables frame capture,
kernel dump analysis and full process details. Because it runs as administrator, it
only lives in, and only starts helper programs (PresentMon, cdb) from, folders that
ordinary programs cannot change: Program Files and the Windows folder. Uninstalling removes the program; your data folder
(`%LOCALAPPDATA%\app.veyl.monitor`) is kept unless you delete it. Windows crash
history and crash-dump settings are never modified.

### Background running

Closing or minimizing the window sends it to the notification area (both configurable
in Settings → General); monitoring, history and alerts keep running. Use the tray
icon's **Exit** to quit.

### Updates

Installed builds check GitHub Releases for a newer version shortly after start and
every 12 hours (Settings → Updates; can be turned off). Updates are signed and verified
before installation, and are only installed when you click **Install and restart**.

## Optional integrations

| Integration | Adds | How |
|---|---|---|
| [Intel PresentMon](https://github.com/GameTechDev/PresentMon) (MIT) | FPS and frame-time capture | Download the console app (`PresentMon-x.y.z-x64.exe`), place it in the tools folder inside Veyl's Program Files folder (Settings → Integrations → Tools folder) or select a copy inside Program Files in Settings |
| [PawnIO](https://pawnio.eu) (open source, signed) | CPU package/per-core temperatures (Intel), motherboard fans, voltages and temperatures (Nuvoton) | Install PawnIO; runs with the installed (elevated) build. The app ships only PawnIO's official signed LGPL modules, never the driver |
| Debugging Tools for Windows | Blue-screen dump analysis (`!analyze -v`) | Install "Debugging Tools for Windows" from the Windows SDK installer, or select `cdb.exe` in Settings |
| Microsoft symbol server | Better stack traces in dump analysis | Off by default; enable in Settings → Integrations. Only symbol files are downloaded; dumps never leave the PC |

None of these are bundled. When one is missing, the related page says so and the rest
of the application works normally.

## Permissions

Release builds request administrator rights at start. Features that rely on it:

- FPS capture (PresentMon starts an ETW session; alternatively membership in *Performance Log Users*)
- Listing/reading `%SystemRoot%\Minidump`, `MEMORY.DMP` and `LiveKernelReports`
- Details (path, command line) and priority changes of other users' and service processes

Everything else — CPU/GPU/memory/disk/network telemetry, processes, sensors exposed by
Windows, NVMe health (on most systems), event logs — works as a standard user (debug
builds run unelevated). Critical Windows processes are always protected from being ended.

## Privacy

All data stays in the local data folder. The only network requests are the update check
(GitHub, can be disabled) and optional symbol downloads. Exports are written only to
files you pick, with optional redaction of user name, profile path, computer name, SIDs
and drive serial numbers.

## Development

See [DEVELOPMENT.md](DEVELOPMENT.md) for the toolchain and extension guides.

```powershell
npm install
npm run tauri dev                                     # run in development (unelevated)
npm run tauri build                                   # release exe + NSIS installer in target/release/bundle/nsis
powershell -ExecutionPolicy Bypass -File scripts/check.ps1   # formatting, lints, all tests, build
```

### Publishing a release (auto-update)

1. Create the GitHub repository (the updater endpoint is
   `https://github.com/LunoviaVR/Veyl/releases/latest/download/latest.json`; change
   `plugins.updater.endpoints` in `src-tauri/tauri.conf.json` if the repository name differs).
2. Add repository secrets `TAURI_SIGNING_PRIVATE_KEY` (the contents of
   `%USERPROFILE%\.tauri\veyl.key`) and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`
   (empty). Back the key up: without it, installed copies cannot be updated.
3. Bump the version in `src-tauri/tauri.conf.json`, `package.json` and `Cargo.toml`,
   commit, and push a tag `vX.Y.Z`. The *Release* workflow builds, signs and drafts the
   release; publish the draft to deliver the update.

## Troubleshooting

| Symptom | Cause / fix |
|---|---|
| "Package temp —" for the CPU | Windows exposes CPU die temperatures only to kernel drivers. Install PawnIO and run the installed (elevated) build. |
| GPU hotspot "not reported" | Shown for AMD (ADLX) and NVIDIA (NVML) GPUs when the vendor driver reports it; WDDM alone reports edge temperature only. |
| GPU power "not reported" | Your driver does not report power through WDDM (common on AMD). |
| Gaming: "PresentMon not found" | Install PresentMon and select it in Settings. |
| Gaming: permission error | Run the installed (elevated) build, or add your account to *Performance Log Users*. |
| No dump files listed | Windows keeps minidumps only when *Startup and Recovery → Write debugging information* is enabled; old dumps are deleted by Windows. |
| History empty for a period | The application was not running then; gaps are shown, never filled in. |
| Logs | Settings → About → Logs (`%LOCALAPPDATA%\app.veyl.monitor\logs`). Set `VEYL_LOG=debug` for more detail. |

## Documentation

- [ARCHITECTURE.md](ARCHITECTURE.md) — modules, threads, data flow, security boundaries
- [DEVELOPMENT.md](DEVELOPMENT.md) — adding sensor providers, diagnostic rules, pages
- [COMPATIBILITY.md](COMPATIBILITY.md) — hardware, Windows versions, APIs, limitations
- [PERFORMANCE.md](PERFORMANCE.md) — measured overhead and methodology
- [IMPLEMENTATION_STATUS.md](IMPLEMENTATION_STATUS.md) — what is done, partial, unsupported
- [docs/METRICS.md](docs/METRICS.md) — exact metric definitions
