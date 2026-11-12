// Sample data standing in for the Tauri backend, so the UI runs in a plain browser.
// Loaded only by .preview/vite.config.ts; the app itself never imports it.
// Scenario: ?s=issues (default: hot GPU, alert, findings) | ok (healthy PC) | empty (first start, no data).
// Add &cap=1 to show a running frame capture, &theme=dark|light|system for the standard look (default: cassette).
import type * as T from "../src/lib/types";

const query = new URLSearchParams(location.search);
const scenario = query.get("s") ?? "issues";
const issues = scenario === "issues";
const empty = scenario === "empty";
const GB = 1024 ** 3;
const MB = 1024 ** 2;
const MIN = 60_000;
const DAY = 86_400_000;
const START = Date.now();
const BOOT = START - (6 * 24 + 11) * 3_600_000 - 57 * MIN;
const available: T.Status = { state: "available", message: null };
const clamp = (v: number, lo = 0, hi = 100) => Math.min(hi, Math.max(lo, v));
const rnd = (amp: number) => (Math.random() - 0.5) * amp;
const range = <R>(n: number, f: (i: number) => R) => Array.from({ length: n }, (_, i) => f(i));

// ---- live telemetry -------------------------------------------------------

let seq = 0;
function frame(at = Date.now()): T.TelemetryFrame {
  const n = ++seq;
  const cpu = clamp((issues ? 46 : 14) + 16 * Math.sin(n / 9) + rnd(12));
  const gpu = clamp((issues ? 93 : 5) + 4 * Math.sin(n / 6) + rnd(4));
  const memTotal = 63.8 * GB;
  const memUsed = (issues ? 58.6 : 34.1) * GB + 0.3 * GB * Math.sin(n / 30);
  const rx = Math.max(0, (n % 23 < 3 ? 6_000_000 : 5_000) + rnd(4_000));
  const tx = Math.max(0, 1_900 + rnd(1_500));
  const volume = (mount: string, label: string, totalGb: number, freeGb: number): T.VolumeSnapshot => ({ mount, label, fileSystem: "NTFS", driveType: "fixed", totalBytes: totalGb * GB, freeBytes: freeGb * GB });
  const disk = (name: string, index: number, busy: number): T.PhysicalDiskActivity => ({ name, index, readBps: busy * 180_000 * Math.random(), writeBps: busy * 502_000 * Math.random(), activePct: clamp(busy * 7 + rnd(busy * 4)), queueLength: busy * 0.02 });
  return {
    seq: n,
    timestampMs: at,
    cpu: {
      status: available,
      totalPct: cpu,
      utilityPct: clamp(cpu * 1.38, 0, 140),
      perCorePct: range(20, (i) => (i === 14 ? null : clamp(cpu + 26 * Math.sin(n / 5 + i * 1.7) + rnd(10) - (i >= 12 ? 18 : 0)))),
      baseMhz: 3500,
      effectiveMhz: 4760 + 240 * Math.sin(n / 7),
      perCoreMhz: range(20, (i) => (i < 12 ? 4950 + 150 * Math.sin(n / 4 + i) : 2050 + 140 * Math.sin(n / 6 + i))),
      packageTempC: (issues ? 81 : 54) + 4 * Math.sin(n / 14),
      packageTempSource: "pawnio-cpu · CPU (digital thermal sensor)",
      packagePowerW: 38 + cpu * 1.1,
      processCount: 318,
      threadCount: 5121,
      handleCount: 152_340,
      uptimeSecs: (at - BOOT) / 1000,
    },
    memory: {
      status: available,
      totalBytes: memTotal,
      availableBytes: memTotal - memUsed,
      usedBytes: memUsed,
      loadPct: (memUsed / memTotal) * 100,
      commitBytes: memUsed * 1.42,
      commitLimitBytes: 100 * GB,
      cachedBytes: 11.2 * GB,
      pagedPoolBytes: 1.1 * GB,
      nonpagedPoolBytes: 0.9 * GB,
      pagesInputPerSec: issues ? Math.round(40 + rnd(60)) : 0,
    },
    disks: {
      status: available,
      volumes: [volume("C:\\", "Windows", 931, 329), volume("D:\\", "Games", 1863, 1638), volume("E:\\", "Media", 3726, 2048), volume("S:\\", "Scratch", 932, 926), volume("V:\\", "Recordings", 1863, issues ? 96 : 1640)],
      physical: [disk("0 S:", 0, 0), disk("1 V:", 1, 0), disk("2 C:", 2, 1), disk("3 E:", 3, 0), disk("4 D:", 4, issues ? 3 : 0)],
      totalReadBps: issues ? 2_400_000 + rnd(800_000) : 0,
      totalWriteBps: 502_000 + rnd(200_000),
    },
    network: {
      status: available,
      adapters: [
        { luid: 1, ifIndex: 12, alias: "Ethernet", description: "Intel(R) Ethernet Controller I226-V", media: "Ethernet", connected: true, linkSpeedBps: 125_000_000, rxBps: rx, txBps: tx, rxTotalBytes: 81 * GB, txTotalBytes: 9 * GB },
        { luid: 2, ifIndex: 7, alias: "Ethernet 2", description: "Realtek Gaming 2.5GbE Family Controller", media: "Ethernet", connected: true, linkSpeedBps: 312_500_000, rxBps: 0, txBps: 0, rxTotalBytes: 2 * GB, txTotalBytes: GB },
        { luid: 3, ifIndex: 18, alias: "Wi-Fi", description: "MediaTek Wi-Fi 7 MT7925", media: "Wi-Fi", connected: false, linkSpeedBps: null, rxBps: null, txBps: null, rxTotalBytes: 0, txTotalBytes: 0 },
      ],
      totalRxBps: rx,
      totalTxBps: tx,
    },
    gpus: {
      status: available,
      adapters: [
        {
          luid: "0x0000F1A2",
          name: "AMD Radeon RX 9060 XT",
          vendorId: 0x1002,
          utilPct: gpu,
          engines: [
            { engineType: "3D", utilPct: gpu },
            { engineType: "Copy", utilPct: 2 },
            { engineType: "VideoDecode", utilPct: 0 },
          ],
          dedicatedTotalBytes: 15.8 * GB,
          dedicatedUsedBytes: (issues ? 13.9 : 5.3) * GB,
          sharedUsedBytes: 0.6 * GB,
          temperatureC: (issues ? 78 : 42) + 2 * Math.sin(n / 10),
          temperatureMaxC: 110,
          hotspotC: (issues ? 106 : 46) + 2 * Math.sin(n / 10),
          fanRpm: Math.round((issues ? 2180 : 1198) + rnd(30)),
          fanPct: issues ? 71 : 33,
          powerPct: null,
          powerW: (issues ? 152 : 32) + rnd(6),
          coreClockMhz: issues ? 2890 : 1810,
          memoryClockMhz: 2500,
          countersStatus: available,
          sensorsStatus: available,
        },
      ],
    },
    selfUsage: { cpuPct: 0.1, workingSetBytes: 80.3 * MB, privateBytes: 61 * MB, tickCostMs: 14.9 },
  };
}

// ---- static fixtures ------------------------------------------------------

