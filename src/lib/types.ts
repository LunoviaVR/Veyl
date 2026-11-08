// TypeScript mirrors of the Rust models (serde camelCase). Optional Rust values
// arrive as `null`: null always means "not measured", never zero.

export type Availability =
  | "available"
  | "unsupported"
  | "permissionRequired"
  | "providerUnavailable"
  | "stale"
  | "waitingForSamples"
  | "error";

export interface Status {
  state: Availability;
  message: string | null;
}

export type N = number | null;

export interface CpuInfo {
  brand: string | null;
  vendor: string | null;
  physicalCores: N;
  logicalProcessors: N;
  packages: N;
  performanceCores: N;
  efficiencyCores: N;
  baseMhz: N;
}

export interface GpuInfo {
  luid: string;
  name: string;
  vendorId: number;
  deviceId: number;
  dedicatedMemoryBytes: number;
  sharedMemoryBytes: number;
  isSoftware: boolean;
}

export interface SystemInfo {
  computerName: string | null;
  osName: string | null;
  osBuild: string | null;
  bootTimeMs: N;
  cpu: CpuInfo;
  installedMemoryBytes: N;
  gpus: GpuInfo[];
  elevated: boolean;
  status: string[];
}

export interface CpuSnapshot {
  status: Status;
  totalPct: N;
  utilityPct: N;
  /** One slot per logical processor; null when Windows had no valid value this tick. */
  perCorePct: (number | null)[];
  baseMhz: N;
  effectiveMhz: N;
  perCoreMhz: (number | null)[];
  packageTempC: N;
  packageTempSource: string | null;
  packagePowerW: N;
  processCount: N;
  threadCount: N;
  handleCount: N;
  uptimeSecs: N;
}

export interface MemorySnapshot {
  status: Status;
  totalBytes: N;
  availableBytes: N;
  usedBytes: N;
  loadPct: N;
  commitBytes: N;
  commitLimitBytes: N;
  cachedBytes: N;
  pagedPoolBytes: N;
  nonpagedPoolBytes: N;
  pagesInputPerSec: N;
}

export interface VolumeSnapshot {
  mount: string;
  label: string | null;
  fileSystem: string | null;
  driveType: string;
  totalBytes: N;
  freeBytes: N;
}

export interface PhysicalDiskActivity {
  name: string;
  index: N;
  readBps: N;
  writeBps: N;
  activePct: N;
  queueLength: N;
}

export interface DisksSnapshot {
  status: Status;
  volumes: VolumeSnapshot[];
  physical: PhysicalDiskActivity[];
  totalReadBps: N;
  totalWriteBps: N;
}

export interface NetworkAdapterSnapshot {
  luid: number;
  ifIndex: number;
  alias: string;
  description: string;
  media: string;
  connected: boolean;
  linkSpeedBps: N;
  rxBps: N;
  txBps: N;
  rxTotalBytes: number;
  txTotalBytes: number;
}

export interface NetworkSnapshot {
  status: Status;
  adapters: NetworkAdapterSnapshot[];
  totalRxBps: N;
  totalTxBps: N;
}

export interface GpuSnapshot {
  luid: string;
  name: string;
  vendorId: number;
  utilPct: N;
  engines: { engineType: string; utilPct: number }[];
  dedicatedTotalBytes: number;
  dedicatedUsedBytes: N;
  sharedUsedBytes: N;
  temperatureC: N;
  temperatureMaxC: N;
  hotspotC: N;
  fanRpm: N;
  fanPct: N;
  powerPct: N;
  powerW: N;
  coreClockMhz: N;
  memoryClockMhz: N;
  countersStatus: Status;
  sensorsStatus: Status;
}

export interface SelfUsage {
  cpuPct: N;
  workingSetBytes: N;
  privateBytes: N;
  tickCostMs: N;
}

export interface TelemetryFrame {
  seq: number;
  timestampMs: number;
  cpu: CpuSnapshot;
  memory: MemorySnapshot;
  disks: DisksSnapshot;
  network: NetworkSnapshot;
  gpus: { status: Status; adapters: GpuSnapshot[] };
  selfUsage: SelfUsage;
}

export interface ProcessRow {
  pid: number;
  parentPid: number;
  startTimeMs: N;
  name: string;
  path: string | null;
  cpuPct: N;
  workingSetBytes: number;
  privateWorkingSetBytes: number;
  privateBytes: number;
  ioReadBps: N;
  ioWriteBps: N;
  gpuPct: N;
  gpuDedicatedBytes: N;
  threads: number;
  handles: number;
  sessionId: number;
  suspended: boolean;
}

