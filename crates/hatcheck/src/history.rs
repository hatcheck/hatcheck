//! The readings of each plan limit hatcheck has seen, kept so the usage can be read as a
//! trend: how fast a limit is being used up, and whether, at that pace, it runs out before it
//! resets. Every reading `hatcheck`, `hatcheck usage` and `hatcheck best` take, live or from an
//! agent's snapshot, is appended to usage-history.jsonl in hatcheck's cache folder, one line per
//! limit: the account, the limit, the percentage used, the reset ahead and when it was read; a
//! reading already there isn't written twice, and lines older than 30 days go when the file is
//! next rewritten. Nothing is sent anywhere, and the file is a cache: cleared, the trend starts
//! over. docs/commands.md's `hatcheck usage` section says what the trend means.

use crate::facts;
use crate::ospath;
use crate::py::{self, get, s, Num};
use crate::times;
use crate::usage::{Entry, Usage};
use serde_json::{json, Value};
use std::sync::Mutex;

const FILE: &str = "usage-history.jsonl";
/// How long a reading is kept.
const KEEP: f64 = 30.0 * 86400.0;
/// How far apart the first and last readings in a window have to be for a pace to be told
/// from them: two readings a minute apart say nothing about the hours ahead.
const MIN_SPAN: f64 = 600.0;

/// One reading of one limit.
#[derive(Clone, Debug, PartialEq)]
pub struct Reading {
    pub account: String,
    pub label: String,
    pub used: f64,
    pub resets_at: Option<f64>,
    pub window: Option<f64>,
    pub t: f64,
}

/// What the readings in a limit's current window say.
#[derive(Clone, Debug, PartialEq)]
pub struct Trend {
    /// Percentage points used per hour, from the first reading in the window to the last.
    pub pace: f64,
    /// How long the readings span, in seconds.
    pub over: f64,
    pub readings: usize,
    /// When, at that pace, the limit runs out: None when it doesn't before the reset, or when
    /// there is no reset ahead to measure against and the pace is zero.
    pub runs_out_at: Option<f64>,
    /// Whether that's before the limit resets; None without a reset ahead.
    pub before_reset: Option<bool>,
}

pub fn path() -> String {
    ospath::join(&facts::cache_dir(), FILE)
}

/// The key a reading files an account under: its agent and first profile or home.
pub fn account_key(entry: &Entry) -> String {
    format!(
        "{}:{}",
        entry.agent.key(),
        entry
            .profiles
            .first()
            .map_or(String::new(), |p| ospath::abspath(p))
    )
}

fn reading_of(line: &str) -> Option<Reading> {
    let v: Value = serde_json::from_str(line).ok()?;
    if !v.is_object() {
        return None;
    }
    Some(Reading {
        account: s(get(&v, "account"))?.to_string(),
        label: s(get(&v, "label"))?.to_string(),
        used: Num::of(get(&v, "used"))?.f(),
        resets_at: Num::of(get(&v, "resets_at")).map(Num::f),
        window: Num::of(get(&v, "window")).map(Num::f),
        t: Num::of(get(&v, "t"))?.f(),
    })
}

fn line_of(r: &Reading) -> String {
    py::dumps_compact(
        &json!({"t": r.t, "account": r.account, "label": r.label, "used": r.used,
        "resets_at": r.resets_at, "window": r.window}),
    )
}

static LOADED: Mutex<Option<Vec<Reading>>> = Mutex::new(None);

/// Every reading in the file, in its order; read once per run.
pub fn readings() -> Vec<Reading> {
    let mut guard = LOADED.lock().unwrap_or_else(|e| e.into_inner());
    guard
        .get_or_insert_with(|| {
            std::fs::read_to_string(path())
                .map(|text| text.lines().filter_map(reading_of).collect())
                .unwrap_or_default()
        })
        .clone()
}

