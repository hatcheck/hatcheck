//! hatcheck best [--for SESSION] [--cached]: which account to use now, and where to carry a
//! session. Advice only: it prints commands and never starts, moves or switches anything.
//! The README's `hatcheck best` section says how accounts are ranked.

use crate::args::Parser;
use crate::facts::Summary;
use crate::find;
use crate::out::{self, Paint};
use crate::paths;
use crate::py::{self, get, s};
use crate::sessions::session_command;
use crate::times;
use crate::usage::{self, Entry, Usage};
use crate::widths::truncate;
use serde_json::Value;

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

/// Sort key and state: (group, time or -% left, reset), and whether it has room now. Groups:
/// 0 Fable left, 1 out of Fable, 2 no Fable limit, 3 no data.
fn rank(usage: Option<&Value>, saved_at: Option<f64>, now: f64) -> ((u8, f64, f64), bool) {
    if usage.is_none() {
        return ((3, 0.0, 0.0), false);
    }
    let (group, at) = usage::fable_rank(usage, saved_at, now);
    if group < 2 {
        return ((group, at, 0.0), group == 0);
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
        return ((2, 0.0, 0.0), true);
    };
    (
        (2, -tightest.1, tightest.2.unwrap_or(f64::INFINITY)),
        tightest.1 > 0.0,
    )
}

/// What the line says about an account: its Fable limit first, then the tightest other.
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
        parts.push(format!("{name} {}% left{when}", left.trunc() as i64));
    }
    for (name, left, reset, _) in others.iter().take(if fable.is_some() { 1 } else { 2 }) {
        match reset {
            Some(r) if *left <= 0.0 && *r != 0.0 => {
                parts.push(format!("{name} used up, back {}", times::clock(*r, now)))
            }
            _ => parts.push(format!("{name} {}%", left.trunc() as i64)),
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
            if *agent == "codex" {
                "Codex"
            } else {
                "Claude Code"
            }
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

pub fn main(argv: &[String], prog: &str) -> i32 {
    let parser = Parser::new(prog, "Which account to use now, and where to carry a session. Advice only: it never starts, moves or switches anything.")
        .option(&["--for"], "session", "SESSION", "where to carry this session: the hatcheck move command for the best other account")
        .flag(&["--cached"], "cached", "don't call the APIs; use the snapshots Claude Code and Codex last saved");
    let args = match parser.parse(argv) {
        Ok(a) => a,
        Err(stop) => return stop.exit_code(),
    };
    let paint = Paint::new();
    let p = |code: &str, text: &str| paint.p(code, text);
    let now = times::now();
    let loaded = usage::load_usage(
        &paths::find_profiles(),
        args.flag("cached"),
        &paths::find_codex_homes(),
    );
    let (codex, claude): (Vec<_>, Vec<_>) = loaded.into_iter().partition(|a| a.0.codex);
    let by_agent = [
        ("claude", ranked(claude, now)),
        ("codex", ranked(codex, now)),
    ];
    let me = prog.split_whitespace().next().unwrap_or("hatcheck");
    if let Some(session) = args.value("session").filter(|s| !s.is_empty()) {
        return carry(session, &by_agent, now, me);
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
                let number = if key.0 < 3 {
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
