//! hatcheck best [--for SESSION] [--cached] [--wait [PROFILE]]: which account to use now, and
//! where to carry a session. Advice only: it prints commands and never starts, moves or
//! switches anything. --wait sleeps until an account has room, then says which. The README's
//! `hatcheck best` section says how accounts are ranked.

use crate::args::Parser;
use crate::facts::Summary;
use crate::find;
use crate::out::{self, Paint};
use crate::paths;
use crate::py::{self, get, s};
use crate::sessions::session_command;
use crate::times;
use crate::usage::{self, Entry, Usage, FABLE_OUT, NO_FABLE};
use crate::widths::truncate;
use serde_json::Value;

/// After the Fable bands and the accounts without a Fable limit: those with no usage data.
const NO_DATA: u8 = NO_FABLE + 1;
/// The longest one sleep while waiting: the numbers are read again after it, whatever they
/// said before.
const LONGEST_SLEEP: f64 = 7.0 * 86400.0;

/// (short name, % left, next reset, kind) for an account's limits.
type Meter = (String, f64, Option<f64>, Value);

fn meters(usage: Option<&Value>, saved_at: Option<f64>, now: f64) -> Vec<Meter> {
    let Some(Value::Array(rows)) = usage.map(|u| get(u, "limits")) else {
        return Vec::new();
    };
    rows.iter()
        .filter(|r| r.is_object())
        .map(|r| {
            let (left, reset) = usage::meter_state(r, saved_at, now);
            (
                usage::row_label(r, true),
                left,
                reset,
                get(r, "kind").clone(),
            )
        })
        .collect()
}

fn scoped(kind: &Value) -> bool {
    s(kind).unwrap_or("").ends_with("_scoped")
}

/// Sort key and state: (band, time or -% left, reset), and whether it has room now. The
/// bands are usage's Fable ones, then NO_FABLE, ranked by the tightest limit, then NO_DATA.
fn rank(usage: Option<&Value>, saved_at: Option<f64>, now: f64) -> ((u8, f64, f64), bool) {
    if usage.is_none() {
        return ((NO_DATA, 0.0, 0.0), false);
    }
    let (band, at) = usage::fable_rank(usage, saved_at, now);
    if band < NO_FABLE {
        return ((band, at, 0.0), band < FABLE_OUT);
    }
    let rows: Vec<Meter> = meters(usage, saved_at, now)
        .into_iter()
        .filter(|m| !scoped(&m.3))
        .collect();
    let key = |m: &Meter| (m.1, m.2.unwrap_or(f64::INFINITY));
    let Some(tightest) = rows.iter().fold(None::<&Meter>, |best, m| match best {
        Some(b) if key(m).partial_cmp(&key(b)) == Some(std::cmp::Ordering::Less) => Some(m),
        Some(b) => Some(b),
        None => Some(m),
    }) else {
        return ((NO_FABLE, 0.0, 0.0), true);
    };
    (
        (NO_FABLE, -tightest.1, tightest.2.unwrap_or(f64::INFINITY)),
        tightest.1 > 0.0,
    )
}

/// ", 0.9%/h": what's left of a meter per hour until it resets, or "" without a reset ahead.
fn pace_part(left: f64, reset: Option<f64>, now: f64) -> String {
    usage::pace(left, reset, now)
        .filter(|_| left > 0.0)
        .map_or(String::new(), |p| format!(", {}", usage::pace_text(p)))
}

