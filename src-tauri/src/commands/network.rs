use serde::Serialize;
use std::sync::Arc;
use tauri::State;

use veyl_core::packet::{dissect, Layer, LinkType};
use veyl_core::packet_filter;
use veyl_core::time::{filetime_to_unix_ms, format_iso8601_utc_ms};
use veyl_windows::packet_capture::session_losses;

use super::CmdResult;
use crate::packets::{Capture, PacketRow, PAGE};
use crate::state::AppState;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureInterface {
    pub if_index: u32,
    pub name: String,
    pub description: String,
    pub media: String,
    pub connected: bool,
    /// Current receive + send rate, bytes/s.
    pub rate_bps: Option<f64>,
}

#[tauri::command]
pub fn get_capture_interfaces(state: State<'_, AppState>) -> Vec<CaptureInterface> {
    let live = state.live.read();
    let Some(frame) = live.frame.as_ref() else {
        return Vec::new();
    };
    frame
        .network
        .adapters
        .iter()
        .map(|a| CaptureInterface {
            if_index: a.if_index,
            name: a.alias.clone(),
            description: a.description.clone(),
            media: a.media.clone(),
            connected: a.connected,
            rate_bps: a.rx_bps.zip(a.tx_bps).map(|(r, t)| r + t),
        })
        .collect()
}

#[tauri::command(async)]
pub fn start_packet_capture(capture: State<'_, Capture>, if_index: Option<u32>) -> CmdResult<()> {
    capture.start(if_index)
}

#[tauri::command(async)]
pub fn stop_packet_capture(capture: State<'_, Capture>) {
    capture.stop();
}

#[tauri::command]
pub fn clear_packets(capture: State<'_, Capture>) {
    let mut s = capture.store.lock();
    let interface = s.interface;
    s.reset(interface);
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PacketPage {
    pub rows: Vec<PacketRow>,
    /// Id of the last packet examined; pass it as `afterId` next time.
    pub last_id: u64,
    /// More packets are waiting beyond this page.
    pub more: bool,
    /// Oldest packet still in memory (older rows can be dropped by the UI).
    pub oldest_id: u64,
    pub total: usize,
    pub evicted: u64,
    pub running: bool,
    /// Interface the current capture keeps (`None` = all).
    pub interface: Option<u32>,
    /// Events and buffers Windows dropped because the capture fell behind.
    pub lost_events: u64,
    pub lost_buffers: u64,
    pub error: Option<String>,
}

/// Packets after `after_id` that match `filter` (a display filter; empty = all).
#[tauri::command(async)]
pub fn get_packets(
    capture: State<'_, Capture>,
    after_id: u64,
    filter: String,
) -> CmdResult<PacketPage> {
    if filter.len() > 2_000 {
        return Err("The filter is too long".into());
    }
    let expr = packet_filter::parse(&filter)?;
    // Copy out what to examine and decode it after releasing the lock: decoding while
    // holding it stalls the capture thread, and Windows then drops packets.
    let candidates: Vec<(u64, Arc<[u8]>, LinkType, u32)> = {
        let s = capture.store.lock();
        let first = s.packets.front().map_or(0, |p| p.id);
        let skip = after_id.saturating_sub(first.saturating_sub(1)) as usize;
        s.packets
            .iter()
            .skip(skip)
            .map(|p| (p.id, p.data.clone(), p.link, p.orig_len))
            .collect()
    };
    let mut ids = Vec::new();
    let mut last_id = after_id;
    let mut more = false;
    for (id, data, link, len) in &candidates {
        if ids.len() >= PAGE {
            more = true;
            break;
        }
        last_id = *id;
        if expr
            .as_ref()
            .is_none_or(|e| packet_filter::matches(e, &dissect(data, *link, *len as usize)))
        {
            ids.push(*id);
        }
    }
    let running = capture.is_running();
    if running {
        if let Some(l) = session_losses() {
            *capture.lost.lock() = l;
        }
    }
    let (lost_events, lost_buffers) = *capture.lost.lock();
    let s = capture.store.lock();
    let rows = ids
        .iter()
        .filter_map(|id| s.get(*id))
        .map(|p| s.row(p))
        .collect();
    let first = s.packets.front().map_or(0, |p| p.id);
    Ok(PacketPage {
        rows,
        last_id,
        more,
        oldest_id: first,
        total: s.packets.len(),
        evicted: s.evicted,
        running,
        interface: s.interface,
        lost_events,
        lost_buffers,
        error: capture.error.lock().clone(),
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PacketDetail {
    pub id: u64,
    pub time_utc: String,
    pub time_us: i64,
    pub if_index: u32,
    pub outbound: Option<bool>,
    pub layers: Vec<Layer>,
    pub bytes: Vec<u8>,
}

#[tauri::command(async)]
pub fn get_packet_detail(capture: State<'_, Capture>, id: u64) -> CmdResult<PacketDetail> {
    let s = capture.store.lock();
    let p = s.get(id).ok_or("This packet is no longer in memory")?;
    let d = dissect(&p.data, p.link, p.orig_len as usize);
    let unix_ms = filetime_to_unix_ms(p.time_100ns as u64).unwrap_or(0);
    Ok(PacketDetail {
        id,
        time_utc: format_iso8601_utc_ms(unix_ms),
        time_us: s.row(p).time_us,
        if_index: p.if_index,
        outbound: p.outbound,
        layers: d.layers,
        bytes: p.data.to_vec(),
    })
}

/// Validates a display filter as the user types.
#[tauri::command]
pub fn check_packet_filter(filter: String) -> CmdResult<()> {
    if filter.len() > 2_000 {
        return Err("The filter is too long".into());
    }
    packet_filter::parse(&filter).map(|_| ())
}
