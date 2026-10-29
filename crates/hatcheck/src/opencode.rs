//! OpenCode (opencode) as hatcheck reads it. OpenCode 2 keeps its data in the XDG data folder,
//! ~/.local/share/opencode (XDG_DATA_HOME's opencode when that is set): opencode.db, a SQLite
//! database with every project's sessions (session_v2), their messages (session_message, a JSON
//! record each: a user or assistant message, a shell command, a compaction or a record of
//! opencode's own) and its provider logins (credential, account), which hatcheck never reads,
//! with its logs, shell history and repository cache beside it. Its settings are in
//! ~/.config/opencode, and the background service it runs through registers under
//! ~/.local/state/opencode. The V1 store, storage/ beside the database, is what opencode 2
//! imports on its first run, and isn't read. hatcheck opens the database read-only and writes
//! nothing: a session converted for opencode is a file for its own `opencode session import`.
//! docs/agents.md's `OpenCode` section says what is read, and that it was checked against
//! opencode 2.1.4.

use crate::agent::Agent;
use crate::conv::{self, Conversation, Turn};
use crate::facts::{self, Summary};
use crate::ospath;
use crate::paths;
use crate::py::{self, get, s, truthy};
use crate::times;
use rusqlite::{Connection, OpenFlags};
use serde_json::{json, Value};
use std::collections::HashMap;

/// The data folder plain opencode uses: $XDG_DATA_HOME/opencode, or ~/.local/share/opencode.
pub fn default_home() -> String {
    let data = std::env::var("XDG_DATA_HOME")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .map(|v| ospath::expanduser(&v))
        .unwrap_or_else(|| ospath::join_all(&paths::home(), &[".local", "share"]));
    ospath::join(&data, "opencode")
}

pub fn db_path(home: &str) -> String {
    ospath::join(home, "opencode.db")
}

/// Whether a folder is an opencode data folder: it has the database opencode 2 makes on its
/// first run. ~/.opencode, where its installer puts the program, isn't one.
pub fn is_home(path: &str) -> bool {
    ospath::isfile(&db_path(path))
}

/// Every opencode data folder: the one plain opencode uses, when opencode has run there. A
/// second data folder is a product decision not taken yet: see agent.rs.
pub fn find_homes() -> Vec<String> {
    let home = default_home();
    if is_home(&home) {
        vec![ospath::abspath(&home)]
    } else {
        Vec::new()
    }
}

