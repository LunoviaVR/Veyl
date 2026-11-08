//! Vendor-neutral GPU telemetry from Windows itself (WDDM):
//! - Adapters: DXGI (`IDXGIFactory1::EnumAdapters1`).
//! - Utilization and video memory: PDH `GPU Engine`, `GPU Adapter Memory` and
//!   `GPU Process Memory` counters, aggregated with Task Manager's rules.
//! - Temperature, fan, power share, memory clock: `D3DKMTQueryAdapterInfo
//!   (KMTQAITYPE_ADAPTERPERFDATA)`, the source Task Manager uses for GPU temperature.
//! - Engine clock: `KMTQAITYPE_NODEPERFDATA` for the first 3D engine node.
//!
//! A driver that does not implement a perf-data field reports 0, which is shown
//! as "not reported", never as a zero reading.

use std::collections::{BTreeMap, HashMap};

use veyl_core::{GpuEngineUsage, GpuInfo, GpuSnapshot, GpusSnapshot, Status};
use windows::core::Interface;
use windows::Wdk::Graphics::Direct3D::{
    D3DKMTCloseAdapter, D3DKMTOpenAdapterFromLuid, D3DKMTQueryAdapterInfo, D3DKMT_ADAPTER_PERFDATA,
    D3DKMT_ADAPTER_PERFDATACAPS, D3DKMT_CLOSEADAPTER, D3DKMT_NODE_PERFDATA,
    D3DKMT_OPENADAPTERFROMLUID, D3DKMT_QUERYADAPTERINFO, KMTQAITYPE_ADAPTERPERFDATA,
    KMTQAITYPE_ADAPTERPERFDATA_CAPS, KMTQAITYPE_NODEPERFDATA, KMTQUERYADAPTERINFOTYPE,
};
use windows::Win32::Foundation::LUID;
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, IDXGIDevice, IDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE,
};

use crate::handle::from_wide;
use crate::pdh::{Counter, PdhError, PdhQuery};

/// LUID formatted the way PDH GPU counter instances spell it: `0xHIGH_0xLOW`.
pub fn luid_key(high: i32, low: u32) -> String {
    format!("0x{:08X}_0x{:08X}", high as u32, low)
}

/// Enumerates hardware GPU adapters (software rasterizers excluded).
pub fn adapters() -> Result<Vec<(GpuInfo, LUID)>, String> {
    // SAFETY: plain factory creation; COM pointers released on drop.
    let factory: IDXGIFactory1 =
        unsafe { CreateDXGIFactory1() }.map_err(|e| format!("CreateDXGIFactory1 failed: {e}"))?;
    let mut out = Vec::new();
    for i in 0.. {
        // SAFETY: valid factory; DXGI_ERROR_NOT_FOUND ends the enumeration.
        let Ok(adapter) = (unsafe { factory.EnumAdapters1(i) }) else {
            break;
        };
        // SAFETY: valid adapter.
        let Ok(d) = (unsafe { adapter.GetDesc1() }) else {
            continue;
        };
        let is_software = d.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 != 0;
        if is_software {
            continue;
        }
        let luid = d.AdapterLuid;
        // The same physical adapter can be enumerated once per output configuration; keep the first.
        let key = luid_key(luid.HighPart, luid.LowPart);
        if out.iter().any(|(g, _): &(GpuInfo, LUID)| g.luid == key) {
            continue;
        }
        out.push((
            GpuInfo {
                luid: key,
                name: from_wide(&d.Description),
                vendor_id: d.VendorId,
                device_id: d.DeviceId,
                dedicated_memory_bytes: d.DedicatedVideoMemory as u64,
                shared_memory_bytes: d.SharedSystemMemory as u64,
                is_software,
            },
            luid,
        ));
    }
    Ok(out)
}

/// User-mode driver version per adapter LUID key, as Device Manager shows it
/// (e.g. 32.0.15.6094).
pub fn driver_versions() -> Vec<(String, String)> {
    // SAFETY: plain factory creation; COM pointers released on drop.
    let Ok(factory) = (unsafe { CreateDXGIFactory1::<IDXGIFactory1>() }) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for i in 0.. {
        // SAFETY: valid factory; DXGI_ERROR_NOT_FOUND ends the enumeration.
        let Ok(adapter) = (unsafe { factory.EnumAdapters1(i) }) else {
            break;
        };
        // SAFETY: valid adapter.
        let Ok(d) = (unsafe { adapter.GetDesc1() }) else {
            continue;
        };
        // SAFETY: valid adapter; asking about IDXGIDevice returns the UMD version.
        if let Ok(v) = unsafe { adapter.CheckInterfaceSupport(&IDXGIDevice::IID) } {
            let v = v as u64;
            out.push((
                luid_key(d.AdapterLuid.HighPart, d.AdapterLuid.LowPart),
                format!(
                    "{}.{}.{}.{}",
                    v >> 48,
                    (v >> 32) & 0xFFFF,
                    (v >> 16) & 0xFFFF,
                    v & 0xFFFF
                ),
            ));
        }
    }
    out
}

