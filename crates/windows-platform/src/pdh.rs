//! Minimal safe wrapper over the Performance Data Helper (PDH) API.
//!
//! Counters are always added with `PdhAddEnglishCounterW`, because counter and
//! object names are localized on non-English Windows.

use std::fmt;

use windows::Win32::System::Performance::{
    PdhAddEnglishCounterW, PdhCloseQuery, PdhCollectQueryData, PdhGetFormattedCounterArrayW,
    PdhGetFormattedCounterValue, PdhOpenQueryW, PDH_CSTATUS_NEW_DATA, PDH_CSTATUS_VALID_DATA,
    PDH_FMT, PDH_FMT_COUNTERVALUE, PDH_FMT_COUNTERVALUE_ITEM_W, PDH_FMT_DOUBLE, PDH_HCOUNTER,
    PDH_HQUERY, PDH_INVALID_DATA, PDH_MORE_DATA,
};

use crate::handle::{from_pwstr, wide};

/// pdh.h: `#define PDH_FMT_NOCAP100 ((DWORD) 0x00008000)` — do not clamp values above 100.
const PDH_FMT_NOCAP100: u32 = 0x0000_8000;
/// pdhmsg.h: `PDH_CSTATUS_NO_INSTANCE` 0x800007D1, `PDH_CSTATUS_NO_OBJECT` 0xC0000BB8,
/// `PDH_CSTATUS_NO_COUNTER` 0xC0000BB9: the object or counter does not exist on this system.
const NOT_FOUND: [u32; 3] = [0x8000_07D1, 0xC000_0BB8, 0xC000_0BB9];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PdhError {
    pub call: &'static str,
    pub code: u32,
}

impl PdhError {
    /// The object or counter is not present on this system (e.g. no thermal zones).
    pub fn is_not_found(&self) -> bool {
        NOT_FOUND.contains(&self.code)
    }
}

impl fmt::Display for PdhError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} failed (0x{:08X})", self.call, self.code)
    }
}

impl std::error::Error for PdhError {}

