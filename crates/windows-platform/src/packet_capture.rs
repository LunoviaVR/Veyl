//! Packet capture through Windows' built-in NDIS packet capture filter
//! (ndiscap.sys, the component behind `netsh trace` and NetEventPacketCapture).
//! Enabling its ETW provider in a real-time session makes the filter log the
//! frames each adapter sends and receives. Nothing is installed; administrator
//! rights are required to start the session.

use std::collections::HashMap;
use std::ffi::c_void;
use std::thread::JoinHandle;

use windows::core::{GUID, PCWSTR, PWSTR};
use windows::Win32::Foundation::{ERROR_ACCESS_DENIED, ERROR_SUCCESS, WIN32_ERROR};
use windows::Win32::System::Diagnostics::Etw::{
    CloseTrace, ControlTraceW, EnableTraceEx2, OpenTraceW, ProcessTrace, StartTraceW,
    CONTROLTRACE_HANDLE, EVENT_CONTROL_CODE_ENABLE_PROVIDER, EVENT_RECORD,
    EVENT_TRACE_CONTROL_STOP, EVENT_TRACE_LOGFILEW, EVENT_TRACE_PROPERTIES,
    EVENT_TRACE_REAL_TIME_MODE, PROCESS_TRACE_MODE_EVENT_RECORD, PROCESS_TRACE_MODE_REAL_TIME,
    WNODE_FLAG_TRACED_GUID,
};

pub const SESSION_NAME: &str = "Veyl-PacketCapture";
/// Microsoft-Windows-NDIS-PacketCapture.
const PROVIDER: GUID = GUID::from_u128(0x2ed6006e_4729_4609_b423_3ee7bcd678ef);
/// "Packet fragment" event: MiniportIfIndex, LowerIfIndex, FragmentSize, Fragment.
const EVENT_FRAGMENT: u16 = 1001;
const KW_MEDIA_WIRELESS_WAN: u64 = 0x200;
const KW_MEDIA_NATIVE_802_11: u64 = 0x1_0000;
const KW_PACKET_START: u64 = 0x4000_0000;
const KW_PACKET_END: u64 = 0x8000_0000;
const KW_SEND: u64 = 0x1_0000_0000;
const KW_RECEIVE: u64 = 0x2_0000_0000;
/// Longest frame kept; larger (offloaded) sends are truncated.
pub const SNAP_LEN: usize = 65_535;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Media {
    Ethernet,
    /// Mobile broadband: IP packets without a link header.
    RawIp,
    /// Raw 802.11 frames (monitor mode only).
    Wifi80211,
}

#[derive(Debug, Clone)]
pub struct RawPacket {
    /// FILETIME (100 ns ticks since 1601, UTC).
    pub time_100ns: i64,
    pub if_index: u32,
    /// `Some(true)` sent, `Some(false)` received.
    pub outbound: Option<bool>,
    pub media: Media,
    /// Bytes on the wire before truncation to [`SNAP_LEN`].
    pub orig_len: u32,
    pub data: Vec<u8>,
}

