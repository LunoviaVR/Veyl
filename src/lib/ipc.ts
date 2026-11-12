// Typed wrappers for every Rust command. The UI never calls `invoke` directly.
import { invoke } from "@tauri-apps/api/core";
import type * as T from "./types";

const call = <R>(cmd: string, args?: Record<string, unknown>) => invoke<R>(cmd, args);

export const api = {
  systemInfo: () => call<T.SystemInfo>("get_system_info"),
  latest: () => call<T.TelemetryFrame | null>("get_latest"),
  setUiVisible: (visible: boolean) => call<void>("set_ui_visible", { visible }),
  runtimeStatus: () => call<T.RuntimeStatus>("get_runtime_status"),
  restartAsAdmin: () => call<void>("restart_as_admin"),
  bootStatus: () => call<T.BootStatus>("get_boot_status"),
  restartInto: (target: T.RestartTarget) => call<void>("restart_into", { target }),
  cancelSafeMode: () => call<void>("cancel_safe_mode"),
  openAppFolder: (which: "data" | "logs" | "tools") => call<void>("open_app_folder", { which }),
  updateStatus: () => call<T.UpdateStatus>("get_update_status"),
  checkForUpdate: () => call<T.UpdateStatus>("check_for_update"),
  installUpdate: () => call<void>("install_update"),

  processes: () => call<T.ProcessTable>("get_processes"),
  processDetails: (pid: number, name: string) => call<T.ProcessDetails>("get_process_details", { pid, name }),
  endProcess: (pid: number, name: string, startTimeMs: number | null) => call<void>("end_process", { pid, name, startTimeMs }),
  endProcessTree: (pid: number, startTimeMs: number | null) => call<T.TreeResult>("end_process_tree", { pid, startTimeMs }),
  setPriority: (pid: number, name: string, startTimeMs: number | null, priority: T.Priority) =>
    call<void>("set_process_priority", { pid, name, startTimeMs, priority }),
  revealPath: (path: string) => call<void>("reveal_path", { path }),
  services: () => call<T.ServiceInfo[]>("get_services"),
  startupItems: () => call<T.StartupItem[]>("get_startup_items"),
  connections: () => call<T.ConnectionRow[]>("get_connections"),

  sensors: () => call<T.SensorsView>("get_sensors"),
  resetSensorStats: () => call<void>("reset_sensor_stats"),
  driveHealth: () => call<{ drives: T.DriveHealth[]; updatedMs: number | null }>("get_drive_health"),
  refreshDriveHealth: () => call<void>("refresh_drive_health"),
  installPawnio: () => call<void>("install_pawnio"),

  stabilityEvents: (fromMs: number, toMs: number, limit?: number) => call<T.StabilityEvent[]>("get_stability_events", { fromMs, toMs, limit }),
  eventChannels: () => call<T.ChannelStatus[]>("get_event_channels"),
  eventDetail: (channel: string, recordId: number) => call<T.EventDetailView>("get_event_detail", { channel, recordId }),
  bsodOverview: () => call<T.BsodOverview>("get_bsod_overview"),
  analyzeDump: (path: string) => call<T.DumpAnalysis>("analyze_dump", { path }),
  installDebugger: () => call<void>("install_debugger"),
  diagnostics: () => call<T.Finding[]>("get_diagnostics"),
  timeline: (centerMs: number, halfWindowMs: number) => call<T.TimelineView>("get_timeline", { centerMs, halfWindowMs }),

  gamingStatus: () => call<T.GamingLive>("get_gaming_status"),
  gameCandidates: () => call<T.Candidate[]>("get_game_candidates"),
  startCapture: (pid: number) => call<void>("start_capture", { pid }),
  stopCapture: () => call<void>("stop_capture"),
  gameSessions: (limit?: number) => call<T.SessionRow[]>("get_game_sessions", { limit }),
  gameSession: (id: number) => call<T.SessionDetail>("get_game_session", { id }),
  deleteGameSession: (id: number) => call<void>("delete_game_session", { id }),
  setOverlayEnabled: (enabled: boolean) => call<void>("set_overlay_enabled", { enabled }),
  setAutoCaptureGames: (enabled: boolean) => call<void>("set_auto_capture_games", { enabled }),
  setOverlayStyle: (design: string, theme: string) => call<void>("set_overlay_style", { design, theme }),
  setOverlayPosition: (position: string) => call<void>("set_overlay_position", { position }),

  metricCatalog: () => call<T.MetricInfo[]>("get_metric_catalog"),
  history: (keys: string[], fromMs: number, toMs: number, maxPoints?: number) => call<T.Series[]>("get_history", { keys, fromMs, toMs, maxPoints }),
  alertHistory: (limit?: number) => call<T.AlertRow[]>("get_alert_history", { limit }),
  activeAlerts: () => call<T.AlertSignal[]>("get_active_alerts"),
  pcSpecs: () => call<T.SpecSection[]>("get_pc_specs"),
  exportPcSpecs: (format: "txt" | "csv") => call<string | null>("export_pc_specs", { format }),
  clearActiveAlerts: () => call<void>("clear_active_alerts"),

  captureInterfaces: () => call<T.CaptureInterface[]>("get_capture_interfaces"),
  startPacketCapture: (ifIndex: number | null) => call<void>("start_packet_capture", { ifIndex }),
  stopPacketCapture: () => call<void>("stop_packet_capture"),
  clearPackets: () => call<void>("clear_packets"),
  packets: (afterId: number, filter: string) => call<T.PacketPage>("get_packets", { afterId, filter }),
  packetDetail: (id: number) => call<T.PacketDetail>("get_packet_detail", { id }),
  checkPacketFilter: (filter: string) => call<void>("check_packet_filter", { filter }),

  settings: () => call<T.Settings>("get_settings"),
  saveSettings: (settings: T.Settings) => call<T.Settings>("save_settings", { settings }),
  resetAlertRules: () => call<T.Settings>("reset_alert_rules"),

  exportHistoryCsv: (keys: string[], fromMs: number, toMs: number) => call<string | null>("export_history_csv", { keys, fromMs, toMs }),
  exportSessionCsv: (id: number) => call<string | null>("export_session_csv", { id }),
  exportEventsCsv: (fromMs: number, toMs: number, redact: boolean) => call<string | null>("export_events_csv", { fromMs, toMs, redact }),
  exportReport: (redactPersonal: boolean) => call<string | null>("export_report", { redactPersonal }),
};

/** Error text from a rejected command (Rust returns plain strings). */
export function errorText(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return JSON.stringify(e);
}
