//! Windows Event Log reader (wevtapi): history queries, push subscriptions and
//! message formatting. Read-only: this module never clears or writes logs.

use pch_core::events::{parse_event_xml, RawEvent};
use windows::Win32::Foundation::{ERROR_INSUFFICIENT_BUFFER, WAIT_OBJECT_0};
use windows::Win32::System::EventLog::{
    EvtClose, EvtFormatMessage, EvtFormatMessageEvent, EvtNext, EvtOpenPublisherMetadata, EvtQuery,
    EvtQueryChannelPath, EvtQueryReverseDirection, EvtRender, EvtRenderEventXml, EvtSubscribe,
    EvtSubscribeToFutureEvents, EVT_HANDLE,
};
use windows::Win32::System::Threading::{CreateEventW, WaitForSingleObject};

use crate::handle::{from_wide, wide, OwnedHandle};

#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum EventLogError {
    #[error("Access to the {0} log was denied")]
    AccessDenied(String),
    #[error("The {0} log does not exist")]
    NotFound(String),
    #[error("{call} failed for {channel}: {message}")]
    Os {
        call: &'static str,
        channel: String,
        message: String,
    },
}

struct Evt(EVT_HANDLE);

impl Drop for Evt {
    fn drop(&mut self) {
        if self.0 .0 != 0 {
            // SAFETY: closing a handle we own exactly once.
            if let Err(e) = unsafe { EvtClose(self.0) } {
                tracing::warn!("EvtClose failed: {e}");
            }
        }
    }
}

fn map_err(call: &'static str, channel: &str, e: windows::core::Error) -> EventLogError {
    match e.code().0 as u32 & 0xFFFF {
        5 => EventLogError::AccessDenied(channel.into()),
        // ERROR_EVT_CHANNEL_NOT_FOUND (15007)
        15007 => EventLogError::NotFound(channel.into()),
        _ => EventLogError::Os {
            call,
            channel: channel.into(),
            message: e.message(),
        },
    }
}

fn render_xml(event: EVT_HANDLE) -> Option<String> {
    let mut used = 0u32;
    let mut count = 0u32;
    // SAFETY: size query.
    let r = unsafe {
        EvtRender(
            None,
            event,
            EvtRenderEventXml.0,
            0,
            None,
            &mut used,
            &mut count,
        )
    };
    if let Err(e) = r {
        if e.code() != ERROR_INSUFFICIENT_BUFFER.to_hresult() {
            return None;
        }
    }
    let mut buf = vec![0u16; (used as usize).div_ceil(2) + 1];
    // SAFETY: buffer holds `used` bytes.
    unsafe {
        EvtRender(
            None,
            event,
            EvtRenderEventXml.0,
            (buf.len() * 2) as u32,
            Some(buf.as_mut_ptr().cast()),
            &mut used,
            &mut count,
        )
    }
    .ok()?;
    Some(from_wide(&buf))
}

/// Reads events from a result set until it is exhausted or `max` is reached.
/// Returns parsed events and the number that could not be parsed.
fn drain(results: EVT_HANDLE, max: usize, timeout_ms: u32) -> (Vec<RawEvent>, usize) {
    let mut out = Vec::new();
    let mut bad = 0usize;
    let mut batch = [0isize; 64];
    while out.len() < max {
        let mut returned = 0u32;
        // SAFETY: valid result set; batch array of handles.
        if unsafe { EvtNext(results, &mut batch, timeout_ms, 0, &mut returned) }.is_err() {
            break; // ERROR_NO_MORE_ITEMS or timeout ends the read
        }
        for &h in &batch[..returned as usize] {
            let ev = Evt(EVT_HANDLE(h));
            match render_xml(ev.0).map(|x| parse_event_xml(&x)) {
                Some(Ok(e)) => out.push(e),
                _ => bad += 1,
            }
        }
    }
    (out, bad)
}

/// Queries `channel` with an XPath filter. `newest_first` reads in reverse order.
pub fn query(
    channel: &str,
    xpath: &str,
    newest_first: bool,
    max: usize,
) -> Result<(Vec<RawEvent>, usize), EventLogError> {
    let (c, q) = (wide(channel), wide(xpath));
    let flags = EvtQueryChannelPath.0
        | if newest_first {
            EvtQueryReverseDirection.0
        } else {
            0
        };
    // SAFETY: NUL-terminated strings; handle closed by Evt.
    let results = Evt(unsafe {
        EvtQuery(
            None,
            windows::core::PCWSTR(c.as_ptr()),
            windows::core::PCWSTR(q.as_ptr()),
            flags,
        )
    }
    .map_err(|e| map_err("EvtQuery", channel, e))?);
    Ok(drain(results.0, max, 1000))
}

/// A push subscription to future events of one channel, signalled through a
/// Win32 event so the owning thread can wait with a timeout and stop cleanly.
pub struct Subscription {
    channel: String,
    handle: Evt,
    signal: OwnedHandle,
}

