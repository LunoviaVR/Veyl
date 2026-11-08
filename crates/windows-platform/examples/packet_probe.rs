//! Captures for a few seconds, then decodes every frame and runs sample filters.
//! Needs an elevated prompt: `cargo run -p veyl-windows --example packet_probe`.
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use veyl_core::packet::{dissect, LinkType};
use veyl_core::packet_filter::{matches, parse};
use veyl_windows::packet_capture::{PacketCapture, RawPacket};

fn main() {
    let seen: Arc<Mutex<Vec<RawPacket>>> = Arc::default();
    let s = seen.clone();
    let cap = match PacketCapture::start(Box::new(move |p| s.lock().expect("test value").push(p))) {
        Ok(c) => c,
        Err(e) => return println!("start failed: {e}"),
    };
    std::thread::sleep(std::time::Duration::from_secs(8));
    cap.stop();
    let v = seen.lock().expect("test value");
    let t0 = std::time::Instant::now();
    let ds: Vec<_> = v
        .iter()
        .map(|p| dissect(&p.data, LinkType::Ethernet, p.orig_len as usize))
        .collect();
    println!("packets: {}, dissected in {:?}", v.len(), t0.elapsed());
    let mut protos: BTreeMap<&str, usize> = BTreeMap::new();
    for d in &ds {
        *protos.entry(d.protocol.as_str()).or_default() += 1;
    }
    println!("protocols: {protos:?}");
    for f in [
        "dns",
        "tls",
        "tcp.port == 443",
        "udp and not dns",
        "tcp.flags.reset",
        "dns.flags.rcode > 0",
        "ip.addr == 192.168.0.0/16",
        "arp",
        "quic",
    ] {
        let e = parse(f).expect("test value").expect("test value");
        println!(
            "filter {f:?}: {}",
            ds.iter().filter(|d| matches(&e, d)).count()
        );
    }
    let mut shown: BTreeMap<String, usize> = BTreeMap::new();
    for d in &ds {
        let n = shown.entry(d.protocol.clone()).or_default();
        if *n < 2 {
            *n += 1;
            println!("{:<6} {}", d.protocol, d.info);
        }
    }
    let after = std::process::Command::new("logman")
        .args(["query", "-ets"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("Veyl-PacketCapture"));
    println!("session still running after stop: {after:?}");
}