fn check(call: &'static str, code: u32) -> Result<(), PdhError> {
    if code == 0 {
        Ok(())
    } else {
        Err(PdhError { call, code })
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Counter(PDH_HCOUNTER);

/// An open PDH query. Counter handles belong to it and are freed with it.
pub struct PdhQuery {
    handle: PDH_HQUERY,
    collects: u32,
}

impl PdhQuery {
    pub fn open() -> Result<Self, PdhError> {
        let mut handle = PDH_HQUERY::default();
        // SAFETY: out-pointer to a local; null data source = live data.
        check("PdhOpenQueryW", unsafe {
            PdhOpenQueryW(windows::core::PCWSTR::null(), 0, &mut handle)
        })?;
        Ok(Self {
            handle,
            collects: 0,
        })
    }

    /// Adds a counter by its English path, e.g. `\Processor Information(*)\% Processor Time`.
    pub fn add(&mut self, path: &str) -> Result<Counter, PdhError> {
        let w = wide(path);
        let mut c = PDH_HCOUNTER::default();
        // SAFETY: `w` is NUL-terminated and outlives the call; out-pointer to a local.
        check("PdhAddEnglishCounterW", unsafe {
            PdhAddEnglishCounterW(self.handle, windows::core::PCWSTR(w.as_ptr()), 0, &mut c)
        })?;
        Ok(Counter(c))
    }

    pub fn collect(&mut self) -> Result<(), PdhError> {
        // SAFETY: the query handle is valid until drop.
        check("PdhCollectQueryData", unsafe {
            PdhCollectQueryData(self.handle)
        })?;
        self.collects = self.collects.saturating_add(1);
        Ok(())
    }

    /// Rate counters need two collections before they produce a value.
    pub fn primed(&self) -> bool {
        self.collects >= 2
    }

    fn format(nocap: bool) -> PDH_FMT {
        PDH_FMT(PDH_FMT_DOUBLE.0 | if nocap { PDH_FMT_NOCAP100 } else { 0 })
    }

    /// Value of a single-instance counter. `Ok(None)` while not primed or when the
    /// counter reports no valid data for this interval.
    pub fn value(&self, c: Counter, nocap: bool) -> Result<Option<f64>, PdhError> {
        let mut v = PDH_FMT_COUNTERVALUE::default();
        // SAFETY: counter belongs to this live query; out-pointer to a local.
        let code = unsafe { PdhGetFormattedCounterValue(c.0, Self::format(nocap), None, &mut v) };
        if code == PDH_INVALID_DATA && !self.primed() {
            return Ok(None);
        }
        check("PdhGetFormattedCounterValue", code)?;
        if v.CStatus != PDH_CSTATUS_VALID_DATA && v.CStatus != PDH_CSTATUS_NEW_DATA {
            return Ok(None);
        }
        // SAFETY: PDH_FMT_DOUBLE selects the doubleValue member.
        Ok(Some(unsafe { v.Anonymous.doubleValue }))
    }

    /// All instances of a wildcard counter as `(instance name, value)`. Items whose
    /// status is not valid are skipped. Empty while not primed.
    pub fn array(&self, c: Counter, nocap: bool) -> Result<Vec<(String, f64)>, PdhError> {
        Ok(self
            .array_all(c, nocap)?
            .into_iter()
            .filter_map(|(n, v)| v.map(|v| (n, v)))
            .collect())
    }

    /// Like [`Self::array`], but keeps every instance; `None` where the value is not
    /// valid this tick, so positional data (per-core) never shifts.
    pub fn array_all(
        &self,
        c: Counter,
        nocap: bool,
    ) -> Result<Vec<(String, Option<f64>)>, PdhError> {
        let fmt = Self::format(nocap);
        let mut buf: Vec<u64> = Vec::new();
        for _ in 0..4 {
            let mut bytes = (buf.len() * 8) as u32;
            let mut count = 0u32;
            let ptr = if buf.is_empty() {
                None
            } else {
                Some(buf.as_mut_ptr().cast::<PDH_FMT_COUNTERVALUE_ITEM_W>())
            };
            // SAFETY: `ptr` is null (size query) or an 8-byte-aligned buffer of `bytes` bytes.
            let code =
                unsafe { PdhGetFormattedCounterArrayW(c.0, fmt, &mut bytes, &mut count, ptr) };
            if code == PDH_MORE_DATA {
                // Instances can appear between the size query and the read; leave headroom.
                buf = vec![0u64; (bytes as usize).div_ceil(8) + 64];
                continue;
            }
            if code == PDH_INVALID_DATA && !self.primed() {
                return Ok(Vec::new());
            }
            check("PdhGetFormattedCounterArrayW", code)?;
            if buf.is_empty() {
                return Ok(Vec::new());
            }
            let items = buf.as_ptr().cast::<PDH_FMT_COUNTERVALUE_ITEM_W>();
            let mut out = Vec::with_capacity(count as usize);
            for i in 0..count as usize {
                // SAFETY: PDH wrote `count` items at the start of `buf`; names point into `buf`, which is alive.
                let item = unsafe { &*items.add(i) };
                let valid = item.FmtValue.CStatus == PDH_CSTATUS_VALID_DATA
                    || item.FmtValue.CStatus == PDH_CSTATUS_NEW_DATA;
                // SAFETY: szName is a NUL-terminated string inside `buf`.
                let name = unsafe { from_pwstr(item.szName.0) };
                // SAFETY: PDH_FMT_DOUBLE selects the doubleValue member.
                out.push((
                    name,
                    valid.then_some(unsafe { item.FmtValue.Anonymous.doubleValue }),
                ));
            }
            return Ok(out);
        }
        Err(PdhError {
            call: "PdhGetFormattedCounterArrayW (instance list kept changing)",
            code: PDH_MORE_DATA,
        })
    }
}

impl Drop for PdhQuery {
    fn drop(&mut self) {
        // SAFETY: closing the query we opened; frees its counters too.
        let code = unsafe { PdhCloseQuery(self.handle) };
        if code != 0 {
            tracing::warn!("PdhCloseQuery failed (0x{code:08X})");
        }
    }
}
