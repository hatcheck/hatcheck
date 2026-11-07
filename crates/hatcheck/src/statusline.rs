//! hatcheck statusline: a status line for Claude Code that says which profile the session runs
//! in and what's left of its plan: `work · Opus 5.5 · 5h 62% ↻2h41m · wk 30% ↻3d2h · ctx 23% ·
//! $1.23`. Run by Claude Code, from the `statusLine` setting that `--install PROFILE` adds to a
//! profile's settings.json, it reads what Claude Code hands it on stdin, Claude Code's own
//! rate limits among it, and asks nothing; run in a terminal, or with `--show`, it previews the
//! line from the profile's last usage snapshot. docs/commands.md's `hatcheck statusline` section
//! has the rest.

use crate::args::{Count, Parser};
use crate::bind;
use crate::find;
use crate::ospath;
use crate::out::{self, Paint};
use crate::paths;
use crate::py::{self, get, s, truthy, Num};
use crate::sessions;
use crate::times;
use crate::usage;
use serde_json::{json, Value};
use std::io::{IsTerminal, Read};

/// What the setting's command is found by: the one hatcheck added, to update or remove it.
const MARK: &str = "statusline";
/// How old the profile's usage snapshot may be for its per-model limits to join the line
/// Claude Code's own numbers make.
const SNAPSHOT_MAX_AGE: f64 = 3600.0;
/// How much of stdin is read at most.
const STDIN_MAX: u64 = 1 << 20;

/// The profile's name as the line shows it: default for ~/.claude, NAME for ~/.claude-NAME, or
/// the folder's name.
pub fn profile_name(folder: &str) -> String {
    if paths::same_folder(folder, &paths::default_profile()) {
        return "default".into();
    }
    let base = ospath::basename(folder.trim_end_matches(ospath::is_sep));
    base.strip_prefix(".claude-").unwrap_or(&base).to_string()
}

/// The profile Claude Code runs in: CLAUDE_CONFIG_DIR, else ~/.claude.
fn profile_from_env() -> String {
    match std::env::var("CLAUDE_CONFIG_DIR") {
        Ok(dir) if !dir.is_empty() => ospath::abspath(&ospath::expanduser(&dir)),
        _ => paths::default_folder(crate::agent::Agent::Claude),
    }
}

/// A limit as the line shows it: its label, what's left and when it resets.
struct Limit {
    label: String,
    left: Option<f64>,
    reset: Option<f64>,
}

impl Limit {
    fn text(&self, now: f64, paint: &Paint) -> String {
        let mut text = match self.left {
            None => paint.p("2", &format!("{} ?", self.label)),
            Some(left) => {
                let code = if left <= 10.0 {
                    "31"
                } else if left <= 25.0 {
                    "33"
                } else {
                    ""
                };
                format!(
                    "{} {}",
                    self.label,
                    paint.p(code, &format!("{}%", left.round() as i64))
                )
            }
        };
        if let Some(reset) = self.reset.filter(|r| *r > now) {
            text += &paint.p("2", &format!(" ↻{}", times::duration(reset - now)));
        }
        text
    }
}

/// The limits in what Claude Code hands the status line: its own 5-hour and 7-day rate limits,
/// each with the percentage used and the reset as epoch seconds (Claude Code 2.1.319).
fn payload_limits(payload: &Value) -> Vec<Limit> {
    let rates = get(payload, "rate_limits");
    [("five_hour", "5h"), ("seven_day", "wk")]
        .iter()
        .filter_map(|(key, label)| {
            let row = get(rates, key);
            if !row.is_object() {
                return None;
            }
            let used = Num::of(get(row, "used_percentage")).map(Num::f);
            Some(Limit {
                label: label.to_string(),
                left: used.map(|u| (100.0 - u).clamp(0.0, 100.0)),
                reset: Num::of(get(row, "resets_at")).map(Num::f),
            })
        })
        .collect()
}

