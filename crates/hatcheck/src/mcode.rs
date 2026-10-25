//! MiniMax Code (mcode) as hatcheck reads it. The current data home is ~/.minimax, moved by
//! MINIMAX_DATA_DIR (MAVIS_DATA_DIR when that is unset); ~/.minimax-code is the installer, and
//! legacy .mavis data isn't merged or migrated. v2/sqlite/runtime-state.sqlite catalogs sessions;
//! dated folders hold the active canonical messages.jsonl, manifests, snapshots and reports.
//! Hatcheck opens SQLite read-only, resolves history without runtime initialization, and reads
//! expiring writer leases from local_runtime_session_locks. It never writes the store.
//! docs/agents.md's `MiniMax Code` section names the checked source and installed versions.

use crate::agent::Agent;
use crate::conv::{self, Conversation};
use crate::facts::Summary;
use crate::ospath::{self, Part};
use crate::paths;
use crate::py::{self, get, s, Num};
use crate::times;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::{TimeZone, Utc};
use rusqlite::{Connection, OpenFlags};
use serde_json::{json, Value};

const DB: [&str; 3] = ["v2", "sqlite", "runtime-state.sqlite"];
const SESSIONS: [&str; 2] = ["v2", "sessions"];

/// ~/.minimax, the current data home, not the installer's ~/.minimax-code.
pub fn default_home() -> String {
    ospath::join(&paths::home(), Agent::Mcode.base())
}

pub fn db_path(home: &str) -> String {
    ospath::join_all(home, &DB)
}

fn session_root(home: &str) -> String {
    ospath::join_all(home, &SESSIONS)
}

/// Whether a folder has mcode's current v2 store. Name alone isn't enough: the installer and
/// older data layouts are deliberately not included.
pub fn is_home(path: &str) -> bool {
    ospath::isfile(&db_path(path))
}

fn override_home() -> Option<String> {
    for name in ["MINIMAX_DATA_DIR", "MAVIS_DATA_DIR"] {
        if let Some(value) = std::env::var(name).ok().filter(|v| !v.trim().is_empty()) {
            return Some(ospath::expanduser(value.trim()));
        }
    }
    None
}

