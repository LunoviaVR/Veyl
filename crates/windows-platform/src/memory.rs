//! Physical and committed memory.

use veyl_core::{MemorySnapshot, Status};
use windows::Win32::System::ProcessStatus::{GetPerformanceInfo, PERFORMANCE_INFORMATION};
use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};

use crate::pdh::{Counter, PdhError, PdhQuery};

pub struct MemorySampler {
    pages_input: Result<(PdhQuery, Counter), PdhError>,
}

/// Installed RAM as reported by `GlobalMemoryStatusEx` (usable by Windows; firmware-reserved memory excluded).
pub fn total_physical_bytes() -> Option<u64> {
    let mut m = MEMORYSTATUSEX {
        dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
        ..Default::default()
    };
    // SAFETY: dwLength set, out-pointer to a local.
    unsafe { GlobalMemoryStatusEx(&mut m) }
        .ok()
        .map(|_| m.ullTotalPhys)
}

impl MemorySampler {
    pub fn new() -> Self {
        let pages_input = (|| {
            let mut q = PdhQuery::open()?;
            let c = q.add(r"\Memory\Pages Input/sec")?;
            q.collect()?;
            Ok((q, c))
        })();
        Self { pages_input }
    }

    pub fn sample(&mut self) -> MemorySnapshot {
        let mut snap = MemorySnapshot::default();
        let mut m = MEMORYSTATUSEX {
            dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
            ..Default::default()
        };
        // SAFETY: dwLength set, out-pointer to a local.
        if let Err(e) = unsafe { GlobalMemoryStatusEx(&mut m) } {
            snap.status = Status::error(format!("GlobalMemoryStatusEx failed: {e}"));
            return snap;
        }
        snap.total_bytes = Some(m.ullTotalPhys);
        snap.available_bytes = Some(m.ullAvailPhys);
        snap.used_bytes = Some(m.ullTotalPhys.saturating_sub(m.ullAvailPhys));
        snap.load_pct = (m.ullTotalPhys > 0).then(|| {
            (m.ullTotalPhys - m.ullAvailPhys.min(m.ullTotalPhys)) as f32 / m.ullTotalPhys as f32
                * 100.0
        });

        let mut p = PERFORMANCE_INFORMATION {
            cb: std::mem::size_of::<PERFORMANCE_INFORMATION>() as u32,
            ..Default::default()
        };
        // SAFETY: cb set, size passed.
        match unsafe { GetPerformanceInfo(&mut p, p.cb) } {
            Ok(()) => {
                let page = p.PageSize as u64;
                snap.commit_bytes = Some(p.CommitTotal as u64 * page);
                snap.commit_limit_bytes = Some(p.CommitLimit as u64 * page);
                snap.cached_bytes = Some(p.SystemCache as u64 * page);
                snap.paged_pool_bytes = Some(p.KernelPaged as u64 * page);
                snap.nonpaged_pool_bytes = Some(p.KernelNonpaged as u64 * page);
                snap.status = Status::available();
            }
            Err(e) => {
                snap.status = Status::error(format!(
                    "GetPerformanceInfo failed: {e}; commit and pool sizes unavailable"
                ))
            }
        }
        if let Ok((q, c)) = &mut self.pages_input {
            if q.collect().is_ok() {
                snap.pages_input_per_sec = q.value(*c, true).ok().flatten().map(|v| v as f32);
            }
        }
        snap
    }
}

impl Default for MemorySampler {
    fn default() -> Self {
        Self::new()
    }
}
