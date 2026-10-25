//! Qwen Code (qwen) as hatcheck reads it. The home is ~/.qwen, moved whole by QWEN_HOME. A
//! project's sessions are projects/<the folder's path with every character that isn't a letter
//! or digit made a dash>/chats/<uuid>.jsonl, archived ones in chats/archive/: a record per line,
//! with uuid, parentUuid, type, timestamp, cwd and gitBranch, and for a message a Gemini-style
//! `parts` list. sessions/<pid>.json is qwen's registry of the sessions running now, removed on
//! exit, and chats/<uuid>.runtime.json the sidecar its terminal UI writes and never removes; both
//! name the process. hatcheck reads all of it and writes none of it: qwen has no importer, so no
//! session is written for it. docs/agents.md's `Qwen Code` section says what is read, and that it
//! was checked against qwen 0.26.0.

use crate::agent::Agent;
use crate::conv::{self, Conversation};
use crate::facts::Summary;
use crate::ospath::{self, Part};
use crate::paths;
use crate::py::{self, get, s, truthy};
use crate::times;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};

/// ~/.qwen, where plain qwen keeps everything.
pub fn default_home() -> String {
    ospath::join(&paths::home(), Agent::Qwen.base())
}

/// Whether a folder is a qwen home: it has the installation_id qwen writes on its first run, or
/// a project's chats under projects/. A Claude Code profile has projects/ too, but neither.
pub fn is_home(path: &str) -> bool {
    ospath::isfile(&ospath::join(path, "installation_id"))
        || !ospath::glob(
            path,
            &[
                Part::Name("projects"),
                Part::Wild("", ""),
                Part::Name("chats"),
            ],
        )
        .is_empty()
}

fn override_home() -> Option<String> {
    std::env::var("QWEN_HOME")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .map(|v| ospath::expanduser(&v))
}

