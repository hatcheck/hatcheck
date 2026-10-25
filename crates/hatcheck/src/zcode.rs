//! ZCode (zcode) as hatcheck reads it. ZCode keeps its sessions in one SQLite database,
//! ~/.zcode/cli/db/db.sqlite, whose path only HOME moves, and shares it with the ZCode desktop
//! app: a `session` row per session (sess_<uuid>, the folder it ran in, its title and times),
//! its `message` rows, and the `part` rows of each (text, reasoning, a tool call with its input
//! and output). Beside the database, cli/artifacts/sess_<id>/ holds tool results spilled to
//! files, and v2/ the desktop app's own files, with the login, which hatcheck never reads.
//! Nothing marks a session open. hatcheck reads the database and never writes it: a session
//! converted for zcode is written as a file zcode is started on. docs/agents.md's `ZCode`
//! section says what is read, and that it was checked against ZCode 0.16.9 and compared with
//! 0.17.0, which keeps a deleted session's row and marks it.

use crate::agent::Agent;
use crate::conv::{self, Conversation, Turn};
use crate::facts::Summary;
use crate::ospath;
use crate::paths;
use crate::py::{self, get, s, truthy, Num};
use crate::times;
use rusqlite::{Connection, OpenFlags};
use serde_json::{json, Value};

/// The database, under the data folder.
const DB: [&str; 3] = ["cli", "db", "db.sqlite"];
/// Tool results spilled to files, one folder per session.
const ARTIFACTS: [&str; 2] = ["cli", "artifacts"];

/// ~/.zcode.
pub fn default_root() -> String {
    ospath::join(&paths::home(), Agent::Zcode.base())
}

/// Whether a folder is ZCode's data folder: it has the session database.
pub fn is_root(path: &str) -> bool {
    ospath::isfile(&db_path(path))
}

/// Every ZCode data folder with sessions: the default one, since only HOME moves it.
pub fn find_roots() -> Vec<String> {
    let root = default_root();
    if is_root(&root) {
        vec![root]
    } else {
        Vec::new()
    }
}

pub fn db_path(root: &str) -> String {
    ospath::join_all(root, &DB)
}

/// The folder of a session's spilled tool results, which may not exist: the one path of a
/// session's own, used to name it.
pub fn artifacts_path(root: &str, id: &str) -> String {
    ospath::join_all(
        root,
        &[ARTIFACTS[0], ARTIFACTS[1], &Agent::Zcode.full_id(id)],
    )
}

/// The ID in a session's artifacts path, cli/artifacts/sess_<id>.
pub fn file_id(path: &str) -> Option<&str> {
    let name = path
        .trim_end_matches(ospath::is_sep)
        .rsplit(['/', '\\'])
        .next()?;
    let id = name.strip_prefix(Agent::Zcode.id_prefix())?;
    let hex = |c: char| c.is_ascii_hexdigit() || c == '-';
    (id.len() == 36 && id.chars().all(hex)).then_some(id)
}

/// The data folder an artifacts path is under: two folders up from cli/artifacts/sess_<id>.
pub fn root_of(path: &str) -> String {
    let trimmed = path.trim_end_matches(ospath::is_sep);
    ospath::dirname(&ospath::dirname(&ospath::dirname(trimmed)))
}

/// One session as its row has it.
#[derive(Clone, Debug, Default)]
pub struct Row {
    /// The UUID after sess_.
    pub id: String,
    pub title: String,
    pub directory: String,
    /// Seconds since the epoch (the rows keep ms).
    pub created: f64,
    pub updated: f64,
    pub archived: bool,
    /// interactive for one you ran; subagent_child and the like for ZCode's own.
    pub task_type: String,
    /// The zcode that wrote it.
    pub version: String,
}

