//! Windows platform layer for Veyl.
//!
//! Every function that talks to the operating system lives here, behind small
//! typed APIs that return `veyl-core` models. Collectors own their OS handles and
//! release them in `Drop`; none of them are shared across threads. Failures are
//! returned as values (a `Status` or an error) so one failing source never
//! affects the others.
//!
//! Safety: `unsafe` blocks are confined to the FFI calls and buffer reads they
//! need; each states the invariant it relies on in a `// SAFETY:` comment.

#![cfg(windows)]

pub mod adlx;
pub mod app_windows;
pub mod connections;
pub mod cpu;
pub mod debugger;
pub mod disk;
pub mod display;
pub mod elevation;
pub mod energy;
pub mod eventlog;
pub mod games;
pub mod gpu;
pub mod handle;
pub mod memory;
pub mod network;
pub mod nvml;
pub mod packet_capture;
pub mod pawnio;
pub mod pdh;
pub mod presentmon;
pub mod process_control;
pub mod processes;
pub mod registry;
pub mod services;
pub mod startup;
pub mod storage_health;
pub mod superio;
pub mod system_info;
pub mod thermal;
pub mod version_info;
