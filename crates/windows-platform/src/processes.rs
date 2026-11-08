//! Process enumeration via one `NtQuerySystemInformation(SystemProcessInformation)`
//! call per sample: every process with its times, memory, I/O counters, handle
//! and thread counts, as Task Manager and Resource Monitor read them.
//!
//! Identity: a process is `(pid, creation time)`. Deltas (CPU %, I/O rates) are
//! only computed against a previous sample of the same identity, so a reused PID
//! never inherits another process's counters.

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use pch_core::rate::{per_second, share_pct};
use pch_core::time::filetime_to_unix_ms;
use pch_core::{ProcessRow, ProcessTable, SelfUsage, Status};
use windows::Wdk::System::SystemInformation::{NtQuerySystemInformation, SystemProcessInformation};
use windows::Win32::Foundation::NTSTATUS;
use windows::Win32::System::Threading::{
    GetCurrentProcessId, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION,
};

use crate::handle::OwnedHandle;

/// ntstatus.h: STATUS_INFO_LENGTH_MISMATCH.
const STATUS_INFO_LENGTH_MISMATCH: i32 = 0xC000_0004_u32 as i32;

/// `SYSTEM_PROCESS_INFORMATION` as laid out on x64 Windows Vista and later.
/// Source: phnt `ntexapi.h` (the public `winternl.h` version marks most of these
/// fields Reserved). Offsets are asserted below.
#[repr(C)]
#[derive(Clone, Copy)]
struct SysProc {
    next_entry_offset: u32,
    number_of_threads: u32,
    working_set_private_size: i64,
    hard_fault_count: u32,
    number_of_threads_high_watermark: u32,
    cycle_time: u64,
    create_time: i64,
    user_time: i64,
    kernel_time: i64,
    image_name_length: u16,
    image_name_maximum_length: u16,
    image_name_buffer: usize,
    base_priority: i32,
    unique_process_id: usize,
    inherited_from_unique_process_id: usize,
    handle_count: u32,
    session_id: u32,
    unique_process_key: usize,
    peak_virtual_size: usize,
    virtual_size: usize,
    page_fault_count: u32,
    peak_working_set_size: usize,
    working_set_size: usize,
    quota_peak_paged_pool_usage: usize,
    quota_paged_pool_usage: usize,
    quota_peak_non_paged_pool_usage: usize,
    quota_non_paged_pool_usage: usize,
    pagefile_usage: usize,
    peak_pagefile_usage: usize,
    private_page_count: usize,
    read_operation_count: i64,
    write_operation_count: i64,
    other_operation_count: i64,
    read_transfer_count: i64,
    write_transfer_count: i64,
    other_transfer_count: i64,
}

/// `SYSTEM_THREAD_INFORMATION` (phnt `ntexapi.h`); only state and wait reason are read.
#[repr(C)]
#[derive(Clone, Copy)]
struct SysThread {
    kernel_time: i64,
    user_time: i64,
    create_time: i64,
    wait_time: u32,
    start_address: usize,
    client_id: [usize; 2],
    priority: i32,
    base_priority: i32,
    context_switches: u32,
    thread_state: u32,
    wait_reason: u32,
}

const _: () = assert!(std::mem::size_of::<SysProc>() == 256);
const _: () = assert!(std::mem::offset_of!(SysProc, image_name_buffer) == 64);
const _: () = assert!(std::mem::offset_of!(SysProc, unique_process_id) == 80);
const _: () = assert!(std::mem::offset_of!(SysProc, working_set_size) == 144);
const _: () = assert!(std::mem::offset_of!(SysProc, read_transfer_count) == 232);
const _: () = assert!(std::mem::size_of::<SysThread>() == 80);

/// KTHREAD_STATE Waiting = 5, KWAIT_REASON Suspended = 5 (phnt `ntkeapi.h`).
const THREAD_STATE_WAITING: u32 = 5;
const WAIT_REASON_SUSPENDED: u32 = 5;

/// One process as parsed from the buffer, before rates are computed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RawProcess {
    pub pid: u32,
    pub parent_pid: u32,
    pub create_time: u64,
    pub cpu_time_100ns: u64,
    pub name: String,
    pub threads: u32,
    pub handles: u32,
    pub session_id: u32,
    pub working_set_bytes: u64,
    pub private_working_set_bytes: u64,
    pub private_bytes: u64,
    pub read_bytes: u64,
    pub write_bytes: u64,
    pub suspended: bool,
}