const systemInfo: T.SystemInfo = {
  computerName: "GAMINGPC",
  osName: "Windows 11 Home 26H2",
  osBuild: "26300.9711",
  bootTimeMs: BOOT,
  cpu: { brand: "13th Gen Intel(R) Core(TM) i5-13600KF", vendor: "GenuineIntel", physicalCores: 14, logicalProcessors: 20, packages: 1, performanceCores: 6, efficiencyCores: 8, baseMhz: 3500 },
  installedMemoryBytes: 64 * GB,
  gpus: [{ luid: "0x0000F1A2", name: "AMD Radeon RX 9060 XT", vendorId: 0x1002, deviceId: 0x7590, dedicatedMemoryBytes: 15.8 * GB, sharedMemoryBytes: 31.9 * GB, isSoftware: false }],
  elevated: true,
  status: [],
};

const PROCESS_NAMES: [string, number, number, boolean][] = [
  // name, private working set (MB), CPU %, is an app
  ["Cyberpunk2077.exe", 9200, 18.4, true],
  ["chrome.exe", 1840, 3.1, true],
  ["Discord.exe", 612, 0.8, true],
  ["steam.exe", 318, 0.4, true],
  ["Code.exe", 944, 1.2, true],
  ["explorer.exe", 212, 0.3, true],
  ["veyl.exe", 31.5, 0.1, true],
  ["obs64.exe", 702, 4.6, true],
  ["Spotify.exe", 286, 0.2, true],
  ["dwm.exe", 164, 1.9, false],
  ["MsMpEng.exe", 402, 0.6, false],
  ["System", 0.2, 0.4, false],
  ["Registry", 96, 0, false],
  ["csrss.exe", 6.1, 0.1, false],
  ["lsass.exe", 18, 0, false],
  ["services.exe", 9.4, 0, false],
  ["audiodg.exe", 22, 0.3, false],
  ["SearchIndexer.exe", 58, 0, false],
  ["RuntimeBroker.exe", 14, 0, false],
  ["WmiPrvSE.exe", 25.1, 0.9, false],
  ["steamwebhelper.exe", 410, 0.5, false],
  ["XboxPcAppFT.exe", 1.7, 0, false],
  ["WidgetService.exe", 2.7, 0, false],
  ["wslservice.exe", 0.8, 0, false],
  ["python.exe", 148, 0.2, false],
  ["node.exe", 233, 0.7, false],
];

function processes(): T.ProcessTable {
  const named = PROCESS_NAMES.map(([name, mem, cpu, app], i): T.ProcessRow & { app: boolean } => ({
    app,
    pid: name === "System" ? 4 : 2000 + i * 412,
    parentPid: app ? 2000 + 5 * 412 : 844,
    startTimeMs: BOOT + 40_000 + i * 90_000,
    name,
    path: name === "System" || name === "Registry" ? null : `C:\\Program Files\\${name.replace(".exe", "")}\\${name}`,
    cpuPct: Math.max(0, cpu + rnd(cpu * 0.5)),
    workingSetBytes: mem * 1.4 * MB,
    privateWorkingSetBytes: mem * MB,
    privateBytes: mem * 1.7 * MB,
    ioReadBps: i % 3 === 0 ? Math.round(Math.random() * 400_000) : 0,
    ioWriteBps: i % 4 === 0 ? Math.round(Math.random() * 90_000) : 0,
    gpuPct: name.startsWith("Cyberpunk") ? (issues ? 92 : 4) : name === "dwm.exe" ? 3 : 0,
    gpuDedicatedBytes: name.startsWith("Cyberpunk") ? 11.2 * GB : name === "chrome.exe" ? 0.4 * GB : null,
    threads: 4 + ((i * 7) % 60),
    handles: 180 + ((i * 131) % 2400),
    sessionId: app ? 1 : 0,
    suspended: name === "XboxPcAppFT.exe",
  }));
  const hosts = range(46, (i): T.ProcessRow & { app: boolean } => ({
    app: false,
    pid: 14_000 + i * 216,
    parentPid: 844,
    startTimeMs: BOOT + 12_000 + i * 700,
    name: i % 5 === 0 ? "uvx.exe" : "svchost.exe",
    path: i % 5 === 0 ? "C:\\Users\\Sample\\.local\\bin\\uvx.exe" : "C:\\Windows\\System32\\svchost.exe",
    cpuPct: i % 9 === 0 ? 0.1 : 0,
    workingSetBytes: (3 + (i % 11)) * 1.5 * MB,
    privateWorkingSetBytes: (3 + (i % 11)) * MB,
    privateBytes: (4 + (i % 13)) * MB,
    ioReadBps: 0,
    ioWriteBps: 0,
    gpuPct: 0,
    gpuDedicatedBytes: null,
    threads: 1 + (i % 9),
    handles: 90 + ((i * 37) % 400),
    sessionId: 0,
    suspended: false,
  }));
  const rows = [...named, ...hosts];
  return { status: available, timestampMs: Date.now(), logicalProcessors: 20, rows: rows.map(({ app: _app, ...r }) => r), appPids: rows.filter((r) => r.app).map((r) => r.pid) };
}

const services: T.ServiceInfo[] = [
  ["AudioSrv", "Windows Audio", "running", "automatic"],
  ["BITS", "Background Intelligent Transfer Service", "stopped", "manual"],
  ["Dhcp", "DHCP Client", "running", "automatic"],
  ["Dnscache", "DNS Client", "running", "automatic"],
  ["EventLog", "Windows Event Log", "running", "automatic"],
  ["PawnIO", "PawnIO", "running", "manual"],
  ["Spooler", "Print Spooler", "stopped", "disabled"],
  ["Steam Client Service", "Steam Client Service", "running", "manual"],
  ["WinDefend", "Microsoft Defender Antivirus Service", "running", "automatic"],
  ["wuauserv", "Windows Update", "stopped", "manual"],
].map(([name, displayName, state, startType], i) => ({ name: name!, displayName: displayName!, state: state!, pid: state === "running" ? 1200 + i * 88 : null, startType: startType!, binaryPath: `C:\\Windows\\System32\\svchost.exe -k ${name}`, account: i % 3 === 0 ? "NT AUTHORITY\\LocalService" : "LocalSystem" }));

const startupItems: T.StartupItem[] = [
  ["Discord", '"C:\\Users\\Sample\\AppData\\Local\\Discord\\Update.exe" --processStart Discord.exe', true],
  ["Steam", '"C:\\Program Files (x86)\\Steam\\steam.exe" -silent', true],
  ["SecurityHealth", "%windir%\\system32\\SecurityHealthSystray.exe", true],
  ["Spotify", '"C:\\Users\\Sample\\AppData\\Roaming\\Spotify\\Spotify.exe" --autostart', false],
  ["Veyl", '"C:\\Program Files\\Veyl\\veyl.exe" --minimized', true],
].map(([name, command, enabled]) => ({ name: name as string, command: command as string, executable: null, location: "HKCU\\...\\Run", scope: "user", enabled: enabled as boolean }));