fn open(root: &str) -> rusqlite::Result<Connection> {
    Connection::open_with_flags(
        db_path(root),
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
}

fn text(v: rusqlite::types::Value) -> String {
    match v {
        rusqlite::types::Value::Text(t) => t,
        rusqlite::types::Value::Integer(n) => n.to_string(),
        rusqlite::types::Value::Real(x) => x.to_string(),
        _ => String::new(),
    }
}

fn seconds(v: rusqlite::types::Value) -> f64 {
    match v {
        rusqlite::types::Value::Integer(n) => n as f64 / 1000.0,
        rusqlite::types::Value::Real(x) => x / 1000.0,
        rusqlite::types::Value::Text(t) => t.trim().parse::<f64>().map_or(0.0, |x| x / 1000.0),
        _ => 0.0,
    }
}

const COLUMNS: &str =
    "id, title, directory, time_created, time_updated, time_archived, task_type, version";

fn row_of(r: &rusqlite::Row) -> rusqlite::Result<Row> {
    let v = |i: usize| r.get::<_, rusqlite::types::Value>(i);
    let full = text(v(0)?);
    Ok(Row {
        id: Agent::Zcode.bare_id(&full).to_string(),
        title: text(v(1)?),
        directory: text(v(2)?),
        created: seconds(v(3)?),
        updated: seconds(v(4)?),
        archived: !matches!(v(5)?, rusqlite::types::Value::Null),
        task_type: text(v(6)?),
        version: text(v(7)?),
    })
}

/// Every session of your own in the database, the newest change first: the rows with no
/// parent, which leaves out the sessions ZCode runs itself (subagents), and none ZCode has
/// deleted; none when the database can't be read.
pub fn rows(root: &str) -> Vec<Row> {
    let Ok(db) = open(root) else {
        return Vec::new();
    };
    // ZCode 0.17.0 deletes a session by setting time_deleted, a column older databases lack,
    // and keeps the row; its own lists leave those out.
    let soft_delete = db
        .query_row(
            "SELECT count(*) FROM pragma_table_info('session') WHERE name = 'time_deleted'",
            [],
            |r| r.get::<_, i64>(0),
        )
        .is_ok_and(|n| n > 0);
    let kept = if soft_delete {
        " AND time_deleted IS NULL"
    } else {
        ""
    };
    let Ok(mut query) = db.prepare(&format!(
        "SELECT {COLUMNS} FROM session WHERE (parent_id IS NULL OR parent_id = ''){kept} \
         ORDER BY time_updated DESC, id"
    )) else {
        return Vec::new();
    };
    let Ok(found) = query.query_map([], row_of) else {
        return Vec::new();
    };
    found.flatten().collect()
}

/// The sessions whose ID starts with prefix, the agent's own prefix left off.
pub fn rows_starting_with(root: &str, prefix: &str) -> Vec<Row> {
    let lower = Agent::Zcode.bare_id(prefix).to_lowercase();
    rows(root)
        .into_iter()
        .filter(|r| r.id.starts_with(&lower))
        .collect()
}

pub fn row(root: &str, id: &str) -> Option<Row> {
    rows(root).into_iter().find(|r| r.id == id)
}

/// How the session list files a session: cli for one you ran, else what ZCode calls it.
pub fn entrypoint(row: &Row) -> String {
    match row.task_type.as_str() {
        "" | "interactive" => "cli".into(),
        other if other.contains("subagent") => "subagent".into(),
        other => other.to_string(),
    }
}

/// A session as the session list shows it.
pub fn summary(root: &str, row: &Row) -> Summary {
    let title = py::collapse(&row.title);
    let when = if row.updated != 0.0 {
        row.updated
    } else {
        row.created
    };
    Summary {
        agent: Agent::Zcode,
        id: row.id.clone(),
        when,
        entrypoint: json!(entrypoint(row)),
        branch: Value::Null,
        cwd: Some(row.directory.clone()).filter(|d| !d.is_empty()),
        canonical: true,
        size: 0,
        has_messages: !title.is_empty(),
        title: if title.is_empty() {
            "(untitled)".into()
        } else {
            title
        },
        continued_in: Value::Null,
        model: Value::Null,
        profile: root.to_string(),
        path: artifacts_path(root, &row.id),
        open: None,
        bound_to: None,
    }
}

/// The summary of the session an artifacts path names, or None when the database doesn't
/// list it.
pub fn summarize(root: &str, path: &str) -> Option<Summary> {
    let id = file_id(path)?;
    row(root, id).map(|r| summary(root, &r))
}

/// A time in ms, as the rows keep them, as text for a turn.
fn stamp(v: &Value) -> Value {
    match Num::of(v) {
        Some(n) if n.f() != 0.0 => {
            times::iso_utc(n.f() / 1000.0).map_or(Value::Null, Value::String)
        }
        _ => Value::Null,
    }
}

/// A conversation as hatcheck shows and converts it, from the session's message and part
/// rows: what you typed, what the model said, each tool call as text with its input and
/// output. ZCode's reasoning is left out, as the other agents' is.
pub fn read_conversation(root: &str, id: &str) -> std::io::Result<Conversation> {
    let io = |e: rusqlite::Error| std::io::Error::other(e.to_string());
    let db = open(root).map_err(io)?;
    let full = Agent::Zcode.full_id(id);
    let mut messages = db
        .prepare(
            "SELECT id, data FROM message WHERE session_id = ?1 \
             ORDER BY time_created, COALESCE(sequence, 0), id",
        )
        .map_err(io)?;
    let mut parts = db
        .prepare(
            "SELECT data FROM part WHERE message_id = ?1 \
             ORDER BY COALESCE(sequence, 0), time_created, id",
        )
        .map_err(io)?;
    let found: Vec<(String, String)> = messages
        .query_map([&full], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })
        .map_err(io)?
        .flatten()
        .collect();
    let (mut turns, mut calls, mut model): (Vec<Turn>, usize, Value) = (Vec::new(), 0, Value::Null);
    for (mid, data) in found {
        let message: Value = serde_json::from_str(&data).unwrap_or(Value::Null);
        let role = match s(get(&message, "role")) {
            Some("user") => "user",
            Some("assistant") => "assistant",
            _ => continue,
        };
        let when = stamp(get(get(&message, "time"), "created"));
        if role == "assistant" && truthy(get(&message, "modelID")) {
            model = get(&message, "modelID").clone();
        }
        let list: Vec<String> = parts
            .query_map([&mid], |r| r.get::<_, String>(0))
            .map_err(io)?
            .flatten()
            .collect();
        for text in list {
            let part: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
            match s(get(&part, "type")) {
                Some("text") => {
                    conv::add_turn(&mut turns, role, s(get(&part, "text")).unwrap_or(""), &when)
                }
                Some("tool") => {
                    let name = s(get(&part, "tool")).unwrap_or("tool");
                    let state = get(&part, "state");
                    let fields: Vec<(String, Value)> = match get(state, "input") {
                        Value::Object(o) => o.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
                        Value::Null => Vec::new(),
                        other => vec![("input".into(), other.clone())],
                    };
                    conv::add_turn(
                        &mut turns,
                        "assistant",
                        &conv::call_text(name, &fields),
                        &when,
                    );
                    calls += 1;
                    let output = match get(state, "output") {
                        Value::String(t) => t.clone(),
                        Value::Null => s(get(state, "error")).unwrap_or("").to_string(),
                        other => py::dumps_compact(other),
                    };
                    if !output.is_empty() {
                        conv::add_turn(&mut turns, "assistant", &conv::result_text(&output), &when);
                    }
                }
                _ => {} // reasoning, step markers, ZCode's own timeline
            }
        }
    }
    let row = row(root, id).unwrap_or_default();
    Ok(Conversation {
        agent: Agent::Zcode,
        id: id.to_string(),
        cwd: Some(row.directory.clone()).filter(|d| !d.is_empty()),
        model,
        turns,
        calls,
        title: Some(py::collapse(&row.title)).filter(|t| !t.is_empty()),
        started: if row.created != 0.0 {
            times::iso_utc(row.created).map_or(Value::Null, Value::String)
        } else {
            Value::Null
        },
        branch: Value::Null,
        reasoning: "ZCode's reasoning",
        left_out: "ZCode's reasoning, images, checkpoints and subagent sessions",
    })
}

