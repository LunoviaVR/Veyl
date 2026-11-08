//! PC spec sheet: SMBIOS parsing (DMTF DSP0134) and the label/value sections shown
//! on the PC Specs page and written by its TXT and CSV exports. Serial numbers and
//! the computer name are deliberately left out so an export can be shared.

use serde::Serialize;

use crate::{DriveHealth, NetworkAdapterSnapshot, SystemInfo};

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Smbios {
    pub bios_vendor: Option<String>,
    pub bios_version: Option<String>,
    pub bios_date: Option<String>,
    pub system_manufacturer: Option<String>,
    pub system_model: Option<String>,
    pub board_manufacturer: Option<String>,
    pub board_model: Option<String>,
    pub board_version: Option<String>,
    pub cpu_socket: Option<String>,
    /// Memory slots of all system-memory arrays (type 16), when reported.
    pub memory_slots: Option<u32>,
    /// Installed modules only; empty slots are skipped.
    pub modules: Vec<MemoryModule>,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct MemoryModule {
    pub locator: Option<String>,
    pub size_mb: Option<u64>,
    pub kind: Option<&'static str>,
    pub form_factor: Option<&'static str>,
    /// Maximum speed the module supports (SPD), MT/s.
    pub rated_mts: Option<u32>,
    /// Speed the memory controller runs it at, MT/s.
    pub configured_mts: Option<u32>,
    pub manufacturer: Option<String>,
    pub part_number: Option<String>,
}

/// Placeholder strings firmware vendors leave in unused SMBIOS fields.
const JUNK: &[&str] = &[
    "to be filled by o.e.m.",
    "default string",
    "not specified",
    "not available",
    "none",
    "n/a",
    "o.e.m.",
    "oem",
    "unknown",
    "undefined",
    "system manufacturer",
    "system product name",
    "system version",
];

fn clean(s: &str) -> Option<String> {
    let t = s.trim();
    (!t.is_empty() && !JUNK.contains(&t.to_ascii_lowercase().as_str())).then(|| t.to_owned())
}

fn memory_type(v: u8) -> Option<&'static str> {
    Some(match v {
        0x0F => "SDRAM",
        0x12 => "DDR",
        0x13 => "DDR2",
        0x18 => "DDR3",
        0x1A => "DDR4",
        0x1B => "LPDDR",
        0x1C => "LPDDR2",
        0x1D => "LPDDR3",
        0x1E => "LPDDR4",
        0x22 => "DDR5",
        0x23 => "LPDDR5",
        _ => return None,
    })
}

fn form_factor(v: u8) -> Option<&'static str> {
    match v {
        0x09 => Some("DIMM"),
        0x0D => Some("SO-DIMM"),
        _ => None,
    }
}

/// Parses the SMBIOS structure table (the bytes after the 8-byte RawSMBIOSData
/// header that `GetSystemFirmwareTable('RSMB')` returns). Stops quietly at the
/// first malformed structure and keeps what was read before it.
pub fn parse_smbios(table: &[u8]) -> Smbios {
    let mut out = Smbios::default();
    let mut slots: Option<u32> = None;
    let mut i = 0;
    while let Some((kind, f, strings, next)) = structure(table, i) {
        let byte = |o: usize| f.get(o).copied();
        let word = |o: usize| f.get(o..o + 2).map(|b| u16::from_le_bytes([b[0], b[1]]));
        let dword = |o: usize| {
            f.get(o..o + 4)
                .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        };
        let s = |o: usize| {
            let n = byte(o)? as usize;
            clean(strings.get(n.checked_sub(1)?)?)
        };
        // 0xFFFF in a 16-bit speed field means "see the 32-bit extended field".
        let speed = |o: usize, ext: usize| match word(o)? {
            0 => None,
            0xFFFF => dword(ext).filter(|&v| v != 0),
            v => Some(u32::from(v)),
        };
        match kind {
            0 => {
                out.bios_vendor = s(0x04);
                out.bios_version = s(0x05);
                out.bios_date = s(0x08);
            }
            1 => {
                out.system_manufacturer = s(0x04);
                out.system_model = s(0x05);
            }
            2 if out.board_model.is_none() => {
                out.board_manufacturer = s(0x04);
                out.board_model = s(0x05);
                out.board_version = s(0x06);
            }
            4 if out.cpu_socket.is_none() => out.cpu_socket = s(0x04),
            // Use 03h = system memory (not cache or flash arrays).
            16 if byte(0x05) == Some(3) => {
                if let Some(n) = word(0x0D) {
                    slots = Some(slots.unwrap_or(0) + u32::from(n));
                }
            }
            17 => {
                let size_mb = match word(0x0C) {
                    None | Some(0) => {
                        i = next;
                        continue; // empty slot
                    }
                    Some(0xFFFF) => None,
                    Some(0x7FFF) => dword(0x1C).map(|v| u64::from(v & 0x7FFF_FFFF)),
                    Some(v) if v & 0x8000 != 0 => Some(u64::from(v & 0x7FFF) / 1024), // KB
                    Some(v) => Some(u64::from(v)),
                };
                out.modules.push(MemoryModule {
                    locator: s(0x10),
                    size_mb,
                    kind: byte(0x12).and_then(memory_type),
                    form_factor: byte(0x0E).and_then(form_factor),
                    rated_mts: speed(0x15, 0x54),
                    configured_mts: speed(0x20, 0x58),
                    manufacturer: s(0x17),
                    part_number: s(0x1A),
                });
            }
            127 => break,
            _ => {}
        }
        i = next;
    }
    out.memory_slots = slots;
    out
}

