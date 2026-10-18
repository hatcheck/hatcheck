//! hatcheck [sessions] [HOURS] ...: recent Claude Code sessions from every ~/.claude*
//! profile and Codex sessions from every ~/.codex* home, grouped by folder, each with the
//! command that resumes it and what's left of its account's plan. The README's `hatcheck`
//! section describes the list.

use crate::args::{Count, Parser};
use crate::facts::{self, Summary};
use crate::ospath::{self, Part};
use crate::out::{self, Paint};
use crate::paths;
use crate::py::{self, get, s, truthy};
use crate::times;
use crate::usage::{self, Entry, Usage};
use crate::widths::truncate;
use serde_json::{json, Map, Value};
use std::collections::HashMap;

const DEFAULT_HOURS: i64 = 24;

/// How far back to look: 24, 1.5, 12h or 3d, in hours.
pub fn parse_hours(text: &str) -> Option<f64> {
    let t = py::strip(text).to_lowercase();
    let (number, unit) = match t.strip_suffix('d') {
        Some(n) => (n, 'd'),
        None => (t.strip_suffix('h').unwrap_or(&t), 'h'),
    };
    let (whole, frac) = number.split_once('.').unwrap_or((number, ""));
    let digits = |p: &str| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit());
    if !digits(whole) || (number.contains('.') && !digits(frac)) {
        return None;
    }
    let value: f64 = number.parse().ok()?;
    Some(if unit == 'd' { value * 24.0 } else { value })
}

/// (profile, session ID) -> Claude Code's registry entry, for sessions whose claude process
/// is alive. Claude Code writes <profile>/sessions/<pid>.json with when the process started,
/// which guards against reused PIDs: as `ps -o lstart` prints it in UTC (procStart), or on
/// Windows as a FILETIME number (procStartFt).
pub fn open_sessions(profiles: &[String]) -> HashMap<(String, String), Value> {
    let mut started: HashMap<String, String> = HashMap::new();
    if !ospath::WINDOWS {
        let out = std::process::Command::new("ps")
            .args(["-A", "-o", "pid=,lstart="])
            .env("TZ", "UTC")
            .env("LC_ALL", "C")
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
            .unwrap_or_default();
        for line in out.lines() {
            let line = py::strip(line);
            let (pid, lstart) = line.split_once(' ').unwrap_or((line, ""));
            started.insert(pid.to_string(), py::collapse(lstart));
        }
    }
    let mut live = HashMap::new();
    for profile in profiles {
        for path in ospath::glob(profile, &[Part::Name("sessions"), Part::Wild("", ".json")]) {
            let Some(entry) = std::fs::read_to_string(&path)
                .ok()
                .and_then(|t| serde_json::from_str::<Value>(&t).ok())
            else {
                continue;
            };
            let pid = py::str_of(get(&entry, "pid"));
            if !truthy(get(&entry, "sessionId")) {
                continue;
            }
            let alive = if ospath::WINDOWS {
                let ft = pid
                    .parse::<u32>()
                    .ok()
                    .filter(|_| pid.bytes().all(|b| b.is_ascii_digit()))
                    .and_then(windows_start_time);
                match ft {
                    Some(ft) => {
                        let recorded = get(&entry, "procStartFt");
                        let recorded = if truthy(recorded) {
                            py::str_of(recorded)
                        } else {
                            ft.to_string()
                        };
                        recorded == ft.to_string()
                    }
                    None => false,
                }
            } else {
                let recorded = get(&entry, "procStart");
                let proc_start = py::collapse(&if truthy(recorded) {
                    py::str_of(recorded)
                } else {
                    String::new()
                });
                started
                    .get(&pid)
                    .is_some_and(|s| proc_start.is_empty() || proc_start == *s)
            };
            if alive {
                live.insert(
                    (
                        ospath::realpath(profile),
                        py::str_of(get(&entry, "sessionId")),
                    ),
                    entry,
                );
            }
        }
    }
    live
}

