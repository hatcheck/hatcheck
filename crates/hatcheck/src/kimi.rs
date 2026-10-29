//! Kimi Code CLI (kimi) as hatcheck reads and writes it. Kimi keeps everything in one home
//! folder, ~/.kimi-code, which KIMI_CODE_HOME moves whole, so a second account is a second home,
//! ~/.kimi-code-NAME, as a Codex home is. In it: sessions/<wd_<name>_<hash>>/session_<uuid>/, one
//! folder per session with state.json (the title, the folder it ran in, its times, and an
//! absolute path into the home) and agents/main/wire.jsonl, the event stream kimi resumes from,
//! with a wire per subagent beside it; session_index.jsonl at the root, one line per session with
//! absolute paths, which `kimi --session` resolves a session through; and credentials/, which
//! hatcheck never reads. Nothing marks a session open. docs/agents.md's `Kimi Code CLI` section
//! says what is read and written, and that it was checked against kimi 2.2.0 and again 2.3.1.

use crate::agent::Agent;
use crate::conv::{self, Conversation, Turn};
use crate::facts::{self, Summary};
use crate::ospath::{self, Part};
use crate::paths;
use crate::py::{self, get, s, truthy, Num};
use crate::times;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::io::Write;

const SESSIONS: &str = "sessions";
const INDEX: &str = "session_index.jsonl";
/// The protocol version kimi 2.2.0's own importer gives a wire it writes (its live sessions
/// say 1.5).
const PROTOCOL: &str = "1.0";

/// ~/.kimi-code.
pub fn default_home() -> String {
    ospath::join(&paths::home(), Agent::Kimi.base())
}

/// Whether a folder is a Kimi home: named like one, or with kimi's sessions/ folders in it,
/// sessions/wd_<name>_<hash> (Codex's sessions/ holds years).
pub fn is_home(path: &str) -> bool {
    let base = ospath::basename(path.trim_end_matches(ospath::is_sep));
    base.starts_with(Agent::Kimi.base())
        || !ospath::glob(path, &[Part::Name(SESSIONS), Part::Wild("wd_", "")]).is_empty()
}

/// Every Kimi home with sessions: ~/.kimi-code and ~/.kimi-code-* folders (and
/// $KIMI_CODE_HOME) that hold a sessions/ folder, ~/.kimi-code first, the rest by name.
pub fn find_homes() -> Vec<String> {
    let h = paths::home();
    let mut candidates = ospath::glob(&h, &[Part::Name(Agent::Kimi.base())]);
    candidates.extend(ospath::glob(
        &h,
        &[Part::Wild(&format!("{}-", Agent::Kimi.base()), "")],
    ));
    if let Some(dir) = std::env::var("KIMI_CODE_HOME")
        .ok()
        .filter(|v| !v.is_empty())
    {
        candidates.push(ospath::expanduser(&dir));
    }
    let mut seen: Vec<(String, String)> = Vec::new();
    for d in candidates {
        if ospath::isdir(&ospath::join(&d, SESSIONS)) {
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

/// The folder kimi files a working directory's sessions under: wd_<its name>_<the first 12 hex
/// digits of the SHA-256 of its absolute path>. Checked against kimi 2.2.0 for a plain name;
/// a name with characters outside letters, digits, dots and dashes is guessed at.
pub fn wd_key(cwd: &str) -> String {
    let name: String = ospath::basename(cwd.trim_end_matches(ospath::is_sep))
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || "._-".contains(c) {
                c
            } else {
                '-'
            }
        })
        .collect();
    let hash = format!("{:x}", Sha256::digest(cwd.as_bytes()));
    format!("wd_{name}_{}", &hash[..12])
}

/// One session, as its folder holds it.
#[derive(Clone, Debug)]
pub struct Session {
    /// The UUID after session_.
    pub id: String,
    /// sessions/<wd key>/session_<id>, the folder.
    pub dir: String,
    /// Its state.json.
    pub state: Value,
}

/// The session folder for an ID in a home, under the working directory's key.
pub fn session_dir(home: &str, cwd: &str, id: &str) -> String {
    ospath::join_all(home, &[SESSIONS, &wd_key(cwd), &Agent::Kimi.full_id(id)])
}

/// The main agent's event stream in a session folder.
pub fn wire_path(dir: &str) -> String {
    ospath::join_all(dir, &["agents", "main", "wire.jsonl"])
}

/// The ID in a session folder's path, sessions/<wd key>/session_<id>.
pub fn dir_id(path: &str) -> Option<&str> {
    let name = path
        .trim_end_matches(ospath::is_sep)
        .rsplit(['/', '\\'])
        .next()?;
    let id = name.strip_prefix(Agent::Kimi.id_prefix())?;
    let hex = |c: char| c.is_ascii_hexdigit() || c == '-';
    (id.len() == 36 && id.chars().all(hex)).then_some(id)
}

fn read_state(dir: &str) -> Option<Value> {
    let text = std::fs::read_to_string(ospath::join(dir, "state.json")).ok()?;
    serde_json::from_str::<Value>(&text)
        .ok()
        .filter(Value::is_object)
}

/// Every session in a home, from the session folders, not the index (which kimi has had
/// break), the newest change first.
pub fn sessions(home: &str) -> Vec<Session> {
    let mut found: Vec<Session> = ospath::glob(
        home,
        &[
            Part::Name(SESSIONS),
            Part::Wild("wd_", ""),
            Part::Wild(Agent::Kimi.id_prefix(), ""),
        ],
    )
    .into_iter()
    .filter_map(|dir| {
        let id = dir_id(&dir)?.to_string();
        let state = read_state(&dir)?;
        Some(Session { id, dir, state })
    })
    .collect();
    found.sort_by(|a, b| {
        updated(b)
            .partial_cmp(&updated(a))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.id.cmp(&b.id))
    });
    found
}

