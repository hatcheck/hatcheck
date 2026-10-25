//! hatcheck best [--for SESSION] [--cached] [--wait [PROFILE]]: the best account now, shown as
//! hatcheck usage shows it, and the hatcheck move command that carries a session to it: --for's,
//! or the newest started from this folder. hatcheck move with nothing goes by the same pick.
//! Advice only: it prints commands and never starts, moves or switches anything. --wait sleeps
//! until an account has room, then says which. docs/commands.md's `hatcheck best` section says
//! how accounts are ranked.

use crate::agent::Agent;
use crate::args::Parser;
use crate::facts::Summary;
use crate::find;
use crate::ospath;
use crate::out::{self, Paint};
use crate::paths;
use crate::py::{self, get, s, truthy};
use crate::sessions::session_command;
use crate::times;
use crate::usage::{self, Entry, Usage, NO_FABLE};
use crate::widths::truncate;
use serde_json::{json, Value};

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

fn is_fable(m: &Meter) -> bool {
    s(&m.3) == Some("weekly_scoped") && m.0.to_lowercase().contains("fable")
}

/// The limits an account needs room in to be used: every one that isn't per model, and its
/// Fable limit; for an account whose limits are all per model, as agy's are, all of them.
fn needed(rows: &[Meter]) -> Vec<&Meter> {
    let wanted: Vec<&Meter> = rows
        .iter()
        .filter(|m| !scoped(&m.3) || is_fable(m))
        .collect();
    if wanted.is_empty() {
        rows.iter().collect()
    } else {
        wanted
    }
}

