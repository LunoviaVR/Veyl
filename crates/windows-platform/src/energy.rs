//! CPU and memory power from the Windows Energy Meter counters, which publish the
//! processor's RAPL (Running Average Power Limit) energy domains through the
//! Energy Metering Interface. No driver or elevation is needed.
//!
//! Domains: `RAPL_Package0_PKG` (whole package), `PP0` (cores), `PP1` (integrated
//! graphics), `DRAM`. Domains the platform does not implement read 0 forever, so a
//! domain is only reported after it has produced a non-zero value once.

use std::collections::HashSet;

use veyl_core::{
    Availability, HardwareSensorProvider, ProviderPoll, SensorCategory, SensorKind, SensorReading,
    Status,
};

use crate::pdh::{Counter, PdhError, PdhQuery};

/// Human label and category for a RAPL instance name (case-insensitive).
pub fn domain(instance: &str) -> Option<(&'static str, SensorCategory)> {
    let i = instance.to_ascii_lowercase();
    if i == "_total" {
        return None;
    }
    Some(if i.ends_with("_pkg") {
        ("CPU package power", SensorCategory::Cpu)
    } else if i.ends_with("_pp0") {
        ("CPU cores power", SensorCategory::Cpu)
    } else if i.ends_with("_pp1") {
        ("CPU integrated graphics power", SensorCategory::Cpu)
    } else if i.ends_with("_dram") {
        ("Memory (DRAM) power", SensorCategory::Memory)
    } else {
        ("Energy meter", SensorCategory::System)
    })
}

pub struct EnergyMeterProvider {
    counters: Result<(PdhQuery, Counter), PdhError>,
    seen_nonzero: HashSet<String>,
}

impl EnergyMeterProvider {
    pub fn new() -> Self {
        let counters = (|| {
            let mut q = PdhQuery::open()?;
            let c = q.add(r"\Energy Meter(*)\Power")?;
            q.collect()?;
            Ok((q, c))
        })();
        Self {
            counters,
            seen_nonzero: HashSet::new(),
        }
    }
}

impl Default for EnergyMeterProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl HardwareSensorProvider for EnergyMeterProvider {
    fn id(&self) -> &'static str {
        "energy-meter"
    }
    fn name(&self) -> &'static str {
        "CPU power (Windows Energy Meter / RAPL)"
    }

    fn poll(&mut self, now_ms: i64) -> ProviderPoll {
        let fail = |e: PdhError| ProviderPoll {
            status: if e.is_not_found() {
                Status::new(
                    Availability::Unsupported,
                    "This system does not publish Energy Meter counters",
                )
            } else {
                Status::error(format!("Energy Meter counters: {e}"))
            },
            readings: vec![],
        };
        let (q, c) = match &mut self.counters {
            Ok(x) => x,
            Err(e) => return fail(*e),
        };
        if let Err(e) = q.collect() {
            return fail(e);
        }
        let items = match q.array(*c, true) {
            Ok(i) => i,
            Err(e) => {
                return ProviderPoll {
                    status: Status::error(format!("Energy Meter counters: {e}")),
                    readings: vec![],
                }
            }
        };
        let mut readings = Vec::new();
        for (inst, mw) in items {
            let Some((label, category)) = domain(&inst) else {
                continue;
            };
            if mw > 0.0 {
                self.seen_nonzero.insert(inst.to_ascii_lowercase());
            }
            if !self.seen_nonzero.contains(&inst.to_ascii_lowercase()) {
                continue;
            }
            readings.push(SensorReading {
                key: format!("energy-meter/{}", inst.to_ascii_lowercase()),
                provider: "energy-meter".into(),
                device: "Processor (RAPL)".into(),
                category,
                kind: SensorKind::Power,
                label: label.into(),
                unit: veyl_core::unit::WATT.into(),
                // The Energy Meter Power counter is in milliwatts.
                value: Some(mw / 1000.0),
                timestamp_ms: now_ms,
            });
        }
        let status = if readings.is_empty() && q.primed() {
            Status::new(
                Availability::Unsupported,
                "No active RAPL power domains are published on this system",
            )
        } else if readings.is_empty() {
            Status::waiting("Waiting for the second Energy Meter sample")
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
    fn domain_names() {
        assert_eq!(
            domain("rapl_package0_pkg").map(|d| d.0),
            Some("CPU package power")
        );
        assert_eq!(
            domain("RAPL_Package0_DRAM").map(|d| d.1),
            Some(SensorCategory::Memory)
        );
        assert_eq!(domain("_total"), None);
    }

    #[test]
    fn reads_package_power_after_two_samples() {
        let mut p = EnergyMeterProvider::new();
        let _ = p.poll(0);
        std::thread::sleep(std::time::Duration::from_millis(1100));
        let poll = p.poll(1);
        assert_ne!(
            poll.status.state,
            Availability::Error,
            "{:?}",
            poll.status.message
        );
    }
}
