//! Frame-time statistics (FPS, lows, percentiles, stutter, pacing).
//!
//! All inputs are per-frame times in milliseconds, in presentation order, as
//! reported by the frame provider (PresentMon `MsBetweenPresents` / `FrameTime`).
//! The exact definitions are documented in docs/METRICS.md and repeated here:
//!
//! - **Average FPS** = frames / total time = `1000 / mean(frame time)`. (Not the mean of
//!   per-frame FPS values, which overweights fast frames.)
//! - **Percentile frame time Pn** uses the nearest-rank method on the ascending sorted
//!   frame times: rank = ⌈n × p / 100⌉, value = sorted[rank − 1].
//! - **1 % low / 0.1 % low FPS** = `1000 / mean` of the slowest ⌈n × 1 %⌉ (resp. 0.1 %) frame
//!   times (at least one frame). This is the "average of the worst frames" definition.
//! - **Current FPS** = frames / time over the newest frames that together span at least
//!   `current_window_ms` (or all frames if fewer).
//! - **Stutter**: a frame whose time exceeds `stutter_factor` × the median frame time of the
//!   analysed window. Stutter time % = total stutter-frame time / total time × 100.
//! - **Pacing variation** = mean absolute difference between consecutive frame times; a
//!   perfectly paced sequence scores 0 regardless of its frame rate.
//!
//! Frame times that are not finite or not positive are discarded and counted in
//! `discarded_frames`. With no valid frames every statistic is `None`.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameStatsConfig {
    pub stutter_factor: f64,
    pub current_window_ms: f64,
}