/// The limits in the profile's last usage snapshot, as `hatcheck usage --cached` reads them,
/// with when it was saved; or None without one. only_models keeps the per-model weekly limits
/// alone, which Claude Code's own numbers don't carry.
fn snapshot_limits(folder: &str, now: f64, only_models: bool) -> Option<(Vec<Limit>, f64)> {
    let (_, cache) = usage::read_config(folder);
    let utilization = get(&cache, "utilization");
    if !truthy(utilization) {
        return None;
    }
    let saved_at = Num::of(get(&cache, "fetchedAtMs")).map_or(0.0, Num::f) / 1000.0;
    let Value::Array(limits) = get(utilization, "limits") else {
        return Some((Vec::new(), saved_at));
    };
    let rows = usage::fold_limits(limits);
    let limits = rows
        .iter()
        .filter(|row| !only_models || s(get(row, "kind")) == Some("weekly_scoped"))
        .map(|row| Limit {
            label: match usage::row_label(row, true).as_str() {
                "week" => "wk".to_string(),
                other => other.to_string(),
            },
            left: usage::percent_left(
                get(row, "percent"),
                get(row, "resets_at"),
                Some(saved_at),
                now,
            )
            .map(Num::f),
            reset: s(get(row, "resets_at")).and_then(times::parse_time),
        })
        .collect();
    Some((limits, saved_at))
}

/// The line, from what Claude Code handed over and the profile it runs in.
fn line_from_payload(payload: &Value, folder: &str, now: f64, paint: &Paint) -> String {
    let dot = paint.p("2", " · ");
    let mut parts = vec![paint.accent(&profile_name(folder))];
    if let Some(model) = s(get(get(payload, "model"), "display_name")).filter(|m| !m.is_empty()) {
        parts.push(model.to_string());
    }
    let mut limits = payload_limits(payload);
    // The per-model weekly limits from the profile's snapshot, while it's fresh: Claude Code's
    // own numbers are the 5-hour and 7-day ones.
    if let Some((models, saved_at)) = snapshot_limits(folder, now, true) {
        if now - saved_at <= SNAPSHOT_MAX_AGE {
            limits.extend(models);
        }
    }
    parts.extend(limits.iter().map(|l| l.text(now, paint)));
    if let Some(used) = Num::of(get(get(payload, "context_window"), "used_percentage")) {
        let used = used.f().round() as i64;
        let code = if used >= 90 {
            "31"
        } else if used >= 75 {
            "33"
        } else {
            ""
        };
        parts.push(format!("ctx {}", paint.p(code, &format!("{used}%"))));
    }
    if let Some(cost) = Num::of(get(get(payload, "cost"), "total_cost_usd")).map(Num::f) {
        if cost > 0.0 {
            parts.push(sessions::dollars(cost));
        }
    }
    parts.join(&dot)
}

/// The line as a preview, from the profile's snapshot alone, with the snapshot's age.
fn line_from_snapshot(folder: &str, now: f64, paint: &Paint) -> String {
    let dot = paint.p("2", " · ");
    let mut parts = vec![paint.accent(&profile_name(folder))];
    match snapshot_limits(folder, now, false) {
        None => parts.push(paint.p("2", "usage ?")),
        Some((limits, saved_at)) => {
            parts.extend(limits.iter().map(|l| l.text(now, paint)));
            parts.push(paint.p("2", &format!("cached {} ago", times::ago(now - saved_at))));
        }
    }
    parts.join(&dot)
}

/// Colours for a line Claude Code shows: on unless NO_COLOR says otherwise, since Claude Code
/// renders them, though stdout is its pipe.
fn claude_paint() -> Paint {
    Paint {
        on: std::env::var_os("NO_COLOR").is_none_or(|v| v.is_empty()),
        wide: false,
    }
}

/// What Claude Code handed over on stdin, when it did: not when stdin is a terminal, empty or
/// not JSON.
fn payload_from_stdin() -> Option<Value> {
    let stdin = std::io::stdin();
    if stdin.is_terminal() {
        return None;
    }
    let mut text = String::new();
    stdin
        .lock()
        .take(STDIN_MAX)
        .read_to_string(&mut text)
        .ok()?;
    serde_json::from_str::<Value>(&text)
        .ok()
        .filter(Value::is_object)
}

/// The command the setting runs: this hatcheck, by its full path, so it's found from any PATH.
fn command() -> String {
    format!("{} {MARK}", bind::hatcheck_exe_quoted())
}

fn ours(setting: &Value) -> bool {
    s(get(setting, "command")).is_some_and(|c| c.contains("hatcheck") && c.contains(MARK))
}

