//! Live packet capture for the Network page. Packets are kept in memory only, in
//! a bounded buffer (oldest dropped first), never written to disk or sent
//! anywhere, and are cleared when a new capture starts or the user clears them.

use std::collections::VecDeque;
use std::sync::Arc;

use parking_lot::Mutex;
use serde::Serialize;
use veyl_core::packet::{dissect, LinkType};
use veyl_windows::packet_capture::{Media, PacketCapture, RawPacket};

pub const MAX_PACKETS: usize = 200_000;
pub const MAX_BYTES: usize = 128 * 1024 * 1024;
/// Rows returned per request; the UI asks again for the rest.
pub const PAGE: usize = 20_000;

pub struct Packet {
    pub id: u64,
    pub time_100ns: i64,
    pub if_index: u32,
    pub outbound: Option<bool>,
    pub link: LinkType,
    pub orig_len: u32,
    pub data: Vec<u8>,
    pub src: String,
    pub dst: String,
    pub protocol: String,
    pub info: String,
}

impl Packet {
    fn cost(&self) -> usize {
        self.data.len()
            + self.src.len()
            + self.dst.len()
            + self.protocol.len()
            + self.info.len()
            + 96
    }
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PacketRow {
    pub id: u64,
    /// Microseconds since the capture started.
    pub time_us: i64,
    pub if_index: u32,
    pub outbound: Option<bool>,
    pub src: String,
    pub dst: String,
    pub protocol: String,
    pub length: u32,
    pub info: String,
}

#[derive(Default)]
pub struct Store {
    pub packets: VecDeque<Packet>,
    bytes: usize,
    next_id: u64,
    /// Packets removed to stay within the memory limit.
    pub evicted: u64,
    pub start_100ns: Option<i64>,
    /// Only packets from this interface are kept; `None` keeps all.
    pub interface: Option<u32>,
}

impl Store {
    /// Empties the buffer; numbering starts again at 1, as in Wireshark.
    pub fn reset(&mut self, interface: Option<u32>) {
        *self = Store {
            interface,
            ..Store::default()
        };
    }

    fn push(&mut self, raw: RawPacket) {
        if self.interface.is_some_and(|i| i != raw.if_index) {
            return;
        }
        let link = match raw.media {
            Media::Ethernet => LinkType::Ethernet,
            Media::RawIp => LinkType::RawIp,
            Media::Wifi80211 => LinkType::Ieee80211,
        };
        let d = dissect(&raw.data, link, raw.orig_len as usize);
        self.next_id += 1;
        self.start_100ns.get_or_insert(raw.time_100ns);
        let p = Packet {
            id: self.next_id,
            time_100ns: raw.time_100ns,
            if_index: raw.if_index,
            outbound: raw.outbound,
            link,
            orig_len: raw.orig_len,
            data: raw.data,
            src: d.src,
            dst: d.dst,
            protocol: d.protocol,
            info: d.info,
        };
        self.bytes += p.cost();
        self.packets.push_back(p);
        while self.packets.len() > MAX_PACKETS || self.bytes > MAX_BYTES {
            let Some(old) = self.packets.pop_front() else {
                break;
            };
            self.bytes -= old.cost();
            self.evicted += 1;
        }
    }

    pub fn row(&self, p: &Packet) -> PacketRow {
        PacketRow {
            id: p.id,
            time_us: (p.time_100ns - self.start_100ns.unwrap_or(p.time_100ns)) / 10,
            if_index: p.if_index,
            outbound: p.outbound,
            src: p.src.clone(),
            dst: p.dst.clone(),
            protocol: p.protocol.clone(),
            length: p.orig_len,
            info: p.info.clone(),
        }
    }

    pub fn get(&self, id: u64) -> Option<&Packet> {
        // Ids are consecutive, so the position follows from the first id.
        let first = self.packets.front()?.id;
        self.packets
            .get(id.checked_sub(first)? as usize)
            .filter(|p| p.id == id)
    }
}

#[derive(Default)]
pub struct Capture {
    pub store: Arc<Mutex<Store>>,
    pub running: Mutex<Option<PacketCapture>>,
    pub error: Mutex<Option<String>>,
}

impl Capture {
    pub fn start(&self, interface: Option<u32>) -> Result<(), String> {
        self.stop();
        self.store.lock().reset(interface);
        *self.error.lock() = None;
        let store = self.store.clone();
        let cap = PacketCapture::start(Box::new(move |p| store.lock().push(p)))
            .inspect_err(|e| *self.error.lock() = Some(e.clone()))?;
        *self.running.lock() = Some(cap);
        Ok(())
    }

    /// Stops capturing; the packets stay until cleared or the next capture starts.
    pub fn stop(&self) {
        // Taken out first: stopping joins the capture thread, which may be waiting on the store lock.
        let cap = self.running.lock().take();
        if let Some(c) = cap {
            c.stop();
        }
    }

    pub fn is_running(&self) -> bool {
        self.running
            .lock()
            .as_ref()
            .is_some_and(PacketCapture::is_active)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(if_index: u32, t: i64, len: usize) -> RawPacket {
        RawPacket {
            time_100ns: t,
            if_index,
            outbound: Some(true),
            media: Media::Ethernet,
            orig_len: len as u32,
            data: vec![0; len],
        }
    }

    #[test]
    fn keeps_interface_ids_and_limits() {
        let mut s = Store::default();
        s.reset(Some(9));
        s.push(raw(15, 100, 60));
        s.push(raw(9, 1_000, 60));
        s.push(raw(9, 26_000, 60));
        assert_eq!(s.packets.len(), 2, "other interface skipped");
        let ids: Vec<u64> = s.packets.iter().map(|p| p.id).collect();
        assert_eq!(ids, vec![1, 2]);
        assert_eq!(s.row(&s.packets[1]).time_us, 2_500);
        assert_eq!(s.get(2).map(|p| p.time_100ns), Some(26_000));
        assert!(s.get(3).is_none() && s.get(0).is_none());

        s.reset(None);
        for i in 0..(MAX_PACKETS + 5) {
            s.push(raw(1, i as i64, 0));
        }
        assert_eq!(s.packets.len(), MAX_PACKETS);
        assert_eq!(s.evicted, 5);
        let first = s.packets.front().expect("packets kept").id;
        assert_eq!(
            s.get(first + 10).map(|p| p.id),
            Some(first + 10),
            "lookup after eviction"
        );
    }
}
