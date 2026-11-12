//! Packet dissection for the Network page: Ethernet / 802.1Q → ARP, IPv4, IPv6 →
//! TCP, UDP, ICMP, ICMPv6 → DNS (also mDNS and LLMNR), HTTP/1.x, TLS, SMB2, QUIC,
//! DHCP. Every field keeps its byte range (for the hex view) and a display-filter
//! key in Wireshark's naming. Nothing is reassembled across packets.

use std::fmt::Write as _;
use std::net::{Ipv4Addr, Ipv6Addr};

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkType {
    Ethernet,
    /// IP packet without a link header (mobile broadband).
    RawIp,
    /// Raw 802.11; shown but not decoded.
    Ieee80211,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Field {
    /// Display-filter key, e.g. `tcp.dstport`; empty for informational rows.
    #[serde(skip_serializing_if = "str::is_empty")]
    pub key: &'static str,
    pub label: String,
    pub value: String,
    pub offset: u32,
    pub len: u32,
    #[serde(skip)]
    pub num: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Layer {
    /// Filter keyword, e.g. `tcp`.
    pub proto: &'static str,
    pub name: String,
    pub summary: String,
    pub offset: u32,
    pub len: u32,
    pub fields: Vec<Field>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Dissection {
    pub layers: Vec<Layer>,
    /// Highest decoded protocol, as shown in the Protocol column.
    pub protocol: String,
    pub src: String,
    pub dst: String,
    pub info: String,
}

/// Field keys the dissector can produce, plus convenience aliases. Used to
/// reject typos in display filters.
pub const KNOWN_FIELDS: &[&str] = &[
    "frame.len",
    "frame.cap_len",
    "wlan.fc.type",
    "wlan.fc.subtype",
    "wlan.sa",
    "wlan.da",
    "eth.src",
    "eth.dst",
    "eth.addr",
    "eth.type",
    "vlan.id",
    "arp.opcode",
    "arp.src.hw_mac",
    "arp.src.proto_ipv4",
    "arp.dst.hw_mac",
    "arp.dst.proto_ipv4",
    "ip.version",
    "ip.hdr_len",
    "ip.dsfield",
    "ip.len",
    "ip.id",
    "ip.flags.df",
    "ip.flags.mf",
    "ip.frag_offset",
    "ip.ttl",
    "ip.proto",
    "ip.checksum",
    "ip.src",
    "ip.dst",
    "ip.addr",
    "ipv6.tclass",
    "ipv6.flow",
    "ipv6.plen",
    "ipv6.nxt",
    "ipv6.hlim",
    "ipv6.src",
    "ipv6.dst",
    "ipv6.addr",
    "icmp.type",
    "icmp.code",
    "icmp.checksum",
    "icmp.ident",
    "icmp.seq",
    "icmpv6.type",
    "icmpv6.code",
    "icmpv6.checksum",
    "tcp.srcport",
    "tcp.dstport",
    "tcp.port",
    "tcp.seq",
    "tcp.ack",
    "tcp.hdr_len",
    "tcp.flags",
    "tcp.flags.fin",
    "tcp.flags.syn",
    "tcp.flags.reset",
    "tcp.flags.push",
    "tcp.flags.ack",
    "tcp.flags.urg",
    "tcp.window_size_value",
    "tcp.checksum",
    "tcp.urgent_pointer",
    "tcp.len",
    "udp.srcport",
    "udp.dstport",
    "udp.port",
    "udp.length",
    "udp.checksum",
    "port",
    "dns.id",
    "dns.flags.response",
    "dns.flags.opcode",
    "dns.flags.rcode",
    "dns.count.queries",
    "dns.count.answers",
    "dns.count.auth_rr",
    "dns.count.add_rr",
    "dns.qry.name",
    "dns.qry.type",
    "dns.resp.name",
    "dns.resp.type",
    "dns.resp.ttl",
    "dns.a",
    "dns.aaaa",
    "dns.cname",
    "dns.ptr",
    "dns.ns",
    "dns.mx",
    "http.request.method",
    "http.request.uri",
    "http.request.version",
    "http.response.version",
    "http.response.code",
    "http.response.phrase",
    "http.host",
    "http.user_agent",
    "http.content_type",
    "http.content_length",
    "http.server",
    "http.location",
    "http.header",
    "tls.record.content_type",
    "tls.record.version",
    "tls.record.length",
    "tls.handshake.type",
    "tls.handshake.extensions_server_name",
    "tls.alert_message.level",
    "tls.alert_message.desc",
    "smb2.cmd",
    "smb2.nt_status",
    "smb2.flags.response",
    "smb2.msg_id",
    "quic.long.packet_type",
    "quic.version",
    "dhcp.type",
    "dhcp.option.dhcp",
    "dhcp.ip.your",
    "dhcp.hw.mac_addr",
];

/// Protocol keywords usable on their own in a filter (e.g. `dns`).
pub const PROTOCOLS: &[&str] = &[
    "frame", "eth", "wlan", "vlan", "arp", "ip", "ipv6", "icmp", "icmpv6", "tcp", "udp", "dns",
    "mdns", "llmnr", "http", "tls", "smb", "smb2", "quic", "dhcp", "ntp", "ssdp", "nbns", "stun",
    "data",
];

#[derive(Debug, Clone, PartialEq)]
pub enum FieldValue<'a> {
    Num(u64),
    Text(&'a str),
}

impl Dissection {
    pub fn has_protocol(&self, proto: &str) -> bool {
        self.layers.iter().any(|l| l.proto == proto)
    }

    /// Values of a filter field across all layers. Aliases: `ip.addr` (source or
    /// destination, IPv4 or IPv6), `ip.src`/`ip.dst` (either version),
    /// `tcp.port`/`udp.port`/`port`, `eth.addr`.
    pub fn values(&self, key: &str) -> Vec<FieldValue<'_>> {
        let single = [key];
        let keys: &[&str] = match key {
            "ip.addr" => &["ip.src", "ip.dst", "ipv6.src", "ipv6.dst"],
            "ip.src" => &["ip.src", "ipv6.src"],
            "ip.dst" => &["ip.dst", "ipv6.dst"],
            "ipv6.addr" => &["ipv6.src", "ipv6.dst"],
            "tcp.port" => &["tcp.srcport", "tcp.dstport"],
            "udp.port" => &["udp.srcport", "udp.dstport"],
            "port" => &["tcp.srcport", "tcp.dstport", "udp.srcport", "udp.dstport"],
            "eth.addr" => &["eth.src", "eth.dst"],
            _ => &single,
        };
        let mut out = Vec::new();
        for l in &self.layers {
            for f in &l.fields {
                if keys.contains(&f.key) {
                    out.push(match f.num {
                        Some(n) => FieldValue::Num(n),
                        None => FieldValue::Text(&f.value),
                    });
                }
            }
        }
        out
    }
}

// ---------------------------------------------------------------------------
// Byte helpers and the layer builder
// ---------------------------------------------------------------------------

fn u8_at(d: &[u8], o: usize) -> Option<u8> {
    d.get(o).copied()
}
fn u16_at(d: &[u8], o: usize) -> Option<u16> {
    d.get(o..o + 2).map(|b| u16::from_be_bytes([b[0], b[1]]))
}
fn u32_at(d: &[u8], o: usize) -> Option<u32> {
    d.get(o..o + 4)
        .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
}
fn ipv4_at(d: &[u8], o: usize) -> Option<Ipv4Addr> {
    d.get(o..o + 4)
        .map(|b| Ipv4Addr::new(b[0], b[1], b[2], b[3]))
}
fn ipv6_at(d: &[u8], o: usize) -> Option<Ipv6Addr> {
    let b: [u8; 16] = d.get(o..o + 16)?.try_into().ok()?;
    Some(Ipv6Addr::from(b))
}
fn mac_at(d: &[u8], o: usize) -> Option<String> {
    let b = d.get(o..o + 6)?;
    Some(
        b.iter()
            .map(|x| format!("{x:02x}"))
            .collect::<Vec<_>>()
            .join(":"),
    )
}

struct Lb {
    l: Layer,
    /// Absolute offset of this layer in the frame.
    base: usize,
}

impl Lb {
    fn new(proto: &'static str, name: &str, base: usize, len: usize) -> Self {
        Self {
            l: Layer {
                proto,
                name: name.into(),
                summary: String::new(),
                offset: base as u32,
                len: len as u32,
                fields: Vec::new(),
            },
            base,
        }
    }
    /// Numeric field; `rel` is relative to the layer start.
    fn num(&mut self, key: &'static str, label: &str, v: u64, rel: usize, len: usize) {
        self.push(key, label, v.to_string(), Some(v), rel, len);
    }
    fn text(
        &mut self,
        key: &'static str,
        label: &str,
        v: impl Into<String>,
        rel: usize,
        len: usize,
    ) {
        self.push(key, label, v.into(), None, rel, len);
    }
    /// Numeric field shown with a description, e.g. "443 (HTTPS)".
    fn named(
        &mut self,
        key: &'static str,
        label: &str,
        v: u64,
        desc: &str,
        rel: usize,
        len: usize,
    ) {
        let shown = if desc.is_empty() {
            v.to_string()
        } else {
            format!("{v} ({desc})")
        };
        self.push(key, label, shown, Some(v), rel, len);
    }
    fn push(
        &mut self,
        key: &'static str,
        label: &str,
        value: String,
        num: Option<u64>,
        rel: usize,
        len: usize,
    ) {
        self.l.fields.push(Field {
            key,
            label: label.into(),
            value,
            offset: (self.base + rel) as u32,
            len: len as u32,
            num,
        });
    }
    fn done(mut self, summary: String) -> Layer {
        self.l.summary = summary;
        self.l
    }
}

struct Out {
    layers: Vec<Layer>,
    protocol: String,
    src: String,
    dst: String,
    info: String,
}

impl Out {
    fn set(&mut self, protocol: &str, info: String) {
        self.protocol = protocol.into();
        self.info = info;
    }
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

/// Dissects one captured frame. `orig_len` is the length on the wire.
pub fn dissect(data: &[u8], link: LinkType, orig_len: usize) -> Dissection {
    let mut o = Out {
        layers: Vec::new(),
        protocol: "Frame".into(),
        src: String::new(),
        dst: String::new(),
        info: String::new(),
    };
    let mut f = Lb::new("frame", "Frame", 0, data.len());
    f.num("frame.len", "Length on the wire", orig_len as u64, 0, 0);
    f.num("frame.cap_len", "Captured length", data.len() as u64, 0, 0);
    let summary = if orig_len > data.len() {
        format!("{orig_len} bytes on wire, {} bytes captured", data.len())
    } else {
        format!("{orig_len} bytes")
    };
    o.layers.push(f.done(summary));
    match link {
        LinkType::Ethernet => ethernet(data, &mut o),
        LinkType::RawIp => match data.first().map(|b| b >> 4) {
            Some(4) => ipv4(data, 0, &mut o),
            Some(6) => ipv6(data, 0, &mut o),
            _ => o.set("Data", format!("{} bytes", data.len())),
        },
        LinkType::Ieee80211 => ieee80211(data, &mut o),
    }
    Dissection {
        layers: o.layers,
        protocol: o.protocol,
        src: o.src,
        dst: o.dst,
        info: o.info,
    }
}

fn ethernet(d: &[u8], o: &mut Out) {
    let (Some(dst), Some(src), Some(mut ty)) = (mac_at(d, 0), mac_at(d, 6), u16_at(d, 12)) else {
        o.set("Ethernet", "Truncated Ethernet header".into());
        return;
    };
    if ty <= 1500 {
        // IEEE 802.3: the field is a length and an LLC header follows (STP, CDP, NetBIOS…).
        let mut l = Lb::new("eth", "IEEE 802.3 Ethernet", 0, 14);
        l.text("eth.dst", "Destination", &dst, 0, 6);
        l.text("eth.src", "Source", &src, 6, 6);
        l.num("", "Length", ty.into(), 12, 2);
        o.layers.push(l.done(format!("Src: {src}, Dst: {dst}")));
        o.src = src;
        o.dst = dst;
        let info = match (u8_at(d, 14), u8_at(d, 15)) {
            (Some(0x42), Some(0x42)) => "Spanning Tree Protocol".to_owned(),
            (Some(dsap), Some(ssap)) => format!("LLC DSAP 0x{dsap:02x} SSAP 0x{ssap:02x}"),
            _ => "802.3 frame".to_owned(),
        };
        o.set("LLC", info);
        return;
    }
    let mut l = Lb::new("eth", "Ethernet II", 0, 14);
    l.text("eth.dst", "Destination", &dst, 0, 6);
    l.text("eth.src", "Source", &src, 6, 6);
    l.push(
        "eth.type",
        "Type",
        ethertype_text(ty),
        Some(ty.into()),
        12,
        2,
    );
    o.layers.push(l.done(format!("Src: {src}, Dst: {dst}")));
    o.src = src;
    o.dst = dst;
    o.set("Ethernet", format!("Ethertype 0x{ty:04x}"));
    let mut off = 14;
    if ty == 0x8100 {
        let (Some(tci), Some(inner)) = (u16_at(d, 14), u16_at(d, 16)) else {
            return;
        };
        let mut v = Lb::new("vlan", "802.1Q Virtual LAN", 14, 4);
        v.num("vlan.id", "ID", (tci & 0x0FFF).into(), 0, 2);
        v.push(
            "eth.type",
            "Type",
            ethertype_text(inner),
            Some(inner.into()),
            2,
            2,
        );
        o.layers.push(v.done(format!("ID: {}", tci & 0x0FFF)));
        ty = inner;
        off = 18;
    }
    match ty {
        0x0800 => ipv4(d, off, o),
        0x86DD => ipv6(d, off, o),
        0x0806 => arp(d, off, o),
        0x88CC => o.set("LLDP", "Link Layer Discovery Protocol".into()),
        0x888E => o.set("EAPOL", "802.1X authentication".into()),
        _ => {}
    }
}

/// Native 802.11 frames (logged by some Wi-Fi drivers): data frames carry an
/// LLC/SNAP header and then the same payload an Ethernet frame would.
fn ieee80211(d: &[u8], o: &mut Out) {
    let (Some(fc0), Some(fc1)) = (u8_at(d, 0), u8_at(d, 1)) else {
        o.set("802.11", "Truncated 802.11 frame".into());
        return;
    };
    let ty = (fc0 >> 2) & 3;
    let sub = fc0 >> 4;
    let (to_ds, from_ds) = (fc1 & 0x01 != 0, fc1 & 0x02 != 0);
    let kind = ["Management", "Control", "Data", "Reserved"][usize::from(ty)];
    let mut hdr = 24;
    if ty == 2 && to_ds && from_ds {
        hdr += 6;
    }
    if ty == 2 && sub & 0x8 != 0 {
        hdr += 2; // QoS control
        if fc1 & 0x80 != 0 {
            hdr += 4; // HT control
        }
    }
    let mut l = Lb::new("wlan", "IEEE 802.11", 0, hdr.min(d.len()));
    l.num("wlan.fc.type", "Type", ty.into(), 0, 1);
    l.num("wlan.fc.subtype", "Subtype", sub.into(), 0, 1);
    // Source and destination depend on the To DS / From DS bits.
    let (sa_at, da_at) = match (to_ds, from_ds) {
        (false, false) => (10, 4),
        (true, false) => (10, 16),
        (false, true) => (16, 4),
        (true, true) => (24, 16),
    };
    let (sa, da) = if ty == 2 {
        (mac_at(d, sa_at), mac_at(d, da_at))
    } else {
        (mac_at(d, 10), mac_at(d, 4))
    };
    if let Some(sa) = &sa {
        l.text("wlan.sa", "Source", sa, sa_at, 6);
        o.src = sa.clone();
    }
    if let Some(da) = &da {
        l.text("wlan.da", "Destination", da, da_at, 6);
        o.dst = da.clone();
    }
    o.layers.push(l.done(format!("{kind} frame")));
    if ty != 2 {
        o.set("802.11", format!("{kind} frame, subtype {sub}"));
        return;
    }
    let snap = d.get(hdr..hdr + 6) == Some(&[0xAA, 0xAA, 0x03, 0, 0, 0][..]);
    match (snap, u16_at(d, hdr + 6)) {
        (true, Some(0x0800)) => ipv4(d, hdr + 8, o),
        (true, Some(0x86DD)) => ipv6(d, hdr + 8, o),
        (true, Some(0x0806)) => arp(d, hdr + 8, o),
        (true, Some(t)) => o.set("802.11", format!("Data, ethertype 0x{t:04x}")),
        _ if sub & 0x4 != 0 => o.set("802.11", "Null function (no data)".into()),
        _ => o.set("802.11", "Data frame (encrypted or not LLC)".into()),
    }
}

/// "0x0800 (IPv4)": hex as Wireshark shows it; the "(…)" part is dropped when filtering on the field.
fn ethertype_text(t: u16) -> String {
    match ethertype(t) {
        "" => format!("0x{t:04x}"),
        name => format!("0x{t:04x} ({name})"),
    }
}

fn ethertype(t: u16) -> &'static str {
    match t {
        0x0800 => "IPv4",
        0x86DD => "IPv6",
        0x0806 => "ARP",
        0x8100 => "802.1Q",
        0x88CC => "LLDP",
        0x888E => "802.1X",
        _ => "",
    }
}

fn arp(d: &[u8], b: usize, o: &mut Out) {
    let p = &d[b.min(d.len())..];
    let (Some(op), Some(sha), Some(spa), Some(tha), Some(tpa)) = (
        u16_at(p, 6),
        mac_at(p, 8),
        ipv4_at(p, 14),
        mac_at(p, 18),
        ipv4_at(p, 24),
    ) else {
        o.set("ARP", "Truncated or non-IPv4 ARP".into());
        return;
    };
    let mut l = Lb::new("arp", "Address Resolution Protocol", b, 28);
    l.named(
        "arp.opcode",
        "Opcode",
        op.into(),
        match op {
            1 => "request",
            2 => "reply",
            _ => "",
        },
        6,
        2,
    );
    l.text("arp.src.hw_mac", "Sender MAC", &sha, 8, 6);
    l.text("arp.src.proto_ipv4", "Sender IP", spa.to_string(), 14, 4);
    l.text("arp.dst.hw_mac", "Target MAC", &tha, 18, 6);
    l.text("arp.dst.proto_ipv4", "Target IP", tpa.to_string(), 24, 4);
    let info = match op {
        1 if spa == tpa => format!("ARP announcement for {spa}"),
        1 => format!("Who has {tpa}? Tell {spa}"),
        2 => format!("{spa} is at {sha}"),
        _ => format!("Opcode {op}"),
    };
    o.layers.push(l.done(info.clone()));
    o.set("ARP", info);
}

fn ip_proto(p: u8) -> &'static str {
    match p {
        1 => "ICMP",
        2 => "IGMP",
        6 => "TCP",
        17 => "UDP",
        41 => "IPv6",
        47 => "GRE",
        50 => "ESP",
        58 => "ICMPv6",
        _ => "",
    }
}

fn ipv4(d: &[u8], b: usize, o: &mut Out) {
    let p = &d[b.min(d.len())..];
    let (Some(vihl), Some(total), Some(src), Some(dst)) =
        (u8_at(p, 0), u16_at(p, 2), ipv4_at(p, 12), ipv4_at(p, 16))
    else {
        o.set("IPv4", "Truncated IPv4 header".into());
        return;
    };
    let ihl = usize::from(vihl & 0x0F) * 4;
    if ihl < 20 || p.len() < ihl {
        o.set("IPv4", "Invalid IPv4 header length".into());
        return;
    }
    let flags = u16_at(p, 6).unwrap_or(0);
    let proto = p[9];
    let ttl = p[8];
    let frag = flags & 0x1FFF;
    let mut l = Lb::new("ip", "Internet Protocol Version 4", b, ihl);
    l.num("ip.version", "Version", 4, 0, 1);
    l.num("ip.hdr_len", "Header length", ihl as u64, 0, 1);
    l.num("ip.dsfield", "Differentiated services", p[1].into(), 1, 1);
    l.num("ip.len", "Total length", total.into(), 2, 2);
    l.num(
        "ip.id",
        "Identification",
        u16_at(p, 4).unwrap_or(0).into(),
        4,
        2,
    );
    l.num(
        "ip.flags.df",
        "Don't fragment",
        u64::from(flags & 0x4000 != 0),
        6,
        1,
    );
    l.num(
        "ip.flags.mf",
        "More fragments",
        u64::from(flags & 0x2000 != 0),
        6,
        1,
    );
    l.num(
        "ip.frag_offset",
        "Fragment offset",
        u64::from(frag) * 8,
        6,
        2,
    );
    l.num("ip.ttl", "Time to live", ttl.into(), 8, 1);
    l.named("ip.proto", "Protocol", proto.into(), ip_proto(proto), 9, 1);
    l.text(
        "ip.checksum",
        "Header checksum",
        format!("0x{:04x}", u16_at(p, 10).unwrap_or(0)),
        10,
        2,
    );
    l.text("ip.src", "Source address", src.to_string(), 12, 4);
    l.text("ip.dst", "Destination address", dst.to_string(), 16, 4);
    o.layers.push(l.done(format!("Src: {src}, Dst: {dst}")));
    o.src = src.to_string();
    o.dst = dst.to_string();
    o.set("IPv4", format!("Protocol {proto}"));
    // Large sends captured before segmentation can carry a total length of 0.
    let end = if total as usize >= ihl && (total as usize) <= p.len() {
        total as usize
    } else {
        p.len()
    };
    if frag != 0 {
        o.set(
            "IPv4",
            format!("Fragment of an IP packet (offset {})", u32::from(frag) * 8),
        );
        return;
    }
    transport(&d[..b + end], b + ihl, proto, o);
}

fn ipv6(d: &[u8], b: usize, o: &mut Out) {
    let p = &d[b.min(d.len())..];
    let (Some(first), Some(plen), Some(src), Some(dst)) =
        (u32_at(p, 0), u16_at(p, 4), ipv6_at(p, 8), ipv6_at(p, 24))
    else {
        o.set("IPv6", "Truncated IPv6 header".into());
        return;
    };
    let mut next = p[6];
    let mut l = Lb::new("ipv6", "Internet Protocol Version 6", b, 40);
    l.num("ip.version", "Version", 6, 0, 1);
    l.num(
        "ipv6.tclass",
        "Traffic class",
        ((first >> 20) & 0xFF).into(),
        0,
        2,
    );
    l.num("ipv6.flow", "Flow label", (first & 0xF_FFFF).into(), 1, 3);
    l.num("ipv6.plen", "Payload length", plen.into(), 4, 2);
    l.named("ipv6.nxt", "Next header", next.into(), ip_proto(next), 6, 1);
    l.num("ipv6.hlim", "Hop limit", p[7].into(), 7, 1);
    l.text("ipv6.src", "Source address", src.to_string(), 8, 16);
    l.text("ipv6.dst", "Destination address", dst.to_string(), 24, 16);
    let mut off = 40;
    // Skip hop-by-hop, routing, destination-options and fragment extension headers.
    for _ in 0..8 {
        match next {
            0 | 43 | 60 => {
                let (Some(n), Some(len)) = (u8_at(p, off), u8_at(p, off + 1)) else {
                    break;
                };
                next = n;
                off += (usize::from(len) + 1) * 8;
            }
            44 => {
                let (Some(n), Some(fo)) = (u8_at(p, off), u16_at(p, off + 2)) else {
                    break;
                };
                if fo >> 3 != 0 {
                    o.layers.push(l.done(format!("Src: {src}, Dst: {dst}")));
                    o.src = src.to_string();
                    o.dst = dst.to_string();
                    o.set("IPv6", "Fragment of an IPv6 packet".into());
                    return;
                }
                next = n;
                off += 8;
            }
            _ => break,
        }
    }
    o.layers.push(l.done(format!("Src: {src}, Dst: {dst}")));
    o.src = src.to_string();
    o.dst = dst.to_string();
    o.set("IPv6", format!("Next header {next}"));
    let end = if plen > 0 && 40 + (plen as usize) <= p.len() {
        40 + plen as usize
    } else {
        p.len()
    };
    if off <= end {
        transport(&d[..b + end], b + off, next, o);
    }
}

fn transport(d: &[u8], b: usize, proto: u8, o: &mut Out) {
    match proto {
        6 => tcp(d, b, o),
        17 => udp(d, b, o),
        1 => icmp(d, b, o, false),
        58 => icmp(d, b, o, true),
        2 => o.set("IGMP", "Internet Group Management Protocol".into()),
        47 => o.set("GRE", "Generic Routing Encapsulation".into()),
        50 => o.set("ESP", "Encapsulating Security Payload".into()),
        _ => {}
    }
}

fn icmp(d: &[u8], b: usize, o: &mut Out, v6: bool) {
    let p = &d[b.min(d.len())..];
    let (Some(ty), Some(code)) = (u8_at(p, 0), u8_at(p, 1)) else {
        o.set(if v6 { "ICMPv6" } else { "ICMP" }, "Truncated".into());
        return;
    };
    let desc = if v6 {
        icmpv6_type(ty, code)
    } else {
        icmp_type(ty, code)
    };
    let (proto, name, k) = if v6 {
        (
            "icmpv6",
            "Internet Control Message Protocol v6",
            ["icmpv6.type", "icmpv6.code", "icmpv6.checksum"],
        )
    } else {
        (
            "icmp",
            "Internet Control Message Protocol",
            ["icmp.type", "icmp.code", "icmp.checksum"],
        )
    };
    let mut l = Lb::new(proto, name, b, p.len());
    l.named(k[0], "Type", ty.into(), &desc, 0, 1);
    l.num(k[1], "Code", code.into(), 1, 1);
    l.text(
        k[2],
        "Checksum",
        format!("0x{:04x}", u16_at(p, 2).unwrap_or(0)),
        2,
        2,
    );
    let mut info = desc.clone();
    let echo = if v6 {
        ty == 128 || ty == 129
    } else {
        ty == 0 || ty == 8
    };
    if echo && !v6 {
        if let (Some(id), Some(seq)) = (u16_at(p, 4), u16_at(p, 6)) {
            l.num("icmp.ident", "Identifier", id.into(), 4, 2);
            l.num("icmp.seq", "Sequence", seq.into(), 6, 2);
            let _ = write!(info, " id=0x{id:04x}, seq={seq}");
        }
    }
    o.layers.push(l.done(desc));
    o.set(if v6 { "ICMPv6" } else { "ICMP" }, info);
}

fn icmp_type(t: u8, c: u8) -> String {
    match t {
        0 => "Echo (ping) reply".into(),
        3 => format!(
            "Destination unreachable ({})",
            match c {
                0 => "network unreachable",
                1 => "host unreachable",
                2 => "protocol unreachable",
                3 => "port unreachable",
                4 => "fragmentation needed",
                13 => "administratively prohibited",
                _ => "other",
            }
        ),
        5 => "Redirect".into(),
        8 => "Echo (ping) request".into(),
        11 => "Time-to-live exceeded".into(),
        _ => format!("Type {t}"),
    }
}

fn icmpv6_type(t: u8, _c: u8) -> String {
    match t {
        1 => "Destination unreachable",
        2 => "Packet too big",
        3 => "Time exceeded",
        128 => "Echo (ping) request",
        129 => "Echo (ping) reply",
        133 => "Router solicitation",
        134 => "Router advertisement",
        135 => "Neighbor solicitation",
        136 => "Neighbor advertisement",
        143 => "Multicast listener report",
        _ => return format!("Type {t}"),
    }
    .into()
}

fn tcp(d: &[u8], b: usize, o: &mut Out) {
    let p = &d[b.min(d.len())..];
    let (Some(sp), Some(dp), Some(seq), Some(ack), Some(off_flags), Some(win)) = (
        u16_at(p, 0),
        u16_at(p, 2),
        u32_at(p, 4),
        u32_at(p, 8),
        u16_at(p, 12),
        u16_at(p, 14),
    ) else {
        o.set("TCP", "Truncated TCP header".into());
        return;
    };
    let hdr = usize::from(off_flags >> 12) * 4;
    if hdr < 20 || p.len() < hdr {
        o.set("TCP", "Invalid TCP header length".into());
        return;
    }
    let flags = off_flags & 0x01FF;
    let payload = &p[hdr..];
    let names: Vec<&str> = [
        (0x002, "SYN"),
        (0x010, "ACK"),
        (0x001, "FIN"),
        (0x004, "RST"),
        (0x008, "PSH"),
        (0x020, "URG"),
    ]
    .iter()
    .filter(|(bit, _)| flags & bit != 0)
    .map(|(_, n)| *n)
    .collect();
    let mut l = Lb::new("tcp", "Transmission Control Protocol", b, hdr);
    l.named("tcp.srcport", "Source port", sp.into(), port_name(sp), 0, 2);
    l.named(
        "tcp.dstport",
        "Destination port",
        dp.into(),
        port_name(dp),
        2,
        2,
    );
    l.num("tcp.seq", "Sequence number", seq.into(), 4, 4);
    l.num("tcp.ack", "Acknowledgment number", ack.into(), 8, 4);
    l.num("tcp.hdr_len", "Header length", hdr as u64, 12, 1);
    l.push(
        "tcp.flags",
        "Flags",
        format!("0x{flags:03x} ({})", names.join(", ")),
        Some(flags.into()),
        12,
        2,
    );
    for (key, label, bit) in [
        ("tcp.flags.syn", "SYN", 0x002),
        ("tcp.flags.ack", "ACK", 0x010),
        ("tcp.flags.fin", "FIN", 0x001),
        ("tcp.flags.reset", "RST (reset)", 0x004),
        ("tcp.flags.push", "PSH", 0x008),
        ("tcp.flags.urg", "URG", 0x020),
    ] {
        l.num(key, label, u64::from(flags & bit != 0), 13, 1);
    }
    l.num("tcp.window_size_value", "Window", win.into(), 14, 2);
    l.text(
        "tcp.checksum",
        "Checksum",
        format!("0x{:04x}", u16_at(p, 16).unwrap_or(0)),
        16,
        2,
    );
    l.num(
        "tcp.urgent_pointer",
        "Urgent pointer",
        u16_at(p, 18).unwrap_or(0).into(),
        18,
        2,
    );
    l.num(
        "tcp.len",
        "Payload length",
        payload.len() as u64,
        hdr,
        payload.len(),
    );
    let mut info = format!("{sp} → {dp} [{}] Seq={seq}", names.join(", "));
    if flags & 0x010 != 0 {
        let _ = write!(info, " Ack={ack}");
    }
    let _ = write!(info, " Win={win} Len={}", payload.len());
    o.layers.push(l.done(format!(
        "Src port: {sp}, Dst port: {dp}, Len: {}",
        payload.len()
    )));
    o.set("TCP", info);
    if payload.is_empty() {
        return;
    }
    let pb = b + hdr;
    let on = |port: u16| sp == port || dp == port;
    if on(445) || on(139) {
        smb(d, pb, o);
    } else if on(53) {
        // DNS over TCP has a two-byte length prefix, which some stacks send in its own segment.
        if payload.len() > 2 {
            let prefixed = u16_at(payload, 0) == Some((payload.len() - 2) as u16);
            dns(
                d,
                pb + if prefixed { 2 } else { 0 },
                o,
                "dns",
                "Domain Name System",
                "DNS",
            );
        }
    } else if looks_like_http(payload) {
        http(d, pb, o);
    } else if is_tls_record(payload) {
        tls(d, pb, o);
    }
}

fn port_name(p: u16) -> &'static str {
    match p {
        20 | 21 => "FTP",
        22 => "SSH",
        23 => "Telnet",
        25 => "SMTP",
        53 => "DNS",
        67 | 68 => "DHCP",
        80 => "HTTP",
        110 => "POP3",
        123 => "NTP",
        137 => "NetBIOS name",
        138 => "NetBIOS datagram",
        139 => "NetBIOS session",
        143 => "IMAP",
        443 => "HTTPS",
        445 => "SMB",
        993 => "IMAPS",
        995 => "POP3S",
        1900 => "SSDP",
        3389 => "RDP",
        3478 => "STUN",
        5353 => "mDNS",
        5355 => "LLMNR",
        8080 => "HTTP alternate",
        _ => "",
    }
}