export interface ProcessTable {
  status: Status;
  timestampMs: number;
  logicalProcessors: number;
  rows: ProcessRow[];
  /** Processes owning a visible app window (Task Manager's "Apps" group). */
  appPids: number[];
}

export type Priority = "idle" | "belowNormal" | "normal" | "aboveNormal" | "high" | "realtime";

export interface FileVersionInfo {
  company: string | null;
  description: string | null;
  product: string | null;
  fileVersion: string | null;
  sizeBytes: N;
}

export interface ProcessDetails {
  pid: number;
  path: string | null;
  commandLine: string | null;
  priority: Priority | null;
  critical: boolean | null;
  protectedByPolicy: boolean;
  file: FileVersionInfo | null;
  accessNote: string | null;
}

export interface ServiceInfo {
  name: string;
  displayName: string;
  state: string;
  pid: N;
  startType: string | null;
  binaryPath: string | null;
  account: string | null;
}

export interface StartupItem {
  name: string;
  command: string;
  executable: string | null;
  location: string;
  scope: string;
  enabled: boolean | null;
}

export interface ConnectionRow {
  protocol: string;
  localAddress: string;
  localPort: number;
  remoteAddress: string | null;
  remotePort: N;
  state: string | null;
  pid: number;
  processName: string | null;
}

export type SensorCategory = "cpu" | "gpu" | "motherboard" | "memory" | "storage" | "system" | "other";
export type SensorKind = "temperature" | "voltage" | "fan" | "current" | "power" | "clock" | "usage" | "throttle" | "health" | "other";

export interface SensorRow {
  key: string;
  provider: string;
  device: string;
  category: SensorCategory;
  kind: SensorKind;
  label: string;
  unit: string;
  value: N;
  timestampMs: number;
  stats: { min: number; max: number; avg: number; samples: number } | null;
}

export interface ProviderStatus {
  id: string;
  name: string;
  status: Status;
  lastUpdateMs: N;
  readingCount: number;
}

export interface SensorsView {
  providers: ProviderStatus[];
  rows: SensorRow[];
  statsSinceMs: number;
}

export interface NvmeHealth {
  criticalWarning: number;
  compositeTemperatureC: N;
  availableSparePct: number;
  availableSpareThresholdPct: number;
  percentageUsed: number;
  dataUnitsRead: number;
  dataUnitsWritten: number;
  powerCycles: number;
  powerOnHours: number;
  unsafeShutdowns: number;
  mediaErrors: number;
  errorLogEntries: number;
  warningTempMinutes: number;
  criticalTempMinutes: number;
}

export interface DriveHealth {
  index: number;
  model: string | null;
  serial: string | null;
  firmware: string | null;
  busType: string | null;
  sizeBytes: N;
  temperatureC: N;
  temperatureWarningC: N;
  temperatureCriticalC: N;
  nvme: NvmeHealth | null;
  status: Status;
  healthStatus: Status;
}

export type Severity = "info" | "warning" | "error" | "critical";
export type EventKind =
  | "appCrash"
  | "appHang"
  | "werReport"
  | "bugcheck"
  | "kernelPowerUnexpected"
  | "unexpectedShutdown"
  | "wheaError"
  | "wheaCorrected"
  | "gpuDriverReset"
  | "storageError"
  | "serviceFailure"
  | "systemStart"
  | "systemShutdown";

export interface BugcheckInfo {
  code: N;
  name: string | null;
  description: string | null;
  area: string | null;
  parameters: number[];
  dumpPath: string | null;
  reportId: string | null;
}

export interface AppCrash {
  appName: string | null;
  appVersion: string | null;
  appPath: string | null;
  moduleName: string | null;
  moduleVersion: string | null;
  modulePath: string | null;
  exceptionCode: N;
  exceptionName: string | null;
  exceptionDescription: string | null;
  faultOffset: string | null;
  processId: N;
  reportId: string | null;
  packageFullName: string | null;
}

export type EventDetail =
  | ({ type: "appCrash" } & AppCrash)
  | { type: "appHang"; appName: string | null; appVersion: string | null; appPath: string | null; processId: N; reportId: string | null; hangType: string | null }
  | { type: "werReport"; eventName: string | null; bucket: string | null; reportId: string | null; parameters: string[]; attachedFiles: string[]; storePath: string | null }
  | ({ type: "bugcheck" } & BugcheckInfo)
  | { type: "kernelPower"; bugcheck: BugcheckInfo | null; powerButtonPressed: boolean; sleepInProgress: boolean }
  | { type: "generic" };