const connections: T.ConnectionRow[] = range(14, (i) => ({
  protocol: i % 4 === 3 ? "UDP" : "TCP",
  localAddress: "192.168.1.24",
  localPort: 49_600 + i * 37,
  remoteAddress: i % 4 === 3 ? null : `142.250.${70 + i}.${10 + i * 3}`,
  remotePort: i % 4 === 3 ? null : 443,
  state: i % 4 === 3 ? null : i % 5 === 0 ? "TimeWait" : "Established",
  pid: 2000 + (i % 9) * 412,
  processName: PROCESS_NAMES[i % 9]![0],
}));

function sensors(): T.SensorsView {
  const row = (provider: string, device: string, category: T.SensorCategory, kind: T.SensorKind, label: string, unit: string, value: number | null, spread = 0.12): T.SensorRow => ({
    key: `${provider}/${device}/${label}`,
    provider,
    device,
    category,
    kind,
    label,
    unit,
    value: value == null ? null : value + rnd(value * 0.01),
    timestampMs: Date.now(),
    stats: value == null ? null : { min: value * (1 - spread), max: value * (1 + spread), avg: value * (1 - spread / 3), samples: 1840 },
  });
  const cpuT = issues ? 81 : 54;
  const gpuT = issues ? 78 : 42;
  return {
    statsSinceMs: START - 62 * MIN,
    providers: [
      { id: "energy-meter", name: "CPU power (Windows Energy Meter)", status: available, lastUpdateMs: Date.now(), readingCount: 2 },
      { id: "adlx", name: "AMD GPU sensors (ADLX)", status: available, lastUpdateMs: Date.now(), readingCount: 7 },
      empty
        ? { id: "pawnio-cpu", name: "CPU temperatures (PawnIO)", status: { state: "providerUnavailable", message: "The PawnIO driver is not installed." }, lastUpdateMs: null, readingCount: 0 }
        : { id: "pawnio-cpu", name: "CPU temperatures (PawnIO)", status: available, lastUpdateMs: Date.now(), readingCount: 7 },
      { id: "pawnio-lpc", name: "Motherboard sensors (PawnIO)", status: empty ? { state: "providerUnavailable", message: "The PawnIO driver is not installed." } : available, lastUpdateMs: empty ? null : Date.now(), readingCount: empty ? 0 : 11 },
      { id: "thermal-zone", name: "ACPI thermal zones", status: { state: "unsupported", message: "This firmware exposes no thermal zones." }, lastUpdateMs: null, readingCount: 0 },
    ],
    rows: [
      row("pawnio-cpu", "Intel Core i5-13600KF", "cpu", "temperature", "CPU package", "°C", cpuT),
      ...range(6, (i) => row("pawnio-cpu", "Intel Core i5-13600KF", "cpu", "temperature", `P-core ${i}`, "°C", cpuT - 3 - i)),
      row("energy-meter", "Intel Core i5-13600KF", "cpu", "power", "Package power", "W", issues ? 96 : 41),
      row("energy-meter", "Intel Core i5-13600KF", "cpu", "power", "Cores power", "W", issues ? 81 : 29),
      row("adlx", "AMD Radeon RX 9060 XT", "gpu", "temperature", "GPU temperature", "°C", gpuT),
      row("adlx", "AMD Radeon RX 9060 XT", "gpu", "temperature", "GPU hotspot", "°C", issues ? 106 : 46),
      row("adlx", "AMD Radeon RX 9060 XT", "gpu", "fan", "Fan", "RPM", issues ? 2180 : 1198),
      row("adlx", "AMD Radeon RX 9060 XT", "gpu", "power", "Board power", "W", issues ? 152 : 32),
      row("adlx", "AMD Radeon RX 9060 XT", "gpu", "clock", "Core clock", "MHz", issues ? 2890 : 1810),
      row("adlx", "AMD Radeon RX 9060 XT", "gpu", "clock", "Memory clock", "MHz", 2500, 0),
      row("adlx", "AMD Radeon RX 9060 XT", "gpu", "usage", "VRAM temperature", "°C", null),
      row("pawnio-lpc", "Nuvoton NCT6798D", "motherboard", "fan", "CPU fan", "RPM", issues ? 1640 : 880),
      row("pawnio-lpc", "Nuvoton NCT6798D", "motherboard", "fan", "Chassis fan 1", "RPM", 760),
      row("pawnio-lpc", "Nuvoton NCT6798D", "motherboard", "fan", "Chassis fan 2", "RPM", 742),
      row("pawnio-lpc", "Nuvoton NCT6798D", "motherboard", "fan", "AIO pump", "RPM", 2810, 0.02),
      row("pawnio-lpc", "Nuvoton NCT6798D", "motherboard", "voltage", "Vcore", "V", 1.264, 0.06),
      row("pawnio-lpc", "Nuvoton NCT6798D", "motherboard", "voltage", "+12 V", "V", 12.096, 0.01),
      row("pawnio-lpc", "Nuvoton NCT6798D", "motherboard", "voltage", "+5 V", "V", 5.04, 0.01),
      row("pawnio-lpc", "Nuvoton NCT6798D", "motherboard", "voltage", "+3.3 V", "V", 3.328, 0.01),
      row("pawnio-lpc", "Nuvoton NCT6798D", "motherboard", "temperature", "Motherboard", "°C", 36),
      row("pawnio-lpc", "Nuvoton NCT6798D", "motherboard", "temperature", "VRM", "°C", issues ? 68 : 44),
      row("pawnio-lpc", "Nuvoton NCT6798D", "motherboard", "temperature", "Chipset", "°C", 49),
    ].filter((r) => !empty || !r.provider.startsWith("pawnio")),
  };
}

const nvme = (used: number, hours: number, unsafe: number): T.NvmeHealth => ({ criticalWarning: 0, compositeTemperatureC: 41, availableSparePct: 100, availableSpareThresholdPct: 10, percentageUsed: used, dataUnitsRead: 61_000_000, dataUnitsWritten: 48_000_000, powerCycles: 812, powerOnHours: hours, unsafeShutdowns: unsafe, mediaErrors: 0, errorLogEntries: 0, warningTempMinutes: 0, criticalTempMinutes: 0 });
const drives: T.DriveHealth[] = [
  { index: 2, model: "Samsung SSD 990 PRO 1TB", serial: null, firmware: "4B2QJXD7", busType: "NVMe", sizeBytes: 931 * GB, temperatureC: 41, temperatureWarningC: 82, temperatureCriticalC: 85, nvme: nvme(4, 6120, 21), status: available, healthStatus: available },
  { index: 4, model: "WD_BLACK SN850X 2000GB", serial: null, firmware: "620361WD", busType: "NVMe", sizeBytes: 1863 * GB, temperatureC: 47, temperatureWarningC: 90, temperatureCriticalC: 94, nvme: nvme(issues ? 83 : 2, 3410, 9), status: available, healthStatus: available },
  { index: 3, model: "WDC WD40EZAZ-00SF3B0", serial: null, firmware: "80.00A80", busType: "SATA", sizeBytes: 3726 * GB, temperatureC: null, temperatureWarningC: null, temperatureCriticalC: null, nvme: null, status: { state: "unsupported", message: "This drive does not report NVMe health data." }, healthStatus: available },
];

// ---- stability ------------------------------------------------------------

