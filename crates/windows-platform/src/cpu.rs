//! CPU identification and utilization/frequency sampling.
//!
//! Utilization and frequency come from the `Processor Information` performance
//! counter object (one instance per logical processor, named `group,index`):
//! - `% Processor Time`: time-based busy share.
//! - `% Processor Utility`: busy share scaled by the actual frequency relative to
//!   nominal; what Task Manager displays. Can exceed 100 under boost.
//! - `% Processor Performance` × `Processor Frequency` / 100 = effective MHz.

use std::arch::x86_64::__cpuid;

use veyl_core::{CpuInfo, CpuSnapshot, Status};
use windows::Win32::System::ProcessStatus::{GetPerformanceInfo, PERFORMANCE_INFORMATION};
use windows::Win32::System::SystemInformation::{
    GetLogicalProcessorInformationEx, GetTickCount64, RelationAll, RelationCache,
    RelationProcessorCore, RelationProcessorPackage, SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX,
};

use crate::pdh::{Counter, PdhError, PdhQuery};

/// CPU brand and vendor from CPUID leaves 0 and 0x80000002-4 (Intel SDM Vol. 2A, CPUID).
fn cpuid_strings() -> (Option<String>, Option<String>) {
    // CPUID is available on every x86-64 processor (safe intrinsic since Rust 1.87).
    let leaf0 = __cpuid(0);
    let mut vendor = Vec::with_capacity(12);
    for r in [leaf0.ebx, leaf0.edx, leaf0.ecx] {
        vendor.extend_from_slice(&r.to_le_bytes());
    }
    let max_ext = __cpuid(0x8000_0000).eax;
    let brand = (max_ext >= 0x8000_0004).then(|| {
        let mut b = Vec::with_capacity(48);
        for leaf in 0x8000_0002u32..=0x8000_0004 {
            let r = __cpuid(leaf);
            for v in [r.eax, r.ebx, r.ecx, r.edx] {
                b.extend_from_slice(&v.to_le_bytes());
            }
        }
        String::from_utf8_lossy(&b)
            .trim_matches(char::from(0))
            .trim()
            .to_owned()
    });
    let vendor = String::from_utf8_lossy(&vendor).trim().to_owned();
    (
        brand.filter(|s| !s.is_empty()),
        (!vendor.is_empty()).then_some(vendor),
    )
}

#[derive(Default, Debug, PartialEq, Eq)]
struct Topology {
    cores: u32,
    logical: u32,
    packages: u32,
    /// Number of cores per efficiency class.
    classes: std::collections::BTreeMap<u8, u32>,
    /// Total bytes of L1 (data + instruction), L2 and L3 cache.
    caches: [u64; 3],
}

fn topology() -> Result<Topology, String> {
    let mut len = 0u32;
    // SAFETY: size query.
    let _ = unsafe { GetLogicalProcessorInformationEx(RelationAll, None, &mut len) };
    if len == 0 {
        return Err("GetLogicalProcessorInformationEx returned no size".into());
    }
    let mut buf = vec![0u64; (len as usize).div_ceil(8)];
    // SAFETY: buffer of `len` bytes, 8-byte aligned.
    unsafe {
        GetLogicalProcessorInformationEx(RelationAll, Some(buf.as_mut_ptr().cast()), &mut len)
    }
    .map_err(|e| format!("GetLogicalProcessorInformationEx failed: {e}"))?;
    let bytes = len as usize;
    let base = buf.as_ptr().cast::<u8>();
    let mut off = 0usize;
    let mut t = Topology::default();
    let header = std::mem::size_of::<u32>() * 2;
    while off + header <= bytes {
        // SAFETY: `off + header` is within the written region; entries are variable-size and 8-aligned.
        let entry = unsafe {
            &*base
                .add(off)
                .cast::<SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX>()
        };
        let size = entry.Size as usize;
        if size == 0 || off + size > bytes {
            break;
        }
        if entry.Relationship == RelationProcessorCore {
            // SAFETY: relationship selects the Processor member.
            let p = unsafe { &entry.Anonymous.Processor };
            t.cores += 1;
            *t.classes.entry(p.EfficiencyClass).or_insert(0) += 1;
            let groups = p.GroupCount as usize;
            let masks = p.GroupMask.as_ptr();
            for g in 0..groups {
                // SAFETY: GroupCount masks follow inside this entry (`size` bytes).
                t.logical += unsafe { (*masks.add(g)).Mask }.count_ones();
            }
        } else if entry.Relationship == RelationProcessorPackage {
            t.packages += 1;
        } else if entry.Relationship == RelationCache {
            // SAFETY: relationship selects the Cache member.
            let c = unsafe { &entry.Anonymous.Cache };
            if let Some(total) = t.caches.get_mut(usize::from(c.Level).wrapping_sub(1)) {
                *total += u64::from(c.CacheSize);
            }
        }
        off += size;
    }
    Ok(t)
}

