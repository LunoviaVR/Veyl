//! Flattens a telemetry frame into named metrics: the keys used by history,
//! alerts and diagnostics. One function, so the three can never disagree on a
//! key or a unit. Only measured values appear; missing readings are omitted.

use pch_core::{unit, SensorCategory, SensorKind, SensorReading, TelemetryFrame};

/// Maximum number of provider sensor readings recorded to history per tick.
pub const MAX_SENSOR_METRICS: usize = 48;

#[derive(Debug, Clone, PartialEq)]
pub struct Metric {
    pub key: String,
    pub unit: &'static str,
    pub value: f64,
}

fn push(out: &mut Vec<Metric>, key: impl Into<String>, unit: &'static str, value: Option<f64>) {
    if let Some(v) = value.filter(|v| v.is_finite()) {
        out.push(Metric {
            key: key.into(),
            unit,
            value: v,
        });
    }
}

/// Mount `C:\` → `C:` for metric keys.
pub fn volume_id(mount: &str) -> String {
    mount.trim_end_matches('\\').to_owned()
}

pub fn from_frame(f: &TelemetryFrame, sensors: &[SensorReading]) -> Vec<Metric> {
    let mut m = Vec::with_capacity(48);
    push(
        &mut m,
        "cpu.total_pct",
        unit::PERCENT,
        f.cpu.total_pct.map(f64::from),
    );
    push(
        &mut m,
        "cpu.utility_pct",
        unit::PERCENT,
        f.cpu.utility_pct.map(f64::from),
    );
    push(
        &mut m,
        "cpu.effective_mhz",
        unit::MHZ,
        f.cpu.effective_mhz.map(f64::from),
    );
    push(
        &mut m,
        "cpu.package_temp_c",
        unit::CELSIUS,
        f.cpu.package_temp_c.map(f64::from),
    );
    push(
        &mut m,
        "cpu.package_power_w",
        unit::WATT,
        f.cpu.package_power_w.map(f64::from),
    );
    push(
        &mut m,
        "memory.load_pct",
        unit::PERCENT,
        f.memory.load_pct.map(f64::from),
    );
    push(
        &mut m,
        "memory.used_bytes",
        unit::BYTES,
        f.memory.used_bytes.map(|v| v as f64),
    );
    if let (Some(c), Some(l)) = (f.memory.commit_bytes, f.memory.commit_limit_bytes) {
        push(
            &mut m,
            "memory.commit_pct",
            unit::PERCENT,
            (l > 0).then(|| c as f64 / l as f64 * 100.0),
        );
        push(&mut m, "memory.commit_bytes", unit::BYTES, Some(c as f64));
    }
    push(
        &mut m,
        "memory.pages_input_per_sec",
        unit::COUNT,
        f.memory.pages_input_per_sec.map(f64::from),
    );
    push(
        &mut m,
        "disk.read_bps",
        unit::BYTES_PER_SEC,
        f.disks.total_read_bps,
    );
    push(
        &mut m,
        "disk.write_bps",
        unit::BYTES_PER_SEC,
        f.disks.total_write_bps,
    );
    for v in &f.disks.volumes {
        if let (Some(t), Some(fr)) = (v.total_bytes, v.free_bytes) {
            if t > 0 && v.drive_type == "fixed" {
                push(
                    &mut m,
                    format!("volume.{}.free_pct", volume_id(&v.mount)),
                    unit::PERCENT,
                    Some(fr as f64 / t as f64 * 100.0),
                );
            }
        }
    }
    push(
        &mut m,
        "net.rx_bps",
        unit::BYTES_PER_SEC,
        f.network.total_rx_bps,
    );
    push(
        &mut m,
        "net.tx_bps",
        unit::BYTES_PER_SEC,
        f.network.total_tx_bps,
    );
    for (i, g) in f.gpus.adapters.iter().enumerate() {
        push(
            &mut m,
            format!("gpu.{i}.util_pct"),
            unit::PERCENT,
            g.util_pct.map(f64::from),
        );
        push(
            &mut m,
            format!("gpu.{i}.vram_used_bytes"),
            unit::BYTES,
            g.dedicated_used_bytes.map(|v| v as f64),
        );
        push(
            &mut m,
            format!("gpu.{i}.temp_c"),
            unit::CELSIUS,
            g.temperature_c.map(f64::from),
        );
        push(
            &mut m,
            format!("gpu.{i}.hotspot_c"),
            unit::CELSIUS,
            g.hotspot_c.map(f64::from),
        );
        push(
            &mut m,
            format!("gpu.{i}.core_mhz"),
            unit::MHZ,
            g.core_clock_mhz.map(f64::from),
        );
        push(
            &mut m,
            format!("gpu.{i}.fan_rpm"),
            unit::RPM,
            g.fan_rpm.map(f64::from),
        );
        push(
            &mut m,
            format!("gpu.{i}.power_pct"),
            unit::PERCENT,
            g.power_pct.map(f64::from),
        );
        push(
            &mut m,
            format!("gpu.{i}.power_w"),
            unit::WATT,
            g.power_w.map(f64::from),
        );
    }
    let passive = sensors
        .iter()
        .filter(|r| r.provider == "thermal-zone" && r.label == "Passive cooling limit")
        .filter_map(|r| r.value)
        .fold(None, |acc: Option<f64>, v| {
            Some(acc.map_or(v, |a| a.min(v)))
        });
    push(
        &mut m,
        "thermal.passive_limit_min_pct",
        unit::PERCENT,
        passive,
    );
    push(
        &mut m,
        "self.cpu_pct",
        unit::PERCENT,
        f.self_usage.cpu_pct.map(f64::from),
    );
    push(
        &mut m,
        "self.working_set_bytes",
        unit::BYTES,
        f.self_usage.working_set_bytes.map(|v| v as f64),
    );
    push(
        &mut m,
        "self.tick_cost_ms",
        unit::MILLISECONDS,
        f.self_usage.tick_cost_ms.map(f64::from),
    );

    // Provider temperatures, fans and power for CPU/GPU/board/storage, bounded.
    let mut n = 0;
    for r in sensors {
        if n >= MAX_SENSOR_METRICS {
            break;
        }
        let wanted_kind = matches!(
            r.kind,
            SensorKind::Temperature | SensorKind::Fan | SensorKind::Power
        );
        let wanted_cat = !matches!(r.category, SensorCategory::Other);
        let unit = match r.kind {
            SensorKind::Temperature => unit::CELSIUS,
            SensorKind::Fan => unit::RPM,
            SensorKind::Power => unit::WATT,
            _ => continue,
        };
        // Power readings are only recorded when the provider labels them in watts.
        if r.kind == SensorKind::Power && r.unit != unit::WATT {
            continue;
        }
        if wanted_kind && wanted_cat && r.value.is_some() {
            push(&mut m, format!("sensor.{}", r.key), unit, r.value);
            n += 1;
        }
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;
    use pch_core::{GpuSnapshot, VolumeSnapshot};

    #[test]
    fn only_measured_values_become_metrics() {
        let mut f = TelemetryFrame::default();
        f.cpu.total_pct = Some(12.5);
        f.memory.commit_bytes = Some(50);
        f.memory.commit_limit_bytes = Some(200);
        f.gpus.adapters.push(GpuSnapshot {
            util_pct: Some(40.0),
            temperature_c: None,
            ..Default::default()
        });
        f.disks.volumes.push(VolumeSnapshot {
            mount: "C:\\".into(),
            drive_type: "fixed".into(),
            total_bytes: Some(100),
            free_bytes: Some(25),
            ..Default::default()
        });
        f.disks.volumes.push(VolumeSnapshot {
            mount: "E:\\".into(),
            drive_type: "optical".into(),
            ..Default::default()
        });
        let m = from_frame(&f, &[]);
        let get = |k: &str| m.iter().find(|x| x.key == k).map(|x| x.value);
        assert_eq!(get("cpu.total_pct"), Some(12.5));
        assert_eq!(get("memory.commit_pct"), Some(25.0));
        assert_eq!(get("gpu.0.util_pct"), Some(40.0));
        assert_eq!(
            get("gpu.0.temp_c"),
            None,
            "missing reading is not recorded as 0"
        );
        assert_eq!(get("volume.C:.free_pct"), Some(25.0));
        assert!(m.iter().all(|x| !x.key.starts_with("volume.E:")));
        assert_eq!(get("memory.load_pct"), None);
    }
}
