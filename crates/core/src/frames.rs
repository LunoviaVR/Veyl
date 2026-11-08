//! Frame presentation records and the provider abstraction for frame capture.

use serde::{Deserialize, Serialize};

use crate::model::Status;

/// One presented frame as reported by a frame provider.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameRecord {
    pub pid: u32,
    pub application: String,
    pub swap_chain: String,
    pub runtime: String,
    pub present_mode: String,
    /// Seconds since the provider started capturing (provider clock, monotonic).
    pub time_secs: f64,
    /// Time since the previous present of the same swap chain.
    pub frametime_ms: f64,
    /// Time between displayed frames, when the provider reports it.
    pub display_ms: Option<f64>,
    /// Whether the frame was displayed (false = dropped), when reported.
    pub displayed: Option<bool>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FramePoll {
    pub status: Status,
    pub frames: Vec<FrameRecord>,
    /// Frames dropped by bounded buffers since the last poll.
    pub lost: u64,
}

/// A source of real presentation events for one process.
pub trait FrameMetricsProvider: Send {
    fn name(&self) -> &'static str;
    /// Starts capturing presents of `pid`. Restarting replaces any previous capture.
    fn start(&mut self, pid: u32) -> Result<(), String>;
    /// New frames since the last poll.
    fn poll(&mut self) -> FramePoll;
    /// Stops capturing and releases provider resources.
    fn stop(&mut self);
    fn is_running(&self) -> bool;
}

/// Picks the swap chain with the most frames: a process can present to several
/// (launcher overlay, game window), and mixing them would distort frame times.
pub fn dominant_swap_chain(frames: &[FrameRecord]) -> Option<String> {
    let mut counts: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for f in frames {
        *counts.entry(f.swap_chain.as_str()).or_insert(0) += 1;
    }
    counts
        .into_iter()
        .max_by(|a, b| a.1.cmp(&b.1).then(b.0.cmp(a.0)))
        .map(|(k, _)| k.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(sc: &str) -> FrameRecord {
        FrameRecord {
            pid: 1,
            application: "g.exe".into(),
            swap_chain: sc.into(),
            runtime: "DXGI".into(),
            present_mode: String::new(),
            time_secs: 0.0,
            frametime_ms: 16.6,
            display_ms: None,
            displayed: None,
        }
    }

    #[test]
    fn dominant_chain() {
        assert_eq!(
            dominant_swap_chain(&[f("a"), f("b"), f("b")]).as_deref(),
            Some("b")
        );
        assert_eq!(dominant_swap_chain(&[]), None);
        // Ties resolve deterministically to the lexicographically smaller id.
        assert_eq!(dominant_swap_chain(&[f("b"), f("a")]).as_deref(), Some("a"));
    }
}