fn udp(d: &[u8], b: usize, o: &mut Out) {
    let p = &d[b.min(d.len())..];
    let (Some(sp), Some(dp), Some(len)) = (u16_at(p, 0), u16_at(p, 2), u16_at(p, 4)) else {
        o.set("UDP", "Truncated UDP header".into());
        return;
    };
    let payload = p.get(8..).unwrap_or(&[]);
    let mut l = Lb::new("udp", "User Datagram Protocol", b, 8);
    l.named("udp.srcport", "Source port", sp.into(), port_name(sp), 0, 2);
    l.named(
        "udp.dstport",
        "Destination port",
        dp.into(),
        port_name(dp),
        2,
        2,
    );
    l.num("udp.length", "Length", len.into(), 4, 2);
    l.text(
        "udp.checksum",
        "Checksum",
        format!("0x{:04x}", u16_at(p, 6).unwrap_or(0)),
        6,
        2,
    );
    o.layers
        .push(l.done(format!("Src port: {sp}, Dst port: {dp}")));
    o.set("UDP", format!("{sp} → {dp} Len={}", payload.len()));
    if payload.is_empty() {
        return;
    }
    let pb = b + 8;
    let on = |port: u16| sp == port || dp == port;
    if on(53) {
        dns(d, pb, o, "dns", "Domain Name System", "DNS");
    } else if on(5353) {
        dns(d, pb, o, "mdns", "Multicast Domain Name System", "MDNS");
    } else if on(5355) {
        dns(
            d,
            pb,
            o,
            "llmnr",
            "Link-local Multicast Name Resolution",
            "LLMNR",
        );
    } else if on(67) || on(68) {
        dhcp(d, pb, o);
    } else if on(443) {
        quic(d, pb, o);
    } else if on(123) {
        label(
            d,
            pb,
            o,
            "ntp",
            "Network Time Protocol",
            "NTP",
            "NTP".into(),
        );
    } else if on(1900) {
        let first = String::from_utf8_lossy(payload)
            .lines()
            .next()
            .unwrap_or_default()
            .to_owned();
        label(
            d,
            pb,
            o,
            "ssdp",
            "Simple Service Discovery Protocol",
            "SSDP",
            first,
        );
    } else if on(137) {
        label(
            d,
            pb,
            o,
            "nbns",
            "NetBIOS Name Service",
            "NBNS",
            "Name query or registration".into(),
        );
    } else if on(3478) {
        label(
            d,
            pb,
            o,
            "stun",
            "Session Traversal Utilities for NAT",
            "STUN",
            "STUN message".into(),
        );
    }
}