// A conversation converted for zcode: written as a file for zcode to read, since hatcheck
// never writes ZCode's database (docs/proposals/kimi-zcode.md says why).

/// The first prompt zcode gets with an imported conversation: read the attached file, and say
/// where it left off; the next prompt, in the TUI, carries it on. zcode titles the session by
/// its first words, so they say what it is.
pub fn prompt() -> String {
    "Imported conversation: the attached file is a conversation imported from another coding agent. Read it, say in a few lines where it left off and whether its last action completed, and wait: the next prompt carries it on.".into()
}

/// The command that starts zcode on an imported conversation, from the session's folder: one
/// prompt in print mode, with the file attached and plan mode so nothing is changed, which
/// persists the session; `zcode -c` in the folder then carries it on.
pub fn start_command(root: &str, file: &str, full: bool) -> String {
    paths::zcode_command(
        root,
        &format!(
            "--attach {} --mode plan -p {}",
            paths::shell_path(file),
            paths::quote(&prompt())
        ),
        full,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn artifacts_paths_name_the_session() {
        let id = "1fb068a4-b6d4-4a9f-b2d3-6e428e094fbe";
        let path = format!("/h/.zcode/cli/artifacts/sess_{id}");
        assert_eq!(file_id(&path), Some(id));
        assert_eq!(file_id(&format!("{path}/")), Some(id));
        assert_eq!(root_of(&path), "/h/.zcode");
        assert_eq!(
            file_id("/h/.zcode/cli/artifacts/sess_subagent_agent_x"),
            None
        );
        assert_eq!(file_id("/h/.zcode/cli/db/db.sqlite"), None);
        assert_eq!(artifacts_path("/h/.zcode", id).replace('\\', "/"), path);
    }

    #[test]
    fn task_types_become_entrypoints() {
        let mut r = Row::default();
        assert_eq!(entrypoint(&r), "cli");
        r.task_type = "interactive".into();
        assert_eq!(entrypoint(&r), "cli");
        r.task_type = "subagent_child".into();
        assert_eq!(entrypoint(&r), "subagent");
        r.task_type = "goal".into();
        assert_eq!(entrypoint(&r), "goal");
    }
}