/// Every current mcode home, default first, deduplicated by real path.
pub fn find_homes() -> Vec<String> {
    let h = paths::home();
    let mut candidates = ospath::glob(&h, &[Part::Name(Agent::Mcode.base())]);
    candidates.extend(ospath::glob(
        &h,
        &[Part::Wild(&format!("{}-", Agent::Mcode.base()), "")],
    ));
    candidates.extend(override_home());
    let mut seen: Vec<(String, String)> = Vec::new();
    for d in candidates {
        if is_home(&d) {
            let real = ospath::realpath(&d);
            if !seen.iter().any(|(r, _)| *r == real) {
                seen.push((real, ospath::abspath(&d)));
            }
        }
    }
    let default = ospath::realpath(&default_home());
    let mut homes: Vec<(bool, String)> = seen.into_iter().map(|(r, a)| (r != default, a)).collect();
    homes.sort();
    homes.into_iter().map(|(_, a)| a).collect()
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

fn seconds(v: rusqlite::types::Value) -> f64 {
    match v {
        rusqlite::types::Value::Integer(n) => n as f64 / 1000.0,
        rusqlite::types::Value::Real(x) => x / 1000.0,
        rusqlite::types::Value::Text(t) => t.trim().parse::<f64>().map_or(0.0, |x| x / 1000.0),
        _ => 0.0,
    }
}

/// One catalog row, without the inert record_json compatibility projection.
#[derive(Clone, Debug, Default)]
pub struct Row {
    pub id: String,
    pub title: String,
    pub directory: String,
    pub created: f64,
    pub updated: f64,
    pub archived: bool,
    pub visibility: String,
    pub kind: String,
    pub session_type: String,
    pub purpose: String,
    pub agent_name: String,
    pub parent: String,
    pub model: Value,
    pub history: String,
}

const COLUMNS: &str = "session_id, title, workspace_dir, created_at_ms, updated_at_ms, archived, \
visibility, session_kind, session_type, purpose, agent_name, parent_session_id, extra_data_json, history_relative_dir";

fn row_of(r: &rusqlite::Row) -> rusqlite::Result<Row> {
    let v = |i: usize| r.get::<_, rusqlite::types::Value>(i);
    let full = text(v(0)?);
    let extra = text(v(12)?);
    let model = serde_json::from_str::<Value>(&extra)
        .ok()
        .map(|x| get(&x, "effectiveModel").clone())
        .filter(py::truthy)
        .unwrap_or(Value::Null);
    Ok(Row {
        id: Agent::Mcode.bare_id(&full).to_string(),
        title: text(v(1)?),
        directory: text(v(2)?),
        created: seconds(v(3)?),
        updated: seconds(v(4)?),
        archived: !matches!(v(5)?, rusqlite::types::Value::Integer(0)),
        visibility: text(v(6)?),
        kind: text(v(7)?),
        session_type: text(v(8)?),
        purpose: text(v(9)?),
        agent_name: text(v(10)?),
        parent: text(v(11)?),
        model,
        history: text(v(13)?),
    })
}

/// Current v3 catalog rows, newest activity first. Other column versions aren't silently read.
pub fn rows(home: &str) -> Vec<Row> {
    let Ok(db) = open(home) else {
        return Vec::new();
    };
    let Ok(mut query) = db.prepare(&format!(
        "SELECT {COLUMNS} FROM local_runtime_sessions WHERE columnar_version = 3 \
         ORDER BY updated_at_ms DESC, created_at_ms DESC, session_id"
    )) else {
        return Vec::new();
    };
    let Ok(found) = query.query_map([], row_of) else {
        return Vec::new();
    };
    found.flatten().collect()
}

pub fn rows_starting_with(home: &str, prefix: &str) -> Vec<Row> {
    let lower = Agent::Mcode.bare_id(prefix).to_lowercase();
    rows(home)
        .into_iter()
        .filter(|r| r.id.to_lowercase().starts_with(&lower))
        .collect()
}

/// How the list files a session: explicit internal work, then hidden/internal trees, then ordinary
/// user-facing conversations and branches. Parentage alone doesn't make a subagent.
pub fn entrypoint(row: &Row) -> String {
    let purpose = row.purpose.as_str();
    if row.kind == "task"
        || purpose.starts_with("local-task:")
        || purpose.starts_with("local-background-task:")
        || purpose.starts_with("team-plan:")
        || matches!(row.agent_name.as_str(), "explore" | "worker" | "verifier")
    {
        return "subagent".into();
    }
    if matches!(row.kind.as_str(), "peek" | "cron" | "channel") {
        return row.kind.clone();
    }
    if row.visibility == "hidden" {
        return "internal".into();
    }
    "cli".into()
}

/// A session and its active canonical history.
#[derive(Clone, Debug)]
pub struct Session {
    pub row: Row,
    pub dir: String,
    pub messages: String,
}

impl Session {
    pub fn id(&self) -> &str {
        &self.row.id
    }
}

fn invalid(message: impl Into<String>) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, message.into())
}

fn regular_file(path: &str) -> bool {
    std::fs::symlink_metadata(path)
        .map(|m| m.file_type().is_file() && !m.file_type().is_symlink())
        .unwrap_or(false)
}

fn regular_dir(path: &str) -> bool {
    std::fs::symlink_metadata(path)
        .map(|m| m.file_type().is_dir() && !m.file_type().is_symlink())
        .unwrap_or(false)
}

fn history_from_dir(row: Row, dir: String) -> std::io::Result<Session> {
    if !regular_dir(&dir) {
        return Err(invalid("mcode session directory is missing or linked"));
    }
    let messages = ospath::join(&dir, "messages.jsonl");
    if !regular_file(&messages) {
        return Err(invalid("mcode canonical history is missing or linked"));
    }
    Ok(Session { row, dir, messages })
}

