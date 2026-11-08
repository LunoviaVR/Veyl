//! Mapping provider-specific sensor names to the few canonical readings the
//! overview, alerts and history use (CPU package temperature, GPU hotspot).
//!
//! These are name heuristics over labels reported by an external tool, so every
//! canonical value keeps a `source` string naming the exact provider reading it
//! came from. No value is ever derived or estimated.

use crate::model::{SensorCategory, SensorKind, SensorReading};

/// Classifies a hardware-monitor sensor (device) name into a category.
pub fn category_for_device(device: &str) -> SensorCategory {
    let d = device.to_ascii_lowercase();
    let has = |words: &[&str]| words.iter().any(|w| d.contains(w));
    if has(&["gpu", "radeon", "geforce", "arc ", "nvidia", "amd radeon"]) {
        SensorCategory::Gpu
    } else if has(&["cpu", "core", "ryzen", "intel", "processor"]) {
        SensorCategory::Cpu
    } else if has(&["s.m.a.r.t", "smart", "drive", "nvme", "ssd", "disk"]) {
        SensorCategory::Storage
    } else if has(&["dimm", "memory", "spd", "ddr"]) {
        SensorCategory::Memory
    } else if has(&[
        "nuvoton",
        "ite ",
        "it86",
        "it87",
        "fintek",
        "asus",
        "msi",
        "gigabyte",
        "asrock",
        "embedded controller",
        "ec:",
        "motherboard",
        "chipset",
    ]) {
        SensorCategory::Motherboard
    } else if has(&["system", "windows", "acpi"]) {
        SensorCategory::System
    } else {
        SensorCategory::Other
    }
}

/// Labels that denote the whole-package CPU temperature, in priority order.
const CPU_PACKAGE_LABELS: [&str; 5] = [
    "cpu package",
    "cpu (tctl/tdie)",
    "tctl/tdie",
    "cpu die (average)",
    "package",
];

/// Picks the CPU package temperature from provider readings: the first label in
/// [`CPU_PACKAGE_LABELS`] priority order among CPU temperature readings.
pub fn cpu_package_temp(readings: &[SensorReading]) -> Option<(f64, String)> {
    let temps: Vec<&SensorReading> = readings
        .iter()
        .filter(|r| {
            r.category == SensorCategory::Cpu
                && r.kind == SensorKind::Temperature
                && r.value.is_some()
        })
        .collect();
    for wanted in CPU_PACKAGE_LABELS {
        if let Some(r) = temps
            .iter()
            .find(|r| r.label.to_ascii_lowercase() == wanted)
        {
            return r
                .value
                .map(|v| (v, format!("{} · {} · {}", r.provider, r.device, r.label)));
        }
    }
    None
}

/// First GPU reading for `gpu_name` whose label (lower-case) contains one of `labels`,
/// using the same device-matching rule as [`gpu_hotspot_temp`].
pub fn gpu_reading(
    readings: &[SensorReading],
    gpu_name: &str,
    gpu_count: usize,
    kind: SensorKind,
    labels: &[&str],
) -> Option<f64> {
    let name = gpu_name.to_ascii_lowercase();
    readings
        .iter()
        .filter(|r| r.category == SensorCategory::Gpu && r.kind == kind)
        .filter(|r| {
            labels
                .iter()
                .any(|l| r.label.to_ascii_lowercase().contains(l))
        })
        .find(|r| gpu_count <= 1 || r.device.to_ascii_lowercase().contains(&name))
        .and_then(|r| r.value)
}

/// CPU package power from a RAPL provider.
pub fn cpu_package_power(readings: &[SensorReading]) -> Option<f64> {
    readings
        .iter()
        .find(|r| {
            r.category == SensorCategory::Cpu
                && r.kind == SensorKind::Power
                && r.label == "CPU package power"
        })
        .and_then(|r| r.value)
}

/// Picks a GPU hotspot ("junction") temperature for the adapter named `gpu_name`.
/// With one GPU in the readings, any GPU hotspot reading matches; with several,
/// the provider's device name must contain the adapter name.
pub fn gpu_hotspot_temp(
    readings: &[SensorReading],
    gpu_name: &str,
    gpu_count: usize,
) -> Option<(f64, String)> {
    let name = gpu_name.to_ascii_lowercase();
    readings
        .iter()
        .filter(|r| r.category == SensorCategory::Gpu && r.kind == SensorKind::Temperature)
        .filter(|r| {
            let l = r.label.to_ascii_lowercase();
            l.contains("hot spot") || l.contains("hotspot") || l.contains("junction")
        })
        .find(|r| gpu_count <= 1 || r.device.to_ascii_lowercase().contains(&name))
        .and_then(|r| {
            r.value
                .map(|v| (v, format!("{} · {} · {}", r.provider, r.device, r.label)))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(device: &str, label: &str, kind: SensorKind, v: f64) -> SensorReading {
        SensorReading {
            key: format!("test/{device}/{label}"),
            provider: "test".into(),
            device: device.into(),
            category: category_for_device(device),
            kind,
            label: label.into(),
            unit: "°C".into(),
            value: Some(v),
            timestamp_ms: 0,
        }
    }

    #[test]
    fn categories() {
        assert_eq!(
            category_for_device("CPU [#0]: Intel Core i5-13600KF"),
            SensorCategory::Cpu
        );
        assert_eq!(
            category_for_device("GPU [#0]: AMD Radeon RX 9060 XT"),
            SensorCategory::Gpu
        );
        assert_eq!(
            category_for_device("S.M.A.R.T.: Samsung SSD 990 PRO"),
            SensorCategory::Storage
        );
        assert_eq!(
            category_for_device("Nuvoton NCT6798D"),
            SensorCategory::Motherboard
        );
        assert_eq!(category_for_device("Something"), SensorCategory::Other);
    }

    #[test]
    fn package_priority() {
        let rs = vec![
            r(
                "CPU [#0]: Intel Core i5",
                "Core Max",
                SensorKind::Temperature,
                70.0,
            ),
            r(
                "CPU [#0]: Intel Core i5: DTS",
                "CPU Package",
                SensorKind::Temperature,
                65.0,
            ),
            r(
                "CPU [#0]: Intel Core i5: Enhanced",
                "Package",
                SensorKind::Temperature,
                64.0,
            ),
        ];
        let (v, src) = cpu_package_temp(&rs).expect("package");
        assert_eq!(v, 65.0);
        assert!(src.contains("CPU Package"));
        assert!(cpu_package_temp(&[]).is_none());
    }

    #[test]
    fn hotspot_matching() {
        let rs = vec![r(
            "GPU [#0]: AMD Radeon RX 9060 XT",
            "GPU Hot Spot Temperature",
            SensorKind::Temperature,
            80.0,
        )];
        assert_eq!(
            gpu_hotspot_temp(&rs, "AMD Radeon RX 9060 XT", 1).map(|x| x.0),
            Some(80.0)
        );
        assert_eq!(gpu_hotspot_temp(&rs, "NVIDIA GeForce RTX 4090", 2), None);
        assert_eq!(
            gpu_hotspot_temp(&rs, "anything", 1).map(|x| x.0),
            Some(80.0)
        );
    }
}
