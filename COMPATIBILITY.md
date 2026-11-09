# Compatibility

## Windows

| Version | Status |
|---|---|
| Windows 11 x64 (22H2+) | Primary target. Verified on Windows 11 Home build 26300 (26H2). |
| Windows 10 x64 21H2/22H2 | Supported APIs are the same; not yet verified on hardware. GPU temperature needs a WDDM 2.4+ driver. |
| ARM64, 32-bit, Windows 8.1 and older | Not supported (CPUID brand string and x64 structure layouts are assumed). |

## Verified hardware

| Component | Result |
|---|---|
| Intel Core i5-13600KF (6P + 8E, 20 threads) | Utilization, utility, per-core %, per-core and effective clocks, hybrid core detection, package and core power (RAPL via Windows Energy Meter). No die temperature (needs a kernel driver). |
| AMD Radeon RX 9060 XT 16 GB, Adrenalin driver 32.0.32015 | WDDM: utilization per engine, VRAM, temperature, fan, clocks, rated max (110 °C). ADLX: hotspot temperature, total board power, ASIC power, core voltage, fan RPM, clocks. |
| 4 × NVMe (Samsung 990 EVO Plus, WD SN580, Teamgroup) | Model, firmware, temperature, full SMART/Health log **without elevation**. |
| Seagate SATA HDD | Model, firmware, capacity. SMART not available (see limitations). |

## Data sources and what they need

| Data | API | Elevation |
|---|---|---|
| CPU utilization, clocks | PDH `Processor Information` (English counter names) | No |
| Memory | `GlobalMemoryStatusEx`, `GetPerformanceInfo`, PDH `Memory` | No |
| Processes | `NtQuerySystemInformation(SystemProcessInformation)`, `QueryFullProcessImageNameW`, `NtQueryInformationProcess(ProcessCommandLineInformation)` | No (protected processes: path/command line hidden) |
| Disk activity / capacity | PDH `PhysicalDisk`, `GetDiskFreeSpaceExW` | No |
| Network | `GetIfTable2`, `GetExtendedTcpTable/UdpTable` | No |
| GPU utilization / VRAM | PDH `GPU Engine`, `GPU Adapter Memory`, `GPU Process Memory` | No |
| GPU temperature / fan / power / clocks | `D3DKMTQueryAdapterInfo` (`ADAPTERPERFDATA`, `NODEPERFDATA`) | No; WDDM 2.4+ driver; driver decides what it reports |
| ACPI thermal zones | PDH `Thermal Zone Information` | No |
| Drive identity, temperature, NVMe health | `IOCTL_STORAGE_QUERY_PROPERTY` | No (NVMe health on most drivers) |
| Event logs | `EvtQuery`, `EvtSubscribe`, `EvtFormatMessage` (Application, System) | No |
| Kernel dumps | Files in `%SystemRoot%\Minidump`, `MEMORY.DMP`, `LiveKernelReports` | Usually yes |
| Frame times | PresentMon (ETW) | Admin or Performance Log Users |

## Graphics APIs for frame capture

Through PresentMon: DirectX 9, 10, 11, 12 (DXGI), and Vulkan/OpenGL via the graphics
kernel's present events. Compatibility depends on PresentMon and the game. Anti-cheat
systems are not affected: nothing is injected; PresentMon only reads ETW events.

## Overlay limitations

The overlay is a transparent, click-through, always-on-top window composed by DWM.

- Shown over windowed and borderless-fullscreen games.
- **Not** shown over exclusive-fullscreen games (they bypass DWM composition).
- Not shown on the secure desktop (UAC) or over some elevated/full-screen video apps.
- It never hooks or injects into the game process.

## Known limitations

