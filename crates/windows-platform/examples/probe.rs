//! Prints what every collector reports on this machine. Used to verify hardware
//! support and to fill COMPATIBILITY.md: `cargo run -p veyl-windows --example probe`.

use veyl_core::HardwareSensorProvider;
use veyl_windows::*;

fn main() {
    let now = 0;
    let (cpu, st) = cpu::cpu_info();
    println!("CPU: {cpu:?} {st:?}");
    let mut cs = cpu::CpuSampler::new();
    let mut gs = gpu::GpuSampler::new();
    let mut ds = disk::DiskSampler::new();
    let mut ms = memory::MemorySampler::new();
    std::thread::sleep(std::time::Duration::from_millis(1000));
    let c = cs.sample();
    println!(
        "CPU total {:?} utility {:?} base {:?} eff {:?} cores {} status {:?}",
        c.total_pct,
        c.utility_pct,
        c.base_mhz,
        c.effective_mhz,
        c.per_core_pct.len(),
        c.status
    );
    let (g, p) = gs.sample();
    for a in &g.adapters {
        println!(
            "GPU {} util {:?} vram {:?}/{} temp {:?} max {:?} fan {:?} power {:?} core {:?} mem {:?}\n    counters {:?}\n    sensors {:?}",
            a.name, a.util_pct, a.dedicated_used_bytes, a.dedicated_total_bytes, a.temperature_c, a.temperature_max_c, a.fan_rpm, a.power_pct, a.core_clock_mhz, a.memory_clock_mhz, a.counters_status, a.sensors_status
        );
    }
    println!("GPU processes with usage: {}", p.util_pct.len());
    let d = ds.sample();
    println!(
        "Disks: {:?} total r/w {:?}/{:?}",
        d.status, d.total_read_bps, d.total_write_bps
    );
    let m = ms.sample();
    println!(
        "Memory: load {:?} commit {:?}/{:?} pages-in {:?}",
        m.load_pct, m.commit_bytes, m.commit_limit_bytes, m.pages_input_per_sec
    );
    let tz = thermal::ThermalZoneProvider::new().poll(now);
    println!(
        "Thermal zones: {:?} {:?}",
        tz.status,
        tz.readings
            .iter()
            .map(|r| (&r.device, &r.label, r.value))
            .collect::<Vec<_>>()
    );
    let mut em = energy::EnergyMeterProvider::new();
    let _ = em.poll(now);
    let mut ad = adlx::AdlxProvider::new();
    std::thread::sleep(std::time::Duration::from_millis(1100));
    for (name, poll) in [
        ("Energy Meter", em.poll(now)),
        ("ADLX", ad.poll(now)),
        ("NVML", nvml::NvmlProvider::new().poll(now)),
        ("PawnIO CPU", pawnio::IntelCpuTempProvider::new().poll(now)),
        ("PawnIO SuperIO", superio::SuperIoProvider::new().poll(now)),
    ] {
        println!("{name}: {:?}", poll.status);
        for r in poll.readings {
            println!("    {} · {} = {:?} {}", r.device, r.label, r.value, r.unit);
        }
    }
    for dr in storage_health::read_all() {
        println!(
            "Drive {} {:?} {:?} temp {:?} nvme {:?} health {:?}",
            dr.index,
            dr.model,
            dr.bus_type,
            dr.temperature_c,
            dr.nvme
                .as_ref()
                .map(|n| (n.percentage_used, n.power_on_hours)),
            dr.health_status
        );
    }
    let dumps = debugger::list_dumps();
    println!(
        "Dumps: {} files, inaccessible {:?}; cdb {:?}",
        dumps.files.len(),
        dumps.inaccessible,
        debugger::find_cdb(None)
    );
    println!("Elevated: {}", elevation::is_elevated());
}