/// Splits a fragment event's user data into (miniport interface index, bytes).
pub fn parse_fragment(user_data: &[u8]) -> Option<(u32, &[u8])> {
    let le = |o: usize| {
        user_data
            .get(o..o + 4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    };
    let size = le(8)? as usize;
    Some((le(0)?, user_data.get(12..12 + size)?))
}

fn media(keywords: u64) -> Media {
    if keywords & KW_MEDIA_WIRELESS_WAN != 0 {
        Media::RawIp
    } else if keywords & KW_MEDIA_NATIVE_802_11 != 0 {
        Media::Wifi80211
    } else {
        Media::Ethernet
    }
}

fn direction(keywords: u64) -> Option<bool> {
    match (keywords & KW_SEND != 0, keywords & KW_RECEIVE != 0) {
        (true, false) => Some(true),
        (false, true) => Some(false),
        _ => None,
    }
}

/// Joins the fragments of one frame. A frame is logged as consecutive events on
/// one thread, the first flagged PACKET_START and the last PACKET_END.
#[derive(Default)]
pub struct Reassembler {
    partial: HashMap<u32, (Vec<u8>, usize)>,
}

impl Reassembler {
    /// Adds a fragment; returns `(frame, original length)` when it completes one.
    pub fn push(&mut self, thread: u32, keywords: u64, bytes: &[u8]) -> Option<(Vec<u8>, usize)> {
        let start = keywords & KW_PACKET_START != 0;
        let end = keywords & KW_PACKET_END != 0;
        let take = |buf: &mut Vec<u8>| {
            let room = SNAP_LEN.saturating_sub(buf.len());
            buf.extend_from_slice(&bytes[..bytes.len().min(room)]);
        };
        if start || (!end && !self.partial.contains_key(&thread)) {
            let mut buf = Vec::with_capacity(bytes.len().min(SNAP_LEN));
            take(&mut buf);
            if end || !start {
                // Whole frame in one event (or a provider that does not flag fragments).
                self.partial.remove(&thread);
                return Some((buf, bytes.len()));
            }
            self.partial.insert(thread, (buf, bytes.len()));
            return None;
        }
        let (buf, total) = self.partial.get_mut(&thread)?; // a middle fragment without its start is dropped
        take(buf);
        *total += bytes.len();
        if end {
            return self.partial.remove(&thread);
        }
        None
    }
}

#[repr(C)]
struct Props {
    p: EVENT_TRACE_PROPERTIES,
    name: [u16; 64],
}

fn props() -> Box<Props> {
    // SAFETY: EVENT_TRACE_PROPERTIES is plain data; all-zero is a valid starting state.
    let mut b: Box<Props> = Box::new(unsafe { std::mem::zeroed() });
    b.p.Wnode.BufferSize = std::mem::size_of::<Props>() as u32;
    b.p.Wnode.Flags = WNODE_FLAG_TRACED_GUID;
    b.p.Wnode.ClientContext = 1; // QPC timestamps, converted to precise FILETIME on read
    b.p.LogFileMode = EVENT_TRACE_REAL_TIME_MODE;
    b.p.BufferSize = 256; // KB
    b.p.MinimumBuffers = 16;
    b.p.MaximumBuffers = 128;
    b.p.FlushTimer = 1; // deliver at least once a second
    b.p.LoggerNameOffset = std::mem::offset_of!(Props, name) as u32;
    b
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn stop_session() -> WIN32_ERROR {
    let name = wide(SESSION_NAME);
    let mut p = props();
    // SAFETY: properties buffer sized for the session name; the name is NUL-terminated.
    unsafe {
        ControlTraceW(
            CONTROLTRACE_HANDLE::default(),
            PCWSTR(name.as_ptr()),
            &mut p.p,
            EVENT_TRACE_CONTROL_STOP,
        )
    }
}

/// Stops a session left behind by a previous run that did not exit cleanly.
pub fn stop_stale_session() {
    let _ = stop_session();
}

struct Consumer {
    sink: Box<dyn FnMut(RawPacket) + Send>,
    reassembler: Reassembler,
}

unsafe extern "system" fn on_event(record: *mut EVENT_RECORD) {
    // SAFETY: ETW passes a valid record for the duration of the callback.
    let r = unsafe { &*record };
    let h = &r.EventHeader;
    if h.ProviderId != PROVIDER || h.EventDescriptor.Id != EVENT_FRAGMENT || r.UserData.is_null() {
        return;
    }
    // SAFETY: UserContext is the Consumer owned by the capture thread until ProcessTrace returns.
    let c = unsafe { &mut *(r.UserContext as *mut Consumer) };
    // SAFETY: UserData points to UserDataLength bytes.
    let data =
        unsafe { std::slice::from_raw_parts(r.UserData as *const u8, r.UserDataLength as usize) };
    let Some((if_index, fragment)) = parse_fragment(data) else {
        return;
    };
    let kw = h.EventDescriptor.Keyword;
    if let Some((frame, orig_len)) = c.reassembler.push(h.ThreadId, kw, fragment) {
        (c.sink)(RawPacket {
            time_100ns: h.TimeStamp,
            if_index,
            outbound: direction(kw),
            media: media(kw),
            orig_len: orig_len as u32,
            data: frame,
        });
    }
}

/// A running capture; stops when [`PacketCapture::stop`] is called or it is dropped.
pub struct PacketCapture {
    thread: Option<JoinHandle<()>>,
}

impl PacketCapture {
    /// Starts capturing on every adapter; `sink` runs on the capture thread for each frame.
    pub fn start(sink: Box<dyn FnMut(RawPacket) + Send>) -> Result<Self, String> {
        stop_stale_session();
        let name = wide(SESSION_NAME);
        let mut p = props();
        let mut control = CONTROLTRACE_HANDLE::default();
        // SAFETY: out-handle, NUL-terminated name, properties sized for the name.
        let rc = unsafe { StartTraceW(&mut control, PCWSTR(name.as_ptr()), &mut p.p) };
        if rc == ERROR_ACCESS_DENIED {
            return Err(
                "Packet capture needs administrator rights. Restart Veyl as administrator.".into(),
            );
        }
        if rc != ERROR_SUCCESS {
            return Err(format!(
                "Could not start the capture session (StartTrace error {})",
                rc.0
            ));
        }
        // SAFETY: valid session handle and provider GUID.
        let rc = unsafe {
            EnableTraceEx2(
                control,
                &PROVIDER,
                EVENT_CONTROL_CODE_ENABLE_PROVIDER.0,
                5, // TRACE_LEVEL_VERBOSE
                u64::MAX,
                0,
                0,
                None,
            )
        };
        if rc != ERROR_SUCCESS {
            let _ = stop_session();
            return Err(format!(
                "Windows packet capture could not be enabled (error {})",
                rc.0
            ));
        }

        let ctx = Box::into_raw(Box::new(Consumer {
            sink,
            reassembler: Reassembler::default(),
        }));
        let mut logger = wide(SESSION_NAME);
        let mut logfile = EVENT_TRACE_LOGFILEW {
            LoggerName: PWSTR(logger.as_mut_ptr()),
            Context: ctx.cast::<c_void>(),
            ..Default::default()
        };
        logfile.Anonymous1.ProcessTraceMode =
            PROCESS_TRACE_MODE_REAL_TIME | PROCESS_TRACE_MODE_EVENT_RECORD;
        logfile.Anonymous2.EventRecordCallback = Some(on_event);
        // SAFETY: logfile fully initialised; the logger name outlives the call.
        let trace = unsafe { OpenTraceW(&mut logfile) };
        if trace.Value == u64::MAX {
            let _ = stop_session();
            // SAFETY: no consumer was opened, so nothing else references ctx.
            drop(unsafe { Box::from_raw(ctx) });
            return Err("Could not read the capture session (OpenTrace failed)".into());
        }
        let ctx_addr = ctx as usize;
        let thread = std::thread::Builder::new()
            .name("packet-capture".into())
            .spawn(move || {
                // SAFETY: blocks delivering events until the session stops.
                let _ = unsafe { ProcessTrace(&[trace], None, None) };
                // SAFETY: handle from OpenTrace; closed once.
                let _ = unsafe { CloseTrace(trace) };
                // SAFETY: ProcessTrace has returned, so no callback can use ctx any more.
                drop(unsafe { Box::from_raw(ctx_addr as *mut Consumer) });
            })
            .map_err(|e| {
                let _ = stop_session();
                format!("Could not start the capture thread: {e}")
            })?;
        Ok(Self {
            thread: Some(thread),
        })
    }

    /// False once the capture thread has ended, e.g. because the session was stopped from outside.
    pub fn is_active(&self) -> bool {
        self.thread.as_ref().is_some_and(|t| !t.is_finished())
    }

    pub fn stop(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        if let Some(t) = self.thread.take() {
            let _ = stop_session(); // ends ProcessTrace
            let _ = t.join();
        }
    }
}

impl Drop for PacketCapture {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(if_index: u32, bytes: &[u8]) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&if_index.to_le_bytes());
        v.extend_from_slice(&if_index.to_le_bytes());
        v.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        v.extend_from_slice(bytes);
        v
    }

    #[test]
    fn parses_fragment_event() {
        let e = event(7, &[1, 2, 3]);
        assert_eq!(parse_fragment(&e), Some((7, &[1u8, 2, 3][..])));
        assert_eq!(
            parse_fragment(&e[..e.len() - 1]),
            None,
            "size beyond the data"
        );
    }

    #[test]
    fn reassembles_fragments_per_thread() {
        let mut r = Reassembler::default();
        assert_eq!(
            r.push(1, KW_PACKET_START | KW_PACKET_END, b"ab"),
            Some((b"ab".to_vec(), 2))
        );
        assert_eq!(r.push(1, KW_PACKET_START, b"he"), None);
        assert_eq!(
            r.push(2, KW_PACKET_START | KW_PACKET_END, b"x"),
            Some((b"x".to_vec(), 1))
        );
        assert_eq!(r.push(1, 0, b"ll"), None);
        assert_eq!(r.push(1, KW_PACKET_END, b"o"), Some((b"hello".to_vec(), 5)));
        assert_eq!(
            r.push(3, KW_PACKET_END, b"orphan"),
            None,
            "end without start dropped"
        );
        assert_eq!(r.push(4, 0, b"unflagged"), Some((b"unflagged".to_vec(), 9)));
    }

    #[test]
    fn keywords_give_direction_and_media() {
        assert_eq!(direction(KW_SEND), Some(true));
        assert_eq!(direction(KW_RECEIVE), Some(false));
        assert_eq!(media(KW_MEDIA_WIRELESS_WAN), Media::RawIp);
        assert_eq!(media(1), Media::Ethernet);
    }
}