/// When a Windows process started, as the FILETIME number Claude Code records (procStartFt),
/// or None when it isn't running.
#[cfg(windows)]
pub fn windows_start_time(pid: u32) -> Option<u64> {
    use windows_sys::Win32::Foundation::{CloseHandle, FILETIME};
    use windows_sys::Win32::System::Threading::{
        GetExitCodeProcess, GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return None;
        }
        let zero = FILETIME {
            dwLowDateTime: 0,
            dwHighDateTime: 0,
        };
        let (mut created, mut exited, mut kernel, mut user) = (zero, zero, zero, zero);
        let mut code: u32 = 0;
        let ok = GetProcessTimes(handle, &mut created, &mut exited, &mut kernel, &mut user) != 0;
        let running = GetExitCodeProcess(handle, &mut code) == 0 || code == 259; // STILL_ACTIVE
        CloseHandle(handle);
        (ok && running)
            .then(|| (u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime))
    }
}

#[cfg(not(windows))]
pub fn windows_start_time(_pid: u32) -> Option<u64> {
    None
}

/// The command that resumes a listed session, Claude Code's or Codex's.
pub fn session_command(s: &Summary, full: bool) -> String {
    if s.agent == "codex" {
        return paths::codex_resume_command(&s.profile, &s.id, full);
    }
    paths::resume_command(&s.profile, &s.id, full, paths::provider_of(py::s(&s.model)))
}

/// profile -> text: each plan limit's short name (per-model ones in violet), % left and a bar
/// of it, then ↻, a bar of the time left until it resets and that time, coloured by urgency,
/// in columns aligned across profiles.
pub fn usage_cells(
    results: &[(String, Usage)],
    now: f64,
    paint: &Paint,
) -> HashMap<String, String> {
    type Meter = (
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
    );
    let mut meters: Vec<(String, Vec<Meter>)> = Vec::new();
    for (profile, (usage, saved_at, _)) in results {
        let mut rows: Vec<Meter> = Vec::new();
        let limits = usage.as_ref().map(|u| get(u, "limits"));
        if let Some(Value::Array(limits)) = limits {
            for row in limits.iter().filter(|r| r.is_object()) {
                let left =
                    usage::percent_left(get(row, "percent"), get(row, "resets_at"), *saved_at, now);
                let reset = s(get(row, "resets_at")).and_then(times::parse_time);
                let window = usage::row_window(row);
                let window_n = py::Num::of(&window)
                    .filter(|_| truthy(&window))
                    .map(py::Num::f);
                let blocks = usage::bar_cells(&window);
                let (pct, code, plan, mut until, mut until_code);
                match left {
                    None => {
                        // reset since the snapshot
                        pct = "?".to_string();
                        code = "2".to_string();
                        plan = "·".repeat(blocks);
                        until = String::new();
                        until_code = String::new();
                    }
                    Some(left) => {
                        pct = format!("{}%", left.d());
                        code = usage::severity_color(get(row, "severity"))
                            .unwrap_or("")
                            .to_string();
                        plan = usage::bar(left.f() / 100.0, blocks);
                        until = String::new();
                        until_code = String::new();
                        if let Some(reset) = reset {
                            let to_go = window_n.map(|w| (reset - now) / w);
                            let time_bar =
                                to_go.map_or(String::new(), |f| usage::bar(f, blocks) + " ");
                            until = format!("↻{time_bar}{}", times::duration(reset - now));
                            until_code = match to_go {
                                Some(f) => usage::urgency_color(f, paint.wide),
                                None => "2".into(),
                            };
                        }
                    }
                }
                // A per-model limit's bar is violet like its name; its % keeps the severity's colour.
                let kind = get(row, "kind");
                rows.push((
                    usage::row_label(row, true),
                    usage::label_color(kind, "2", paint.wide),
                    pct,
                    code.clone(),
                    plan,
                    usage::label_color(kind, &code, paint.wide),
                    until,
                    until_code,
                ));
            }
        }
        meters.push((profile.clone(), rows));
    }
    let mut widths: Vec<[usize; 4]> = Vec::new(); // meter slot -> widest [label, % left, plan bar, time bar]
    for (_, rows) in &meters {
        for (i, m) in rows.iter().enumerate() {
            if widths.len() <= i {
                widths.push([0; 4]);
            }
            let w = &mut widths[i];
            for (k, text) in [&m.0, &m.2, &m.4, &m.6].into_iter().enumerate() {
                w[k] = w[k].max(text.chars().count());
            }
        }
    }
    let padded = |code: &str, text: &str, width: usize, right: bool| {
        let gap = " ".repeat(width.saturating_sub(text.chars().count()));
        let text = if text.is_empty() {
            String::new()
        } else {
            paint.p(code, text)
        };
        if right {
            gap + &text
        } else {
            text + &gap
        }
    };
    let mut cells = HashMap::new();
    for (profile, rows) in meters {
        let parts: Vec<String> = rows
            .iter()
            .enumerate()
            .map(
                |(i, (label, label_code, pct, code, plan, plan_code, until, until_code))| {
                    let [lw, pw, bw, uw] = widths[i];
                    // Padded so the next meter lines up; the trailing gap is cut below.
                    [
                        padded(label_code, label, lw, false),
                        padded(code, pct, pw, true),
                        padded(plan_code, plan, bw, false),
                        padded(until_code, until, uw, false),
                    ]
                    .join(" ")
                },
            )
            .collect();
        let text = if parts.is_empty() {
            paint.p("2", "usage ?")
        } else {
            parts.join("  ").trim_end_matches(py::is_space).to_string()
        };
        cells.insert(profile, text);
    }
    cells
}

