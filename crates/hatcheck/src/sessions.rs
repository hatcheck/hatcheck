//! hatcheck [sessions] [HOURS] ...: recent sessions from every agent's profiles and homes,
//! grouped by folder, each with the command that resumes it or opens DSH's session history,
//! and what's left of its account's plan.
//! docs/commands.md's `hatcheck` section describes the list.

use crate::agent::Agent;
use crate::args::{Count, Parser, Takes};
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

/// What marks a session open when a lock says so and no process is known: Codex's or agy's.
fn open_in(holder: &str) -> Value {
    json!({"pid": null, "status": null, "in": holder})
}

/// The command that resumes a listed session, Claude Code's or Codex's.
pub fn session_command(s: &Summary, full: bool) -> String {
    session_command_in(&paths::Shell::here(), s, full)
}

/// The same, in the words of a given shell.
pub fn session_command_in(shell: &paths::Shell, s: &Summary, full: bool) -> String {
    shell.session_command(
        s.agent,
        &s.profile,
        &s.id,
        full,
        paths::provider_of(py::s(&s.model)),
    )
}

/// A session's cost as its row shows it: dollars and cents, `<$0.01` under a cent.
pub fn dollars(usd: f64) -> String {
    if usd < 0.005 {
        "<$0.01".into()
    } else {
        format!("${usd:.2}")
    }
}

