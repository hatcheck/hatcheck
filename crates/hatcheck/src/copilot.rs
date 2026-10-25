//! GitHub Copilot CLI (copilot) as hatcheck reads it. The home is ~/.copilot, moved whole by
//! COPILOT_HOME: its settings, its sessions and, when there is no system credential store, its
//! login, which hatcheck never reads. A session is session-state/<uuid>/: events.jsonl, the
//! record copilot writes as the session runs (session.start, user.message, assistant.message,
//! the tool events and copilot's own records), workspace.yaml (its folder, branch, name and
//! times) and copilot's working files beside them. session-store.db at the root indexes them for
//! copilot's session browser, and isn't read. inuse.<pid>.lock in a session's folder names the
//! process that has it open. hatcheck reads all of it and writes none of it: a session converted
//! for copilot is a file in the interchange form its own `copilot sessions import` takes.
//! docs/agents.md's `GitHub Copilot CLI` section says what is read, and that it was checked
//! against copilot 1.0.103.

use crate::agent::Agent;
use crate::conv::{self, Conversation, Turn};
use crate::facts::Summary;
use crate::ospath::{self, Part};
use crate::paths;
use crate::py::{self, get, s, truthy};
use crate::times;
use serde_json::{json, Value};
use std::collections::HashMap;

/// ~/.copilot, where plain copilot keeps everything.
pub fn default_home() -> String {
    ospath::join(&paths::home(), Agent::Copilot.base())
}

/// Whether a folder is a copilot home: it has the session-state folder copilot makes on its
/// first run, or the config.json it writes then.
pub fn is_home(path: &str) -> bool {
    ospath::isdir(&ospath::join(path, "session-state"))
        || ospath::isfile(&ospath::join(path, "config.json"))
}

fn override_home() -> Option<String> {
    std::env::var("COPILOT_HOME")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .map(|v| ospath::expanduser(&v))
}