fn label(
    d: &[u8],
    b: usize,
    o: &mut Out,
    proto: &'static str,
    name: &str,
    short: &str,
    info: String,
) {
    o.layers
        .push(Lb::new(proto, name, b, d.len().saturating_sub(b)).done(info.clone()));
    o.set(short, info);
}

// ---------------------------------------------------------------------------
// DNS
// ---------------------------------------------------------------------------

fn dns_type(t: u16) -> String {
    match t {
        1 => "A",
        2 => "NS",
        5 => "CNAME",
        6 => "SOA",
        12 => "PTR",
        15 => "MX",
        16 => "TXT",
        28 => "AAAA",
        33 => "SRV",
        47 => "NSEC",
        64 => "SVCB",
        65 => "HTTPS",
        255 => "ANY",
        _ => return format!("TYPE{t}"),
    }
    .into()
}

fn rcode_name(r: u16) -> &'static str {
    match r {
        0 => "No error",
        1 => "Format error",
        2 => "Server failure",
        3 => "No such name",
        4 => "Not implemented",
        5 => "Refused",
        _ => "Error",
    }
}

/// Reads a (possibly compressed) name at `o` in the DNS message `m`; returns it
/// and the offset just after it in the original position.
fn dns_name(m: &[u8], mut o: usize) -> Option<(String, usize)> {
    let mut labels: Vec<String> = Vec::new();
    let mut end = None;
    for _ in 0..64 {
        let len = *m.get(o)? as usize;
        if len == 0 {
            end.get_or_insert(o + 1);
            let name = labels.join(".");
            return Some((
                if name.is_empty() {
                    "<Root>".into()
                } else {
                    name
                },
                end?,
            ));
        }
        if len & 0xC0 == 0xC0 {
            let ptr = ((len & 0x3F) << 8) | usize::from(*m.get(o + 1)?);
            end.get_or_insert(o + 2);
            if ptr >= m.len() {
                return None;
            }
            o = ptr;
            continue;
        }
        labels.push(String::from_utf8_lossy(m.get(o + 1..o + 1 + len)?).into_owned());
        o += 1 + len;
    }
    None // pointer loop
}