const bugcheck: T.BugcheckInfo = {
  code: 0x13a,
  name: "KERNEL_MODE_HEAP_CORRUPTION",
  description: "The kernel-mode heap manager detected corruption in a heap.",
  area: "Drivers or faulty memory",
  parameters: [0x12, 0xffffa90f3e402140, 0xffffa90f4a1c5000, 0],
  dumpPath: "C:\\Windows\\Minidump\\110426-9812-01.dmp",
  reportId: "c41b8a52-0d0e-4f44-9d36-2a1b6f0e7c11",
};

function buildEvents(): T.StabilityEvent[] {
  if (!issues) return [];
  let record = 48_000;
  const ev = (daysAgo: number, kind: T.EventKind, severity: T.Severity, title: string, provider: string, eventId: number, detail: T.EventDetail, channel = "Application"): T.StabilityEvent => ({
    channel,
    recordId: record++,
    timeMs: START - daysAgo * DAY - ((record * 7919) % (6 * 3_600_000)),
    provider,
    eventId,
    kind,
    severity,
    title,
    subject: null,
    detail,
    data: [
      ["AppName", "python.exe"],
      ["ExceptionCode", "c0000005"],
      ["FaultingOffset", "00000000001a4c2f"],
    ],
  });
  const crash = (daysAgo: number, appName: string, moduleName: string, code: number, name: string): T.StabilityEvent =>
    ev(daysAgo, "appCrash", "error", `${appName} crashed in ${moduleName}`, "Application Error", 1000, {
      type: "appCrash",
      appName,
      appVersion: "3.14.150.1013",
      appPath: `C:\\Program Files\\${appName.replace(".exe", "")}\\${appName}`,
      moduleName,
      moduleVersion: "3.14.150.1013",
      modulePath: `C:\\Program Files\\${appName.replace(".exe", "")}\\${moduleName}`,
      exceptionCode: code,
      exceptionName: name,
      exceptionDescription: "The program tried to read or write memory it is not allowed to use.",
      faultOffset: "0x00000000001a4c2f",
      processId: 11_204,
      reportId: "7f3a1c0e-2b61-4c0d-8f1a-5d2e9b7c4a10",
      packageFullName: null,
    });
  return [
    ...range(8, (i) => crash(1 + i * 2.4, "python.exe", "python314.DLL", 0xc0000005, "STATUS_ACCESS_VIOLATION")),
    crash(3.2, "Cyberpunk2077.exe", "amdxx64.dll", 0xc0000005, "STATUS_ACCESS_VIOLATION"),
    crash(12.5, "chrome.exe", "ntdll.dll", 0xc0000409, "STATUS_STACK_BUFFER_OVERRUN"),
    ev(5.1, "appHang", "warning", "obs64.exe stopped responding", "Application Hang", 1002, { type: "appHang", appName: "obs64.exe", appVersion: "31.0.2", appPath: "C:\\Program Files\\obs-studio\\bin\\64bit\\obs64.exe", processId: 9120, reportId: null, hangType: "Top level window is idle" }),
    ev(7, "bugcheck", "critical", "Blue screen 0x0000013A KERNEL_MODE_HEAP_CORRUPTION", "Microsoft-Windows-WER-SystemErrorReporting", 1001, { type: "bugcheck", ...bugcheck }, "System"),
    ev(7, "kernelPowerUnexpected", "critical", "Windows restarted without shutting down cleanly", "Microsoft-Windows-Kernel-Power", 41, { type: "kernelPower", bugcheck, powerButtonPressed: false, sleepInProgress: false }, "System"),
    ev(19, "unexpectedShutdown", "error", "The previous system shutdown was unexpected", "EventLog", 6008, { type: "generic" }, "System"),
    ev(7.01, "gpuDriverReset", "warning", "Display driver amduw23g stopped responding and has recovered", "Display", 4101, { type: "generic" }, "System"),
    ev(9, "wheaCorrected", "warning", "A corrected hardware error has occurred (PCI Express Root Port)", "Microsoft-Windows-WHEA-Logger", 17, { type: "generic" }, "System"),
    ev(2, "serviceFailure", "warning", "The Steam Client Service terminated unexpectedly", "Service Control Manager", 7034, { type: "generic" }, "System"),
    ev(6.99, "systemStart", "info", "Windows started", "Microsoft-Windows-Kernel-General", 12, { type: "generic" }, "System"),
    ev(21, "systemShutdown", "info", "Windows shut down", "Microsoft-Windows-Kernel-General", 13, { type: "generic" }, "System"),
  ].sort((a, b) => b.timeMs - a.timeMs);
}
const events = buildEvents();

const findings: T.Finding[] = issues
  ? [
      {
        id: "bugcheck-13a",
        rule: "bugcheck",
        severity: "error",
        title: "Blue screen 0x0000013A KERNEL_MODE_HEAP_CORRUPTION",
        explanation: "Windows stopped itself after the kernel heap manager found corrupted memory. A driver writing outside its own memory is the usual cause; faulty RAM can produce the same code.",
        caveat: "One blue screen on its own does not show which driver is at fault. Analyze the minidump for a lead.",
        suggestions: ["Analyze the minidump in the Blue Screen Analyzer.", "Update or roll back drivers installed shortly before the crash."],
        evidence: [{ timeMs: START - 7 * DAY, text: "WER-SystemErrorReporting 1001: bugcheck 0x0000013a, dump saved to C:\\Windows\\Minidump\\110426-9812-01.dmp", eventRef: null }],
        evidenceTotal: 1,
        firstMs: START - 7 * DAY,
        lastMs: START - 7 * DAY,
      },
      {
        id: "module-python314",
        rule: "repeat-module",
        severity: "info",
        title: "8 crashes faulted in python314.DLL (1 application)",
        explanation: "The same module was the faulting module in 8 application crashes in the last 30 days, all in python.exe.",
        caveat: null,
        suggestions: ["Update or reinstall the application that ships this module."],
        evidence: range(3, (i) => ({ timeMs: START - (1 + i * 2.4) * DAY, text: "Application Error 1000: python.exe 3.14.150.1013, faulting module python314.DLL, exception 0xc0000005", eventRef: null })),
        evidenceTotal: 8,
        firstMs: START - 18 * DAY,
        lastMs: START - DAY,
      },
    ]
  : [];

const activeAlerts: T.AlertSignal[] = issues
  ? [{ ruleId: "gpu-hotspot", ruleName: "GPU hotspot temperature high", metric: "gpu.0.hotspot_c", transition: "fired", value: 106.2, threshold: 105, severity: "warning", notify: true, timeMs: START - 4 * MIN, sinceMs: START - 5 * MIN }]
  : [];

const alertHistory: T.AlertRow[] = issues
  ? range(9, (i) => ({
      id: 90 - i,
      timeMs: START - 4 * MIN - i * 5 * 3_600_000,
      ruleId: i === 3 ? "event:newBsod" : i % 2 ? "memory-pressure" : "gpu-hotspot",
      ruleName: i === 3 ? "New blue screen" : i % 2 ? "Memory almost full" : "GPU hotspot temperature high",
      metric: i === 3 ? "Blue screen 0x0000013A KERNEL_MODE_HEAP_CORRUPTION" : i % 2 ? "memory.load_pct" : "gpu.0.hotspot_c",
      severity: i === 3 ? "critical" : "warning",
      transition: i % 3 === 1 ? "resolved" : "fired",
      value: i % 2 ? 93.4 : 106.2,
      threshold: i % 2 ? 92 : 105,
      sinceMs: START - 5 * MIN - i * 5 * 3_600_000,
    }))
  : [];