/// Parses a `SystemProcessInformation` buffer. `base_addr` is the address the
/// buffer had when the kernel filled it (image-name pointers refer to it).
/// Every offset and pointer is bounds-checked; a malformed entry ends parsing.
pub fn parse_process_buffer(buf: &[u8], base_addr: usize) -> Vec<RawProcess> {
    let mut out = Vec::new();
    let mut off = 0usize;
    let proc_size = std::mem::size_of::<SysProc>();
    let thread_size = std::mem::size_of::<SysThread>();
    while let Some(entry_bytes) = buf.get(off..off + proc_size) {
        // SAFETY: `entry_bytes` holds size_of::<SysProc>() bytes; read_unaligned has no alignment requirement.
        let p: SysProc = unsafe { std::ptr::read_unaligned(entry_bytes.as_ptr().cast()) };

        let name = if p.image_name_length == 0 || p.image_name_buffer == 0 {
            if p.unique_process_id == 0 {
                "System Idle Process".to_owned()
            } else {
                String::new()
            }
        } else {
            let start = p.image_name_buffer.wrapping_sub(base_addr);
            let len = usize::from(p.image_name_length);
            match buf.get(start..start.saturating_add(len)) {
                Some(bytes) if p.image_name_buffer >= base_addr && len % 2 == 0 => {
                    let words: Vec<u16> = bytes
                        .chunks_exact(2)
                        .map(|c| u16::from_le_bytes([c[0], c[1]]))
                        .collect();
                    String::from_utf16_lossy(&words)
                }
                _ => String::new(),
            }
        };

        let threads_start = off + proc_size;
        let mut all_suspended = p.number_of_threads > 0;
        for t in 0..p.number_of_threads as usize {
            let at = threads_start + t * thread_size;
            let Some(tb) = buf.get(at..at + thread_size) else {
                all_suspended = false;
                break;
            };
            // SAFETY: `tb` holds size_of::<SysThread>() bytes.
            let th: SysThread = unsafe { std::ptr::read_unaligned(tb.as_ptr().cast()) };
            if !(th.thread_state == THREAD_STATE_WAITING && th.wait_reason == WAIT_REASON_SUSPENDED)
            {
                all_suspended = false;
                break;
            }
        }

        out.push(RawProcess {
            pid: p.unique_process_id as u32,
            parent_pid: p.inherited_from_unique_process_id as u32,
            create_time: p.create_time.max(0) as u64,
            cpu_time_100ns: (p.user_time.max(0) as u64).saturating_add(p.kernel_time.max(0) as u64),
            name,
            threads: p.number_of_threads,
            handles: p.handle_count,
            session_id: p.session_id,
            working_set_bytes: p.working_set_size as u64,
            private_working_set_bytes: p.working_set_private_size.max(0) as u64,
            private_bytes: p.pagefile_usage as u64,
            read_bytes: p.read_transfer_count.max(0) as u64,
            write_bytes: p.write_transfer_count.max(0) as u64,
            suspended: all_suspended,
        });

        if p.next_entry_offset == 0 {
            break;
        }
        let next = off.saturating_add(p.next_entry_offset as usize);
        if next <= off {
            break;
        }
        off = next;
    }
    out
}

#[derive(Clone, Copy)]
struct Prev {
    cpu_time_100ns: u64,
    read_bytes: u64,
    write_bytes: u64,
}

pub struct ProcessSampler {
    buf: Vec<u64>,
    prev: HashMap<(u32, u64), Prev>,
    prev_at: Option<Instant>,
    paths: HashMap<(u32, u64), Option<String>>,
    logical_processors: u32,
    own_pid: u32,
}

pub struct ProcessSample {
    pub table: ProcessTable,
    pub self_usage: SelfUsage,
}

/// Full Win32 path of a process image, or `None` (protected or exited process).
pub fn image_path(pid: u32) -> Option<String> {
    if pid == 0 || pid == 4 {
        return None;
    }
    // SAFETY: plain OpenProcess; handle owned below.
    let h =
        OwnedHandle(unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?);
    let mut buf = vec![0u16; 32_768];
    let mut len = buf.len() as u32;
    // SAFETY: buffer of `len` UTF-16 units; handle valid.
    unsafe {
        QueryFullProcessImageNameW(
            h.raw(),
            PROCESS_NAME_WIN32,
            windows::core::PWSTR(buf.as_mut_ptr()),
            &mut len,
        )
    }
    .ok()?;
    Some(String::from_utf16_lossy(&buf[..len as usize]))
}

