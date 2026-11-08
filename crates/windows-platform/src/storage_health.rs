//! Physical drive identity, temperature and NVMe SMART / Health information
//! through `IOCTL_STORAGE_QUERY_PROPERTY` — the storage stack's documented
//! query interface; no vendor tools or drivers.
//!
//! Each drive is opened with *no* data access rights (query only), which works
//! without elevation for identity and, on most systems, temperature. Whether
//! the NVMe health log page is returned to a non-elevated caller depends on the
//! driver; a refusal is reported as "Permission required". SATA SMART attributes
//! need ATA pass-through, which requires elevation and is not implemented.

use pch_core::{Availability, DriveHealth, NvmeHealth, Status};
use windows::Win32::Foundation::GENERIC_ACCESS_RIGHTS;
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows::Win32::System::Ioctl::{
    IOCTL_DISK_GET_DRIVE_GEOMETRY_EX, IOCTL_STORAGE_QUERY_PROPERTY,
};
use windows::Win32::System::IO::DeviceIoControl;

use crate::handle::{wide, OwnedHandle};

// winioctl.h STORAGE_PROPERTY_ID / STORAGE_QUERY_TYPE / STORAGE_PROTOCOL_* values.
const STORAGE_DEVICE_PROPERTY: u32 = 0;
const STORAGE_DEVICE_PROTOCOL_SPECIFIC_PROPERTY: u32 = 50;
const STORAGE_DEVICE_TEMPERATURE_PROPERTY: u32 = 52;
const PROPERTY_STANDARD_QUERY: u32 = 0;
const PROTOCOL_TYPE_NVME: u32 = 3;
const NVME_DATA_TYPE_LOG_PAGE: u32 = 2;
/// NVMe Base Specification: Log Identifier 02h = SMART / Health Information, 512 bytes.
const NVME_LOG_HEALTH: u32 = 2;
const NVME_HEALTH_LEN: usize = 512;
/// sizeof(STORAGE_PROTOCOL_SPECIFIC_DATA): ten 32-bit fields.
const PROTOCOL_SPECIFIC_DATA_LEN: usize = 40;
/// FIELD_OFFSET(STORAGE_PROPERTY_QUERY, AdditionalParameters).
const QUERY_HEADER_LEN: usize = 8;
const STORAGE_BUS_TYPE_NVME: u32 = 17;
const MAX_DRIVES: u32 = 32;

fn u16_at(b: &[u8], o: usize) -> Option<u16> {
    b.get(o..o + 2).map(|s| u16::from_le_bytes([s[0], s[1]]))
}
fn i16_at(b: &[u8], o: usize) -> Option<i16> {
    b.get(o..o + 2).map(|s| i16::from_le_bytes([s[0], s[1]]))
}
fn u32_at(b: &[u8], o: usize) -> Option<u32> {
    b.get(o..o + 4)
        .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}
fn u128_at(b: &[u8], o: usize) -> u128 {
    b.get(o..o + 16)
        .map(|s| {
            let mut a = [0u8; 16];
            a.copy_from_slice(s);
            u128::from_le_bytes(a)
        })
        .unwrap_or(0)
}
fn cstr_at(b: &[u8], off: u32) -> Option<String> {
    if off == 0 {
        return None;
    }
    let s = b.get(off as usize..)?;
    let end = s.iter().position(|&c| c == 0).unwrap_or(s.len());
    let v = String::from_utf8_lossy(&s[..end]).trim().to_owned();
    (!v.is_empty()).then_some(v)
}

pub fn bus_type_name(bus: u32) -> &'static str {
    // winioctl.h STORAGE_BUS_TYPE
    match bus {
        1 => "SCSI",
        2 => "ATAPI",
        3 => "ATA",
        4 => "IEEE 1394",
        7 => "USB",
        8 => "RAID",
        9 => "iSCSI",
        10 => "SAS",
        11 => "SATA",
        12 => "SD",
        13 => "MMC",
        14 | 15 => "Virtual",
        16 => "Storage Spaces",
        17 => "NVMe",
        18 => "SCM",
        19 => "UFS",
        _ => "Unknown",
    }
}