impl Default for FrameStatsConfig {
    fn default() -> Self {
        // 2.5× median matches the common "stuttering factor" default used by frame analysis tools.
        Self {
            stutter_factor: 2.5,
            current_window_ms: 1000.0,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameStats {
    pub frames: usize,
    pub discarded_frames: usize,
    pub duration_ms: f64,
    pub avg_fps: Option<f64>,
    pub current_fps: Option<f64>,
    pub low_1pct_fps: Option<f64>,
    pub low_01pct_fps: Option<f64>,
    pub avg_frametime_ms: Option<f64>,
    pub p50_frametime_ms: Option<f64>,
    pub p95_frametime_ms: Option<f64>,
    pub p99_frametime_ms: Option<f64>,
    pub max_frametime_ms: Option<f64>,
    pub stddev_frametime_ms: Option<f64>,
    pub pacing_variation_ms: Option<f64>,
    pub stutter_count: usize,
    pub stutter_time_pct: Option<f64>,
}

/// Nearest-rank percentile of an ascending-sorted slice; `None` if empty.
pub fn percentile_sorted(sorted: &[f64], p: f64) -> Option<f64> {
    if sorted.is_empty() || !(0.0..=100.0).contains(&p) {
        return None;
    }
    let rank = ((p / 100.0) * sorted.len() as f64).ceil() as usize;
    sorted.get(rank.clamp(1, sorted.len()) - 1).copied()
}

/// `1000 / mean` of the slowest ⌈n × pct / 100⌉ frames of an ascending-sorted slice.
pub fn low_fps_sorted(sorted: &[f64], pct: f64) -> Option<f64> {
    if sorted.is_empty() || pct <= 0.0 {
        return None;
    }
    let count = ((pct / 100.0) * sorted.len() as f64).ceil().max(1.0) as usize;
    let slowest = &sorted[sorted.len() - count.min(sorted.len())..];
    let mean = slowest.iter().sum::<f64>() / slowest.len() as f64;
    (mean > 0.0).then(|| 1000.0 / mean)
}

pub fn compute(frametimes_ms: &[f64], cfg: &FrameStatsConfig) -> FrameStats {
    let valid: Vec<f64> = frametimes_ms
        .iter()
        .copied()
        .filter(|v| v.is_finite() && *v > 0.0)
        .collect();
    let discarded = frametimes_ms.len() - valid.len();
    if valid.is_empty() {
        return FrameStats {
            discarded_frames: discarded,
            ..Default::default()
        };
    }
    let n = valid.len();
    let total: f64 = valid.iter().sum();
    let mean = total / n as f64;

    let mut sorted = valid.clone();
    sorted.sort_by(f64::total_cmp);
    let median = percentile_sorted(&sorted, 50.0);

    let variance = valid.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / n as f64;
    let pacing = (n > 1)
        .then(|| valid.windows(2).map(|w| (w[1] - w[0]).abs()).sum::<f64>() / (n - 1) as f64);

    let (stutter_count, stutter_time) = match median {
        Some(m) => {
            let limit = m * cfg.stutter_factor;
            valid
                .iter()
                .filter(|v| **v > limit)
                .fold((0, 0.0), |(c, t), v| (c + 1, t + v))
        }
        None => (0, 0.0),
    };

    // Newest frames spanning at least the current window.
    let mut span = 0.0;
    let mut count = 0usize;
    for v in valid.iter().rev() {
        span += v;
        count += 1;
        if span >= cfg.current_window_ms {
            break;
        }
    }

    FrameStats {
        frames: n,
        discarded_frames: discarded,
        duration_ms: total,
        avg_fps: Some(1000.0 / mean),
        current_fps: Some(count as f64 * 1000.0 / span),
        low_1pct_fps: low_fps_sorted(&sorted, 1.0),
        low_01pct_fps: low_fps_sorted(&sorted, 0.1),
        avg_frametime_ms: Some(mean),
        p50_frametime_ms: median,
        p95_frametime_ms: percentile_sorted(&sorted, 95.0),
        p99_frametime_ms: percentile_sorted(&sorted, 99.0),
        max_frametime_ms: sorted.last().copied(),
        stddev_frametime_ms: Some(variance.sqrt()),
        pacing_variation_ms: pacing,
        stutter_count,
        stutter_time_pct: Some(stutter_time / total * 100.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Option<f64>, b: f64) -> bool {
        a.is_some_and(|a| (a - b).abs() < 1e-9)
    }

    #[test]
    fn empty_and_invalid_input_reports_nothing() {
        let s = compute(&[], &FrameStatsConfig::default());
        assert_eq!(s.frames, 0);
        assert_eq!(s.avg_fps, None);
        let s = compute(
            &[0.0, -1.0, f64::NAN, f64::INFINITY],
            &FrameStatsConfig::default(),
        );
        assert_eq!(s.frames, 0);
        assert_eq!(s.discarded_frames, 4);
        assert_eq!(s.low_1pct_fps, None);
    }

    #[test]
    fn single_frame() {
        let s = compute(&[20.0], &FrameStatsConfig::default());
        assert!(close(s.avg_fps, 50.0));
        assert!(close(s.low_1pct_fps, 50.0));
        assert!(close(s.p99_frametime_ms, 20.0));
        assert_eq!(s.pacing_variation_ms, None);
        assert_eq!(s.stutter_count, 0);
    }

    #[test]
    fn steady_sixty_fps() {
        let frames = vec![1000.0 / 60.0; 600];
        let s = compute(&frames, &FrameStatsConfig::default());
        assert!(close(s.avg_fps, 60.0));
        assert!(close(s.low_1pct_fps, 60.0));
        assert!(close(s.low_01pct_fps, 60.0));
        assert!(close(s.current_fps, 60.0));
        assert!(close(s.pacing_variation_ms, 0.0));
        assert_eq!(s.stutter_count, 0);
    }

    #[test]
    fn average_fps_is_frames_over_time_not_mean_of_fps() {
        // 10 ms and 30 ms: 2 frames in 40 ms = 50 FPS (mean of per-frame FPS would be 66.7).
        let s = compute(&[10.0, 30.0], &FrameStatsConfig::default());
        assert!(close(s.avg_fps, 50.0));
    }

    #[test]
    fn lows_use_slowest_frames_and_nearest_rank() {
        // 990 frames at 10 ms, 10 frames at 50 ms -> 1 % of 1000 = 10 slowest = all 50 ms.
        let mut frames = vec![10.0; 990];
        frames.extend(vec![50.0; 10]);
        let s = compute(&frames, &FrameStatsConfig::default());
        assert!(close(s.low_1pct_fps, 20.0));
        assert!(close(s.low_01pct_fps, 20.0));
        // rank ceil(1000*0.99)=990 -> sorted[989] = 10 ms; rank 991 would be 50 ms.
        assert!(close(s.p99_frametime_ms, 10.0));
        assert!(close(s.max_frametime_ms, 50.0));
        assert_eq!(s.stutter_count, 10);
        assert!(close(s.stutter_time_pct, 500.0 / 10_400.0 * 100.0));
    }

    #[test]
    fn percentile_edges() {
        let sorted = [1.0, 2.0, 3.0, 4.0];
        assert_eq!(percentile_sorted(&sorted, 0.0), Some(1.0));
        assert_eq!(percentile_sorted(&sorted, 50.0), Some(2.0));
        assert_eq!(percentile_sorted(&sorted, 100.0), Some(4.0));
        assert_eq!(percentile_sorted(&sorted, 101.0), None);
        assert_eq!(percentile_sorted(&[], 50.0), None);
    }

    #[test]
    fn current_fps_uses_newest_second() {
        // 100 old frames at 50 ms, then 200 new frames at 5 ms (1 s).
        let mut frames = vec![50.0; 100];
        frames.extend(vec![5.0; 200]);
        let s = compute(&frames, &FrameStatsConfig::default());
        assert!(close(s.current_fps, 200.0));
    }

    #[test]
    fn pacing_variation_detects_alternating_frames() {
        let frames: Vec<f64> = (0..100)
            .map(|i| if i % 2 == 0 { 10.0 } else { 20.0 })
            .collect();
        let s = compute(&frames, &FrameStatsConfig::default());
        assert!(close(s.pacing_variation_ms, 10.0));
        assert!(close(s.avg_fps, 1000.0 / 15.0));
    }
}
