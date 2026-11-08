//! The telemetry thread: owns every collector, samples on the configured
//! interval, publishes snapshots, feeds history and alert evaluation.
//!
//! Scheduling: system counters every `sample_interval_ms`; hardware sensor
//! providers every `sensor_interval_ms`; drive health every
//! `storage_health_interval_min`. Each provider is isolated: a failing or
//! panicking provider becomes a status message and the others keep running.
//! Waiting uses the shutdown channel's timeout, so exit is immediate.

use std::collections::{HashMap, HashSet};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::Ordering;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::sync::Arc;
use std::time::{Duration, Instant};

use pch_core::aggregate::BucketAggregator;
use pch_core::alerts::{AlertEngine, AlertTransition};
use pch_core::sensor_map::{cpu_package_power, cpu_package_temp, gpu_hotspot_temp, gpu_reading};
use pch_core::{
    HardwareSensorProvider, ProviderStatus, SensorKind, SensorRow, SensorStatsTracker, SensorsView,
    Status, TelemetryFrame,
};
use pch_windows::{
    adlx, cpu, disk, energy, gpu, memory, network, nvml, pawnio, processes, storage_health,
    superio, thermal,
};
use tauri::{AppHandle, Emitter, Manager};

use crate::metrics;
use crate::state::{now_ms, AppState};
use crate::writer::{self, WriteMsg};

/// History bucket width: telemetry is stored as 5-second avg/min/max.
pub const BUCKET_MS: i64 = 5_000;

struct Provider {
    inner: Box<dyn HardwareSensorProvider>,
    status: ProviderStatus,
    last: Vec<pch_core::SensorReading>,
}