fn relative_segments(value: &str) -> std::io::Result<Vec<&str>> {
    if value.is_empty()
        || value.trim() != value
        || value.contains('\\')
        || value.starts_with('/')
        || value.as_bytes().get(1) == Some(&b':')
    {
        return Err(invalid("unsafe mcode history directory"));
    }
    let segments: Vec<&str> = value.split('/').collect();
    if segments.len() != 4
        || segments
            .iter()
            .any(|part| part.is_empty() || *part == "." || *part == "..")
    {
        return Err(invalid("invalid mcode history directory"));
    }
    Ok(segments)
}

fn derived_history(home: &str, row: &Row) -> String {
    let full = Agent::Mcode.full_id(&row.id);
    let encoded = URL_SAFE_NO_PAD.encode(full.as_bytes());
    let millis = (row.created * 1000.0) as i64;
    let stamp = Utc
        .timestamp_millis_opt(millis)
        .single()
        .map(|t| t.format("%Y/%m/%d/%H-%M-%S-%3f").to_string())
        .unwrap_or_else(|| "1970/01/01/00-00-00-000".into());
    ospath::join_all(
        &session_root(home),
        &[
            &stamp[..4],
            &stamp[5..7],
            &stamp[8..10],
            &format!("{}-session_{}", &stamp[11..], encoded),
        ],
    )
}

fn manifest_history(home: &str, row: &Row) -> std::io::Result<Session> {
    let root = session_root(home);
    let full = Agent::Mcode.full_id(&row.id);
    let mut found: Option<Session> = None;
    for manifest in ospath::glob(
        &root,
        &[
            Part::Wild("", ""),
            Part::Wild("", ""),
            Part::Wild("", ""),
            Part::Wild("", ""),
            Part::Name("manifest.json"),
        ],
    ) {
        if !regular_file(&manifest) {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&manifest) else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        let created = Num::of(get(&value, "createdAtMs")).map_or(0.0, |n| n.f() / 1000.0);
        if get(&value, "schemaVersion").as_u64() != Some(1)
            || s(get(&value, "sessionId")) != Some(full.as_str())
            || (created - row.created).abs() > 0.000_001
        {
            continue;
        }
        if found.is_some() {
            return Err(invalid("ambiguous mcode history manifests"));
        }
        found = Some(history_from_dir(row.clone(), ospath::dirname(&manifest))?);
    }
    found.ok_or_else(|| invalid("no mcode history manifest matches the catalog row"))
}

fn history(home: &str, row: &Row) -> std::io::Result<Session> {
    if !row.history.is_empty() {
        let segments = relative_segments(&row.history)?;
        let dir = segments
            .iter()
            .fold(session_root(home), |base, part| ospath::join(&base, part));
        return history_from_dir(row.clone(), dir);
    }
    let derived = derived_history(home, row);
    if regular_file(&ospath::join(&derived, "messages.jsonl")) {
        return history_from_dir(row.clone(), derived);
    }
    manifest_history(home, row)
}

/// Every readable current session with its canonical history resolved.
pub fn sessions(home: &str) -> Vec<Session> {
    rows(home)
        .into_iter()
        .filter_map(|row| history(home, &row).ok())
        .collect()
}

pub fn sessions_starting_with(home: &str, prefix: &str) -> Vec<Session> {
    let lower = Agent::Mcode.bare_id(prefix).to_lowercase();
    sessions(home)
        .into_iter()
        .filter(|x| x.row.id.to_lowercase().starts_with(&lower))
        .collect()
}

pub fn session(home: &str, id: &str) -> Option<Session> {
    let id = Agent::Mcode.bare_id(id);
    sessions(home).into_iter().find(|x| x.row.id == id)
}

