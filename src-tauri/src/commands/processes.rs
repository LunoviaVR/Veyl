use std::path::PathBuf;
use std::sync::Arc;

use pch_core::ProcessTable;
use pch_windows::process_control::{self, ControlError, Priority, ProcessDetails};
use pch_windows::{connections, services, startup};
use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

use super::{blocking, CmdResult};
use crate::state::AppState;

fn check_name(name: &str) -> Result<(), ControlError> {
    if name.len() > 260 {
        return Err(ControlError::Refused("Invalid process name".into()));
    }
    Ok(())
}

#[tauri::command]
pub fn get_processes(state: State<'_, AppState>) -> Arc<ProcessTable> {
    state.live.read().processes.clone()
}

#[tauri::command(async)]
pub fn get_process_details(pid: u32, name: String) -> Result<ProcessDetails, ControlError> {
    check_name(&name)?;
    Ok(process_control::details(pid, &name))
}

#[tauri::command(async)]
pub fn end_process(pid: u32, name: String, start_time_ms: Option<i64>) -> Result<(), ControlError> {
    check_name(&name)?;
    tracing::info!("user requested end process {name} ({pid})");
    process_control::end_process(pid, &name, start_time_ms)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TreeResult {
    pub ended: Vec<u32>,
    pub failed: Vec<(u32, String, String)>,
}

/// Ends a process and its descendants (deepest first). Children are found from
/// the latest snapshot; a child counts only if it started after its parent, so
/// a reused parent PID never adopts unrelated processes.
#[tauri::command(async)]
pub fn end_process_tree(
    state: State<'_, AppState>,
    pid: u32,
    start_time_ms: Option<i64>,
) -> Result<TreeResult, ControlError> {
    let table = state.live.read().processes.clone();
    let root = table
        .rows
        .iter()
        .find(|r| r.pid == pid && r.start_time_ms == start_time_ms)
        .ok_or(ControlError::PidReused(pid))?;
    if let Some(r) = process_control::refusal_reason(root.pid, &root.name) {
        return Err(ControlError::Refused(r));
    }
    let mut order = vec![root.clone()];
    let mut i = 0;
    while i < order.len() {
        let parent = order[i].clone();
        for c in table.rows.iter().filter(|c| {
            c.parent_pid == parent.pid && c.pid != parent.pid && matches!((c.start_time_ms, parent.start_time_ms), (Some(cs), Some(ps)) if cs >= ps)
        }) {
            if !order.iter().any(|o| o.pid == c.pid) {
                order.push(c.clone());
            }
        }
        i += 1;
    }
    let mut result = TreeResult {
        ended: vec![],
        failed: vec![],
    };
    for p in order.iter().rev() {
        match process_control::end_process(p.pid, &p.name, p.start_time_ms) {
            Ok(()) => result.ended.push(p.pid),
            Err(e) => result.failed.push((p.pid, p.name.clone(), e.to_string())),
        }
    }
    tracing::info!(
        "end process tree {pid}: ended {:?}, failed {}",
        result.ended,
        result.failed.len()
    );
    Ok(result)
}

#[tauri::command(async)]
pub fn set_process_priority(
    pid: u32,
    name: String,
    start_time_ms: Option<i64>,
    priority: Priority,
) -> Result<(), ControlError> {
    check_name(&name)?;
    process_control::set_priority(pid, &name, start_time_ms, priority)
}

/// Opens Explorer with the file selected. Only absolute paths to existing files are accepted.
#[tauri::command]
pub fn reveal_path(app: AppHandle, path: String) -> CmdResult<()> {
    let p = PathBuf::from(&path);
    if path.len() > 32_767 || !p.is_absolute() || !p.exists() {
        return Err("The file no longer exists".into());
    }
    app.opener()
        .reveal_item_in_dir(&p)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_services() -> CmdResult<Vec<services::ServiceInfo>> {
    blocking(services::list).await?
}

#[tauri::command]
pub async fn get_startup_items() -> CmdResult<Vec<startup::StartupItem>> {
    blocking(startup::list).await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionRow {
    #[serde(flatten)]
    pub conn: connections::Connection,
    pub process_name: Option<String>,
}

#[tauri::command]
pub async fn get_connections(state: State<'_, AppState>) -> CmdResult<Vec<ConnectionRow>> {
    let table = state.live.read().processes.clone();
    let list = blocking(connections::list).await?;
    Ok(list
        .into_iter()
        .map(|c| ConnectionRow {
            process_name: table
                .rows
                .iter()
                .find(|r| r.pid == c.pid)
                .map(|r| r.name.clone()),
            conn: c,
        })
        .collect())
}