function series(key: string, unit: string, fromMs: number, toMs: number, points = 240, gap = true): T.Series {
  const bucketMs = Math.max(5000, Math.round((toMs - fromMs) / points / 5000) * 5000);
  const scale: Record<string, [number, number]> = { "%": [38, 26], "°C": [61, 12], B: [34 * GB, 6 * GB], "B/s": [900_000, 800_000], MHz: [4600, 500], RPM: [1300, 400], W: [84, 40], ms: [14, 3], V: [1.26, 0.04] };
  const [base, amp] = scale[unit] ?? [10, 4];
  const phase = [...key].reduce((a, c) => a + c.charCodeAt(0), 0);
  const t: number[] = [];
  const avg: T.N[] = [];
  const min: T.N[] = [];
  const max: T.N[] = [];
  for (let x = fromMs, i = 0; x <= toMs; x += bucketMs, i++) {
    // One recorded gap a third of the way in, as when the PC was off.
    const off = gap && i > points * 0.3 && i < points * 0.38;
    if (off && i !== Math.ceil(points * 0.3) + 1) continue;
    const v = base + amp * Math.sin(i / 11 + phase) * 0.7 + rnd(amp * 0.25);
    t.push(x);
    avg.push(off ? null : v);
    min.push(off ? null : v - amp * 0.2);
    max.push(off ? null : v + amp * 0.2);
  }
  return { key, unit, bucketMs, source: bucketMs <= 5000 ? "raw" : "minute", t, avg, min, max };
}

const context = (centerMs: number, halfMs: number): T.TelemetryContext => ({
  fromMs: centerMs - halfMs,
  toMs: centerMs + halfMs,
  empty: false,
  series: [series("cpu.total_pct", "%", centerMs - halfMs, centerMs, 120, false), series("memory.load_pct", "%", centerMs - halfMs, centerMs, 120, false), series("gpu.0.util_pct", "%", centerMs - halfMs, centerMs, 120, false), series("gpu.0.temp_c", "°C", centerMs - halfMs, centerMs, 120, false)],
});

const CATALOG: [string, string][] = [
  ["cpu.total_pct", "%"],
  ["cpu.utility_pct", "%"],
  ["cpu.effective_mhz", "MHz"],
  ["cpu.package_temp_c", "°C"],
  ["cpu.package_power_w", "W"],
  ["gpu.0.util_pct", "%"],
  ["gpu.0.temp_c", "°C"],
  ["gpu.0.hotspot_c", "°C"],
  ["gpu.0.vram_used_bytes", "B"],
  ["gpu.0.fan_rpm", "RPM"],
  ["gpu.0.power_w", "W"],
  ["memory.load_pct", "%"],
  ["memory.used_bytes", "B"],
  ["memory.commit_pct", "%"],
  ["disk.read_bps", "B/s"],
  ["disk.write_bps", "B/s"],
  ["volume.C:.free_pct", "%"],
  ["net.rx_bps", "B/s"],
  ["net.tx_bps", "B/s"],
  ["sensor.pawnio-lpc/Nuvoton NCT6798D/CPU fan", "RPM"],
  ["sensor.pawnio-lpc/Nuvoton NCT6798D/Vcore", "V"],
  ["self.cpu_pct", "%"],
  ["self.working_set_bytes", "B"],
  ["self.tick_cost_ms", "ms"],
];

// ---- gaming ---------------------------------------------------------------

const frameStats = (avgFps: number): T.FrameStats => ({ frames: 412_800, discardedFrames: 0, durationMs: 3_480_000, avgFps, currentFps: avgFps + rnd(8), low1pctFps: avgFps * 0.71, low01pctFps: avgFps * 0.52, avgFrametimeMs: 1000 / avgFps, p50FrametimeMs: 990 / avgFps, p95FrametimeMs: 1280 / avgFps, p99FrametimeMs: 1650 / avgFps, maxFrametimeMs: 4100 / avgFps, stddevFrametimeMs: 1.4, pacingVariationMs: 0.82, stutterCount: 37, stutterTimePct: 0.6 });
const frametimes = (n: number, fpsNow: number) => range(n, (i) => 1000 / fpsNow + rnd(1.6) + (i % 190 === 0 ? 14 : 0));
const agg = (avg: number, max: number): T.Agg => ({ avg, max, samples: 3480 });
const sessions: T.SessionRow[] = empty
  ? []
  : range(6, (i) => ({
      id: 40 - i,
      processName: ["Cyberpunk2077.exe", "eldenring.exe", "cs2.exe", "Cyberpunk2077.exe", "HorizonForbiddenWest.exe", "cs2.exe"][i]!,
      exePath: null,
      pid: 9000 + i,
      startedMs: START - (i + 0.2) * DAY,
      endedMs: START - (i + 0.2) * DAY + (38 + i * 9) * MIN,
      provider: "PresentMon",
      stats: { ...frameStats([118, 60, 342, 104, 91, 356][i]!), runtime: "DX12", presentMode: "Hardware: Independent Flip" },
      context: { cpuPct: agg(41, 78), gpuPct: agg(i === 2 ? 62 : 96, 99), gpuTempC: agg(74, 79), cpuTempC: agg(72, 88), vramBytes: agg(11 * GB, 13.9 * GB), ramUsedBytes: agg(38 * GB, 44 * GB), processCpuPct: agg(22, 41), processGpuPct: agg(94, 99), timeline: [] },
    }));

let capturing = query.has("cap") && !empty;
const captureStart = START - 14 * MIN;
function gaming(): T.GamingLive {
  return {
    provider: "PresentMon",
    providerStatus: empty ? { state: "providerUnavailable", message: "PresentMon was not found in the tools folder." } : available,
    capturing,
    pid: capturing ? 2000 : null,
    processName: capturing ? "Cyberpunk2077.exe" : null,
    exePath: null,
    startedMs: capturing ? captureStart : null,
    durationSecs: capturing ? (Date.now() - captureStart) / 1000 : 0,
    frames: capturing ? Math.round(((Date.now() - captureStart) / 1000) * 118) : 0,
    otherSwapChainFrames: 0,
    lostFrames: 0,
    runtime: capturing ? "DX12" : null,
    presentMode: capturing ? "Hardware: Independent Flip" : null,
    live: frameStats(118),
    recentFrametimes: capturing ? frametimes(600, 118) : [],
    context: capturing ? { t: Date.now(), cpuPct: 44, gpuPct: 96, gpuTempC: 78, cpuTempC: 81, vramBytes: 13.9 * GB, ramUsedBytes: 41 * GB, processCpuPct: 23, processGpuPct: 94 } : null,
    lastSavedSession: null,
    autoStarted: true,
  };
}

// ---- packets --------------------------------------------------------------

