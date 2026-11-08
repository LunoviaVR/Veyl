//! CSV and JSON report formatting (pure) for user-initiated exports.

use pch_core::events::StabilityEvent;
use pch_core::redact::{redact, RedactionContext};
use pch_core::time::format_iso8601_utc_ms;
use pch_storage::Series;

/// RFC 4180 field quoting.
pub fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_owned()
    }
}

fn num(v: Option<f64>) -> String {
    v.map(|v| format!("{v}")).unwrap_or_default()
}

/// One row per timestamp, one avg/min/max column triple per series. Gaps stay empty.
pub fn series_csv(series: &[Series]) -> String {
    let mut ts: Vec<i64> = series.iter().flat_map(|s| s.t.iter().copied()).collect();
    ts.sort_unstable();
    ts.dedup();
    let mut out = String::from("timestamp_utc_ms,time_utc");
    for s in series {
        for part in ["avg", "min", "max"] {
            out.push(',');
            out.push_str(&csv_field(&format!("{} {part} ({})", s.key, s.unit)));
        }
    }
    out.push('\n');
    // Each series' timestamps ascend, so one forward cursor per series keeps this linear.
    let mut cursor = vec![0usize; series.len()];
    for t in ts {
        out.push_str(&t.to_string());
        out.push(',');
        out.push_str(&format_iso8601_utc_ms(t));
        for (s, i) in series.iter().zip(cursor.iter_mut()) {
            while *i < s.t.len() && s.t[*i] < t {
                *i += 1;
            }
            if s.t.get(*i) == Some(&t) {
                for v in [s.avg[*i], s.min[*i], s.max[*i]] {
                    out.push(',');
                    out.push_str(&num(v));
                }
            } else {
                out.push_str(",,,");
            }
        }
        out.push('\n');
    }
    out
}

/// Frame index, cumulative time and frame time, as recorded.
pub fn frametimes_csv(frametimes: &[f32]) -> String {
    let mut out = String::from("frame,elapsed_ms,frametime_ms\n");
    let mut elapsed = 0.0f64;
    for (i, f) in frametimes.iter().enumerate() {
        elapsed += f64::from(*f);
        out.push_str(&format!("{i},{elapsed:.3},{f}\n"));
    }
    out
}

pub fn events_csv(events: &[StabilityEvent], ctx: Option<&RedactionContext>) -> String {
    let r = |s: &str| ctx.map(|c| redact(s, c)).unwrap_or_else(|| s.to_owned());
    let mut out = String::from(
        "time_utc_ms,time_utc,kind,severity,channel,provider,event_id,record_id,subject,title,data\n",
    );
    for e in events {
        let data: Vec<String> = e.data.iter().map(|(k, v)| format!("{k}={v}")).collect();
        let fields = [
            e.time_ms.to_string(),
            format_iso8601_utc_ms(e.time_ms),
            format!("{:?}", e.kind),
            format!("{:?}", e.severity),
            e.channel.clone(),
            e.provider.clone(),
            e.event_id.to_string(),
            e.record_id.to_string(),
            r(e.subject.as_deref().unwrap_or("")),
            r(&e.title),
            r(&data.join("; ")),
        ];
        out.push_str(
            &fields
                .iter()
                .map(|f| csv_field(f))
                .collect::<Vec<_>>()
                .join(","),
        );
        out.push('\n');
    }
    out
}

pub fn redaction_context(computer_name: Option<&str>) -> RedactionContext {
    RedactionContext {
        user_name: std::env::var("USERNAME").unwrap_or_default(),
        computer_name: computer_name.unwrap_or_default().to_owned(),
        user_profile: std::env::var("USERPROFILE").unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoting() {
        assert_eq!(csv_field("plain"), "plain");
        assert_eq!(csv_field("a,b"), "\"a,b\"");
        assert_eq!(csv_field("say \"hi\""), "\"say \"\"hi\"\"\"");
    }

    #[test]
    fn series_with_gap_and_missing_points() {
        let s = vec![
            Series {
                key: "a".into(),
                unit: "%".into(),
                bucket_ms: 5,
                source: "raw",
                t: vec![0, 5],
                avg: vec![Some(1.0), None],
                min: vec![Some(0.5), None],
                max: vec![Some(2.0), None],
            },
            Series {
                key: "b".into(),
                unit: "°C".into(),
                bucket_ms: 5,
                source: "raw",
                t: vec![5],
                avg: vec![Some(40.0)],
                min: vec![Some(39.0)],
                max: vec![Some(41.0)],
            },
        ];
        let csv = series_csv(&s);
        let lines: Vec<&str> = csv.lines().collect();
        assert_eq!(
            lines[0],
            "timestamp_utc_ms,time_utc,a avg (%),a min (%),a max (%),b avg (°C),b min (°C),b max (°C)"
        );
        assert_eq!(lines[1], "0,1970-01-01T00:00:00.000Z,1,0.5,2,,,");
        assert_eq!(lines[2], "5,1970-01-01T00:00:00.005Z,,,,40,39,41");
    }

    #[test]
    fn frametimes() {
        assert_eq!(
            frametimes_csv(&[10.0, 20.5]),
            "frame,elapsed_ms,frametime_ms\n0,10.000,10\n1,30.500,20.5\n"
        );
    }
}