fn dns(d: &[u8], b: usize, o: &mut Out, proto: &'static str, name: &str, short: &str) {
    let m = &d[b.min(d.len())..];
    let (Some(id), Some(flags), Some(qd), Some(an), Some(ns), Some(ar)) = (
        u16_at(m, 0),
        u16_at(m, 2),
        u16_at(m, 4),
        u16_at(m, 6),
        u16_at(m, 8),
        u16_at(m, 10),
    ) else {
        return;
    };
    let response = flags & 0x8000 != 0;
    let rcode = flags & 0x000F;
    let mut l = Lb::new(proto, name, b, m.len());
    l.text("dns.id", "Transaction ID", format!("0x{id:04x}"), 0, 2);
    l.num("dns.flags.response", "Response", u64::from(response), 2, 2);
    l.num(
        "dns.flags.opcode",
        "Opcode",
        ((flags >> 11) & 0xF).into(),
        2,
        2,
    );
    l.named(
        "dns.flags.rcode",
        "Reply code",
        rcode.into(),
        if response { rcode_name(rcode) } else { "" },
        2,
        2,
    );
    l.num("dns.count.queries", "Questions", qd.into(), 4, 2);
    l.num("dns.count.answers", "Answers", an.into(), 6, 2);
    l.num("dns.count.auth_rr", "Authority records", ns.into(), 8, 2);
    l.num("dns.count.add_rr", "Additional records", ar.into(), 10, 2);
    let mut off = 12;
    let mut first_q: Option<(String, String)> = None;
    for _ in 0..qd.min(16) {
        let Some((qname, after)) = dns_name(m, off) else {
            break;
        };
        let (Some(qt), Some(_qc)) = (u16_at(m, after), u16_at(m, after + 2)) else {
            break;
        };
        l.text("dns.qry.name", "Query name", &qname, off, after - off);
        l.text("dns.qry.type", "Query type", dns_type(qt), after, 2);
        first_q.get_or_insert((dns_type(qt), qname));
        off = after + 4;
    }
    let mut answers: Vec<String> = Vec::new();
    for _ in 0..an.min(32) {
        let Some((rname, after)) = dns_name(m, off) else {
            break;
        };
        let (Some(rt), Some(ttl), Some(rdlen)) =
            (u16_at(m, after), u32_at(m, after + 4), u16_at(m, after + 8))
        else {
            break;
        };
        let rd = after + 10;
        let rdlen = usize::from(rdlen);
        if rd + rdlen > m.len() {
            break;
        }
        l.text("dns.resp.name", "Answer name", &rname, off, after - off);
        l.text("dns.resp.type", "Answer type", dns_type(rt), after, 2);
        l.num("dns.resp.ttl", "Time to live", ttl.into(), after + 4, 4);
        let value = match rt {
            1 if rdlen == 4 => ipv4_at(m, rd).map(|a| ("dns.a", "Address", a.to_string())),
            28 if rdlen == 16 => {
                ipv6_at(m, rd).map(|a| ("dns.aaaa", "IPv6 address", a.to_string()))
            }
            5 => dns_name(m, rd).map(|(n, _)| ("dns.cname", "Canonical name", n)),
            12 => dns_name(m, rd).map(|(n, _)| ("dns.ptr", "Domain name", n)),
            2 => dns_name(m, rd).map(|(n, _)| ("dns.ns", "Name server", n)),
            15 => dns_name(m, rd + 2).map(|(n, _)| ("dns.mx", "Mail exchange", n)),
            _ => None,
        };
        if let Some((key, label, v)) = value {
            l.text(key, label, &v, rd, rdlen);
            answers.push(format!("{} {v}", dns_type(rt)));
        }
        off = rd + rdlen;
    }
    let mut parts = vec![format!(
        "Standard query{} 0x{id:04x}",
        if response { " response" } else { "" }
    )];
    if let Some((qt, qn)) = first_q {
        parts.push(format!("{qt} {qn}"));
    }
    if response && rcode != 0 {
        parts.push(rcode_name(rcode).into());
    }
    parts.extend(answers);
    let info = parts.join(" ");
    let summary = format!("{} 0x{id:04x}", if response { "Response" } else { "Query" });
    o.layers.push(l.done(summary));
    o.set(short, info);
}

