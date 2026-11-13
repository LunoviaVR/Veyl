/// Every command in `generate_handler!`. Listing them turns on Tauri's ACL for app commands:
/// a window can then call only what its capability file allows (`allow-<command-name>`),
/// and a command missing here is rejected everywhere. `commands::tests` keeps this in sync.
const COMMANDS: &[&str] = &[
    "get_system_info",
    "get_pc_specs",
    "get_latest",
    "set_ui_visible",
    "get_runtime_status",
    "restart_as_admin",
    "get_boot_status",
    "restart_into",
    "cancel_safe_mode",
    "open_app_folder",
    "get_update_status",
    "check_for_update",
    "install_update",
    "get_processes",
    "get_process_details",
    "end_process",
    "end_process_tree",
    "set_process_priority",
    "reveal_path",
    "get_services",
    "get_startup_items",
    "get_connections",
    "get_sensors",
    "reset_sensor_stats",
    "get_drive_health",
    "refresh_drive_health",
    "install_pawnio",
    "get_stability_events",
    "get_event_channels",
    "get_event_detail",
    "get_bsod_overview",
    "analyze_dump",
    "install_debugger",
    "get_diagnostics",
    "get_timeline",
    "get_gaming_status",
    "get_game_candidates",
    "start_capture",
    "stop_capture",
    "get_game_sessions",
    "get_game_session",
    "delete_game_session",
    "set_overlay_enabled",
    "set_auto_capture_games",
    "set_overlay_style",
    "set_overlay_position",
    "get_capture_interfaces",
    "start_packet_capture",
    "stop_packet_capture",
    "clear_packets",
    "get_packets",
    "get_packet_detail",
    "check_packet_filter",
    "get_metric_catalog",
    "get_history",
    "get_alert_history",
    "get_active_alerts",
    "clear_active_alerts",
    "get_settings",
    "save_settings",
    "reset_alert_rules",
    "export_history_csv",
    "export_pc_specs",
    "export_session_csv",
    "export_events_csv",
    "export_report",
];

fn main() {
    // Release builds start elevated (UAC prompt) so frame capture, kernel dumps and
    // protected-process details work without a restart. Debug builds stay asInvoker:
    // an exe that requires elevation cannot be launched by `tauri dev` (OS error 740).
    println!("cargo:rerun-if-changed=manifests/admin.xml");
    let release = std::env::var("PROFILE").is_ok_and(|p| p == "release");
    let mut windows = tauri_build::WindowsAttributes::new();
    if release {
        // The Windows (UAC) manifest, not the command ACL below.
        windows = windows.app_manifest(include_str!("manifests/admin.xml"));
    }
    let attrs = tauri_build::Attributes::new()
        .windows_attributes(windows)
        .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS));
    tauri_build::try_build(attrs).expect("tauri build script failed");
}