impl ProcessSampler {
    pub fn new(logical_processors: u32) -> Self {
        Self {
            buf: vec![0u64; 512 * 1024 / 8],
            prev: HashMap::new(),
            prev_at: None,
            paths: HashMap::new(),
            logical_processors: logical_processors.max(1),
            // SAFETY: no preconditions.
            own_pid: unsafe { GetCurrentProcessId() },
        }
    }

    fn query(&mut self) -> Result<usize, String> {
        for _ in 0..5 {
            let mut needed = 0u32;
            let bytes = (self.buf.len() * 8) as u32;
            // SAFETY: buffer of `bytes` bytes, 8-byte aligned; out-pointer to a local.
            let status: NTSTATUS = unsafe {
                NtQuerySystemInformation(
                    SystemProcessInformation,
                    self.buf.as_mut_ptr().cast(),
                    bytes,
                    &mut needed,
                )
            };
            if status.0 == STATUS_INFO_LENGTH_MISMATCH {
                // Processes can start between calls; grow with headroom.
                self.buf = vec![0u64; (needed as usize + 128 * 1024).div_ceil(8)];
                continue;
            }
            if status.0 < 0 {
                return Err(format!(
                    "NtQuerySystemInformation(SystemProcessInformation) failed (0x{:08X})",
                    status.0 as u32
                ));
            }
            return Ok(needed as usize);
        }
        Err("NtQuerySystemInformation: process list kept growing".into())
    }

