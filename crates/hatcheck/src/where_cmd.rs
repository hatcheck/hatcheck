//! hatcheck where SESSION [--json]: find a session by its ID, or the start of it. Reports
//! each agent's transcript or readable store and any session-owned data folder; for DSH, the
//! selected v4 log and its session directory, for mcode the active messages.jsonl and session
//! directory. Exits 1 when nothing matches. --json prints copies as data.

use crate::agent::Agent;
use crate::args::{Count, Parser};
use crate::ospath::{self, Part};
use crate::{out, paths, py};
use serde_json::{json, Value};

/// The files (and Claude Code's data folders) of the sessions whose ID starts with prefix.
pub fn find(prefix: &str) -> Vec<String> {
    let mut found = Vec::new();
    for profile in paths::find_profiles() {
        let mut matches = ospath::glob(
            &profile,
            &[
                Part::Name("projects"),
                Part::Wild("", ""),
                Part::Wild(prefix, ""),
            ],
        );
        matches.sort();
        found.extend(matches);
    }
    let home = paths::home();
    let mut homes = vec![ospath::join(&home, ".codex")];
    let mut others = ospath::glob(&home, &[Part::Wild(".codex-", "")]);
    others.sort();
    homes.extend(others);
    if let Ok(dir) = std::env::var("CODEX_HOME") {
        if !dir.is_empty() {
            homes.push(ospath::abspath(&ospath::expanduser(&dir)));
        }
    }
    let mut seen: Vec<String> = Vec::new();
    let lower = prefix.to_lowercase();
    for h in homes {
        let real = ospath::realpath(&h);
        if seen.contains(&real) {
            continue;
        }
        seen.push(real);
        let mut files = ospath::glob(
            &h,
            &[
                Part::Name("sessions"),
                Part::Wild("", ""),
                Part::Wild("", ""),
                Part::Wild("", ""),
                Part::Wild("rollout-", ".jsonl"),
            ],
        );
        files.extend(ospath::glob(
            &h,
            &[
                Part::Name("archived_sessions"),
                Part::Wild("rollout-", ".jsonl"),
            ],
        ));
        files.sort();
        for path in files {
            if paths::codex_file_id(&path).is_some_and(|id| id.starts_with(&lower)) {
                found.push(path);
            }
        }
    }
    for root in paths::find_agy_stores() {
        for row in crate::agy::rows_starting_with(&root, prefix) {
            found.push(crate::agy::conversation_path(&root, &row.id));
            let brain = crate::agy::brain_path(&root, &row.id);
            if ospath::isdir(&brain) {
                found.push(brain);
            }
        }
    }
    for home_dir in paths::find_kimi_homes() {
        for x in crate::kimi::sessions_starting_with(&home_dir, prefix) {
            found.push(x.dir);
        }
    }
    // A ZCode session is a row: its database, then its folder of tool results when it has one.
    for root in paths::find_zcode_roots() {
        for row in crate::zcode::rows_starting_with(&root, prefix) {
            found.push(crate::zcode::db_path(&root));
            let artifacts = crate::zcode::artifacts_path(&root, &row.id);
            if ospath::isdir(&artifacts) {
                found.push(artifacts);
            }
        }
    }
    for home in paths::find_dsh_homes() {
        for x in crate::dsh::sessions_starting_with(&home, prefix) {
            found.push(x.dir);
        }
    }
    for home in paths::find_mcode_homes() {
        for x in crate::mcode::sessions_starting_with(&home, prefix) {
            found.push(x.dir);
        }
    }
    found
}

