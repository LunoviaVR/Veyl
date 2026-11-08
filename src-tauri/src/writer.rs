//! The only thread that writes the database. Producers send messages over a
//! bounded channel with `try_send`, so a slow disk can never stall sampling:
//! when the queue is full the batch is dropped and counted, never blocked on.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, SyncSender, TrySendError};
use std::sync::Arc;
use std::time::{Duration, Instant};

use pch_core::aggregate::BucketValue;
use pch_core::events::RawEvent;
use pch_storage::{AlertRow, Db, NewSession, Retention};

pub enum WriteMsg {
    Samples(Vec<(i64, String, String, BucketValue)>),
    Events(Vec<RawEvent>, std::sync::mpsc::Sender<Vec<RawEvent>>),
    Session(
        Box<NewSession>,
        std::sync::mpsc::Sender<Result<i64, String>>,
    ),
    Alert(AlertRow),
    DeleteSession(i64),
    Retention(Retention),
}

pub const QUEUE_LEN: usize = 256;
const ROLLUP_EVERY: Duration = Duration::from_secs(60);
const RETENTION_EVERY: Duration = Duration::from_secs(3600);

/// Dropped write batches since start (exposed in diagnostics/overhead view).
pub static DROPPED_BATCHES: AtomicU64 = AtomicU64::new(0);

/// Sends without blocking; a full queue drops the message and counts it.
pub fn send(tx: &SyncSender<WriteMsg>, msg: WriteMsg) {
    match tx.try_send(msg) {
        Ok(()) => {}
        Err(TrySendError::Full(_)) => {
            DROPPED_BATCHES.fetch_add(1, Ordering::Relaxed);
            tracing::warn!("database write queue full; batch dropped");
        }
        Err(TrySendError::Disconnected(_)) => {}
    }
}

fn handle(db: &mut Db, msg: WriteMsg, retention: &mut Retention) {
    match msg {
        WriteMsg::Samples(s) => {
            if let Err(e) = db.write_samples(&s) {
                tracing::error!("writing samples failed: {e}");
            }
        }
        WriteMsg::Events(evs, reply) => {
            let fresh = db.insert_events(&evs).unwrap_or_else(|e| {
                tracing::error!("writing events failed: {e}");
                Vec::new()
            });
            let _ = reply.send(fresh); // the requester may have stopped waiting
        }
        WriteMsg::Session(s, reply) => {
            let _ = reply.send(db.insert_session(&s).map_err(|e| e.to_string()));
        }
        WriteMsg::Alert(a) => {
            if let Err(e) = db.insert_alert(&a) {
                tracing::error!("writing alert failed: {e}");
            }
        }
        WriteMsg::DeleteSession(id) => {
            if let Err(e) = db.delete_session(id) {
                tracing::error!("deleting session failed: {e}");
            }
        }
        WriteMsg::Retention(r) => *retention = r,
    }
}

pub fn run(
    mut db: Db,
    rx: Receiver<WriteMsg>,
    mut retention: Retention,
    shutdown: Arc<std::sync::atomic::AtomicBool>,
) {
    let mut last_rollup = Instant::now();
    // First retention pass two minutes after start, then hourly.
    let mut last_retention = Instant::now()
        .checked_sub(RETENTION_EVERY - Duration::from_secs(120))
        .unwrap_or_else(Instant::now);
    let mut rollup_from: i64 = db
        .kv_get("rollup_until")
        .ok()
        .flatten()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    loop {
        match rx.recv_timeout(Duration::from_secs(2)) {
            Ok(msg) => handle(&mut db, msg, &mut retention),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
        let now = crate::state::now_ms();
        if last_rollup.elapsed() >= ROLLUP_EVERY {
            last_rollup = Instant::now();
            // Roll up minutes that ended at least one raw bucket ago.
            let until = now - 10_000;
            let from = if rollup_from == 0 {
                until - 48 * 3_600_000
            } else {
                rollup_from - pch_storage::MINUTE_MS
            };
            match db.rollup(from, until) {
                Ok(_) => {
                    rollup_from = until.div_euclid(pch_storage::MINUTE_MS) * pch_storage::MINUTE_MS;
                    if let Err(e) = db.kv_set("rollup_until", &rollup_from.to_string()) {
                        tracing::warn!("saving rollup position failed: {e}");
                    }
                }
                Err(e) => tracing::error!("rollup failed: {e}"),
            }
        }
        if last_retention.elapsed() >= RETENTION_EVERY {
            last_retention = Instant::now();
            match db.apply_retention(now, &retention) {
                Ok(n) if n > 0 => tracing::info!("retention removed {n} rows"),
                Ok(_) => {}
                Err(e) => tracing::error!("retention failed: {e}"),
            }
            if let Err(e) = db.checkpoint() {
                tracing::warn!("checkpoint failed: {e}");
            }
        }
        if shutdown.load(Ordering::Relaxed) {
            while let Ok(msg) = rx.try_recv() {
                handle(&mut db, msg, &mut retention);
            }
            break;
        }
    }
    // Final rollup so the last minutes reach long-term history.
    let now = crate::state::now_ms();
    if let Err(e) = db.rollup(rollup_from.max(now - 3_600_000), now) {
        tracing::warn!("final rollup failed: {e}");
    }
    tracing::info!("database writer stopped");
}
