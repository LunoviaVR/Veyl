//! TCP and UDP endpoints with their owning process (`GetExtendedTcpTable` /
//! `GetExtendedUdpTable`). Per-connection byte counts are not exposed by these
//! APIs without elevation, so none are shown.

use std::net::{Ipv4Addr, Ipv6Addr};

use serde::Serialize;
use windows::Win32::NetworkManagement::IpHelper::{
    GetExtendedTcpTable, GetExtendedUdpTable, MIB_TCP6ROW_OWNER_PID, MIB_TCPROW_OWNER_PID,
    MIB_UDP6ROW_OWNER_PID, MIB_UDPROW_OWNER_PID, TCP_TABLE_OWNER_PID_ALL, UDP_TABLE_OWNER_PID,
};
use windows::Win32::Networking::WinSock::{AF_INET, AF_INET6};

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Connection {
    pub protocol: &'static str,
    pub local_address: String,
    pub local_port: u16,
    pub remote_address: Option<String>,
    pub remote_port: Option<u16>,
    pub state: Option<&'static str>,
    pub pid: u32,
}

/// tcpmib.h MIB_TCP_STATE values.
pub fn tcp_state(s: u32) -> &'static str {
    match s {
        1 => "Closed",
        2 => "Listen",
        3 => "SYN sent",
        4 => "SYN received",
        5 => "Established",
        6 => "FIN wait 1",
        7 => "FIN wait 2",
        8 => "Close wait",
        9 => "Closing",
        10 => "Last ACK",
        11 => "Time wait",
        12 => "Delete TCB",
        _ => "Unknown",
    }
}

/// Ports are stored in network byte order in the low 16 bits.
pub fn port(v: u32) -> u16 {
    u16::from_be((v & 0xFFFF) as u16)
}

/// IPv4 addresses are stored in network byte order.
fn v4(addr: u32) -> String {
    Ipv4Addr::from(u32::from_be(addr)).to_string()
}

/// Calls a GetExtended*Table function with the size-query pattern; returns the raw table.
fn table(call: impl Fn(Option<*mut core::ffi::c_void>, *mut u32) -> u32) -> Option<Vec<u64>> {
    let mut size = 0u32;
    for _ in 0..4 {
        let mut buf = vec![0u64; (size as usize).div_ceil(8).max(1)];
        let rc = call(Some(buf.as_mut_ptr().cast()), &mut size);
        match rc {
            0 => return Some(buf),
            122 => continue, // ERROR_INSUFFICIENT_BUFFER: size updated
            _ => return None,
        }
    }
    None
}

/// Rows following the leading `dwNumEntries` (padded to the row alignment).
///
/// # Safety
/// `buf` must hold a table returned by the API for row type `T`.
unsafe fn rows<T: Copy>(buf: &[u64]) -> Vec<T> {
    let count = buf
        .first()
        .map(|v| (*v & 0xFFFF_FFFF) as usize)
        .unwrap_or(0);
    let base = buf.as_ptr().cast::<u8>();
    let first = std::mem::align_of::<T>().max(4);
    let max = (buf.len() * 8).saturating_sub(first) / std::mem::size_of::<T>();
    // SAFETY: caller guarantees layout; count bounded by the buffer size.
    (0..count.min(max))
        .map(|i| unsafe {
            std::ptr::read_unaligned(base.add(first + i * std::mem::size_of::<T>()).cast::<T>())
        })
        .collect()
}

pub fn list() -> Vec<Connection> {
    let mut out = Vec::new();
    // SAFETY (all four calls): buffer and size pointers valid for the call.
    if let Some(b) = table(|p, s| unsafe {
        GetExtendedTcpTable(p, s, true, u32::from(AF_INET.0), TCP_TABLE_OWNER_PID_ALL, 0)
    }) {
        // SAFETY: table of MIB_TCPROW_OWNER_PID.
        for r in unsafe { rows::<MIB_TCPROW_OWNER_PID>(&b) } {
            let listen = r.dwState == 2;
            out.push(Connection {
                protocol: "TCP",
                local_address: v4(r.dwLocalAddr),
                local_port: port(r.dwLocalPort),
                remote_address: (!listen).then(|| v4(r.dwRemoteAddr)),
                remote_port: (!listen).then(|| port(r.dwRemotePort)),
                state: Some(tcp_state(r.dwState)),
                pid: r.dwOwningPid,
            });
        }
    }
    if let Some(b) = table(|p, s| unsafe {
        GetExtendedTcpTable(
            p,
            s,
            true,
            u32::from(AF_INET6.0),
            TCP_TABLE_OWNER_PID_ALL,
            0,
        )
    }) {
        // SAFETY: table of MIB_TCP6ROW_OWNER_PID.
        for r in unsafe { rows::<MIB_TCP6ROW_OWNER_PID>(&b) } {
            let listen = r.dwState == 2;
            out.push(Connection {
                protocol: "TCPv6",
                local_address: Ipv6Addr::from(r.ucLocalAddr).to_string(),
                local_port: port(r.dwLocalPort),
                remote_address: (!listen).then(|| Ipv6Addr::from(r.ucRemoteAddr).to_string()),
                remote_port: (!listen).then(|| port(r.dwRemotePort)),
                state: Some(tcp_state(r.dwState)),
                pid: r.dwOwningPid,
            });
        }
    }
    if let Some(b) = table(|p, s| unsafe {
        GetExtendedUdpTable(p, s, true, u32::from(AF_INET.0), UDP_TABLE_OWNER_PID, 0)
    }) {
        // SAFETY: table of MIB_UDPROW_OWNER_PID.
        for r in unsafe { rows::<MIB_UDPROW_OWNER_PID>(&b) } {
            out.push(Connection {
                protocol: "UDP",
                local_address: v4(r.dwLocalAddr),
                local_port: port(r.dwLocalPort),
                remote_address: None,
                remote_port: None,
                state: None,
                pid: r.dwOwningPid,
            });
        }
    }
    if let Some(b) = table(|p, s| unsafe {
        GetExtendedUdpTable(p, s, true, u32::from(AF_INET6.0), UDP_TABLE_OWNER_PID, 0)
    }) {
        // SAFETY: table of MIB_UDP6ROW_OWNER_PID.
        for r in unsafe { rows::<MIB_UDP6ROW_OWNER_PID>(&b) } {
            out.push(Connection {
                protocol: "UDPv6",
                local_address: Ipv6Addr::from(r.ucLocalAddr).to_string(),
                local_port: port(r.dwLocalPort),
                remote_address: None,
                remote_port: None,
                state: None,
                pid: r.dwOwningPid,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_order() {
        assert_eq!(port(u32::from(443u16.to_be())), 443);
        assert_eq!(v4(u32::from_be_bytes([127, 0, 0, 1]).to_be()), "127.0.0.1");
        assert_eq!(tcp_state(5), "Established");
    }

    #[test]
    fn lists_a_listener_we_open() {
        let l = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let p = l.local_addr().expect("addr").port();
        let conns = list();
        assert!(conns.iter().any(|c| c.protocol == "TCP"
            && c.local_port == p
            && c.pid == std::process::id()
            && c.state == Some("Listen")));
    }
}
