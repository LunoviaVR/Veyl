//! Network adapter throughput from `GetIfTable2` octet counters.

use std::collections::HashMap;
use std::time::Instant;

use pch_core::rate::per_second;
use pch_core::{NetworkAdapterSnapshot, NetworkSnapshot, Status};
use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::NetworkManagement::IpHelper::{
    FreeMibTable, GetIfTable2, IF_TYPE_ETHERNET_CSMACD, IF_TYPE_IEEE80211, IF_TYPE_PPP,
    IF_TYPE_WWANPP, MIB_IF_TABLE2,
};
use windows::Win32::NetworkManagement::Ndis::{IfOperStatusUp, MediaConnectStateConnected};

use crate::handle::from_wide;

/// netioapi.h `InterfaceAndOperStatusFlags`: bit 0 HardwareInterface, bit 1 FilterInterface.
const FLAG_HARDWARE: u8 = 0x01;
const FLAG_FILTER: u8 = 0x02;

fn media_name(if_type: u32) -> &'static str {
    match if_type {
        IF_TYPE_ETHERNET_CSMACD => "Ethernet",
        IF_TYPE_IEEE80211 => "Wi-Fi",
        IF_TYPE_WWANPP => "Mobile broadband",
        IF_TYPE_PPP => "PPP",
        _ => "Other",
    }
}

pub struct NetworkSampler {
    prev: HashMap<u64, (u64, u64)>,
    prev_at: Option<Instant>,
}

impl NetworkSampler {
    pub fn new() -> Self {
        let mut s = Self {
            prev: HashMap::new(),
            prev_at: None,
        };
        let _ = s.sample(); // baseline so the first published sample has rates
        s
    }

    pub fn sample(&mut self) -> NetworkSnapshot {
        let mut snap = NetworkSnapshot::default();
        let mut table: *mut MIB_IF_TABLE2 = std::ptr::null_mut();
        let at = Instant::now();
        // SAFETY: out-pointer; freed with FreeMibTable below.
        let rc = unsafe { GetIfTable2(&mut table) };
        if rc != ERROR_SUCCESS || table.is_null() {
            snap.status = Status::error(format!("GetIfTable2 failed ({})", rc.0));
            self.prev_at = None;
            return snap;
        }
        let elapsed = self.prev_at.map(|p| at.duration_since(p));
        let mut next = HashMap::new();
        // SAFETY: GetIfTable2 returned a table of NumEntries rows; read before FreeMibTable.
        unsafe {
            let n = (*table).NumEntries as usize;
            let rows = std::slice::from_raw_parts((*table).Table.as_ptr(), n);
            for r in rows {
                let flags = r.InterfaceAndOperStatusFlags._bitfield;
                if flags & FLAG_HARDWARE == 0 || flags & FLAG_FILTER != 0 {
                    continue;
                }
                // SAFETY: NET_LUID_LH is a union over a u64 Value.
                let luid = r.InterfaceLuid.Value;
                next.insert(luid, (r.InOctets, r.OutOctets));
                let prev = self.prev.get(&luid);
                let (rx, tx) = match (prev, elapsed) {
                    (Some(&(pi, po)), Some(e)) => (
                        per_second(pi, r.InOctets, e),
                        per_second(po, r.OutOctets, e),
                    ),
                    _ => (None, None),
                };
                snap.adapters.push(NetworkAdapterSnapshot {
                    luid,
                    alias: from_wide(&r.Alias),
                    description: from_wide(&r.Description),
                    media: media_name(r.Type).into(),
                    connected: r.OperStatus == IfOperStatusUp
                        && r.MediaConnectState == MediaConnectStateConnected,
                    link_speed_bps: (r.ReceiveLinkSpeed > 0 && r.ReceiveLinkSpeed != u64::MAX)
                        .then_some(r.ReceiveLinkSpeed / 8),
                    rx_bps: rx,
                    tx_bps: tx,
                    rx_total_bytes: r.InOctets,
                    tx_total_bytes: r.OutOctets,
                });
            }
            FreeMibTable(table.cast());
        }
        self.prev = next;
        self.prev_at = Some(at);
        if elapsed.is_some() {
            snap.total_rx_bps = Some(snap.adapters.iter().filter_map(|a| a.rx_bps).sum());
            snap.total_tx_bps = Some(snap.adapters.iter().filter_map(|a| a.tx_bps).sum());
            snap.status = Status::available();
        } else {
            snap.status = Status::waiting("Waiting for the second network sample");
        }
        snap.adapters
            .sort_by(|a, b| b.connected.cmp(&a.connected).then(a.alias.cmp(&b.alias)));
        snap
    }
}

impl Default for NetworkSampler {
    fn default() -> Self {
        Self::new()
    }
}
