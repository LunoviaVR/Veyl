//! Nuvoton NCT679x / NCT6799 Super I/O hardware monitor (motherboard fans,
//! voltages, temperatures) through PawnIO's signed `LpcIO` module.
//!
//! Register layout: Linux `nct6775` driver and LibreHardwareMonitor `Nct677X`.
//! Every access holds the system-wide ISA bus mutex shared with other monitoring
//! tools (LibreHardwareMonitor, FanControl, HWiNFO), so reads never interleave
//! with another tool's bank selection. Only index and bank-select writes are made,
//! which is how this chip's registers are read; no configuration is changed.

use pch_core::{
    Availability, HardwareSensorProvider, ProviderPoll, SensorCategory, SensorKind, SensorReading,
    Status,
};
use windows::core::s;
use windows::Win32::Foundation::{HANDLE, WAIT_OBJECT_0};
use windows::Win32::System::Threading::{CreateMutexW, ReleaseMutex, WaitForSingleObject};

use crate::handle::{wide, OwnedHandle};
use crate::pawnio::{Executor, LPC_IO};

const ISA_MUTEX: &str = r"Global\Access_ISABUS.HTP.Method";
/// Nuvoton extended-function mode enter/exit keys.
const NUVOTON_ENTER: u64 = 0x87;
const NUVOTON_EXIT: u64 = 0xAA;
/// Logical device 0x0B = hardware monitor; base address at config register 0x60.
const LDN_HWM: u64 = 0x0B;
const BANK_SELECT: u64 = 0x4E;
const FAN_COUNT_REGS: [u16; 7] = [0x4B0, 0x4B2, 0x4B4, 0x4B6, 0x4B8, 0x4BA, 0x4CC];
/// (label, register, millivolts per LSB): nct6775 `nct6779_in_label` / `scale_in`.
const VOLTAGES: [(&str, u16, f64); 16] = [
    ("CPU Vcore", 0x480, 8.0),
    ("VIN1", 0x481, 8.0),
    ("AVCC", 0x482, 16.0),
    ("+3.3V (3VCC)", 0x483, 16.0),
    ("VIN0", 0x484, 8.0),
    ("VIN8", 0x485, 8.0),
    ("VIN4", 0x486, 8.0),
    ("3VSB", 0x487, 16.0),
    ("VBAT", 0x488, 16.0),
    ("VTT", 0x489, 8.0),
    ("VIN5", 0x48A, 8.0),
    ("VIN6", 0x48B, 8.0),
    ("VIN2", 0x48C, 8.0),
    ("VIN3", 0x48D, 8.0),
    ("VIN7", 0x48E, 8.0),
    ("VIN9", 0x48F, 8.0),
];
const TEMPS: [(&str, u16); 6] = [
    ("SYSTIN (motherboard)", 0x490),
    ("CPUTIN (CPU socket)", 0x491),
    ("AUXTIN0", 0x492),
    ("AUXTIN1", 0x493),
    ("AUXTIN2", 0x494),
    ("AUXTIN3", 0x495),
];

/// RPM from a 13-bit fan count: count = high << 5 | low & 0x1F; rpm = 1.35e6 / count.
pub fn fan_rpm(high: u8, low: u8) -> Option<f64> {
    let count = (u32::from(high) << 5) | u32::from(low & 0x1F);
    match count {
        0x1FFF => Some(0.0), // counter saturated: fan stopped or header unused
        c if c < 0x15 => None,
        c => Some(1.35e6 / f64::from(c)),
    }
}

/// A Nuvoton temperature byte is signed °C; out-of-range values are invalid.
pub fn nuvoton_temp(raw: u8) -> Option<f64> {
    let t = f64::from(raw as i8);
    (raw != 0x80 && (-40.0..=120.0).contains(&t)).then_some(t)
}

/// The held ISA bus mutex; released on drop.
struct IsaLock(HANDLE);

impl IsaLock {
    fn acquire(timeout_ms: u32) -> Option<Self> {
        let name = wide(ISA_MUTEX);
        // SAFETY: named mutex shared with other monitoring tools; handle owned below.
        let h = unsafe { CreateMutexW(None, false, windows::core::PCWSTR(name.as_ptr())) }.ok()?;
        // SAFETY: valid mutex handle. WAIT_ABANDONED (0x80) also grants ownership.
        let w = unsafe { WaitForSingleObject(h, timeout_ms) };
        if w == WAIT_OBJECT_0 || w.0 == 0x80 {
            Some(Self(h))
        } else {
            drop(OwnedHandle(h));
            None
        }
    }
}

impl Drop for IsaLock {
    fn drop(&mut self) {
        // SAFETY: releasing the mutex this thread owns.
        if let Err(e) = unsafe { ReleaseMutex(self.0) } {
            tracing::warn!("ReleaseMutex(ISA bus) failed: {e}");
        }
        drop(OwnedHandle(self.0));
    }
}

struct Nuvoton {
    ex: Executor,
    addr_port: u64,
    data_port: u64,
    device: String,
}

impl Nuvoton {
    fn read(&self, reg: u16) -> Option<u8> {
        let out = |port: u64, v: u64| self.ex.execute(s!("ioctl_pio_outb"), &[port, v], 0).ok();
        out(self.addr_port, BANK_SELECT)?;
        out(self.data_port, u64::from(reg >> 8))?;
        out(self.addr_port, u64::from(reg & 0xFF))?;
        self.ex
            .execute(s!("ioctl_pio_inb"), &[self.data_port], 1)
            .ok()?
            .first()
            .map(|v| *v as u8)
    }

