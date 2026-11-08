//! Local SQLite storage: telemetry history, Windows stability events, game
//! sessions and alert history.
//!
//! - Schema changes are numbered migrations tracked in `PRAGMA user_version`.
//! - Telemetry is written as pre-aggregated buckets (avg/min/max) in batched
//!   transactions; complete minutes are rolled up into a long-term table, and
//!   both tables are pruned by configurable retention.
//! - Events are deduplicated on (channel, record id, time) and stored raw;
//!   interpretation happens in `pch-core` when they are read, so improving a
//!   classifier never requires a data migration.

use std::path::Path;

use pch_core::aggregate::BucketValue;
use pch_core::events::RawEvent;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

#[derive(thiserror::Error, Debug)]
pub enum StorageError {
    #[error("database error: {0}")]
    Sql(#[from] rusqlite::Error),
    #[error("serialization error: {0}")]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, StorageError>;

/// Ordered schema migrations. Never edit an applied entry; append new ones.
const MIGRATIONS: &[&str] = &[
    // 1: initial schema
    r"
    CREATE TABLE metric (
        id    INTEGER PRIMARY KEY,
        key   TEXT NOT NULL UNIQUE,
        unit  TEXT NOT NULL
    );
    CREATE TABLE sample_raw (
        metric_id INTEGER NOT NULL REFERENCES metric(id),
        ts        INTEGER NOT NULL,
        avg REAL NOT NULL, min REAL NOT NULL, max REAL NOT NULL,
        PRIMARY KEY (metric_id, ts)
    ) WITHOUT ROWID;
    CREATE INDEX sample_raw_ts ON sample_raw(ts);
    CREATE TABLE sample_1m (
        metric_id INTEGER NOT NULL REFERENCES metric(id),
        ts        INTEGER NOT NULL,
        avg REAL NOT NULL, min REAL NOT NULL, max REAL NOT NULL,
        PRIMARY KEY (metric_id, ts)
    ) WITHOUT ROWID;
    CREATE INDEX sample_1m_ts ON sample_1m(ts);
    CREATE TABLE stability_event (
        id         INTEGER PRIMARY KEY,
        channel    TEXT NOT NULL,
        record_id  INTEGER NOT NULL,
        time_ms    INTEGER NOT NULL,
        provider   TEXT NOT NULL,
        event_id   INTEGER NOT NULL,
        level      INTEGER NOT NULL,
        data_json  TEXT NOT NULL,
        UNIQUE (channel, record_id, time_ms)
    );
    CREATE INDEX stability_event_time ON stability_event(time_ms);
    CREATE TABLE game_session (
        id            INTEGER PRIMARY KEY,
        process_name  TEXT NOT NULL,
        exe_path      TEXT,
        pid           INTEGER NOT NULL,
        started_ms    INTEGER NOT NULL,
        ended_ms      INTEGER NOT NULL,
        provider      TEXT NOT NULL,
        stats_json    TEXT NOT NULL,
        context_json  TEXT NOT NULL,
        frametimes    BLOB NOT NULL
    );
    CREATE INDEX game_session_started ON game_session(started_ms);
    CREATE TABLE alert_event (
        id         INTEGER PRIMARY KEY,
        time_ms    INTEGER NOT NULL,
        rule_id    TEXT NOT NULL,
        rule_name  TEXT NOT NULL,
        metric     TEXT NOT NULL,
        severity   TEXT NOT NULL,
        transition TEXT NOT NULL,
        value      REAL NOT NULL,
        threshold  REAL NOT NULL,
        since_ms   INTEGER NOT NULL
    );
    CREATE INDEX alert_event_time ON alert_event(time_ms);
    CREATE TABLE kv (key TEXT PRIMARY KEY, value TEXT NOT NULL);
    ",
    // 2: drop stored copies of informational storage notices (Ntfs 98 "volume is
    // healthy"), which 0.1.0 ingested by ID alone; only this app's copies are removed.
    r"
    DELETE FROM stability_event
    WHERE provider IN ('disk', 'Ntfs', 'Microsoft-Windows-Ntfs', 'stornvme', 'storahci')
      AND level NOT IN (1, 2, 3);
    ",
];

pub struct Db {
    conn: Connection,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Series {
    pub key: String,
    pub unit: String,
    pub bucket_ms: i64,
    pub source: &'static str,
    /// Timestamps; a `None` in the value arrays marks a gap with no recorded data.
    pub t: Vec<i64>,
    pub avg: Vec<Option<f64>>,
    pub min: Vec<Option<f64>>,
    pub max: Vec<Option<f64>>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MetricInfo {
    pub key: String,
    pub unit: String,
    pub first_ms: Option<i64>,
    pub last_ms: Option<i64>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Retention {
    pub raw_hours: u32,
    pub minute_days: u32,
    pub event_days: u32,
    pub session_days: u32,
}

impl Default for Retention {
    fn default() -> Self {
        Self {
            raw_hours: 48,
            minute_days: 30,
            event_days: 365,
            session_days: 365,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NewSession {
    pub process_name: String,
    pub exe_path: Option<String>,
    pub pid: u32,
    pub started_ms: i64,
    pub ended_ms: i64,
    pub provider: String,
    pub stats: serde_json::Value,
    pub context: serde_json::Value,
    pub frametimes_ms: Vec<f32>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionRow {
    pub id: i64,
    pub process_name: String,
    pub exe_path: Option<String>,
    pub pid: u32,
    pub started_ms: i64,
    pub ended_ms: i64,
    pub provider: String,
    pub stats: serde_json::Value,
    pub context: serde_json::Value,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AlertRow {
    pub id: i64,
    pub time_ms: i64,
    pub rule_id: String,
    pub rule_name: String,
    pub metric: String,
    pub severity: String,
    pub transition: String,
    pub value: f64,
    pub threshold: f64,
    pub since_ms: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct DbStats {
    pub file_bytes: Option<u64>,
    pub raw_rows: i64,
    pub minute_rows: i64,
    pub events: i64,
    pub sessions: i64,
    pub alerts: i64,
}

/// A metric key with its `(bucket start ms, avg, max)` points.
pub type KeyPoints = (String, Vec<(i64, f64, f64)>);

/// Width of long-term rollup buckets.
pub const MINUTE_MS: i64 = 60_000;
/// Ranges up to this length are served from raw buckets when available.
const RAW_QUERY_MAX_SPAN_MS: i64 = 6 * 3_600_000;

impl Db {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        Self::init(conn)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        // WAL lets the UI read while the writer thread commits; NORMAL sync is durable across app crashes.
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        let mut db = Self { conn };
        db.migrate()?;
        Ok(db)
    }

    pub fn schema_version(&self) -> Result<u32> {
        Ok(self
            .conn
            .pragma_query_value(None, "user_version", |r| r.get(0))?)
    }

    fn migrate(&mut self) -> Result<()> {
        let current = self.schema_version()? as usize;
        for (i, sql) in MIGRATIONS.iter().enumerate().skip(current) {
            let tx = self.conn.transaction()?;
            tx.execute_batch(sql)?;
            tx.pragma_update(None, "user_version", (i + 1) as u32)?;
            tx.commit()?;
            tracing::info!("database migrated to schema version {}", i + 1);
        }
        Ok(())
    }

    fn metric_id(tx: &rusqlite::Transaction<'_>, key: &str, unit: &str) -> Result<i64> {
        tx.execute("INSERT INTO metric(key, unit) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET unit = excluded.unit", params![key, unit])?;
        Ok(tx.query_row("SELECT id FROM metric WHERE key = ?1", [key], |r| r.get(0))?)
    }

    /// Writes completed telemetry buckets in one transaction. Re-writing a bucket replaces it.
    pub fn write_samples(
        &mut self,
        samples: &[(i64, String, String, BucketValue)],
    ) -> Result<usize> {
        let tx = self.conn.transaction()?;
        let mut ids: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
        {
            let mut stmt = tx.prepare_cached("INSERT OR REPLACE INTO sample_raw(metric_id, ts, avg, min, max) VALUES (?1, ?2, ?3, ?4, ?5)")?;
            for (ts, key, unit, v) in samples {
                let id = match ids.get(key) {
                    Some(id) => *id,
                    None => {
                        let id = Self::metric_id(&tx, key, unit)?;
                        ids.insert(key.clone(), id);
                        id
                    }
                };
                stmt.execute(params![id, ts, v.avg, v.min, v.max])?;
            }
        }
        tx.commit()?;
        Ok(samples.len())
    }

    /// Rolls complete minutes `[from_ms, until_ms)` of raw buckets into `sample_1m`.
    pub fn rollup(&mut self, from_ms: i64, until_ms: i64) -> Result<usize> {
        let from = from_ms.div_euclid(MINUTE_MS) * MINUTE_MS;
        let until = until_ms.div_euclid(MINUTE_MS) * MINUTE_MS;
        if until <= from {
            return Ok(0);
        }
        let n = self.conn.execute(
            "INSERT OR REPLACE INTO sample_1m(metric_id, ts, avg, min, max)
             SELECT metric_id, (ts / ?3) * ?3, avg(avg), min(min), max(max)
             FROM sample_raw WHERE ts >= ?1 AND ts < ?2 GROUP BY metric_id, ts / ?3",
            params![from, until, MINUTE_MS],
        )?;
        Ok(n)
    }

    pub fn kv_get(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row("SELECT value FROM kv WHERE key = ?1", [key], |r| r.get(0))
            .optional()?)
    }

    pub fn kv_set(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO kv(key, value) VALUES (?1, ?2)",
            params![key, value],
        )?;
        Ok(())
    }

    /// Deletes data older than the retention windows. Returns rows removed.
    pub fn apply_retention(&mut self, now_ms: i64, r: &Retention) -> Result<usize> {
        let hour = 3_600_000i64;
        let day = 24 * hour;
        let tx = self.conn.transaction()?;
        let mut n = tx.execute(
            "DELETE FROM sample_raw WHERE ts < ?1",
            [now_ms - i64::from(r.raw_hours) * hour],
        )?;
        n += tx.execute(
            "DELETE FROM sample_1m WHERE ts < ?1",
            [now_ms - i64::from(r.minute_days) * day],
        )?;
        n += tx.execute(
            "DELETE FROM stability_event WHERE time_ms < ?1",
            [now_ms - i64::from(r.event_days) * day],
        )?;
        n += tx.execute(
            "DELETE FROM game_session WHERE started_ms < ?1",
            [now_ms - i64::from(r.session_days) * day],
        )?;
        n += tx.execute(
            "DELETE FROM alert_event WHERE time_ms < ?1",
            [now_ms - i64::from(r.event_days) * day],
        )?;
        tx.commit()?;
        Ok(n)
    }

    pub fn metric_catalog(&self) -> Result<Vec<MetricInfo>> {
        let mut stmt = self.conn.prepare(
            "SELECT m.key, m.unit,
                    (SELECT min(t) FROM (SELECT min(ts) AS t FROM sample_raw WHERE metric_id = m.id
                                         UNION ALL SELECT min(ts) FROM sample_1m WHERE metric_id = m.id)),
                    (SELECT max(t) FROM (SELECT max(ts) AS t FROM sample_raw WHERE metric_id = m.id
                                         UNION ALL SELECT max(ts) FROM sample_1m WHERE metric_id = m.id))
             FROM metric m ORDER BY m.key",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(MetricInfo {
                key: r.get(0)?,
                unit: r.get(1)?,
                first_ms: r.get(2)?,
                last_ms: r.get(3)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Time series for `keys` over `[from_ms, to_ms]`, downsampled to at most
    /// `max_points` buckets, with gaps marked where no data was recorded.
    pub fn series(
        &self,
        keys: &[String],
        from_ms: i64,
        to_ms: i64,
        max_points: u32,
        now_ms: i64,
        raw_hours: u32,
    ) -> Result<Vec<Series>> {
        let span = (to_ms - from_ms).max(1);
        let raw_available_from = now_ms - i64::from(raw_hours) * 3_600_000;
        let use_raw = span <= RAW_QUERY_MAX_SPAN_MS && from_ms >= raw_available_from;
        let (table, native) = if use_raw {
            ("sample_raw", 5_000)
        } else {
            ("sample_1m", MINUTE_MS)
        };
        let bucket = (span / i64::from(max_points.max(10))).max(native);
        let sql = format!(
            "SELECT (s.ts / ?4) * ?4 AS b, avg(s.avg), min(s.min), max(s.max), m.unit
             FROM {table} s JOIN metric m ON m.id = s.metric_id
             WHERE m.key = ?1 AND s.ts >= ?2 AND s.ts <= ?3 GROUP BY b ORDER BY b"
        );
        let mut stmt = self.conn.prepare_cached(&sql)?;
        let mut out = Vec::new();
        for key in keys {
            let mut unit = String::new();
            let mut pts: Vec<(i64, f64, f64, f64)> = Vec::new();
            let rows = stmt.query_map(params![key, from_ms, to_ms, bucket], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, f64>(1)?,
                    r.get::<_, f64>(2)?,
                    r.get::<_, f64>(3)?,
                    r.get::<_, String>(4)?,
                ))
            })?;
            for row in rows {
                let (t, a, mi, ma, u) = row?;
                unit = u;
                pts.push((t, a, mi, ma));
            }
            let mut s = Series {
                key: key.clone(),
                unit,
                bucket_ms: bucket,
                source: if use_raw { "raw" } else { "1m" },
                t: vec![],
                avg: vec![],
                min: vec![],
                max: vec![],
            };
            // A gap is any missing bucket beyond 2.5 intervals (sampling jitter tolerated).
            let max_gap = bucket * 5 / 2;
            for (i, (t, a, mi, ma)) in pts.iter().enumerate() {
                if i > 0 && t - pts[i - 1].0 > max_gap {
                    s.t.push(pts[i - 1].0 + bucket);
                    s.avg.push(None);
                    s.min.push(None);
                    s.max.push(None);
                }
                s.t.push(*t);
                s.avg.push(Some(*a));
                s.min.push(Some(*mi));
                s.max.push(Some(*ma));
            }
            out.push(s);
        }
        Ok(out)
    }

    /// One-minute `(ts, avg, max)` points for diagnostics, for keys matching a SQL LIKE pattern.
    pub fn minute_points(&self, like: &str, from_ms: i64) -> Result<Vec<KeyPoints>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT m.key, s.ts, s.avg, s.max FROM sample_1m s JOIN metric m ON m.id = s.metric_id
             WHERE m.key LIKE ?1 AND s.ts >= ?2 ORDER BY m.key, s.ts",
        )?;
        let rows = stmt.query_map(params![like, from_ms], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, f64>(2)?,
                r.get::<_, f64>(3)?,
            ))
        })?;
        let mut out: Vec<KeyPoints> = Vec::new();
        for row in rows {
            let (k, t, a, m) = row?;
            match out.last_mut() {
                Some((lk, v)) if *lk == k => v.push((t, a, m)),
                _ => out.push((k, vec![(t, a, m)])),
            }
        }
        Ok(out)
    }

    /// Inserts events not already stored. Returns the newly stored ones.
    pub fn insert_events(&mut self, events: &[RawEvent]) -> Result<Vec<RawEvent>> {
        let tx = self.conn.transaction()?;
        let mut fresh = Vec::new();
        {
            let mut stmt = tx.prepare_cached(
                "INSERT OR IGNORE INTO stability_event(channel, record_id, time_ms, provider, event_id, level, data_json)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            )?;
            for e in events {
                let json = serde_json::to_string(&e.data)?;
                if stmt.execute(params![
                    e.channel,
                    e.record_id as i64,
                    e.time_ms,
                    e.provider,
                    e.event_id,
                    e.level,
                    json
                ])? > 0
                {
                    fresh.push(e.clone());
                }
            }
        }
        tx.commit()?;
        Ok(fresh)
    }

    fn event_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<(RawEvent, String)> {
        Ok((
            RawEvent {
                channel: r.get(0)?,
                record_id: r.get::<_, i64>(1)? as u64,
                time_ms: r.get(2)?,
                provider: r.get(3)?,
                event_id: r.get(4)?,
                level: r.get(5)?,
                data: Vec::new(),
            },
            r.get(6)?,
        ))
    }

    fn decode(rows: Vec<(RawEvent, String)>) -> Vec<RawEvent> {
        rows.into_iter()
            .map(|(mut e, json)| {
                e.data = serde_json::from_str(&json).unwrap_or_default();
                e
            })
            .collect()
    }

    /// Events in `[from_ms, to_ms]`, newest first.
    pub fn events(&self, from_ms: i64, to_ms: i64, limit: u32) -> Result<Vec<RawEvent>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT channel, record_id, time_ms, provider, event_id, level, data_json FROM stability_event
             WHERE time_ms >= ?1 AND time_ms <= ?2 ORDER BY time_ms DESC, record_id DESC LIMIT ?3",
        )?;
        let rows = stmt
            .query_map(params![from_ms, to_ms, limit], Self::event_from_row)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(Self::decode(rows))
    }

    pub fn event(&self, channel: &str, record_id: u64) -> Result<Option<RawEvent>> {
        let row = self
            .conn
            .query_row(
                "SELECT channel, record_id, time_ms, provider, event_id, level, data_json FROM stability_event
                 WHERE channel = ?1 AND record_id = ?2 ORDER BY time_ms DESC LIMIT 1",
                params![channel, record_id as i64],
                Self::event_from_row,
            )
            .optional()?;
        Ok(row.map(|r| Self::decode(vec![r]).remove(0)))
    }

    pub fn insert_session(&mut self, s: &NewSession) -> Result<i64> {
        let blob: Vec<u8> = s
            .frametimes_ms
            .iter()
            .flat_map(|f| f.to_le_bytes())
            .collect();
        self.conn.execute(
            "INSERT INTO game_session(process_name, exe_path, pid, started_ms, ended_ms, provider, stats_json, context_json, frametimes)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![s.process_name, s.exe_path, s.pid, s.started_ms, s.ended_ms, s.provider, serde_json::to_string(&s.stats)?, serde_json::to_string(&s.context)?, blob],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    fn session_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<(SessionRow, String, String)> {
        Ok((
            SessionRow {
                id: r.get(0)?,
                process_name: r.get(1)?,
                exe_path: r.get(2)?,
                pid: r.get(3)?,
                started_ms: r.get(4)?,
                ended_ms: r.get(5)?,
                provider: r.get(6)?,
                stats: serde_json::Value::Null,
                context: serde_json::Value::Null,
            },
            r.get(7)?,
            r.get(8)?,
        ))
    }

    pub fn sessions(&self, limit: u32) -> Result<Vec<SessionRow>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT id, process_name, exe_path, pid, started_ms, ended_ms, provider, stats_json, context_json FROM game_session ORDER BY started_ms DESC LIMIT ?1",
        )?;
        let rows = stmt
            .query_map([limit], Self::session_from_row)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows
            .into_iter()
            .map(|(mut s, st, ctx)| {
                s.stats = serde_json::from_str(&st).unwrap_or_default();
                s.context = serde_json::from_str(&ctx).unwrap_or_default();
                s
            })
            .collect())
    }

    /// One session with its frame times.
    pub fn session(&self, id: i64) -> Result<Option<(SessionRow, Vec<f32>)>> {
        let row = self
            .conn
            .query_row(
                "SELECT id, process_name, exe_path, pid, started_ms, ended_ms, provider, stats_json, context_json, frametimes FROM game_session WHERE id = ?1",
                [id],
                |r| Ok((Self::session_from_row(r)?, r.get::<_, Vec<u8>>(9)?)),
            )
            .optional()?;
        Ok(row.map(|((mut s, st, ctx), blob)| {
            s.stats = serde_json::from_str(&st).unwrap_or_default();
            s.context = serde_json::from_str(&ctx).unwrap_or_default();
            let ft = blob
                .chunks_exact(4)
                .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                .collect();
            (s, ft)
        }))
    }

    pub fn delete_session(&mut self, id: i64) -> Result<bool> {
        Ok(self
            .conn
            .execute("DELETE FROM game_session WHERE id = ?1", [id])?
            > 0)
    }

    pub fn insert_alert(&mut self, a: &AlertRow) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO alert_event(time_ms, rule_id, rule_name, metric, severity, transition, value, threshold, since_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![a.time_ms, a.rule_id, a.rule_name, a.metric, a.severity, a.transition, a.value, a.threshold, a.since_ms],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn alerts(&self, limit: u32) -> Result<Vec<AlertRow>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT id, time_ms, rule_id, rule_name, metric, severity, transition, value, threshold, since_ms FROM alert_event ORDER BY time_ms DESC, id DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map([limit], |r| {
            Ok(AlertRow {
                id: r.get(0)?,
                time_ms: r.get(1)?,
                rule_id: r.get(2)?,
                rule_name: r.get(3)?,
                metric: r.get(4)?,
                severity: r.get(5)?,
                transition: r.get(6)?,
                value: r.get(7)?,
                threshold: r.get(8)?,
                since_ms: r.get(9)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn stats(&self, path: Option<&Path>) -> Result<DbStats> {
        let count = |t: &str| -> Result<i64> {
            Ok(self
                .conn
                .query_row(&format!("SELECT count(*) FROM {t}"), [], |r| r.get(0))?)
        };
        Ok(DbStats {
            file_bytes: path
                .and_then(|p| std::fs::metadata(p).ok())
                .map(|m| m.len()),
            raw_rows: count("sample_raw")?,
            minute_rows: count("sample_1m")?,
            events: count("stability_event")?,
            sessions: count("game_session")?,
            alerts: count("alert_event")?,
        })
    }

    /// Reclaims free pages after large deletions (run rarely; rewrites the file).
    pub fn checkpoint(&self) -> Result<()> {
        self.conn
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE); PRAGMA optimize;")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bv(v: f64) -> BucketValue {
        BucketValue {
            avg: v,
            min: v - 1.0,
            max: v + 1.0,
            count: 5,
        }
    }

    fn ev(rec: u64, t: i64) -> RawEvent {
        RawEvent {
            channel: "System".into(),
            provider: "EventLog".into(),
            event_id: 6008,
            level: 2,
            record_id: rec,
            time_ms: t,
            data: vec![("#0".into(), "x".into())],
        }
    }

    #[test]
    fn migrations_apply_once_and_are_idempotent() {
        let dir = std::env::temp_dir().join(format!("pch-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let path = dir.join("m.db");
        let _ = std::fs::remove_file(&path);
        {
            let db = Db::open(&path).expect("open");
            assert_eq!(db.schema_version().expect("v") as usize, MIGRATIONS.len());
        }
        let db = Db::open(&path).expect("reopen");
        assert_eq!(db.schema_version().expect("v") as usize, MIGRATIONS.len());
        drop(db);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn samples_roll_up_and_query_with_gaps() {
        let mut db = Db::open_in_memory().expect("db");
        let mut s = Vec::new();
        for i in 0..24 {
            s.push((
                i * 5_000,
                "cpu.total_pct".to_string(),
                "%".to_string(),
                bv(10.0 + i as f64),
            ));
        }
        // gap of 10 minutes, then more data
        for i in 0..12 {
            s.push((
                720_000 + i * 5_000,
                "cpu.total_pct".to_string(),
                "%".to_string(),
                bv(50.0),
            ));
        }
        assert_eq!(db.write_samples(&s).expect("write"), 36);
        let series = db
            .series(&["cpu.total_pct".into()], 0, 800_000, 1000, 800_000, 48)
            .expect("series");
        assert_eq!(series[0].source, "raw");
        assert_eq!(series[0].unit, "%");
        assert_eq!(
            series[0].avg.iter().filter(|v| v.is_none()).count(),
            1,
            "one gap marked"
        );

        assert!(db.rollup(0, 780_000).expect("rollup") >= 3);
        let pts = db.minute_points("cpu.%", 0).expect("points");
        assert_eq!(pts[0].0, "cpu.total_pct");
        assert_eq!(pts[0].1[0], (0, 15.5, 22.0));
        let cat = db.metric_catalog().expect("catalog");
        assert_eq!((cat[0].first_ms, cat[0].last_ms), (Some(0), Some(775_000)));
    }

    #[test]
    fn retention_removes_old_rows() {
        let mut db = Db::open_in_memory().expect("db");
        db.write_samples(&[
            (0, "a".into(), "%".into(), bv(1.0)),
            (100 * 3_600_000, "a".into(), "%".into(), bv(1.0)),
        ])
        .expect("w");
        db.insert_events(&[ev(1, 0)]).expect("e");
        let removed = db
            .apply_retention(100 * 3_600_000, &Retention::default())
            .expect("ret");
        assert_eq!(
            removed, 1,
            "only the raw sample older than 48 h; the event is within 365 days"
        );
        assert_eq!(db.stats(None).expect("stats").raw_rows, 1);
    }

    #[test]
    fn events_are_deduplicated() {
        let mut db = Db::open_in_memory().expect("db");
        assert_eq!(
            db.insert_events(&[ev(1, 10), ev(2, 20)])
                .expect("first")
                .len(),
            2
        );
        assert_eq!(
            db.insert_events(&[ev(2, 20), ev(3, 30)])
                .expect("second")
                .len(),
            1
        );
        let all = db.events(0, 100, 10).expect("read");
        assert_eq!(
            all.iter().map(|e| e.record_id).collect::<Vec<_>>(),
            vec![3, 2, 1]
        );
        assert_eq!(all[0].data, vec![("#0".to_string(), "x".to_string())]);
        assert!(db.event("System", 2).expect("one").is_some());
        assert!(db.event("Application", 2).expect("none").is_none());
    }

    #[test]
    fn sessions_round_trip() {
        let mut db = Db::open_in_memory().expect("db");
        let id = db
            .insert_session(&NewSession {
                process_name: "game.exe".into(),
                exe_path: None,
                pid: 7,
                started_ms: 1,
                ended_ms: 2,
                provider: "PresentMon".into(),
                stats: serde_json::json!({"avgFps": 60.0}),
                context: serde_json::json!({}),
                frametimes_ms: vec![16.5, 17.25],
            })
            .expect("insert");
        let (s, ft) = db.session(id).expect("read").expect("exists");
        assert_eq!(s.stats["avgFps"], 60.0);
        assert_eq!(ft, vec![16.5, 17.25]);
        assert_eq!(db.sessions(10).expect("list").len(), 1);
        assert!(db.delete_session(id).expect("delete"));
        assert!(db.session(id).expect("read").is_none());
    }

    #[test]
    fn large_event_history_is_fast_enough() {
        let mut db = Db::open_in_memory().expect("db");
        let events: Vec<RawEvent> = (0..20_000).map(|i| ev(i, i as i64 * 1000)).collect();
        let t = std::time::Instant::now();
        db.insert_events(&events).expect("insert");
        let read = db.events(0, i64::MAX, 5000).expect("read");
        assert_eq!(read.len(), 5000);
        assert!(
            t.elapsed().as_secs() < 5,
            "20k inserts + 5k read took {:?}",
            t.elapsed()
        );
    }
}