/// Add the setting to each profile's settings.json, or point the one hatcheck added at this
/// hatcheck; one somebody else set is left as it is, and said so. The exit status says whether
/// every profile has it now.
fn install(folders: &[String], prog: &str) -> i32 {
    let paint = Paint::new();
    let command = command();
    let mut code = 0;
    for folder in folders {
        let path = ospath::join(folder, "settings.json");
        let shown = paths::shell_path(&path);
        let mut settings = match bind::read_settings_object(&path) {
            Ok(o) => o,
            Err(why) => {
                out::error(&format!("{prog}: {shown}: {why}"));
                code = 1;
                continue;
            }
        };
        let current = settings.get("statusLine").cloned().unwrap_or(Value::Null);
        if truthy(&current) && !ours(&current) {
            out::warn(&format!(
                "{shown} has a status line of its own ({}): left as it is. Take it out to use hatcheck's.",
                py::str_of(get(&current, "command"))
            ));
            code = 1;
            continue;
        }
        let same = s(get(&current, "command")) == Some(command.as_str());
        if !same {
            settings.insert(
                "statusLine".into(),
                json!({"type": "command", "command": command}),
            );
            if let Err(e) = bind::write_json(&path, &Value::Object(settings)) {
                out::error(&format!("{prog}: can't write {shown}: {e}"));
                code = 1;
                continue;
            }
        }
        out::success(&format!(
            "{} {shown}: statusLine runs {}",
            if same {
                "Already in"
            } else if truthy(&current) {
                "Updated in"
            } else {
                "Added to"
            },
            paint.command(&command)
        ));
        out::println(&format!(
            "  Now, in a session there: {}",
            line_from_snapshot(folder, times::now(), &paint)
        ));
    }
    code
}

/// Take out the setting hatcheck added; one somebody else set is left, and said so.
fn uninstall(folders: &[String], prog: &str) -> i32 {
    let mut code = 0;
    for folder in folders {
        let path = ospath::join(folder, "settings.json");
        let shown = paths::shell_path(&path);
        let mut settings = match bind::read_settings_object(&path) {
            Ok(o) => o,
            Err(why) => {
                out::error(&format!("{prog}: {shown}: {why}"));
                code = 1;
                continue;
            }
        };
        match settings.get("statusLine") {
            None => out::println(&format!("{shown} has no status line.")),
            Some(current) if !ours(current) => {
                out::warn(&format!(
                    "{shown} has a status line that isn't hatcheck's ({}): left as it is.",
                    py::str_of(get(current, "command"))
                ));
                code = 1;
            }
            Some(_) => {
                settings.remove("statusLine");
                if let Err(e) = bind::write_json(&path, &Value::Object(settings)) {
                    out::error(&format!("{prog}: can't write {shown}: {e}"));
                    code = 1;
                    continue;
                }
                out::success(&format!("Took hatcheck's status line out of {shown}"));
            }
        }
    }
    code
}

/// The Claude Code profiles named, or every one with --all.
fn named_profiles(names: &[String], all: bool, prog: &str) -> Result<Vec<String>, i32> {
    if all {
        return Ok(usage::find_profiles());
    }
    if names.is_empty() {
        out::error(&format!(
            "{prog}: name a profile (work, default, a path), or --all for every one"
        ));
        return Err(2);
    }
    let mut folders = Vec::new();
    for name in names {
        let (agent, folder) = find::place_seen(name).map_err(|m| out::fail(&m))?;
        if agent != crate::agent::Agent::Claude {
            out::error(&format!(
                "{prog}: {} is {}'s; the status line is Claude Code's",
                paths::shell_path(&folder),
                agent.name()
            ));
            return Err(1);
        }
        if !ospath::isdir(&folder) {
            out::error(&format!(
                "{prog}: there's no {}",
                paths::shell_path(&folder)
            ));
            return Err(1);
        }
        folders.push(folder);
    }
    Ok(folders)
}