// ---------------------------------------------------------------------------
// PDH instance parsing and aggregation (pure)
// ---------------------------------------------------------------------------

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct EngineInstance {
    pub pid: u32,
    pub luid: String,
    pub phys: u32,
    pub eng: u32,
    pub engine_type: String,
}

/// Parses `pid_1234_luid_0x00000000_0x0000D1B2_phys_0_eng_3_engtype_VideoDecode`.
pub fn parse_engine_instance(s: &str) -> Option<EngineInstance> {
    let rest = s.strip_prefix("pid_")?;
    let (pid, rest) = rest.split_once("_luid_")?;
    let (luid, rest) = rest.split_once("_phys_")?;
    let (phys, rest) = rest.split_once("_eng_")?;
    let (eng, engtype) = rest.split_once("_engtype_")?;
    if luid.len() != 21 {
        return None;
    }
    Some(EngineInstance {
        pid: pid.parse().ok()?,
        luid: luid.to_ascii_uppercase().replace("0X", "0x"),
        phys: phys.parse().ok()?,
        eng: eng.parse().ok()?,
        engine_type: engtype.to_owned(),
    })
}

/// Parses `luid_0x..._0x..._phys_0` (adapter memory) or `pid_N_luid_..._phys_0` (process memory).
pub fn parse_memory_instance(s: &str) -> Option<(Option<u32>, String)> {
    let (pid, rest) = match s.strip_prefix("pid_") {
        Some(r) => {
            let (p, r) = r.split_once('_')?;
            (Some(p.parse().ok()?), r)
        }
        None => (None, s),
    };
    let rest = rest.strip_prefix("luid_")?;
    let (luid, _) = rest.split_once("_phys_")?;
    (luid.len() == 21).then(|| (pid, luid.to_ascii_uppercase().replace("0X", "0x")))
}

#[derive(Debug, Default, PartialEq)]
pub struct EngineAggregate {
    /// luid -> busiest engine %, clamped 0..=100.
    pub adapter_util: HashMap<String, f32>,
    /// luid -> engine type -> busiest engine of that type %.
    pub adapter_engines: HashMap<String, BTreeMap<String, f32>>,
    /// pid -> busiest engine % used by that process.
    pub process_util: HashMap<u32, f32>,
    /// luid -> lowest engine ordinal whose type is 3D.
    pub first_3d_engine: HashMap<String, u32>,
}

/// Task Manager's rule: an engine's utilization is the sum over the processes using
/// it; an adapter's utilization is its busiest engine; a process's is its busiest engine.
pub fn aggregate_engines(items: &[(String, f64)]) -> EngineAggregate {
    let mut per_engine: HashMap<(String, u32, u32), (f64, String)> = HashMap::new();
    let mut agg = EngineAggregate::default();
    for (name, v) in items {
        let Some(e) = parse_engine_instance(name) else {
            continue;
        };
        let v = v.max(0.0);
        let slot = per_engine
            .entry((e.luid.clone(), e.phys, e.eng))
            .or_insert((0.0, e.engine_type.clone()));
        slot.0 += v;
        let p = agg.process_util.entry(e.pid).or_insert(0.0);
        *p = p.max(v.min(100.0) as f32);
        if e.engine_type == "3D" {
            let first = agg.first_3d_engine.entry(e.luid.clone()).or_insert(e.eng);
            *first = (*first).min(e.eng);
        }
    }
    for ((luid, _, _), (sum, ty)) in per_engine {
        let util = sum.min(100.0) as f32;
        let a = agg.adapter_util.entry(luid.clone()).or_insert(0.0);
        *a = a.max(util);
        let t = agg
            .adapter_engines
            .entry(luid)
            .or_default()
            .entry(if ty.is_empty() { "Other".into() } else { ty })
            .or_insert(0.0);
        *t = t.max(util);
    }
    agg
}

// ---------------------------------------------------------------------------
// D3DKMT adapter handle
// ---------------------------------------------------------------------------

struct KmtAdapter(u32);