impl Subscription {
    pub fn new(channel: &str, xpath: &str) -> Result<Self, EventLogError> {
        // SAFETY: unnamed auto-reset event, owned below.
        let signal = OwnedHandle(
            unsafe { CreateEventW(None, false, false, windows::core::PCWSTR::null()) }
                .map_err(|e| map_err("CreateEventW", channel, e))?,
        );
        let (c, q) = (wide(channel), wide(xpath));
        // SAFETY: pull-model subscription (no callback) signalling `signal`; strings NUL-terminated.
        let handle = unsafe {
            EvtSubscribe(
                None,
                Some(signal.raw()),
                windows::core::PCWSTR(c.as_ptr()),
                windows::core::PCWSTR(q.as_ptr()),
                None,
                None,
                None,
                EvtSubscribeToFutureEvents.0,
            )
        }
        .map_err(|e| map_err("EvtSubscribe", channel, e))?;
        Ok(Self {
            channel: channel.into(),
            handle: Evt(handle),
            signal,
        })
    }

    pub fn channel(&self) -> &str {
        &self.channel
    }

    /// Waits up to `timeout_ms` for new events; true if signalled.
    pub fn wait(&self, timeout_ms: u32) -> bool {
        // SAFETY: valid event handle.
        (unsafe { WaitForSingleObject(self.signal.raw(), timeout_ms) }) == WAIT_OBJECT_0
    }

    /// Returns all events delivered since the last call.
    pub fn take(&mut self) -> Vec<RawEvent> {
        drain(self.handle.0, 10_000, 0).0
    }
}

/// The event's rendered message text in the user's language, via the
/// publisher's message table (what Event Viewer shows in "General").
pub fn format_message(channel: &str, provider: &str, record_id: u64) -> Option<String> {
    let c = wide(channel);
    let q = wide(&format!("*[System[EventRecordID={record_id}]]"));
    // SAFETY: NUL-terminated strings; handle closed by Evt.
    let results = Evt(unsafe {
        EvtQuery(
            None,
            windows::core::PCWSTR(c.as_ptr()),
            windows::core::PCWSTR(q.as_ptr()),
            EvtQueryChannelPath.0,
        )
    }
    .ok()?);
    let mut batch = [0isize; 1];
    let mut returned = 0u32;
    // SAFETY: valid result set.
    unsafe { EvtNext(results.0, &mut batch, 1000, 0, &mut returned) }.ok()?;
    if returned == 0 {
        return None;
    }
    let event = Evt(EVT_HANDLE(batch[0]));
    let p = wide(provider);
    // SAFETY: NUL-terminated provider name; handle closed by Evt.
    let meta = Evt(unsafe {
        EvtOpenPublisherMetadata(
            None,
            windows::core::PCWSTR(p.as_ptr()),
            windows::core::PCWSTR::null(),
            0,
            0,
        )
    }
    .ok()?);
    let mut used = 0u32;
    // SAFETY: size query.
    let r = unsafe {
        EvtFormatMessage(
            Some(meta.0),
            Some(event.0),
            0,
            None,
            EvtFormatMessageEvent.0,
            None,
            &mut used,
        )
    };
    if let Err(e) = r {
        if e.code() != ERROR_INSUFFICIENT_BUFFER.to_hresult() {
            return None;
        }
    }
    let mut buf = vec![0u16; used as usize + 1];
    // SAFETY: buffer of `used` UTF-16 units.
    unsafe {
        EvtFormatMessage(
            Some(meta.0),
            Some(event.0),
            0,
            None,
            EvtFormatMessageEvent.0,
            Some(&mut buf),
            &mut used,
        )
    }
    .ok()?;
    let s = from_wide(&buf).trim().to_owned();
    (!s.is_empty()).then_some(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pch_core::events::{classify, xpath_for_channel};

    #[test]
    fn reads_recent_system_events() {
        let (events, _) = query("System", "*", true, 5).expect("System log is readable");
        assert!(!events.is_empty());
        assert!(
            events.windows(2).all(|w| w[0].record_id >= w[1].record_id),
            "newest first"
        );
    }

    #[test]
    fn rule_xpath_is_accepted_by_the_service() {
        for ch in pch_core::events::CHANNELS {
            let (events, bad) =
                query(ch, &xpath_for_channel(ch), true, 50).expect("query accepted");
            assert_eq!(bad, 0);
            for e in events {
                assert!(
                    classify(&e).is_some(),
                    "XPath returned an event the classifier rejects: {} {}",
                    e.provider,
                    e.event_id
                );
            }
        }
    }

    #[test]
    fn missing_channel_is_not_found() {
        assert!(matches!(
            query("No-Such-Channel/Nope", "*", true, 1),
            Err(EventLogError::NotFound(_))
        ));
    }

    #[test]
    fn subscription_starts_and_stops() {
        let mut s = Subscription::new("Application", "*").expect("subscribe");
        let _ = s.wait(10);
        let _ = s.take();
    }
}