/// The readings in what was just loaded: each labeled limit of each account with numbers,
/// at the time they were read (live: now; a snapshot: when it was saved).
fn readings_in(loaded: &[(Entry, Usage)], now: f64) -> Vec<Reading> {
    let mut found = Vec::new();
    for (entry, (usage, saved_at, _)) in loaded {
        let Some(usage) = usage.as_ref().filter(|u| py::truthy(u)) else {
            continue;
        };
        let account = account_key(entry);
        let t = saved_at.unwrap_or(now);
        for row in crate::usage::usage_rows(usage) {
            if !row.labeled {
                continue;
            }
            let Some(used) = Num::of(&row.used).filter(|_| py::truthy(&row.used)) else {
                continue;
            };
            found.push(Reading {
                account: account.clone(),
                label: row.label.clone(),
                used: used.f(),
                resets_at: s(&row.resets_at).and_then(times::parse_time),
                window: Num::of(&row.window)
                    .filter(|_| py::truthy(&row.window))
                    .map(Num::f),
                t,
            });
        }
    }
    found
}

/// Write the readings just loaded to the file, leaving out any already there, and the lines
/// older than KEEP. A file that can't be written is left as it is: the trend is only ever
/// advice.
pub fn record(loaded: &[(Entry, Usage)], now: f64) {
    let new = readings_in(loaded, now);
    if new.is_empty() {
        return;
    }
    let old = readings();
    let fresh: Vec<&Reading> = new
        .iter()
        .filter(|r| {
            !old.iter()
                .any(|o| o.account == r.account && o.label == r.label && (o.t - r.t).abs() < 1.0)
        })
        .collect();
    if fresh.is_empty() {
        return;
    }
    let keep_from = now - KEEP;
    let dir = facts::cache_dir();
    let path = path();
    let written = if old.iter().any(|o| o.t < keep_from) {
        // Rewritten without the old lines, through a temporary file.
        let kept: Vec<String> = old
            .iter()
            .filter(|o| o.t >= keep_from)
            .chain(fresh.iter().copied())
            .map(line_of)
            .collect();
        let tmp = format!("{path}.{}.tmp", std::process::id());
        std::fs::create_dir_all(&dir)
            .and_then(|()| std::fs::write(&tmp, kept.join("\n") + "\n"))
            .and_then(|()| std::fs::rename(&tmp, &path))
    } else {
        use std::io::Write;
        let text: String = fresh.iter().map(|r| line_of(r) + "\n").collect();
        std::fs::create_dir_all(&dir).and_then(|()| {
            std::fs::OpenOptions::new()
                .append(true)
                .create(true)
                .open(&path)
                .and_then(|mut f| f.write_all(text.as_bytes()))
        })
    };
    if written.is_ok() {
        let mut guard = LOADED.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(all) = guard.as_mut() {
            all.extend(fresh.into_iter().cloned());
        }
    }
}

/// The trend of one limit from its readings: those in the window the newest reading is in
/// (the same reset ahead, or, without one, the last window's length, or a day), the first
/// against the last. None with fewer than two readings far enough apart, or when the limit
/// has reset since the newest reading.
pub fn trend_of(all: &[Reading], now: f64) -> Option<Trend> {
    let newest = all.iter().max_by(|a, b| a.t.total_cmp(&b.t))?;
    if newest.resets_at.is_some_and(|r| r <= now) {
        return None;
    }
    let since = match (newest.resets_at, newest.window) {
        (Some(r), Some(w)) => r - w,
        (_, Some(w)) => newest.t - w,
        _ => newest.t - 86400.0,
    };
    let mut in_window: Vec<&Reading> = all
        .iter()
        .filter(|r| r.t >= since && r.t <= newest.t && r.resets_at == newest.resets_at)
        .collect();
    in_window.sort_by(|a, b| a.t.total_cmp(&b.t));
    let (first, last) = (in_window.first()?, in_window.last()?);
    let over = last.t - first.t;
    if in_window.len() < 2 || over < MIN_SPAN {
        return None;
    }
    let pace = ((last.used - first.used).max(0.0) / (over / 3600.0) * 10.0).round() / 10.0;
    let reset = newest.resets_at.filter(|r| *r > now);
    let runs_out_at = (pace > 0.0).then(|| last.t + (100.0 - last.used).max(0.0) / pace * 3600.0);
    let before_reset = reset.map(|r| runs_out_at.is_some_and(|out| out < r));
    Some(Trend {
        pace,
        over,
        readings: in_window.len(),
        runs_out_at: runs_out_at.filter(|_| before_reset != Some(false)),
        before_reset,
    })
}