fn open(home: &str) -> rusqlite::Result<Connection> {
    Connection::open_with_flags(
        db_path(home),
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

/// A number as the rows keep it: a real, an integer or text.
fn number(v: rusqlite::types::Value) -> f64 {
    match v {
        rusqlite::types::Value::Integer(n) => n as f64,
        rusqlite::types::Value::Real(x) => x,
        rusqlite::types::Value::Text(t) => t.trim().parse().unwrap_or(0.0),
        _ => 0.0,
    }
}

/// A time in milliseconds, as the rows keep them, in seconds.
fn seconds(v: rusqlite::types::Value) -> f64 {
    number(v) / 1000.0
}

/// A model reference as the rows keep it, {"providerID": …, "id": …}, as provider/model.
fn model_text(value: &Value) -> Value {
    let (provider, id) = (s(get(value, "providerID")), s(get(value, "id")));
    match (provider, id) {
        (Some(p), Some(m)) if !p.is_empty() => json!(format!("{p}/{m}")),
        (_, Some(m)) if !m.is_empty() => json!(m),
        _ => Value::Null,
    }
}

/// One session_v2 row.
#[derive(Clone, Debug, Default)]
pub struct Row {
    pub id: String,
    pub parent: String,
    pub directory: String,
    pub title: String,
    pub version: String,
    pub agent: String,
    pub model: Value,
    pub created: f64,
    pub updated: f64,
    pub archived: bool,
    /// What the session cost in dollars as opencode prices its tokens: 0 for a model it has no
    /// price for.
    pub cost: f64,
    /// The tokens it spent, on every model together: input, output, reasoning, cache read and
    /// cache write.
    pub tokens: [u64; 5],
}

const COLUMNS: &str = "id, parent_id, directory, title, version, agent, model, time_created, \
                       time_updated, time_archived, cost, tokens_input, tokens_output, \
                       tokens_reasoning, tokens_cache_read, tokens_cache_write";

fn row_of(r: &rusqlite::Row) -> rusqlite::Result<Row> {
    let v = |i: usize| r.get::<_, rusqlite::types::Value>(i);
    let model = serde_json::from_str::<Value>(&text(v(6)?))
        .map(|m| model_text(&m))
        .unwrap_or(Value::Null);
    Ok(Row {
        id: Agent::Opencode.bare_id(&text(v(0)?)).to_string(),
        parent: text(v(1)?),
        directory: text(v(2)?),
        title: text(v(3)?),
        version: text(v(4)?),
        agent: text(v(5)?),
        model,
        created: seconds(v(7)?),
        updated: seconds(v(8)?),
        archived: !matches!(v(9)?, rusqlite::types::Value::Null),
        cost: number(v(10)?),
        tokens: [
            number(v(11)?).max(0.0) as u64,
            number(v(12)?).max(0.0) as u64,
            number(v(13)?).max(0.0) as u64,
            number(v(14)?).max(0.0) as u64,
            number(v(15)?).max(0.0) as u64,
        ],
    })
}

/// Every session row, the newest activity first.
pub fn rows(home: &str) -> Vec<Row> {
    let Ok(db) = open(home) else {
        return Vec::new();
    };
    let Ok(mut query) = db.prepare(&format!(
        "SELECT {COLUMNS} FROM session_v2 ORDER BY time_updated DESC, time_created DESC, id"
    )) else {
        return Vec::new();
    };
    let Ok(found) = query.query_map([], row_of) else {
        return Vec::new();
    };
    found.flatten().collect()
}

/// How the list files a session: a child session, which a subagent ran, is a subagent's; the
/// rest are the terminal's.
pub fn entrypoint(row: &Row) -> String {
    if row.parent.is_empty() {
        "cli".into()
    } else {
        "subagent".into()
    }
}

/// A session: its row, and what its messages say.
#[derive(Clone, Debug)]
pub struct Session {
    pub row: Row,
    pub first_prompt: Option<String>,
    pub has_messages: bool,
    /// The size of its messages, in bytes.
    pub size: u64,
}

impl Session {
    pub fn id(&self) -> &str {
        &self.row.id
    }
}

/// Every session in a data folder, the newest activity first, with the first prompt and the
/// size of the messages of each.
pub fn sessions(home: &str) -> Vec<Session> {
    let rows = rows(home);
    if rows.is_empty() {
        return Vec::new();
    }
    let mut first: HashMap<String, String> = HashMap::new();
    let mut counts: HashMap<String, (i64, i64)> = HashMap::new();
    if let Ok(db) = open(home) {
        if let Ok(mut query) = db.prepare(
            "SELECT session_id, data FROM session_message WHERE type = 'user' ORDER BY session_id, seq",
        ) {
            if let Ok(found) = query.query_map([], |r| {
                Ok((
                    text(r.get::<_, rusqlite::types::Value>(0)?),
                    text(r.get::<_, rusqlite::types::Value>(1)?),
                ))
            }) {
                for (sid, data) in found.flatten() {
                    first.entry(sid).or_insert_with(|| {
                        serde_json::from_str::<Value>(&data)
                            .ok()
                            .and_then(|d| s(get(&d, "text")).map(String::from))
                            .unwrap_or_default()
                    });
                }
            }
        }
        if let Ok(mut query) = db.prepare(
            "SELECT session_id, COUNT(*), COALESCE(SUM(LENGTH(data)), 0) FROM session_message \
             WHERE type IN ('user', 'assistant') GROUP BY session_id",
        ) {
            if let Ok(found) = query.query_map([], |r| {
                Ok((
                    text(r.get::<_, rusqlite::types::Value>(0)?),
                    r.get::<_, i64>(1)?,
                    r.get::<_, i64>(2)?,
                ))
            }) {
                for (sid, n, size) in found.flatten() {
                    counts.insert(sid, (n, size));
                }
            }
        }
    }
    rows.into_iter()
        .map(|row| {
            let full = Agent::Opencode.full_id(&row.id);
            let (n, size) = counts.get(&full).copied().unwrap_or((0, 0));
            Session {
                first_prompt: first
                    .get(&full)
                    .cloned()
                    .filter(|t| !py::strip(t).is_empty()),
                has_messages: n > 0,
                size: u64::try_from(size).unwrap_or(0),
                row,
            }
        })
        .collect()
}

/// The sessions whose ID starts with prefix, pasted with or without ses_, in any case.
pub fn sessions_starting_with(home: &str, prefix: &str) -> Vec<Session> {
    let lower = prefix.to_lowercase();
    let bare = Agent::Opencode.bare_id(&lower);
    sessions(home)
        .into_iter()
        .filter(|x| x.row.id.to_lowercase().starts_with(bare))
        .collect()
}

pub fn session(home: &str, id: &str) -> Option<Session> {
    let lower = id.to_lowercase();
    let bare = Agent::Opencode.bare_id(&lower);
    sessions(home)
        .into_iter()
        .find(|x| x.row.id.to_lowercase() == bare)
}

/// A name for a session's row, where every other agent has a file: <the database>/ses_<id>.
/// Nothing is at that path; it says which row, as ZCode's artifacts path names a row of its
/// own, and what the commands show is the database.
pub fn session_ref(home: &str, id: &str) -> String {
    ospath::join(&db_path(home), &Agent::Opencode.full_id(id))
}

/// The bare ID a session's reference names, when the path is one.
pub fn ref_id(path: &str) -> Option<String> {
    let name = ospath::basename(path);
    let id = name.strip_prefix(Agent::Opencode.id_prefix())?;
    (!id.is_empty() && ospath::basename(&ospath::dirname(path)) == "opencode.db")
        .then(|| id.to_string())
}

/// The summary of the session a reference names, or None when the path isn't one.
pub fn summarize(home: &str, path: &str) -> Option<Summary> {
    let id = ref_id(path)?;
    session(home, &id).map(|x| summary(home, &x))
}

/// A session as the list shows it.
pub fn summary(home: &str, x: &Session) -> Summary {
    let title = Some(x.row.title.clone())
        .filter(|t| !t.trim().is_empty())
        .or_else(|| x.first_prompt.clone())
        .map(|t| py::collapse(&t))
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| "(untitled)".into());
    Summary {
        agent: Agent::Opencode,
        id: x.row.id.clone(),
        when: x.row.updated,
        entrypoint: json!(entrypoint(&x.row)),
        branch: Value::Null,
        cwd: Some(x.row.directory.clone()).filter(|d| !d.is_empty()),
        canonical: true,
        size: x.size,
        has_messages: x.has_messages,
        title,
        continued_in: Value::Null,
        model: x.row.model.clone(),
        profile: home.to_string(),
        path: db_path(home),
        open: None,
        bound_to: None,
        cost: facts::cost_of(x.row.cost),
        spend: spend(&x.row),
    }
}

/// The tokens a row counts, on every model together, as one line of --details; None when it
/// counts none.
fn spend(row: &Row) -> Option<facts::Spend> {
    let [input, output, reasoning, cache_read, cache_write] = row.tokens;
    let tokens = facts::ModelSpend {
        model: None,
        usd: None,
        input,
        output,
        thinking: reasoning,
        cache_read,
        cache_write,
        web_searches: 0,
    };
    (tokens.tokens() > 0).then(|| facts::Spend {
        models: vec![tokens],
        ..Default::default()
    })
}

/// A time in milliseconds, as the rows keep them, as text for a turn.
fn stamp(ms: f64) -> Value {
    if ms == 0.0 {
        return Value::Null;
    }
    times::iso_utc(ms / 1000.0).map_or(Value::Null, Value::String)
}

/// The text of a tool's content list: its text parts, a file marked where it was.
fn content_text(content: &Value) -> String {
    match content {
        Value::String(t) => t.clone(),
        Value::Array(parts) => parts
            .iter()
            .map(|p| match s(get(p, "type")) {
                Some("text") => py::str_of(get(p, "text")),
                Some(_) => "[file]".into(),
                None => py::str_of(p),
            })
            .filter(|t| !t.is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

/// The conversation, from the session's message records in order: your messages, the replies'
/// text and tool calls with their results, and the shell commands and compaction summaries;
/// reasoning, files' contents and opencode's own records left out.
fn conversation(home: &str, x: &Session) -> std::io::Result<Conversation> {
    let db = open(home).map_err(|e| std::io::Error::other(e.to_string()))?;
    let full = Agent::Opencode.full_id(&x.row.id);
    let mut query = db
        .prepare("SELECT type, time_created, data FROM session_message WHERE session_id = ?1 ORDER BY seq")
        .map_err(|e| std::io::Error::other(e.to_string()))?;
    let found = query
        .query_map([&full], |r| {
            Ok((
                text(r.get::<_, rusqlite::types::Value>(0)?),
                seconds(r.get::<_, rusqlite::types::Value>(1)?) * 1000.0,
                text(r.get::<_, rusqlite::types::Value>(2)?),
            ))
        })
        .map_err(|e| std::io::Error::other(e.to_string()))?;
    let (mut turns, mut calls, mut model) = (Vec::new(), 0, Value::Null);
    for (kind, created, data) in found.flatten() {
        let data: Value = serde_json::from_str(&data).unwrap_or(Value::Null);
        let when = stamp(created);
        match kind.as_str() {
            "user" => {
                let mut parts = Vec::new();
                if let Some(t) = s(get(&data, "text")).filter(|t| !py::strip(t).is_empty()) {
                    parts.push(t.to_string());
                }
                for file in get(&data, "files").as_array().into_iter().flatten() {
                    let mime = s(get(file, "mime")).unwrap_or("");
                    parts.push(
                        if mime.starts_with("image/") {
                            "[image]"
                        } else {
                            "[file]"
                        }
                        .into(),
                    );
                }
                if !parts.is_empty() {
                    conv::add_turn(&mut turns, "user", &parts.join("\n"), &when);
                }
            }
            "assistant" => {
                if truthy(get(&data, "model")) {
                    let m = model_text(get(&data, "model"));
                    if !m.is_null() {
                        model = m;
                    }
                }
                for part in get(&data, "content").as_array().into_iter().flatten() {
                    match s(get(part, "type")) {
                        Some("text") => {
                            if let Some(t) =
                                s(get(part, "text")).filter(|t| !py::strip(t).is_empty())
                            {
                                conv::add_turn(&mut turns, "assistant", t, &when);
                            }
                        }
                        Some("tool") => {
                            let name = s(get(part, "name")).unwrap_or("tool");
                            let state = get(part, "state");
                            let fields: Vec<(String, Value)> = match get(state, "input") {
                                Value::Object(o) => {
                                    o.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
                                }
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
                            let mut output = content_text(get(state, "content"));
                            if output.is_empty() {
                                if let Some(message) = s(get(get(state, "error"), "message")) {
                                    output = format!("error: {message}");
                                }
                            }
                            if !py::strip(&output).is_empty() {
                                conv::add_turn(
                                    &mut turns,
                                    "assistant",
                                    &conv::result_text(&output),
                                    &when,
                                );
                            }
                        }
                        _ => {} // reasoning: opencode's own
                    }
                }
            }
            "shell" => {
                let command = s(get(&data, "command")).unwrap_or("");
                conv::add_turn(
                    &mut turns,
                    "assistant",
                    &conv::call_text("shell", &[("command".into(), json!(command))]),
                    &when,
                );
                calls += 1;
                let output = py::str_of(get(get(&data, "output"), "output"));
                if !py::strip(&output).is_empty() {
                    conv::add_turn(&mut turns, "assistant", &conv::result_text(&output), &when);
                }
            }
            "compaction" if s(get(&data, "status")) == Some("completed") => {
                if let Some(summary) = s(get(&data, "summary")).filter(|t| !py::strip(t).is_empty())
                {
                    conv::add_turn(&mut turns, "assistant", summary, &when);
                }
            }
            _ => {} // synthetic, system, skill, the switches and a compaction under way: opencode's own
        }
    }
    if model.is_null() {
        model = x.row.model.clone();
    }
    let title = Some(x.row.title.clone())
        .filter(|t| !t.trim().is_empty())
        .or_else(|| x.first_prompt.clone())
        .or_else(|| {
            turns
                .iter()
                .find(|t| t.0 == "user")
                .and_then(|t| t.1.first().cloned())
        })
        .map(|t| py::collapse(&t))
        .filter(|t| !t.is_empty());
    Ok(Conversation {
        agent: Agent::Opencode,
        id: x.row.id.clone(),
        cwd: Some(x.row.directory.clone()).filter(|d| !d.is_empty()),
        model,
        turns,
        calls,
        title,
        started: stamp(x.row.created * 1000.0),
        branch: Value::Null,
        reasoning: "OpenCode's reasoning",
        left_out: "OpenCode's reasoning, the contents of files attached or returned, and its own records (skills, what it adds to the conversation itself, and its agent, model and folder switches)",
    })
}

pub fn read_conversation(home: &str, id: &str) -> std::io::Result<Conversation> {
    let x = session(home, id).ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("no session {id} in {home}"),
        )
    })?;
    conversation(home, &x)
}

const ID_CHARS: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

/// An ID as opencode makes them: 12 hex digits of the time in milliseconds and a counter
/// (inverted for a session, so sessions sort newest first), then 14 random characters.
fn identifier(descending: bool, now_ms: u64, counter: u64) -> String {
    let current = (now_ms << 12) | (counter & 0xfff);
    let value = if descending { !current } else { current };
    let mut id = format!("{:012x}", value & 0xffff_ffff_ffff);
    let mut random = [0u8; 14];
    let _ = getrandom::fill(&mut random);
    id.extend(
        random
            .iter()
            .map(|b| ID_CHARS[usize::from(*b) % 62] as char),
    );
    id
}

/// Where a conversation converted for opencode is written: imports/<id>.json in hatcheck's
/// cache folder, the session data `opencode session import` takes.
pub fn import_path(id: &str) -> String {
    ospath::join(&conv::imports_dir(), &format!("{id}.json"))
}

/// Write the conversation as the session data opencode's own `session export` prints and its
/// `session import` takes: the session's info and its messages, a user or assistant message
/// per turn, and the file's path with the ID the session will have, minted as opencode mints
/// them. opencode files it under the folder the import is run in.
pub fn write_session(
    conv: &Conversation,
    turns: &[Turn],
    _home: &str,
    cwd: &str,
) -> std::io::Result<(String, String)> {
    let now = times::now();
    let now_ms = (now * 1000.0).trunc() as u64;
    let id = identifier(true, now_ms, 1);
    let at = |when: &Value| -> u64 {
        let t = s(when)
            .and_then(times::parse_time)
            .filter(|t| *t != 0.0)
            .unwrap_or(now);
        (t * 1000.0).trunc() as u64
    };
    let created = at(&conv.started);
    let model = match s(&conv.model).filter(|m| !m.is_empty()) {
        Some(m) => match m.split_once('/') {
            Some((provider, id)) if !provider.is_empty() && !id.is_empty() => {
                json!({"providerID": provider, "id": id})
            }
            _ => json!({"providerID": conv.agent.key(), "id": m}),
        },
        None => json!({"providerID": conv.agent.key(), "id": conv.agent.key()}),
    };
    let mut messages = Vec::new();
    let mut counter = 1u64;
    for (role, texts, when) in turns {
        counter += 1;
        let text = texts.join("\n\n");
        let t = at(when);
        let msg_id = format!("msg_{}", identifier(false, t, counter));
        messages.push(if role == "user" {
            json!({"id": msg_id, "type": "user", "text": text, "time": {"created": t}})
        } else {
            json!({"id": msg_id, "type": "assistant", "agent": "build", "model": model,
                   "content": [{"type": "text", "text": text}], "time": {"created": t, "completed": t}})
        });
    }
    let mut info = json!({
        "id": Agent::Opencode.full_id(&id), "projectID": "global", "cost": 0,
        "tokens": {"input": 0, "output": 0, "reasoning": 0, "cache": {"read": 0, "write": 0}},
        "time": {"created": created, "updated": now_ms},
        "location": {"directory": cwd},
        "metadata": {"imported_with_hatcheck": true, "imported_from": format!("{} session {}", conv.agent.name(), conv.id),
                     "imported_at": crate::move_cmd::iso_ms(now)},
    });
    if let Some(title) = conv
        .title
        .as_deref()
        .map(py::collapse)
        .filter(|t| !t.is_empty())
    {
        info["title"] = json!(title);
    }
    let data = json!({"info": info, "messages": messages});
    let path = import_path(&id);
    std::fs::create_dir_all(conv::imports_dir())?;
    let tmp = format!("{path}.hatcheck-tmp");
    std::fs::write(&tmp, py::dumps(&data) + "\n")?;
    std::fs::rename(&tmp, &path)?;
    Ok((id, path))
}

/// The command that imports a converted session into a data folder: opencode's own importer,
/// run from the session's folder, where opencode files it.
pub fn start_command(home: &str, file: &str, full: bool) -> String {
    paths::opencode_command(
        home,
        &format!("session import {}", paths::shell_path(file)),
        full,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "7f3a2b1c0d9e8f7a6b5c4d3e2f1a0b9c";

    fn store(dir: &std::path::Path) -> String {
        std::fs::create_dir_all(dir).unwrap();
        let home = dir.to_string_lossy().into_owned();
        let db = Connection::open(db_path(&home)).unwrap();
        db.execute_batch(
            "CREATE TABLE project (id text PRIMARY KEY, worktree text NOT NULL, vcs text, name text, time_created integer NOT NULL, time_updated integer NOT NULL, time_active integer DEFAULT 0 NOT NULL, time_archived integer, sandboxes text NOT NULL, commands text);
             CREATE TABLE session_v2 (id text PRIMARY KEY, project_id text NOT NULL, workspace_id text, parent_id text, fork_session_id text, fork_boundary text, slug text NOT NULL, directory text NOT NULL, path text, title text, version text NOT NULL, share_url text, summary_additions integer, summary_deletions integer, summary_files integer, summary_diffs text, metadata text, cost real DEFAULT 0 NOT NULL, tokens_input integer DEFAULT 0 NOT NULL, tokens_output integer DEFAULT 0 NOT NULL, tokens_reasoning integer DEFAULT 0 NOT NULL, tokens_cache_read integer DEFAULT 0 NOT NULL, tokens_cache_write integer DEFAULT 0 NOT NULL, revert text, permission text, agent text, model text, time_created integer NOT NULL, time_updated integer NOT NULL, time_idle integer, time_viewed integer, idle_outcome text, time_compacting integer, time_archived integer, time_suspended integer, resume_attempts integer DEFAULT 0 NOT NULL);
             CREATE TABLE session_message (id text PRIMARY KEY, session_id text NOT NULL, type text NOT NULL, seq integer NOT NULL, time_created integer NOT NULL, time_updated integer NOT NULL, data text NOT NULL);",
        )
        .unwrap();
        db.execute(
            "INSERT INTO project VALUES ('p1', '/work/api', 'git', NULL, 1, 1, 1, NULL, '[]', NULL)",
            [],
        )
        .unwrap();
        db.execute(
            "INSERT INTO session_v2 (id, project_id, slug, directory, title, version, agent, model, time_created, time_updated, cost, \
             tokens_input, tokens_output, tokens_reasoning, tokens_cache_read, tokens_cache_write) \
             VALUES (?1, 'p1', 'quiet-fox', '/work/api', 'Uploader fix', '2.1.4', 'build', '{\"id\":\"claude-opus-5-5\",\"providerID\":\"anthropic\"}', 1792958000000, 1792958100000, 0.42, \
             6551, 83883, 42938, 12077475, 696121)",
            [format!("ses_{ID}")],
        )
        .unwrap();
        let rows = [
            (
                "user",
                1,
                json!({"text": "Fix the uploader", "files": [{"mime": "image/png", "filename": "shot.png"}]}),
            ),
            ("synthetic", 2, json!({"text": "INJECTED BY OPENCODE"})),
            (
                "assistant",
                3,
                json!({"agent": "build", "model": {"id": "claude-opus-5-5", "providerID": "anthropic"}, "content": [
                {"type": "reasoning", "text": "OPENCODE REASONING"},
                {"type": "text", "text": "I'll look."},
                {"type": "tool", "id": "c1", "name": "read", "state": {"status": "completed", "input": {"filePath": "src/main.rs"}, "content": [{"type": "text", "text": "fn main() {}"}]}}],
                "time": {"created": 1792958050000u64, "completed": 1792958060000u64}}),
            ),
            (
                "shell",
                4,
                json!({"command": "ls", "status": "exited", "exit": 0, "output": {"output": "Cargo.toml\nsrc", "cursor": 15, "size": 15, "truncated": false}}),
            ),
            (
                "assistant",
                5,
                json!({"agent": "build", "model": {"id": "claude-opus-5-5", "providerID": "anthropic"}, "content": [{"type": "text", "text": "Done."}], "time": {"created": 1792958100000u64, "completed": 1792958100000u64}}),
            ),
        ];
        for (kind, seq, data) in rows {
            db.execute(
                "INSERT INTO session_message (id, session_id, type, seq, time_created, time_updated, data) VALUES (?1, ?2, ?3, ?4, ?5, ?5, ?6)",
                rusqlite::params![format!("msg_{seq}"), format!("ses_{ID}"), kind, seq, 1792958000000i64 + seq * 1000, data.to_string()],
            )
            .unwrap();
        }
        home
    }

    #[test]
    fn reads_the_rows_and_leaves_reasoning_and_its_own_records_out() {
        let dir = std::env::temp_dir().join(format!("hatcheck-opencode-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let home = store(&dir);
        assert!(is_home(&home));
        let listed = sessions(&home);
        assert_eq!(listed.len(), 1);
        let x = &listed[0];
        assert_eq!(x.row.id, ID);
        assert_eq!(x.row.directory, "/work/api");
        assert_eq!(x.row.model, json!("anthropic/claude-opus-5-5"));
        assert_eq!(x.first_prompt.as_deref(), Some("Fix the uploader"));
        assert!(x.has_messages && x.size > 0 && !x.row.archived);
        assert_eq!(entrypoint(&x.row), "cli");
        let summary = summary(&home, x);
        assert_eq!(
            (summary.title.as_str(), summary.id.as_str()),
            ("Uploader fix", ID)
        );
        assert_eq!(summary.when, 1792958100.0);
        assert_eq!((x.row.cost, summary.cost), (0.42, Some(0.42)));
        let spent = summary.spend.unwrap();
        assert_eq!(spent.models.len(), 1);
        let m = &spent.models[0];
        assert_eq!(
            (
                m.model.as_deref(),
                m.usd,
                m.input,
                m.output,
                m.thinking,
                m.cache_read,
                m.cache_write
            ),
            (None, None, 6551, 83883, 42938, 12077475, 696121)
        );
        assert_eq!(spent.api_seconds, None); // opencode keeps no durations
        assert_eq!(sessions_starting_with(&home, "SES_7F3A").len(), 1);
        let reference = session_ref(&home, ID);
        assert_eq!(ref_id(&reference).as_deref(), Some(ID));
        assert_eq!(ref_id(&db_path(&home)), None);
        assert_eq!(
            summarize(&home, &reference).map(|s| s.id).as_deref(),
            Some(ID)
        );
        let conv = read_conversation(&home, &format!("ses_{ID}")).unwrap();
        assert_eq!(conv.calls, 2);
        assert_eq!(conv.model, json!("anthropic/claude-opus-5-5"));
        let text = conv
            .turns
            .iter()
            .map(|t| t.1.join("\n"))
            .collect::<Vec<_>>()
            .join("\n");
        for expected in [
            "Fix the uploader",
            "[image]",
            "I'll look.",
            "read",
            "fn main() {}",
            "shell",
            "Cargo.toml",
            "Done.",
        ] {
            assert!(text.contains(expected), "{expected}");
        }
        for left_out in ["INJECTED BY OPENCODE", "OPENCODE REASONING"] {
            assert!(!text.contains(left_out), "{left_out}");
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn mints_ids_as_opencode_does_and_writes_its_session_data() {
        let id = identifier(true, 1792958000000, 1);
        assert_eq!(id.len(), 26);
        assert!(id[..12].bytes().all(|b| b.is_ascii_hexdigit()));
        assert!(id[12..].bytes().all(|b| ID_CHARS.contains(&b)));
        // A later session sorts first: its inverted time is smaller.
        assert!(identifier(true, 1792958001000, 1)[..12] < id[..12]);
        assert!(
            identifier(false, 1792958001000, 1)[..12] > identifier(false, 1792958000000, 1)[..12]
        );
        let conv = Conversation {
            agent: Agent::Claude,
            id: "abc".into(),
            cwd: Some("/work/api".into()),
            model: json!("claude-opus-5-5"),
            turns: Vec::new(),
            calls: 0,
            title: Some("A title".into()),
            started: json!("2026-10-25T19:55:05.842Z"),
            branch: json!("main"),
            reasoning: "",
            left_out: "",
        };
        let turns: Vec<Turn> = vec![
            (
                "user".into(),
                vec!["Hello".into()],
                json!("2026-10-25T19:55:06.000Z"),
            ),
            ("assistant".into(), vec!["Hi.".into()], json!(null)),
        ];
        let dir =
            std::env::temp_dir().join(format!("hatcheck-opencode-write-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::env::set_var("HATCHECK_CACHE_DIR", &dir);
        let (id, path) =
            write_session(&conv, &turns, "/h/.local/share/opencode", "/work/api").unwrap();
        std::env::remove_var("HATCHECK_CACHE_DIR");
        assert!(path.ends_with(&format!("{id}.json")));
        let data: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(data["info"]["id"], json!(format!("ses_{id}")));
        assert_eq!(data["info"]["title"], "A title");
        assert_eq!(data["info"]["location"]["directory"], "/work/api");
        assert_eq!(data["info"]["time"]["created"], 1792958105842u64);
        assert_eq!(data["info"]["tokens"]["cache"]["read"], 0);
        let messages = data["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0]["type"], "user");
        assert_eq!(messages[0]["text"], "Hello");
        assert_eq!(messages[0]["time"]["created"], 1792958106000u64);
        assert_eq!(messages[1]["type"], "assistant");
        assert_eq!(
            messages[1]["model"],
            json!({"providerID": "claude", "id": "claude-opus-5-5"})
        );
        assert_eq!(messages[1]["content"][0]["text"], "Hi.");
        assert!(messages[1]["time"]["completed"].is_u64());
        assert!(messages[0]["id"].as_str().unwrap().starts_with("msg_"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