/// What the line says about an account: its Fable limit first, with its pace, then the
/// tightest other.
fn summary(result: &Usage, now: f64, room: bool) -> String {
    let (usage, saved_at, why) = result;
    let Some(u) = usage else {
        return format!("no data ({})", why.clone().unwrap_or_else(|| "None".into()));
    };
    let rows = meters(Some(u), *saved_at, now);
    let fable = rows
        .iter()
        .find(|m| s(&m.3) == Some("weekly_scoped") && m.0.to_lowercase().contains("fable"));
    let mut others: Vec<&Meter> = rows.iter().filter(|m| !scoped(&m.3)).collect();
    others.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    let mut parts: Vec<String> = Vec::new();
    if let Some((name, left, reset, _)) = fable {
        let when = match reset {
            Some(r) if *r != 0.0 && *left != 0.0 => format!(", resets {}", times::clock(*r, now)),
            Some(r) if *r != 0.0 => format!(", back {}", times::clock(*r, now)),
            _ => String::new(),
        };
        parts.push(format!(
            "{name} {}% left{when}{}",
            left.trunc() as i64,
            pace_part(*left, *reset, now)
        ));
    }
    for (n, (name, left, reset, _)) in others
        .iter()
        .take(if fable.is_some() { 1 } else { 2 })
        .enumerate()
    {
        match reset {
            Some(r) if *left <= 0.0 && *r != 0.0 => {
                parts.push(format!("{name} used up, back {}", times::clock(*r, now)))
            }
            _ => {
                // The tightest limit's pace, when there's no Fable limit to give one for.
                let pace = if fable.is_none() && n == 0 {
                    pace_part(*left, *reset, now)
                } else {
                    String::new()
                };
                parts.push(format!("{name} {}%{pace}", left.trunc() as i64))
            }
        }
    }
    if !room && fable.is_some_and(|f| f.1 > 0.0) {
        parts.push("(the weekly limit blocks Fable)".into());
    }
    let text = if parts.is_empty() {
        "no limits".to_string()
    } else {
        parts.join(" · ")
    };
    match saved_at {
        Some(t) => format!("{text} · cached {} ago", times::duration(now - t)),
        None => text,
    }
}

/// An account as best ranks it: its sort key, whether it has room now, and its usage.
type Ranked = ((u8, f64, f64), bool, Entry, Usage);

/// One agent's accounts, best first.
fn ranked(accounts: Vec<(Entry, Usage)>, now: f64) -> Vec<Ranked> {
    let mut out: Vec<_> = accounts
        .into_iter()
        .map(|(entry, result)| {
            let (key, room) = rank(result.0.as_ref(), result.1, now);
            (key, room, entry, result)
        })
        .collect();
    out.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    out
}

/// When an account without room will have some again, as far as its numbers say.
fn back_at(account: &Ranked) -> Option<f64> {
    let (band, at, reset) = account.0;
    let t = if band == FABLE_OUT {
        at
    } else if band == NO_FABLE {
        reset
    } else {
        return None;
    };
    (t != 0.0 && t.is_finite()).then_some(t)
}

/// How hatcheck move names a profile or home: default, NAME, codex, codex:NAME, or a path.
fn place_name(agent: &str, profile: &str) -> String {
    let home_dir = paths::home();
    let home = crate::ospath::join(
        &home_dir,
        if agent == "codex" {
            ".codex"
        } else {
            ".claude"
        },
    );
    if paths::same_folder(profile, &home) {
        return (if agent == "codex" { "codex" } else { "default" }).into();
    }
    let trimmed = profile.trim_end_matches(['/', crate::ospath::SEP.chars().next().unwrap_or('/')]);
    let (parent, base) = crate::ospath::split(trimmed);
    let prefix = format!("{}-", crate::ospath::basename(&home));
    if paths::same_folder(&parent, &home_dir) {
        if let Some(name) = base.strip_prefix(&prefix) {
            return format!("{}{name}", if agent == "codex" { "codex:" } else { "" });
        }
    }
    profile.to_string()
}

fn agent_name(agent: &str) -> &'static str {
    if agent == "codex" {
        "Codex"
    } else {
        "Claude Code"
    }
}

/// The one session whose ID starts with prefix: its agent and ID, or why there isn't one.
fn one_session(prefix: &str) -> Result<(&'static str, String), String> {
    let copies = find::session_copies(prefix);
    if copies.is_empty() {
        return Err(format!("No session starting with {prefix}"));
    }
    if copies.len() > 1 {
        return Err(format!(
            "{prefix} matches {} sessions; give more of the ID",
            copies.len()
        ));
    }
    let ((agent, sid), _) = &copies[0];
    Ok((agent, sid.clone()))
}