/// The trend of an account's limit, from the file.
pub fn trend(account: &str, label: &str, now: f64) -> Option<Trend> {
    let all: Vec<Reading> = readings()
        .into_iter()
        .filter(|r| r.account == account && r.label == label)
        .collect();
    trend_of(&all, now)
}

/// The trend as a row's note: `using 3.2%/h over 2h, runs out Thu 2pm` or `using 3.2%/h
/// over 2h, lasts to the reset`, or "" without one.
pub fn text(t: &Trend, now: f64) -> String {
    let pace = format!("using {}%/h over {}", t.pace, times::duration(t.over));
    match (t.runs_out_at, t.before_reset) {
        (Some(out), Some(true)) => format!("{pace}, runs out {}", times::clock(out, now)),
        (_, Some(false)) => format!("{pace}, lasts to the reset"),
        (Some(out), None) => format!("{pace}, out {}", times::clock(out, now)),
        _ => pace,
    }
}

pub fn to_json(t: Option<&Trend>) -> Value {
    match t {
        None => Value::Null,
        Some(t) => {
            json!({"pace_percent_per_hour": t.pace, "over_seconds": t.over.round(), "readings": t.readings,
            "runs_out_at": t.runs_out_at.and_then(times::iso_utc), "before_reset": t.before_reset})
        }
    }
}

/// Add each labeled row's trend to its note, for the lines hatcheck usage and best print.
pub fn annotate(rows: &mut [crate::usage::Row], account: &str, now: f64) {
    for row in rows.iter_mut().filter(|r| r.labeled) {
        if let Some(t) = trend(account, &row.label, now) {
            let note = text(&t, now);
            row.note = if row.note.is_empty() {
                note
            } else {
                format!("{} · {note}", row.note)
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reading(t: f64, used: f64, reset: f64) -> Reading {
        Reading {
            account: "claude:/p".into(),
            label: "week".into(),
            used,
            resets_at: Some(reset),
            window: Some(7.0 * 86400.0),
            t,
        }
    }

    #[test]
    fn a_pace_needs_two_readings_apart_in_one_window() {
        let now = 1_800_000_000.0;
        let reset = now + 4.0 * 86400.0;
        assert_eq!(trend_of(&[reading(now - 7200.0, 20.0, reset)], now), None);
        // Two readings two hours apart: 32 points in 2h is 16%/h, out in 3h, before the reset.
        let t = trend_of(
            &[
                reading(now - 7200.0, 20.0, reset),
                reading(now, 52.0, reset),
            ],
            now,
        )
        .unwrap();
        assert_eq!(
            (t.pace, t.over, t.readings, t.before_reset),
            (16.0, 7200.0, 2, Some(true))
        );
        assert_eq!(t.runs_out_at, Some(now + 3.0 * 3600.0));
        assert!(text(&t, now).starts_with("using 16%/h over 2h00m, runs out "));
        // Slower: lasts to the reset.
        let t = trend_of(
            &[
                reading(now - 7200.0, 20.0, reset),
                reading(now, 21.0, reset),
            ],
            now,
        )
        .unwrap();
        assert_eq!(
            (t.pace, t.runs_out_at, t.before_reset),
            (0.5, None, Some(false))
        );
        assert_eq!(text(&t, now), "using 0.5%/h over 2h00m, lasts to the reset");
        // Readings from before this window don't count, nor two a minute apart.
        let old = reading(now - 10.0 * 86400.0, 90.0, reset - 7.0 * 86400.0);
        assert_eq!(trend_of(&[old, reading(now, 52.0, reset)], now), None);
        assert_eq!(
            trend_of(
                &[reading(now - 60.0, 20.0, reset), reading(now, 52.0, reset)],
                now
            ),
            None
        );
        // A reset that has passed since the newest reading: nothing to say yet.
        assert_eq!(
            trend_of(
                &[
                    reading(now - 7200.0, 20.0, now - 1.0),
                    reading(now - 3600.0, 52.0, now - 1.0)
                ],
                now
            ),
            None
        );
    }

    #[test]
    fn a_line_round_trips() {
        let r = reading(1.5, 20.0, 100.0);
        assert_eq!(reading_of(&line_of(&r)), Some(r));
        assert_eq!(reading_of("not json"), None);
        assert_eq!(reading_of("{\"t\": 1}"), None);
    }
}
