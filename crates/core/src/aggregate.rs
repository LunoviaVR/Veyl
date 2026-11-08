//! Time-bucket aggregation for historical telemetry and gap marking for charts.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// avg/min/max of the samples of one metric inside one time bucket.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BucketValue {
    pub avg: f64,
    pub min: f64,
    pub max: f64,
    pub count: u32,
}

#[derive(Clone, Copy, Debug)]
struct Acc {
    sum: f64,
    min: f64,
    max: f64,
    count: u32,
}

/// Accumulates per-tick metric values into fixed-width wall-clock buckets.
///
/// `push` returns nothing; `drain_complete` hands out every bucket that ended
/// before `now_ms`, so a bucket is written once, after it is complete.
#[derive(Debug)]
pub struct BucketAggregator {
    width_ms: i64,
    // (bucket start, metric key) -> accumulator
    open: BTreeMap<(i64, String), Acc>,
}

impl BucketAggregator {
    pub fn new(width_ms: i64) -> Self {
        Self {
            width_ms: width_ms.max(1),
            open: BTreeMap::new(),
        }
    }

    pub fn width_ms(&self) -> i64 {
        self.width_ms
    }

    pub fn push(&mut self, timestamp_ms: i64, key: &str, value: f64) {
        if !value.is_finite() {
            return;
        }
        let start = timestamp_ms.div_euclid(self.width_ms) * self.width_ms;
        self.open
            .entry((start, key.to_owned()))
            .and_modify(|a| {
                a.sum += value;
                a.min = a.min.min(value);
                a.max = a.max.max(value);
                a.count += 1;
            })
            .or_insert(Acc {
                sum: value,
                min: value,
                max: value,
                count: 1,
            });
    }

    /// Removes and returns `(bucket_start_ms, key, value)` for buckets that ended at or before `now_ms`.
    pub fn drain_complete(&mut self, now_ms: i64) -> Vec<(i64, String, BucketValue)> {
        let width = self.width_ms;
        let done: Vec<(i64, String)> = self
            .open
            .keys()
            .filter(|(start, _)| start + width <= now_ms)
            .cloned()
            .collect();
        done.into_iter()
            .filter_map(|k| {
                self.open.remove(&k).map(|a| {
                    let v = BucketValue {
                        avg: a.sum / f64::from(a.count),
                        min: a.min,
                        max: a.max,
                        count: a.count,
                    };
                    (k.0, k.1, v)
                })
            })
            .collect()
    }

    /// Number of open (incomplete) buckets; bounded by metrics × in-flight buckets.
    pub fn open_len(&self) -> usize {
        self.open.len()
    }
}

/// Inserts `None` points where consecutive timestamps are further apart than
/// `max_gap_ms`, so charts draw a visible break instead of a line across
/// periods with no recorded data.
pub fn mark_gaps(points: &[(i64, f64)], max_gap_ms: i64) -> Vec<(i64, Option<f64>)> {
    let mut out = Vec::with_capacity(points.len() + 8);
    for (i, &(ts, v)) in points.iter().enumerate() {
        if i > 0 {
            let prev = points[i - 1].0;
            if ts - prev > max_gap_ms {
                out.push((prev + 1, None));
            }
        }
        out.push((ts, Some(v)));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aggregates_per_bucket_and_metric() {
        let mut agg = BucketAggregator::new(5_000);
        agg.push(10_000, "cpu", 10.0);
        agg.push(11_000, "cpu", 30.0);
        agg.push(14_999, "cpu", 20.0);
        agg.push(15_000, "cpu", 99.0);
        agg.push(12_000, "mem", 50.0);
        agg.push(12_000, "mem", f64::NAN);

        assert!(agg.drain_complete(14_999).is_empty());
        let mut done = agg.drain_complete(15_000);
        done.sort_by(|a, b| a.1.cmp(&b.1));
        assert_eq!(done.len(), 2);
        assert_eq!(
            done[0],
            (
                10_000,
                "cpu".into(),
                BucketValue {
                    avg: 20.0,
                    min: 10.0,
                    max: 30.0,
                    count: 3
                }
            )
        );
        assert_eq!(done[1].2.count, 1);
        assert_eq!(agg.open_len(), 1);
    }

    #[test]
    fn negative_timestamps_bucket_floor() {
        let mut agg = BucketAggregator::new(1000);
        agg.push(-1, "x", 1.0);
        let done = agg.drain_complete(0);
        assert_eq!(done[0].0, -1000);
    }

    #[test]
    fn gaps_are_marked_only_when_exceeded() {
        let pts = [(0, 1.0), (1000, 2.0), (5000, 3.0)];
        let out = mark_gaps(&pts, 2000);
        assert_eq!(
            out,
            vec![
                (0, Some(1.0)),
                (1000, Some(2.0)),
                (1001, None),
                (5000, Some(3.0))
            ]
        );
        assert!(mark_gaps(&[], 10).is_empty());
    }
}