/// The summary of the session directory named by path, or None when it isn't in the catalog.
pub fn summarize(home: &str, path: &str) -> Option<Summary> {
    sessions(home)
        .into_iter()
        .find(|x| paths::same_folder(&x.dir, path))
        .map(|x| summary(home, &x))
}

fn first_prompt(x: &Session) -> Option<String> {
    conversation(x)
        .ok()?
        .turns
        .iter()
        .find_map(|(role, texts, _)| {
            (role == "user")
                .then(|| texts.first().map(|text| py::collapse(text)))
                .flatten()
                .filter(|title| !title.is_empty())
        })
}

/// A session as the list shows it.
pub fn summary(home: &str, x: &Session) -> Summary {
    let title = if x.row.title.trim().is_empty() {
        first_prompt(x).unwrap_or_else(|| "(untitled)".into())
    } else {
        py::collapse(&x.row.title)
    };
    let size = std::fs::metadata(&x.messages).map(|m| m.len()).unwrap_or(0);
    Summary {
        agent: Agent::Mcode,
        id: x.row.id.clone(),
        when: if x.row.updated != 0.0 {
            x.row.updated
        } else {
            x.row.created
        },
        entrypoint: json!(entrypoint(&x.row)),
        branch: Value::Null,
        cwd: Some(x.row.directory.clone()).filter(|d| !d.is_empty()),
        canonical: true,
        size,
        has_messages: size > 0 || !x.row.title.trim().is_empty(),
        title,
        continued_in: Value::Null,
        model: x.row.model.clone(),
        profile: home.to_string(),
        path: x.dir.clone(),
        open: None,
        bound_to: None,
    }
}

/// Whether an unexpired writer lease exists. Unknown lock state fails closed for writes; an idle
/// selected TUI has no durable marker and can't be detected.
pub fn session_open(home: &str, id: &str) -> bool {
    let Ok(db) = open(home) else {
        return true;
    };
    let full = Agent::Mcode.full_id(id);
    let now = (times::now() * 1000.0) as i64;
    match db.query_row(
        "SELECT expires_at_ms FROM local_runtime_session_locks WHERE session_id = ?1",
        [full],
        |row| row.get::<_, i64>(0),
    ) {
        Ok(expires) => expires > now,
        Err(rusqlite::Error::QueryReturnedNoRows) => false,
        Err(_) => true,
    }
}

fn stamp(value: &Value) -> Value {
    match Num::of(value) {
        Some(n) if n.f() != 0.0 => {
            times::iso_utc(n.f() / 1000.0).map_or(Value::Null, Value::String)
        }
        _ => Value::Null,
    }
}

fn text_content(content: &Value) -> String {
    match content {
        Value::String(text) => text.clone(),
        Value::Array(parts) => parts
            .iter()
            .filter(|part| s(get(part, "type")) == Some("text"))
            .filter_map(|part| s(get(part, "text")))
            .collect::<Vec<_>>()
            .join("\n\n"),
        _ => String::new(),
    }
}