/// Every qwen home: ~/.qwen, ~/.qwen-NAME and the folder QWEN_HOME names, the default first.
pub fn find_homes() -> Vec<String> {
    let h = paths::home();
    let mut candidates = ospath::glob(&h, &[Part::Name(Agent::Qwen.base())]);
    candidates.extend(ospath::glob(
        &h,
        &[Part::Wild(&format!("{}-", Agent::Qwen.base()), "")],
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

/// qwen's name for a project folder: its path, lowercased on Windows, with every character that
/// isn't an ASCII letter or digit made a dash.
pub fn project_name(cwd: &str) -> String {
    let cwd = if ospath::WINDOWS {
        cwd.to_lowercase()
    } else {
        cwd.to_string()
    };
    cwd.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

/// Whether a file name is one of qwen's session transcripts: <uuid>.jsonl, as qwen's own list
/// takes them (32 to 36 hex digits and dashes).
fn is_transcript_name(name: &str) -> bool {
    let Some(stem) = name.strip_suffix(".jsonl") else {
        return false;
    };
    (32..=36).contains(&stem.len()) && stem.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-')
}

/// The session ID of a transcript at this path, when the path is one of qwen's: under a chats
/// folder, or its archive.
pub fn file_id(path: &str) -> Option<String> {
    let name = ospath::basename(path);
    if !is_transcript_name(&name) {
        return None;
    }
    let parent = ospath::dirname(path);
    let under_chats = ospath::basename(&parent) == "chats"
        || (ospath::basename(&parent) == "archive"
            && ospath::basename(&ospath::dirname(&parent)) == "chats");
    under_chats.then(|| name.trim_end_matches(".jsonl").to_lowercase())
}

/// The home a transcript belongs to: four folders up (five for an archived one).
pub fn home_of(path: &str) -> String {
    let archived = ospath::basename(&ospath::dirname(path)) == "archive";
    let mut home = path.to_string();
    for _ in 0..(if archived { 5 } else { 4 }) {
        home = ospath::dirname(&home);
    }
    home
}

/// A session, from what its transcript says.
#[derive(Clone, Debug)]
pub struct Session {
    pub id: String,
    pub file: String,
    pub archived: bool,
    /// The project folder's name in the home, as qwen filed the session.
    pub project: String,
    pub cwd: Option<String>,
    pub branch: Value,
    pub started: Value,
    pub when: f64,
    pub title: Option<String>,
    pub first_prompt: Option<String>,
    pub model: Value,
    pub entrypoint: String,
    pub has_messages: bool,
    pub size: u64,
}

/// The first text part of a message, as qwen takes a prompt for its list.
fn first_text(message: &Value) -> Option<String> {
    get(message, "parts")
        .as_array()?
        .iter()
        .find_map(|p| s(get(p, "text")).map(String::from))
        .filter(|t| !py::strip(t).is_empty())
}

/// Every complete line of a transcript as JSON, leaving a torn last line alone.
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

/// How the list files a session, from qwen's own records: a subagent's transcript names its
/// parent session, or comes from an agent source; a channel's, a scheduled task's or another
/// host's names its source; the rest are the terminal's.
fn entrypoint_of(records: &[Value]) -> String {
    let mut source: Option<String> = None;
    for r in records {
        if s(get(r, "type")) != Some("system") {
            continue;
        }
        let payload = get(r, "systemPayload");
        match s(get(r, "subtype")) {
            Some("parent_session") if truthy(get(payload, "parentSessionId")) => {
                return "subagent".into();
            }
            Some("session_source") => {
                if let Some(kind) = s(get(payload, "sourceType")) {
                    source = Some(kind.to_string());
                }
            }
            _ => {}
        }
    }
    match source.as_deref() {
        Some("agent" | "agent-host" | "side_task") => "subagent".into(),
        Some("standalone" | "default") | None => "cli".into(),
        Some(other) => other.to_string(),
    }
}

/// A session read from its transcript, or None when the file holds no records.
pub fn read(path: &str, archived: bool) -> Option<Session> {
    let id = file_id(path)?;
    let recs = records(path).ok()?;
    let first = recs.first()?;
    let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let mut title: Option<String> = None;
    let mut first_prompt: Option<String> = None;
    let mut model = Value::Null;
    let mut branch = Value::Null;
    let mut last_stamp: Option<f64> = None;
    let mut has_messages = false;
    for r in &recs {
        if let Some(t) = s(get(r, "timestamp")).and_then(times::parse_time) {
            last_stamp = Some(t);
        }
        if truthy(get(r, "gitBranch")) {
            branch = get(r, "gitBranch").clone();
        }
        match s(get(r, "type")) {
            Some("user") => {
                has_messages = true;
                if first_prompt.is_none() && get(r, "subtype").is_null() {
                    let shown = get(get(r, "systemPayload"), "displayText");
                    first_prompt = if shown.is_string() {
                        s(shown).map(String::from).filter(|t| !t.is_empty())
                    } else {
                        first_text(get(r, "message"))
                    };
                }
            }
            Some("assistant") => {
                has_messages = true;
                if truthy(get(r, "model")) {
                    model = get(r, "model").clone();
                }
            }
            Some("system") => match s(get(r, "subtype")) {
                Some("custom_title") => {
                    let custom = get(get(r, "systemPayload"), "customTitle");
                    if truthy(custom) {
                        title = Some(py::str_of(custom));
                    }
                }
                Some("session_model") if model.is_null() => {
                    let named = get(get(r, "systemPayload"), "modelId");
                    if truthy(named) {
                        model = named.clone();
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }
    let project = if archived {
        ospath::basename(&ospath::dirname(&ospath::dirname(&ospath::dirname(path))))
    } else {
        ospath::basename(&ospath::dirname(&ospath::dirname(path)))
    };
    Some(Session {
        id,
        file: path.to_string(),
        archived,
        project,
        cwd: s(get(first, "cwd"))
            .map(String::from)
            .filter(|c| !c.is_empty()),
        branch,
        started: get(first, "timestamp").clone(),
        when: last_stamp.unwrap_or_else(|| ospath::getmtime(path).unwrap_or(0.0)),
        title,
        first_prompt,
        model,
        entrypoint: entrypoint_of(&recs),
        has_messages,
        size,
    })
}

/// Every session in a home, the newest change first: each project's chats, and their archives.
pub fn sessions(home: &str) -> Vec<Session> {
    let mut found = Vec::new();
    for (parts, archived) in [
        (
            vec![
                Part::Name("projects"),
                Part::Wild("", ""),
                Part::Name("chats"),
                Part::Wild("", ".jsonl"),
            ],
            false,
        ),
        (
            vec![
                Part::Name("projects"),
                Part::Wild("", ""),
                Part::Name("chats"),
                Part::Name("archive"),
                Part::Wild("", ".jsonl"),
            ],
            true,
        ),
    ] {
        for path in ospath::glob(home, &parts) {
            if let Some(x) = read(&path, archived) {
                found.push(x);
            }
        }
    }
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

/// The summary of the transcript at path, or None when it isn't one of the home's.
pub fn summarize(home: &str, path: &str) -> Option<Summary> {
    let real = ospath::realpath(path);
    sessions(home)
        .into_iter()
        .find(|x| ospath::realpath(&x.file) == real)
        .map(|x| summary(home, &x))
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
    let canonical = x
        .cwd
        .as_deref()
        .is_some_and(|c| project_name(&ospath::realpath(c)) == x.project);
    Summary {
        agent: Agent::Qwen,
        id: x.id.clone(),
        when: x.when,
        entrypoint: json!(x.entrypoint),
        branch: x.branch.clone(),
        cwd: x.cwd.clone(),
        canonical,
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

/// Whether a process is running: asked of ps, as for Claude Code's registry; on Windows, of the
/// process itself.
fn pid_alive(pid: u64) -> bool {
    if ospath::WINDOWS {
        return u32::try_from(pid).is_ok_and(|p| crate::sessions::windows_start_time(p).is_some());
    }
    std::process::Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "pid="])
        .output()
        .map(|o| !String::from_utf8_lossy(&o.stdout).trim().is_empty())
        .unwrap_or(false)
}

/// Whether a registry file name is one qwen writes: <pid>.json, or <pid>-<8 hex>.json from a
/// process hosting several sessions. Anything else in the folder is left alone.
fn is_registry_name(name: &str) -> bool {
    let Some(stem) = name.strip_suffix(".json") else {
        return false;
    };
    let (digits, suffix) = stem.split_once('-').unwrap_or((stem, ""));
    !digits.is_empty()
        && digits.bytes().all(|b| b.is_ascii_digit())
        && (suffix.is_empty()
            || (suffix.len() == 8 && suffix.bytes().all(|b| b.is_ascii_hexdigit())))
}

/// Whether qwen has the session open: a registry record of a running process names it, or the
/// sidecar its terminal UI wrote beside the transcript names a running process. A record of a
/// process that has gone isn't open.
pub fn session_open(home: &str, file: &str, id: &str) -> bool {
    let id = id.to_lowercase();
    let names =
        |value: &Value, key: &str| s(get(value, key)).map(str::to_lowercase) == Some(id.clone());
    let pid_of = |value: &Value| get(value, "pid").as_u64();
    for path in ospath::glob(home, &[Part::Name("sessions"), Part::Wild("", ".json")]) {
        if !is_registry_name(&ospath::basename(&path)) {
            continue;
        }
        let Some(record) = std::fs::read_to_string(&path)
            .ok()
            .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        else {
            continue;
        };
        if names(&record, "sessionId") && pid_of(&record).is_some_and(pid_alive) {
            return true;
        }
    }
    let sidecar = ospath::join(&ospath::dirname(file), &format!("{id}.runtime.json"));
    if let Some(record) = std::fs::read_to_string(&sidecar)
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
    {
        if names(&record, "session_id") && pid_of(&record).is_some_and(pid_alive) {
            return true;
        }
    }
    false
}

/// Whether a record is one of the user's own messages: qwen marks what it adds to the
/// conversation itself with another provenance.
fn is_users(record: &Value) -> bool {
    matches!(s(get(record, "provenance")), None | Some("real_user"))
}

/// A tool's result as text: its output, or the whole response as JSON.
fn response_text(response: &Value) -> String {
    match get(response, "output") {
        Value::String(text) => text.clone(),
        Value::Null if response.is_string() => py::str_of(response),
        Value::Null => py::dumps_compact(response),
        other => py::dumps_compact(other),
    }
}

/// The conversation: the messages on the chain of parentUuid links ending at the last one, so a
/// rewound session reads as qwen resumes it; thoughts, sidechains and qwen's own records left out.
fn conversation(x: &Session) -> std::io::Result<Conversation> {
    let recs = records(&x.file)?;
    let by_id: HashMap<String, usize> = recs
        .iter()
        .enumerate()
        .filter(|(_, r)| truthy(get(r, "uuid")))
        .map(|(n, r)| (py::str_of(get(r, "uuid")), n))
        .collect();
    let is_message = |r: &Value| {
        matches!(
            s(get(r, "type")),
            Some("user" | "assistant" | "tool_result")
        )
    };
    let mut node = recs
        .iter()
        .rposition(|r| is_message(r) && !truthy(get(r, "isSidechain")));
    let mut chain: Vec<usize> = Vec::new();
    if let Some(n) = node {
        if !truthy(get(&recs[n], "uuid")) {
            chain = (0..recs.len())
                .rev()
                .filter(|&k| is_message(&recs[k]))
                .collect();
            node = None;
        }
    }
    let mut seen = HashSet::new();
    while let Some(n) = node {
        if !seen.insert(n) {
            break;
        }
        chain.push(n);
        let parent = get(&recs[n], "parentUuid");
        node = if truthy(parent) {
            by_id.get(&py::str_of(parent)).copied()
        } else {
            None
        };
    }
    let (mut turns, mut calls, mut model) = (Vec::new(), 0, Value::Null);
    for &k in chain.iter().rev() {
        let r = &recs[k];
        if !is_message(r) || truthy(get(r, "isSidechain")) {
            continue;
        }
        let kind = s(get(r, "type")).unwrap_or("");
        if kind == "user" && !is_users(r) {
            continue;
        }
        let when = get(r, "timestamp");
        if kind == "assistant" && truthy(get(r, "model")) {
            model = get(r, "model").clone();
        }
        let role = if kind == "user" { "user" } else { "assistant" };
        let Some(parts) = get(get(r, "message"), "parts").as_array() else {
            continue;
        };
        for part in parts {
            if !part.is_object() || truthy(get(part, "thought")) {
                continue; // a thought is qwen's own, as thinking is Claude's
            }
            if let Some(text) = s(get(part, "text")) {
                conv::add_turn(&mut turns, role, text, when);
            } else if get(part, "functionCall").is_object() {
                let call = get(part, "functionCall");
                let name = s(get(call, "name")).unwrap_or("tool");
                let fields: Vec<(String, Value)> = match get(call, "args") {
                    Value::Object(o) => o.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
                    Value::Null => Vec::new(),
                    other => vec![("input".into(), other.clone())],
                };
                conv::add_turn(
                    &mut turns,
                    "assistant",
                    &conv::call_text(name, &fields),
                    when,
                );
                calls += 1;
            } else if get(part, "functionResponse").is_object() {
                let output = response_text(get(get(part, "functionResponse"), "response"));
                if !py::strip(&output).is_empty() {
                    conv::add_turn(&mut turns, "assistant", &conv::result_text(&output), when);
                }
            } else if get(part, "inlineData").is_object() || get(part, "fileData").is_object() {
                let mime = py::or(
                    get(get(part, "inlineData"), "mimeType"),
                    get(get(part, "fileData"), "mimeType"),
                );
                let what = if s(mime).is_some_and(|m| m.starts_with("image/")) {
                    "[image]"
                } else {
                    "[file]"
                };
                conv::add_turn(&mut turns, role, what, when);
            }
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
        agent: Agent::Qwen,
        id: x.id.clone(),
        cwd: x.cwd.clone(),
        model,
        turns,
        calls,
        title,
        started: x.started.clone(),
        branch: x.branch.clone(),
        reasoning: "Qwen Code's thoughts",
        left_out: "Qwen Code's thoughts, images and files, what qwen adds to the conversation itself, its own records and subagent sessions",
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

/// The first prompt for a conversation converted for qwen: the file to read, and to carry on.
pub fn prompt(file: &str) -> String {
    crate::conv::handover_prompt(file)
}

/// The command that starts qwen on an imported conversation, from the session's folder: the
/// imports folder added to the workspace, so qwen may read the file, and a first prompt that
/// has it carry on from there (-i keeps the terminal UI open afterwards). The session qwen
/// makes is its own, with its own ID, so hatcheck writes nothing in qwen's store.
pub fn start_command(home: &str, file: &str, full: bool) -> String {
    paths::qwen_command(
        home,
        &format!(
            "--add-dir {} -i {}",
            paths::shell_path(&conv::imports_dir()),
            paths::quote(&prompt(file))
        ),
        full,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "c6f30025-d241-4388-a849-acb3ea844c99";

    fn line(n: usize, parent: Option<usize>, kind: &str, rest: Value) -> Value {
        let mut r = json!({
            "uuid": format!("u{n}"), "parentUuid": parent.map(|p| format!("u{p}")), "sessionId": ID,
            "timestamp": format!("2026-10-25T01:11:{:02}.000Z", n), "type": kind,
            "cwd": "/work/api", "version": "0.26.0", "gitBranch": "main",
        });
        for (k, v) in rest.as_object().unwrap() {
            r[k] = v.clone();
        }
        r
    }

    fn transcript(dir: &std::path::Path, extra: Vec<Value>) -> String {
        let chats = dir.join("projects").join("-work-api").join("chats");
        std::fs::create_dir_all(&chats).unwrap();
        let path = chats.join(format!("{ID}.jsonl"));
        let mut rows = vec![
            line(
                0,
                None,
                "system",
                json!({"subtype": "slash_command", "provenance": "system", "systemPayload": {"rawCommand": "/model"}}),
            ),
            line(
                1,
                Some(0),
                "user",
                json!({"provenance": "real_user", "message": {"role": "user", "parts": [{"text": "Fix the uploader"}]}}),
            ),
            line(
                2,
                Some(1),
                "user",
                json!({"provenance": "goal_runtime", "subtype": "agent_message", "message": {"role": "user", "parts": [{"text": "INJECTED BY QWEN"}]}}),
            ),
            line(
                3,
                Some(2),
                "assistant",
                json!({"provenance": "assistant_output", "model": "GLM-5.3", "message": {"role": "model", "parts": [
                {"text": "a thought kept to itself", "thought": true},
                {"text": "I'll look."},
                {"functionCall": {"id": "c1", "name": "read_file", "args": {"path": "src/main.rs"}}}]}}),
            ),
            line(
                4,
                Some(3),
                "tool_result",
                json!({"provenance": "tool_result", "message": {"role": "user", "parts": [
                {"functionResponse": {"id": "c1", "name": "read_file", "response": {"output": "fn main() {}"}}}]}}),
            ),
            line(
                5,
                Some(4),
                "assistant",
                json!({"provenance": "subagent", "isSidechain": true, "agentId": "explore-1", "message": {"role": "model", "parts": [{"text": "SIDECHAIN TEXT"}]}}),
            ),
            line(
                6,
                Some(4),
                "assistant",
                json!({"provenance": "assistant_output", "model": "GLM-5.3", "message": {"role": "model", "parts": [{"text": "Done."}]}}),
            ),
        ];
        rows.extend(extra);
        let text: String = rows.iter().map(|r| r.to_string() + "\n").collect();
        std::fs::write(&path, text).unwrap();
        path.to_string_lossy().into_owned()
    }

    #[test]
    fn names_projects_as_qwen_does_and_knows_its_files() {
        // Lowercased first on Windows, as qwen names it there.
        let named = if ospath::WINDOWS {
            "-volumes-x-repo"
        } else {
            "-Volumes-x-repo"
        };
        assert_eq!(project_name("/Volumes/x/repo"), named);
        assert!(is_transcript_name(&format!("{ID}.jsonl")));
        assert!(!is_transcript_name(&format!("{ID}.runtime.json")));
        assert!(!is_transcript_name("notes.jsonl"));
        assert_eq!(
            file_id(&format!("/h/.qwen/projects/-x/chats/{ID}.jsonl")).as_deref(),
            Some(ID)
        );
        assert_eq!(
            file_id(&format!("/h/.qwen/projects/-x/chats/archive/{ID}.jsonl")).as_deref(),
            Some(ID)
        );
        assert_eq!(file_id(&format!("/h/.claude/projects/-x/{ID}.jsonl")), None);
        assert_eq!(
            home_of(&format!("/h/.qwen/projects/-x/chats/{ID}.jsonl")),
            "/h/.qwen"
        );
        assert_eq!(
            home_of(&format!("/h/.qwen/projects/-x/chats/archive/{ID}.jsonl")),
            "/h/.qwen"
        );
        assert!(is_registry_name("19434.json"));
        assert!(is_registry_name("19434-0a1b2c3d.json"));
        assert!(!is_registry_name("2026-planning-notes.json"));
        assert!(!is_registry_name("19434.json.0a1b2c3d4e5f.tmp"));
    }

    #[test]
    fn reads_a_transcript_and_leaves_thoughts_and_sidechains_out() {
        let dir = std::env::temp_dir().join(format!("hatcheck-qwen-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = transcript(
            &dir,
            vec![line(
                7,
                Some(6),
                "system",
                json!({"subtype": "custom_title", "provenance": "system", "systemPayload": {"customTitle": "Uploader fix", "titleSource": "auto"}}),
            )],
        );
        let home = dir.to_string_lossy().into_owned();
        let x = read(&path, false).unwrap();
        assert_eq!(
            (x.id.as_str(), x.project.as_str(), x.archived),
            (ID, "-work-api", false)
        );
        assert_eq!(x.cwd.as_deref(), Some("/work/api"));
        assert_eq!(x.title.as_deref(), Some("Uploader fix"));
        assert_eq!(x.first_prompt.as_deref(), Some("Fix the uploader"));
        assert_eq!(x.model, json!("GLM-5.3"));
        assert_eq!(x.entrypoint, "cli");
        assert!(x.has_messages);
        let listed = sessions(&home);
        assert_eq!(listed.len(), 1);
        let conv = conversation(&x).unwrap();
        assert_eq!((conv.calls, conv.turns.len()), (1, 2)); // the replies join into one turn
        let text = conv
            .turns
            .iter()
            .map(|t| t.1.join("\n"))
            .collect::<Vec<_>>()
            .join("\n");
        for expected in [
            "Fix the uploader",
            "I'll look.",
            "read_file",
            "fn main() {}",
            "Done.",
        ] {
            assert!(text.contains(expected), "{expected}");
        }
        for left_out in ["kept to itself", "INJECTED BY QWEN", "SIDECHAIN TEXT"] {
            assert!(!text.contains(left_out), "{left_out}");
        }
        assert_eq!(conv.title.as_deref(), Some("Uploader fix"));
        assert!(!session_open(&home, &path, ID));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_subagents_transcript_and_a_channels_are_filed_by_their_records() {
        let parent = vec![line(
            0,
            None,
            "system",
            json!({"subtype": "parent_session", "provenance": "system", "systemPayload": {"parentSessionId": "p"}}),
        )];
        assert_eq!(entrypoint_of(&parent), "subagent");
        let agent = vec![line(
            0,
            None,
            "system",
            json!({"subtype": "session_source", "provenance": "system", "systemPayload": {"sourceType": "agent"}}),
        )];
        assert_eq!(entrypoint_of(&agent), "subagent");
        let channel = vec![line(
            0,
            None,
            "system",
            json!({"subtype": "session_source", "provenance": "system", "systemPayload": {"sourceType": "channel", "sourceId": "telegram"}}),
        )];
        assert_eq!(entrypoint_of(&channel), "channel");
        let plain = vec![line(
            0,
            None,
            "user",
            json!({"provenance": "real_user", "message": {"parts": [{"text": "hi"}]}}),
        )];
        assert_eq!(entrypoint_of(&plain), "cli");
    }
}