| Limitation | Reason | Workaround |
|---|---|---|
| No CPU package/core temperatures without PawnIO | Windows exposes them only to kernel drivers (MSR access) | Install PawnIO (below); without it CPU package power (RAPL) and ACPI thermal zones are shown |
| GPU hotspot / power on GPUs without a vendor library | WDDM does not include them | AMD via ADLX, NVIDIA via NVML (Intel Arc: not implemented) |
| No motherboard voltages and fans | Super I/O chips need a kernel driver | GPU fans and drive temperatures are shown |
| SATA SMART attributes | Need ATA pass-through, i.e. elevation plus a device handle with read/write access; not implemented | NVMe drives are fully supported |
| Per-process network throughput | Requires a kernel ETW network session; not implemented | Connections tab shows endpoints per process |
| Per-process "disk" column is all I/O | Kernel counters include file, network and device I/O | Labelled "I/O" |
| Process CPU % is time-based | Matches Task Manager's Details tab, not the frequency-scaled Processes tab | Overview shows CPU utility separately |
| Application crash dumps | Only exist if Windows Error Reporting LocalDumps is configured; this app never changes it | — |
| UWP games | Present events are attributed to the game process; foreground detection can report ApplicationFrameHost | Pick the game in the candidate list |

## Driver-free CPU temperature: what was tried (2026-11-07)

| Source | Result on ASUS ROG STRIX Z790-E (BIOS 3107) |
|---|---|
| ACPI thermal zones (PDH / `MSAcpi_ThermalZoneTemperature`) | One zone (`\_TZ.TZ00`, ~28 °C): a board sensor, not the CPU die. Shown as such. |
| SMBIOS probes (`Win32_TemperatureProbe`, `Win32_VoltageProbe`, `Win32_Fan`) | No live readings (static descriptors only). |
| ASUS firmware ACPI-WMI (`ASUSManagement.asio_hw_fun15` MSR read, `asio_hw_fun21` hardware-monitor read) | Methods exist but return 0 for every MSR and register: ASUS gates them behind Armoury Crate's authentication. Not bypassed. |
| Intel Innovation Platform Framework | Service present but stopped; desktop boards have no Dynamic Tuning participants. |

Conclusion: without a driver these values are unreachable. They are now read through **PawnIO** (below), an open-source, Microsoft-signed driver that Veyl installs when the user clicks Install PawnIO.
## PawnIO sensors (optional, open-source signed driver)

Veyl's installer puts the official signed PawnIO 2.2.0 setup in its `tools` folder; **Install PawnIO** on the Hardware page runs it (`-install -silent`) and reopens the PawnIO providers. PawnIO from https://pawnio.eu, FanControl or LibreHardwareMonitor works the same. Requires administrator rights (release builds run elevated). The app embeds the official signed LGPL modules `IntelMSR` and `LpcIO`.

| Provider | Reads | Verified (i5-13600KF, ROG STRIX Z790-E) |
|---|---|---|
| CPU temperatures (`IntelMSR`) | IA32_PACKAGE_THERM_STATUS, per-core IA32_THERM_STATUS (thread pinned per core), TjMax | Package 56 °C, 6 P-cores, 8 E-cores, TjMax 100 °C |
| Motherboard (`LpcIO`, Nuvoton NCT6796–NCT6799) | 7 fan headers, 16 voltage inputs, SYSTIN/CPUTIN/AUXTIN0–3 | NCT6798D 0xD42B: Vcore 1.104 V, 3.3 V 3.328 V, SYSTIN 38 °C, CPUTIN 44 °C, 2 fans |

Limits: Intel CPUs only (AMD Ryzen module not integrated yet); Nuvoton chips only (ITE/Fintek not implemented); voltage inputs other than Vcore/3.3V/AVCC/3VSB/VBAT are shown as raw chip-pin voltages because board-specific resistor dividers are not applied; fan headers are numbered by chip input, not board label. Super I/O access holds the shared `Access_ISABUS.HTP.Method` mutex used by other monitoring tools.

## In-house sensor providers

| Provider | Source | Needs |
|---|---|---|
| ACPI thermal zones | PDH `Thermal Zone Information` | Nothing; firmware-defined zones (often not the CPU) |
| CPU power | PDH `Energy Meter` (RAPL domains PKG/PP0/PP1/DRAM) | Windows 10+ with EMI support; verified on Intel 13th gen |
| AMD GPU | ADLX (`amdadlx64.dll`, loaded from System32) | AMD Adrenalin driver with ADLX 2.0 |
| NVIDIA GPU | NVML (`nvml.dll`, loaded from System32) | NVIDIA driver; **not verified on NVIDIA hardware** |
| Drives | `IOCTL_STORAGE_QUERY_PROPERTY` | Nothing for NVMe on most drivers |

Vendor libraries are loaded only from `System32` (no search-path DLL loading). No third-party monitoring tool and no kernel driver is used or installed.
