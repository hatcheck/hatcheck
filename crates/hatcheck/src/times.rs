//! Times as the Python code handled them: ISO 8601 stamps read the way
//! datetime.fromisoformat reads them, epoch seconds as floats, UTC stamps for --json, and
//! local clock times and durations for the text.

use chrono::{
    DateTime, Datelike, Local, LocalResult, NaiveDate, NaiveDateTime, NaiveTime, TimeZone,
    Timelike, Utc,
};

/// Seconds since the epoch, now.
pub fn now() -> f64 {
    let d = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    d.as_secs_f64()
}

/// Python's datetime.fromisoformat(stamp.replace("Z", "+00:00")).timestamp(): None when
/// it isn't a time Python would read. A stamp without an offset is local time.
pub fn parse_time(stamp: &str) -> Option<f64> {
    let stamp = stamp.replace('Z', "+00:00");
    let (date, rest) = parse_date(&stamp)?;
    let mut chars = rest.chars();
    let (time, offset) = match chars.next() {
        None => ((0, 0, 0, 0), None),
        Some(_separator) => parse_time_and_offset(chars.as_str())?,
    };
    let (h, m, s, us) = time;
    let naive = date.and_time(NaiveTime::from_hms_micro_opt(h, m, s, us)?);
    let micros = match offset {
        Some(offset) => naive.and_utc().timestamp_micros() - i64::from(offset) * 1_000_000,
        None => local_micros(naive)?,
    };
    Some(micros as f64 / 1e6)
}

/// Microseconds since the epoch of a local wall-clock time; the earlier one when the clock
/// went back, and the time an hour on when it skipped ahead.
fn local_micros(naive: NaiveDateTime) -> Option<i64> {
    match Local.from_local_datetime(&naive) {
        LocalResult::Single(t) | LocalResult::Ambiguous(t, _) => Some(t.timestamp_micros()),
        LocalResult::None => {
            let later = naive + chrono::Duration::hours(1);
            Local
                .from_local_datetime(&later)
                .earliest()
                .map(|t| t.timestamp_micros() - 3_600_000_000)
        }
    }
}

fn digits(text: &str, n: usize) -> Option<u32> {
    let part = text.get(..n)?;
    part.bytes()
        .all(|b| b.is_ascii_digit())
        .then(|| part.parse().ok())?
}

/// YYYY-MM-DD or YYYYMMDD, and what follows.
fn parse_date(text: &str) -> Option<(NaiveDate, &str)> {
    let (y, m, d, len) = if text.get(4..5) == Some("-") {
        (
            digits(text, 4)?,
            digits(text.get(5..)?, 2)?,
            digits(text.get(8..)?, 2).filter(|_| text.get(7..8) == Some("-"))?,
            10,
        )
    } else {
        (
            digits(text, 4)?,
            digits(text.get(4..)?, 2)?,
            digits(text.get(6..)?, 2)?,
            8,
        )
    };
    Some((NaiveDate::from_ymd_opt(y as i32, m, d)?, &text[len..]))
}

/// HH[:MM[:SS[.ffffff]]] (or HHMMSS) and an optional +HH:MM offset, in seconds.
/// Hours, minutes, seconds and microseconds.
type Clock = (u32, u32, u32, u32);

fn parse_time_and_offset(text: &str) -> Option<(Clock, Option<i32>)> {
    let split = text.find(['+', '-']).unwrap_or(text.len());
    let (time, offset) = text.split_at(split);
    let clock = parse_clock(time)?;
    let offset = if offset.is_empty() {
        None
    } else {
        let sign = if offset.starts_with('-') { -1 } else { 1 };
        let (h, m, s, us) = parse_clock(&offset[1..])?;
        if us != 0 || h > 23 {
            return None;
        }
        Some(sign * (h * 3600 + m * 60 + s) as i32)
    };
    Some((clock, offset))
}

fn parse_clock(text: &str) -> Option<Clock> {
    let (main, fraction) = match text.find(['.', ',']) {
        Some(i) => (&text[..i], Some(&text[i + 1..])),
        None => (text, None),
    };
    let fields: Vec<u32> = if main.contains(':') {
        let parts: Vec<&str> = main.split(':').collect();
        if parts.len() > 3 || parts.iter().any(|p| p.len() != 2) {
            return None;
        }
        parts.iter().map(|p| digits(p, 2)).collect::<Option<_>>()?
    } else {
        if main.is_empty() || main.len() % 2 != 0 || main.len() > 6 {
            return None;
        }
        (0..main.len() / 2)
            .map(|i| digits(&main[i * 2..], 2))
            .collect::<Option<_>>()?
    };
    if fields.is_empty() {
        return None;
    }
    let (h, m, s) = (
        fields[0],
        *fields.get(1).unwrap_or(&0),
        *fields.get(2).unwrap_or(&0),
    );
    if h > 23 || m > 59 || s > 59 {
        return None;
    }
    let us = match fraction {
        None => 0,
        Some(f) if !f.is_empty() && f.bytes().all(|b| b.is_ascii_digit()) && fields.len() == 3 => {
            let six: String = f.chars().chain(std::iter::repeat('0')).take(6).collect();
            six.parse().ok()?
        }
        Some(_) => return None,
    };
    Some((h, m, s, us))
}

