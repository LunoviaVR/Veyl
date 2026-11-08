//! Platform-independent core of PC Health Console.
//!
//! Everything here is pure: no operating-system calls, no I/O. Collectors in
//! `pch-windows` produce raw values; this crate defines the typed models they
//! normalize into, plus every calculation, interpretation and rule that turns
//! those values into statistics, alerts and diagnostic findings. Keeping it pure
//! makes all of it unit-testable on any machine with synthetic or recorded data.

pub mod aggregate;
pub mod alerts;
pub mod codes;
pub mod diagnostics;
pub mod events;
pub mod frame_stats;
pub mod frames;
pub mod model;
pub mod rate;
pub mod redact;
pub mod sensor_map;
pub mod time;

pub use model::*;
