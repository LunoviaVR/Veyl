//! Shared application state managed by Tauri.
//!
//! Lock discipline: locks are held only to read or swap values, never while
//! sampling, doing I/O or calling Windows. Samplers live on their own threads
//! and publish finished snapshots (`Arc`) into `Live`.

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::SyncSender;
use std::sync::Arc;

use parking_lot::{Mutex, RwLock};
use pch_core::alerts::AlertSignal;
use pch_core::{DriveHealth, ProcessTable, SensorsView, Status, SystemInfo, TelemetryFrame};
use pch_storage::Db;
use serde::Serialize;

use crate::gaming::{GamingCommand, GamingLive};
use crate::settings::Settings;
use crate::writer::WriteMsg;

#[derive(Clone, Debug)]
pub struct Paths {
    pub data: PathBuf,
    pub db: PathBuf,
    pub settings: PathBuf,
    pub logs: PathBuf,
    pub tools: PathBuf,
    pub symbols: PathBuf,
}

#[derive(Default)]
pub struct Live {
    pub frame: Option<Arc<TelemetryFrame>>,
    pub processes: Arc<ProcessTable>,
    pub sensors: Arc<SensorsView>,
    pub drives: Arc<Vec<DriveHealth>>,
    pub drives_updated_ms: Option<i64>,
    pub active_alerts: Vec<AlertSignal>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ChannelStatus {
    pub channel: String,
    pub status: Status,
    pub stored_events: usize,
}

pub struct AppState {
    pub paths: Paths,
    pub settings: RwLock<Settings>,
    pub live: RwLock<Live>,
    pub system_info: RwLock<SystemInfo>,
    /// Read connection for UI queries; `None` if the database could not be opened.
    pub reader: Mutex<Option<Db>>,
    pub db_error: RwLock<Option<String>>,
    pub writer: SyncSender<WriteMsg>,
    pub gaming_tx: SyncSender<GamingCommand>,
    pub gaming: RwLock<GamingLive>,
    pub event_channels: RwLock<Vec<ChannelStatus>>,
    pub ui_visible: AtomicBool,
    pub overlay_open: AtomicBool,
    pub reset_sensor_stats: AtomicBool,
    pub refresh_drives: AtomicBool,
    pub shutdown: Arc<AtomicBool>,
    pub threads: Mutex<Vec<(&'static str, std::thread::JoinHandle<()>)>>,
    pub started_ms: i64,
}

impl AppState {
    /// Runs `f` against the read connection, or explains why history is unavailable.
    pub fn with_db<T>(&self, f: impl FnOnce(&Db) -> pch_storage::Result<T>) -> Result<T, String> {
        let guard = self.reader.lock();
        match guard.as_ref() {
            Some(db) => f(db).map_err(|e| e.to_string()),
            None => Err(self
                .db_error
                .read()
                .clone()
                .unwrap_or_else(|| "History database is not available".into())),
        }
    }

    pub fn settings(&self) -> Settings {
        self.settings.read().clone()
    }
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