/// Total L1, L2 and L3 cache in bytes, as Task Manager sums them.
pub fn cache_sizes() -> [Option<u64>; 3] {
    topology().map_or([None; 3], |t| t.caches.map(|b| (b > 0).then_some(b)))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoreKind {
    Performance,
    Efficiency,
    Uniform,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CoreSlot {
    pub group: u16,
    /// Lowest logical-processor bit of the core within its group.
    pub first_index: u32,
    pub kind: CoreKind,
}

/// One entry per physical core with its first logical processor, in firmware order.
pub fn core_list() -> Result<Vec<CoreSlot>, String> {
    let mut len = 0u32;
    // SAFETY: size query.
    let _ = unsafe { GetLogicalProcessorInformationEx(RelationProcessorCore, None, &mut len) };
    if len == 0 {
        return Err("GetLogicalProcessorInformationEx returned no size".into());
    }
    let mut buf = vec![0u64; (len as usize).div_ceil(8)];
    // SAFETY: buffer of `len` bytes, 8-byte aligned.
    unsafe {
        GetLogicalProcessorInformationEx(
            RelationProcessorCore,
            Some(buf.as_mut_ptr().cast()),
            &mut len,
        )
    }
    .map_err(|e| format!("GetLogicalProcessorInformationEx failed: {e}"))?;
    let base = buf.as_ptr().cast::<u8>();
    let mut raw = Vec::new();
    let mut off = 0usize;
    while off + 8 <= len as usize {
        // SAFETY: within the written region; entries are variable-size.
        let e = unsafe {
            &*base
                .add(off)
                .cast::<SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX>()
        };
        let size = e.Size as usize;
        if size == 0 || off + size > len as usize {
            break;
        }
        // SAFETY: RelationProcessorCore entries hold the Processor member with at least one group mask.
        let p = unsafe { &e.Anonymous.Processor };
        let m = p.GroupMask[0];
        if m.Mask != 0 {
            raw.push((m.Group, m.Mask.trailing_zeros(), p.EfficiencyClass));
        }
        off += size;
    }
    let max = raw.iter().map(|r| r.2).max().unwrap_or(0);
    let hybrid = raw.iter().any(|r| r.2 != max);
    Ok(raw
        .into_iter()
        .map(|(group, first_index, class)| CoreSlot {
            group,
            first_index,
            kind: if !hybrid {
                CoreKind::Uniform
            } else if class == max {
                CoreKind::Performance
            } else {
                CoreKind::Efficiency
            },
        })
        .collect())
}

pub fn cpu_info() -> (CpuInfo, Vec<String>) {
    let (brand, vendor) = cpuid_strings();
    let mut status = Vec::new();
    let mut info = CpuInfo {
        brand,
        vendor,
        // Nominal (base) clock as the firmware reports it to Windows.
        base_mhz: crate::registry::read_dword(
            crate::registry::HKEY_LOCAL_MACHINE,
            r"HARDWARE\DESCRIPTION\System\CentralProcessor\0",
            "~MHz",
        )
        .filter(|&m| m > 0)
        .map(|m| m as f32),
        ..Default::default()
    };
    match topology() {
        Ok(t) => {
            info.physical_cores = Some(t.cores);
            info.logical_processors = Some(t.logical);
            info.packages = Some(t.packages);
            if t.classes.len() > 1 {
                let max = t.classes.keys().max().copied().unwrap_or(0);
                info.performance_cores = t.classes.get(&max).copied();
                info.efficiency_cores = Some(
                    t.classes
                        .iter()
                        .filter(|(c, _)| **c < max)
                        .map(|(_, n)| n)
                        .sum(),
                );
            }
        }
        Err(e) => status.push(e),
    }
    (info, status)
}

/// Parses a `Processor Information` instance name (`0,5`) into `(group, index)`.
/// Returns `None` for `_Total` and `0,_Total` aggregate instances and malformed names.
pub fn parse_cpu_instance(name: &str) -> Option<(u32, u32)> {
    let (g, i) = name.split_once(',')?;
    Some((g.trim().parse().ok()?, i.trim().parse().ok()?))
}

/// Sorts `(instance, value)` pairs by processor and returns only the values; an
/// instance without a valid value this tick keeps its slot as `None`.
fn per_core(items: &[(String, Option<f64>)]) -> Vec<Option<f32>> {
    let mut keyed: Vec<((u32, u32), Option<f64>)> = items
        .iter()
        .filter_map(|(n, v)| parse_cpu_instance(n).map(|k| (k, *v)))
        .collect();
    keyed.sort_by_key(|(k, _)| *k);
    keyed
        .into_iter()
        .map(|(_, v)| v.map(|v| v as f32))
        .collect()
}

fn total(items: &[(String, Option<f64>)]) -> Option<f64> {
    items
        .iter()
        .find(|(n, _)| n == "_Total")
        .and_then(|(_, v)| *v)
}

struct Counters {
    query: PdhQuery,
    time: Counter,
    utility: Counter,
    performance: Counter,
    frequency: Counter,
}

pub struct CpuSampler {
    counters: Result<Counters, PdhError>,
}

impl CpuSampler {
    pub fn new() -> Self {
        let counters = (|| {
            let mut query = PdhQuery::open()?;
            let c = Counters {
                time: query.add(r"\Processor Information(*)\% Processor Time")?,
                utility: query.add(r"\Processor Information(*)\% Processor Utility")?,
                performance: query.add(r"\Processor Information(*)\% Processor Performance")?,
                frequency: query.add(r"\Processor Information(*)\Processor Frequency")?,
                query,
            };
            Ok(c)
        })();
        let mut s = Self { counters };
        if let Ok(c) = &mut s.counters {
            if let Err(e) = c.query.collect() {
                s.counters = Err(e);
            }
        }
        s
    }

    pub fn sample(&mut self) -> CpuSnapshot {
        let mut snap = CpuSnapshot::default();
        // System-wide counts and uptime do not depend on PDH.
        let mut perf = PERFORMANCE_INFORMATION {
            cb: std::mem::size_of::<PERFORMANCE_INFORMATION>() as u32,
            ..Default::default()
        };
        // SAFETY: struct with cb set, size passed.
        if unsafe { GetPerformanceInfo(&mut perf, perf.cb) }.is_ok() {
            snap.process_count = Some(perf.ProcessCount);
            snap.thread_count = Some(perf.ThreadCount);
            snap.handle_count = Some(perf.HandleCount);
        }
        // SAFETY: no preconditions.
        snap.uptime_secs = Some(unsafe { GetTickCount64() } / 1000);

        let c = match &mut self.counters {
            Ok(c) => c,
            Err(e) => {
                snap.status = Status::error(format!("CPU counters unavailable: {e}"));
                return snap;
            }
        };
        if let Err(e) = c.query.collect() {
            snap.status = Status::error(format!("CPU counters: {e}"));
            return snap;
        }
        if !c.query.primed() {
            snap.status = Status::waiting("Waiting for the second CPU sample");
            return snap;
        }
        let read = |counter, nocap| c.query.array_all(counter, nocap);
        match (
            read(c.time, false),
            read(c.utility, true),
            read(c.performance, true),
            read(c.frequency, true),
        ) {
            (Ok(time), Ok(utility), Ok(perf), Ok(freq)) => {
                snap.total_pct = total(&time).map(|v| v.clamp(0.0, 100.0) as f32);
                snap.utility_pct = total(&utility).map(|v| v.max(0.0) as f32);
                snap.per_core_pct = per_core(&time)
                    .into_iter()
                    .map(|v| v.map(|v| v.clamp(0.0, 100.0)))
                    .collect();
                snap.base_mhz = total(&freq).map(|v| v as f32).filter(|v| *v > 0.0);
                if let (Some(base), Some(p)) = (snap.base_mhz, total(&perf)) {
                    snap.effective_mhz = Some(base * p as f32 / 100.0);
                }
                let freq_by_core = per_core(&freq);
                let perf_by_core = per_core(&perf);
                if freq_by_core.len() == perf_by_core.len() {
                    snap.per_core_mhz = freq_by_core
                        .iter()
                        .zip(&perf_by_core)
                        .map(|(f, p)| Some(f.as_ref()? * p.as_ref()? / 100.0))
                        .collect();
                }
                snap.status = Status::available();
            }
            (Err(e), ..) | (_, Err(e), ..) | (_, _, Err(e), _) | (.., Err(e)) => {
                snap.status = Status::error(format!("CPU counters: {e}"));
            }
        }
        snap
    }
}

impl Default for CpuSampler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_base_clock() {
        let mhz = cpu_info().0.base_mhz.expect("base clock");
        assert!((400.0..10_000.0).contains(&mhz), "{mhz}");
    }

    #[test]
    fn parses_instances() {
        assert_eq!(parse_cpu_instance("0,5"), Some((0, 5)));
        assert_eq!(parse_cpu_instance("1,12"), Some((1, 12)));
        assert_eq!(parse_cpu_instance("0,_Total"), None);
        assert_eq!(parse_cpu_instance("_Total"), None);
        assert_eq!(parse_cpu_instance(""), None);
    }

    #[test]
    fn per_core_sorts_numerically_and_drops_totals() {
        let items = vec![
            ("0,10".into(), Some(10.0)),
            ("0,2".into(), Some(2.0)),
            ("_Total".into(), Some(99.0)),
            ("0,_Total".into(), Some(98.0)),
            ("0,3".into(), None),
            ("1,0".into(), Some(100.0)),
        ];
        // An invalid core keeps its slot, so later cores are not shifted.
        assert_eq!(
            per_core(&items),
            vec![Some(2.0), None, Some(10.0), Some(100.0)]
        );
        assert_eq!(total(&items), Some(99.0));
    }

    #[test]
    fn cpuid_and_topology_on_this_machine() {
        let (info, status) = cpu_info();
        assert!(status.is_empty(), "{status:?}");
        assert!(info.logical_processors.unwrap_or(0) >= info.physical_cores.unwrap_or(0));
        assert!(info.vendor.is_some());
    }
}