impl KmtAdapter {
    fn open(luid: LUID) -> Result<Self, String> {
        let mut o = D3DKMT_OPENADAPTERFROMLUID {
            AdapterLuid: luid,
            hAdapter: 0,
        };
        // SAFETY: in/out struct on the stack.
        let st = unsafe { D3DKMTOpenAdapterFromLuid(&mut o) };
        if st.0 < 0 {
            return Err(format!(
                "D3DKMTOpenAdapterFromLuid failed (0x{:08X})",
                st.0 as u32
            ));
        }
        Ok(Self(o.hAdapter))
    }

    fn query<T: Copy>(&self, kind: KMTQUERYADAPTERINFOTYPE, data: &mut T) -> Result<(), u32> {
        let mut q = D3DKMT_QUERYADAPTERINFO {
            hAdapter: self.0,
            Type: kind,
            pPrivateDriverData: (data as *mut T).cast(),
            PrivateDriverDataSize: std::mem::size_of::<T>() as u32,
        };
        // SAFETY: `data` is a correctly sized, writable struct for this query type.
        let st = unsafe { D3DKMTQueryAdapterInfo(&mut q) };
        if st.0 < 0 {
            Err(st.0 as u32)
        } else {
            Ok(())
        }
    }
}

impl Drop for KmtAdapter {
    fn drop(&mut self) {
        let c = D3DKMT_CLOSEADAPTER { hAdapter: self.0 };
        // SAFETY: closing the handle we opened.
        let st = unsafe { D3DKMTCloseAdapter(&c) };
        if st.0 < 0 {
            tracing::warn!("D3DKMTCloseAdapter failed (0x{:08X})", st.0 as u32);
        }
    }
}

/// Converts a perf-data value where 0 means "not implemented by the driver".
fn nonzero<T: PartialEq + Default>(v: T) -> Option<T> {
    (v != T::default()).then_some(v)
}

/// d3dkmthk.h: Temperature and TemperatureMax/Warning are in deci-°C (1 = 0.1 °C).
fn deci_celsius(v: u32) -> Option<f32> {
    let c = v as f32 / 10.0;
    (v != 0 && c < 200.0).then_some(c)
}

/// `TemperatureMax` in D3DKMT_ADAPTER_PERFDATACAPS has no documented unit. Observed:
/// 110 on an AMD Radeon RX 9060 XT (driver 32.0.32015), i.e. whole °C. Values that
/// only make sense as deci-°C are converted; anything else is rejected.
fn caps_celsius(v: u32) -> Option<f32> {
    match v {
        40..=150 => Some(v as f32),
        400..=1500 => Some(v as f32 / 10.0),
        _ => None,
    }
}

struct Adapter {
    info: GpuInfo,
    kmt: Result<KmtAdapter, String>,
    temp_max_c: Option<f32>,
}

struct Counters {
    query: PdhQuery,
    engine: Counter,
    adapter_dedicated: Counter,
    adapter_shared: Counter,
    process_dedicated: Counter,
}

/// Per-process GPU figures for merging into the process table.
#[derive(Default, Debug, Clone)]
pub struct ProcessGpu {
    pub util_pct: HashMap<u32, f32>,
    pub dedicated_bytes: HashMap<u32, u64>,
}

pub struct GpuSampler {
    adapters: Vec<Adapter>,
    counters: Result<Counters, PdhError>,
    enumerate_error: Option<String>,
}

impl GpuSampler {
    pub fn new() -> Self {
        let (adapters, enumerate_error) = match adapters() {
            Ok(list) => (
                list.into_iter()
                    .map(|(info, luid)| {
                        let kmt = KmtAdapter::open(luid);
                        let temp_max_c = kmt.as_ref().ok().and_then(|k| {
                            let mut caps = D3DKMT_ADAPTER_PERFDATACAPS::default();
                            k.query(KMTQAITYPE_ADAPTERPERFDATA_CAPS, &mut caps).ok()?;
                            caps_celsius(caps.TemperatureMax)
                        });
                        Adapter {
                            info,
                            kmt,
                            temp_max_c,
                        }
                    })
                    .collect(),
                None,
            ),
            Err(e) => (Vec::new(), Some(e)),
        };
        let counters = (|| {
            let mut query = PdhQuery::open()?;
            let c = Counters {
                engine: query.add(r"\GPU Engine(*)\Utilization Percentage")?,
                adapter_dedicated: query.add(r"\GPU Adapter Memory(*)\Dedicated Usage")?,
                adapter_shared: query.add(r"\GPU Adapter Memory(*)\Shared Usage")?,
                process_dedicated: query.add(r"\GPU Process Memory(*)\Dedicated Usage")?,
                query,
            };
            Ok(c)
        })();
        let mut s = Self {
            adapters,
            counters,
            enumerate_error,
        };
        if let Ok(c) = &mut s.counters {
            if let Err(e) = c.query.collect() {
                s.counters = Err(e);
            }
        }
        s
    }