/// --for: the best other account for a session of the same agent, as a hatcheck move command.
fn carry(prefix: &str, by_agent: &[(&str, Vec<Ranked>)], now: f64, me: &str) -> i32 {
    let copies = find::session_copies(prefix);
    if copies.is_empty() {
        return out::fail(&format!("No session starting with {prefix}"));
    }
    if copies.len() > 1 {
        return out::fail(&format!(
            "{prefix} matches {} sessions; give more of the ID",
            copies.len()
        ));
    }
    let ((agent, sid), places) = &copies[0];
    let found: Vec<Summary> = places
        .iter()
        .filter_map(|(p, f)| crate::facts::summarize(agent, p, f))
        .collect();
    crate::facts::save_cache();
    let Some(s) = found.iter().fold(None::<&Summary>, |best, s| match best {
        Some(b) if (s.when, s.canonical, s.size) > (b.when, b.canonical, b.size) => Some(s),
        Some(b) => Some(b),
        None => Some(s),
    }) else {
        return out::fail(&format!(
            "Session {} holds no conversation",
            py::head(sid, 8)
        ));
    };
    let accounts = &by_agent
        .iter()
        .find(|(a, _)| a == agent)
        .expect("both agents are ranked")
        .1;
    let mine = accounts.iter().position(|a| {
        a.2.profiles
            .iter()
            .any(|p| paths::same_folder(p, &s.profile))
    });
    let state = match mine {
        Some(k) => summary(&accounts[k].3, now, accounts[k].1),
        None => "no usage data".into(),
    };
    out::println(&format!(
        "Session {} \"{}\" is in {}: {state}.",
        py::head(sid, 8),
        truncate(&s.title, 60),
        paths::shell_path(&s.profile)
    ));
    let best = accounts
        .iter()
        .enumerate()
        .position(|(k, a)| a.1 && Some(k) != mine);
    if let Some(k) = mine {
        if accounts[k].1 && k < best.unwrap_or(accounts.len()) {
            out::println("Its own account is the best one now; resume it there with:");
            if let Some(cwd) = s.cwd.as_deref().filter(|c| !c.is_empty()) {
                out::println(&format!("  cd {}", paths::shell_path(cwd)));
            }
            out::println(&format!("  {}", session_command(s, false)));
            return 0;
        }
    }
    let Some(b) = best else {
        out::println(&format!(
            "No other {} account has room now.",
            agent_name(agent)
        ));
        return 1;
    };
    let (_, _, entry, result) = &accounts[b];
    let dest = &entry.profiles[0];
    out::println(&format!(
        "Carry it to {}: {}.",
        paths::shell_path(dest),
        summary(result, now, true)
    ));
    out::println(&format!(
        "  {me} move {} {} --from {}",
        py::head(sid, 8),
        place_name(agent, dest),
        place_name(agent, &s.profile)
    ));
    out::println("It prints the command that resumes it there, and offers to run it.");
    0
}

/// Whom --wait waits for: an agent's accounts, or one profile or home of it.
struct Waiting {
    agent: &'static str,
    profile: Option<String>,
}

/// Whether what --wait waits for has room now; when it hasn't, when to look again, or the
/// reason nothing can be waited for.
fn wait_state(w: &Waiting, by_agent: &[(&str, Vec<Ranked>)]) -> Result<Option<f64>, String> {
    let accounts = &by_agent
        .iter()
        .find(|(a, _)| *a == w.agent)
        .expect("both agents are ranked")
        .1;
    let relevant: Vec<&Ranked> = accounts
        .iter()
        .filter(|a| {
            w.profile
                .as_ref()
                .is_none_or(|p| a.2.profiles.iter().any(|q| paths::same_folder(q, p)))
        })
        .collect();
    if relevant.is_empty() {
        return Err(match &w.profile {
            Some(p) => format!("{} has no sessions or usage to go by", paths::shell_path(p)),
            None => format!("No {} profile found", agent_name(w.agent)),
        });
    }
    if relevant.iter().any(|a| a.1) {
        return Ok(None); // has room
    }
    let wake = relevant
        .iter()
        .filter_map(|a| back_at(a))
        .fold(None::<f64>, |m, t| Some(m.map_or(t, |m| m.min(t))));
    match wake {
        Some(t) => Ok(Some(t)),
        None => Err(format!(
            "No {} account has room, and none says when it will again",
            agent_name(w.agent)
        )),
    }
}