pub fn main(argv: &[String], prog: &str) -> i32 {
    let parser = Parser::new(
        prog,
        "A status line for Claude Code: the profile the session runs in, its model, what's left of the plan's 5-hour and weekly limits and when each resets, the context used and the session's cost. Run by Claude Code, from the statusLine setting --install adds to a profile's settings.json, it reads what Claude Code hands it and asks nothing. Run in a terminal, it previews the line from the profile's last usage snapshot.",
    )
    .positional("profiles", Count::Any, "profiles for --install, --uninstall or --show: NAME for ~/.claude-NAME, default, or a path")
    .flag(&["--install"], "install", "add the statusLine setting to each profile's settings.json, or point the one hatcheck added at this hatcheck")
    .flag(&["--uninstall"], "uninstall", "take out the statusLine setting hatcheck added")
    .flag(&["--all"], "all", "with --install or --uninstall: every profile")
    .flag(&["--show"], "show", "preview the line for a profile (default: the one CLAUDE_CONFIG_DIR names, or ~/.claude)")
    .exclusive(&["install", "uninstall", "show"]);
    let args = match parser.parse(argv) {
        Ok(a) => a,
        Err(stop) => return stop.exit_code(),
    };
    out::setup_console();
    let names: Vec<String> = args.values("profiles").to_vec();
    if args.flag("install") || args.flag("uninstall") {
        let folders = match named_profiles(&names, args.flag("all"), prog) {
            Ok(f) => f,
            Err(code) => return code,
        };
        if folders.is_empty() {
            out::println("No Claude Code profile to change.");
            return 1;
        }
        return if args.flag("install") {
            install(&folders, prog)
        } else {
            uninstall(&folders, prog)
        };
    }
    if names.len() > 1 {
        return parser
            .error("one profile to show; --install and --uninstall take several")
            .exit_code();
    }
    let now = times::now();
    if !args.flag("show") && names.is_empty() {
        if let Some(payload) = payload_from_stdin() {
            // Run by Claude Code: the line, coloured for it, and nothing else.
            let folder = profile_from_env();
            out::println(&line_from_payload(&payload, &folder, now, &claude_paint()));
            return 0;
        }
    }
    let folder = match names.first() {
        None => profile_from_env(),
        Some(name) => match find::place_seen(name) {
            Ok((crate::agent::Agent::Claude, folder)) => folder,
            Ok((agent, folder)) => {
                return out::fail(&format!(
                    "{} is {}'s; the status line is Claude Code's",
                    paths::shell_path(&folder),
                    agent.name()
                ))
            }
            Err(m) => return out::fail(&m),
        },
    };
    if !ospath::isdir(&folder) {
        return out::fail(&format!("There's no {}", paths::shell_path(&folder)));
    }
    let paint = Paint::new();
    out::println(&line_from_snapshot(&folder, now, &paint));
    if !out::capturing() && std::io::stdout().is_terminal() {
        out::println(&paint.p(
            "2",
            &format!(
                "A preview from the profile's last usage snapshot; in a session Claude Code hands the line its own numbers. {prog} --install {} adds it.",
                profile_name(&folder)
            ),
        ));
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_line_reads_claude_codes_numbers() {
        let now = 1_800_000_000.0;
        let payload = json!({"model": {"id": "claude-opus-5-5", "display_name": "Opus 5.5"},
            "rate_limits": {"five_hour": {"used_percentage": 38, "resets_at": now + 2.0 * 3600.0 + 41.0 * 60.0},
                            "seven_day": {"used_percentage": 70.4, "resets_at": now + 3.0 * 86400.0}},
            "context_window": {"used_percentage": 23.4}, "cost": {"total_cost_usd": 1.234}});
        let paint = Paint {
            on: false,
            wide: false,
        };
        let dir = std::env::temp_dir().join(format!("hatcheck-statusline-{}", std::process::id()));
        let folder = dir.join(".claude-work");
        std::fs::create_dir_all(&folder).unwrap();
        let line = line_from_payload(&payload, &folder.to_string_lossy(), now, &paint);
        assert_eq!(
            line,
            "work · Opus 5.5 · 5h 62% ↻2h41m · wk 30% ↻3d0h · ctx 23% · $1.23"
        );
        // Without rate limits or a cost, only what there is.
        let line = line_from_payload(
            &json!({"model": {"display_name": "Opus 5.5"}}),
            &folder.to_string_lossy(),
            now,
            &paint,
        );
        assert_eq!(line, "work · Opus 5.5");
        // Colours: a limit low on what's left is marked, the profile stands out.
        let on = Paint {
            on: true,
            wide: false,
        };
        let line = line_from_payload(
            &json!({"rate_limits": {"five_hour": {"used_percentage": 95}}}),
            &folder.to_string_lossy(),
            now,
            &on,
        );
        assert!(line.contains("\x1b[31m5%\x1b[0m"), "{line}");
        assert!(line.starts_with("\x1b[1;36mwork\x1b[0m"), "{line}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_profile_is_named_as_the_launcher_names_it() {
        assert_eq!(profile_name("/Users/me/.claude-work"), "work");
        assert_eq!(profile_name("/Users/me/.claude-work/"), "work");
        assert_eq!(profile_name("/elsewhere/claude-x"), "claude-x");
    }
}
