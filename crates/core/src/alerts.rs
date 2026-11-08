//! Threshold alert engine with trigger duration, hysteresis and cooldown.
//!
//! A rule watches one metric key, or every key matching a pattern with a single
//! `*` (one independent state per matching device, e.g. `gpu.*.temp_c`).
//! - It **fires** once the value has been beyond `threshold` continuously for
//!   `duration_secs`, unless it fired within the last `cooldown_secs`.
//! - It stays **active** until the value comes back past `threshold ∓ hysteresis`,
//!   then **resolves**. Fluctuation around the threshold therefore cannot re-fire.
//! - A missing reading (sensor unavailable) resets the pending timer and never
//!   fires or resolves anything: no data is not evidence.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::events::Severity;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Comparator {
    Above,
    Below,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ThresholdRule {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    /// Metric key or pattern with one `*` (see module docs).
    pub metric: String,
    pub comparator: Comparator,
    pub threshold: f64,
    pub hysteresis: f64,
    pub duration_secs: u32,
    pub cooldown_secs: u32,
    pub severity: Severity,
    pub notify: bool,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum AlertTransition {
    Fired,
    Resolved,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AlertSignal {
    pub rule_id: String,
    pub rule_name: String,
    pub metric: String,
    pub transition: AlertTransition,
    pub value: f64,
    pub threshold: f64,
    pub severity: Severity,
    pub notify: bool,
    pub time_ms: i64,
    /// When the condition first became true (for Fired) — the evidence window start.
    pub since_ms: i64,
}

#[derive(Default, Debug, Clone)]
struct State {
    beyond_since: Option<i64>,
    active: bool,
    last_fired: Option<i64>,
}

#[derive(Default, Debug)]
pub struct AlertEngine {
    states: HashMap<(String, String), State>,
}

/// True if `key` matches `pattern`, where `pattern` may contain a single `*`
/// standing for one or more characters without a `.`.
pub fn metric_matches(pattern: &str, key: &str) -> bool {
    match pattern.split_once('*') {
        None => pattern == key,
        Some((pre, post)) => {
            key.len() > pre.len() + post.len()
                && key.starts_with(pre)
                && key.ends_with(post)
                && !key[pre.len()..key.len() - post.len()].contains('.')
        }
    }
}

impl AlertEngine {
    pub fn new() -> Self {
        Self::default()
    }

    /// Evaluates every enabled rule against the current metric values.
    pub fn evaluate(
        &mut self,
        rules: &[ThresholdRule],
        values: &HashMap<String, f64>,
        now_ms: i64,
    ) -> Vec<AlertSignal> {
        let mut out = Vec::new();
        for rule in rules.iter().filter(|r| r.enabled) {
            let mut matched: Vec<&String> = values
                .keys()
                .filter(|k| metric_matches(&rule.metric, k))
                .collect();
            matched.sort();
            for key in matched {
                let Some(&value) = values.get(key) else {
                    continue;
                };
                if let Some(sig) = self.step(rule, key, value, now_ms) {
                    out.push(sig);
                }
            }
            // Metrics that disappeared reset their pending timers.
            for ((rid, key), st) in self.states.iter_mut() {
                if rid == &rule.id && !values.contains_key(key) {
                    st.beyond_since = None;
                }
            }
        }
        // Drop states of rules that were removed or disabled.
        self.states
            .retain(|(rid, _), _| rules.iter().any(|r| r.enabled && &r.id == rid));
        out
    }

    fn step(
        &mut self,
        rule: &ThresholdRule,
        key: &str,
        value: f64,
        now_ms: i64,
    ) -> Option<AlertSignal> {
        if !value.is_finite() {
            return None;
        }
        let st = self
            .states
            .entry((rule.id.clone(), key.to_owned()))
            .or_default();
        let hyst = rule.hysteresis.abs();
        let (beyond, cleared) = match rule.comparator {
            Comparator::Above => (value > rule.threshold, value <= rule.threshold - hyst),
            Comparator::Below => (value < rule.threshold, value >= rule.threshold + hyst),
        };
        let signal = |transition, since_ms| AlertSignal {
            rule_id: rule.id.clone(),
            rule_name: rule.name.clone(),
            metric: key.to_owned(),
            transition,
            value,
            threshold: rule.threshold,
            severity: rule.severity,
            notify: rule.notify,
            time_ms: now_ms,
            since_ms,
        };

        if st.active {
            if cleared {
                st.active = false;
                st.beyond_since = None;
                return Some(signal(AlertTransition::Resolved, now_ms));
            }
            return None;
        }
        if !beyond {
            st.beyond_since = None;
            return None;
        }
        let since = *st.beyond_since.get_or_insert(now_ms);
        let held = now_ms - since >= i64::from(rule.duration_secs) * 1000;
        let cooling = st
            .last_fired
            .is_some_and(|t| now_ms - t < i64::from(rule.cooldown_secs) * 1000);
        if held && !cooling {
            st.active = true;
            st.last_fired = Some(now_ms);
            return Some(signal(AlertTransition::Fired, since));
        }
        None
    }

    /// Keys of currently active (fired, unresolved) alerts.
    pub fn active(&self) -> Vec<(String, String)> {
        self.states
            .iter()
            .filter(|(_, s)| s.active)
            .map(|(k, _)| k.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule() -> ThresholdRule {
        ThresholdRule {
            id: "gpu-temp".into(),
            name: "GPU temperature".into(),
            enabled: true,
            metric: "gpu.*.temp_c".into(),
            comparator: Comparator::Above,
            threshold: 85.0,
            hysteresis: 5.0,
            duration_secs: 10,
            cooldown_secs: 60,
            severity: Severity::Warning,
            notify: true,
        }
    }

    fn vals(pairs: &[(&str, f64)]) -> HashMap<String, f64> {
        pairs.iter().map(|(k, v)| ((*k).to_owned(), *v)).collect()
    }

    #[test]
    fn pattern_matching() {
        assert!(metric_matches("gpu.*.temp_c", "gpu.0.temp_c"));
        assert!(!metric_matches("gpu.*.temp_c", "gpu..temp_c"));
        assert!(!metric_matches("gpu.*.temp_c", "gpu.0.x.temp_c"));
        assert!(metric_matches("cpu.total_pct", "cpu.total_pct"));
        assert!(!metric_matches("cpu.total_pct", "cpu.total_pct2"));
    }

    #[test]
    fn fires_after_duration_and_not_before() {
        let mut e = AlertEngine::new();
        let r = [rule()];
        assert!(e
            .evaluate(&r, &vals(&[("gpu.0.temp_c", 90.0)]), 0)
            .is_empty());
        assert!(e
            .evaluate(&r, &vals(&[("gpu.0.temp_c", 90.0)]), 9_999)
            .is_empty());
        let s = e.evaluate(&r, &vals(&[("gpu.0.temp_c", 91.0)]), 10_000);
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].transition, AlertTransition::Fired);
        assert_eq!(s[0].since_ms, 0);
        assert_eq!(s[0].metric, "gpu.0.temp_c");
    }

    #[test]
    fn dip_below_threshold_resets_pending_timer() {
        let mut e = AlertEngine::new();
        let r = [rule()];
        e.evaluate(&r, &vals(&[("gpu.0.temp_c", 90.0)]), 0);
        e.evaluate(&r, &vals(&[("gpu.0.temp_c", 80.0)]), 5_000);
        assert!(e
            .evaluate(&r, &vals(&[("gpu.0.temp_c", 90.0)]), 10_000)
            .is_empty());
    }

    #[test]
    fn hysteresis_prevents_flapping_and_resolves() {
        let mut e = AlertEngine::new();
        let r = [rule()];
        e.evaluate(&r, &vals(&[("gpu.0.temp_c", 90.0)]), 0);
        assert_eq!(
            e.evaluate(&r, &vals(&[("gpu.0.temp_c", 90.0)]), 10_000)
                .len(),
            1
        );
        // 84 is below the threshold but above threshold - hysteresis (80): still active, no signal.
        assert!(e
            .evaluate(&r, &vals(&[("gpu.0.temp_c", 84.0)]), 11_000)
            .is_empty());
        assert!(e
            .evaluate(&r, &vals(&[("gpu.0.temp_c", 90.0)]), 30_000)
            .is_empty());
        let s = e.evaluate(&r, &vals(&[("gpu.0.temp_c", 79.0)]), 31_000);
        assert_eq!(s[0].transition, AlertTransition::Resolved);
    }

    #[test]
    fn cooldown_suppresses_refire() {
        let mut e = AlertEngine::new();
        let r = [rule()];
        e.evaluate(&r, &vals(&[("gpu.0.temp_c", 90.0)]), 0);
        e.evaluate(&r, &vals(&[("gpu.0.temp_c", 90.0)]), 10_000);
        e.evaluate(&r, &vals(&[("gpu.0.temp_c", 70.0)]), 11_000);
        e.evaluate(&r, &vals(&[("gpu.0.temp_c", 90.0)]), 12_000);
        assert!(
            e.evaluate(&r, &vals(&[("gpu.0.temp_c", 90.0)]), 22_000)
                .is_empty(),
            "inside 60 s cooldown"
        );
        assert_eq!(
            e.evaluate(&r, &vals(&[("gpu.0.temp_c", 90.0)]), 70_000)
                .len(),
            1
        );
    }

    #[test]
    fn devices_are_independent_and_missing_data_resets() {
        let mut e = AlertEngine::new();
        let r = [rule()];
        e.evaluate(
            &r,
            &vals(&[("gpu.0.temp_c", 90.0), ("gpu.1.temp_c", 90.0)]),
            0,
        );
        // gpu.1 disappears (sensor unavailable) at 5 s and returns at 6 s.
        e.evaluate(&r, &vals(&[("gpu.0.temp_c", 90.0)]), 5_000);
        e.evaluate(
            &r,
            &vals(&[("gpu.0.temp_c", 90.0), ("gpu.1.temp_c", 90.0)]),
            6_000,
        );
        let s = e.evaluate(
            &r,
            &vals(&[("gpu.0.temp_c", 90.0), ("gpu.1.temp_c", 90.0)]),
            10_000,
        );
        assert_eq!(
            s.iter().map(|s| s.metric.as_str()).collect::<Vec<_>>(),
            vec!["gpu.0.temp_c"]
        );
    }

    #[test]
    fn below_comparator_and_disabled_rules() {
        let mut e = AlertEngine::new();
        let mut r = rule();
        r.metric = "volume.C:.free_pct".into();
        r.comparator = Comparator::Below;
        r.threshold = 10.0;
        r.duration_secs = 0;
        let s = e.evaluate(&[r.clone()], &vals(&[("volume.C:.free_pct", 5.0)]), 0);
        assert_eq!(s.len(), 1);
        r.enabled = false;
        let mut e = AlertEngine::new();
        assert!(e
            .evaluate(&[r], &vals(&[("volume.C:.free_pct", 5.0)]), 0)
            .is_empty());
    }

    #[test]
    fn nan_is_ignored() {
        let mut e = AlertEngine::new();
        let mut r = rule();
        r.duration_secs = 0;
        assert!(e
            .evaluate(&[r], &vals(&[("gpu.0.temp_c", f64::NAN)]), 0)
            .is_empty());
    }
}