// ---------------------------------------------------------------------------
// HTTP, TLS, SMB2, QUIC, DHCP
// ---------------------------------------------------------------------------

const HTTP_METHODS: &[&str] = &[
    "GET ", "POST ", "PUT ", "DELETE ", "HEAD ", "OPTIONS ", "PATCH ", "CONNECT ", "TRACE ",
];

fn looks_like_http(p: &[u8]) -> bool {
    p.starts_with(b"HTTP/1.") || HTTP_METHODS.iter().any(|m| p.starts_with(m.as_bytes()))
}

fn http(d: &[u8], b: usize, o: &mut Out) {
    let p = &d[b..];
    let head_len = p
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .map_or(p.len(), |i| i + 4);
    let head = &p[..head_len];
    let mut l = Lb::new("http", "Hypertext Transfer Protocol", b, p.len());
    let mut off = 0;
    let mut info = String::new();
    for (i, raw) in head.split(|&c| c == b'\n').enumerate() {
        let line_len = raw.len() + 1;
        let line = String::from_utf8_lossy(raw)
            .trim_end_matches('\r')
            .to_owned();
        if line.is_empty() {
            break;
        }
        if i == 0 {
            let parts: Vec<&str> = line.splitn(3, ' ').collect();
            if line.starts_with("HTTP/") {
                l.text(
                    "http.response.version",
                    "Version",
                    parts[0],
                    off,
                    parts[0].len(),
                );
                if let Some(code) = parts.get(1).and_then(|c| c.parse::<u64>().ok()) {
                    l.num(
                        "http.response.code",
                        "Status code",
                        code,
                        off + parts[0].len() + 1,
                        3,
                    );
                }
                if let Some(phrase) = parts.get(2) {
                    l.text("http.response.phrase", "Reason phrase", *phrase, 0, 0);
                }
            } else {
                l.text(
                    "http.request.method",
                    "Method",
                    parts[0],
                    off,
                    parts[0].len(),
                );
                if let Some(uri) = parts.get(1) {
                    l.text(
                        "http.request.uri",
                        "URI",
                        *uri,
                        off + parts[0].len() + 1,
                        uri.len(),
                    );
                }
                if let Some(v) = parts.get(2) {
                    l.text("http.request.version", "Version", *v, 0, 0);
                }
            }
            info = line.clone();
        } else if let Some((name, value)) = line.split_once(':') {
            let value = value.trim();
            let key = match name.to_ascii_lowercase().as_str() {
                "host" => "http.host",
                "user-agent" => "http.user_agent",
                "content-type" => "http.content_type",
                "content-length" => "http.content_length",
                "server" => "http.server",
                "location" => "http.location",
                _ => "http.header",
            };
            match (key, value.parse::<u64>()) {
                ("http.content_length", Ok(n)) => l.num(key, name, n, off, raw.len()),
                _ => l.text(key, name, value, off, raw.len()),
            }
        }
        off += line_len;
    }
    if head_len < p.len() {
        l.text(
            "",
            "Body",
            format!("{} bytes", p.len() - head_len),
            head_len,
            p.len() - head_len,
        );
    }
    o.layers.push(l.done(info.clone()));
    o.set("HTTP", info);
}