/// The command with its program in cyan: `cl`, `claude`, `claude-k`, `cx`, `codex` or `agy`, the
/// word that says which agent the session belongs to, since the row carries no tag for it. In
/// the full form it follows the variable that names the profile.
pub fn paint_program(command: &str, agent: Agent, paint: &Paint) -> String {
    let is_program = |word: &str| {
        Some(word) == agent.launcher()
            || word
                .strip_prefix(agent.program())
                .is_some_and(|rest| rest.is_empty() || rest.starts_with('-'))
    };
    let mut found = false;
    command
        .split(' ')
        .map(|word| {
            if !found && is_program(word) {
                found = true;
                paint.p("36", word)
            } else {
                word.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The agents that have a profile, home or folder here, as the heading names them before
/// "CLI sessions": Claude Code, then each of the others that has one, without its own "CLI".
fn agents_text(present: &[Agent]) -> String {
    let names: Vec<&str> = Agent::ALL
        .iter()
        .filter(|a| present.contains(a))
        .map(|a| a.heading_name())
        .collect();
    match names.len() {
        0 => "No".to_string(), // every agent left out by config.toml
        1 => names[0].to_string(),
        2 => format!("{} and {}", names[0], names[1]),
        _ => format!(
            "{} and {}",
            names[..names.len() - 1].join(", "),
            names[names.len() - 1]
        ),
    }
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
            // One line: an account with many limits that read the same shows them as one.
            for row in &usage::fold_limits(limits) {
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

/// One more session hidden for a reason, in the count the end of the list gives.
fn count_hidden(hidden: &mut Vec<(String, usize)>, label: &str) {
    match hidden.iter_mut().find(|(k, _)| k == label) {
        Some((_, n)) => *n += 1,
        None => hidden.push((label.to_string(), 1)),
    }
}

/// A folder's sessions as the list shows them: the ones listed, how many more it has in the
/// window, and what they all cost together, with how many of them have a cost.
struct Folder {
    path: String,
    listed: Vec<Summary>,
    more: usize,
    cost: f64,
    priced: usize,
    total: usize,
}

/// Colour for a cost, by its size, so a big one stands out: dim under $1, plain to $10, then
/// gold to $50, orange to $100 and red from there, from the time bars' palette. wide: the
/// terminal shows 256 colours.
pub fn cost_color(usd: f64, wide: bool) -> String {
    let (step, basic) = if usd < 1.0 {
        return "2".into();
    } else if usd < 10.0 {
        return String::new();
    } else if usd < 50.0 {
        (3, "33")
    } else if usd < 100.0 {
        (1, "33")
    } else {
        (0, "31")
    };
    if wide {
        format!("38;5;{}", usage::URGENCY_COLORS[step])
    } else {
        basic.into()
    }
}

/// A cost with the detail Claude Code's /cost gives a small one: cents, or four decimals under
/// a cent.
pub fn dollars_fine(usd: f64) -> String {
    if usd >= 0.005 {
        format!("${usd:.2}")
    } else {
        format!("${usd:.4}")
    }
}

/// A count of tokens as Claude Code's /cost shows one: 137, 6.6k, 12.1m.
pub fn tokens(n: u64) -> String {
    let scaled = |x: f64, unit: &str| {
        let text = format!("{x:.1}");
        format!("{}{unit}", text.strip_suffix(".0").unwrap_or(&text))
    };
    if n < 1000 {
        n.to_string()
    } else if n < 999_950 {
        scaled(n as f64 / 1000.0, "k")
    } else {
        scaled(n as f64 / 1e6, "m")
    }
}

/// How many turns a session has, as hatcheck show counts them, with how many are your prompts
/// and how many tool calls there were, by reading it whole; None when it can't be read.
fn turn_counts(s: &Summary) -> Option<(usize, usize, usize)> {
    let conv = crate::conv::read_for(s).ok()?;
    let prompts = conv.turns.iter().filter(|t| t.0 == "user").count();
    Some((conv.turns.len(), prompts, conv.calls))
}

/// What --details shows for a session, kept as numbers so the columns line up across the whole
/// list and the biggest in each can be marked: its turns, as hatcheck show counts them, and
/// what its agent wrote down of what it spent.
struct Block {
    turns: Option<(usize, usize, usize)>,
    spend: Option<facts::Spend>,
}

/// The first line's labels, each before its number: turns, prompts and tool calls, the API,
/// wall and tool time, and the lines added and removed.
const STATS: [&str; 7] = ["turns", "prompts", "calls", "API", "wall", "tools", "lines"];

/// The token columns' headers: in, out, thinking, cache read, cache write and web searches.
const KINDS: [&str; 6] = ["in", "out", "think", "read", "write", "web"];

impl Block {
    fn of(s: &Summary) -> Block {
        Block {
            turns: turn_counts(s),
            spend: s.spend.clone(),
        }
    }

    /// The first line's cells, each a value with its text, or None where the agent keeps none.
    fn stats(&self) -> [Option<(f64, String)>; 7] {
        let count = |n: usize| Some((n as f64, n.to_string()));
        let (turns, prompts, calls) = match self.turns {
            Some((t, p, c)) => (count(t), count(p), count(c)),
            None => (None, None, None),
        };
        let x = self.spend.as_ref();
        let span = |t: Option<f64>| t.map(|t| (t, times::duration(t)));
        [
            turns,
            prompts,
            calls,
            span(x.and_then(|x| x.api_seconds)),
            span(x.and_then(|x| x.wall_seconds)),
            span(x.and_then(|x| x.tool_seconds)),
            x.and_then(|x| match (x.lines_added, x.lines_removed) {
                (Some(a), Some(r)) => Some((a as f64, format!("+{a} -{r}"))),
                _ => None,
            }),
        ]
    }

    /// A row per model that spent anything, the costliest first: its name (`tokens` where the
    /// agent counts every model's together), its cost, and its tokens by KINDS.
    fn models(&self) -> Vec<(String, Option<f64>, [u64; 6])> {
        let Some(spend) = &self.spend else {
            return Vec::new();
        };
        let mut rows: Vec<&facts::ModelSpend> = spend
            .models
            .iter()
            .filter(|m| m.tokens() > 0 || m.usd.is_some())
            .collect();
        rows.sort_by(|a, b| {
            b.usd
                .unwrap_or(0.0)
                .partial_cmp(&a.usd.unwrap_or(0.0))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        rows.into_iter()
            .map(|m| {
                (
                    m.model.clone().unwrap_or_else(|| "tokens".into()),
                    m.usd,
                    [
                        m.input,
                        m.output,
                        m.thinking,
                        m.cache_read,
                        m.cache_write,
                        m.web_searches,
                    ],
                )
            })
            .collect()
    }
}

/// The columns of every block listed: each one's width, its biggest value, and how many rows
/// have one, since the biggest is marked only with another to compare it with.
struct Columns {
    stats: [(usize, f64, usize); 7],
    name: usize,
    /// 0 when no row has a cost.
    cost: usize,
    /// (width, biggest, any at all) per kind of token: a kind no row has is left out.
    kinds: [(usize, u64, bool); 6],
    rows: usize,
}

impl Columns {
    fn of(blocks: &[Block]) -> Columns {
        let mut stats = [(0usize, 0f64, 0usize); 7];
        let (mut name, mut cost, mut rows) = (0, 0, 0);
        let mut kinds = [(0usize, 0u64, false); 6];
        for b in blocks {
            for (i, cell) in b.stats().iter().enumerate() {
                if let Some((v, text)) = cell {
                    stats[i] = (
                        stats[i].0.max(text.chars().count()),
                        stats[i].1.max(*v),
                        stats[i].2 + 1,
                    );
                }
            }
            for (model, usd, by_kind) in b.models() {
                rows += 1;
                name = name.max(model.chars().count());
                if let Some(u) = usd {
                    cost = cost.max(dollars_fine(u).len()).max("cost".len());
                }
                for (k, n) in by_kind.iter().enumerate() {
                    if *n > 0 {
                        kinds[k] = (
                            kinds[k].0.max(tokens(*n).chars().count()),
                            kinds[k].1.max(*n),
                            true,
                        );
                    }
                }
            }
        }
        for (k, label) in KINDS.iter().enumerate() {
            kinds[k].0 = kinds[k].0.max(label.len()); // the header counts in the width
        }
        Columns {
            stats,
            name,
            cost,
            kinds,
            rows,
        }
    }
}

/// A block's lines in the columns: the first with its labels, then, when a model spent
/// anything, a dim header and a row per model, the numbers right-aligned, `·` for none, the
/// biggest in a column in bold when another row has one, and a cost in its colour.
fn render(block: &Block, c: &Columns, paint: &Paint) -> Vec<String> {
    let p = |code: &str, text: &str| paint.p(code, text);
    let right = |text: &str, width: usize| {
        format!(
            "{}{text}",
            " ".repeat(width.saturating_sub(text.chars().count()))
        )
    };
    let stats = block.stats();
    // The trailing cells a row lacks are left off, not shown as none.
    let shown = stats.iter().rposition(Option::is_some).map_or(0, |i| i + 1);
    let first: Vec<String> = stats[..shown]
        .iter()
        .enumerate()
        .map(|(i, cell)| {
            let (width, biggest, rows) = c.stats[i];
            let number = match cell {
                Some((v, text)) if rows > 1 && *v == biggest && biggest > 0.0 => {
                    p("1", &right(text, width))
                }
                Some((_, text)) => right(text, width),
                None => p("2", &right("·", width)),
            };
            format!("{} {number}", p("2", STATS[i]))
        })
        .collect();
    let mut lines = vec![if first.is_empty() {
        p("2", "turns ?")
    } else {
        first.join("  ")
    }];
    let models = block.models();
    if models.is_empty() {
        return lines;
    }
    let kinds: Vec<usize> = (0..KINDS.len()).filter(|k| c.kinds[*k].2).collect();
    let mut header = vec![" ".repeat(c.name)];
    if c.cost > 0 {
        header.push(right("cost", c.cost));
    }
    header.extend(kinds.iter().map(|&k| right(KINDS[k], c.kinds[k].0)));
    lines.push(p("2", header.join("  ").trim_end()));
    for (model, usd, by_kind) in models {
        let mut row = vec![format!("{model:<width$}", width = c.name)];
        if c.cost > 0 {
            row.push(match usd {
                Some(u) => {
                    let text = dollars_fine(u);
                    format!(
                        "{}{}",
                        " ".repeat(c.cost.saturating_sub(text.len())),
                        p(&cost_color(u, paint.wide), &text)
                    )
                }
                None => p("2", &right("·", c.cost)),
            });
        }
        for &k in &kinds {
            let (width, biggest, _) = c.kinds[k];
            let n = by_kind[k];
            row.push(if n == 0 {
                p("2", &right("·", width))
            } else if c.rows > 1 && n == biggest {
                p("1", &right(&tokens(n), width))
            } else {
                right(&tokens(n), width)
            });
        }
        lines.push(row.join("  ").trim_end().to_string());
    }
    lines
}

pub fn main(argv: &[String], prog: &str) -> i32 {
    let parser = Parser::new(prog, "List recent Claude Code CLI sessions across all ~/.claude* profiles, Codex CLI sessions across all ~/.codex* homes, Antigravity CLI sessions from ~/.gemini/antigravity-cli, Kimi Code CLI sessions across all ~/.kimi-code* homes, ZCode sessions from ~/.zcode, DeepSeek Harness sessions across all ~/.dsh* homes, MiniMax Code sessions across all ~/.minimax* homes and the homes MINIMAX_DATA_DIR and MAVIS_DATA_DIR select, Qwen Code sessions across all ~/.qwen* homes and the home QWEN_HOME names, GitHub Copilot CLI sessions across all ~/.copilot* homes and the home COPILOT_HOME names, and OpenCode sessions from ~/.local/share/opencode, with the command that resumes each one or opens DSH's session history.")
        .positional("hours", Count::Optional, &format!("how far back to look (default {DEFAULT_HOURS}); accepts 36, 1.5, 12h or 3d"))
        .option(&["-n", "--per-folder"], "per_folder", "N", "show only the newest N sessions per folder (default: all)")
        .flag(&["-a", "--all"], "all", "also list sessions last used from Claude Desktop or the SDK, Codex sessions from the Codex app, its IDE extension or codex exec, the Antigravity app's conversations, Qwen Code sessions from its channels, scheduled tasks and other hosts, and Copilot sessions from clients other than its CLI")
        .flag(&["-A", "--all-folders"], "all_folders", "list every folder, even when run from a folder that has sessions")
        .flag(&["-f", "--full"], "full", "print CLAUDE_CONFIG_DIR=... claude instead of the cl shortcut, CODEX_HOME=... codex instead of cx, and KIMI_CODE_HOME=... kimi instead of ck")
        .flag(&["-C", "--cached"], "cached", "take plan usage from the snapshots Claude Code and Codex last saved, and hatcheck's last readings of agy's /usage and Kimi's usage, instead of the APIs")
        .flag(&["-d", "--details"], "details", "under each session, its turns and what it spent, by model and kind of token, with the durations its agent wrote down")
        .flag(&["-c", "--by-cost"], "by_cost", "the costliest sessions and folders first, instead of the newest")
        // Undo what config.toml sets, for one run; docs/commands.md's config.toml section lists them.
        .hidden(&["--no-details"], "no_details", Takes::Nothing)
        .hidden(&["--by-time"], "by_time", Takes::Nothing)
        .hidden(&["--no-full"], "no_full", Takes::Nothing)
        .hidden(&["--this-folder"], "this_folder", Takes::Nothing)
        .hidden(&["--usage"], "usage", Takes::Nothing)
        .exclusive(&["details", "no_details"])
        .exclusive(&["by_cost", "by_time"])
        .exclusive(&["full", "no_full"])
        .exclusive(&["all_folders", "this_folder"])
        .exclusive(&["no_usage", "usage"])
        .flag(&["-q", "--no-usage"], "no_usage", "leave plan usage out")
        .flag(&["-j", "--json"], "json", "print the sessions, and the accounts behind them, as JSON for scripts and apps")
        .flag(&["-e", "--email"], "email", "with --json, include each account's email address");
    let args = match parser.parse(argv) {
        Ok(a) => a,
        Err(stop) => return stop.exit_code(),
    };
    // What config.toml sets stands unless the command line says otherwise.
    let cfg = crate::config::current();
    let hours_text = args
        .value("hours")
        .map(str::to_string)
        .or_else(|| cfg.hours.clone());
    let hours_value: Value = match hours_text {
        None => json!(DEFAULT_HOURS),
        Some(text) => match parse_hours(&text) {
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
        None => cfg.per_folder,
        Some(n) => match n.parse::<usize>() {
            Ok(0) => None, // every session: undoes what config.toml set
            Ok(n) => Some(n),
            Err(_) => {
                return parser
                    .error("argument -n/--per-folder: expected a whole number (0 for all)")
                    .exit_code()
            }
        },
    };
    let setting = |flag: &str, undo: &str, from_file: Option<bool>| {
        args.flag(flag) || (from_file == Some(true) && !args.flag(undo))
    };
    let (all, json_out) = (args.flag("all"), args.flag("json"));
    let full = setting("full", "no_full", cfg.full);
    let details = setting("details", "no_details", cfg.details);
    let by_cost = setting("by_cost", "by_time", cfg.by_cost);
    let all_folders = setting("all_folders", "this_folder", cfg.all_folders);
    let use_usage = !setting("no_usage", "usage", cfg.usage.map(|u| !u));
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
                format!("{}\u{0}{}\u{0}{}", Agent::Claude.key(), key.0, key.1),
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
            // Codex holds a lock on a session it has open: listed even if its last message is older.
            let is_open = paths::codex_file_id(&path)
                .is_some_and(|sid| crate::find::codex_session_open(home, sid));
            let session = match ospath::getmtime(&path) {
                Ok(t) if t < cutoff && !is_open => continue,
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
            let Some(mut session) = session.filter(|s| s.has_messages || is_open) else {
                empty += 1;
                continue;
            };
            if session.when < cutoff && !is_open {
                continue;
            }
            session.profile = home.clone();
            session.open = is_open.then(|| open_in(Agent::Codex.program()));
            let key = format!(
                "{}\u{0}{}\u{0}{}",
                Agent::Codex.key(),
                ospath::realpath(home),
                session.id
            );
            found.put(key, (session.when, true, session.size), session, false);
        }
    }
    let agy_roots = paths::find_agy_stores();
    for root in &agy_roots {
        for row in crate::agy::rows(root) {
            let mut session = crate::agy::summary(root, &row);
            if session.entrypoint_text() == "subagent" {
                continue; // one agy ran itself
            }
            let is_open = crate::agy::session_open(root, &row.id);
            if !session.has_messages && !is_open {
                empty += 1;
                continue;
            }
            if session.when < cutoff && !is_open {
                continue;
            }
            session.open = is_open.then(|| open_in(crate::agy::holder(root)));
            let key = format!(
                "{}\u{0}{}\u{0}{}",
                Agent::Agy.key(),
                ospath::realpath(root),
                session.id
            );
            found.put(key, (session.when, true, session.size), session, false);
        }
    }
    // Nothing marks a Kimi or ZCode session open, so the window alone decides.
    let kimi_homes = paths::find_kimi_homes();
    for home in &kimi_homes {
        for x in crate::kimi::sessions(home) {
            if ospath::getmtime(&crate::kimi::wire_path(&x.dir)).is_ok_and(|t| t < cutoff) {
                continue; // the wire's mtime is never older than its last message
            }
            let session = crate::kimi::summary(home, &x);
            if !session.has_messages {
                empty += 1;
                continue;
            }
            if session.when < cutoff {
                continue;
            }
            let key = format!(
                "{}\u{0}{}\u{0}{}",
                Agent::Kimi.key(),
                ospath::realpath(home),
                session.id
            );
            found.put(key, (session.when, true, session.size), session, false);
        }
    }
    let zcode_roots = paths::find_zcode_roots();
    for root in &zcode_roots {
        for row in crate::zcode::rows(root) {
            let session = crate::zcode::summary(root, &row);
            if session.entrypoint_text() == "subagent" {
                continue; // one zcode ran itself
            }
            if !session.has_messages {
                empty += 1;
                continue;
            }
            if session.when < cutoff {
                continue;
            }
            let key = format!(
                "{}\u{0}{}\u{0}{}",
                Agent::Zcode.key(),
                ospath::realpath(root),
                session.id
            );
            found.put(key, (session.when, true, session.size), session, false);
        }
    }
    let dsh_homes = paths::find_dsh_homes();
    for home in &dsh_homes {
        for x in crate::dsh::sessions(home) {
            let mut session = crate::dsh::summary(home, &x);
            if session.entrypoint_text() == "subagent" {
                continue;
            }
            let is_open = crate::dsh::session_open(&x.dir);
            if !session.has_messages && !is_open {
                empty += 1;
                continue;
            }
            if session.when < cutoff && !is_open {
                continue;
            }
            session.open = is_open.then(|| open_in(Agent::Dsh.program()));
            let key = format!(
                "{}\u{0}{}\u{0}{}",
                Agent::Dsh.key(),
                ospath::realpath(home),
                session.id
            );
            found.put(key, (session.when, true, session.size), session, false);
        }
    }
    let mcode_homes = paths::find_mcode_homes();
    for home in &mcode_homes {
        for x in crate::mcode::sessions(home) {
            let mut session = crate::mcode::summary(home, &x);
            if session.entrypoint_text() == "subagent" {
                continue;
            }
            let is_open = crate::mcode::session_open(home, &x.row.id);
            if !session.has_messages && !is_open {
                empty += 1;
                continue;
            }
            if session.when < cutoff && !is_open {
                continue;
            }
            session.open = is_open.then(|| open_in(Agent::Mcode.program()));
            let key = format!(
                "{}\u{0}{}\u{0}{}",
                Agent::Mcode.key(),
                ospath::realpath(home),
                session.id
            );
            found.put(key, (session.when, true, session.size), session, false);
        }
    }
    let qwen_homes = paths::find_qwen_homes();
    for home in &qwen_homes {
        for x in crate::qwen::sessions(home) {
            let mut session = crate::qwen::summary(home, &x);
            if session.entrypoint_text() == "subagent" {
                continue;
            }
            let is_open = crate::qwen::session_open(home, &x.file, &x.id);
            if !session.has_messages && !is_open {
                empty += 1;
                continue;
            }
            if session.when < cutoff && !is_open {
                continue;
            }
            session.open = is_open.then(|| open_in(Agent::Qwen.program()));
            let key = format!(
                "{}\u{0}{}\u{0}{}",
                Agent::Qwen.key(),
                ospath::realpath(home),
                session.id
            );
            found.put(key, (session.when, true, session.size), session, false);
        }
    }
    let copilot_homes = paths::find_copilot_homes();
    for home in &copilot_homes {
        for x in crate::copilot::sessions(home) {
            let mut session = crate::copilot::summary(home, &x);
            let is_open = crate::copilot::session_open(&x.dir);
            if !session.has_messages && !is_open {
                empty += 1;
                continue;
            }
            if session.when < cutoff && !is_open {
                continue;
            }
            session.open = is_open.then(|| open_in(Agent::Copilot.program()));
            let key = format!(
                "{}\u{0}{}\u{0}{}",
                Agent::Copilot.key(),
                ospath::realpath(home),
                session.id
            );
            found.put(key, (session.when, true, session.size), session, false);
        }
    }
    let opencode_homes = paths::find_opencode_homes();
    for home in &opencode_homes {
        for x in crate::opencode::sessions(home) {
            let session = crate::opencode::summary(home, &x);
            if session.entrypoint_text() == "subagent" {
                continue; // a child session, which a subagent ran
            }
            if !session.has_messages {
                empty += 1;
                continue;
            }
            if session.when < cutoff {
                continue;
            }
            let key = format!(
                "{}\u{0}{}\u{0}{}",
                Agent::Opencode.key(),
                ospath::realpath(home),
                session.id
            );
            found.put(key, (session.when, true, session.size), session, false);
        }
    }
    facts::save_cache();

    let mut shown: Vec<Summary> = Vec::new();
    let mut hidden: Vec<(String, usize)> = Vec::new();
    for key in &found.order {
        let s = found.by_key[key].1.clone();
        if all {
            shown.push(s);
            continue;
        }
        // A folder config.toml leaves out, or doesn't list, counted with the rest hidden, which
        // --all shows.
        if s.cwd.as_deref().is_some_and(crate::config::folder_left_out) {
            count_hidden(&mut hidden, "by config");
            continue;
        }
        let entrypoint = s.entrypoint_text();
        if entrypoint != "cli" {
            count_hidden(
                &mut hidden,
                if entrypoint == "claude-desktop" {
                    "Desktop"
                } else {
                    entrypoint.as_str()
                },
            );
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
    let only_here = !mine.is_empty() && !all_folders;
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
        .chain(&agy_roots)
        .chain(&kimi_homes)
        .chain(&zcode_roots)
        .chain(&dsh_homes)
        .chain(&mcode_homes)
        .chain(&qwen_homes)
        .chain(&copilot_homes)
        .chain(&opencode_homes)
        .map(|p| (p.clone(), 0))
        .collect();
    for s in &shown {
        if let Some(entry) = per_profile.iter_mut().find(|(p, _)| *p == s.profile) {
            entry.1 += 1;
        }
    }
    // The Antigravity app's folder is named only when one of its conversations is listed.
    per_profile.retain(|(p, n)| *n > 0 || !crate::agy::is_app_root(p));

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
    // Folders A-Z, each newest first; with --by-cost, the costliest first: sessions by what
    // each cost, those without one after, and folders by what their sessions cost together.
    let folder_cost = |sessions: &[Summary]| sessions.iter().filter_map(|s| s.cost).sum::<f64>();
    groups.sort_by(|(fa, sa), (fb, sb)| {
        let by_name = fa.to_lowercase().cmp(&fb.to_lowercase());
        if by_cost {
            folder_cost(sb)
                .partial_cmp(&folder_cost(sa))
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(by_name)
        } else {
            by_name
        }
    });
    let mut folders: Vec<Folder> = Vec::new();
    for (path, mut sessions) in groups {
        sessions.sort_by(|a, b| {
            let newest = b
                .when
                .partial_cmp(&a.when)
                .unwrap_or(std::cmp::Ordering::Equal);
            if by_cost {
                b.cost
                    .unwrap_or(0.0)
                    .partial_cmp(&a.cost.unwrap_or(0.0))
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then(newest)
            } else {
                newest
            }
        });
        let total = sessions.len();
        let cost = folder_cost(&sessions);
        let priced = sessions.iter().filter(|s| s.cost.is_some()).count();
        let listed: Vec<Summary> = sessions
            .into_iter()
            .take(per_folder.unwrap_or(usize::MAX))
            .collect();
        folders.push(Folder {
            more: total - listed.len(),
            path,
            listed,
            cost,
            priced,
            total,
        });
    }

    // Plan usage for the profiles behind the listed sessions, one request per account.
    let used: Vec<&String> = folders
        .iter()
        .flat_map(|f| f.listed.iter().map(|s| &s.profile))
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
    // The app's folder has no account of its own to ask: agy's /usage answers for both.
    let used_roots: Vec<String> = agy_roots
        .iter()
        .filter(|r| used.contains(r) && !crate::agy::is_app_root(r))
        .cloned()
        .collect();
    let used_kimi: Vec<String> = kimi_homes
        .iter()
        .filter(|h| used.contains(h))
        .cloned()
        .collect();
    let used_zcode: Vec<String> = zcode_roots
        .iter()
        .filter(|r| used.contains(r))
        .cloned()
        .collect();
    let used_dsh: Vec<String> = dsh_homes
        .iter()
        .filter(|h| used.contains(h))
        .cloned()
        .collect();
    let used_mcode: Vec<String> = mcode_homes
        .iter()
        .filter(|h| used.contains(h))
        .cloned()
        .collect();
    let used_qwen: Vec<String> = qwen_homes
        .iter()
        .filter(|h| used.contains(h))
        .cloned()
        .collect();
    let used_copilot: Vec<String> = copilot_homes
        .iter()
        .filter(|h| used.contains(h))
        .cloned()
        .collect();
    let used_opencode: Vec<String> = opencode_homes
        .iter()
        .filter(|h| used.contains(h))
        .cloned()
        .collect();
    let loaded: Vec<(Entry, Usage)> = if use_usage {
        usage::load_usage(
            &used_profiles,
            args.flag("cached"),
            &used_homes,
            &used_roots,
            &used_kimi,
            &used_zcode,
            &used_dsh,
            &used_mcode,
            &used_qwen,
            &used_copilot,
            &used_opencode,
        )
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
    for f in folders.iter_mut() {
        for s in f.listed.iter_mut() {
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
        data.insert(
            "sorted_by".into(),
            json!(if by_cost { "cost" } else { "time" }),
        );
        let mut sessions = Vec::new();
        for f in &folders {
            for s in &f.listed {
                let mut row = json!({
                    "agent": s.agent.key(), "id": s.id, "profile": ospath::abspath(&s.profile),
                    "account": account.get(&s.profile).map_or(Value::Null, |n| json!(n)), "title": s.title, "cwd": s.cwd,
                    "last_active": times::iso_utc(s.when), "entrypoint": s.entrypoint, "branch": s.branch, "model": s.model,
                    "cost_usd": s.cost,
                    "open": s.open.as_ref().map_or(Value::Null, |live| json!({"pid": get(live, "pid"), "status": get(live, "status"),
                        "in": if truthy(get(live, "in")) { get(live, "in").clone() } else { json!(s.agent.program()) }})),
                    "continued_in": s.continued_in,
                    "bound_to": s.bound_to.as_ref().map_or(Value::Null, |b| json!(ospath::abspath(b))),
                    "resume": session_command(s, full)});
                if details {
                    // Only asked for: the turns mean reading the whole session.
                    row["spend"] = s.spend.as_ref().map_or(Value::Null, facts::Spend::to_json);
                    row["turns"] = turn_counts(s).map_or(Value::Null, |(total, prompts, calls)| {
                        json!({"total": total, "prompts": prompts, "tool_calls": calls})
                    });
                }
                sessions.push(row);
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
                    .filter(|f| f.more > 0)
                    .map(|f| (f.path.clone(), json!(f.more)))
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
        .flat_map(|f| {
            f.listed
                .iter()
                .map(|s| session_command(s, full).chars().count())
        })
        .max()
        .unwrap_or(0);

    let window = format!("{}h", py::g(hours));
    let mut present = Vec::new();
    if !crate::config::ignores(Agent::Claude) {
        present.push(Agent::Claude);
    }
    for (agent, folders) in [
        (Agent::Codex, &codex_homes),
        (Agent::Agy, &agy_roots),
        (Agent::Kimi, &kimi_homes),
        (Agent::Zcode, &zcode_roots),
        (Agent::Dsh, &dsh_homes),
        (Agent::Mcode, &mcode_homes),
        (Agent::Qwen, &qwen_homes),
        (Agent::Copilot, &copilot_homes),
        (Agent::Opencode, &opencode_homes),
    ] {
        if !folders.is_empty() {
            present.push(agent);
        }
    }
    out::println(
        &(paint.heading(&format!(
            "🎩 {} CLI sessions {}active in the last {window} or open now",
            agents_text(&present),
            if only_here { "in this folder, " } else { "" }
        )) + &p(
            "2",
            &format!(
                " ({}{} first{})",
                if only_here { "" } else { "by folder, " },
                if by_cost { "costliest" } else { "newest" },
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
    // --details: every session's block first, so the columns line up across the whole list.
    let blocks: Vec<Block> = if details {
        folders
            .iter()
            .flat_map(|f| f.listed.iter().map(Block::of))
            .collect()
    } else {
        Vec::new()
    };
    let columns = Columns::of(&blocks);
    let mut block_at = 0;
    let mut previous: Option<(Option<usize>, Option<String>)> = None; // (account, usage text) of the row above
    for f in &folders {
        let (folder, listed, older) = (&f.path, &f.listed, f.more);
        let missing = if ospath::isdir(folder) {
            String::new()
        } else {
            format!("  {}", p("31", "(directory missing)"))
        };
        let mut head = paint.accent(&format!("cd {}", paths::shell_path(folder)));
        if by_cost && f.priced > 0 {
            // What the folder's sessions cost together, as a shell comment like the usage after
            // a command, so the line still pastes.
            let of = if f.priced == f.total {
                format!("{} session{}", f.total, if f.total == 1 { "" } else { "s" })
            } else {
                format!("{} of {} sessions", f.priced, f.total)
            };
            head = format!(
                "{head}  {} {}{}",
                p("2", "#"),
                p(&cost_color(f.cost, paint.wide), &dollars(f.cost)),
                p("2", &format!(" in {of}"))
            );
        }
        out::println(&(head + &missing));
        for s in listed {
            // No profile or agent here: the command shows both (plain `claude` means ~/.claude).
            let cost_code = s.cost.map(|c| cost_color(c, paint.wide)); // outlives meta, which borrows it
            let mut meta: Vec<(String, &str)> = Vec::new();
            if truthy(&s.branch) {
                meta.push((py::str_of(&s.branch), "2"));
            }
            if let Some(model) = py::s(&s.model)
                .filter(|m| s.agent == Agent::Claude && !m.is_empty() && !m.starts_with("claude"))
            {
                meta.push((format!("on {model}"), "35")); // e.g. GLM or Kimi, through claude-z or claude-k
            }
            if let (Some(cost), Some(code)) = (s.cost, &cost_code) {
                meta.push((dollars(cost), code)); // what it cost, as the agent worked it out
            }
            let entrypoint = s.entrypoint_text();
            if entrypoint != "cli" {
                meta.push((format!("via {entrypoint}"), "33"));
            }
            if let Some(entry) = s.open.as_ref().filter(|e| truthy(e)) {
                let status = get(entry, "status");
                let pid = get(entry, "pid");
                let holder = if truthy(pid) {
                    format!("pid {}", py::str_of(pid))
                } else {
                    py::str_of(get(entry, "in"))
                };
                meta.push((
                    format!(
                        "● open in {holder}{}",
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
            let plain = session_command(s, full);
            let mut command = paint_program(&plain, s.agent, &paint);
            let usage = cells.get(&s.profile).cloned();
            let this = (account.get(&s.profile).copied(), usage.clone());
            // Left out when the session above uses the same account, even through another
            // profile, so it would only repeat.
            if let Some(text) = usage.as_ref().filter(|u| !u.is_empty()) {
                if previous.as_ref() != Some(&this) {
                    // A shell comment, so pasting the whole line still runs just the command,
                    // padded by the plain command's width: the colour codes take no room.
                    let pad = " ".repeat(command_width.saturating_sub(plain.chars().count()));
                    command = format!("{command}{pad}  {} {text}", p("2", "#"));
                }
            }
            previous = Some(this);
            out::println(&format!("{}{command}", " ".repeat(8)));
            if details {
                for line in render(&blocks[block_at], &columns, &paint) {
                    out::println(&format!("{}{line}", " ".repeat(8)));
                }
                block_at += 1;
            }
        }
        if older > 0 {
            // Cut by -n: the older ones, or with --by-cost the cheaper and the unpriced.
            out::println(&format!(
                "  {}",
                p(
                    "2",
                    &format!(
                        "{:>4}  {} session{}",
                        format!("+{older}"),
                        if by_cost { "other" } else { "older" },
                        if older > 1 { "s" } else { "" }
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
    for pr in used_profiles
        .iter()
        .chain(&used_homes)
        .chain(&used_roots)
        .chain(&used_kimi)
        .chain(&used_zcode)
        .chain(&used_dsh)
        .chain(&used_mcode)
        .chain(&used_qwen)
        .chain(&used_copilot)
        .chain(&used_opencode)
    {
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
            } else if used_roots.contains(pr) {
                "hatcheck's last reading of agy's /usage"
            } else if used_kimi.contains(pr) {
                "hatcheck's last reading of Kimi's usage"
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
    fn the_program_is_cyan_in_the_command() {
        let on = Paint {
            on: true,
            wide: false,
        };
        let off = Paint {
            on: false,
            wide: false,
        };
        let cyan = |word: &str| format!("\x1b[36m{word}\x1b[0m");
        let cases = [
            (
                "cl -k work --resume 3f1c",
                Agent::Claude,
                format!("{} -k work --resume 3f1c", cyan("cl")),
            ),
            (
                "claude-k --resume 3f1c",
                Agent::Claude,
                format!("{} --resume 3f1c", cyan("claude-k")),
            ),
            (
                "CLAUDE_CONFIG_DIR=~/.claude-work claude --resume 3f1c",
                Agent::Claude,
                format!(
                    "CLAUDE_CONFIG_DIR=~/.claude-work {} --resume 3f1c",
                    cyan("claude")
                ),
            ),
            (
                "$env:CODEX_HOME='C:\\x'; codex resume 019a; $env:CODEX_HOME=$null",
                Agent::Codex,
                format!(
                    "$env:CODEX_HOME='C:\\x'; {} resume 019a; $env:CODEX_HOME=$null",
                    cyan("codex")
                ),
            ),
            (
                "cx work resume 019a",
                Agent::Codex,
                format!("{} work resume 019a", cyan("cx")),
            ),
            (
                "agy --conversation c134",
                Agent::Agy,
                format!("{} --conversation c134", cyan("agy")),
            ),
            ("agy", Agent::Agy, cyan("agy")),
            (
                "ck work --session session_0752",
                Agent::Kimi,
                format!("{} work --session session_0752", cyan("ck")),
            ),
            (
                "KIMI_CODE_HOME=~/.kimi-code-work kimi --session session_0752",
                Agent::Kimi,
                format!(
                    "KIMI_CODE_HOME=~/.kimi-code-work {} --session session_0752",
                    cyan("kimi")
                ),
            ),
            (
                "zcode --resume sess_1fb0",
                Agent::Zcode,
                format!("{} --resume sess_1fb0", cyan("zcode")),
            ),
        ];
        for (command, agent, painted) in cases {
            assert_eq!(paint_program(command, agent, &on), painted);
            assert_eq!(paint_program(command, agent, &off), command);
        }
    }

    #[test]
    fn a_cost_is_shown_in_cents() {
        assert_eq!(dollars(5.009506499999996), "$5.01");
        assert_eq!(dollars(27.729848199999996), "$27.73");
        assert_eq!(dollars(0.005), "$0.01");
        assert_eq!(dollars(0.0042), "<$0.01");
        assert_eq!(dollars_fine(0.0018), "$0.0018"); // --details keeps a small one's detail
        assert_eq!(dollars_fine(5.0095), "$5.01");
    }

    #[test]
    fn a_cost_is_coloured_by_its_size() {
        assert_eq!(cost_color(0.5, true), "2");
        assert_eq!(cost_color(5.0, true), "");
        assert_eq!(cost_color(12.0, true), "38;5;178"); // gold
        assert_eq!(cost_color(60.0, true), "38;5;166"); // orange
        assert_eq!(cost_color(150.0, true), "38;5;160"); // red
        assert_eq!(cost_color(12.0, false), "33"); // a 16-colour terminal
        assert_eq!(cost_color(150.0, false), "31");
    }

    #[test]
    fn tokens_are_counted_as_claude_code_shows_them() {
        assert_eq!(tokens(137), "137");
        assert_eq!(tokens(1000), "1k");
        assert_eq!(tokens(1092), "1.1k");
        assert_eq!(tokens(83883), "83.9k");
        assert_eq!(tokens(999_960), "1m");
        assert_eq!(tokens(12_077_475), "12.1m");
    }

    #[test]
    fn the_details_line_up_in_columns() {
        let spend = facts::Spend {
            models: vec![
                facts::ModelSpend {
                    model: Some("claude-haiku-4-5".into()),
                    usd: Some(0.001777),
                    input: 1092,
                    output: 137,
                    ..Default::default()
                },
                facts::ModelSpend {
                    model: Some("claude-sonnet-5".into()),
                    usd: Some(5.0077),
                    input: 6551,
                    output: 83883,
                    thinking: 42938,
                    cache_read: 12077475,
                    cache_write: 696121,
                    web_searches: 1,
                },
            ],
            api_seconds: Some(1146.6),
            wall_seconds: Some(18300.0),
            tool_seconds: Some(80.5),
            lines_added: Some(130),
            lines_removed: Some(7),
        };
        let block = Block {
            turns: Some((42, 21, 97)),
            spend: Some(spend),
        };
        let bare = Block {
            turns: None,
            spend: None,
        };
        let off = Paint {
            on: false,
            wide: false,
        };
        let columns = Columns::of(std::slice::from_ref(&block));
        assert_eq!(
            render(&block, &columns, &off),
            [
                "turns 42  prompts 21  calls 97  API 19m  wall 5h05m  tools 1m  lines +130 -7",
                "                     cost    in    out  think   read   write  web",
                "claude-sonnet-5     $5.01  6.6k  83.9k  42.9k  12.1m  696.1k    1",
                "claude-haiku-4-5  $0.0018  1.1k    137      ·      ·       ·    ·",
            ]
        );
        assert_eq!(render(&bare, &columns, &off), ["turns ?"]);
        // On a terminal: the biggest in each column bold, a cost in its colour, none dim.
        let on = Paint {
            on: true,
            wide: false,
        };
        let lines = render(&block, &columns, &on);
        assert!(lines[2].contains("\x1b[1m6.6k\x1b[0m"), "{}", lines[2]);
        assert!(lines[2].contains("  $5.01  "), "{}", lines[2]); // under $10: plain
        assert!(lines[3].contains("\x1b[2m$0.0018\x1b[0m"), "{}", lines[3]);
        assert!(lines[3].ends_with("\x1b[2m  ·\x1b[0m"), "{}", lines[3]);
        // Columns run across blocks: a bigger count elsewhere widens them and takes the bold.
        let more = Block {
            turns: Some((7, 3, 120)),
            spend: None,
        };
        let both = Columns::of(&[block, more]);
        assert_eq!(render(&bare, &both, &off), ["turns ?"],);
        let lines = render(
            &Block {
                turns: Some((7, 3, 120)),
                spend: None,
            },
            &both,
            &on,
        );
        assert_eq!(
            lines,
            [format!(
                "{}  {}  {}",
                "\x1b[2mturns\x1b[0m  7",
                "\x1b[2mprompts\x1b[0m  3",
                "\x1b[2mcalls\x1b[0m \x1b[1m120\x1b[0m"
            )]
        );
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