#[derive(Debug, Default, PartialEq)]
pub struct DeviceIdentity {
    pub vendor: Option<String>,
    pub product: Option<String>,
    pub revision: Option<String>,
    pub serial: Option<String>,
    pub bus_type: u32,
}

/// Parses a STORAGE_DEVICE_DESCRIPTOR (offsets: winioctl.h layout, strings ASCII at the given offsets).
pub fn parse_device_descriptor(b: &[u8]) -> Option<DeviceIdentity> {
    Some(DeviceIdentity {
        vendor: cstr_at(b, u32_at(b, 12)?),
        product: cstr_at(b, u32_at(b, 16)?),
        revision: cstr_at(b, u32_at(b, 20)?),
        serial: cstr_at(b, u32_at(b, 24)?),
        bus_type: u32_at(b, 28)?,
    })
}

/// Parses the 512-byte NVMe SMART / Health Information log page
/// (NVMe Base Specification 1.4, Figure 194). Temperatures are kelvin on the wire.
pub fn parse_nvme_health(p: &[u8]) -> Option<NvmeHealth> {
    if p.len() < NVME_HEALTH_LEN {
        return None;
    }
    let kelvin = u16_at(p, 1)?;
    Some(NvmeHealth {
        critical_warning: p[0],
        composite_temperature_c: (kelvin > 0).then(|| f32::from(kelvin) - 273.15),
        available_spare_pct: p[3],
        available_spare_threshold_pct: p[4],
        percentage_used: p[5],
        data_units_read: u128_at(p, 32),
        data_units_written: u128_at(p, 48),
        power_cycles: u128_at(p, 112),
        power_on_hours: u128_at(p, 128),
        unsafe_shutdowns: u128_at(p, 144),
        media_errors: u128_at(p, 160),
        error_log_entries: u128_at(p, 176),
        warning_temp_minutes: u32_at(p, 192)?,
        critical_temp_minutes: u32_at(p, 196)?,
    })
}

/// Parses STORAGE_TEMPERATURE_DATA_DESCRIPTOR: (current of info[0], warning, critical), °C.
pub fn parse_temperature_descriptor(b: &[u8]) -> Option<(Option<f32>, Option<f32>, Option<f32>)> {
    let critical = i16_at(b, 8)?;
    let warning = i16_at(b, 10)?;
    let count = u16_at(b, 12)?;
    // TemperatureInfo[0] starts at offset 24; its Temperature field is at +2.
    let current = if count > 0 { i16_at(b, 24 + 2) } else { None };
    let valid = |t: i16| (t > -40 && t < 150).then_some(f32::from(t));
    Some((current.and_then(valid), valid(warning), valid(critical)))
}

fn open_drive(index: u32) -> Result<OwnedHandle, windows::core::Error> {
    let path = wide(&format!(r"\\.\PhysicalDrive{index}"));
    // SAFETY: NUL-terminated path; 0 desired access = metadata/IOCTL query only.
    let h = unsafe {
        CreateFileW(
            windows::core::PCWSTR(path.as_ptr()),
            GENERIC_ACCESS_RIGHTS(0).0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            FILE_FLAGS_AND_ATTRIBUTES(0),
            None,
        )
    }?;
    Ok(OwnedHandle(h))
}

fn ioctl(
    h: &OwnedHandle,
    code: u32,
    input: &[u8],
    out_len: usize,
) -> Result<Vec<u8>, windows::core::Error> {
    let mut out = vec![0u8; out_len];
    let mut returned = 0u32;
    // SAFETY: input/output buffers sized as passed; synchronous call.
    unsafe {
        DeviceIoControl(
            h.raw(),
            code,
            (!input.is_empty()).then_some(input.as_ptr().cast()),
            input.len() as u32,
            Some(out.as_mut_ptr().cast()),
            out.len() as u32,
            Some(&mut returned),
            None,
        )
    }?;
    out.truncate(returned as usize);
    Ok(out)
}