/// Whole seconds and microseconds, as datetime.fromtimestamp splits a float: the
/// microseconds rounded half to even, carrying into the seconds.
fn split_seconds(t: f64) -> (i64, u32) {
    let whole = t.trunc();
    let mut secs = whole as i64;
    let mut us = ((t - whole) * 1e6).round_ties_even() as i64;
    if us >= 1_000_000 {
        secs += 1;
        us -= 1_000_000;
    } else if us < 0 {
        secs -= 1;
        us += 1_000_000;
    }
    (secs, us as u32)
}

pub fn utc(t: f64) -> Option<DateTime<Utc>> {
    let (secs, us) = split_seconds(t);
    Utc.timestamp_opt(secs, us * 1000).single()
}

pub fn local(t: f64) -> Option<DateTime<Local>> {
    let (secs, us) = split_seconds(t);
    Local.timestamp_opt(secs, us * 1000).earliest()
}

/// A time as ISO 8601 in UTC, as --json gives times: 2026-10-17T21:58:36Z.
pub fn iso_utc(t: f64) -> Option<String> {
    utc(t).map(|d| d.format("%Y-%m-%dT%H:%M:%SZ").to_string())
}

/// datetime.fromtimestamp(t, timezone.utc).isoformat(): 2026-10-17T21:58:36+00:00, with
/// microseconds when there are any.
pub fn isoformat_utc(t: f64) -> Option<String> {
    utc(t).map(|d| {
        if d.nanosecond() == 0 {
            d.format("%Y-%m-%dT%H:%M:%S+00:00").to_string()
        } else {
            d.format("%Y-%m-%dT%H:%M:%S%.6f+00:00").to_string()
        }
    })
}

/// How long until (or since) something, as the plan meters say it: 45m, 3h05m, 2d12h, 14d.
pub fn duration(seconds: f64) -> String {
    let seconds = (seconds.trunc() as i64).max(0);
    if seconds < 60 {
        "<1m".into()
    } else if seconds < 3600 {
        format!("{}m", seconds / 60)
    } else if seconds < 86400 {
        format!("{}h{:02}m", seconds / 3600, seconds % 3600 / 60)
    } else if seconds < 10 * 86400 {
        format!("{}d{}h", seconds / 86400, seconds % 86400 / 3600)
    } else {
        format!("{}d", seconds / 86400)
    }
}

/// How long ago, in the session list's short form: 4m, 3h, 2d.
pub fn ago(seconds: f64) -> String {
    if seconds < 3600.0 {
        format!("{}m", ((seconds / 60.0).floor()).max(0.0) as i64)
    } else if seconds < 86400.0 {
        format!("{}h", (seconds / 3600.0).floor() as i64)
    } else {
        format!("{}d", (seconds / 86400.0).floor() as i64)
    }
}

/// When a limit resets, by the local clock: 9:40pm within a day, Sat 9:40pm within the
/// week, else Nov 5.
pub fn clock(t: f64, now: f64) -> String {
    let Some(d) = local(t) else { return "?".into() };
    let (pm, hour) = d.hour12();
    let time_of_day = format!("{}:{:02}{}", hour, d.minute(), if pm { "pm" } else { "am" });
    if (t - now).abs() < 86400.0 {
        time_of_day
    } else if (t - now).abs() < 6.0 * 86400.0 {
        format!("{} {}", d.format("%a"), time_of_day)
    } else {
        format!("{} {}", d.format("%b"), d.day())
    }
}

/// time.strftime("%b %d %H:%M", time.localtime(t)): Oct 17 21:58.
pub fn short_local(t: f64) -> String {
    match Local.timestamp_opt(t.floor() as i64, 0).earliest() {
        Some(d) => d.format("%b %d %H:%M").to_string(),
        None => "?".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_stamps_python_reads() {
        assert_eq!(parse_time("2026-10-17T21:58:36.123Z"), Some(1792274316.123));
        assert_eq!(parse_time("2026-10-17T21:58:36+00:00"), Some(1792274316.0));
        assert_eq!(parse_time("2026-10-17T23:58:36+02:00"), Some(1792274316.0));
        assert_eq!(
            parse_time("2026-10-17T21:58:36.123456789Z"),
            Some(1792274316.123456)
        );
        assert_eq!(parse_time("20261017T215836Z"), Some(1792274316.0));
        assert_eq!(parse_time("2026-10-17 21:58:36Z"), Some(1792274316.0));
        assert_eq!(parse_time("nonsense"), None);
        assert_eq!(parse_time("2026-13-01T00:00:00Z"), None);
        assert_eq!(parse_time("2026-10-17T24:00:00Z"), None);
    }

    #[test]
    fn writes_utc_stamps() {
        assert_eq!(
            iso_utc(1792274316.9999996).as_deref(),
            Some("2026-10-17T21:58:37Z")
        );
        assert_eq!(
            isoformat_utc(1792274316.0).as_deref(),
            Some("2026-10-17T21:58:36+00:00")
        );
        assert_eq!(
            isoformat_utc(1792274316.5).as_deref(),
            Some("2026-10-17T21:58:36.500000+00:00")
        );
    }

    #[test]
    fn durations() {
        assert_eq!(duration(30.0), "<1m");
        assert_eq!(duration(3599.0), "59m");
        assert_eq!(duration(3.0 * 3600.0 + 300.0), "3h05m");
        assert_eq!(duration(2.5 * 86400.0), "2d12h");
        assert_eq!(duration(14.0 * 86400.0), "14d");
        assert_eq!(duration(-5.0), "<1m");
        assert_eq!(ago(-30.0), "0m");
        assert_eq!(ago(7200.0), "2h");
    }
}