/// Every copilot home: ~/.copilot, ~/.copilot-NAME and the folder COPILOT_HOME names, the
/// default first.
pub fn find_homes() -> Vec<String> {
    let h = paths::home();
    let mut candidates = ospath::glob(&h, &[Part::Name(Agent::Copilot.base())]);
    candidates.extend(ospath::glob(
        &h,
        &[Part::Wild(&format!("{}-", Agent::Copilot.base()), "")],
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

/// A session's folder in a home: session-state/<id>.
pub fn session_dir(home: &str, id: &str) -> String {
    ospath::join_all(home, &["session-state", id])
}

/// Whether a folder name is one of copilot's session IDs: a UUID, as copilot names them.
fn is_session_name(name: &str) -> bool {
    (32..=36).contains(&name.len()) && name.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-')
}

/// The session ID of an events file at this path, when the path is one of copilot's:
/// session-state/<id>/events.jsonl.
pub fn file_id(path: &str) -> Option<String> {
    if ospath::basename(path) != "events.jsonl" {
        return None;
    }
    let dir = ospath::dirname(path);
    let name = ospath::basename(&dir);
    (is_session_name(&name) && ospath::basename(&ospath::dirname(&dir)) == "session-state")
        .then(|| name.to_lowercase())
}

/// The home an events file belongs to: three folders up.
pub fn home_of(path: &str) -> String {
    let mut home = path.to_string();
    for _ in 0..3 {
        home = ospath::dirname(&home);
    }
    home
}

/// A session, from its folder.
#[derive(Clone, Debug)]
pub struct Session {
    pub id: String,
    pub dir: String,
    /// Its events.jsonl.
    pub file: String,
    pub cwd: Option<String>,
    pub branch: Value,
    pub started: Value,
    pub when: f64,
    /// copilot's own name for it, from workspace.yaml.
    pub title: Option<String>,
    pub first_prompt: Option<String>,
    pub model: Value,
    /// The client that made it, as workspace.yaml names it: github/cli for the CLI.
    pub client: String,
    pub has_messages: bool,
    pub size: u64,
}

/// workspace.yaml's fields: copilot writes it as `key: value` lines, the value quoted when
/// YAML needs it. Nested or multi-line values aren't read; copilot writes none.
fn yaml_fields(text: &str) -> HashMap<String, String> {
    let mut fields = HashMap::new();
    for line in text.lines() {
        if line.starts_with(' ') || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let mut value = value.trim().to_string();
        let quoted = |q: char| value.len() >= 2 && value.starts_with(q) && value.ends_with(q);
        if quoted('"') {
            value = value[1..value.len() - 1]
                .replace("\\\"", "\"")
                .replace("\\\\", "\\");
        } else if quoted('\'') {
            value = value[1..value.len() - 1].replace("''", "'");
        }
        if value == "~" || value == "null" {
            value.clear();
        }
        fields.insert(key.trim().to_string(), value);
    }
    fields
}

/// Every complete line of an events file as JSON, leaving a torn last line alone.
fn records(path: &str) -> std::io::Result<Vec<Value>> {
    let data = std::fs::read(path)?;
    let mut records = Vec::new();
    for line in data.split_inclusive(|b| *b == b'\n') {
        if !line.ends_with(b"\n") {
            break;
        }
        if line.iter().all(|b| b.is_ascii_whitespace()) {
            continue;
        }
        if let Ok(value) = serde_json::from_slice::<Value>(line) {
            if value.is_object() {
                records.push(value);
            }
        }
    }
    Ok(records)
}

/// A message's content as text: a string as it is, or a list of parts, their text with an
/// image or file marked where it was.
fn content_text(content: &Value) -> String {
    match content {
        Value::String(text) => text.clone(),
        Value::Array(parts) => parts
            .iter()
            .map(|p| match p {
                Value::String(text) => text.clone(),
                _ if s(get(p, "text")).is_some() => py::str_of(get(p, "text")),
                _ => {
                    let kind = py::or(get(p, "mimeType"), get(p, "mediaType"));
                    if s(get(p, "type")) == Some("image")
                        || s(kind).is_some_and(|m| m.starts_with("image/"))
                    {
                        "[image]".into()
                    } else {
                        "[file]".into()
                    }
                }
            })
            .filter(|t| !t.is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
        _ if s(get(content, "text")).is_some() => py::str_of(get(content, "text")),
        _ => String::new(),
    }
}

/// A session read from its folder, or None when it has no events file, as a session copilot
/// started and never spoke in hasn't.
pub fn read(dir: &str) -> Option<Session> {
    let name = ospath::basename(dir);
    if !is_session_name(&name) {
        return None;
    }
    let file = ospath::join(dir, "events.jsonl");
    let recs = records(&file).ok()?;
    let size = std::fs::metadata(&file).map(|m| m.len()).unwrap_or(0);
    let yaml = std::fs::read_to_string(ospath::join(dir, "workspace.yaml"))
        .map(|t| yaml_fields(&t))
        .unwrap_or_default();
    let field = |key: &str| yaml.get(key).cloned().filter(|v| !v.is_empty());
    let id = field("id")
        .map(|v| v.to_lowercase())
        .unwrap_or_else(|| name.to_lowercase());
    let mut cwd = field("cwd");
    let mut branch = field("branch").map_or(Value::Null, Value::String);
    let mut started = Value::Null;
    let mut last_stamp: Option<f64> = None;
    let mut first_prompt: Option<String> = None;
    let mut model = Value::Null;
    let mut chosen = Value::Null; // what copilot picked, when no reply has named a model
    let mut has_messages = false;
    for r in &recs {
        if let Some(t) = s(get(r, "timestamp")).and_then(times::parse_time) {
            last_stamp = Some(t);
        }
        let data = get(r, "data");
        match s(get(r, "type")) {
            Some("session.start") => {
                let context = get(data, "context");
                if cwd.is_none() {
                    cwd = s(get(context, "cwd"))
                        .map(String::from)
                        .filter(|c| !c.is_empty());
                }
                if branch.is_null() && truthy(get(context, "branch")) {
                    branch = get(context, "branch").clone();
                }
                started = py::or(get(data, "startTime"), get(r, "timestamp")).clone();
            }
            Some("user.message") => {
                has_messages = true;
                if first_prompt.is_none() {
                    first_prompt = Some(content_text(get(data, "content")))
                        .filter(|t| !py::strip(t).is_empty());
                }
            }
            Some("assistant.message") => {
                has_messages = true;
                if truthy(get(data, "model")) {
                    model = get(data, "model").clone();
                }
            }
            Some("session.model_change") if truthy(get(data, "newModel")) => {
                chosen = get(data, "newModel").clone();
            }
            _ => {}
        }
    }
    if model.is_null() {
        model = chosen;
    }
    if started.is_null() {
        started = field("created_at").map_or(Value::Null, Value::String);
    }
    let when = last_stamp
        .or_else(|| field("updated_at").and_then(|t| times::parse_time(&t)))
        .unwrap_or_else(|| ospath::getmtime(&file).unwrap_or(0.0));
    Some(Session {
        id,
        dir: dir.to_string(),
        file,
        cwd,
        branch,
        started,
        when,
        title: field("name"),
        first_prompt,
        model,
        client: field("client_name").unwrap_or_default(),
        has_messages,
        size,
    })
}

/// Every session in a home, the newest change first.
pub fn sessions(home: &str) -> Vec<Session> {
    let mut found: Vec<Session> = ospath::glob(
        home,
        &[
            Part::Name("session-state"),
            Part::Wild("", ""),
            Part::Name("events.jsonl"),
        ],
    )
    .iter()
    .filter_map(|file| read(&ospath::dirname(file)))
    .collect();
    found.sort_by(|a, b| {
        b.when
            .partial_cmp(&a.when)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    found
}

pub fn sessions_starting_with(home: &str, prefix: &str) -> Vec<Session> {
    let lower = prefix.to_lowercase();
    sessions(home)
        .into_iter()
        .filter(|x| x.id.starts_with(&lower))
        .collect()
}

pub fn session(home: &str, id: &str) -> Option<Session> {
    let lower = id.to_lowercase();
    sessions(home).into_iter().find(|x| x.id == lower)
}

/// The summary of the events file at path, or None when it isn't one of the home's.
pub fn summarize(home: &str, path: &str) -> Option<Summary> {
    let real = ospath::realpath(path);
    sessions(home)
        .into_iter()
        .find(|x| ospath::realpath(&x.file) == real)
        .map(|x| summary(home, &x))
}

/// How the list files a session: the CLI's are cli; one made through another client, such as
/// the Copilot app, goes under that client's name, which waits for --all.
pub fn entrypoint(x: &Session) -> String {
    if x.client.is_empty() || x.client == "github/cli" {
        "cli".into()
    } else {
        x.client.clone()
    }
}

/// A session as the list shows it.
pub fn summary(home: &str, x: &Session) -> Summary {
    let title = x
        .title
        .clone()
        .or_else(|| x.first_prompt.clone())
        .map(|t| py::collapse(&t))
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| "(untitled)".into());
    Summary {
        agent: Agent::Copilot,
        id: x.id.clone(),
        when: x.when,
        entrypoint: json!(entrypoint(x)),
        branch: x.branch.clone(),
        cwd: x.cwd.clone(),
        canonical: true,
        size: x.size,
        has_messages: x.has_messages,
        title,
        continued_in: Value::Null,
        model: x.model.clone(),
        profile: home.to_string(),
        path: x.file.clone(),
        open: None,
        bound_to: None,
    }
}

/// Whether copilot has the session open: inuse.<pid>.lock in its folder names a running
/// process. copilot removes the file when it quits, so one naming a process that has gone is
/// a crash's, and isn't open.
pub fn session_open(dir: &str) -> bool {
    ospath::glob(dir, &[Part::Wild("inuse.", ".lock")])
        .iter()
        .filter_map(|path| {
            let name = ospath::basename(path);
            name.strip_prefix("inuse.")?
                .strip_suffix(".lock")?
                .parse::<u64>()
                .ok()
        })
        .any(crate::qwen::pid_alive)
}

/// The input of a tool call as (name, value) lines for the call's text.
fn call_fields(arguments: &Value) -> Vec<(String, Value)> {
    match arguments {
        Value::Object(o) => o.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
        Value::Null => Vec::new(),
        other => vec![("input".into(), other.clone())],
    }
}

/// The conversation: your messages and the shell commands you ran, the replies' text and tool
/// calls, and the tools' results; copilot's own records, its hooks' and what it adds to the
/// conversation itself left out.
fn conversation(x: &Session) -> std::io::Result<Conversation> {
    let recs = records(&x.file)?;
    let (mut turns, mut calls, mut model) = (Vec::new(), 0, Value::Null);
    for r in &recs {
        let data = get(r, "data");
        let when = get(r, "timestamp");
        match s(get(r, "type")) {
            Some("user.message") => {
                let text = content_text(get(data, "content"));
                if !py::strip(&text).is_empty() {
                    conv::add_turn(&mut turns, "user", &text, when);
                }
            }
            Some("tool.user_requested") => {
                // A command you ran yourself, with ! in copilot's prompt.
                let name = s(get(data, "toolName")).unwrap_or("tool");
                let arguments = get(data, "arguments");
                let text = match s(get(arguments, "command")) {
                    Some(command) if name == "local_shell" => format!("!{command}"),
                    _ => conv::call_text(name, &call_fields(arguments)),
                };
                conv::add_turn(&mut turns, "user", &text, when);
            }
            Some("assistant.message") => {
                if truthy(get(data, "model")) {
                    model = get(data, "model").clone();
                }
                let text = content_text(get(data, "content"));
                if !py::strip(&text).is_empty() {
                    conv::add_turn(&mut turns, "assistant", &text, when);
                }
                for call in get(data, "toolRequests").as_array().into_iter().flatten() {
                    let name =
                        s(py::or(get(call, "name"), get(call, "toolName"))).unwrap_or("tool");
                    let arguments = py::or(get(call, "arguments"), get(call, "input"));
                    conv::add_turn(
                        &mut turns,
                        "assistant",
                        &conv::call_text(name, &call_fields(arguments)),
                        when,
                    );
                    calls += 1;
                }
            }
            Some("tool.execution_complete") => {
                let result = get(data, "result");
                let output = match result {
                    Value::String(text) => text.clone(),
                    Value::Object(_) => {
                        let content = get(result, "content");
                        if content.is_null() {
                            py::dumps_compact(result)
                        } else {
                            content_text(content)
                        }
                    }
                    _ => String::new(),
                };
                if !py::strip(&output).is_empty() {
                    conv::add_turn(&mut turns, "assistant", &conv::result_text(&output), when);
                }
            }
            _ => {}
        }
    }
    if model.is_null() {
        model = x.model.clone();
    }
    let title = x
        .title
        .clone()
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
        agent: Agent::Copilot,
        id: x.id.clone(),
        cwd: x.cwd.clone(),
        model,
        turns,
        calls,
        title,
        started: x.started.clone(),
        branch: x.branch.clone(),
        reasoning: "GitHub Copilot CLI's reasoning",
        left_out: "GitHub Copilot CLI's reasoning, what copilot adds to the conversation itself, its hooks' and its own records, and images and files",
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

/// Where a conversation converted for copilot is written: imports/<id>.jsonl in hatcheck's
/// cache folder, in the interchange form `copilot sessions import` takes.
pub fn import_path(id: &str) -> String {
    ospath::join(&conv::imports_dir(), &format!("{id}.jsonl"))
}

/// Write the conversation as a session interchange file (version 1): a session header, then a
/// message record per turn, as copilot's own importer validates them, and the file's path with
/// the ID hatcheck gave the file. copilot gives the session it makes from it an ID of its own.
pub fn write_session(
    conv: &Conversation,
    turns: &[Turn],
    _home: &str,
    cwd: &str,
) -> std::io::Result<(String, String)> {
    let id = crate::move_cmd::uuid4();
    let now = times::now();
    let at = |when: &Value| -> f64 {
        s(when)
            .and_then(times::parse_time)
            .filter(|t| *t != 0.0)
            .unwrap_or(now)
    };
    let mut header = json!({
        "type": "session", "version": 1, "externalId": id,
        "createdAt": crate::move_cmd::iso_ms(at(&conv.started)), "updatedAt": crate::move_cmd::iso_ms(now),
        "source": {"application": "hatcheck", "version": crate::cli::version()},
        "workspace": {"workingDirectory": cwd},
    });
    if let Some(title) = conv
        .title
        .as_deref()
        .map(py::collapse)
        .filter(|t| !t.is_empty())
    {
        header["title"] = json!(title);
    }
    if let Some(branch) = s(&conv.branch).filter(|b| !b.trim().is_empty()) {
        header["workspace"]["branch"] = json!(branch);
    }
    let mut lines = vec![py::dumps_compact(&header)];
    for (role, texts, when) in turns {
        let record = json!({
            "type": "message", "id": crate::move_cmd::uuid4(), "timestamp": crate::move_cmd::iso_ms(at(when)),
            "role": role, "content": [{"type": "text", "text": texts.join("\n\n")}],
        });
        lines.push(py::dumps_compact(&record));
    }
    let path = import_path(&id);
    std::fs::create_dir_all(conv::imports_dir())?;
    let tmp = format!("{path}.hatcheck-tmp");
    std::fs::write(&tmp, lines.join("\n") + "\n")?;
    std::fs::rename(&tmp, &path)?;
    Ok((id, path))
}

/// The command that imports a converted session into a home: copilot's own importer, which
/// prints the ID of the session it makes, for `copilot --resume`.
pub fn start_command(home: &str, file: &str, full: bool) -> String {
    paths::copilot_command(
        home,
        &format!("sessions import {}", paths::shell_path(file)),
        full,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "50b1f120-6353-4117-b918-fb4553e92eb4";

    fn event(n: usize, kind: &str, data: Value) -> Value {
        json!({
            "type": kind, "data": data, "id": format!("e{n}"),
            "timestamp": format!("2026-10-25T19:55:{:02}.000Z", n), "parentId": null,
        })
    }

    fn session_folder(root: &std::path::Path, name: Option<&str>) -> String {
        let dir = root.join("session-state").join(ID);
        std::fs::create_dir_all(&dir).unwrap();
        let mut yaml = format!(
            "id: {ID}\ncwd: /work/api\ngit_root: /work/api\nrepository: me/api\nhost_type: github\nbranch: main\nclient_name: github/cli\n"
        );
        if let Some(name) = name {
            yaml += &format!("name: \"{name}\"\nuser_named: false\n");
        }
        yaml += "created_at: 2026-10-25T19:55:05.842Z\nupdated_at: 2026-10-25T19:55:30.252Z\n";
        std::fs::write(dir.join("workspace.yaml"), yaml).unwrap();
        let events = vec![
            event(
                0,
                "session.start",
                json!({"sessionId": ID, "version": 1, "producer": "copilot-agent", "copilotVersion": "1.0.103",
                "startTime": "2026-10-25T19:55:05.842Z", "context": {"cwd": "/work/api", "branch": "main"}}),
            ),
            event(
                1,
                "session.model_change",
                json!({"newModel": "auto", "source": "automatic"}),
            ),
            event(2, "hook.start", json!({"hookType": "sessionStart"})),
            event(
                3,
                "user.message",
                json!({"content": "Fix the uploader", "transformedContent": "<current_datetime>…</current_datetime>\n\nFix the uploader", "turnId": "0"}),
            ),
            event(
                4,
                "system.message",
                json!({"content": "INJECTED BY COPILOT", "role": "system"}),
            ),
            event(
                5,
                "model.message",
                json!({"message": {"reasoning": "COPILOT REASONING"}}),
            ),
            event(
                6,
                "assistant.message",
                json!({"content": "I'll look.", "model": "mai-code-1.1-flash", "toolRequests": [
                {"toolCallId": "c1", "name": "view", "arguments": {"path": "src/main.rs"}}], "turnId": "0"}),
            ),
            event(
                7,
                "tool.execution_complete",
                json!({"toolCallId": "c1", "success": true, "result": {"content": "fn main() {}"}}),
            ),
            event(
                8,
                "assistant.message",
                json!({"content": "Done.", "model": "mai-code-1.1-flash", "toolRequests": []}),
            ),
            event(
                9,
                "tool.user_requested",
                json!({"toolCallId": "u1", "toolName": "local_shell", "arguments": {"command": "ls"}}),
            ),
            event(
                10,
                "tool.execution_complete",
                json!({"toolCallId": "u1", "isUserRequested": true, "success": true, "result": {"content": "Cargo.toml\nsrc"}}),
            ),
            event(11, "session.shutdown", json!({"shutdownType": "normal"})),
        ];
        let text: String = events.iter().map(|e| e.to_string() + "\n").collect();
        std::fs::write(dir.join("events.jsonl"), text).unwrap();
        dir.to_string_lossy().into_owned()
    }

    #[test]
    fn names_its_files_and_reads_workspace_yaml() {
        assert!(is_session_name(ID));
        assert!(!is_session_name(".session-operation-locks"));
        assert_eq!(
            file_id(&format!("/h/.copilot/session-state/{ID}/events.jsonl")).as_deref(),
            Some(ID)
        );
        assert_eq!(
            file_id(&format!("/h/.copilot/session-state/{ID}/workspace.yaml")),
            None
        );
        assert_eq!(
            home_of(&format!("/h/.copilot/session-state/{ID}/events.jsonl")),
            "/h/.copilot"
        );
        let fields =
            yaml_fields("id: x\nname: \"A \\\"quoted\\\" name\"\nbranch: ~\nother: 'it''s'\n");
        assert_eq!(fields["name"], "A \"quoted\" name");
        assert_eq!(fields["branch"], "");
        assert_eq!(fields["other"], "it's");
    }

    #[test]
    fn reads_a_session_and_leaves_copilots_own_records_out() {
        let dir = std::env::temp_dir().join(format!("hatcheck-copilot-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let folder = session_folder(&dir, Some("Uploader fix"));
        let home = dir.to_string_lossy().into_owned();
        let x = read(&folder).unwrap();
        assert_eq!((x.id.as_str(), x.cwd.as_deref()), (ID, Some("/work/api")));
        assert_eq!(x.title.as_deref(), Some("Uploader fix"));
        assert_eq!(x.first_prompt.as_deref(), Some("Fix the uploader"));
        assert_eq!(x.model, json!("mai-code-1.1-flash"));
        assert_eq!(entrypoint(&x), "cli");
        assert!(x.has_messages);
        assert_eq!(sessions(&home).len(), 1);
        let conv = conversation(&x).unwrap();
        assert_eq!(conv.calls, 1);
        let text = conv
            .turns
            .iter()
            .map(|t| t.1.join("\n"))
            .collect::<Vec<_>>()
            .join("\n");
        for expected in [
            "Fix the uploader",
            "I'll look.",
            "view",
            "fn main() {}",
            "Done.",
            "!ls",
            "Cargo.toml",
        ] {
            assert!(text.contains(expected), "{expected}");
        }
        for left_out in [
            "INJECTED BY COPILOT",
            "COPILOT REASONING",
            "current_datetime",
        ] {
            assert!(!text.contains(left_out), "{left_out}");
        }
        assert_eq!(conv.title.as_deref(), Some("Uploader fix"));
        assert!(!session_open(&folder));
        // A session copilot started and never spoke in has no events file: not a session.
        let empty = dir
            .join("session-state")
            .join("73c5fe6b-a7e7-4dda-88b9-1c5289a50a61");
        std::fs::create_dir_all(&empty).unwrap();
        std::fs::write(
            empty.join("workspace.yaml"),
            "id: 73c5fe6b-a7e7-4dda-88b9-1c5289a50a61\n",
        )
        .unwrap();
        assert_eq!(sessions(&home).len(), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn writes_the_interchange_form_its_importer_takes() {
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
            (
                "assistant".into(),
                vec!["Hi.".into(), "More.".into()],
                json!(null),
            ),
        ];
        let dir =
            std::env::temp_dir().join(format!("hatcheck-copilot-write-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::env::set_var("HATCHECK_CACHE_DIR", &dir);
        let (id, path) = write_session(&conv, &turns, "/h/.copilot", "/work/api").unwrap();
        std::env::remove_var("HATCHECK_CACHE_DIR");
        assert!(path.ends_with(&format!("{id}.jsonl")));
        let text = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<Value> = text
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0]["type"], "session");
        assert_eq!(lines[0]["version"], 1);
        assert_eq!(lines[0]["externalId"], json!(id));
        assert_eq!(lines[0]["createdAt"], "2026-10-25T19:55:05.842Z");
        assert_eq!(lines[0]["title"], "A title");
        assert_eq!(lines[0]["workspace"]["workingDirectory"], "/work/api");
        assert_eq!(lines[0]["workspace"]["branch"], "main");
        assert_eq!(lines[1]["role"], "user");
        assert_eq!(
            lines[1]["content"],
            json!([{"type": "text", "text": "Hello"}])
        );
        assert_eq!(lines[1]["timestamp"], "2026-10-25T19:55:06.000Z");
        assert_eq!(lines[2]["role"], "assistant");
        assert_eq!(lines[2]["content"][0]["text"], "Hi.\n\nMore.");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