/// One structure at `i`: (type, formatted area, strings, offset of the next structure).
fn structure(table: &[u8], i: usize) -> Option<(u8, &[u8], Vec<String>, usize)> {
    let kind = *table.get(i)?;
    let len = *table.get(i + 1)? as usize;
    if len < 4 {
        return None;
    }
    let formatted = table.get(i..i + len)?;
    // The string set follows: NUL-terminated strings closed by an extra NUL,
    // or just two NULs when the structure has no strings.
    let mut j = i + len;
    let mut strings = Vec::new();
    loop {
        let rest = table.get(j..)?;
        let end = rest.iter().position(|&b| b == 0)?;
        if end == 0 {
            j += if strings.is_empty() { 2 } else { 1 };
            break;
        }
        strings.push(String::from_utf8_lossy(&rest[..end]).into_owned());
        j += end + 1;
    }
    Some((kind, formatted, strings, j))
}

// ---------------------------------------------------------------------------
// Spec sheet
// ---------------------------------------------------------------------------

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SpecSection {
    pub title: String,
    pub items: Vec<SpecItem>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SpecItem {
    pub label: String,
    pub value: String,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct DisplayInfo {
    pub name: Option<String>,
    pub width: u32,
    pub height: u32,
    pub refresh_hz: Option<f64>,
}

pub struct SpecsInput<'a> {
    pub system: &'a SystemInfo,
    pub smbios: &'a Smbios,
    /// Total L1, L2 and L3 cache in bytes.
    pub caches: [Option<u64>; 3],
    /// GPU LUID key → driver version.
    pub gpu_drivers: &'a [(String, String)],
    pub displays: &'a [DisplayInfo],
    pub drives: &'a [DriveHealth],
    pub adapters: &'a [NetworkAdapterSnapshot],
}

/// Binary units, the way Windows reports memory: 34359738368 → "32 GB".
fn binary(bytes: u64) -> String {
    const MB: f64 = 1024.0 * 1024.0;
    let b = bytes as f64;
    if b >= 1024.0 * MB {
        trim(format!("{:.1} GB", b / (1024.0 * MB)))
    } else if b >= MB {
        trim(format!("{:.1} MB", b / MB))
    } else {
        format!("{} KB", bytes / 1024)
    }
}

/// Decimal units, the way drive capacity is sold: 2000398934016 → "2.0 TB".
fn decimal(bytes: u64) -> String {
    let b = bytes as f64;
    if b >= 1e12 {
        format!("{:.1} TB", b / 1e12)
    } else {
        format!("{:.0} GB", b / 1e9)
    }
}

fn trim(s: String) -> String {
    s.replace(".0 ", " ")
}

fn join(parts: impl IntoIterator<Item = Option<String>>) -> Option<String> {
    let v: Vec<String> = parts.into_iter().flatten().collect();
    (!v.is_empty()).then(|| v.join(" · "))
}

fn numbered(base: &str, n: usize, count: usize) -> String {
    if count > 1 {
        format!("{base} {n}")
    } else {
        base.to_owned()
    }
}

#[derive(Default)]
struct Section(Vec<SpecItem>);

impl Section {
    fn add(&mut self, label: impl Into<String>, value: Option<String>) {
        if let Some(value) = value {
            self.0.push(SpecItem {
                label: label.into(),
                value,
            });
        }
    }
}

/// Builds the spec sheet. Rows whose value is unknown are omitted, and so are
/// sections left empty.
pub fn sections(i: &SpecsInput) -> Vec<SpecSection> {
    let (sys, sm) = (i.system, i.smbios);
    let mut out = Vec::new();
    let mut push = |title: &str, s: Section| {
        if !s.0.is_empty() {
            out.push(SpecSection {
                title: title.into(),
                items: s.0,
            });
        }
    };

    let mut s = Section::default();
    s.add("Manufacturer", sm.system_manufacturer.clone());
    s.add("Model", sm.system_model.clone());
    s.add("Operating system", sys.os_name.clone());
    s.add("OS build", sys.os_build.clone());
    push("System", s);

    let c = &sys.cpu;
    let mut s = Section::default();
    s.add("Name", c.brand.clone());
    s.add(
        "Cores",
        c.physical_cores
            .map(|n| match (c.performance_cores, c.efficiency_cores) {
                (Some(p), Some(e)) => format!("{n} ({p} performance + {e} efficiency)"),
                _ => n.to_string(),
            }),
    );
    s.add("Threads", c.logical_processors.map(|n| n.to_string()));
    s.add(
        "Base clock",
        c.base_mhz
            .filter(|&m| m > 0.0)
            // The registry value is measured and reads slightly low (3494 for 3.5 GHz); bases are 50 MHz steps.
            .map(|m| format!("{:.2} GHz", (m / 50.0).round() * 50.0 / 1000.0)),
    );
    for (lvl, bytes) in i.caches.iter().enumerate() {
        s.add(format!("L{} cache", lvl + 1), bytes.map(binary));
    }
    s.add("Socket", sm.cpu_socket.clone());
    s.add(
        "Processors",
        c.packages.filter(|&p| p > 1).map(|p| p.to_string()),
    );
    push("Processor", s);

    let mut s = Section::default();
    s.add("Manufacturer", sm.board_manufacturer.clone());
    s.add("Model", sm.board_model.clone());
    s.add("Revision", sm.board_version.clone());
    s.add("BIOS vendor", sm.bios_vendor.clone());
    s.add("BIOS version", sm.bios_version.clone());
    s.add("BIOS date", sm.bios_date.clone());
    push("Motherboard", s);

    let mut s = Section::default();
    let module_mb: u64 = sm.modules.iter().filter_map(|m| m.size_mb).sum();
    let usable = sys.installed_memory_bytes.map(binary);
    s.add(
        "Installed",
        if module_mb > 0 {
            Some(match &usable {
                Some(u) => format!("{} ({u} usable)", binary(module_mb * 1024 * 1024)),
                None => binary(module_mb * 1024 * 1024),
            })
        } else {
            usable
        },
    );
    let used = sm.modules.len();
    s.add(
        "Slots",
        match sm.memory_slots {
            Some(total) if total as usize >= used && used > 0 => {
                Some(format!("{used} of {total} used"))
            }
            _ if used > 0 => Some(format!("{used} modules")),
            _ => None,
        },
    );
    let first = sm.modules.first();
    s.add(
        "Type",
        join([
            first.and_then(|m| m.kind).map(str::to_owned),
            first
                .and_then(|m| m.configured_mts.or(m.rated_mts))
                .map(|v| format!("{v} MT/s")),
        ]),
    );
    for (n, m) in sm.modules.iter().enumerate() {
        let size = m.size_mb.map(|mb| binary(mb * 1024 * 1024));
        let kind = join([
            size,
            m.kind.map(str::to_owned),
            m.form_factor.map(str::to_owned),
        ])
        .map(|v| v.replace(" · ", " "));
        let speed = match (m.configured_mts, m.rated_mts) {
            (Some(c), Some(r)) if c != r => Some(format!("{c} MT/s (rated {r})")),
            (c, r) => c.or(r).map(|v| format!("{v} MT/s")),
        };
        s.add(
            m.locator
                .clone()
                .unwrap_or_else(|| format!("Module {}", n + 1)),
            join([kind, speed, m.manufacturer.clone(), m.part_number.clone()]),
        );
    }
    push("Memory", s);

    let mut s = Section::default();
    let gpus: Vec<_> = sys.gpus.iter().filter(|g| !g.is_software).collect();
    for (n, g) in gpus.iter().enumerate() {
        let label = numbered("GPU", n + 1, gpus.len());
        s.add(label.clone(), Some(g.name.clone()));
        s.add(
            format!("{label} memory"),
            (g.dedicated_memory_bytes > 0).then(|| binary(g.dedicated_memory_bytes)),
        );
        let driver = i.gpu_drivers.iter().find(|(luid, _)| *luid == g.luid);
        s.add(format!("{label} driver"), driver.map(|(_, v)| v.clone()));
    }
    for (n, d) in i.displays.iter().enumerate() {
        s.add(
            numbered("Display", n + 1, i.displays.len()),
            join([
                d.name.clone(),
                Some(format!("{} × {}", d.width, d.height)),
                d.refresh_hz.map(|hz| trim(format!("{hz:.1} Hz"))),
            ]),
        );
    }
    push("Graphics", s);

    let mut s = Section::default();
    for d in i.drives {
        s.add(
            format!("Disk {}", d.index),
            join([
                d.model.clone(),
                d.size_bytes.map(decimal),
                d.bus_type.clone(),
                d.firmware.clone().map(|f| format!("firmware {f}")),
            ]),
        );
    }
    push("Storage", s);

    let mut s = Section::default();
    for a in i.adapters {
        // link_speed_bps is bytes per second, like every rate in the model.
        let speed = a
            .link_speed_bps
            .filter(|_| a.connected)
            .map(|bytes| bytes * 8)
            .map(|b| {
                if b >= 1_000_000_000 {
                    format!("{} Gbit/s", b as f64 / 1e9)
                } else {
                    format!("{} Mbit/s", b / 1_000_000)
                }
            });
        s.add(
            a.alias.clone(),
            join([
                Some(a.description.clone()),
                Some(a.media.clone()),
                speed.or_else(|| (!a.connected).then(|| "not connected".to_owned())),
            ]),
        );
    }
    push("Network", s);

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CpuInfo, GpuInfo};

    fn st(kind: u8, formatted: &[u8], strings: &[&str]) -> Vec<u8> {
        let mut v = vec![kind, (formatted.len() + 4) as u8, 0, 0];
        v.extend_from_slice(formatted);
        if strings.is_empty() {
            v.extend_from_slice(&[0, 0]);
        } else {
            for s in strings {
                v.extend_from_slice(s.as_bytes());
                v.push(0);
            }
            v.push(0);
        }
        v
    }

    /// Type 17 formatted area (after the 4-byte header), version 3.2 layout.
    fn dimm(size: u16, ext_mb: u32, kind: u8, rated: u16, configured: u16) -> Vec<u8> {
        let mut f = vec![0u8; 0x5C - 4];
        let mut put = |o: usize, b: &[u8]| f[o - 4..o - 4 + b.len()].copy_from_slice(b);
        put(0x0C, &size.to_le_bytes());
        put(0x0E, &[0x09]); // DIMM
        put(0x10, &[1]); // locator string
        put(0x12, &[kind]);
        put(0x15, &rated.to_le_bytes());
        put(0x17, &[2]); // manufacturer
        put(0x1A, &[3]); // part number
        put(0x1C, &ext_mb.to_le_bytes());
        put(0x20, &configured.to_le_bytes());
        f
    }

    fn table() -> Vec<u8> {
        let mut t = Vec::new();
        // BIOS: vendor=1, version=2, start segment, date=3
        t.extend(st(
            0,
            &[1, 2, 0, 0, 3],
            &["American Megatrends Inc.", "1820", "07/15/2025"],
        ));
        t.extend(st(
            1,
            &[1, 2],
            &["System manufacturer", "System Product Name"],
        ));
        t.extend(st(
            2,
            &[1, 2, 3],
            &[
                "ASUSTeK COMPUTER INC.",
                "ROG STRIX Z790-E GAMING WIFI",
                "Rev 1.xx",
            ],
        ));
        t.extend(st(4, &[1], &["LGA1700"]));
        // Physical memory array: location, use=3 (system memory), ..., 4 devices at 0x0D
        let mut arr = vec![0u8; 0x0F - 4];
        arr[0x05 - 4] = 3;
        arr[0x0D - 4] = 4;
        t.extend(st(16, &arr, &[]));
        t.extend(st(17, &dimm(0, 0, 0x22, 0, 0), &[])); // empty slot
        t.extend(st(
            17,
            &dimm(16_384, 0, 0x22, 4800, 6000),
            &["DIMM_A2", "Kingston", "KF560C36-16     "],
        ));
        t.extend(st(
            17,
            &dimm(0x7FFF, 32_768, 0x22, 6000, 6000),
            &["DIMM_B2", "Kingston", "KF560C36-32"],
        ));
        t.extend(st(127, &[], &[]));
        t
    }

    #[test]
    fn parses_board_bios_and_modules() {
        let s = parse_smbios(&table());
        assert_eq!(s.bios_vendor.as_deref(), Some("American Megatrends Inc."));
        assert_eq!(s.bios_date.as_deref(), Some("07/15/2025"));
        assert_eq!(
            s.system_manufacturer, None,
            "placeholder strings are dropped"
        );
        assert_eq!(
            s.board_model.as_deref(),
            Some("ROG STRIX Z790-E GAMING WIFI")
        );
        assert_eq!(s.cpu_socket.as_deref(), Some("LGA1700"));
        assert_eq!(s.memory_slots, Some(4));
        assert_eq!(s.modules.len(), 2, "empty slot skipped");
        let m = &s.modules[0];
        assert_eq!(m.size_mb, Some(16_384));
        assert_eq!((m.kind, m.form_factor), (Some("DDR5"), Some("DIMM")));
        assert_eq!((m.rated_mts, m.configured_mts), (Some(4800), Some(6000)));
        assert_eq!(
            m.part_number.as_deref(),
            Some("KF560C36-16"),
            "padding trimmed"
        );
        assert_eq!(s.modules[1].size_mb, Some(32_768), "extended size field");
    }

    #[test]
    fn truncated_table_keeps_what_was_read() {
        let t = table();
        let s = parse_smbios(&t[..t.len() - 30]);
        assert_eq!(s.bios_version.as_deref(), Some("1820"));
        assert_eq!(s.modules.len(), 1);
        assert_eq!(parse_smbios(&[]), Smbios::default());
    }

    #[test]
    fn spec_sheet_rows() {
        let system = SystemInfo {
            os_name: Some("Windows 11 Home 25H2".into()),
            installed_memory_bytes: Some(51_000_000_000),
            cpu: CpuInfo {
                brand: Some("Intel(R) Core(TM) i9-14900K".into()),
                physical_cores: Some(24),
                logical_processors: Some(32),
                performance_cores: Some(8),
                efficiency_cores: Some(16),
                base_mhz: Some(3194.0),
                ..Default::default()
            },
            gpus: vec![GpuInfo {
                luid: "L1".into(),
                name: "NVIDIA GeForce RTX 4070".into(),
                dedicated_memory_bytes: 12 << 30,
                ..Default::default()
            }],
            ..Default::default()
        };
        let smbios = parse_smbios(&table());
        let displays = [DisplayInfo {
            name: Some("LG ULTRAGEAR".into()),
            width: 2560,
            height: 1440,
            refresh_hz: Some(164.999),
        }];
        let out = sections(&SpecsInput {
            system: &system,
            smbios: &smbios,
            caches: [Some(2_176 << 10), Some(32 << 20), Some(36 << 20)],
            gpu_drivers: &[("L1".into(), "32.0.15.6094".into())],
            displays: &displays,
            drives: &[],
            adapters: &[NetworkAdapterSnapshot {
                alias: "Ethernet 2".into(),
                description: "Intel(R) Ethernet Controller I226-V".into(),
                media: "Ethernet".into(),
                connected: true,
                link_speed_bps: Some(312_500_000),
                ..Default::default()
            }],
        });
        let get = |sec: &str, label: &str| {
            out.iter()
                .find(|s| s.title == sec)
                .and_then(|s| s.items.iter().find(|i| i.label == label))
                .map(|i| i.value.as_str())
        };
        assert_eq!(get("System", "Manufacturer"), None);
        assert_eq!(
            get("Processor", "Cores"),
            Some("24 (8 performance + 16 efficiency)")
        );
        assert_eq!(get("Processor", "Base clock"), Some("3.20 GHz"));
        assert_eq!(
            get("Network", "Ethernet 2"),
            Some("Intel(R) Ethernet Controller I226-V · Ethernet · 2.5 Gbit/s")
        );
        assert_eq!(get("Processor", "L3 cache"), Some("36 MB"));
        assert_eq!(get("Processor", "L1 cache"), Some("2.1 MB"));
        assert_eq!(get("Memory", "Installed"), Some("48 GB (47.5 GB usable)"));
        assert_eq!(get("Memory", "Slots"), Some("2 of 4 used"));
        assert_eq!(get("Memory", "Type"), Some("DDR5 · 6000 MT/s"));
        assert_eq!(
            get("Memory", "DIMM_A2"),
            Some("16 GB DDR5 DIMM · 6000 MT/s (rated 4800) · Kingston · KF560C36-16")
        );
        assert_eq!(get("Graphics", "GPU"), Some("NVIDIA GeForce RTX 4070"));
        assert_eq!(get("Graphics", "GPU memory"), Some("12 GB"));
        assert_eq!(get("Graphics", "GPU driver"), Some("32.0.15.6094"));
        assert_eq!(
            get("Graphics", "Display"),
            Some("LG ULTRAGEAR · 2560 × 1440 · 165 Hz")
        );
        assert!(
            out.iter().all(|s| s.title != "Storage"),
            "empty sections are omitted"
        );
    }
}
