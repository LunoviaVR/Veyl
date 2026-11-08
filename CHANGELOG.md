# Changelog

All notable changes to Veyl. Versions follow [Semantic Versioning](https://semver.org/).

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