const PACKETS: [string, string, string, number, string][] = [
  ["192.168.1.24", "1.1.1.1", "DNS", 74, "Standard query 0x3f1a A api.steampowered.com"],
  ["1.1.1.1", "192.168.1.24", "DNS", 106, "Standard query response 0x3f1a A 23.52.74.146"],
  ["192.168.1.24", "23.52.74.146", "TCP", 66, "51844 → 443 [SYN] Seq=0 Win=64240 Len=0 MSS=1460"],
  ["23.52.74.146", "192.168.1.24", "TCP", 66, "443 → 51844 [SYN, ACK] Seq=0 Ack=1 Win=65160 Len=0"],
  ["192.168.1.24", "23.52.74.146", "TLSv1.3", 571, "Client Hello (SNI=api.steampowered.com)"],
  ["23.52.74.146", "192.168.1.24", "TLSv1.3", 1514, "Server Hello, Change Cipher Spec, Application Data"],
  ["192.168.1.24", "192.168.1.1", "ICMP", 74, "Echo (ping) request id=0x0001, seq=411"],
  ["192.168.1.1", "192.168.1.24", "ICMP", 74, "Echo (ping) reply id=0x0001, seq=411"],
  ["192.168.1.24", "142.250.74.14", "QUIC", 1292, "Initial, DCID=8f1c2ab4"],
  ["192.168.1.24", "224.0.0.251", "mDNS", 87, "Standard query PTR _googlecast._tcp.local"],
  ["192.168.1.24", "1.1.1.1", "DNS", 81, "Standard query 0x77c0 A telemetry.invalid"],
  ["1.1.1.1", "192.168.1.24", "DNS", 81, "Standard query response 0x77c0 No such name"],
  ["34.117.59.81", "192.168.1.24", "TCP", 54, "443 → 51790 [RST, ACK] Seq=1 Ack=518 Win=0 Len=0"],
  ["192.168.1.24", "192.168.1.255", "ARP", 42, "Who has 192.168.1.50? Tell 192.168.1.24"],
];
let packetState: T.CaptureState["state"] = issues ? "stopped" : "idle";
let packets: T.PacketRow[] = issues ? range(84, packetRow) : [];
function packetRow(i: number): T.PacketRow {
  const [src, dst, protocol, length, info] = PACKETS[i % PACKETS.length]!;
  return { id: i + 1, timeUs: i * 41_337, ifIndex: 12, outbound: src.startsWith("192.168.1.24"), src, dst, protocol, length, info };
}
function packetPage(afterId: number, filter: string): T.PacketPage {
  if (packetState === "capturing") packets = [...packets, ...range(3, (i) => packetRow(packets.length + i))];
  const term = filter.trim().split(/[\s.=]/)[0]?.toLowerCase() ?? "";
  const rows = packets.filter((p) => p.id > afterId && (!term || `${p.protocol} ${p.info}`.toLowerCase().includes(term)));
  const state = (packetState === "failed" ? { state: "failed", error: "sample" } : { state: packetState }) as T.CaptureState;
  return { rows, lastId: packets.at(-1)?.id ?? 0, more: false, oldestId: 1, total: packets.length, evicted: 0, interface: 12, lostEvents: 0, lostBuffers: 0, ...state };
}
function packetDetail(id: number): T.PacketDetail {
  const p = packets.find((x) => x.id === id) ?? packetRow(0);
  return {
    id,
    timeUtc: new Date(START - 60_000 + p.timeUs / 1000).toISOString(),
    timeUs: p.timeUs,
    ifIndex: 12,
    outbound: p.outbound,
    bytes: range(Math.min(p.length, 96), (i) => (i * 37 + id * 11) % 256),
    layers: [
      { proto: "frame", name: `Frame ${id}`, summary: `${p.length} bytes on wire`, offset: 0, len: p.length, fields: [{ label: "Length", value: `${p.length} bytes`, offset: 0, len: p.length }] },
      { proto: "eth", name: "Ethernet II", summary: "Src: 04:7c:16:2a:91:0e, Dst: f4:92:bf:71:20:c4", offset: 0, len: 14, fields: [{ key: "eth.src", label: "Source", value: "04:7c:16:2a:91:0e", offset: 6, len: 6 }, { key: "eth.dst", label: "Destination", value: "f4:92:bf:71:20:c4", offset: 0, len: 6 }, { key: "eth.type", label: "Type", value: "0x0800 (IPv4)", offset: 12, len: 2 }] },
      { proto: "ip", name: "Internet Protocol Version 4", summary: `Src: ${p.src}, Dst: ${p.dst}`, offset: 14, len: 20, fields: [{ key: "ip.src", label: "Source address", value: p.src, offset: 26, len: 4 }, { key: "ip.dst", label: "Destination address", value: p.dst, offset: 30, len: 4 }, { key: "ip.ttl", label: "Time to live", value: "128", offset: 22, len: 1 }] },
      { proto: p.protocol.toLowerCase(), name: p.protocol, summary: p.info, offset: 34, len: p.length - 34, fields: [{ label: "Info", value: p.info, offset: 34, len: p.length - 34 }] },
    ],
  };
}

// ---- settings and app state -----------------------------------------------

const rule = (id: string, name: string, metric: string, threshold: number, hysteresis: number, durationSecs: number, cooldownSecs: number, severity: T.Severity, notify: boolean, comparator: "above" | "below" = "above"): T.ThresholdRule => ({ id, name, enabled: true, metric, comparator, threshold, hysteresis, durationSecs, cooldownSecs, severity, notify });
let settings: T.Settings = {
  theme: (query.get("theme") as T.Settings["theme"] | null) ?? "cassette",
  sampleIntervalMs: 1000,
  sensorIntervalMs: 2000,
  storageHealthIntervalMin: 30,
  historyEnabled: true,
  retention: { rawHours: 48, minuteDays: 30, eventDays: 365, sessionDays: 365 },
  closeToTray: true,
  minimizeToTray: false,
  autoUpdateCheck: true,
  startMinimized: false,
  notificationsEnabled: true,
  eventAlerts: { newBsod: true, unexpectedRestart: true, repeatedCrashes: true, crashCount: 3, crashWindowMin: 60 },
  alertRules: [
    rule("gpu-temp", "GPU temperature high", "gpu.*.temp_c", 90, 5, 30, 900, "warning", true),
    rule("gpu-hotspot", "GPU hotspot temperature high", "gpu.*.hotspot_c", 105, 5, 30, 900, "warning", true),
    rule("cpu-temp", "CPU package temperature high", "cpu.package_temp_c", 95, 5, 30, 900, "warning", true),
    rule("cpu-sustained", "Sustained high CPU usage", "cpu.total_pct", 95, 10, 300, 3600, "info", false),
    rule("memory-pressure", "Memory almost full", "memory.load_pct", 92, 4, 120, 1800, "warning", true),
    rule("disk-space", "Drive almost full", "volume.*.free_pct", 5, 2, 60, 86_400, "warning", true, "below"),
  ],
  presentmonPath: null,
  debuggerPath: null,
  symbolServerEnabled: false,
  stutterFactor: 2.5,
  overlay: { enabled: true, position: "topLeft", scale: 1, opacity: 1, refreshMs: 1000, design: "essential/sleek-compact", theme: "dark" },
  exportRedactDefault: true,
  autoCaptureGames: true,
};