fn property_query(id: u32) -> Vec<u8> {
    let mut q = vec![0u8; QUERY_HEADER_LEN + 4];
    q[0..4].copy_from_slice(&id.to_le_bytes());
    q[4..8].copy_from_slice(&PROPERTY_STANDARD_QUERY.to_le_bytes());
    q
}

fn nvme_health(h: &OwnedHandle) -> Result<NvmeHealth, windows::core::Error> {
    let len = QUERY_HEADER_LEN + PROTOCOL_SPECIFIC_DATA_LEN + NVME_HEALTH_LEN;
    let mut q = vec![0u8; len];
    q[0..4].copy_from_slice(&STORAGE_DEVICE_PROTOCOL_SPECIFIC_PROPERTY.to_le_bytes());
    q[4..8].copy_from_slice(&PROPERTY_STANDARD_QUERY.to_le_bytes());
    let fields = [
        PROTOCOL_TYPE_NVME,
        NVME_DATA_TYPE_LOG_PAGE,
        NVME_LOG_HEALTH,
        0,
        PROTOCOL_SPECIFIC_DATA_LEN as u32,
        NVME_HEALTH_LEN as u32,
    ];
    for (i, v) in fields.iter().enumerate() {
        let o = QUERY_HEADER_LEN + i * 4;
        q[o..o + 4].copy_from_slice(&v.to_le_bytes());
    }
    let out = ioctl(h, IOCTL_STORAGE_QUERY_PROPERTY, &q, len)?;
    // Output: STORAGE_PROTOCOL_DATA_DESCRIPTOR {Version, Size, ProtocolSpecificData}; data offset is relative to ProtocolSpecificData.
    let data_off = u32_at(&out, 8 + 16).unwrap_or(0) as usize;
    let data_len = u32_at(&out, 8 + 20).unwrap_or(0) as usize;
    let page = out.get(8 + data_off..8 + data_off + data_len.min(NVME_HEALTH_LEN));
    page.and_then(parse_nvme_health).ok_or_else(|| {
        windows::core::Error::new(
            windows::core::HRESULT(0x8007_000Du32 as i32),
            "short NVMe health page",
        )
    })
}

fn is_access_denied(e: &windows::core::Error) -> bool {
    e.code().0 as u32 == 0x8007_0005
}

