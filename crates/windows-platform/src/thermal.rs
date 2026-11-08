//! ACPI thermal zones via the `Thermal Zone Information` performance counters.
//!
//! Readable without administrator rights. These are firmware-defined zones: on
//! most desktops they are a motherboard or chipset sensor, not the CPU die, so
//! they are labelled as ACPI zones and never presented as "CPU temperature".

use veyl_core::{
    Availability, HardwareSensorProvider, ProviderPoll, SensorCategory, SensorKind, SensorReading,
    Status,
};

use crate::pdh::{Counter, PdhError, PdhQuery};

struct Counters {
    query: PdhQuery,
    temp: Counter,
    /// `High Precision Temperature` is tenths of a kelvin; `Temperature` is kelvin.
    temp_tenths: bool,
    passive: Option<Counter>,
    throttle: Option<Counter>,
}

pub struct ThermalZoneProvider {
    counters: Result<Counters, PdhError>,
}

/// Converts a counter value to °C. Values outside -50..150 °C are rejected as implausible.
pub fn kelvin_to_celsius(v: f64, tenths: bool) -> Option<f64> {
    let k = if tenths { v / 10.0 } else { v };
    let c = k - 273.15;
    (k > 0.0 && (-50.0..150.0).contains(&c)).then_some((c * 10.0).round() / 10.0)
}

impl ThermalZoneProvider {
    pub fn new() -> Self {
        let counters = (|| {
            let mut query = PdhQuery::open()?;
            let (temp, temp_tenths) =
                match query.add(r"\Thermal Zone Information(*)\High Precision Temperature") {
                    Ok(c) => (c, true),
                    Err(_) => (
                        query.add(r"\Thermal Zone Information(*)\Temperature")?,
                        false,
                    ),
                };
            let passive = query
                .add(r"\Thermal Zone Information(*)\% Passive Limit")
                .ok();
            let throttle = query
                .add(r"\Thermal Zone Information(*)\Throttle Reasons")
                .ok();
            Ok(Counters {
                query,
                temp,
                temp_tenths,
                passive,
                throttle,
            })
        })();
        Self { counters }
    }
}

impl Default for ThermalZoneProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl HardwareSensorProvider for ThermalZoneProvider {
    fn id(&self) -> &'static str {
        "thermal-zone"
    }
    fn name(&self) -> &'static str {
        "ACPI thermal zones (Windows)"
    }

    fn poll(&mut self, now_ms: i64) -> ProviderPoll {
        let c = match &mut self.counters {
            Ok(c) => c,
            Err(e) if e.is_not_found() => {
                return ProviderPoll {
                    status: Status::new(
                        Availability::Unsupported,
                        "This system exposes no ACPI thermal zones",
                    ),
                    readings: vec![],
                }
            }
            Err(e) => {
                return ProviderPoll {
                    status: Status::error(format!("Thermal zone counters: {e}")),
                    readings: vec![],
                }
            }
        };
        if let Err(e) = c.query.collect() {
            return ProviderPoll {
                status: Status::error(format!("Thermal zone counters: {e}")),
                readings: vec![],
            };
        }
        let mut readings = Vec::new();
        let mut push = |zone: &str, label: &str, kind, unit: &str, value: Option<f64>| {
            readings.push(SensorReading {
                key: format!("thermal-zone/{zone}/{label}"),
                provider: "thermal-zone".into(),
                device: format!("ACPI thermal zone {zone}"),
                category: SensorCategory::System,
                kind,
                label: label.into(),
                unit: unit.into(),
                value,
                timestamp_ms: now_ms,
            })
        };
        match c.query.array(c.temp, true) {
            Ok(items) => {
                for (zone, v) in items {
                    push(
                        &zone,
                        "Temperature",
                        SensorKind::Temperature,
                        veyl_core::unit::CELSIUS,
                        kelvin_to_celsius(v, c.temp_tenths),
                    );
                }
            }
            Err(e) => {
                return ProviderPoll {
                    status: Status::error(format!("Thermal zone temperature: {e}")),
                    readings: vec![],
                }
            }
        }
        if let Some(p) = c.passive {
            for (zone, v) in c.query.array(p, true).unwrap_or_default() {
                push(
                    &zone,
                    "Passive cooling limit",
                    SensorKind::Throttle,
                    veyl_core::unit::PERCENT,
                    Some(v),
                );
            }
        }
        if let Some(t) = c.throttle {
            for (zone, v) in c.query.array(t, true).unwrap_or_default() {
                push(
                    &zone,
                    "Throttle reasons (bitmask)",
                    SensorKind::Throttle,
                    veyl_core::unit::COUNT,
                    Some(v),
                );
            }
        }
        let status = if readings.is_empty() {
            Status::new(
                Availability::Unsupported,
                "This system exposes no ACPI thermal zones",
            )
        } else {
            Status::available()
        };
        ProviderPoll { status, readings }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_kelvin() {
        assert_eq!(kelvin_to_celsius(3232.0, true), Some(50.1));
        assert_eq!(kelvin_to_celsius(300.0, false), Some(26.9));
        assert_eq!(kelvin_to_celsius(0.0, false), None);
        assert_eq!(kelvin_to_celsius(1000.0, false), None);
    }
}