const runtimeStatus: T.RuntimeStatus = {
  version: "1.0.5",
  elevated: true,
  dataDir: "C:\\Users\\Sample\\AppData\\Local\\app.veyl.monitor",
  logDir: "C:\\Users\\Sample\\AppData\\Local\\app.veyl.monitor\\logs",
  toolsDir: "C:\\Program Files\\Veyl\\tools",
  dbError: null,
  dbStats: { fileBytes: 61.4 * MB, rawRows: 1_382_400, minuteRows: 734_000, events: 1840, sessions: 6, alerts: 90 },
  droppedWriteBatches: 0,
  eventChannels: [
    { channel: "Application", status: available, storedEvents: 1410 },
    { channel: "System", status: available, storedEvents: 430 },
  ],
  startedMs: START - 62 * MIN,
};

const updateStatus: T.UpdateStatus = issues
  ? { currentVersion: "1.0.5", lastCheckMs: START - 20 * MIN, state: "available", update: { version: "1.0.6", notes: null, date: null } }
  : { currentVersion: "1.0.5", lastCheckMs: START - 20 * MIN, state: "upToDate" };

const specs: T.SpecSection[] = [
  { title: "System", items: [["Windows", "Windows 11 Home 26H2 (build 26300.9711)"], ["Architecture", "x64"], ["Secure Boot", "On"], ["TPM", "2.0"]] },
  { title: "Processor", items: [["Model", "13th Gen Intel(R) Core(TM) i5-13600KF"], ["Cores", "14 (6 performance + 8 efficiency)"], ["Threads", "20"], ["Base clock", "3.50 GHz"], ["Socket", "LGA1700"]] },
  { title: "Motherboard", items: [["Manufacturer", "ASUSTeK COMPUTER INC."], ["Model", "TUF GAMING Z790-PLUS WIFI"], ["BIOS", "1812 (2026-06-12)"]] },
  { title: "Memory", items: [["Installed", "64 GB (2 × 32 GB)"], ["Type", "DDR5"], ["Speed", "6000 MHz"], ["Slots used", "2 of 4"]] },
  { title: "Graphics", items: [["Adapter", "AMD Radeon RX 9060 XT"], ["Dedicated memory", "16 GB"], ["Driver", "32.0.21030.2001 (2026-10-02)"]] },
  { title: "Storage", items: [["Disk 2", "Samsung SSD 990 PRO 1TB (NVMe)"], ["Disk 4", "WD_BLACK SN850X 2000GB (NVMe)"], ["Disk 3", "WDC WD40EZAZ 4 TB (SATA)"]] },
  { title: "Displays", items: [["Display 1", "2560 × 1440 at 165 Hz"], ["Display 2", "1920 × 1080 at 60 Hz"]] },
  { title: "Network", items: [["Ethernet", "Intel(R) Ethernet Controller I226-V (1.0 Gbps)"], ["Ethernet 2", "Realtek Gaming 2.5GbE Family Controller"], ["Wi-Fi", "MediaTek Wi-Fi 7 MT7925"]] },
].map((s) => ({ title: s.title, items: s.items.map(([label, value]) => ({ label: label!, value: value! })) }));

// ---- IPC ------------------------------------------------------------------

type Args = Record<string, any>; // eslint-disable-line @typescript-eslint/no-explicit-any
const later = <R>(value: R, ms: number) => new Promise<R>((resolve) => setTimeout(() => resolve(value), ms));