/// Reads identity, temperature and NVMe health of every physical drive.
pub fn read_all() -> Vec<DriveHealth> {
    let mut out = Vec::new();
    for index in 0..MAX_DRIVES {
        let h = match open_drive(index) {
            Ok(h) => h,
            Err(_) => continue,
        };
        let mut d = DriveHealth {
            index,
            status: Status::available(),
            ..Default::default()
        };
        match ioctl(
            &h,
            IOCTL_STORAGE_QUERY_PROPERTY,
            &property_query(STORAGE_DEVICE_PROPERTY),
            1024,
        ) {
            Ok(b) => {
                if let Some(id) = parse_device_descriptor(&b) {
                    d.model = match (id.vendor, id.product) {
                        (Some(v), Some(p)) if !p.starts_with(&v) => Some(format!("{v} {p}")),
                        (_, Some(p)) => Some(p),
                        (v, None) => v,
                    };
                    d.firmware = id.revision;
                    d.serial = id.serial;
                    d.bus_type = Some(bus_type_name(id.bus_type).into());
                    if id.bus_type == STORAGE_BUS_TYPE_NVME {
                        match nvme_health(&h) {
                            Ok(n) => {
                                d.health_status = Status::available();
                                d.nvme = Some(n);
                            }
                            Err(e) if is_access_denied(&e) => {
                                d.health_status = Status::new(
                                    Availability::PermissionRequired,
                                    "The NVMe health log needs administrator rights on this system",
                                )
                            }
                            Err(e) => {
                                d.health_status = Status::new(
                                    Availability::Unsupported,
                                    format!("NVMe health log not returned: {e}"),
                                )
                            }
                        }
                    } else {
                        d.health_status = Status::new(
                            Availability::Unsupported,
                            "Health data is read from NVMe drives only; SMART attributes of SATA and USB drives are not read by this version",
                        );
                    }
                }
            }
            Err(e) => d.status = Status::error(format!("Device query failed: {e}")),
        }
        if let Ok(g) = ioctl(&h, IOCTL_DISK_GET_DRIVE_GEOMETRY_EX, &[], 256) {
            // DISK_GEOMETRY_EX: DISK_GEOMETRY (24 bytes) then DiskSize i64.
            if g.len() >= 32 {
                let mut a = [0u8; 8];
                a.copy_from_slice(&g[24..32]);
                d.size_bytes = u64::try_from(i64::from_le_bytes(a)).ok();
            }
        }
        if let Ok(t) = ioctl(
            &h,
            IOCTL_STORAGE_QUERY_PROPERTY,
            &property_query(STORAGE_DEVICE_TEMPERATURE_PROPERTY),
            256,
        ) {
            if let Some((cur, warn, crit)) = parse_temperature_descriptor(&t) {
                d.temperature_c = cur;
                d.temperature_warning_c = warn;
                d.temperature_critical_c = crit;
            }
        }
        if d.temperature_c.is_none() {
            d.temperature_c = d.nvme.as_ref().and_then(|n| n.composite_temperature_c);
        }
        out.push(d);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nvme_page_fields() {
        let mut p = vec![0u8; 512];
        p[0] = 0x04;
        p[1..3].copy_from_slice(&313u16.to_le_bytes()); // 39.85 °C
        p[3] = 100;
        p[4] = 10;
        p[5] = 3;
        p[128..144].copy_from_slice(&1234u128.to_le_bytes());
        p[160..176].copy_from_slice(&2u128.to_le_bytes());
        p[192..196].copy_from_slice(&7u32.to_le_bytes());
        let h = parse_nvme_health(&p).expect("parse");
        assert_eq!(h.critical_warning, 4);
        assert!((h.composite_temperature_c.expect("temp") - 39.85).abs() < 0.01);
        assert_eq!(h.percentage_used, 3);
        assert_eq!(h.power_on_hours, 1234);
        assert_eq!(h.media_errors, 2);
        assert_eq!(h.warning_temp_minutes, 7);
        assert!(parse_nvme_health(&p[..511]).is_none());
    }

    #[test]
    fn device_descriptor_strings() {
        let mut b = vec![0u8; 128];
        b[16..20].copy_from_slice(&64u32.to_le_bytes());
        b[20..24].copy_from_slice(&80u32.to_le_bytes());
        b[28..32].copy_from_slice(&17u32.to_le_bytes());
        b[64..75].copy_from_slice(b"Samsung 990");
        b[80..85].copy_from_slice(b"4B2QJ");
        let id = parse_device_descriptor(&b).expect("parse");
        assert_eq!(id.product.as_deref(), Some("Samsung 990"));
        assert_eq!(id.revision.as_deref(), Some("4B2QJ"));
        assert_eq!(id.vendor, None);
        assert_eq!(bus_type_name(id.bus_type), "NVMe");
        assert!(parse_device_descriptor(&b[..10]).is_none());
    }

    #[test]
    fn temperature_descriptor() {
        let mut b = vec![0u8; 40];
        b[8..10].copy_from_slice(&84i16.to_le_bytes());
        b[10..12].copy_from_slice(&75i16.to_le_bytes());
        b[12..14].copy_from_slice(&1u16.to_le_bytes());
        b[26..28].copy_from_slice(&41i16.to_le_bytes());
        assert_eq!(
            parse_temperature_descriptor(&b),
            Some((Some(41.0), Some(75.0), Some(84.0)))
        );
        b[12..14].copy_from_slice(&0u16.to_le_bytes());
        assert_eq!(parse_temperature_descriptor(&b).map(|t| t.0), Some(None));
    }

    #[test]
    fn reads_drives_on_this_machine() {
        let drives = read_all();
        assert!(!drives.is_empty(), "at least the system drive");
        assert!(drives.iter().any(|d| d.model.is_some()));
    }
}
