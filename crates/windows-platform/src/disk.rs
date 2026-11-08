//! Volumes (capacity) and physical-disk activity (PDH `PhysicalDisk`).

use veyl_core::{DisksSnapshot, PhysicalDiskActivity, Status, VolumeSnapshot};
use windows::Win32::Storage::FileSystem::{
    GetDiskFreeSpaceExW, GetDriveTypeW, GetLogicalDriveStringsW, GetVolumeInformationW,
};

use crate::handle::{from_wide, wide};
use crate::pdh::{Counter, PdhError, PdhQuery};

/// winbase.h DRIVE_* values returned by GetDriveTypeW.
fn drive_type_name(t: u32) -> &'static str {
    match t {
        2 => "removable",
        3 => "fixed",
        4 => "network",
        5 => "optical",
        6 => "ramdisk",
        _ => "unknown",
    }
}

/// Mounted drive letters with their capacity. Optical drives without media and
/// unreachable network drives report `None` sizes instead of zero.
pub fn volumes() -> Vec<VolumeSnapshot> {
    let mut buf = vec![0u16; 512];
    // SAFETY: buffer passed as a slice.
    let n = unsafe { GetLogicalDriveStringsW(Some(&mut buf)) } as usize;
    if n == 0 || n > buf.len() {
        return Vec::new();
    }
    buf[..n]
        .split(|&c| c == 0)
        .filter(|s| !s.is_empty())
        .map(|root| {
            let mount = String::from_utf16_lossy(root);
            let w = wide(&mount);
            let path = windows::core::PCWSTR(w.as_ptr());
            // SAFETY: NUL-terminated root path.
            let kind = unsafe { GetDriveTypeW(path) };
            let mut v = VolumeSnapshot {
                mount: mount.clone(),
                drive_type: drive_type_name(kind).into(),
                ..Default::default()
            };
            // Network drives can block for seconds when the server is unreachable; capacity is only read for local media.
            if matches!(kind, 2 | 3 | 6) {
                let (mut free, mut total) = (0u64, 0u64);
                // SAFETY: out-pointers to locals.
                if unsafe { GetDiskFreeSpaceExW(path, Some(&mut free), Some(&mut total), None) }
                    .is_ok()
                {
                    v.free_bytes = Some(free);
                    v.total_bytes = Some(total);
                }
                let mut label = [0u16; 261];
                let mut fs = [0u16; 261];
                // SAFETY: buffers passed as slices.
                if unsafe {
                    GetVolumeInformationW(path, Some(&mut label), None, None, None, Some(&mut fs))
                }
                .is_ok()
                {
                    v.label = Some(from_wide(&label)).filter(|s| !s.is_empty());
                    v.file_system = Some(from_wide(&fs)).filter(|s| !s.is_empty());
                }
            }
            v
        })
        .collect()
}

/// Parses a `PhysicalDisk` instance name such as `0 C: D:` into its disk index.
pub fn parse_disk_index(instance: &str) -> Option<u32> {
    instance.split_whitespace().next()?.parse().ok()
}

struct Counters {
    query: PdhQuery,
    read: Counter,
    write: Counter,
    idle: Counter,
    queue: Counter,
}

pub struct DiskSampler {
    counters: Result<Counters, PdhError>,
    volumes: Vec<VolumeSnapshot>,
    ticks: u32,
}

/// Volume capacity changes slowly; refresh it every N samples.
const VOLUME_REFRESH_TICKS: u32 = 10;

impl DiskSampler {
    pub fn new() -> Self {
        let counters = (|| {
            let mut query = PdhQuery::open()?;
            let c = Counters {
                read: query.add(r"\PhysicalDisk(*)\Disk Read Bytes/sec")?,
                write: query.add(r"\PhysicalDisk(*)\Disk Write Bytes/sec")?,
                idle: query.add(r"\PhysicalDisk(*)\% Idle Time")?,
                queue: query.add(r"\PhysicalDisk(*)\Current Disk Queue Length")?,
                query,
            };
            Ok(c)
        })();
        let mut s = Self {
            counters,
            volumes: volumes(),
            ticks: 0,
        };
        if let Ok(c) = &mut s.counters {
            if let Err(e) = c.query.collect() {
                s.counters = Err(e);
            }
        }
        s
    }

    pub fn sample(&mut self) -> DisksSnapshot {
        self.ticks = self.ticks.wrapping_add(1);
        if self.ticks % VOLUME_REFRESH_TICKS == 0 {
            self.volumes = volumes();
        }
        let mut snap = DisksSnapshot {
            volumes: self.volumes.clone(),
            ..Default::default()
        };
        let c = match &mut self.counters {
            Ok(c) => c,
            Err(e) => {
                snap.status = Status::error(format!("Disk counters unavailable: {e}"));
                return snap;
            }
        };
        if let Err(e) = c.query.collect() {
            snap.status = Status::error(format!("Disk counters: {e}"));
            return snap;
        }
        if !c.query.primed() {
            snap.status = Status::waiting("Waiting for the second disk sample");
            return snap;
        }
        let (read, write, idle, queue) = match (
            c.query.array(c.read, true),
            c.query.array(c.write, true),
            c.query.array(c.idle, true),
            c.query.array(c.queue, true),
        ) {
            (Ok(a), Ok(b), Ok(i), Ok(q)) => (a, b, i, q),
            (Err(e), ..) | (_, Err(e), ..) | (_, _, Err(e), _) | (.., Err(e)) => {
                snap.status = Status::error(format!("Disk counters: {e}"));
                return snap;
            }
        };
        let find = |items: &[(String, f64)], name: &str| {
            items.iter().find(|(n, _)| n == name).map(|(_, v)| *v)
        };
        for (name, r) in &read {
            if name == "_Total" {
                snap.total_read_bps = Some(*r);
                snap.total_write_bps = find(&write, name);
                continue;
            }
            snap.physical.push(PhysicalDiskActivity {
                name: name.clone(),
                index: parse_disk_index(name),
                read_bps: Some(*r),
                write_bps: find(&write, name),
                active_pct: find(&idle, name).map(|i| (100.0 - i).clamp(0.0, 100.0) as f32),
                queue_length: find(&queue, name).map(|q| q as f32),
            });
        }
        snap.physical.sort_by_key(|d| d.index.unwrap_or(u32::MAX));
        snap.status = Status::available();
        snap
    }
}

impl Default for DiskSampler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_physical_disk_instances() {
        assert_eq!(parse_disk_index("0 C: D:"), Some(0));
        assert_eq!(parse_disk_index("12"), Some(12));
        assert_eq!(parse_disk_index("_Total"), None);
        assert_eq!(parse_disk_index(""), None);
    }

    #[test]
    fn lists_system_volume() {
        let v = volumes();
        let c = v
            .iter()
            .find(|v| v.mount.eq_ignore_ascii_case("C:\\"))
            .expect("C:");
        assert_eq!(c.drive_type, "fixed");
        assert!(
            c.total_bytes.unwrap_or(0)
                > c.free_bytes
                    .unwrap_or(u64::MAX)
                    .min(c.total_bytes.unwrap_or(0))
                || c.free_bytes == c.total_bytes
        );
    }
}