    pub fn infos(&self) -> Vec<GpuInfo> {
        self.adapters.iter().map(|a| a.info.clone()).collect()
    }

    pub fn sample(&mut self) -> (GpusSnapshot, ProcessGpu) {
        let mut snap = GpusSnapshot::default();
        let mut procs = ProcessGpu::default();
        if let Some(e) = &self.enumerate_error {
            snap.status = Status::error(e.clone());
            return (snap, procs);
        }
        if self.adapters.is_empty() {
            snap.status = Status::new(
                veyl_core::Availability::Unsupported,
                "No hardware GPU adapter was found",
            );
            return (snap, procs);
        }

        // PDH counters (utilization, memory).
        let mut counters_status = Status::available();
        let mut agg = EngineAggregate::default();
        let mut adapter_mem: HashMap<String, (u64, u64)> = HashMap::new();
        let mut adapter_mem_seen = false;
        match &mut self.counters {
            Err(e) => {
                counters_status =
                    Status::error(format!("GPU performance counters unavailable: {e}"))
            }
            Ok(c) => match c.query.collect() {
                Err(e) => counters_status = Status::error(format!("GPU counters: {e}")),
                Ok(()) if !c.query.primed() => {
                    counters_status = Status::waiting("Waiting for the second GPU sample")
                }
                Ok(()) => {
                    match c.query.array(c.engine, true) {
                        Ok(items) => agg = aggregate_engines(&items),
                        Err(e) => {
                            counters_status = Status::error(format!("GPU engine counters: {e}"))
                        }
                    }
                    for (counter, shared) in
                        [(c.adapter_dedicated, false), (c.adapter_shared, true)]
                    {
                        if let Ok(items) = c.query.array(counter, true) {
                            adapter_mem_seen = true;
                            for (name, v) in items {
                                if let Some((None, luid)) = parse_memory_instance(&name) {
                                    let e = adapter_mem.entry(luid).or_default();
                                    if shared {
                                        e.1 = e.1.saturating_add(v.max(0.0) as u64);
                                    } else {
                                        e.0 = e.0.saturating_add(v.max(0.0) as u64);
                                    }
                                }
                            }
                        }
                    }
                    if let Ok(items) = c.query.array(c.process_dedicated, true) {
                        for (name, v) in items {
                            if let Some((Some(pid), _)) = parse_memory_instance(&name) {
                                *procs.dedicated_bytes.entry(pid).or_insert(0) += v.max(0.0) as u64;
                            }
                        }
                    }
                    procs.util_pct = agg.process_util.clone();
                }
            },
        }

        for a in &self.adapters {
            let luid = &a.info.luid;
            let mut g = GpuSnapshot {
                luid: luid.clone(),
                name: a.info.name.clone(),
                vendor_id: a.info.vendor_id,
                dedicated_total_bytes: a.info.dedicated_memory_bytes,
                temperature_max_c: a.temp_max_c,
                counters_status: counters_status.clone(),
                ..Default::default()
            };
            if counters_status.is_available() {
                g.util_pct = agg.adapter_util.get(luid).copied();
                if g.util_pct.is_none() {
                    g.counters_status = Status::new(
                        veyl_core::Availability::Unsupported,
                        "No GPU engine counters for this adapter",
                    );
                }
                g.engines = agg
                    .adapter_engines
                    .get(luid)
                    .map(|m| {
                        m.iter()
                            .map(|(t, u)| GpuEngineUsage {
                                engine_type: t.clone(),
                                util_pct: *u,
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                if adapter_mem_seen {
                    if let Some((d, s)) = adapter_mem.get(luid) {
                        g.dedicated_used_bytes = Some(*d);
                        g.shared_used_bytes = Some(*s);
                    }
                }
            }
            match &a.kmt {
                Err(e) => g.sensors_status = Status::error(e.clone()),
                Ok(k) => {
                    let mut p = D3DKMT_ADAPTER_PERFDATA::default();
                    match k.query(KMTQAITYPE_ADAPTERPERFDATA, &mut p) {
                        Ok(()) => {
                            g.temperature_c = deci_celsius(p.Temperature);
                            g.fan_rpm = nonzero(p.FanRPM);
                            // d3dkmthk.h: Power is in tenths of a percent of the board's power budget.
                            g.power_pct = nonzero(p.Power).map(|v| v as f32 / 10.0);
                            // d3dkmthk.h: MemoryFrequency is in hertz.
                            g.memory_clock_mhz = nonzero(p.MemoryFrequency).map(|hz| (hz as f64 / 1e6) as f32);
                            g.sensors_status = if g.temperature_c.is_none() && g.fan_rpm.is_none() && g.power_pct.is_none() {
                                Status::new(veyl_core::Availability::Unsupported, "The GPU driver does not report temperature, fan or power to Windows")
                            } else {
                                Status::available()
                            };
                        }
                        Err(code) => {
                            g.sensors_status = Status::new(
                                veyl_core::Availability::Unsupported,
                                format!("D3DKMTQueryAdapterInfo(ADAPTERPERFDATA) failed (0x{code:08X}); requires a WDDM 2.4+ driver"),
                            )
                        }
                    }
                    if let Some(&node) = agg.first_3d_engine.get(luid) {
                        let mut n = D3DKMT_NODE_PERFDATA {
                            NodeOrdinal: node,
                            ..Default::default()
                        };
                        if k.query(KMTQAITYPE_NODEPERFDATA, &mut n).is_ok() {
                            // d3dkmthk.h: Frequency is in hertz.
                            g.core_clock_mhz =
                                nonzero(n.Frequency).map(|hz| (hz as f64 / 1e6) as f32);
                        }
                    }
                }
            }
            snap.adapters.push(g);
        }
        snap.status = Status::available();
        (snap, procs)
    }
}

impl Default for GpuSampler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const L: &str = "0x00000000_0x0000D1B2";

    #[test]
    fn parses_engine_instances() {
        let e = parse_engine_instance(
            "pid_1234_luid_0x00000000_0x0000d1b2_phys_0_eng_3_engtype_VideoDecode",
        )
        .expect("parse");
        assert_eq!(
            e,
            EngineInstance {
                pid: 1234,
                luid: L.into(),
                phys: 0,
                eng: 3,
                engine_type: "VideoDecode".into()
            }
        );
        let e = parse_engine_instance("pid_8_luid_0x00000000_0x0000D1B2_phys_0_eng_9_engtype_")
            .expect("empty type");
        assert_eq!(e.engine_type, "");
        assert!(parse_engine_instance("garbage").is_none());
        assert!(
            parse_engine_instance("pid_x_luid_0x00000000_0x0000D1B2_phys_0_eng_0_engtype_3D")
                .is_none()
        );
    }

    #[test]
    fn parses_memory_instances() {
        assert_eq!(
            parse_memory_instance("luid_0x00000000_0x0000D1B2_phys_0"),
            Some((None, L.into()))
        );
        assert_eq!(
            parse_memory_instance("pid_42_luid_0x00000000_0x0000D1B2_phys_0"),
            Some((Some(42), L.into()))
        );
        assert_eq!(
            parse_memory_instance("pid_x_luid_0x00000000_0x0000D1B2_phys_0"),
            None
        );
        assert_eq!(parse_memory_instance("luid_short_phys_0"), None);
    }

    #[test]
    fn aggregates_like_task_manager() {
        let n = |pid: u32, eng: u32, ty: &str| {
            format!("pid_{pid}_luid_{L}_phys_0_eng_{eng}_engtype_{ty}")
        };
        let items = vec![
            (n(1, 0, "3D"), 30.0),
            (n(2, 0, "3D"), 25.0),   // same engine: 55 %
            (n(2, 2, "Copy"), 70.0), // busiest single engine for pid 2
            (n(3, 1, "3D"), 40.0), // second 3D engine is separate hardware: not summed with engine 0
            (n(4, 0, "3D"), 80.0), // engine 0 now sums to 135 -> clamped 100
        ];
        let a = aggregate_engines(&items);
        assert_eq!(a.adapter_util.get(L), Some(&100.0));
        assert_eq!(a.process_util.get(&2), Some(&70.0));
        assert_eq!(a.process_util.get(&3), Some(&40.0));
        assert_eq!(a.adapter_engines[L]["3D"], 100.0);
        assert_eq!(a.adapter_engines[L]["Copy"], 70.0);
        assert_eq!(a.first_3d_engine.get(L), Some(&0));
        assert!(aggregate_engines(&[]).adapter_util.is_empty());
    }

    #[test]
    fn unit_conversions() {
        assert_eq!(deci_celsius(655), Some(65.5));
        assert_eq!(deci_celsius(0), None);
        assert_eq!(deci_celsius(5000), None, "implausible");
        assert_eq!(nonzero(0u32), None);
        assert_eq!(caps_celsius(110), Some(110.0));
        assert_eq!(caps_celsius(1050), Some(105.0));
        assert_eq!(caps_celsius(11), None);
        assert_eq!(luid_key(0, 0xD1B2), L);
    }
}