pub fn run(app: AppHandle, stop: Receiver<()>) {
    let state = app.state::<AppState>();
    let logical = state.system_info.read().cpu.logical_processors.unwrap_or(1);

    let mut cpu_s = cpu::CpuSampler::new();
    let mut mem_s = memory::MemorySampler::new();
    let mut disk_s = disk::DiskSampler::new();
    let mut net_s = network::NetworkSampler::new();
    let mut gpu_s = gpu::GpuSampler::new();
    let mut proc_s = processes::ProcessSampler::new(logical);
    state.system_info.write().gpus = gpu_s.infos();

    let mut providers: Vec<Provider> = vec![
        Box::new(thermal::ThermalZoneProvider::new()) as Box<dyn HardwareSensorProvider>,
        Box::new(energy::EnergyMeterProvider::new()),
        Box::new(adlx::AdlxProvider::new()),
        Box::new(nvml::NvmlProvider::new()),
        Box::new(pawnio::IntelCpuTempProvider::new()),
        Box::new(superio::SuperIoProvider::new()),
    ]
    .into_iter()
    .map(|p| Provider {
        status: ProviderStatus {
            id: p.id().into(),
            name: p.name().into(),
            status: Status::waiting("Not polled yet"),
            last_update_ms: None,
            reading_count: 0,
        },
        inner: p,
        last: Vec::new(),
    })
    .collect();

    let mut tracker = SensorStatsTracker::default();
    let mut stats_since = now_ms();
    let mut agg = BucketAggregator::new(BUCKET_MS);
    let mut alerts = AlertEngine::new();
    let mut seq = 0u64;
    let mut last_sensors: Option<Instant> = None;
    let mut last_drives: Option<Instant> = None;

    // Rate counters need a baseline: wait one interval before the first published sample.
    if matches!(
        stop.recv_timeout(Duration::from_millis(1000)),
        Ok(()) | Err(RecvTimeoutError::Disconnected)
    ) {
        return;
    }

    loop {
        let tick_start = Instant::now();
        let settings = state.settings();
        let now = now_ms();
        seq += 1;

        let mut frame = TelemetryFrame {
            seq,
            timestamp_ms: now,
            cpu: cpu_s.sample(),
            memory: mem_s.sample(),
            disks: disk_s.sample(),
            network: net_s.sample(),
            ..Default::default()
        };
        let (gpus, proc_gpu) = gpu_s.sample();
        frame.gpus = gpus;
        let mut sample = proc_s.sample(now);
        // Once GPU engine counters report, a process without engine activity is measurably at 0 %.
        let gpu_known = !proc_gpu.util_pct.is_empty();
        for row in &mut sample.table.rows {
            row.gpu_pct = proc_gpu
                .util_pct
                .get(&row.pid)
                .copied()
                .or(gpu_known.then_some(0.0));
            row.gpu_dedicated_bytes = proc_gpu.dedicated_bytes.get(&row.pid).copied();
        }
        frame.self_usage = sample.self_usage;

        // Hardware sensor providers on their own interval.
        let sensors_due = last_sensors.is_none_or(|t| {
            t.elapsed() >= Duration::from_millis(u64::from(settings.sensor_interval_ms))
        });
        if sensors_due {
            last_sensors = Some(Instant::now());
            for p in &mut providers {
                match catch_unwind(AssertUnwindSafe(|| p.inner.poll(now))) {
                    Ok(poll) => {
                        if poll.status.is_available() {
                            p.status.last_update_ms = Some(now);
                        }
                        p.status.status = poll.status;
                        p.status.reading_count = poll.readings.len();
                        p.last = poll.readings;
                    }
                    Err(_) => {
                        tracing::error!("sensor provider {} panicked; isolated", p.inner.id());
                        p.status.status = Status::error(
                            "The provider failed unexpectedly and was isolated; see the log",
                        );
                        p.last.clear();
                    }
                }
            }
        }
        let readings: Vec<pch_core::SensorReading> = providers
            .iter()
            .flat_map(|p| p.last.iter().cloned())
            .collect();
        if state.reset_sensor_stats.swap(false, Ordering::Relaxed) {
            tracker.reset();
            stats_since = now;
        }
        if sensors_due {
            tracker.update(&readings);
            let live: HashSet<&str> = readings.iter().map(|r| r.key.as_str()).collect();
            tracker.retain_keys(&live);
        }

        // Canonical CPU package and GPU hotspot temperatures from providers that report them.
        if let Some((v, src)) = cpu_package_temp(&readings) {
            frame.cpu.package_temp_c = Some(v as f32);
            frame.cpu.package_temp_source = Some(src);
        }
        frame.cpu.package_power_w = cpu_package_power(&readings).map(|v| v as f32);
        // Vendor providers (ADLX, NVML) fill what WDDM does not report; WDDM values are kept when present.
        let gpu_count = frame.gpus.adapters.len();
        for g in &mut frame.gpus.adapters {
            if let Some((v, _)) = gpu_hotspot_temp(&readings, &g.name, gpu_count) {
                g.hotspot_c = Some(v as f32);
            }
            let power = &["total board power", "board power"];
            g.power_w = gpu_reading(&readings, &g.name, gpu_count, SensorKind::Power, power)
                .map(|v| v as f32);
            if g.temperature_c.is_none() {
                let t = gpu_reading(
                    &readings,
                    &g.name,
                    gpu_count,
                    SensorKind::Temperature,
                    &["gpu temperature"],
                );
                g.temperature_c = t.map(|v| v as f32);
            }
            if g.fan_rpm.is_none() {
                g.fan_rpm = gpu_reading(&readings, &g.name, gpu_count, SensorKind::Fan, &["fan"])
                    .filter(|_| {
                        readings
                            .iter()
                            .any(|r| r.unit == pch_core::unit::RPM && r.kind == SensorKind::Fan)
                    })
                    .map(|v| v as u32);
            }
        }

        // Drive health (cheap IOCTLs) on its slow interval or on request.
        let drives_due = state.refresh_drives.swap(false, Ordering::Relaxed)
            || last_drives.is_none_or(|t| {
                t.elapsed()
                    >= Duration::from_secs(u64::from(settings.storage_health_interval_min) * 60)
            });
        let drives = if drives_due {
            last_drives = Some(Instant::now());
            Some(Arc::new(
                catch_unwind(storage_health::read_all).unwrap_or_default(),
            ))
        } else {
            None
        };

        frame.self_usage.tick_cost_ms = Some(tick_start.elapsed().as_secs_f32() * 1000.0);
        let metric_list = metrics::from_frame(&frame, &readings);

        // History: 5-second buckets, written when complete.
        if settings.history_enabled {
            for m in &metric_list {
                agg.push(now, &m.key, m.value);
            }
            let units: HashMap<&str, &'static str> = metric_list
                .iter()
                .map(|m| (m.key.as_str(), m.unit))
                .collect();
            let done: Vec<_> = agg
                .drain_complete(now)
                .into_iter()
                .map(|(ts, key, v)| {
                    let unit = units.get(key.as_str()).copied().unwrap_or("");
                    (ts, key, unit.to_owned(), v)
                })
                .collect();
            if !done.is_empty() {
                writer::send(&state.writer, WriteMsg::Samples(done));
            }
        }

        // Threshold alerts.
        let values: HashMap<String, f64> = metric_list
            .iter()
            .map(|m| (m.key.clone(), m.value))
            .collect();
        let signals = alerts.evaluate(&settings.alert_rules, &values, now);
        for s in &signals {
            crate::notify::record_alert(&app, &state, s, settings.notifications_enabled);
        }

        // Publish.
        {
            let rows: Vec<SensorRow> = readings
                .iter()
                .map(|r| SensorRow {
                    stats: tracker.get(&r.key).cloned(),
                    reading: r.clone(),
                })
                .collect();
            let mut live = state.live.write();
            let frame = Arc::new(frame);
            live.frame = Some(frame.clone());
            live.processes = Arc::new(sample.table);
            live.sensors = Arc::new(SensorsView {
                providers: providers.iter().map(|p| p.status.clone()).collect(),
                rows,
                stats_since_ms: stats_since,
            });
            if let Some(d) = drives {
                live.drives = d;
                live.drives_updated_ms = Some(now);
            }
            for s in signals {
                match s.transition {
                    AlertTransition::Fired => live.active_alerts.push(s),
                    AlertTransition::Resolved => live
                        .active_alerts
                        .retain(|a| !(a.rule_id == s.rule_id && a.metric == s.metric)),
                }
            }
            drop(live);
            if state.ui_visible.load(Ordering::Relaxed)
                || state.overlay_open.load(Ordering::Relaxed)
            {
                if let Err(e) = app.emit("telemetry", &*frame) {
                    tracing::debug!("emit telemetry failed: {e}");
                }
            }
        }

        let interval = Duration::from_millis(u64::from(settings.sample_interval_ms));
        let wait = interval.saturating_sub(tick_start.elapsed());
        match stop.recv_timeout(wait) {
            Err(RecvTimeoutError::Timeout) => {}
            _ => break,
        }
    }
    tracing::info!("sampler stopped");
}