    fn detect() -> Result<Self, Status> {
        let ex = Executor::open(LPC_IO)?;
        let _lock = IsaLock::acquire(1000).ok_or_else(|| {
            Status::new(
                Availability::Stale,
                "Super I/O is busy (another monitoring tool holds the ISA bus)",
            )
        })?;
        for (slot, port) in [(0u64, 0x2Eu64), (1, 0x4E)] {
            if ex.execute(s!("ioctl_select_slot"), &[slot], 0).is_err() {
                continue;
            }
            let pio = |v: u64| ex.execute(s!("ioctl_pio_outb"), &[port, v], 0).ok();
            pio(NUVOTON_ENTER);
            pio(NUVOTON_ENTER);
            let id = ex
                .execute(s!("ioctl_superio_inw"), &[0x20], 1)
                .ok()
                .and_then(|v| v.first().copied())
                .unwrap_or(0);
            let family = id >> 8;
            // NCT6796/6797/6798 report 0xD4xx; NCT6799 reports 0xD8xx. Both use the bank-4 layout above.
            if family == 0xD4 || family == 0xD8 {
                let selected = ex
                    .execute(s!("ioctl_superio_outb"), &[0x07, LDN_HWM], 0)
                    .is_ok();
                let base = ex
                    .execute(s!("ioctl_superio_inw"), &[0x60], 1)
                    .ok()
                    .and_then(|v| v.first().copied())
                    .unwrap_or(0)
                    & 0xFFF8;
                let bars = ex.execute(s!("ioctl_find_bars"), &[], 0).is_ok();
                pio(NUVOTON_EXIT);
                if selected && bars && base >= 0x100 {
                    let name = if family == 0xD8 {
                        "NCT6799D"
                    } else {
                        "NCT679x"
                    };
                    return Ok(Self {
                        ex,
                        addr_port: base + 5,
                        data_port: base + 6,
                        device: format!("Nuvoton {name} (ID 0x{id:04X})"),
                    });
                }
            } else {
                pio(NUVOTON_EXIT);
            }
        }
        Err(Status::new(
            Availability::Unsupported,
            "No supported Nuvoton Super I/O chip (NCT6796–NCT6799) was found",
        ))
    }
}

pub struct SuperIoProvider {
    chip: Result<Nuvoton, Status>,
}

impl SuperIoProvider {
    pub fn new() -> Self {
        Self {
            chip: Nuvoton::detect(),
        }
    }
}

impl Default for SuperIoProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl HardwareSensorProvider for SuperIoProvider {
    fn id(&self) -> &'static str {
        "pawnio-superio"
    }
    fn name(&self) -> &'static str {
        "Motherboard fans, voltages, temperatures (PawnIO · LpcIO)"
    }

    fn poll(&mut self, now_ms: i64) -> ProviderPoll {
        let chip = match &self.chip {
            Ok(c) => c,
            Err(s) => {
                return ProviderPoll {
                    status: s.clone(),
                    readings: vec![],
                }
            }
        };
        let Some(_lock) = IsaLock::acquire(200) else {
            return ProviderPoll {
                status: Status::new(Availability::Stale, "Super I/O busy; skipped this poll"),
                readings: vec![],
            };
        };
        let mut readings = Vec::new();
        let mut push = |label: String, kind: SensorKind, unit: &str, v: Option<f64>| {
            readings.push(SensorReading {
                key: format!("pawnio-superio/{label}"),
                provider: "pawnio-superio".into(),
                device: chip.device.clone(),
                category: SensorCategory::Motherboard,
                kind,
                label,
                unit: unit.into(),
                value: v,
                timestamp_ms: now_ms,
            })
        };
        use pch_core::unit::*;
        for (i, reg) in FAN_COUNT_REGS.iter().enumerate() {
            let rpm = match (chip.read(*reg), chip.read(reg + 1)) {
                (Some(h), Some(l)) => fan_rpm(h, l),
                _ => None,
            };
            push(format!("Fan #{}", i + 1), SensorKind::Fan, RPM, rpm);
        }
        for (label, reg, mv) in VOLTAGES {
            let v = chip
                .read(reg)
                .map(|r| f64::from(r) * mv / 1000.0)
                .filter(|v| *v > 0.0);
            push(label.to_owned(), SensorKind::Voltage, VOLT, v);
        }
        for (label, reg) in TEMPS {
            push(
                label.to_owned(),
                SensorKind::Temperature,
                CELSIUS,
                chip.read(reg).and_then(nuvoton_temp),
            );
        }
        ProviderPoll {
            status: Status::available(),
            readings,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fan_and_temp_decoding() {
        // count 675 -> 2000 RPM
        assert_eq!(
            fan_rpm((675u32 >> 5) as u8, (675u32 & 0x1F) as u8),
            Some(2000.0)
        );
        assert_eq!(
            fan_rpm(0xFF, 0x1F),
            Some(0.0),
            "saturated counter = stopped"
        );
        assert_eq!(fan_rpm(0, 1), None);
        assert_eq!(nuvoton_temp(45), Some(45.0));
        assert_eq!(nuvoton_temp(0xF6), Some(-10.0));
        assert_eq!(nuvoton_temp(0x80), None);
        assert_eq!(nuvoton_temp(127), None);
    }
}
