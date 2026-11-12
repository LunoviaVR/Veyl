# Changelog

All notable changes to Veyl. Versions follow [Semantic Versioning](https://semver.org/).

## Unreleased

### Added

- **Recovery** page (Tools in Cassette Futurism, Diagnose in the standard themes): restart straight
  into Safe Mode, Safe Mode with Networking or the Windows recovery options. Safe Mode lasts one
  boot; a restart that was cancelled can be undone from the page. Safe Mode needs administrator rights.

## 1.0.6 (2026-11-11)

### Added

- **Cassette Futurism** theme (Settings › Theme). It replaces the whole interface with a retro
  console and leaves the standard themes as they were:
  - data on CRT screens in green or amber phosphor, keycap buttons, toggle switches, larger text;
    two switches in the sidebar choose the phosphor and a dark or putty-coloured chassis;
  - an Overview with a lamp panel (a tile stays dark while its subsystem is normal and lights
    amber or red for an active alert, a reached limit or a recorded incident), messages that link
    to the evidence, needle dials for CPU, GPU and memory, and a tape recorder for history;
  - a sidebar grouped into Monitor, Diagnose and Tools, and **Ctrl+K** to jump to any page or tab
    by name or by what it does, such as "temperature" or "fps";
  - apps and background processes on separate screens, each with its own filter and sort order.
- **Cassette Futurism** overlay design, available with any theme: a four-line phosphor terminal
  readout with LED bars. Dark and Smoke are green, White is amber in a putty case, Float has no
  case.

## 1.0.5 (2026-11-09)

### Added

- The signed PawnIO 2.2.0 setup now comes with Veyl. **Install PawnIO** on the Hardware page
  installs the driver for CPU temperatures and motherboard fans, voltages and temperatures, and
  the sensors start without restarting Veyl. Nothing is installed until you click.

## 1.0.4 (2026-11-09)

### Fixed

- Updating or uninstalling no longer fails with "Error opening file for writing" when a
  PresentMon capture was still running. The installer stops PresentMon first.
- **Check now** explains that GitHub can take a few minutes to serve a release that was just
  published, instead of saying no release information was found.
- A failed update install is now written to the log.

## 1.0.3 (2026-11-09)

### Changed

- Intel PresentMon 2.6.0 now comes with Veyl, so FPS capture works right after installing.
  The **Install PresentMon** button and its download are gone.

### Fixed

- When Veyl is installed outside Program Files, the Gaming page now explains why it will not run
  PresentMon instead of saying PresentMon is missing.

## 1.0.2 (2026-11-08)

### Added

- **Install Debugging Tools** button on the Blue Screen Analyzer. It runs Microsoft's winget
  to add only the debugger feature of the Windows SDK, so dumps can be analyzed without a
  manual SDK install.

## 1.0.1 (2026-11-08)

### Fixed

- Live data (Overview, Performance, Gaming) froze after reopening Veyl from the notification
  area. It now resumes as soon as the window opens.

## 1.0.0 (2026-11-08)

The first release of Veyl, a Windows 11 and Windows 10 PC health monitor, hardware monitor and
crash analyzer. Every value comes from real Windows data; there is no telemetry.

### Monitoring

- **Overview** of CPU, GPU, memory, storage, network and stability at a glance.
- **Processes:** apps with a window first, like Task Manager, plus a process tree, services,
  startup apps and active connections. End a task or a process tree and change priority, with
  safeguards for critical Windows processes and reused process IDs.
- **Performance:** live CPU, GPU, memory, disk and network charts with per-core load and clocks.
- **Hardware sensors:** CPU package and core temperatures and power (Intel, with PawnIO), GPU
  temperature, hotspot, fans, clocks, power and VRAM (AMD ADLX, NVIDIA NVML, Windows), Nuvoton
  Super I/O fans, voltages and temperatures (with PawnIO), and NVMe health.
- **History:** 5-second detail for 48 hours and 1-minute detail for 30 days, charted from
  15 minutes to 30 days and exported to CSV. Periods when Veyl wasn't running are shaded.
- **PC specs:** CPU, motherboard, BIOS, every RAM module, GPU and driver, monitors, drives and
  network adapters, exported as TXT or CSV without serial numbers.

### Diagnostics

- **Stability:** plain-language findings with their evidence, a daily chart of crashes,
  shutdowns and hardware errors (WHEA), and a searchable reliability timeline.
- **Crash reports:** every app crash and hang with exception code, faulting module, the linked
  Windows Error Reporting report and the system state recorded just before.
- **Blue screen analyzer:** bugchecks separated from unexpected shutdowns, stop codes explained,
  minidumps and live kernel reports listed, and one-click `!analyze -v` with Debugging Tools for
  Windows (optional Microsoft symbol server).
- **Network:** Wireshark-style packet capture with Windows' built-in capture (no Npcap), protocol
  decoding from Ethernet to DNS, HTTP, TLS, SMB2, QUIC and DHCP, display filters and a hex view.
  Packets stay in memory only.
- **Alerts:** thresholds for temperatures, memory, disk space, CPU load and throttling, plus new
  blue screens, unexpected restarts and repeated crashes, shown as Windows notifications.

### Gaming

- **FPS counter and frame-time analysis:** average FPS, 1 % and 0.1 % lows, P95 and P99 frame
  times, stutter and pacing, with CPU and GPU load and temperatures. Save, compare and export
  sessions.
- **Automatic capture** starts when a game opens and stops when it closes. Games are detected from
  Windows' Game Bar list and from Steam, Epic, GOG, EA, Ubisoft and Xbox libraries; apps Steam
  lists as software (Blender, OBS Studio, Wallpaper Engine...) are left out.
- **In-game overlay** on the game's own window, only while the game is in front, at any of nine
  positions. 17 designs in Dark, Smoke, Float and White from Apple-Style RTSS Overlays by realHfde
  (MIT), with a live preview on the Gaming page. No RivaTuner or Afterburner needed.
- **One-click PresentMon install:** Veyl downloads Intel's official release and keeps it only if
  it matches the published checksum. Nothing is injected into games.

### App

- Installs per machine into Program Files and runs as administrator; it starts helper programs
  only from folders ordinary programs can't change.
- Lives in the notification area; signed automatic updates install only when you click
  **Install and restart**.
- Light: about 0.1 % of total CPU while monitoring.

Requirements, optional add-ons and known limitations: see the [README](README.md) and
[COMPATIBILITY.md](COMPATIBILITY.md).