export interface StabilityEvent {
  channel: string;
  recordId: number;
  timeMs: number;
  provider: string;
  eventId: number;
  kind: EventKind;
  severity: Severity;
  title: string;
  subject: string | null;
  detail: EventDetail;
  data: [string, string][];
}

export interface EventRef {
  channel: string;
  recordId: number;
  timeMs: number;
  kind: EventKind;
  title: string;
}

export interface ShutdownIncident {
  timeMs: number;
  kind: "confirmedBugcheck" | "unexpectedShutdown";
  bugcheck: BugcheckInfo | null;
  powerButtonPressed: boolean;
  previousShutdownText: string | null;
  evidence: EventRef[];
}

export interface Evidence {
  timeMs: N;
  text: string;
  eventRef: string | null;
}

export interface Finding {
  id: string;
  rule: string;
  severity: Severity;
  title: string;
  explanation: string;
  caveat: string | null;
  suggestions: string[];
  evidence: Evidence[];
  evidenceTotal: number;
  firstMs: N;
  lastMs: N;
}

export interface Series {
  key: string;
  unit: string;
  bucketMs: number;
  source: string;
  t: number[];
  avg: N[];
  min: N[];
  max: N[];
}

export interface MetricInfo {
  key: string;
  unit: string;
  firstMs: N;
  lastMs: N;
}

export interface TelemetryContext {
  fromMs: number;
  toMs: number;
  series: Series[];
  empty: boolean;
}

export interface EventDetailView {
  event: StabilityEvent;
  message: string | null;
  related: StabilityEvent[];
  linkedReport: StabilityEvent | null;
  moduleFile: FileVersionInfo | null;
  appFile: FileVersionInfo | null;
  sameAppCount30d: number;
  sameModuleCount30d: number;
  context: TelemetryContext;
}

export interface DumpFile {
  path: string;
  sizeBytes: number;
  modifiedMs: N;
  kind: string;
}

export interface BsodOverview {
  incidents: ShutdownIncident[];
  dumps: { files: DumpFile[]; inaccessible: string[] };
  debuggerPath: string | null;
  symbolServerEnabled: boolean;
  elevated: boolean;
}

export interface DumpAnalysis {
  dumpPath: string;
  debuggerPath: string;
  symbolServerUsed: boolean;
  bugcheckCode: N;
  bugcheckParameters: number[];
  moduleName: string | null;
  imageName: string | null;
  imageVersion: string | null;
  processName: string | null;
  symbolName: string | null;
  failureBucket: string | null;
  stack: string[];
  symbolsResolved: boolean;
  warnings: string[];
  fields: [string, string][];
  rawOutput: string;
  durationMs: number;
}

export interface TimelineView {
  events: StabilityEvent[];
  context: TelemetryContext;
}

export interface ThresholdRule {
  id: string;
  name: string;
  enabled: boolean;
  metric: string;
  comparator: "above" | "below";
  threshold: number;
  hysteresis: number;
  durationSecs: number;
  cooldownSecs: number;
  severity: Severity;
  notify: boolean;
}

export interface AlertSignal {
  ruleId: string;
  ruleName: string;
  metric: string;
  transition: "fired" | "resolved";
  value: number;
  threshold: number;
  severity: Severity;
  notify: boolean;
  timeMs: number;
  sinceMs: number;
}

export interface AlertRow {
  id: number;
  timeMs: number;
  ruleId: string;
  ruleName: string;
  metric: string;
  severity: string;
  transition: string;
  value: number;
  threshold: number;
  sinceMs: number;
}

export interface OverlaySettings {
  enabled: boolean;
  position: "topLeft" | "topRight" | "bottomLeft" | "bottomRight";
  scale: number;
  opacity: number;
  metrics: string[];
  refreshMs: number;
  layout: "vertical" | "horizontal";
}

export interface Settings {
  theme: "system" | "dark" | "light";
  sampleIntervalMs: number;
  sensorIntervalMs: number;
  storageHealthIntervalMin: number;
  historyEnabled: boolean;
  retention: { rawHours: number; minuteDays: number; eventDays: number; sessionDays: number };
  closeToTray: boolean;
  minimizeToTray: boolean;
  autoUpdateCheck: boolean;
  startMinimized: boolean;
  notificationsEnabled: boolean;
  eventAlerts: { newBsod: boolean; unexpectedRestart: boolean; repeatedCrashes: boolean; crashCount: number; crashWindowMin: number };
  alertRules: ThresholdRule[];
  presentmonPath: string | null;
  debuggerPath: string | null;
  symbolServerEnabled: boolean;
  stutterFactor: number;
  overlay: OverlaySettings;
  exportRedactDefault: boolean;
}

