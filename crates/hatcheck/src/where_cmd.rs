//! hatcheck where SESSION [--only AGENT] [--ignore AGENT] [--json]: find a session by its ID,
//! or the start of it, in the selected agents. Reports
//! each agent's transcript or readable store and any session-owned data folder; for DSH, the
//! selected v4 log and its session directory, for mcode the active messages.jsonl and session
//! directory, for qwen the transcript, for copilot the session's events file and for opencode
//! the database its rows share. Exits 1 when nothing matches. --json prints copies as data. A
//! session in a folder config.toml leaves out isn't found, as find.rs's lookups don't find one.

use crate::agent::Agent;
use crate::args::{Count, Parser};
use crate::find::left_out;
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
        // A transcript, or the data folder beside it, judged by the transcript's folder.
        matches.retain(|m| {
            let transcript = if m.ends_with(".jsonl") {
                m.clone()
            } else {
                format!("{m}.jsonl")
            };
            !left_out(Agent::Claude, &profile, &transcript)
        });
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
    if crate::config::ignores(crate::agent::Agent::Codex) {
        homes.clear(); // config.toml or the command's agent filter leaves Codex out
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
            if paths::codex_file_id(&path).is_some_and(|id| id.starts_with(&lower))
                && !left_out(Agent::Codex, &h, &path)
            {
                found.push(path);
            }
        }
    }
    for root in paths::find_agy_stores() {
        for row in crate::agy::rows_starting_with(&root, prefix) {
            let conversation = crate::agy::conversation_path(&root, &row.id);
            if left_out(Agent::Agy, &root, &conversation) {
                continue;
            }
            found.push(conversation);
            let brain = crate::agy::brain_path(&root, &row.id);
            if ospath::isdir(&brain) {
                found.push(brain);
            }
        }
    }
    for home_dir in paths::find_kimi_homes() {
        for x in crate::kimi::sessions_starting_with(&home_dir, prefix) {
            if !left_out(Agent::Kimi, &home_dir, &x.dir) {
                found.push(x.dir);
            }
        }
    }
    // A ZCode session is a row: its database, then its folder of tool results when it has one.
    for root in paths::find_zcode_roots() {
        for row in crate::zcode::rows_starting_with(&root, prefix) {
            let artifacts = crate::zcode::artifacts_path(&root, &row.id);
            if left_out(Agent::Zcode, &root, &artifacts) {
                continue;
            }
            found.push(crate::zcode::db_path(&root));
            if ospath::isdir(&artifacts) {
                found.push(artifacts);
            }
        }
    }
    for home in paths::find_dsh_homes() {
        for x in crate::dsh::sessions_starting_with(&home, prefix) {
            if !left_out(Agent::Dsh, &home, &x.dir) {
                found.push(x.dir);
            }
        }
    }
    for home in paths::find_mcode_homes() {
        for x in crate::mcode::sessions_starting_with(&home, prefix) {
            if !left_out(Agent::Mcode, &home, &x.dir) {
                found.push(x.dir);
            }
        }
    }
    for home in paths::find_qwen_homes() {
        for x in crate::qwen::sessions_starting_with(&home, prefix) {
            if !left_out(Agent::Qwen, &home, &x.file) {
                found.push(x.file);
            }
        }
    }
    for home in paths::find_copilot_homes() {
        for x in crate::copilot::sessions_starting_with(&home, prefix) {
            if !left_out(Agent::Copilot, &home, &x.file) {
                found.push(x.file);
            }
        }
    }
    // An opencode session is a row: its database, once per match, as ZCode's.
    for home in paths::find_opencode_homes() {
        for x in crate::opencode::sessions_starting_with(&home, prefix) {
            let reference = crate::opencode::session_ref(&home, &x.row.id);
            if !left_out(Agent::Opencode, &home, &reference) {
                found.push(crate::opencode::db_path(&home));
            }
        }
    }
    crate::facts::save_cache();
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
        } else if let Some(id) = crate::qwen::file_id(path) {
            // Under a chats folder: qwen's, where Claude Code's are the project folder's own.
            let archived = ospath::basename(&ospath::dirname(path)) == "archive";
            copies.push(json!({"agent": Agent::Qwen.key(), "id": id, "profile": crate::qwen::home_of(path), "file": path,
                               "data_folder": null, "archived": archived}));
        } else if let Some(id) = crate::copilot::file_id(path) {
            copies.push(json!({"agent": Agent::Copilot.key(), "id": id, "profile": crate::copilot::home_of(path), "file": path,
                               "data_folder": ospath::dirname(path), "archived": false}));
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
            if left_out(Agent::Zcode, &root, &artifacts) {
                continue;
            }
            copies.push(json!({"agent": Agent::Zcode.key(), "id": row.id, "profile": root, "file": crate::zcode::db_path(&root),
                               "data_folder": if ospath::isdir(&artifacts) { Value::from(artifacts) } else { Value::Null },
                               "archived": row.archived}));
        }
    }
    for home in paths::find_dsh_homes() {
        for x in crate::dsh::sessions_starting_with(&home, prefix) {
            if left_out(Agent::Dsh, &home, &x.dir) {
                continue;
            }
            copies.push(
                json!({"agent": Agent::Dsh.key(), "id": x.id, "profile": home, "file": x.log,
                               "data_folder": x.dir, "archived": false}),
            );
        }
    }
    for home in paths::find_mcode_homes() {
        for x in crate::mcode::sessions_starting_with(&home, prefix) {
            if left_out(Agent::Mcode, &home, &x.dir) {
                continue;
            }
            copies.push(
                json!({"agent": Agent::Mcode.key(), "id": x.row.id, "profile": home, "file": x.messages,
                               "data_folder": x.dir, "archived": x.row.archived}),
            );
        }
    }
    // opencode's sessions are rows too, so they come from the database.
    for home in paths::find_opencode_homes() {
        for x in crate::opencode::sessions_starting_with(&home, prefix) {
            if left_out(
                Agent::Opencode,
                &home,
                &crate::opencode::session_ref(&home, &x.row.id),
            ) {
                continue;
            }
            copies.push(
                json!({"agent": Agent::Opencode.key(), "id": x.row.id, "profile": home, "file": crate::opencode::db_path(&home),
                               "data_folder": null, "archived": x.row.archived}),
            );
        }
    }
    json!({"version": 1, "query": prefix, "sessions": copies})
}

pub fn main(argv: &[String], prog: &str) -> i32 {
    let parser = crate::agent::AgentSelection::options(Parser::new(
        prog,
        "Find a Claude Code, Codex, Antigravity CLI, Kimi Code CLI, ZCode, DeepSeek Harness, MiniMax Code, Qwen Code, GitHub Copilot CLI or OpenCode session by its ID, or the start of it.",
    ))
    .positional("session", Count::One, "the session ID, or the start of it")
    .flag(&["-j", "--json"], "json", "print the matches as JSON");
    let args = match parser.parse(argv) {
        Ok(a) => a,
        Err(stop) => return stop.exit_code(),
    };
    let selection = match crate::agent::AgentSelection::parse(&args) {
        Ok(s) => s,
        Err(e) => return parser.error(&e).exit_code(),
    };
    crate::config::select_agents(selection);
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