pub fn main(argv: &[String], prog: &str) -> i32 {
    let parser = Parser::new(prog, "Which account to use now, and where to carry a session. Advice only: it never starts, moves or switches anything.")
        .option(&["--for"], "session", "SESSION", "where to carry this session: the hatcheck move command for the best other account")
        .flag(&["--cached"], "cached", "don't call the APIs; use the snapshots Claude Code and Codex last saved")
        .maybe(&["--wait"], "wait", "PROFILE", "", "wait until an account has room, then answer: any Claude Code account, PROFILE's (NAME, default, codex, codex:NAME), or with --for one of the session's agent");
    let args = match parser.parse(argv) {
        Ok(a) => a,
        Err(stop) => return stop.exit_code(),
    };
    let paint = Paint::new();
    let p = |code: &str, text: &str| paint.p(code, text);
    let me = prog.split_whitespace().next().unwrap_or("hatcheck");
    let session = args
        .value("session")
        .filter(|s| !s.is_empty())
        .map(String::from);
    let waiting = match args.value("wait") {
        None => None,
        Some(spec) if !spec.is_empty() => {
            let (agent, profile) = find::place(spec);
            Some(Waiting {
                agent,
                profile: Some(profile),
            })
        }
        Some(_) => {
            let agent = match &session {
                Some(prefix) => match one_session(prefix) {
                    Ok((agent, _)) => agent,
                    Err(why) => return out::fail(&why),
                },
                None => "claude",
            };
            Some(Waiting {
                agent,
                profile: None,
            })
        }
    };
    let load = || {
        usage::load_usage(
            &paths::find_profiles(),
            args.flag("cached"),
            &paths::find_codex_homes(),
        )
    };
    let mut now = times::now();
    let mut by_agent;
    loop {
        let (codex, claude): (Vec<_>, Vec<_>) = load().into_iter().partition(|a| a.0.codex);
        by_agent = [
            ("claude", ranked(claude, now)),
            ("codex", ranked(codex, now)),
        ];
        let Some(w) = &waiting else { break };
        match wait_state(w, &by_agent) {
            Err(why) => return out::fail(&why),
            Ok(None) => break,
            Ok(Some(t)) => {
                let whom = match &w.profile {
                    Some(p) => paths::shell_path(p),
                    None => format!("a {} account", agent_name(w.agent)),
                };
                // A little past the reset; when that has gone by on this clock and the
                // numbers still say no (the clocks differ), a minute between looks.
                let sleep = if t <= now {
                    60.0
                } else {
                    (t - now + 2.0).min(LONGEST_SLEEP)
                };
                out::eprint(&format!(
                    "No room yet; waiting for {whom} until {} (in {})…\n",
                    times::clock(now + sleep, now),
                    times::duration(sleep)
                ));
                std::thread::sleep(std::time::Duration::from_secs_f64(sleep));
                now = times::now();
            }
        }
    }
    if let Some(prefix) = &session {
        return carry(prefix, &by_agent, now, me);
    }
    for (agent, accounts) in &by_agent {
        if accounts.is_empty() {
            continue;
        }
        let title = if *agent == "claude" {
            "Claude Code, best first"
        } else {
            "Codex, best first"
        };
        let note = if *agent == "claude" {
            p("2", " (sorted by Fable, as hatcheck usage sorts)")
        } else {
            String::new()
        };
        out::println(&(p("1", title) + &note));
        let rows: Vec<(String, String, String, String, bool)> = accounts
            .iter()
            .enumerate()
            .map(|(n, (key, room, entry, result))| {
                let names = entry
                    .profiles
                    .iter()
                    .map(|pr| paths::shell_path(pr))
                    .collect::<Vec<_>>()
                    .join(", ");
                let number = if key.0 < NO_DATA {
                    format!("{}.", n + 1)
                } else {
                    " -".into()
                };
                (
                    number,
                    names,
                    summary(result, now, *room),
                    paths::start_command(agent, &entry.profiles[0], false),
                    *room,
                )
            })
            .collect();
        let width = rows.iter().map(|r| r.1.chars().count()).max().unwrap_or(0);
        let text_width = rows.iter().map(|r| r.2.chars().count()).max().unwrap_or(0);
        for (number, names, text, command, room) in &rows {
            out::println(&format!(
                "  {:>3} {}  {}  {}",
                number,
                p(if *room { "1" } else { "2" }, &format!("{names:<width$}")),
                p(
                    if *room { "" } else { "2" },
                    &format!("{text:<text_width$}")
                ),
                p("36", &format!("→ {command}"))
            ));
        }
    }
    out::println(&p("2", &format!("Advice only: nothing is started, moved or switched. {me} best --for <session> says where to carry a session.")));
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn fable(used: f64, days: f64, now: f64) -> Value {
        json!({"limits": [
            {"kind": "session", "group": "session", "percent": 20, "resets_at": times::iso_utc(now + 3600.0).unwrap()},
            {"kind": "weekly_scoped", "group": "weekly", "percent": used,
             "resets_at": times::iso_utc(now + days * 86400.0).unwrap(), "scope": {"model": {"display_name": "Fable"}}}]})
    }

    #[test]
    fn bands_rank_and_room() {
        let now = 1_800_000_000.0;
        let (fresh, _) = rank(Some(&fable(50.0, 1.0, now)), None, now);
        let (low, low_room) = rank(Some(&fable(95.0, 0.5, now)), None, now);
        let (out, out_room) = rank(Some(&fable(100.0, 0.2, now)), None, now);
        // Nearly out goes behind fresh even though it resets sooner, and out behind both.
        assert!(fresh < low && low < out);
        assert!(low_room && !out_room);
        assert_eq!(rank(None, None, now).0 .0, NO_DATA);
    }

    #[test]
    fn summaries_say_the_pace() {
        let now = 1_800_000_000.0;
        // 40% left with 2 days to go: 0.8% an hour.
        let text = summary(&(Some(fable(60.0, 2.0, now)), None, None), now, true);
        assert!(text.starts_with("Fable 40% left, resets "), "{text}");
        assert!(text.contains(", 0.8%/h · 5h 80%"), "{text}");
        let out = summary(&(Some(fable(100.0, 0.5, now)), None, None), now, false);
        assert!(
            out.starts_with("Fable 0% left, back ") && !out.contains("%/h"),
            "{out}"
        );
    }

    #[test]
    fn when_an_account_is_back() {
        let now = 1_800_000_000.0;
        let entry = Entry {
            codex: false,
            profiles: vec!["/x/.claude-a".into()],
            email: Value::Null,
            account: Value::Null,
            cache: Value::Null,
            login: Value::Null,
        };
        let spent = (Some(fable(100.0, 0.5, now)), None, None);
        let (key, room) = rank(spent.0.as_ref(), None, now);
        let back = back_at(&(key, room, entry.clone(), spent));
        assert!((back.unwrap() - (now + 0.5 * 86400.0)).abs() < 1.0);
        let fresh = (Some(fable(10.0, 1.0, now)), None, None);
        let (key, room) = rank(fresh.0.as_ref(), None, now);
        assert_eq!(back_at(&(key, room, entry, fresh)), None); // has room: nothing to wait for
    }
}
