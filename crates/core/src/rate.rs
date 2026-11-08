//! Rates from cumulative counters.

use std::time::Duration;

/// Per-second rate of a monotonically increasing counter between two readings.
///
/// Returns `None` when no time elapsed or when the counter went backwards
/// (a driver reset, a reused PID, a re-enumerated adapter): an unknown rate is
/// reported as unknown rather than as zero or as a huge wrapped value.
pub fn per_second(previous: u64, current: u64, elapsed: Duration) -> Option<f64> {
    let secs = elapsed.as_secs_f64();
    if secs <= 0.0 || current < previous {
        return None;
    }
    Some((current - previous) as f64 / secs)
}

/// Share (0..=100) of `capacity` consumed by `used` over an interval, both in the
/// same unit (e.g. 100-ns CPU time ticks). Clamped because counter granularity
/// can push a fully busy process marginally over its share.
pub fn share_pct(used: u64, capacity: u64) -> Option<f32> {
    if capacity == 0 {
        return None;
    }
    Some(((used as f64 / capacity as f64) * 100.0).clamp(0.0, 100.0) as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn per_second_divides_by_real_elapsed() {
        assert_eq!(
            per_second(1_000, 3_000, Duration::from_millis(500)),
            Some(4_000.0)
        );
        assert_eq!(per_second(5, 5, Duration::from_secs(1)), Some(0.0));
    }

    #[test]
    fn per_second_rejects_reset_and_zero_elapsed() {
        assert_eq!(per_second(10, 3, Duration::from_secs(1)), None);
        assert_eq!(per_second(1, 3, Duration::ZERO), None);
    }

    #[test]
    fn per_second_handles_huge_counters() {
        let r = per_second(u64::MAX - 10, u64::MAX, Duration::from_secs(2)).expect("rate");
        assert_eq!(r, 5.0);
    }

    #[test]
    fn share_clamps_and_rejects_zero_capacity() {
        assert_eq!(share_pct(50, 200), Some(25.0));
        assert_eq!(share_pct(300, 200), Some(100.0));
        assert_eq!(share_pct(1, 0), None);
    }
}