/// The active canonical conversation, never the separate display timeline or old snapshots.
fn conversation(x: &Session) -> std::io::Result<Conversation> {
    let data = std::fs::read(&x.messages)?;
    let (mut turns, mut calls, mut model) = (Vec::new(), 0, x.row.model.clone());
    for line in data.split_inclusive(|b| *b == b'\n') {
        if !line.ends_with(b"\n") {
            break;
        }
        if line.iter().all(|b| b.is_ascii_whitespace()) {
            continue;
        }
        let envelope: Value = serde_json::from_slice(line)?;
        if !envelope.is_object()
            || s(get(&envelope, "message_id")).is_none()
            || s(get(&envelope, "turn_id")).is_none()
        {
            return Err(invalid("invalid mcode history envelope"));
        }
        let message = get(&envelope, "message");
        if !message.is_object() {
            return Err(invalid("invalid mcode history message"));
        }
        let role = s(get(message, "role")).unwrap_or("");
        let when = stamp(get(message, "timestamp"));
        match role {
            "user" => conv::add_turn(
                &mut turns,
                "user",
                &text_content(get(message, "content")),
                &when,
            ),
            "assistant" => {
                if model.is_null() {
                    if let Some(value) = s(get(message, "model")) {
                        model = json!(value);
                    }
                }
                conv::add_turn(
                    &mut turns,
                    "assistant",
                    &text_content(get(message, "content")),
                    &when,
                );
                if let Some(parts) = get(message, "content").as_array() {
                    for part in parts
                        .iter()
                        .filter(|part| s(get(part, "type")) == Some("toolCall"))
                    {
                        let name = s(get(part, "name")).unwrap_or("tool");
                        let raw = get(part, "arguments");
                        let args = match raw {
                            Value::String(text) => serde_json::from_str::<Value>(text)
                                .unwrap_or_else(|_| Value::String(text.clone())),
                            other => other.clone(),
                        };
                        let fields: Vec<(String, Value)> = match args {
                            Value::Object(fields) => fields.into_iter().collect(),
                            other => vec![("input".into(), other)],
                        };
                        conv::add_turn(
                            &mut turns,
                            "assistant",
                            &conv::call_text(name, &fields),
                            &when,
                        );
                        calls += 1;
                    }
                }
            }
            "toolResult" => {
                let output = text_content(get(message, "content"));
                if !output.is_empty() {
                    conv::add_turn(&mut turns, "assistant", &conv::result_text(&output), &when);
                }
            }
            "compactionSummary" => {
                if let Some(summary) = s(get(message, "summary")) {
                    conv::add_turn(&mut turns, "user", summary, &when);
                }
            }
            _ => {} // hidden/custom/plugin records, branch summaries and UI-only records
        }
    }
    let title = if x.row.title.trim().is_empty() {
        turns.iter().find_map(|(role, texts, _)| {
            (role == "user")
                .then(|| texts.first().map(|text| py::collapse(text)))
                .flatten()
                .filter(|title| !title.is_empty())
        })
    } else {
        Some(py::collapse(&x.row.title))
    };
    Ok(Conversation {
        agent: Agent::Mcode,
        id: x.row.id.clone(),
        cwd: Some(x.row.directory.clone()).filter(|d| !d.is_empty()),
        model,
        turns,
        calls,
        title,
        started: if x.row.created != 0.0 {
            times::iso_utc(x.row.created).map_or(Value::Null, Value::String)
        } else {
            Value::Null
        },
        branch: Value::Null,
        reasoning: "MiniMax Code's reasoning and signatures",
        left_out: "MiniMax Code's reasoning and signatures, images, files, hidden and custom records, UI-only notices, previous generations, tool-output artifacts and subagent sessions",
    })
}