const handlers: Record<string, (a: Args) => unknown> = {
  get_system_info: () => systemInfo,
  get_latest: () => (empty ? null : frame()),
  get_runtime_status: () => runtimeStatus,
  get_update_status: () => updateStatus,
  check_for_update: () => later(updateStatus, 800),

  get_processes: () => (empty ? { status: available, timestampMs: Date.now(), logicalProcessors: 20, rows: [], appPids: [] } : processes()),
  get_process_details: (a): T.ProcessDetails => ({ pid: a.pid, path: `C:\\Program Files\\${String(a.name).replace(".exe", "")}\\${a.name}`, commandLine: `"C:\\Program Files\\${String(a.name).replace(".exe", "")}\\${a.name}" --launcher`, priority: "normal", critical: false, protectedByPolicy: false, file: { company: "Sample Publisher", description: String(a.name).replace(".exe", ""), product: null, fileVersion: "2.3.1.0", sizeBytes: 71 * MB }, accessNote: null }),
  end_process_tree: (a): T.TreeResult => ({ ended: [a.pid], failed: [] }),
  get_services: () => services,
  get_startup_items: () => startupItems,
  get_connections: () => connections,

  get_sensors: () => sensors(),
  get_drive_health: () => ({ drives: empty ? [] : drives, updatedMs: empty ? null : Date.now() - 3 * MIN }),
  install_pawnio: () => later(Promise.reject("Sample data: nothing is installed in the browser preview."), 1200),
  install_debugger: () => later(Promise.reject("Sample data: nothing is installed in the browser preview."), 1200),

  get_stability_events: (a) => events.filter((e) => e.timeMs >= a.fromMs && e.timeMs <= a.toMs),
  get_event_channels: () => runtimeStatus.eventChannels,
  get_event_detail: (a): T.EventDetailView => {
    const event = events.find((e) => e.channel === a.channel && e.recordId === a.recordId) ?? events[0]!;
    return {
      event,
      message: "Faulting application name: python.exe, version: 3.14.150.1013\nFaulting module name: python314.DLL, version: 3.14.150.1013\nException code: 0xc0000005\nFault offset: 0x00000000001a4c2f",
      related: events.filter((e) => e !== event && Math.abs(e.timeMs - event.timeMs) < DAY).slice(0, 3),
      linkedReport: { ...event, kind: "werReport", severity: "info", title: "Windows Error Reporting", detail: { type: "werReport", eventName: "APPCRASH", bucket: "f1e2d3c4b5a69788", reportId: "7f3a1c0e-2b61-4c0d-8f1a-5d2e9b7c4a10", parameters: [], attachedFiles: ["Report.wer"], storePath: "C:\\ProgramData\\Microsoft\\Windows\\WER\\ReportArchive\\AppCrash_python.exe_f1e2" } },
      moduleFile: { company: "Python Software Foundation", description: "Python Core", product: "Python", fileVersion: "3.14.150.1013", sizeBytes: 6 * MB },
      appFile: null,
      sameAppCount30d: 8,
      sameModuleCount30d: 8,
      context: context(event.timeMs, 15 * MIN),
    };
  },
  get_bsod_overview: (): T.BsodOverview => ({
    incidents: issues
      ? [
          { timeMs: START - 7 * DAY, kind: "confirmedBugcheck", bugcheck, powerButtonPressed: false, previousShutdownText: null, evidence: events.filter((e) => e.kind === "bugcheck" || e.kind === "kernelPowerUnexpected").map((e) => ({ channel: e.channel, recordId: e.recordId, timeMs: e.timeMs, kind: e.kind, title: e.title })) },
          { timeMs: START - 19 * DAY, kind: "unexpectedShutdown", bugcheck: null, powerButtonPressed: true, previousShutdownText: "The previous system shutdown at 21:14:07 was unexpected.", evidence: events.filter((e) => e.kind === "unexpectedShutdown").map((e) => ({ channel: e.channel, recordId: e.recordId, timeMs: e.timeMs, kind: e.kind, title: e.title })) },
        ]
      : [],
    dumps: { files: issues ? [{ path: "C:\\Windows\\Minidump\\110426-9812-01.dmp", sizeBytes: 2.1 * MB, modifiedMs: START - 7 * DAY, kind: "Minidump" }, { path: "C:\\Windows\\LiveKernelReports\\WATCHDOG-20261029-2204.dmp", sizeBytes: 412 * MB, modifiedMs: START - 13 * DAY, kind: "Live kernel report" }] : [], inaccessible: [] },
    debuggerPath: empty ? null : "C:\\Program Files (x86)\\Windows Kits\\10\\Debuggers\\x64\\cdb.exe",
    symbolServerEnabled: false,
    elevated: true,
  }),
  analyze_dump: (a): Promise<T.DumpAnalysis> =>
    later(
      { dumpPath: a.path, debuggerPath: "cdb.exe", symbolServerUsed: false, bugcheckCode: 0x13a, bugcheckParameters: bugcheck.parameters, moduleName: "amdkmdag", imageName: "amdkmdag.sys", imageVersion: "32.0.21030.2001", processName: "Cyberpunk2077.exe", symbolName: "amdkmdag+0x1a4c2f", failureBucket: "0x13a_12_amdkmdag!unknown_function", stack: ["nt!KeBugCheckEx", "nt!RtlpHeapHandleError+0x40", "amdkmdag+0x1a4c2f"], symbolsResolved: false, warnings: ["Symbols for amdkmdag.sys were not found; function names are approximate."], fields: [], rawOutput: "*** Sample debugger output ***\nBUGCHECK_CODE:  13a\nIMAGE_NAME:  amdkmdag.sys", durationMs: 9400 },
      1500,
    ),
  get_diagnostics: () => findings,
  get_timeline: (a): T.TimelineView => ({ events: events.filter((e) => Math.abs(e.timeMs - a.centerMs) <= a.halfWindowMs), context: context(a.centerMs, a.halfWindowMs) }),

  get_gaming_status: () => gaming(),
  get_game_candidates: (): T.Candidate[] => (empty ? [] : [{ pid: 2000, name: "Cyberpunk2077.exe", path: null, gpuPct: issues ? 92 : 4, gpuDedicatedBytes: 11.2 * GB, foreground: true, isGame: true }, { pid: 2412, name: "chrome.exe", path: null, gpuPct: 1, gpuDedicatedBytes: 0.4 * GB, foreground: false, isGame: false }, { pid: 4884, name: "obs64.exe", path: null, gpuPct: 3, gpuDedicatedBytes: 0.3 * GB, foreground: false, isGame: false }]),
  start_capture: () => void (capturing = true),
  stop_capture: () => void (capturing = false),
  get_game_sessions: () => sessions,
  get_game_session: (a): T.SessionDetail => {
    const session = sessions.find((s) => s.id === a.id) ?? sessions[0]!;
    return { session, frametimes: frametimes(12_000, session.stats.avgFps ?? 60) };
  },
  set_overlay_enabled: (a) => void (settings = { ...settings, overlay: { ...settings.overlay, enabled: a.enabled } }),
  set_auto_capture_games: (a) => void (settings = { ...settings, autoCaptureGames: a.enabled }),
  set_overlay_style: (a) => void (settings = { ...settings, overlay: { ...settings.overlay, design: a.design, theme: a.theme } }),
  set_overlay_position: (a) => void (settings = { ...settings, overlay: { ...settings.overlay, position: a.position } }),

  get_metric_catalog: (): T.MetricInfo[] => (empty ? [] : CATALOG.map(([key, unit]) => ({ key, unit, firstMs: START - 30 * DAY, lastMs: START }))),
  get_history: (a) => (a.keys as string[]).map((k) => series(k, CATALOG.find(([key]) => key === k)?.[1] ?? "", a.fromMs, a.toMs)),
  get_alert_history: () => alertHistory,
  get_active_alerts: () => activeAlerts,
  clear_active_alerts: () => void (activeAlerts.length = 0),
  get_pc_specs: () => specs,

  get_capture_interfaces: (): T.CaptureInterface[] => [{ ifIndex: 12, name: "Ethernet", description: "Intel(R) Ethernet Controller I226-V", media: "Ethernet", connected: true, rateBps: 48_000 }, { ifIndex: 18, name: "Wi-Fi", description: "MediaTek Wi-Fi 7 MT7925", media: "Wi-Fi", connected: false, rateBps: null }],
  start_packet_capture: () => void ((packets = []), (packetState = "capturing")),
  stop_packet_capture: () => void (packetState = "stopped"),
  clear_packets: () => void ((packets = []), (packetState = "idle")),
  get_packets: (a) => packetPage(a.afterId, a.filter),
  get_packet_detail: (a) => packetDetail(a.id),
  check_packet_filter: (a) => (String(a.filter).includes("((") ? Promise.reject("Unexpected '(' at position 1") : undefined),

  get_settings: () => settings,
  save_settings: (a) => {
    settings = a.settings;
    emit("settings-changed", null);
    return settings;
  },
  reset_alert_rules: () => settings,
};

// Events: the app registers a callback id per event name; `emit` calls them like the backend would.
const callbacks = new Map<number, (e: unknown) => void>();
const listeners = new Map<string, number[]>();
let nextCallback = 1;
function emit(event: string, payload: unknown) {
  for (const id of listeners.get(event) ?? []) callbacks.get(id)?.({ event, id, payload });
}

function onListen(event: string, handler: number) {
  listeners.set(event, [...(listeners.get(event) ?? []), handler]);
  if (event === "telemetry" && !empty && (listeners.get(event)?.length ?? 0) === 1) {
    // Three minutes of history first so the live charts are not empty, then one frame a second.
    for (let i = 180; i > 0; i--) emit("telemetry", frame(Date.now() - i * 1000));
    setInterval(() => emit("telemetry", frame()), 1000);
    setInterval(() => capturing && emit("gaming", gaming()), 1000);
  }
  return handler;
}

Object.assign(window, {
  __TAURI_INTERNALS__: {
    metadata: { currentWindow: { label: "main" }, currentWebview: { label: "main", windowLabel: "main" } },
    transformCallback(callback: (e: unknown) => void) {
      callbacks.set(nextCallback, callback);
      return nextCallback++;
    },
    unregisterCallback: (id: number) => void callbacks.delete(id),
    convertFileSrc: (path: string) => path,
    async invoke(cmd: string, args: Args = {}) {
      if (cmd === "plugin:event|listen") return onListen(args.event, args.handler);
      if (cmd.startsWith("plugin:")) return null;
      // Commands without sample data (exports, folder openers, process actions) succeed and do nothing.
      return (await handlers[cmd]?.(args)) ?? null;
    },
  },
  __TAURI_EVENT_PLUGIN_INTERNALS__: { unregisterListener: () => undefined },
});