/// The matches as --json gives them.
pub fn as_json(found: &[String], prefix: &str) -> Value {
    let mut copies = Vec::new();
    let projects = format!("{}projects{}", ospath::SEP, ospath::SEP);
    for path in found {
        if crate::kimi::dir_id(path).is_some()
            && path.contains(&format!("{0}sessions{0}wd_", ospath::SEP))
        {
            let id = crate::kimi::dir_id(path).unwrap_or_default();
            let home_dir = ospath::dirname(&ospath::dirname(&ospath::dirname(path)));
            copies.push(json!({"agent": Agent::Kimi.key(), "id": id, "profile": home_dir, "file": crate::kimi::wire_path(path),
                               "data_folder": path, "archived": false}));
        } else if path.ends_with(".jsonl") && path.contains(&projects) {
            let profile = ospath::dirname(&ospath::dirname(&ospath::dirname(path)));
            let data = &path[..path.len() - ".jsonl".len()];
            copies.push(json!({"agent": Agent::Claude.key(), "id": ospath::basename(data), "profile": profile, "file": path,
                               "data_folder": if ospath::isdir(data) { Value::from(data) } else { Value::Null },
                               "archived": false}));
        } else if let Some(id) = paths::codex_file_id(path) {
            let archived = ospath::basename(&ospath::dirname(path)) == "archived_sessions";
            let mut home = ospath::dirname(&ospath::dirname(path));
            if !archived {
                for _ in 0..3 {
                    home = ospath::dirname(&home);
                }
            }
            copies.push(json!({"agent": Agent::Codex.key(), "id": id, "profile": home, "file": path, "data_folder": null,
                               "archived": archived}));
        } else if let Some(id) = crate::agy::file_id(path) {
            let root = ospath::dirname(&ospath::dirname(path));
            let brain = crate::agy::brain_path(&root, id);
            copies.push(json!({"agent": Agent::Agy.key(), "id": id, "profile": root, "file": path,
                               "data_folder": if ospath::isdir(&brain) { Value::from(brain) } else { Value::Null },
                               "archived": false}));
        }
    }
    // ZCode's sessions are rows, so they come from the database, not from the paths.
    for root in paths::find_zcode_roots() {
        for row in crate::zcode::rows_starting_with(&root, prefix) {
            let artifacts = crate::zcode::artifacts_path(&root, &row.id);
            copies.push(json!({"agent": Agent::Zcode.key(), "id": row.id, "profile": root, "file": crate::zcode::db_path(&root),
                               "data_folder": if ospath::isdir(&artifacts) { Value::from(artifacts) } else { Value::Null },
                               "archived": row.archived}));
        }
    }
    for home in paths::find_dsh_homes() {
        for x in crate::dsh::sessions_starting_with(&home, prefix) {
            copies.push(
                json!({"agent": Agent::Dsh.key(), "id": x.id, "profile": home, "file": x.log,
                               "data_folder": x.dir, "archived": false}),
            );
        }
    }
    for home in paths::find_mcode_homes() {
        for x in crate::mcode::sessions_starting_with(&home, prefix) {
            copies.push(
                json!({"agent": Agent::Mcode.key(), "id": x.row.id, "profile": home, "file": x.messages,
                               "data_folder": x.dir, "archived": x.row.archived}),
            );
        }
    }
    json!({"version": 1, "query": prefix, "sessions": copies})
}

pub fn main(argv: &[String], prog: &str) -> i32 {
    let parser = Parser::new(
        prog,
        "Find a Claude Code, Codex, Antigravity CLI, Kimi Code CLI, ZCode, DeepSeek Harness or MiniMax Code session by its ID, or the start of it.",
    )
    .positional("session", Count::One, "the session ID, or the start of it")
    .flag(&["--json"], "json", "print the matches as JSON");
    let args = match parser.parse(argv) {
        Ok(a) => a,
        Err(stop) => return stop.exit_code(),
    };
    let prefix = args.value("session").unwrap_or("");
    let found = find(prefix);
    if args.flag("json") {
        out::println(&py::dumps(&as_json(&found, prefix)));
    } else {
        for path in &found {
            out::println(path);
        }
    }
    i32::from(found.is_empty()) // 1: nothing matched
}