fn is_tls_record(p: &[u8]) -> bool {
    p.len() >= 5 && (20..=23).contains(&p[0]) && p[1] == 3 && p[2] <= 4
}

fn tls_version(v: u16) -> &'static str {
    match v {
        0x0300 => "SSL 3.0",
        0x0301 => "TLS 1.0",
        0x0302 => "TLS 1.1",
        0x0303 => "TLS 1.2",
        0x0304 => "TLS 1.3",
        _ => "",
    }
}

fn handshake_name(t: u8) -> &'static str {
    match t {
        1 => "Client Hello",
        2 => "Server Hello",
        4 => "New Session Ticket",
        8 => "Encrypted Extensions",
        11 => "Certificate",
        12 => "Server Key Exchange",
        13 => "Certificate Request",
        14 => "Server Hello Done",
        15 => "Certificate Verify",
        16 => "Client Key Exchange",
        20 => "Finished",
        _ => "Handshake",
    }
}

/// Server name from a ClientHello body (starting at the client version).
fn sni(h: &[u8]) -> Option<(String, usize)> {
    let mut o = 2 + 32;
    o += 1 + usize::from(*h.get(o)?);
    o += 2 + usize::from(u16_at(h, o)?);
    o += 1 + usize::from(*h.get(o)?);
    let ext_end = o + 2 + usize::from(u16_at(h, o)?);
    o += 2;
    while o + 4 <= ext_end.min(h.len()) {
        let (ty, len) = (u16_at(h, o)?, usize::from(u16_at(h, o + 2)?));
        if ty == 0 {
            let name_len = usize::from(u16_at(h, o + 7)?);
            let name = h.get(o + 9..o + 9 + name_len)?;
            return Some((String::from_utf8_lossy(name).into_owned(), o + 9));
        }
        o += 4 + len;
    }
    None
}

fn tls(d: &[u8], b: usize, o: &mut Out) {
    let p = &d[b..];
    let mut l = Lb::new("tls", "Transport Layer Security", b, p.len());
    let mut parts: Vec<String> = Vec::new();
    let mut off = 0;
    while is_tls_record(&p[off..]) {
        let ct = p[off];
        let ver = u16_at(p, off + 1).unwrap_or(0);
        let len = usize::from(u16_at(p, off + 3).unwrap_or(0));
        let ct_name = match ct {
            20 => "Change Cipher Spec",
            21 => "Alert",
            22 => "Handshake",
            _ => "Application Data",
        };
        l.named(
            "tls.record.content_type",
            "Content type",
            ct.into(),
            ct_name,
            off,
            1,
        );
        l.named(
            "tls.record.version",
            "Version",
            ver.into(),
            tls_version(ver),
            off + 1,
            2,
        );
        l.num("tls.record.length", "Length", len as u64, off + 3, 2);
        let body = off + 5;
        match ct {
            22 => {
                if let Some(ht) = u8_at(p, body) {
                    l.named(
                        "tls.handshake.type",
                        "Handshake type",
                        ht.into(),
                        handshake_name(ht),
                        body,
                        1,
                    );
                    let mut part = handshake_name(ht).to_owned();
                    if ht == 1 {
                        if let Some((name, at)) = p.get(body + 4..).and_then(sni) {
                            l.text(
                                "tls.handshake.extensions_server_name",
                                "Server name",
                                &name,
                                body + 4 + at,
                                name.len(),
                            );
                            let _ = write!(part, " (SNI={name})");
                        }
                    }
                    parts.push(part);
                }
            }
            21 => {
                if let (Some(level), Some(desc)) = (u8_at(p, body), u8_at(p, body + 1)) {
                    l.num(
                        "tls.alert_message.level",
                        "Alert level",
                        level.into(),
                        body,
                        1,
                    );
                    l.num(
                        "tls.alert_message.desc",
                        "Alert description",
                        desc.into(),
                        body + 1,
                        1,
                    );
                }
                parts.push("Alert".into());
            }
            _ => parts.push(ct_name.into()),
        }
        off = body + len;
        if off >= p.len() {
            break;
        }
    }
    parts.dedup();
    let info = parts.join(", ");
    o.layers.push(l.done(info.clone()));
    o.set("TLS", info);
}

fn smb_command(c: u16) -> &'static str {
    match c {
        0 => "Negotiate",
        1 => "Session Setup",
        2 => "Logoff",
        3 => "Tree Connect",
        4 => "Tree Disconnect",
        5 => "Create",
        6 => "Close",
        7 => "Flush",
        8 => "Read",
        9 => "Write",
        10 => "Lock",
        11 => "Ioctl",
        12 => "Cancel",
        13 => "Echo",
        14 => "Query Directory",
        15 => "Change Notify",
        16 => "Query Info",
        17 => "Set Info",
        18 => "Oplock Break",
        _ => "Unknown command",
    }
}

fn nt_status(s: u32) -> &'static str {
    match s {
        0 => "STATUS_SUCCESS",
        0x0000_0103 => "STATUS_PENDING",
        0x8000_0005 => "STATUS_BUFFER_OVERFLOW",
        0xC000_0016 => "STATUS_MORE_PROCESSING_REQUIRED",
        0xC000_0022 => "STATUS_ACCESS_DENIED",
        0xC000_0034 => "STATUS_OBJECT_NAME_NOT_FOUND",
        0xC000_003A => "STATUS_OBJECT_PATH_NOT_FOUND",
        0xC000_006D => "STATUS_LOGON_FAILURE",
        0xC000_00CC => "STATUS_BAD_NETWORK_NAME",
        0x8000_0006 => "STATUS_NO_MORE_FILES",
        _ => "",
    }
}

fn smb(d: &[u8], b: usize, o: &mut Out) {
    // NetBIOS session service header (4 bytes), then the SMB header.
    let h = b + 4;
    let p = &d[h.min(d.len())..];
    if p.starts_with(&[0xFF, b'S', b'M', b'B']) {
        label(
            d,
            h,
            o,
            "smb",
            "SMB (version 1)",
            "SMB",
            "SMB1 message".into(),
        );
        return;
    }
    if !p.starts_with(&[0xFE, b'S', b'M', b'B']) {
        return;
    }
    let (Some(status), Some(cmd), Some(flags)) = (
        u32_at(p, 8).map(u32::swap_bytes),
        u16_at(p, 12).map(u16::swap_bytes),
        u32_at(p, 16).map(u32::swap_bytes),
    ) else {
        return;
    };
    let response = flags & 1 != 0;
    let mut l = Lb::new(
        "smb2",
        "SMB2 (Server Message Block Protocol version 2)",
        h,
        p.len(),
    );
    l.named(
        "smb2.nt_status",
        "NT status",
        status.into(),
        nt_status(status),
        8,
        4,
    );
    l.named("smb2.cmd", "Command", cmd.into(), smb_command(cmd), 12, 2);
    l.num(
        "smb2.flags.response",
        "Response",
        u64::from(response),
        16,
        4,
    );
    if let Some(id) = p.get(24..32) {
        l.num(
            "smb2.msg_id",
            "Message ID",
            u64::from_le_bytes(id.try_into().unwrap_or_default()),
            24,
            8,
        );
    }
    let mut info = format!(
        "{} {}",
        smb_command(cmd),
        if response { "Response" } else { "Request" }
    );
    let ok = matches!(status, 0 | 0x103 | 0xC000_0016);
    if response && !ok {
        let name = nt_status(status);
        let _ = write!(
            info,
            ", Error: {}",
            if name.is_empty() {
                format!("0x{status:08x}")
            } else {
                name.into()
            }
        );
    }
    o.layers.push(l.done(info.clone()));
    o.set("SMB2", info);
}

fn quic(d: &[u8], b: usize, o: &mut Out) {
    let p = &d[b..];
    let Some(first) = p.first().copied() else {
        return;
    };
    let mut l = Lb::new("quic", "QUIC", b, p.len());
    let info = if first & 0x80 != 0 {
        let ty = (first >> 4) & 3;
        let name = ["Initial", "0-RTT", "Handshake", "Retry"][usize::from(ty)];
        l.named(
            "quic.long.packet_type",
            "Packet type",
            ty.into(),
            name,
            0,
            1,
        );
        if let Some(v) = u32_at(p, 1) {
            l.text("quic.version", "Version", format!("0x{v:08x}"), 1, 4);
        }
        name.to_owned()
    } else {
        "Protected payload".to_owned()
    };
    o.layers.push(l.done(info.clone()));
    o.set("QUIC", info);
}