    pub fn sample(&mut self, now_ms: i64) -> ProcessSample {
        let at = Instant::now();
        let used = match self.query() {
            Ok(n) => n.min(self.buf.len() * 8),
            Err(e) => {
                self.prev_at = None;
                return ProcessSample {
                    table: ProcessTable {
                        status: Status::error(e),
                        timestamp_ms: now_ms,
                        logical_processors: self.logical_processors,
                        rows: vec![],
                    },
                    self_usage: SelfUsage::default(),
                };
            }
        };
        let base = self.buf.as_ptr() as usize;
        // SAFETY: the kernel wrote `used` bytes at the start of `buf`; viewing u64 storage as bytes is valid.
        let bytes = unsafe { std::slice::from_raw_parts(self.buf.as_ptr().cast::<u8>(), used) };
        let raws = parse_process_buffer(bytes, base);

        let elapsed = self.prev_at.map(|p| at.duration_since(p));
        let capacity_100ns =
            elapsed.map(|e| (e.as_nanos() / 100) as u64 * u64::from(self.logical_processors));

        let mut rows = Vec::with_capacity(raws.len());
        let mut next_prev = HashMap::with_capacity(raws.len());
        let mut live: HashSet<(u32, u64)> = HashSet::with_capacity(raws.len());
        let mut self_usage = SelfUsage::default();
        for r in raws {
            let key = (r.pid, r.create_time);
            live.insert(key);
            let prev = self.prev.get(&key).copied();
            let cpu_pct = match (prev, capacity_100ns) {
                (Some(p), Some(cap)) => {
                    share_pct(r.cpu_time_100ns.saturating_sub(p.cpu_time_100ns), cap)
                }
                _ => None,
            };
            let (io_read_bps, io_write_bps) = match (prev, elapsed) {
                (Some(p), Some(e)) => (
                    per_second(p.read_bytes, r.read_bytes, e),
                    per_second(p.write_bytes, r.write_bytes, e),
                ),
                _ => (None, None),
            };
            next_prev.insert(
                key,
                Prev {
                    cpu_time_100ns: r.cpu_time_100ns,
                    read_bytes: r.read_bytes,
                    write_bytes: r.write_bytes,
                },
            );
            let path = self
                .paths
                .entry(key)
                .or_insert_with(|| image_path(r.pid))
                .clone();
            if r.pid == self.own_pid {
                self_usage = SelfUsage {
                    cpu_pct,
                    working_set_bytes: Some(r.working_set_bytes),
                    private_bytes: Some(r.private_bytes),
                    tick_cost_ms: None,
                };
            }
            rows.push(ProcessRow {
                pid: r.pid,
                parent_pid: r.parent_pid,
                start_time_ms: filetime_to_unix_ms(r.create_time),
                name: r.name,
                path,
                cpu_pct,
                working_set_bytes: r.working_set_bytes,
                private_working_set_bytes: r.private_working_set_bytes,
                private_bytes: r.private_bytes,
                io_read_bps,
                io_write_bps,
                gpu_pct: None,
                gpu_dedicated_bytes: None,
                threads: r.threads,
                handles: r.handles,
                session_id: r.session_id,
                suspended: r.suspended,
            });
        }
        self.paths.retain(|k, _| live.contains(k));
        self.prev = next_prev;
        self.prev_at = Some(at);
        let status = if elapsed.is_some() {
            Status::available()
        } else {
            Status::waiting("CPU and I/O rates appear after the second sample")
        };
        ProcessSample {
            table: ProcessTable {
                status,
                timestamp_ms: now_ms,
                logical_processors: self.logical_processors,
                rows,
            },
            self_usage,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a two-process buffer with names stored after the entries.
    fn synthetic(base: usize) -> Vec<u8> {
        let ps = std::mem::size_of::<SysProc>();
        let ts = std::mem::size_of::<SysThread>();
        let first_len = ps + ts; // one thread
        let second_len = ps;
        let name_off = first_len + second_len;
        let name: Vec<u8> = "game.exe"
            .encode_utf16()
            .flat_map(|c| c.to_le_bytes())
            .collect();
        let mut buf = vec![0u8; name_off + name.len()];

        // SAFETY: zeroed POD struct.
        let mut a: SysProc = unsafe { std::mem::zeroed() };
        a.next_entry_offset = first_len as u32;
        a.number_of_threads = 1;
        a.unique_process_id = 1234;
        a.inherited_from_unique_process_id = 100;
        a.create_time = 133_000_000_000_000_000;
        a.user_time = 30;
        a.kernel_time = 12;
        a.image_name_length = name.len() as u16;
        a.image_name_buffer = base + name_off;
        a.handle_count = 77;
        a.working_set_size = 4096;
        a.read_transfer_count = 1000;
        // SAFETY: zeroed POD struct.
        let mut t: SysThread = unsafe { std::mem::zeroed() };
        t.thread_state = THREAD_STATE_WAITING;
        t.wait_reason = WAIT_REASON_SUSPENDED;
        // SAFETY: zeroed POD struct; pid 0 with no name.
        let b: SysProc = unsafe { std::mem::zeroed() };

        // SAFETY: writing POD structs into a byte buffer at in-bounds offsets.
        unsafe {
            std::ptr::write_unaligned(buf.as_mut_ptr().cast(), a);
            std::ptr::write_unaligned(buf.as_mut_ptr().add(ps).cast(), t);
            std::ptr::write_unaligned(buf.as_mut_ptr().add(first_len).cast(), b);
        }
        buf[name_off..].copy_from_slice(&name);
        buf
    }

    #[test]
    fn parses_synthetic_buffer() {
        let base = 0x1000_0000;
        let buf = synthetic(base);
        let p = parse_process_buffer(&buf, base);
        assert_eq!(p.len(), 2);
        assert_eq!(p[0].pid, 1234);
        assert_eq!(p[0].parent_pid, 100);
        assert_eq!(p[0].name, "game.exe");
        assert_eq!(p[0].cpu_time_100ns, 42);
        assert_eq!(p[0].handles, 77);
        assert_eq!(p[0].read_bytes, 1000);
        assert!(p[0].suspended);
        assert_eq!(p[1].name, "System Idle Process");
        assert!(!p[1].suspended, "no threads is not suspended");
    }

    #[test]
    fn rejects_out_of_bounds_name_and_truncated_buffer() {
        let base = 0x1000_0000;
        let buf = synthetic(base);
        // Pretend the buffer lived elsewhere: name pointers fall outside it.
        let p = parse_process_buffer(&buf, base + 0x10_0000);
        assert_eq!(p[0].name, "");
        // Truncated: only part of the first entry.
        assert!(parse_process_buffer(&buf[..100], base).is_empty());
        assert!(parse_process_buffer(&[], base).is_empty());
    }

    #[test]
    fn live_sample_lists_this_process_with_rates_after_second_sample() {
        let mut s = ProcessSampler::new(4);
        let first = s.sample(0);
        assert!(
            first.table.rows.len() > 10,
            "a real system has many processes"
        );
        let own = std::process::id();
        let row = first
            .table
            .rows
            .iter()
            .find(|r| r.pid == own)
            .expect("own process");
        assert_eq!(row.cpu_pct, None, "no rate before a second sample");
        assert!(row.path.as_deref().is_some_and(|p| p.ends_with(".exe")));
        std::thread::sleep(std::time::Duration::from_millis(50));
        let second = s.sample(1);
        let row = second
            .table
            .rows
            .iter()
            .find(|r| r.pid == own)
            .expect("own process");
        assert!(row.cpu_pct.is_some());
        assert!(second.self_usage.working_set_bytes.unwrap_or(0) > 0);
    }
}