pub fn read_conversation(home: &str, id: &str) -> std::io::Result<Conversation> {
    let x = session(home, id).ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("no session {id} in {home}"),
        )
    })?;
    conversation(&x)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row() -> Row {
        Row {
            id: "b8bde3ebadeb49d5be04757040a8a2b3".into(),
            title: "Fix the parser".into(),
            directory: "/work".into(),
            created: 1_792_878_957.992,
            updated: 1_792_878_961.960,
            visibility: "visible".into(),
            kind: "conversation".into(),
            session_type: "branch".into(),
            agent_name: "mavis".into(),
            model: json!("minimax/MiniMax-M3.1-Flash"),
            history:
                "2026/10/24/21-55-57-992-session_bXZzX2I4YmRlM2ViYWRlYjQ5ZDViZTA0NzU3MDQwYThhMmIz"
                    .into(),
            ..Row::default()
        }
    }

    #[test]
    fn classifies_internal_work_without_treating_branches_as_subagents() {
        let mut r = row();
        assert_eq!(entrypoint(&r), "cli");
        r.parent = "mvs_parent".into();
        assert_eq!(entrypoint(&r), "cli");
        r.kind = "task".into();
        assert_eq!(entrypoint(&r), "subagent");
        r.kind = "peek".into();
        assert_eq!(entrypoint(&r), "peek");
        r.kind = "conversation".into();
        r.visibility = "hidden".into();
        assert_eq!(entrypoint(&r), "internal");
        r.visibility = "visible".into();
        r.agent_name = "worker".into();
        assert_eq!(entrypoint(&r), "subagent");
    }

    #[test]
    fn validates_history_directories_and_derives_utc_names() {
        let r = row();
        assert!(relative_segments(&r.history).is_ok());
        for bad in [
            "",
            "../x",
            "/tmp/x",
            "C:/x",
            "a/b/c",
            "a//c/d",
            "a\\b\\c\\d",
        ] {
            assert!(relative_segments(bad).is_err(), "{bad}");
        }
        // As this system spells a path: with \ on Windows.
        let native = |p: &str| p.replace('/', ospath::SEP);
        assert!(derived_history("/h/.minimax", &r).ends_with(&native(
            "2026/10/24/21-55-57-992-session_bXZzX2I4YmRlM2ViYWRlYjQ5ZDViZTA0NzU3MDQwYThhMmIz"
        )));
    }

    #[test]
    fn reads_canonical_messages_and_leaves_reasoning_out() {
        let dir = std::env::temp_dir().join(format!("hatcheck-mcode-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("messages.jsonl");
        let rows = [
            json!({"message_id":"msg-u1","turn_id":"turn-1","message":{"role":"user","content":[{"type":"text","text":"Fix it"}],"timestamp":1792878958126i64},"turn_config":{"system_prompt":"not a prompt"}}),
            json!({"message_id":"msg-a1","turn_id":"turn-1","message":{"role":"assistant","content":[{"type":"thinking","thinking":"secret","thinkingSignature":"signed"},{"type":"text","text":"I'll look"},{"type":"toolCall","id":"c1","name":"read_file","arguments":"{\"path\":\"src/main.rs\"}"}],"model":"minimax/MiniMax-M3.1-Flash","timestamp":1792878958175i64}}),
            json!({"message_id":"msg-t1","turn_id":"turn-1","message":{"role":"toolResult","toolCallId":"c1","toolName":"read_file","content":[{"type":"text","text":"fn main() {}"}],"timestamp":1792878958180i64}}),
            json!({"message_id":"msg-c1","turn_id":"turn-1","message":{"role":"compactionSummary","summary":"Earlier work","tokensBefore":1000}}),
        ];
        std::fs::write(
            &path,
            rows.iter()
                .map(|r| r.to_string() + "\n")
                .collect::<String>(),
        )
        .unwrap();
        let x = Session {
            row: row(),
            dir: dir.to_string_lossy().into_owned(),
            messages: path.to_string_lossy().into_owned(),
        };
        let conv = conversation(&x).unwrap();
        assert_eq!(conv.calls, 1);
        assert_eq!(conv.turns.len(), 3);
        let text = conv
            .turns
            .iter()
            .map(|t| t.1.join("\n"))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("Fix it"));
        assert!(text.contains("read_file"));
        assert!(text.contains("fn main() {}"));
        assert!(text.contains("Earlier work"));
        assert!(!text.contains("secret"));
        assert!(!text.contains("not a prompt"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn an_empty_canonical_file_is_a_valid_empty_conversation() {
        let dir = std::env::temp_dir().join(format!("hatcheck-mcode-empty-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("messages.jsonl");
        std::fs::write(&path, []).unwrap();
        let x = Session {
            row: row(),
            dir: dir.to_string_lossy().into_owned(),
            messages: path.to_string_lossy().into_owned(),
        };
        let conv = conversation(&x).unwrap();
        assert!(conv.turns.is_empty());
        assert_eq!(conv.title.as_deref(), Some("Fix the parser"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