/// The sessions whose ID starts with prefix, the agent's own prefix left off.
pub fn sessions_starting_with(home: &str, prefix: &str) -> Vec<Session> {
    let lower = Agent::Kimi.bare_id(prefix).to_lowercase();
    sessions(home)
        .into_iter()
        .filter(|x| x.id.starts_with(&lower))
        .collect()
}

pub fn session(home: &str, id: &str) -> Option<Session> {
    sessions(home).into_iter().find(|x| x.id == id)
}

/// A time in ms since the epoch, as state.json keeps them, in seconds; 0 when it isn't one.
fn ms(v: &Value) -> f64 {
    Num::of(v).map_or(0.0, |n| n.f() / 1000.0)
}

fn updated(x: &Session) -> f64 {
    let t = ms(get(&x.state, "updatedAt"));
    if t != 0.0 {
        t
    } else {
        ospath::getmtime(&wire_path(&x.dir)).unwrap_or(0.0)
    }
}

/// What the session list needs from a wire, read from start on with old for what came before:
/// the newest model asked, and how many prompts were typed.
fn wire_facts(data: &[u8], old: Option<&Value>, start: usize) -> Value {
    let model = facts::last_record(data, "llm.request", start)
        .map(|r| get(&r, "modelAlias").clone())
        .filter(truthy)
        .or_else(|| old.map(|o| get(o, "model").clone()))
        .unwrap_or(Value::Null);
    let prompts = memchr::memmem::find_iter(&data[start..], b"\"type\":\"turn.prompt\"").count()
        + old.and_then(|o| get(o, "prompts").as_u64()).unwrap_or(0) as usize;
    json!({"model": model, "prompts": prompts})
}

/// A session as the session list shows it.
pub fn summary(home: &str, x: &Session) -> Summary {
    let wire = wire_path(&x.dir);
    let (facts, size) = facts::file_facts(&wire, "kimi", wire_facts)
        .ok()
        .map_or((Value::Null, 0), |(f, n)| (f.unwrap_or(Value::Null), n));
    let title = [get(&x.state, "title"), get(&x.state, "lastPrompt")]
        .into_iter()
        .filter_map(s)
        .map(py::collapse)
        .find(|t| !t.is_empty())
        .unwrap_or_else(|| "(untitled)".into());
    let prompts = get(&facts, "prompts").as_u64().unwrap_or(0);
    Summary {
        agent: Agent::Kimi,
        id: x.id.clone(),
        when: updated(x),
        entrypoint: json!("cli"),
        branch: Value::Null,
        cwd: s(get(&x.state, "cwd"))
            .filter(|c| !c.is_empty())
            .map(String::from),
        canonical: true,
        size,
        has_messages: prompts > 0 || truthy(get(&x.state, "lastPrompt")),
        title,
        continued_in: Value::Null,
        model: get(&facts, "model").clone(),
        profile: home.to_string(),
        path: x.dir.clone(),
        open: None,
        bound_to: None,
        cost: None,
        spend: None,
    }
}