/// Sort key and state: (band, time or -% left, reset), and whether it has room now, in every
/// limit it needs. The bands are usage's Fable ones, then NO_FABLE, ranked by the tightest
/// limit, then NO_DATA, for no usage or none that reads: unknown, so never room.
fn rank(usage: Option<&Value>, saved_at: Option<f64>, now: f64) -> ((u8, f64, f64), bool) {
    let rows = meters(usage, saved_at, now);
    if rows.is_empty() {
        return ((NO_DATA, 0.0, 0.0), false);
    }
    let needed = needed(&rows);
    let room = needed.iter().all(|m| m.1 > 0.0);
    let (band, at) = usage::fable_rank(usage, saved_at, now);
    if band < NO_FABLE {
        return ((band, at, 0.0), room);
    }
    let key = |m: &&Meter| (m.1, m.2.unwrap_or(f64::INFINITY));
    let tightest = needed
        .iter()
        .min_by(|a, b| {
            key(a)
                .partial_cmp(&key(b))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .expect("an account with limits needs one");
    (
        (NO_FABLE, -tightest.1, tightest.2.unwrap_or(f64::INFINITY)),
        room,
    )
}

/// ", 0.9%/h": what's left of a meter per hour until it resets, or "" without a reset ahead.
fn pace_part(left: f64, reset: Option<f64>, now: f64) -> String {
    usage::pace(left, reset, now)
        .filter(|_| left > 0.0)
        .map_or(String::new(), |p| format!(", {}", usage::pace_text(p)))
}

/// What the line says about an account: its Fable limit first, with its pace, then the
/// tightest other; for an account whose limits are all per model, as agy's are, the tightest
/// of those.
pub fn summary(result: &Usage, now: f64, room: bool) -> String {
    let (usage, saved_at, why) = result;
    let Some(u) = usage else {
        return format!("no data ({})", why.clone().unwrap_or_else(|| "None".into()));
    };
    let rows = meters(Some(u), *saved_at, now);
    let fable = rows
        .iter()
        .find(|m| s(&m.3) == Some("weekly_scoped") && m.0.to_lowercase().contains("fable"));
    let mut others: Vec<&Meter> = rows.iter().filter(|m| !scoped(&m.3)).collect();
    if others.is_empty() && fable.is_none() {
        others = rows.iter().collect();
    }
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
    // With Fable left and no room, another limit it counts against is used up: which one.
    if let Some(blocking) = fable
        .filter(|f| !room && f.1 > 0.0)
        .and_then(|_| others.iter().find(|m| m.1 <= 0.0))
    {
        parts.push(if s(&blocking.3) == Some("weekly_all") {
            "(the weekly limit blocks Fable)".into()
        } else {
            format!("(the {} limit blocks Fable)", blocking.0)
        });
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
pub type Ranked = ((u8, f64, f64), bool, Entry, Usage);

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

/// When an account without room will have some again, as far as its numbers say: once every
/// limit it needs that's used up is back. None when one of them doesn't say when.
fn back_at(account: &Ranked, now: f64) -> Option<f64> {
    if account.1 {
        return None;
    }
    let (usage, saved_at, _) = &account.3;
    let rows = meters(usage.as_ref(), *saved_at, now);
    let mut back = None::<f64>;
    for m in needed(&rows).into_iter().filter(|m| m.1 <= 0.0) {
        let t = m.2.filter(|t| *t != 0.0 && t.is_finite())?;
        back = Some(back.map_or(t, |b| b.max(t)));
    }
    back
}

/// How hatcheck move names a profile or home: default, NAME, codex, codex:NAME, or a path.
fn place_name(agent: Agent, profile: &str) -> String {
    let home_dir = paths::home();
    let home = crate::ospath::join(&home_dir, agent.base());
    if paths::same_folder(profile, &home) {
        return match agent {
            Agent::Claude => "default",
            other => other.key(),
        }
        .into();
    }
    let trimmed = profile.trim_end_matches(['/', crate::ospath::SEP.chars().next().unwrap_or('/')]);
    let (parent, base) = crate::ospath::split(trimmed);
    let prefix = format!("{}-", crate::ospath::basename(&home));
    if paths::same_folder(&parent, &home_dir) {
        if let Some(name) = base.strip_prefix(&prefix) {
            return match agent {
                Agent::Claude => name.to_string(),
                other => format!("{}:{name}", other.key()),
            };
        }
    }
    profile.to_string()
}

/// The session the advice is for, named by its ID or the start of it: the copy hatcheck open
/// would resume, and how many profiles hold one.
fn one_summary(prefix: &str) -> Result<(Summary, usize), String> {
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
    let ((agent, sid), places) = &copies[0];
    let found: Vec<Summary> = places
        .iter()
        .filter_map(|(p, f)| crate::facts::summarize(*agent, p, f))
        .collect();
    crate::facts::save_cache();
    found
        .into_iter()
        .fold(None::<Summary>, |best, s| match best {
            Some(b) if (s.when, s.canonical, s.size) > (b.when, b.canonical, b.size) => Some(s),
            Some(b) => Some(b),
            None => Some(s),
        })
        .map(|s| (s, places.len()))
        .ok_or_else(|| format!("Session {} holds no conversation", py::head(sid, 8)))
}

/// One agent's accounts, best first, from what load_usage gave.
pub fn ranked_for(agent: Agent, loaded: &[(Entry, Usage)], now: f64) -> Vec<Ranked> {
    let mine: Vec<(Entry, Usage)> = loaded
        .iter()
        .filter(|a| a.0.agent == agent)
        .cloned()
        .collect();
    ranked(mine, now)
}

/// The accounts a profile or home belongs to: their places in the ranking.
pub fn holding(accounts: &[Ranked], profile: &str) -> Vec<usize> {
    accounts
        .iter()
        .enumerate()
        .filter(|(_, a)| a.2.profiles.iter().any(|p| paths::same_folder(p, profile)))
        .map(|(k, _)| k)
        .collect()
}

/// Where a session should go, or that it's in the right place already.
pub enum Pick {
    /// Its own account is the best one with room: its place in the ranking.
    Own(usize),
    /// Another account is: its place in the ranking.
    Other(usize),
    /// No account has room.
    Nothing,
}

/// Which account a session should go to, its own being the accounts at `mine` (none for no
/// session): the best with room other than its own, unless its own has room and ranks ahead.
pub fn pick(accounts: &[Ranked], mine: &[usize]) -> Pick {
    let other = accounts
        .iter()
        .enumerate()
        .position(|(k, a)| a.1 && !mine.contains(&k));
    let own = accounts
        .iter()
        .enumerate()
        .position(|(k, a)| a.1 && mine.contains(&k));
    match (own, other) {
        (Some(k), Some(b)) if k < b => Pick::Own(k),
        (Some(k), None) => Pick::Own(k),
        (_, Some(b)) => Pick::Other(b),
        (None, None) => Pick::Nothing,
    }
}

/// Print an account as hatcheck usage shows it, its heading after `lead`: the profiles, the
/// plan and whether the numbers are live, then one row per limit with its bars.
pub fn print_account(paint: &Paint, lead: &str, account: &Ranked, now: f64) {
    let (_, _, entry, result) = account;
    out::println(&format!(
        "{lead}{}",
        usage::account_header(paint, entry, result, now, false)
    ));
    let (body, saved_at, _) = result;
    let rows = body
        .as_ref()
        .filter(|u| truthy(u))
        .map_or_else(Vec::new, usage::usage_rows);
    let width = rows
        .iter()
        .map(|r| r.label.chars().count())
        .max()
        .unwrap_or(0);
    let (lines, _) = usage::account_lines(paint, &rows, *saved_at, now, width);
    for line in &lines {
        out::println(line);
    }
    let extra = usage::extra_usage_text(body.as_ref().unwrap_or(&json!({})));
    if !extra.is_empty() {
        out::println(&format!("  {}", paint.p("2", &extra)));
    }
}

/// How hatcheck move names a profile of agent for a session of session_agent: a bare NAME
/// follows the session's own agent, so Claude Code's is spelled out as claude or claude:NAME
/// for a Codex or Kimi session.
fn spelled(agent: Agent, session_agent: Agent, profile: &str) -> String {
    let name = place_name(agent, profile);
    if agent != Agent::Claude || session_agent == Agent::Claude || !session_agent.has_profiles() {
        return name;
    }
    if name == "default" {
        "claude".into()
    } else if name.contains('/') || name.contains(ospath::SEP) || name.starts_with('~') {
        name
    } else {
        format!("claude:{name}")
    }
}

/// The ranking's lines, an account each: its place, profiles, what its limits say, and the
/// command that starts the agent there, aligned; `skip` is the one shown in full above.
fn print_ranking(
    paint: &Paint,
    agent: Agent,
    accounts: &[Ranked],
    skip: Option<usize>,
    mine: &[usize],
    now: f64,
) {
    let p = |code: &str, text: &str| paint.p(code, text);
    let rows: Vec<(String, String, String, String, bool)> = accounts
        .iter()
        .enumerate()
        .filter(|(k, _)| Some(*k) != skip)
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
            let mut text = summary(result, now, *room);
            if mine.contains(&n) {
                text += " (where it is)";
            }
            (
                number,
                names,
                text,
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

/// Whom --wait waits for: an agent's accounts, or one profile or home of it.
struct Waiting {
    agent: Agent,
    profile: Option<String>,
}

/// Whether what --wait waits for has room now; when it hasn't, when to look again, or the
/// reason nothing can be waited for.
fn wait_state(
    w: &Waiting,
    by_agent: &[(Agent, Vec<Ranked>)],
    now: f64,
) -> Result<Option<f64>, String> {
    let accounts = &by_agent
        .iter()
        .find(|(a, _)| *a == w.agent)
        .expect("every agent is ranked")
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
            None => format!("No {} profile found", w.agent.name()),
        });
    }
    if relevant.iter().any(|a| a.1) {
        return Ok(None); // has room
    }
    let wake = relevant
        .iter()
        .filter_map(|a| back_at(a, now))
        .fold(None::<f64>, |m, t| Some(m.map_or(t, |m| m.min(t))));
    match wake {
        Some(t) => Ok(Some(t)),
        None => Err(format!(
            "No {} account has room, and none says when it will again",
            w.agent.name()
        )),
    }
}

pub fn main(argv: &[String], prog: &str) -> i32 {
    let parser = Parser::new(prog, "The best account now, shown as hatcheck usage shows it, and the hatcheck move command that carries a session to it: the newest started from this folder, or --for's. Advice only: it never starts, moves or switches anything.")
        .option(&["--for"], "session", "SESSION", "the session to carry: its ID, or the start of it (default: the newest session started from this folder)")
        .flag(&["--cached"], "cached", "don't call the APIs; use the snapshots Claude Code and Codex last saved, and hatcheck's last readings of agy's /usage and Kimi's usage")
        .maybe(&["--wait"], "wait", "PROFILE", "", "wait until an account has room, then answer: any account of the session's agent (Claude Code's, without a session), or PROFILE's (NAME, default, codex, codex:NAME, agy, kimi, kimi:NAME, dsh, dsh:NAME, mcode, mcode:NAME, qwen, qwen:NAME, copilot, copilot:NAME, opencode)");
    let args = match parser.parse(argv) {
        Ok(a) => a,
        Err(stop) => return stop.exit_code(),
    };
    let me = prog.split_whitespace().next().unwrap_or("hatcheck");
    // The session the advice is for: --for's, or the newest started from this folder, as
    // hatcheck move with nothing takes it; and how many profiles hold a copy.
    let named = args.value("session").filter(|s| !s.is_empty());
    let (subject, copies) = match named {
        Some(prefix) => match one_summary(prefix) {
            Ok((s, n)) => (Some(s), n),
            Err(why) => return out::fail(&why),
        },
        None => (find::newest_here(&ospath::cwd(), false), 1),
    };
    // Whose accounts are ranked: the session's agent's, or Claude Code's for an agent without
    // profiles (agy, zcode), whose session can only be converted.
    let agent = subject.as_ref().map_or(Agent::Claude, |s| {
        if s.agent.has_profiles() {
            s.agent
        } else {
            Agent::Claude
        }
    });
    let waiting = match args.value("wait") {
        None => None,
        Some(spec) if !spec.is_empty() => {
            let (agent, profile) = find::place(spec);
            Some(Waiting {
                agent,
                profile: Some(profile),
            })
        }
        Some(_) => Some(Waiting {
            agent,
            profile: None,
        }),
    };
    let load = || {
        usage::load_usage(
            &paths::find_profiles(),
            args.flag("cached"),
            &paths::find_codex_homes(),
            &paths::find_agy_roots(),
            &paths::find_kimi_homes(),
            &paths::find_zcode_roots(),
            &paths::find_dsh_homes(),
            &paths::find_mcode_homes(),
            &paths::find_qwen_homes(),
            &paths::find_copilot_homes(),
            &paths::find_opencode_homes(),
        )
    };
    let mut now = times::now();
    let mut by_agent: Vec<(Agent, Vec<Ranked>)>;
    loop {
        let loaded = load();
        by_agent = Agent::ALL
            .iter()
            .map(|&agent| (agent, ranked_for(agent, &loaded, now)))
            .collect();
        let Some(w) = &waiting else { break };
        match wait_state(w, &by_agent, now) {
            Err(why) => return out::fail(&why),
            Ok(None) => break,
            Ok(Some(t)) => {
                let whom = match &w.profile {
                    Some(p) => paths::shell_path(p),
                    None => format!("a {} account", w.agent.name()),
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
    let paint = Paint::new();
    let p = |code: &str, text: &str| paint.p(code, text);
    let ranking = |a: Agent| {
        &by_agent
            .iter()
            .find(|(b, _)| *b == a)
            .expect("every agent is ranked")
            .1
    };
    let accounts = ranking(agent);
    if accounts.is_empty() {
        return out::fail(&format!("No {} profile found", agent.name()));
    }
    let mine: Vec<usize> = subject
        .as_ref()
        .map_or_else(Vec::new, |s| holding(accounts, &s.profile));
    if let Some(s) = &subject {
        // Where it is and what that account has left, from its own agent's ranking.
        let own = ranking(s.agent);
        let state = holding(own, &s.profile).first().map_or_else(
            || "no usage data".to_string(),
            |&k| summary(&own[k].3, now, own[k].1),
        );
        out::println(&format!(
            "Session {} \"{}\"{} is in {}: {state}.",
            py::head(&s.id, 8),
            truncate(&s.title, 60),
            if named.is_some() {
                ""
            } else {
                ", the newest started from this folder,"
            },
            paths::shell_path(&s.profile)
        ));
    }
    let lead = paint.accent(&format!("Best {} account now: ", agent.name()));
    let mut exit = 0;
    let featured = match pick(accounts, &mine) {
        Pick::Own(k) => {
            print_account(&paint, &lead, &accounts[k], now);
            let s = subject.as_ref().expect("an own account needs a session");
            out::println("Its own account is the best one now; resume it there with:");
            if let Some(cwd) = s.cwd.as_deref().filter(|c| !c.is_empty()) {
                out::command(&format!("  cd {}", paths::shell_path(cwd)));
            }
            out::command(&format!("  {}", session_command(s, false)));
            Some(k)
        }
        Pick::Other(b) => {
            print_account(&paint, &lead, &accounts[b], now);
            let folder = &accounts[b].2.profiles[0];
            match &subject {
                Some(s) => {
                    let dest = spelled(agent, s.agent, folder);
                    let sid = if named.is_some() {
                        format!("{} ", py::head(&s.id, 8))
                    } else {
                        String::new()
                    };
                    let from = if named.is_some() && copies > 1 {
                        format!(" --from {}", place_name(s.agent, &s.profile))
                    } else {
                        String::new()
                    };
                    let keeps = if s.agent == agent {
                        p(
                            "2",
                            &format!("    ({me} fork {sid}{dest}{from} keeps the original)"),
                        )
                    } else {
                        String::new()
                    };
                    out::println("Carry it there with:");
                    out::println(&format!(
                        "{}{keeps}",
                        paint.command(&format!("  {me} move {sid}{dest}{from}"))
                    ));
                }
                None => {
                    out::println("Start there with:");
                    out::command(&format!("  {}", paths::start_command(agent, folder, false)));
                    out::println("Carry a session there with:");
                    out::command(&format!(
                        "  {me} move <session> {}",
                        place_name(agent, folder)
                    ));
                }
            }
            Some(b)
        }
        Pick::Nothing => {
            out::warn(&format!(
                "No {}{} account has room now:",
                if mine.is_empty() { "" } else { "other " },
                agent.name()
            ));
            exit = 1;
            None
        }
    };
    if accounts.len() > usize::from(featured.is_some()) {
        if featured.is_some() {
            out::println(&p("2", "Then:"));
        }
        print_ranking(&paint, agent, accounts, featured, &mine, now);
    }
    for (other, theirs) in &by_agent {
        if *other == agent {
            continue;
        }
        let Some((key, room, entry, result)) = theirs.first() else {
            continue;
        };
        if key.0 == NO_DATA {
            continue;
        }
        let names = entry
            .profiles
            .iter()
            .map(|pr| paths::shell_path(pr))
            .collect::<Vec<_>>()
            .join(", ");
        out::println(&format!(
            "{} {}  {}  {}",
            p("2", &format!("{}:", other.name())),
            p(if *room { "1" } else { "2" }, &names),
            p(if *room { "" } else { "2" }, &summary(result, now, *room)),
            p(
                "36",
                &format!(
                    "→ {}",
                    paths::start_command(*other, &entry.profiles[0], false)
                )
            )
        ));
    }
    out::println(&p("2", &format!("Advice only: nothing is started, moved or switched. {me} usage shows every account's limits.")));
    exit
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
    fn room_needs_every_limit_it_counts_against() {
        let now = 1_800_000_000.0;
        let at = |hours: f64| times::iso_utc(now + hours * 3600.0).unwrap();
        // Fable mostly left, the 5-hour limit used up: ranked by Fable still, but no room.
        let five_hours_out = json!({"limits": [
            {"kind": "session", "group": "session", "percent": 100, "resets_at": at(1.0)},
            {"kind": "weekly_scoped", "group": "weekly", "percent": 20, "resets_at": at(72.0),
             "scope": {"model": {"display_name": "Fable"}}}]});
        let (key, room) = rank(Some(&five_hours_out), None, now);
        assert_eq!((key.0, room), (usage::FABLE_FRESH, false));
        // Nothing hatcheck can read: no data, never room.
        let (key, room) = rank(Some(&json!({})), None, now);
        assert_eq!((key.0, room), (NO_DATA, false));
        // agy's limits are all per model: ranked by the tightest, with room while it has some.
        let agy = |used: i64| {
            json!({"limits": [{"kind": "quota_scoped", "group": "quota", "label": "Gemini 4",
                               "short": "Gemini 4", "percent": used, "resets_at": at(5.0)}]})
        };
        assert!(rank(Some(&agy(40)), None, now).1);
        let (key, room) = rank(Some(&agy(100)), None, now);
        assert_eq!((key.0, room), (NO_FABLE, false));
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
            agent: Agent::Claude,
            profiles: vec!["/x/.claude-a".into()],
            email: Value::Null,
            account: Value::Null,
            cache: Value::Null,
            login: Value::Null,
        };
        let spent = (Some(fable(100.0, 0.5, now)), None, None);
        let (key, room) = rank(spent.0.as_ref(), None, now);
        let back = back_at(&(key, room, entry.clone(), spent), now);
        assert!((back.unwrap() - (now + 0.5 * 86400.0)).abs() < 1.0);
        let fresh = (Some(fable(10.0, 1.0, now)), None, None);
        let (key, room) = rank(fresh.0.as_ref(), None, now);
        assert_eq!(back_at(&(key, room, entry.clone(), fresh), now), None); // has room: nothing to wait for
                                                                            // Fable left, the 5-hour limit used up: back when that resets.
        let mut five_hours_out = fable(10.0, 3.0, now);
        five_hours_out["limits"][0]["percent"] = json!(100);
        let five_hours_out = (Some(five_hours_out), None, None);
        let (key, room) = rank(five_hours_out.0.as_ref(), None, now);
        let back = back_at(&(key, room, entry, five_hours_out), now);
        assert!((back.unwrap() - (now + 3600.0)).abs() < 1.0);
    }
}