/// The sessions found, keyed as the Python version keyed them, in the order first found.
struct Found {
    order: Vec<String>,
    by_key: HashMap<String, ((f64, bool, u64), Summary)>,
}

impl Found {
    fn put(&mut self, key: String, rank: (f64, bool, u64), s: Summary, only_if_better: bool) {
        match self.by_key.get(&key) {
            Some((old, _))
                if only_if_better
                    && !(rank.0 > old.0
                        || (rank.0 == old.0 && (rank.1, rank.2) > (old.1, old.2))) => {}
            Some(_) => {
                self.by_key.insert(key, (rank, s));
            }
            None => {
                self.order.push(key.clone());
                self.by_key.insert(key, (rank, s));
            }
        }
    }
}

pub fn main(argv: &[String], prog: &str) -> i32 {
    let parser = Parser::new(prog, "List recent Claude Code CLI sessions across all ~/.claude* profiles, and Codex CLI sessions across all ~/.codex* homes, with the command that resumes each one.")
        .positional("hours", Count::Optional, &format!("how far back to look (default {DEFAULT_HOURS}); accepts 36, 1.5, 12h or 3d"))
        .option(&["-n", "--per-folder"], "per_folder", "N", "show only the newest N sessions per folder (default: all)")
        .flag(&["--all"], "all", "also list sessions last used from Claude Desktop or the SDK, and Codex sessions from the Codex app, its IDE extension or codex exec")
        .flag(&["--all-folders"], "all_folders", "list every folder, even when run from a folder that has sessions")
        .flag(&["--full"], "full", "print CLAUDE_CONFIG_DIR=... claude instead of the cl shortcut, and CODEX_HOME=... codex instead of cx")
        .flag(&["--cached"], "cached", "take plan usage from the snapshots Claude Code and Codex last saved instead of the APIs")
        .flag(&["--no-usage"], "no_usage", "leave plan usage out")
        .flag(&["--json"], "json", "print the sessions, and the accounts behind them, as JSON for scripts and apps")
        .flag(&["-e", "--email"], "email", "with --json, include each account's email address");
    let args = match parser.parse(argv) {
        Ok(a) => a,
        Err(stop) => return stop.exit_code(),
    };
    let hours_value: Value = match args.value("hours") {
        None => json!(DEFAULT_HOURS),
        Some(text) => match parse_hours(text) {
            Some(h) => json!(h),
            None => {
                return parser
                    .error("argument hours: expected hours like 24, 1.5, 12h or 3d")
                    .exit_code()
            }
        },
    };
    let hours = py::Num::of(&hours_value).map_or(24.0, py::Num::f);
    let per_folder = match args.value("per_folder") {
        None => None,
        Some(n) => match n.parse::<usize>() {
            Ok(n) if n >= 1 && n.to_string() == n.to_string() && !n.to_string().is_empty() => {
                Some(n)
            }
            _ => {
                return parser
                    .error("argument -n/--per-folder: expected a whole number of 1 or more")
                    .exit_code()
            }
        },
    };
    let (all, full, json_out) = (args.flag("all"), args.flag("full"), args.flag("json"));
    let paint = Paint::new();
    let p = |code: &str, text: &str| paint.p(code, text);
    let width = out::columns(100);

    let now = times::now();
    let cutoff = now - hours * 3600.0;
    let profiles = paths::find_profiles();
    let live = open_sessions(&profiles);

    let mut found = Found {
        order: Vec::new(),
        by_key: HashMap::new(),
    };
    let (mut empty, mut unreadable) = (0usize, 0usize);
    for profile in &profiles {
        let real = ospath::realpath(profile);
        for path in ospath::glob(
            profile,
            &[
                Part::Name("projects"),
                Part::Wild("", ""),
                Part::Wild("", ".jsonl"),
            ],
        ) {
            let name = ospath::basename(&path);
            let key = (
                real.clone(),
                name[..name.len() - ".jsonl".len()].to_string(),
            );
            let is_live = live.contains_key(&key);
            // A file's mtime is never older than its last message, so this skips old sessions
            // without reading them.
            let session = match ospath::getmtime(&path) {
                Ok(t) if t < cutoff && !is_live => continue,
                Ok(_) => facts::read_session(&path),
                Err(e) => Err(e),
            };
            let session = match session {
                Ok(s) => s,
                Err(_) => {
                    unreadable += 1;
                    continue;
                }
            };
            let Some(mut session) = session.filter(|s| s.has_messages || is_live) else {
                empty += 1;
                continue;
            };
            // A session open in a terminal is listed even if its last message is older.
            if session.when < cutoff && !is_live {
                continue;
            }
            session.profile = profile.clone();
            session.open = live.get(&key).cloned();
            let rank = (session.when, session.canonical, session.size);
            found.put(
                format!("claude\u{0}{}\u{0}{}", key.0, key.1),
                rank,
                session,
                true,
            );
        }
    }
    let codex_homes = paths::find_codex_homes();
    for home in &codex_homes {
        let mut names: Option<Map<String, Value>> = None; // read once a session in the window needs them
        for path in paths::codex_session_files(home) {
            let session = match ospath::getmtime(&path) {
                Ok(t) if t < cutoff => continue,
                Ok(_) => facts::read_codex_session(
                    &path,
                    names.get_or_insert_with(|| facts::codex_names(home)),
                ),
                Err(e) => Err(e),
            };
            let session = match session {
                Ok(s) => s,
                Err(_) => {
                    unreadable += 1;
                    continue;
                }
            };
            if session
                .as_ref()
                .is_some_and(|s| s.entrypoint_text() == "subagent")
            {
                continue; // one Codex ran itself, such as an approval review
            }
            let Some(mut session) = session.filter(|s| s.has_messages) else {
                empty += 1;
                continue;
            };
            if session.when < cutoff {
                continue;
            }
            session.profile = home.clone();
            let key = format!("codex\u{0}{}\u{0}{}", ospath::realpath(home), session.id);
            found.put(key, (session.when, true, session.size), session, false);
        }
    }
    facts::save_cache();

    let mut shown: Vec<Summary> = Vec::new();
    let mut hidden: Vec<(String, usize)> = Vec::new();
    for key in &found.order {
        let s = found.by_key[key].1.clone();
        let entrypoint = s.entrypoint_text();
        if entrypoint != "cli" && !all {
            let label = if entrypoint == "claude-desktop" {
                "Desktop".to_string()
            } else {
                entrypoint
            };
            match hidden.iter_mut().find(|(k, _)| *k == label) {
                Some((_, n)) => *n += 1,
                None => hidden.push((label, 1)),
            }
            continue;
        }
        shown.push(s);
    }
    // Run from a folder that has sessions, list only that folder's.
    let here = std::env::current_dir()
        .ok()
        .map(|d| ospath::realpath(&d.to_string_lossy()));
    let mine: Vec<Summary> = shown
        .iter()
        .filter(|s| {
            s.cwd
                .as_deref()
                .is_some_and(|c| !c.is_empty() && Some(ospath::realpath(c)) == here)
        })
        .cloned()
        .collect();
    let only_here = !mine.is_empty() && !args.flag("all_folders");
    let elsewhere = if only_here {
        shown.len() - mine.len()
    } else {
        0
    };
    if only_here {
        shown = mine;
    }
    let mut per_profile: Vec<(String, usize)> = profiles
        .iter()
        .chain(&codex_homes)
        .map(|p| (p.clone(), 0))
        .collect();
    for s in &shown {
        if let Some(entry) = per_profile.iter_mut().find(|(p, _)| *p == s.profile) {
            entry.1 += 1;
        }
    }

    // Grouped by the exact path the transcript is filed under, not its realpath: an old
    // session filed under a symlinked path must be resumed from that path.
    let home = paths::home();
    let mut groups: Vec<(String, Vec<Summary>)> = Vec::new();
    for s in shown.iter().cloned() {
        let folder = s
            .cwd
            .clone()
            .filter(|c| !c.is_empty())
            .unwrap_or_else(|| home.clone());
        match groups.iter_mut().find(|(f, _)| *f == folder) {
            Some((_, list)) => list.push(s),
            None => groups.push((folder, vec![s])),
        }
    }
    groups.sort_by_key(|(f, _)| f.to_lowercase());
    let mut folders: Vec<(String, Vec<Summary>, usize)> = Vec::new();
    for (folder, mut sessions) in groups {
        sessions.sort_by(|a, b| {
            b.when
                .partial_cmp(&a.when)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let total = sessions.len();
        let listed: Vec<Summary> = sessions
            .into_iter()
            .take(per_folder.unwrap_or(usize::MAX))
            .collect();
        let older = total - listed.len();
        folders.push((folder, listed, older));
    }

    // Plan usage for the profiles behind the listed sessions, one request per account.
    let use_usage = !args.flag("no_usage");
    let used: Vec<&String> = folders
        .iter()
        .flat_map(|(_, l, _)| l.iter().map(|s| &s.profile))
        .collect();
    let used_profiles: Vec<String> = profiles
        .iter()
        .filter(|p| used.contains(p))
        .cloned()
        .collect();
    let used_homes: Vec<String> = codex_homes
        .iter()
        .filter(|h| used.contains(h))
        .cloned()
        .collect();
    let loaded: Vec<(Entry, Usage)> = if use_usage {
        usage::load_usage(&used_profiles, args.flag("cached"), &used_homes)
    } else {
        Vec::new()
    };
    let mut results: Vec<(String, Usage)> = Vec::new(); // profile or home -> its account's usage
    let mut account: HashMap<String, usize> = HashMap::new(); // and a number for that account
    for (n, (entry, result)) in loaded.iter().enumerate() {
        for profile in &entry.profiles {
            match results.iter_mut().find(|(p, _)| p == profile) {
                Some(r) => r.1 = result.clone(),
                None => results.push((profile.clone(), result.clone())),
            }
            account.insert(profile.clone(), n);
        }
    }
    let bindings = paths::load_bindings();
    for (_, listed, _) in folders.iter_mut() {
        for s in listed.iter_mut() {
            let want = paths::bound_profile(s.cwd.as_deref(), s.agent, &bindings);
            s.bound_to = want.filter(|w| !paths::same_folder(w, &s.profile));
        }
    }
    if json_out {
        let mut data = Map::new();
        data.insert("version".into(), json!(1));
        data.insert(
            "generated_at".into(),
            times::iso_utc(now).map_or(Value::Null, Value::String),
        );
        data.insert("hours".into(), hours_value.clone());
        data.insert("this_folder_only".into(), json!(only_here));
        let mut sessions = Vec::new();
        for (_, listed, _) in &folders {
            for s in listed {
                sessions.push(json!({
                    "agent": s.agent, "id": s.id, "profile": ospath::abspath(&s.profile),
                    "account": account.get(&s.profile).map_or(Value::Null, |n| json!(n)), "title": s.title, "cwd": s.cwd,
                    "last_active": times::iso_utc(s.when), "entrypoint": s.entrypoint, "branch": s.branch, "model": s.model,
                    "open": s.open.as_ref().map_or(Value::Null, |live| json!({"pid": get(live, "pid"), "status": get(live, "status")})),
                    "continued_in": s.continued_in,
                    "bound_to": s.bound_to.as_ref().map_or(Value::Null, |b| json!(ospath::abspath(b))),
                    "resume": session_command(s, full)}));
            }
        }
        data.insert("sessions".into(), Value::Array(sessions));
        data.insert(
            "accounts".into(),
            Value::Array(
                loaded
                    .iter()
                    .map(|(e, r)| usage::account_json(e, r, now, args.flag("email")))
                    .collect(),
            ),
        );
        data.insert(
            "hidden".into(),
            Value::Object(hidden.iter().map(|(k, n)| (k.clone(), json!(n))).collect()),
        );
        data.insert("other_folders".into(), json!(elsewhere));
        data.insert(
            "older_not_listed".into(),
            Value::Object(
                folders
                    .iter()
                    .filter(|f| f.2 > 0)
                    .map(|(f, _, o)| (f.clone(), json!(o)))
                    .collect(),
            ),
        );
        out::println(&py::dumps(&Value::Object(data)));
        return 0;
    }
    let cells = if results.is_empty() {
        HashMap::new()
    } else {
        usage_cells(&results, now, &paint)
    };
    // The usage comments line up after the longest resume command.
    let command_width = folders
        .iter()
        .flat_map(|(_, l, _)| l.iter().map(|s| session_command(s, full).chars().count()))
        .max()
        .unwrap_or(0);

    let window = format!("{}h", py::g(hours));
    out::println(
        &(p(
            "1",
            &format!(
                "{} CLI sessions {}active in the last {window} or open now",
                if codex_homes.is_empty() {
                    "Claude Code"
                } else {
                    "Claude Code and Codex"
                },
                if only_here { "in this folder, " } else { "" }
            ),
        ) + &p(
            "2",
            &format!(
                " ({}newest first{})",
                if only_here { "" } else { "by folder, " },
                if cells.is_empty() {
                    ""
                } else {
                    "; % = plan left, ↻ = hours/days until reset"
                }
            ),
        )),
    );
    if shown.is_empty() {
        out::println(&format!(
            "  No sessions in this window. Try a longer one, e.g. {prog} 72"
        ));
    }
    let mut previous: Option<(Option<usize>, Option<String>)> = None; // (account, usage text) of the row above
    for (folder, listed, older) in &folders {
        let missing = if ospath::isdir(folder) {
            String::new()
        } else {
            format!("  {}", p("31", "(directory missing)"))
        };
        out::println(&(p("1;36", &format!("cd {}", paths::shell_path(folder))) + &missing));
        for s in listed {
            // No profile here: the command shows it (plain `claude` means ~/.claude).
            let mut meta: Vec<(String, &str)> = Vec::new();
            if s.agent == "codex" {
                meta.push(("codex".into(), "36"));
            }
            if truthy(&s.branch) {
                meta.push((py::str_of(&s.branch), "2"));
            }
            if let Some(model) = py::s(&s.model)
                .filter(|m| s.agent != "codex" && !m.is_empty() && !m.starts_with("claude"))
            {
                meta.push((format!("on {model}"), "35")); // e.g. GLM or Kimi, through claude-z or claude-k
            }
            let entrypoint = s.entrypoint_text();
            if entrypoint != "cli" {
                meta.push((format!("via {entrypoint}"), "33"));
            }
            if let Some(entry) = s.open.as_ref().filter(|e| truthy(e)) {
                let status = get(entry, "status");
                meta.push((
                    format!(
                        "● open in pid {}{}",
                        py::str_of(get(entry, "pid")),
                        if truthy(status) {
                            format!(" ({})", py::str_of(status))
                        } else {
                            String::new()
                        }
                    ),
                    "32",
                ));
            }
            if truthy(&s.continued_in) {
                meta.push((
                    format!(
                        "continued in {}",
                        py::str_of(&s.continued_in)
                            .chars()
                            .take(8)
                            .collect::<String>()
                    ),
                    "2",
                ));
            }
            if let Some(bound) = &s.bound_to {
                meta.push((
                    format!("⚠ folder bound to {}", paths::shell_path(bound)),
                    "33",
                )); // hatcheck bind ties this folder to another profile
            }
            let room = width as i64
                - 8
                - meta
                    .iter()
                    .map(|(t, _)| 3 + t.chars().count() as i64)
                    .sum::<i64>();
            let tail: String = meta
                .iter()
                .map(|(text, c)| p("2", " · ") + &p(c, text))
                .collect();
            out::println(&format!(
                "  {}  {}{}",
                p("2", &format!("{:>4}", times::ago(now - s.when))),
                p("1", &truncate(&s.title, room.max(20) as usize)),
                tail
            ));
            let mut command = session_command(s, full);
            let usage = cells.get(&s.profile).cloned();
            let this = (account.get(&s.profile).copied(), usage.clone());
            // Left out when the session above uses the same account, even through another
            // profile, so it would only repeat.
            if let Some(text) = usage.as_ref().filter(|u| !u.is_empty()) {
                if previous.as_ref() != Some(&this) {
                    // A shell comment, so pasting the whole line still runs just the command.
                    command = format!("{command:<command_width$}  {} {text}", p("2", "#"));
                }
            }
            previous = Some(this);
            out::println(&format!("{}{command}", " ".repeat(8)));
        }
        if *older > 0 {
            out::println(&format!(
                "  {}",
                p(
                    "2",
                    &format!(
                        "{:>4}  older session{}",
                        format!("+{older}"),
                        if *older > 1 { "s" } else { "" }
                    )
                )
            ));
        }
    }
    out::println(&p(
        "2",
        &format!(
            "Profiles: {}",
            per_profile
                .iter()
                .map(|(pr, n)| format!("{} ({n})", paths::shell_path(pr)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    ));
    let mut snapshots: Vec<((String, String), Vec<String>)> = Vec::new();
    let mut no_data: Vec<(String, Vec<String>)> = Vec::new();
    for pr in used_profiles.iter().chain(&used_homes) {
        let Some((_, (usage, saved_at, why))) = results.iter().find(|(r, _)| r == pr) else {
            continue;
        };
        let why = why.clone().unwrap_or_else(|| "None".into());
        if usage.is_none() {
            match no_data.iter_mut().find(|(w, _)| *w == why) {
                Some((_, items)) => items.push(paths::shell_path(pr)),
                None => no_data.push((why, vec![paths::shell_path(pr)])),
            }
        } else if let Some(t) = saved_at {
            let source = if used_homes.contains(pr) {
                "Codex's session files"
            } else {
                "Claude Code's last snapshot"
            }
            .to_string();
            let item = format!("{} {} ago", paths::shell_path(pr), times::ago(now - t));
            match snapshots
                .iter_mut()
                .find(|(k, _)| k.0 == source && k.1 == why)
            {
                Some((_, items)) => items.push(item),
                None => snapshots.push(((source, why), vec![item])),
            }
        }
    }
    for ((source, why), items) in &snapshots {
        out::println(&p(
            "2",
            &format!("Usage from {source} ({why}): {}", items.join(", ")),
        ));
    }
    for (why, items) in &no_data {
        out::println(&p(
            "2",
            &format!("No usage data ({why}): {}", items.join(", ")),
        ));
    }
    let mut notes = Vec::new();
    if elsewhere > 0 {
        notes.push(format!(
            "{elsewhere} in other folders (show with --all-folders)"
        ));
    }
    if !hidden.is_empty() {
        let mut sorted = hidden.clone();
        sorted.sort();
        notes.push(format!(
            "Hidden: {} (show with --all)",
            sorted
                .iter()
                .map(|(k, n)| format!("{n} {k}"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if empty > 0 {
        notes.push(format!("{empty} without messages skipped"));
    }
    if unreadable > 0 {
        notes.push(format!("{unreadable} unreadable"));
    }
    if !notes.is_empty() {
        out::println(&p("2", &notes.join("; ")));
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn per_model_limits_are_named_in_violet() {
        let now = 1792274316.0;
        let iso = |t: f64| times::iso_utc(t).unwrap();
        let usage = json!({"limits": [
            {"kind": "session", "group": "session", "percent": 40, "severity": "normal", "resets_at": iso(now + 2.0 * 3600.0)},
            {"kind": "weekly_all", "group": "weekly", "percent": 10, "severity": "normal", "resets_at": iso(now + 3.0 * 86400.0)},
            {"kind": "weekly_scoped", "group": "weekly", "percent": 52, "severity": "normal", "resets_at": iso(now + 4.0 * 86400.0),
             "scope": {"model": {"display_name": "Fable"}}}]});
        let results: Vec<(String, Usage)> = vec![("p".into(), (Some(usage), None, None))];
        let cells = usage_cells(
            &results,
            now,
            &Paint {
                on: true,
                wide: true,
            },
        );
        let cell = &cells["p"];
        for painted in ["\x1b[2m5h", "\x1b[2mweek", "\x1b[38;5;134mFable"] {
            assert!(cell.contains(painted), "{painted:?} in {cell:?}");
        }
    }

    #[test]
    fn hours() {
        assert_eq!(parse_hours("36"), Some(36.0));
        assert_eq!(parse_hours("1.5"), Some(1.5));
        assert_eq!(parse_hours("12h"), Some(12.0));
        assert_eq!(parse_hours("3D"), Some(72.0));
        assert_eq!(parse_hours("nonsense"), None);
        assert_eq!(parse_hours("1."), None);
        assert_eq!(parse_hours(".5"), None);
    }
}