/// The summary of the session kept in a folder, or None when it isn't one.
pub fn summarize(home: &str, path: &str) -> Option<Summary> {
    let id = dir_id(path)?.to_string();
    let state = read_state(path)?;
    Some(summary(
        home,
        &Session {
            id,
            dir: path.to_string(),
            state,
        },
    ))
}

/// A time as kimi stamps events, ms since the epoch, as text for a turn.
fn stamp(v: &Value) -> Value {
    match Num::of(v) {
        Some(n) => times::iso_utc(n.f() / 1000.0).map_or(Value::Null, Value::String),
        None => Value::Null,
    }
}

/// A conversation as hatcheck shows and converts it, from the main agent's wire: what you
/// typed, what the model said, each tool call as text with its arguments, and each tool's
/// output. Kimi's thinking is left out, as the other agents' reasoning is.
pub fn read_conversation(home: &str, id: &str) -> std::io::Result<Conversation> {
    let x = session(home, id).ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("no session {id} in {home}"),
        )
    })?;
    let data = std::fs::read(wire_path(&x.dir))?;
    let text = String::from_utf8_lossy(&data);
    let (mut turns, mut calls, mut model): (Vec<Turn>, usize, Value) = (Vec::new(), 0, Value::Null);
    for line in text.lines() {
        let Ok(event) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if !event.is_object() {
            continue;
        }
        match s(get(&event, "type")) {
            Some("llm.request") => {
                if truthy(get(&event, "modelAlias")) {
                    model = get(&event, "modelAlias").clone();
                }
            }
            Some("agent.message.appended") => {
                let message = get(get(&event, "message"), "message");
                let meta = get(get(&event, "message"), "meta");
                let when = match s(get(meta, "createdAt")) {
                    Some(t) if !t.is_empty() => json!(t),
                    _ => stamp(get(&event, "time")),
                };
                let texts: Vec<&str> = match get(message, "content") {
                    Value::Array(parts) => parts
                        .iter()
                        .filter(|p| s(get(p, "type")) == Some("text"))
                        .filter_map(|p| s(get(p, "text")))
                        .collect(),
                    Value::String(t) => vec![t.as_str()],
                    _ => Vec::new(),
                };
                match s(get(message, "role")) {
                    Some("user") => {
                        for t in texts {
                            conv::add_turn(&mut turns, "user", t, &when);
                        }
                    }
                    Some("assistant") => {
                        for t in texts {
                            conv::add_turn(&mut turns, "assistant", t, &when);
                        }
                        if let Value::Array(list) = get(message, "toolCalls") {
                            for call in list.iter().filter(|c| c.is_object()) {
                                let name = s(get(call, "name")).unwrap_or("tool");
                                let args = get(call, "arguments");
                                let parsed = s(args)
                                    .and_then(|a| serde_json::from_str::<Value>(a).ok())
                                    .unwrap_or_else(|| args.clone());
                                let fields: Vec<(String, Value)> = match parsed {
                                    Value::Object(o) => {
                                        o.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
                                    }
                                    Value::Null => Vec::new(),
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
                    Some("tool") => {
                        let output = texts.join("\n");
                        conv::add_turn(&mut turns, "assistant", &conv::result_text(&output), &when);
                    }
                    _ => {} // system messages of kimi's own
                }
            }
            _ => {} // kimi's own bookkeeping: turns, usage, file history, the context it rebuilds
        }
    }
    let title = [get(&x.state, "title"), get(&x.state, "lastPrompt")]
        .into_iter()
        .filter_map(s)
        .map(py::collapse)
        .find(|t| !t.is_empty());
    Ok(Conversation {
        agent: Agent::Kimi,
        id: id.to_string(),
        cwd: s(get(&x.state, "cwd"))
            .filter(|c| !c.is_empty())
            .map(String::from),
        model,
        turns,
        calls,
        title,
        started: stamp(get(&x.state, "createdAt")),
        branch: Value::Null,
        reasoning: "Kimi's thinking",
        left_out: "Kimi's thinking, images and subagent conversations",
    })
}

// The index: session_index.jsonl at the home's root, one line per session with its folder.

/// Add a session to a home's index. The file is given a newline first when it lacks one at
/// the end, since kimi has had two entries land on one line, which breaks its resume.
pub fn index_add(home: &str, id: &str, dir: &str, cwd: &str) -> std::io::Result<()> {
    let path = ospath::join(home, INDEX);
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .read(true)
        .open(&path)?;
    let ends_well = std::fs::metadata(&path)
        .map(|m| m.len() == 0)
        .unwrap_or(true)
        || {
            use std::io::{Read, Seek, SeekFrom};
            let mut last = [0u8; 1];
            file.seek(SeekFrom::End(-1))?;
            file.read_exact(&mut last)?;
            last[0] == b'\n'
        };
    let mut line = String::new();
    if !ends_well {
        line.push('\n');
    }
    line += &py::dumps_tight(&json!({
        "sessionId": Agent::Kimi.full_id(id), "sessionDir": dir, "workDir": cwd
    })); // as kimi writes its lines: no spaces
    line.push('\n');
    file.write_all(line.as_bytes())
}

/// Take a session out of a home's index, leaving every other line as it was.
pub fn index_remove(home: &str, id: &str) -> std::io::Result<()> {
    let path = ospath::join(home, INDEX);
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Ok(()); // no index: nothing to take out
    };
    let full = Agent::Kimi.full_id(id);
    let kept: Vec<&str> = text
        .lines()
        .filter(|line| {
            serde_json::from_str::<Value>(line)
                .ok()
                .is_none_or(|e| s(get(&e, "sessionId")) != Some(full.as_str()))
        })
        .collect();
    let mut out = kept.join("\n");
    if !out.is_empty() {
        out.push('\n');
    }
    let tmp = format!("{path}.hatcheck-tmp");
    std::fs::write(&tmp, out)?;
    std::fs::rename(&tmp, &path)
}

/// The lines a home's index has for a session: (its folder, its working directory) each.
fn index_lines(home: &str, id: &str) -> Vec<(String, String)> {
    let full = Agent::Kimi.full_id(id);
    let text = std::fs::read_to_string(ospath::join(home, INDEX)).unwrap_or_default();
    text.lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|e| s(get(e, "sessionId")) == Some(full.as_str()))
        .map(|e| {
            (
                py::str_of(get(&e, "sessionDir")),
                py::str_of(get(&e, "workDir")),
            )
        })
        .collect()
}

/// The folder a home's index lists a session at, when it has one line for it.
pub fn indexed_at(home: &str, id: &str) -> Option<String> {
    match index_lines(home, id).as_slice() {
        [(dir, _)] => Some(dir.clone()),
        _ => None,
    }
}

/// Make a home's index list a session at dir, from cwd: as it is when it has just that line,
/// else with whatever lines it has for the session, naming another folder or the same twice,
/// taken out and the right one added. kimi resumes a session through its line, so a wrong one
/// loses it.
pub fn index_put(home: &str, id: &str, dir: &str, cwd: &str) -> std::io::Result<()> {
    let lines = index_lines(home, id);
    if lines.len() == 1 && lines[0] == (dir.to_string(), cwd.to_string()) {
        return Ok(());
    }
    if !lines.is_empty() {
        index_remove(home, id)?;
    }
    index_add(home, id, dir, cwd)
}

/// state.json for a session folder: as the original's, with the ID and the paths into the
/// home changed to the copy's, and forkedFrom set for a fork.
pub fn restate(state: &Value, old_dir: &str, new_dir: &str, ids: (&str, &str)) -> Value {
    let mut state = state.clone();
    if let Value::Object(o) = &mut state {
        if ids.0 != ids.1 {
            o.insert("id".into(), json!(Agent::Kimi.full_id(ids.1)));
            o.insert("forkedFrom".into(), json!(Agent::Kimi.full_id(ids.0)));
        }
    }
    repoint(&state, old_dir, new_dir)
}

fn repoint(value: &Value, old: &str, new: &str) -> Value {
    match value {
        Value::String(t) => Value::String(t.replace(old, new)),
        Value::Array(items) => Value::Array(items.iter().map(|v| repoint(v, old, new)).collect()),
        Value::Object(o) => Value::Object(
            o.iter()
                .map(|(k, v)| (k.clone(), repoint(v, old, new)))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// Write a converted conversation as a new session in a home, the way kimi's own importer
/// (`kimi migrate`, for kimi-cli sessions) writes one: the ID (bare) and the session folder.
/// The wire holds, per turn, the prompt, the messages in the context kimi rebuilds the model's
/// view from, and the turn's end, and nothing else; kimi binds its model and profile when it
/// resumes. Tool calls are text in the replies, as every conversion carries them. Checked
/// against kimi 2.2.0, which validates each event against a schema when it resumes.
pub fn write_session(
    conv: &Conversation,
    turns: &[Turn],
    home: &str,
    cwd: &str,
) -> std::io::Result<(String, String)> {
    let id = crate::move_cmd::uuid4();
    let dir = session_dir(home, cwd, &id);
    let now = times::now();
    let now_ms = (now * 1000.0).trunc() as i64;
    let at = |when: &Value| -> i64 {
        let t = s(when)
            .and_then(times::parse_time)
            .filter(|t| *t != 0.0)
            .unwrap_or(now);
        (t * 1000.0).trunc() as i64
    };
    let main = "main";
    let mut events: Vec<Value> =
        vec![json!({"type": "metadata", "protocol_version": PROTOCOL, "created_at": now_ms})];
    let mut turn_id = 0i64;
    let mut open = false; // a prompt waiting for its reply
    let mut last_prompt = String::new();
    for (role, texts, when) in turns {
        let text = texts.join("\n\n");
        let t_ms = at(when);
        let content = json!([{"type": "text", "text": text}]);
        if role == "user" {
            last_prompt = text.clone();
            events.push(json!({"type": "turn.prompt", "agentId": main, "input": content, "origin": {"kind": "user"}, "time": t_ms}));
            // Every message in the context carries toolCalls, a prompt's empty: kimi maps
            // over it when it builds the model's view.
            events.push(json!({"type": "context.append_message", "message": {"role": "user", "content": content, "toolCalls": []}}));
            open = true;
        } else {
            if !open {
                continue; // fit() alternates, so a reply always follows a prompt
            }
            events.push(json!({"type": "context.append_message", "message": {"role": "assistant", "content": content, "toolCalls": []}}));
            events.push(json!({"type": "turn.ended", "agentId": main, "turnId": turn_id, "reason": "completed", "time": t_ms}));
            turn_id += 1;
            open = false;
        }
    }
    let title = conv
        .title
        .as_deref()
        .map(py::collapse)
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| {
            let fallback = py::head(py::strip(&last_prompt), 50).trim().to_string();
            if fallback.is_empty() {
                "Imported session".into()
            } else {
                fallback
            }
        });
    // state.json as kimi's importer writes it, with what hatcheck knows in custom.
    let state = json!({
        "id": Agent::Kimi.full_id(&id), "version": 2, "cwd": cwd, "createdAt": now_ms, "updatedAt": now_ms, "archived": false,
        "title": title, "titleKind": "replaceable", "isCustomTitle": false,
        "lastPrompt": py::head(&last_prompt, 200), "lastTurnReason": "completed",
        "agents": {"main": {"homedir": ospath::join_all(&dir, &["agents", "main"]), "type": "main", "parentAgentId": null}},
        "custom": {"imported_with_hatcheck": true, "imported_from": format!("{} session {}", conv.agent.name(), conv.id),
                   "imported_at": crate::move_cmd::iso_ms(now)}
    });
    let wire = wire_path(&dir);
    std::fs::create_dir_all(ospath::dirname(&wire))?;
    let tmp = format!("{wire}.hatcheck-tmp");
    std::fs::write(
        &tmp,
        events
            .iter()
            .map(|e| py::dumps_tight(e) + "\n")
            .collect::<String>(),
    )?;
    std::fs::rename(&tmp, &wire)?;
    std::fs::write(ospath::join(&dir, "state.json"), py::dumps_tight(&state))?;
    index_add(home, &id, &dir, cwd)?;
    Ok((id, dir))
}

// Usage: the endpoint kimi's own /usage asks, with the home's token as it is, and hatcheck's
// copy of the last answer, since kimi saves none. kimi's print mode can't be asked: a slash
// command there goes to the model as a message, and every run leaves a session folder behind
// (probed on 2.2.0).

/// The usage endpoints kimi 2.2.0 asks, one per region (the `region` file in the home, global
/// for kimi.ai), under the API base `KIMI_CODE_BASE_URL` overrides. The only addresses of
/// Kimi's the binary itself calls.
pub const DEFAULT_USAGE_URL: &str = "https://api.kimi.com/coding/v1/usages";
pub const GLOBAL_USAGE_URL: &str = "https://api.kimi.ai/coding/v1/usages";

/// The usage endpoint for a home.
pub fn usage_url(home: &str) -> String {
    if let Some(base) = std::env::var("KIMI_CODE_BASE_URL")
        .ok()
        .filter(|v| !v.trim().is_empty())
    {
        return format!("{}/usages", base.trim().trim_end_matches('/'));
    }
    let region = std::fs::read_to_string(ospath::join(home, "region")).unwrap_or_default();
    if region.trim() == "global" {
        GLOBAL_USAGE_URL.into()
    } else {
        DEFAULT_USAGE_URL.into()
    }
}

/// A home's login, from credentials/kimi-code.json as kimi's own token store writes it: the
/// access token, as it is, and when it expires, in seconds; null when the home isn't signed
/// in. The token is sent only to Kimi's usage endpoint, and never refreshed: a refresh would
/// rotate it under kimi.
pub fn read_login(home: &str) -> Value {
    let path = ospath::join_all(home, &["credentials", "kimi-code.json"]);
    let Some(token) = std::fs::read_to_string(&path)
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .filter(Value::is_object)
    else {
        return Value::Null;
    };
    let access = get(&token, "access_token");
    if !truthy(access) {
        return Value::Null;
    }
    // kimi writes seconds; ms are taken as ms, in case that changes.
    let expires = Num::of(get(&token, "expires_at")).map_or(0.0, |n| {
        if n.f() > 1e11 {
            n.f() / 1000.0
        } else {
            n.f()
        }
    });
    json!({"access_token": access, "expires": expires})
}

/// The windows Kimi's usage answer names, as kimi 2.2.0 reads it (usages.limit_5h, limit_7d,
/// limit_month_total and limit_month_code, each with used_ratio and reset_time): their
/// lengths, and the name of the one limit that's a model group's.
const WINDOWS: [(&str, i64, Option<&str>); 4] = [
    ("limit_5h", 5 * 3600, None),
    ("limit_7d", 7 * 86400, None),
    ("limit_month_total", 30 * 86400, None),
    ("limit_month_code", 30 * 86400, Some("code")),
];

/// Kimi's usage answer as a usage body with limits[] rows like Claude's: the 5-hour, 7-day and
/// monthly limits, each the share used and when it resets. Kimi sends no severity, so a limit
/// turns yellow from 75% used and red from 90%, as Codex's do.
pub fn usage_body(data: &Value) -> Value {
    let usages = get(data, "usages");
    let mut rows = Vec::new();
    for (key, seconds, name) in WINDOWS {
        let entry = get(usages, key);
        if !entry.is_object() {
            continue;
        }
        let ratio = match get(entry, "used_ratio") {
            v if v.is_number() => Num::of(v).map(Num::f),
            Value::String(t) => t.trim().parse::<f64>().ok(),
            _ => None,
        };
        let Some(ratio) = ratio.filter(|r| r.is_finite()) else {
            continue;
        };
        let percent = py::round((ratio * 100.0).clamp(0.0, 100.0)) as i64;
        let reset = s(get(entry, "reset_time"))
            .and_then(times::parse_time)
            .map_or(Value::Null, |t| json!(t));
        rows.push(crate::usage::codex_row(
            &json!(seconds),
            &json!(percent),
            &reset,
            name,
            false,
        ));
    }
    json!({"limits": rows})
}

/// The plan, as the usage names it: Kimi Code, the subscription kimi's own /usage reports on.
pub fn plan_text() -> String {
    "Kimi Code".into()
}

/// The folder Kimi's readings are kept in, a file per home.
fn snapshots_dir() -> String {
    ospath::join(&facts::cache_dir(), "kimi-usage")
}

/// A home's reading's file, named by a hash of the home's path: homes read at once, as usage
/// reads them, each write their own, so none's reading is lost to another's.
fn snapshot_file(dir: &str, home: &str) -> String {
    let hash: String = Sha256::digest(home.as_bytes())[..8]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    ospath::join(dir, &format!("{hash}.json"))
}

/// Keep the last reading, with when it was taken, so a later run without one can show it with
/// its age, as the other agents' own snapshots are shown.
pub fn save_snapshot(home: &str, body: &Value, now: f64) {
    save_snapshot_in(&snapshots_dir(), home, body, now);
}

fn save_snapshot_in(dir: &str, home: &str, body: &Value, now: f64) {
    let path = snapshot_file(dir, home);
    let _ = std::fs::create_dir_all(dir);
    let tmp = format!("{path}.{}.tmp", std::process::id());
    let entry = json!({"home": home, "saved_at": now, "usage": body});
    if std::fs::write(&tmp, py::dumps(&entry))
        .and_then(|()| std::fs::rename(&tmp, &path))
        .is_err()
    {
        let _ = std::fs::remove_file(&tmp);
    }
}

/// (usage body, when it was read) of the last reading kept for a home: in its own file, or in
/// last-usage.json, the one file every home shared in 0.5.0.
pub fn read_snapshot(home: &str) -> Option<(Value, f64)> {
    read_snapshot_in(&snapshots_dir(), home)
}

fn read_snapshot_in(dir: &str, home: &str) -> Option<(Value, f64)> {
    let read = |path: &str| -> Option<Value> {
        serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
    };
    let entry = match read(&snapshot_file(dir, home)).filter(|e| s(get(e, "home")) == Some(home)) {
        Some(own) => own,
        None => get(&read(&ospath::join(dir, "last-usage.json"))?, home).clone(),
    };
    let saved = Num::of(get(&entry, "saved_at"))?.f();
    let body = get(&entry, "usage");
    truthy(body).then(|| (body.clone(), saved))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readings_of_homes_kept_at_once_all_stay() {
        let dir =
            std::env::temp_dir().join(format!("hatcheck-kimi-readings-{}", std::process::id()));
        let dir = dir.to_string_lossy().into_owned();
        let homes: Vec<String> = (0..8).map(|n| format!("/h/.kimi-code-{n}")).collect();
        std::thread::scope(|scope| {
            for (n, home) in homes.iter().enumerate() {
                let dir = &dir;
                scope.spawn(move || {
                    for round in 1..=20 {
                        let body = json!({"limits": [{"home": n, "round": round}]});
                        save_snapshot_in(dir, home, &body, f64::from(round));
                    }
                });
            }
        });
        for (n, home) in homes.iter().enumerate() {
            let (body, saved) = read_snapshot_in(&dir, home).expect("every home's reading is kept");
            assert_eq!(
                (body["limits"][0]["home"].as_u64(), saved),
                (Some(n as u64), 20.0)
            );
        }
        // A reading 0.5.0 kept, in the file every home shared then, is still read.
        let old = "/h/.kimi-code-old";
        let shared = json!({old: {"saved_at": 5.0, "usage": {"limits": [{"home": "old"}]}}});
        std::fs::write(ospath::join(&dir, "last-usage.json"), shared.to_string()).unwrap();
        assert_eq!(read_snapshot_in(&dir, old).map(|r| r.1), Some(5.0));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn usage_windows_become_rows() {
        let later = times::isoformat_utc(1_800_000_000.0).unwrap();
        let data = json!({"usages": {
            "limit_5h": {"used_ratio": 0.2, "reset_time": later},
            "limit_7d": {"used_ratio": "0.915", "reset_time": later},
            "limit_month_total": {"used_ratio": 0},
            "limit_month_code": {"used_ratio": 1.0, "reset_time": later},
            "limit_other": {"used_ratio": 0.5}},
            "boosterWallet": null});
        let body = usage_body(&data);
        let rows = body["limits"].as_array().unwrap();
        let shorts: Vec<&str> = rows.iter().map(|r| r["short"].as_str().unwrap()).collect();
        assert_eq!(shorts, ["5h", "week", "month", "code month"]);
        let used: Vec<i64> = rows
            .iter()
            .map(|r| r["percent"].as_i64().unwrap())
            .collect();
        assert_eq!(used, [20, 92, 0, 100]);
        assert_eq!(rows[0]["kind"], "session");
        assert_eq!(rows[1]["severity"], "critical");
        assert_eq!(rows[0]["resets_at"], later);
        assert_eq!(rows[2]["resets_at"], Value::Null);
        assert_eq!(rows[3]["kind"], "monthly_scoped");
        assert!(usage_body(&json!({}))["limits"]
            .as_array()
            .unwrap()
            .is_empty());
    }

    #[test]
    fn logins_expire_in_seconds_or_ms() {
        let dir = std::env::temp_dir().join(format!("hatcheck-kimi-login-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("credentials")).unwrap();
        let home = dir.to_string_lossy().into_owned();
        assert_eq!(read_login(&home), Value::Null);
        let file = dir.join("credentials").join("kimi-code.json");
        std::fs::write(&file, r#"{"access_token": "t", "expires_at": 1800000000}"#).unwrap();
        assert_eq!(read_login(&home)["expires"], 1800000000.0);
        std::fs::write(
            &file,
            r#"{"access_token": "t", "expires_at": 1800000000000}"#,
        )
        .unwrap();
        assert_eq!(read_login(&home)["expires"], 1800000000.0);
        std::fs::write(&file, r#"{"refresh_token": "r"}"#).unwrap();
        assert_eq!(read_login(&home), Value::Null);
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(usage_url(&home), DEFAULT_USAGE_URL);
    }

    #[test]
    fn working_directories_are_keyed_as_kimi_keys_them() {
        // Checked against kimi 2.2.0: /Volumes/ep4t/repos/murky-swamp -> wd_murky-swamp_eccd5d64c70f.
        assert_eq!(
            wd_key("/Volumes/ep4t/repos/murky-swamp"),
            "wd_murky-swamp_eccd5d64c70f"
        );
        assert_eq!(
            wd_key("/Users/yi/repos/murky-swamp"),
            "wd_murky-swamp_896d0e03446c"
        );
        assert!(wd_key("/x/my code").starts_with("wd_my-code_"));
    }

    #[test]
    fn session_folders_name_the_session() {
        let id = "0752ad88-cc62-4e00-9912-48e4146b7628";
        assert_eq!(
            dir_id(&format!(
                "/h/.kimi-code/sessions/wd_api_0123456789ab/session_{id}"
            )),
            Some(id)
        );
        assert_eq!(
            dir_id(&format!("/h/sessions/wd_api_0123456789ab/session_{id}/")),
            Some(id)
        );
        assert_eq!(dir_id("/h/sessions/wd_api_0123456789ab/state.json"), None);
        assert_eq!(dir_id("/h/sessions/wd_api_0123456789ab/session_x"), None);
    }

    #[test]
    fn the_state_of_a_copy_points_into_the_copy() {
        let state = json!({"id": "session_a", "cwd": "/w", "agents": {"main": {"homedir": "/h/.kimi-code/sessions/wd_w_0/session_a/agents/main", "type": "main"}}});
        let moved = restate(
            &state,
            "/h/.kimi-code/sessions/wd_w_0/session_a",
            "/h/.kimi-code-work/sessions/wd_w_0/session_a",
            ("a", "a"),
        );
        assert_eq!(
            moved["agents"]["main"]["homedir"],
            "/h/.kimi-code-work/sessions/wd_w_0/session_a/agents/main"
        );
        assert_eq!(moved["id"], "session_a");
        assert!(moved.get("forkedFrom").is_none());
        let forked = restate(
            &state,
            "/h/.kimi-code/sessions/wd_w_0/session_a",
            "/h/.kimi-code/sessions/wd_w_0/session_b",
            ("a", "b"),
        );
        assert_eq!(forked["id"], "session_b");
        assert_eq!(forked["forkedFrom"], "session_a");
        assert!(forked["agents"]["main"]["homedir"]
            .as_str()
            .unwrap()
            .contains("session_b"));
    }
}