fn dhcp(d: &[u8], b: usize, o: &mut Out) {
    let p = &d[b..];
    if p.len() < 240 || u32_at(p, 236) != Some(0x6382_5363) {
        return;
    }
    let mut l = Lb::new("dhcp", "Dynamic Host Configuration Protocol", b, p.len());
    l.num("dhcp.type", "Message op", p[0].into(), 0, 1);
    if let Some(mac) = mac_at(p, 28) {
        l.text("dhcp.hw.mac_addr", "Client MAC", mac, 28, 6);
    }
    if let Some(ip) = ipv4_at(p, 16) {
        l.text("dhcp.ip.your", "Your IP", ip.to_string(), 16, 4);
    }
    let mut info = "DHCP".to_owned();
    let mut off = 240;
    while let Some(code) = u8_at(p, off) {
        if code == 255 {
            break;
        }
        if code == 0 {
            off += 1;
            continue;
        }
        let Some(len) = u8_at(p, off + 1) else { break };
        if code == 53 {
            if let Some(t) = u8_at(p, off + 2) {
                let name = match t {
                    1 => "Discover",
                    2 => "Offer",
                    3 => "Request",
                    4 => "Decline",
                    5 => "ACK",
                    6 => "NAK",
                    7 => "Release",
                    8 => "Inform",
                    _ => "",
                };
                l.named(
                    "dhcp.option.dhcp",
                    "DHCP message type",
                    t.into(),
                    name,
                    off + 2,
                    1,
                );
                info = format!("DHCP {name}");
            }
        }
        off += 2 + usize::from(len);
    }
    o.layers.push(l.done(info.clone()));
    o.set("DHCP", info);
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub fn eth(ty: u16, payload: &[u8]) -> Vec<u8> {
        let mut v = vec![
            0x02, 0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0x02, 0x1a, 0x2b, 0x3c, 0x4d, 0x5e,
        ];
        v.extend_from_slice(&ty.to_be_bytes());
        v.extend_from_slice(payload);
        v
    }

    pub fn ipv4(proto: u8, src: [u8; 4], dst: [u8; 4], payload: &[u8]) -> Vec<u8> {
        let mut v = vec![0x45, 0, 0, 0, 0x12, 0x34, 0x40, 0, 128, proto, 0, 0];
        let total = (20 + payload.len()) as u16;
        v[2..4].copy_from_slice(&total.to_be_bytes());
        v.extend_from_slice(&src);
        v.extend_from_slice(&dst);
        v.extend_from_slice(payload);
        v
    }

    pub fn tcp(sp: u16, dp: u16, flags: u8, payload: &[u8]) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&sp.to_be_bytes());
        v.extend_from_slice(&dp.to_be_bytes());
        v.extend_from_slice(&1000u32.to_be_bytes());
        v.extend_from_slice(&2000u32.to_be_bytes());
        v.extend_from_slice(&[0x50, flags, 0x01, 0x00, 0, 0, 0, 0]);
        v.extend_from_slice(payload);
        v
    }

    pub fn udp(sp: u16, dp: u16, payload: &[u8]) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&sp.to_be_bytes());
        v.extend_from_slice(&dp.to_be_bytes());
        v.extend_from_slice(&((8 + payload.len()) as u16).to_be_bytes());
        v.extend_from_slice(&[0, 0]);
        v.extend_from_slice(payload);
        v
    }

    fn frame(bytes: &[u8]) -> Dissection {
        dissect(bytes, LinkType::Ethernet, bytes.len())
    }

    const ME: [u8; 4] = [192, 168, 1, 169];
    const SRV: [u8; 4] = [160, 79, 104, 10];

    /// The first frame the capture test recorded on this PC (TCP ACK to port 443).
    #[test]
    fn real_captured_ack() {
        let hex = "02aabbccddee021a2b3c4d5e080045000028a974400080060000c0a801a9a04f680ae0d401bb9e8d1c2a0000000050104000ee7d0000";
        let bytes: Vec<u8> = (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).expect("test value"))
            .collect();
        let d = frame(&bytes);
        assert_eq!(
            (d.src.as_str(), d.dst.as_str(), d.protocol.as_str()),
            ("192.168.1.169", "160.79.104.10", "TCP")
        );
        assert!(d.info.starts_with("57556 → 443 [ACK]"), "{}", d.info);
        assert_eq!(d.values("tcp.dstport"), vec![FieldValue::Num(443)]);
        assert_eq!(d.values("ip.addr").len(), 2);
    }

    #[test]
    fn dns_query_and_error_response() {
        let mut q = vec![0x1a, 0x2b, 0x01, 0x00, 0, 1, 0, 0, 0, 0, 0, 0];
        q.extend_from_slice(b"\x07example\x03com\x00\x00\x01\x00\x01");
        let d = frame(&eth(
            0x0800,
            &ipv4(17, ME, [1, 1, 1, 1], &udp(55000, 53, &q)),
        ));
        assert_eq!(d.protocol, "DNS");
        assert_eq!(d.info, "Standard query 0x1a2b A example.com");
        assert_eq!(
            d.values("dns.qry.name"),
            vec![FieldValue::Text("example.com")]
        );

        let mut r = q.clone();
        r[2] = 0x81;
        r[3] = 0x83; // response, NXDOMAIN
        let d = frame(&eth(
            0x0800,
            &ipv4(17, [1, 1, 1, 1], ME, &udp(53, 55000, &r)),
        ));
        assert_eq!(
            d.info,
            "Standard query response 0x1a2b A example.com No such name"
        );
        assert_eq!(d.values("dns.flags.rcode"), vec![FieldValue::Num(3)]);
    }

    #[test]
    fn dns_answer_with_compression() {
        let mut r = vec![0, 7, 0x81, 0x80, 0, 1, 0, 1, 0, 0, 0, 0];
        r.extend_from_slice(b"\x03one\x03one\x03one\x03one\x00\x00\x01\x00\x01");
        r.extend_from_slice(&[0xC0, 12, 0, 1, 0, 1, 0, 0, 0, 60, 0, 4, 1, 1, 1, 1]);
        let d = frame(&eth(
            0x0800,
            &ipv4(17, [1, 1, 1, 1], ME, &udp(53, 5000, &r)),
        ));
        assert_eq!(d.values("dns.a"), vec![FieldValue::Text("1.1.1.1")]);
        assert!(
            d.info.ends_with("A one.one.one.one A 1.1.1.1"),
            "{}",
            d.info
        );
        // A self-referencing pointer must not hang.
        assert_eq!(dns_name(&[0xC0, 0], 0), None);
    }

    #[test]
    fn http_request_and_response() {
        let req = b"GET /index.html HTTP/1.1\r\nHost: example.com\r\nUser-Agent: test\r\n\r\n";
        let d = frame(&eth(0x0800, &ipv4(6, ME, SRV, &tcp(50000, 80, 0x18, req))));
        assert_eq!(d.protocol, "HTTP");
        assert_eq!(d.info, "GET /index.html HTTP/1.1");
        assert_eq!(d.values("http.host"), vec![FieldValue::Text("example.com")]);
        let res = b"HTTP/1.1 404 Not Found\r\nContent-Length: 9\r\n\r\nnot found";
        let d = frame(&eth(0x0800, &ipv4(6, SRV, ME, &tcp(80, 50000, 0x18, res))));
        assert_eq!(d.values("http.response.code"), vec![FieldValue::Num(404)]);
        assert_eq!(d.values("http.content_length"), vec![FieldValue::Num(9)]);
    }

    #[test]
    fn tls_client_hello_sni() {
        let name = b"claude.ai";
        let mut sni_ext = vec![0, 0];
        let list_len = (3 + name.len()) as u16;
        sni_ext.extend_from_slice(&(list_len + 2).to_be_bytes());
        sni_ext.extend_from_slice(&list_len.to_be_bytes());
        sni_ext.push(0);
        sni_ext.extend_from_slice(&(name.len() as u16).to_be_bytes());
        sni_ext.extend_from_slice(name);
        let mut hello = vec![3, 3];
        hello.extend_from_slice(&[0u8; 32]);
        hello.extend_from_slice(&[0, 0, 2, 0x13, 0x01, 1, 0]);
        hello.extend_from_slice(&(sni_ext.len() as u16).to_be_bytes());
        hello.extend_from_slice(&sni_ext);
        let mut hs = vec![1, 0];
        hs.extend_from_slice(&(hello.len() as u16).to_be_bytes());
        hs.extend_from_slice(&hello);
        let mut rec = vec![22, 3, 1];
        rec.extend_from_slice(&(hs.len() as u16).to_be_bytes());
        rec.extend_from_slice(&hs);
        let d = frame(&eth(
            0x0800,
            &ipv4(6, ME, SRV, &tcp(50000, 443, 0x18, &rec)),
        ));
        assert_eq!(d.protocol, "TLS");
        assert_eq!(d.info, "Client Hello (SNI=claude.ai)");
        assert_eq!(
            d.values("tls.handshake.extensions_server_name"),
            vec![FieldValue::Text("claude.ai")]
        );
    }

    #[test]
    fn arp_icmp_rst_and_ipv6() {
        let mut a = vec![
            0, 1, 8, 0, 6, 4, 0, 1, 0x02, 0x1a, 0x2b, 0x3c, 0x4d, 0x5e, 192, 168, 1, 169,
        ];
        a.extend_from_slice(&[0, 0, 0, 0, 0, 0, 192, 168, 1, 1]);
        assert_eq!(
            frame(&eth(0x0806, &a)).info,
            "Who has 192.168.1.1? Tell 192.168.1.169"
        );

        let ping = [8, 0, 0, 0, 0, 1, 0, 7];
        let d = frame(&eth(0x0800, &ipv4(1, ME, [8, 8, 8, 8], &ping)));
        assert_eq!(d.info, "Echo (ping) request id=0x0001, seq=7");

        let d = frame(&eth(0x0800, &ipv4(6, SRV, ME, &tcp(443, 50000, 0x14, &[]))));
        assert_eq!(d.values("tcp.flags.reset"), vec![FieldValue::Num(1)]);
        assert!(d.info.contains("[ACK, RST]"), "{}", d.info);

        let mut v6 = vec![0x60, 0, 0, 0, 0, 8, 17, 64];
        v6.extend_from_slice(&"fe80::1".parse::<Ipv6Addr>().expect("test value").octets());
        v6.extend_from_slice(&"ff02::fb".parse::<Ipv6Addr>().expect("test value").octets());
        v6.extend_from_slice(&udp(5353, 5353, &[])[..8]);
        let d = frame(&eth(0x86DD, &v6));
        assert_eq!((d.src.as_str(), d.dst.as_str()), ("fe80::1", "ff02::fb"));
        assert_eq!(d.values("ip.src"), vec![FieldValue::Text("fe80::1")]);
    }

    #[test]
    fn issues_seen_on_real_traffic() {
        // DNS over TCP whose length prefix came in an earlier segment.
        let mut q = vec![0x12, 0x34, 0x01, 0x00, 0, 1, 0, 0, 0, 0, 0, 0];
        q.extend_from_slice(b"\x01a\x00\x00\x01\x00\x01");
        let d = frame(&eth(
            0x0800,
            &ipv4(6, ME, [1, 1, 1, 1], &tcp(50000, 53, 0x18, &q)),
        ));
        assert_eq!(d.info, "Standard query 0x1234 A a");
        let mut prefixed = (q.len() as u16).to_be_bytes().to_vec();
        prefixed.extend_from_slice(&q);
        let d = frame(&eth(
            0x0800,
            &ipv4(6, ME, [1, 1, 1, 1], &tcp(50000, 53, 0x18, &prefixed)),
        ));
        assert_eq!(d.info, "Standard query 0x1234 A a");

        // mDNS response without a question section.
        let mut r = vec![0, 0, 0x84, 0, 0, 0, 0, 1, 0, 0, 0, 0];
        r.extend_from_slice(
            b"\x01b\x05local\x00\x00\x01\x00\x01\x00\x00\x00\x78\x00\x04\xc0\xa8\x01\x02",
        );
        let d = frame(&eth(
            0x0800,
            &ipv4(17, ME, [224, 0, 0, 251], &udp(5353, 5353, &r)),
        ));
        assert_eq!(d.info, "Standard query response 0x0000 A 192.168.1.2");

        // 802.3 frame with an LLC header (spanning tree).
        let d = frame(&eth(0x0026, &[0x42, 0x42, 0x03, 0, 0]));
        assert_eq!(
            (d.protocol.as_str(), d.info.as_str()),
            ("LLC", "Spanning Tree Protocol")
        );
    }

    #[test]
    fn wifi_data_frame_is_decoded() {
        // QoS data frame from the access point (From DS) carrying IPv4/UDP DNS.
        let mut f = vec![0x88, 0x02, 0, 0];
        f.extend_from_slice(&[0x11; 6]); // addr1: receiver = destination
        f.extend_from_slice(&[0x22; 6]); // addr2: BSSID
        f.extend_from_slice(&[0x33; 6]); // addr3: source
        f.extend_from_slice(&[0, 0, 0, 0]); // sequence + QoS control
        f.extend_from_slice(&[0xAA, 0xAA, 0x03, 0, 0, 0, 0x08, 0x00]);
        let mut q = vec![0, 9, 0x01, 0x00, 0, 1, 0, 0, 0, 0, 0, 0];
        q.extend_from_slice(b"\x01x\x00\x00\x01\x00\x01");
        f.extend_from_slice(&ipv4(17, [1, 1, 1, 1], ME, &udp(53, 5000, &q)));
        let d = dissect(&f, LinkType::Ieee80211, f.len());
        assert_eq!((d.protocol.as_str(), d.src.as_str()), ("DNS", "1.1.1.1"));
        assert_eq!(
            d.values("wlan.sa"),
            vec![FieldValue::Text("33:33:33:33:33:33")]
        );
        let beacon = dissect(&[0x80, 0, 0, 0], LinkType::Ieee80211, 4);
        assert_eq!(beacon.info, "Management frame, subtype 8");
    }

    /// Thousands of malformed frames: decoding and filtering must never panic.
    #[test]
    fn random_frames_never_panic() {
        // VEYL_FUZZ_SEED / VEYL_FUZZ_N allow longer one-off runs.
        let env = |k: &str| std::env::var(k).ok().and_then(|v| v.parse::<u64>().ok());
        let mut seed = env("VEYL_FUZZ_SEED").unwrap_or(0x9E37_79B9_7F4A_7C15) | 1;
        let rounds = env("VEYL_FUZZ_N").unwrap_or(30_000);
        let mut rnd = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        let filter = crate::packet_filter::parse("(dns.qry.name contains a or tcp.port == 443 or ip.addr == 10.0.0.0/8) and not frame.len > 100000 or http or tls.handshake.extensions_server_name or smb2.cmd == 5 or wlan.sa")
            .expect("valid filter")
            .expect("non-empty");
        let ports = [
            53u16, 80, 443, 445, 67, 5353, 5355, 123, 1900, 137, 3478, 8080, 1,
        ];
        for i in 0..rounds {
            let r = rnd();
            let mut payload: Vec<u8> = (0..(r % 300)).map(|_| rnd() as u8).collect();
            // Seed some payloads with plausible protocol starts so deeper parsers run.
            let prefix: &[u8] = match r % 7 {
                0 => b"GET / HTTP/1.1\r\nHost: x\r\n",
                1 => &[22, 3, 1, 0, 60, 1, 0, 0, 56, 3, 3],
                2 => &[0, 0, 0, 80, 0xFE, b'S', b'M', b'B'],
                _ => &[],
            };
            payload.splice(0..0, prefix.iter().copied());
            let (sp, dp) = (
                ports[(r >> 8) as usize % ports.len()],
                ports[(r >> 16) as usize % ports.len()],
            );
            let l4 = match (r >> 24) % 4 {
                0 => tcp(sp, dp, (r >> 32) as u8, &payload),
                1 => udp(sp, dp, &payload),
                _ => payload.clone(),
            };
            let proto = [6u8, 17, 1, 58, 2][(r >> 40) as usize % 5];
            let frame_bytes = match (r >> 44) % 4 {
                0 => eth(
                    0x86DD,
                    &[&[0x60, 0, 0, 0, 0, 20, proto, 64][..], &[0u8; 32], &l4].concat(),
                ),
                1 => eth(0x0806, &payload),
                _ => eth(0x0800, &ipv4(proto, [10, 0, 0, 1], [10, 0, 0, 2], &l4)),
            };
            let cut = (rnd() as usize) % (frame_bytes.len() + 1);
            for (bytes, link) in [
                (&frame_bytes[..cut], LinkType::Ethernet),
                (&frame_bytes[..], LinkType::Ethernet),
                (&payload[..], LinkType::RawIp),
                (&payload[..], LinkType::Ieee80211),
            ] {
                let d = dissect(bytes, link, bytes.len());
                let _ = crate::packet_filter::matches(&filter, &d);
            }
            if i % 1000 == 0 {
                // Fully random bytes as well.
                let junk: Vec<u8> = (0..(rnd() % 200)).map(|_| rnd() as u8).collect();
                let _ = dissect(&junk, LinkType::Ethernet, junk.len());
            }
        }
    }

    #[test]
    fn truncated_frames_never_panic() {
        let full = eth(
            0x0800,
            &ipv4(6, ME, SRV, &tcp(1, 443, 0x18, b"GET / HTTP/1.1\r\n\r\n")),
        );
        for n in 0..full.len() {
            let _ = frame(&full[..n]);
        }
        let _ = dissect(&[0x45], LinkType::RawIp, 1);
    }

    #[test]
    fn every_produced_key_is_known() {
        let req = b"GET / HTTP/1.1\r\nHost: a\r\nX-Other: b\r\n\r\n";
        for f in [
            eth(0x0800, &ipv4(6, ME, SRV, &tcp(1, 80, 0x18, req))),
            eth(0x0800, &ipv4(1, ME, SRV, &[8, 0, 0, 0, 0, 1, 0, 1])),
        ] {
            for l in frame(&f).layers {
                assert!(PROTOCOLS.contains(&l.proto), "{}", l.proto);
                for fld in l.fields {
                    assert!(
                        fld.key.is_empty() || KNOWN_FIELDS.contains(&fld.key),
                        "{}",
                        fld.key
                    );
                }
            }
        }
    }
}