export interface FrameStats {
  frames: number;
  discardedFrames: number;
  durationMs: number;
  avgFps: N;
  currentFps: N;
  low1pctFps: N;
  low01pctFps: N;
  avgFrametimeMs: N;
  p50FrametimeMs: N;
  p95FrametimeMs: N;
  p99FrametimeMs: N;
  maxFrametimeMs: N;
  stddevFrametimeMs: N;
  pacingVariationMs: N;
  stutterCount: number;
  stutterTimePct: N;
}

export interface ContextPoint {
  t: number;
  cpuPct: N;
  gpuPct: N;
  gpuTempC: N;
  cpuTempC: N;
  vramBytes: N;
  ramUsedBytes: N;
  processCpuPct: N;
  processGpuPct: N;
}

export interface GamingLive {
  provider: string;
  providerStatus: Status;
  capturing: boolean;
  pid: N;
  processName: string | null;
  exePath: string | null;
  startedMs: N;
  durationSecs: number;
  frames: number;
  otherSwapChainFrames: number;
  lostFrames: number;
  runtime: string | null;
  presentMode: string | null;
  live: FrameStats;
  recentFrametimes: number[];
  context: ContextPoint | null;
  lastSavedSession: N;
}

export interface Agg {
  avg: number;
  max: number;
  samples: number;
}

export interface SessionRow {
  id: number;
  processName: string;
  exePath: string | null;
  pid: number;
  startedMs: number;
  endedMs: number;
  provider: string;
  stats: FrameStats & { runtime?: string; presentMode?: string; swapChain?: string; otherSwapChainFrames?: number; stutterFactor?: number };
  context: {
    cpuPct: Agg | null;
    gpuPct: Agg | null;
    gpuTempC: Agg | null;
    cpuTempC: Agg | null;
    vramBytes: Agg | null;
    ramUsedBytes: Agg | null;
    processCpuPct: Agg | null;
    processGpuPct: Agg | null;
    timeline: ContextPoint[];
  };
}

export interface SessionDetail {
  session: SessionRow;
  frametimes: number[];
}

export interface Candidate {
  pid: number;
  name: string;
  path: string | null;
  gpuPct: N;
  gpuDedicatedBytes: N;
  foreground: boolean;
}

export interface ChannelStatus {
  channel: string;
  status: Status;
  storedEvents: number;
}

export interface RuntimeStatus {
  version: string;
  elevated: boolean;
  dataDir: string;
  logDir: string;
  toolsDir: string;
  dbError: string | null;
  dbStats: { fileBytes: N; rawRows: number; minuteRows: number; events: number; sessions: number; alerts: number } | null;
  droppedWriteBatches: number;
  eventChannels: ChannelStatus[];
  startedMs: number;
}

export interface TreeResult {
  ended: number[];
  failed: [number, string, string][];
}

export interface UpdateStatus {
  currentVersion: string;
  available: { version: string; notes: string | null; date: string | null } | null;
  lastCheckMs: N;
  lastError: string | null;
  installing: boolean;
}

export interface SpecItem {
  label: string;
  value: string;
}

export interface SpecSection {
  title: string;
  items: SpecItem[];
}

export interface CaptureInterface {
  ifIndex: number;
  name: string;
  description: string;
  media: string;
  connected: boolean;
  rateBps: number | null;
}

export interface PacketRow {
  id: number;
  timeUs: number;
  ifIndex: number;
  outbound: boolean | null;
  src: string;
  dst: string;
  protocol: string;
  length: number;
  info: string;
}

export interface PacketPage {
  rows: PacketRow[];
  lastId: number;
  more: boolean;
  oldestId: number;
  total: number;
  evicted: number;
  running: boolean;
  interface: number | null;
  error: string | null;
}

export interface PacketField {
  key?: string;
  label: string;
  value: string;
  offset: number;
  len: number;
}

export interface PacketLayer {
  proto: string;
  name: string;
  summary: string;
  offset: number;
  len: number;
  fields: PacketField[];
}

export interface PacketDetail {
  id: number;
  timeUtc: string;
  timeUs: number;
  ifIndex: number;
  outbound: boolean | null;
  layers: PacketLayer[];
  bytes: number[];
}
