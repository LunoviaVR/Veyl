//! Timestamp conversions.

/// 100-ns intervals between 1601-01-01 (FILETIME epoch) and 1970-01-01.
/// Source: MS-DTYP §2.3.3 FILETIME; 11 644 473 600 s × 10^7.
const FILETIME_UNIX_EPOCH_DIFF: u64 = 116_444_736_000_000_000;

/// Converts a Windows FILETIME (100-ns ticks since 1601) to Unix milliseconds.
/// Returns `None` for 0 (the "unknown" FILETIME) and for times before 1970.
pub fn filetime_to_unix_ms(filetime: u64) -> Option<i64> {
    if filetime == 0 || filetime < FILETIME_UNIX_EPOCH_DIFF {
        return None;
    }
    i64::try_from((filetime - FILETIME_UNIX_EPOCH_DIFF) / 10_000).ok()
}

/// Parses the event log's `SystemTime` attribute, e.g. `2026-11-06T14:27:23.2194678Z`,
/// into Unix milliseconds. Accepts 0-9 fractional digits and a trailing `Z`.
pub fn parse_iso8601_utc_ms(s: &str) -> Option<i64> {
    let s = s.strip_suffix('Z').unwrap_or(s);
    let (date, time) = s.split_once('T')?;
    let mut d = date.split('-');
    let year: i64 = d.next()?.parse().ok()?;
    let month: u32 = d.next()?.parse().ok()?;
    let day: u32 = d.next()?.parse().ok()?;
    if d.next().is_some() || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let (hms, frac) = match time.split_once('.') {
        Some((a, b)) => (a, b),
        None => (time, ""),
    };
    let mut t = hms.split(':');
    let hour: i64 = t.next()?.parse().ok()?;
    let minute: i64 = t.next()?.parse().ok()?;
    let second: i64 = t.next()?.parse().ok()?;
    if t.next().is_some() || hour > 23 || minute > 59 || second > 60 {
        return None;
    }
    if !frac.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let millis: i64 = match frac.len() {
        0 => 0,
        n => {
            let digits = &frac[..n.min(3)];
            let v: i64 = digits.parse().ok()?;
            v * 10_i64.pow(3 - digits.len() as u32)
        }
    };
    let days = days_from_civil(year, month, day);
    Some(((days * 24 + hour) * 60 + minute) * 60_000 + second * 1000 + millis)
}

/// Formats Unix milliseconds as ISO 8601 UTC, e.g. `2026-11-06T14:27:23.219Z`.
pub fn format_iso8601_utc_ms(ms: i64) -> String {
    let (y, m, d) = civil_from_days(ms.div_euclid(86_400_000));
    let r = ms.rem_euclid(86_400_000);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.{:03}Z",
        r / 3_600_000,
        r / 60_000 % 60,
        r / 1000 % 60,
        r % 1000
    )
}

/// Proleptic Gregorian date for days since 1970-01-01 (Hinnant, civil_from_days).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    let y = yoe + era * 400;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Days since 1970-01-01 for a proleptic Gregorian date.
/// Algorithm: Howard Hinnant, "chrono-Compatible Low-Level Date Algorithms", days_from_civil.
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let m = i64::from(m);
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filetime_epoch_and_known_value() {
        assert_eq!(filetime_to_unix_ms(FILETIME_UNIX_EPOCH_DIFF), Some(0));
        // 2026-11-06T14:27:23.219Z
        assert_eq!(
            filetime_to_unix_ms(FILETIME_UNIX_EPOCH_DIFF + 1_793_975_243_219 * 10_000),
            Some(1_793_975_243_219)
        );
        assert_eq!(filetime_to_unix_ms(0), None);
        assert_eq!(filetime_to_unix_ms(5), None);
    }

    #[test]
    fn parses_event_log_times() {
        assert_eq!(parse_iso8601_utc_ms("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(
            parse_iso8601_utc_ms("2026-11-06T14:27:23.2194678Z"),
            Some(1_793_975_243_219)
        );
        assert_eq!(
            parse_iso8601_utc_ms("2000-02-29T12:00:00.5Z"),
            Some(951_825_600_500)
        );
        assert_eq!(
            parse_iso8601_utc_ms("2026-10-15T03:32:55Z"),
            Some(1_792_035_175_000)
        );
    }

    #[test]
    fn formats_round_trip() {
        for (ms, s) in [
            (0, "1970-01-01T00:00:00.000Z"),
            (951_825_600_500, "2000-02-29T12:00:00.500Z"),
            (1_793_975_243_219, "2026-11-06T14:27:23.219Z"),
            (-1, "1969-12-31T23:59:59.999Z"),
        ] {
            assert_eq!(format_iso8601_utc_ms(ms), s);
            assert_eq!(parse_iso8601_utc_ms(s), Some(ms));
        }
    }

    #[test]
    fn rejects_malformed_times() {
        for bad in [
            "",
            "2026-13-01T00:00:00Z",
            "2026-01-01",
            "2026-01-01T25:00:00Z",
            "x-y-zTa:b:c",
            "2026-01-01T00:00:00.1a",
        ] {
            assert_eq!(parse_iso8601_utc_ms(bad), None, "{bad}");
        }
    }
}
