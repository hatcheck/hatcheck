//! DeepSeek Harness (dsh) as hatcheck reads it. DSH keeps sessions under ~/.dsh, which
//! DSH_HOME moves whole. Its own profiles are plugin stacks, not account homes; hatcheck's
//! named homes are ~/.dsh-NAME. A session owns sessions/<project>/<id>/session.v4.jsonl[.zstd],
//! with earlier immutable generations beside it and session.lock held by its writer on POSIX.
//! Hatcheck reads the current v4 generation and never writes DSH's store. docs/agents.md's
//! `DeepSeek Harness` section says what is read, checked against dsh 0.2.1-rc.2.

use crate::agent::Agent;
use crate::conv::{self, Conversation};
use crate::facts::Summary;
use crate::ospath::{self, Part};
use crate::paths;
use crate::py::{self, get, s, Num};
use crate::times;
use serde_json::{json, Value};
use std::io::Read;

const SESSIONS: &str = "sessions";
const FORMAT: u64 = 4;

/// ~/.dsh.
pub fn default_home() -> String {
    ospath::join(&paths::home(), Agent::Dsh.base())
}

/// Whether a folder is a DSH home: named like one, or with DSH's project/session folders.
pub fn is_home(path: &str) -> bool {
    let base = ospath::basename(path.trim_end_matches(ospath::is_sep));
    base.starts_with(Agent::Dsh.base())
        || !ospath::glob(
            path,
            &[
                Part::Name(SESSIONS),
                Part::Wild("", ""),
                Part::Wild("session-", ""),
            ],
        )
        .is_empty()
}

/// Every DSH home with sessions: ~/.dsh and ~/.dsh-* (and $DSH_HOME), deduplicated.
pub fn find_homes() -> Vec<String> {
    let h = paths::home();
    let mut candidates = ospath::glob(&h, &[Part::Name(Agent::Dsh.base())]);
    candidates.extend(ospath::glob(
        &h,
        &[Part::Wild(&format!("{}-", Agent::Dsh.base()), "")],
    ));
    if let Some(dir) = std::env::var("DSH_HOME")
        .ok()
        .filter(|v| !v.trim().is_empty())
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

/// The ID hatcheck shows for a session directory. DSH's usual session-<uuid> loses its prefix.
pub fn dir_id(path: &str) -> Option<String> {
    let name = path
        .trim_end_matches(ospath::is_sep)
        .rsplit(['/', '\\'])
        .next()?;
    if name.is_empty() || name == "." || name == ".." || name.contains('~') {
        return None;
    }
    let id = Agent::Dsh.bare_id(name);
    (!id.is_empty()).then(|| id.to_string())
}

/// The DSH home three levels above sessions/<project>/<id>.
pub fn home_of(dir: &str) -> String {
    let trimmed = dir.trim_end_matches(ospath::is_sep);
    ospath::dirname(&ospath::dirname(&ospath::dirname(trimmed)))
}

/// One materialized session, with its selected log already read.
#[derive(Debug)]
pub struct Session {
    pub id: String,
    pub full_id: String,
    pub dir: String,
    pub log: String,
    pub header: Value,
    pub events: Vec<Value>,
    pub size: u64,
    pub mtime: f64,
}

/// Parse session.jsonl[.zstd] or session.vN.jsonl[.zstd], rejecting noncanonical names.
fn generation(name: &str) -> Option<(u64, bool)> {
    let (name, compressed) = match name.strip_suffix(".zstd") {
        Some(stem) => (stem, true),
        None => (name, false),
    };
    let stem = name.strip_suffix(".jsonl")?;
    if stem == "session" {
        return Some((0, compressed));
    }
    let version = stem.strip_prefix("session.v")?;
    if version.is_empty()
        || !version.bytes().all(|b| b.is_ascii_digit())
        || version.starts_with('0')
    {
        return None;
    }
    Some((version.parse().ok()?, compressed))
}

/// The highest canonical generation, or none when the directory has no unambiguous choice.
fn selected_log(dir: &str) -> Option<(String, u64, bool)> {
    let mut found: Vec<(u64, String, bool)> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let (version, compressed) = generation(&name)?;
            let path = ospath::join(dir, &name);
            ospath::isfile(&path).then_some((version, path, compressed))
        })
        .collect();
    found.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    let version = found.first()?.0;
    if found.iter().filter(|x| x.0 == version).count() != 1 {
        return None;
    }
    found
        .into_iter()
        .next()
        .map(|(version, path, compressed)| (path, version, compressed))
}

fn invalid(message: impl Into<String>) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, message.into())
}

/// Decode raw JSONL or the concatenated independent Zstandard frames DSH writes.
fn decode(data: &[u8], compressed: bool) -> std::io::Result<Vec<u8>> {
    if !compressed {
        return Ok(data.to_vec());
    }
    let mut decoder = zstd::stream::read::Decoder::new(data)?;
    let mut decoded = Vec::new();
    decoder.read_to_end(&mut decoded)?;
    Ok(decoded)
}

/// Read a header and complete event rows. An unterminated final row is a torn tail, not an event.
fn parse(data: &[u8], version: u64) -> std::io::Result<(Value, Vec<Value>)> {
    if version != FORMAT {
        return Err(invalid(format!(
            "DSH session format {version} isn't supported; hatcheck reads v{FORMAT}"
        )));
    }
    let mut lines = data.split_inclusive(|b| *b == b'\n');
    let first = lines
        .next()
        .ok_or_else(|| invalid("empty DSH session log"))?;
    if !first.ends_with(b"\n") {
        return Err(invalid("DSH session header isn't complete"));
    }
    let header: Value = serde_json::from_slice(first)?;
    if s(get(&header, "type")) != Some("session")
        || get(&header, "version").as_u64() != Some(version)
    {
        return Err(invalid("DSH session header doesn't match its generation"));
    }
    let mut events = Vec::new();
    for line in lines {
        if !line.ends_with(b"\n") {
            break;
        }
        if line.iter().all(|b| b.is_ascii_whitespace()) {
            continue;
        }
        let event: Value = serde_json::from_slice(line)?;
        if !event.is_object() || s(get(&event, "type")).is_none() {
            return Err(invalid("invalid DSH session event"));
        }
        events.push(event);
    }
    Ok((header, events))
}

/// Read one session directory, refusing an unreadable newest generation rather than using an older one.
fn read_session(dir: &str) -> std::io::Result<Session> {
    let id = dir_id(dir).ok_or_else(|| invalid("not a DSH session directory"))?;
    let full_id = ospath::basename(dir.trim_end_matches(ospath::is_sep));
    let (log, version, compressed) =
        selected_log(dir).ok_or_else(|| invalid("no unambiguous DSH session log"))?;
    let raw = std::fs::read(&log)?;
    let decoded = decode(&raw, compressed)?;
    let (header, events) = parse(&decoded, version)?;
    if s(get(&header, "id")) != Some(full_id.as_str()) {
        return Err(invalid("DSH session header ID doesn't match its directory"));
    }
    let mtime = ospath::getmtime(&log).unwrap_or(0.0);
    Ok(Session {
        id,
        full_id,
        dir: dir.trim_end_matches(ospath::is_sep).to_string(),
        log,
        header,
        events,
        size: raw.len() as u64,
        mtime,
    })
}

/// Every readable current session in a home, newest activity first.
pub fn sessions(home: &str) -> Vec<Session> {
    let mut found: Vec<Session> = ospath::glob(
        home,
        &[Part::Name(SESSIONS), Part::Wild("", ""), Part::Wild("", "")],
    )
    .into_iter()
    .filter(|dir| ospath::isdir(dir))
    .filter_map(|dir| read_session(&dir).ok())
    .collect();
    found.sort_by(|a, b| {
        updated(b)
            .partial_cmp(&updated(a))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.id.cmp(&b.id))
    });
    found
}

/// The sessions whose hatcheck ID starts with prefix; a pasted session- prefix is understood.
pub fn sessions_starting_with(home: &str, prefix: &str) -> Vec<Session> {
    let lower = Agent::Dsh.bare_id(prefix).to_lowercase();
    sessions(home)
        .into_iter()
        .filter(|x| x.id.to_lowercase().starts_with(&lower))
        .collect()
}

pub fn session(home: &str, id: &str) -> Option<Session> {
    let id = Agent::Dsh.bare_id(id);
    sessions(home).into_iter().find(|x| x.id == id)
}

/// A session from its directory, for facts::summarize.
pub fn summarize(home: &str, dir: &str) -> Option<Summary> {
    read_session(dir).ok().map(|x| summary(home, &x))
}

fn ms(value: &Value) -> f64 {
    Num::of(value).map_or(0.0, |n| n.f() / 1000.0)
}

fn updated(x: &Session) -> f64 {
    x.events
        .iter()
        .map(|event| ms(get(event, "time")))
        .fold(ms(get(&x.header, "createdAt")), f64::max)
        .max(x.mtime)
}

fn stamp_ms(value: &Value) -> Value {
    match Num::of(value) {
        Some(n) if n.f() != 0.0 => {
            times::iso_utc(n.f() / 1000.0).map_or(Value::Null, Value::String)
        }
        _ => Value::Null,
    }
}

fn direct_prompt(data: &Value) -> bool {
    s(get(get(data, "source"), "kind")) == Some("user")
}

fn text_content(message: &Value) -> String {
    get(message, "content")
        .as_array()
        .map(|parts| {
            parts
                .iter()
                .filter(|part| s(get(part, "type")) == Some("text"))
                .filter_map(|part| s(get(part, "text")))
                .collect::<Vec<_>>()
                .join("\n\n")
        })
        .unwrap_or_default()
}

fn title(x: &Session) -> Option<String> {
    let named = x
        .events
        .iter()
        .rev()
        .filter(|event| s(get(event, "type")) == Some("session/title"))
        .filter_map(|event| s(get(get(event, "data"), "title")))
        .map(py::collapse)
        .find(|title| !title.is_empty());
    named.or_else(|| {
        x.events
            .iter()
            .filter(|event| {
                s(get(event, "type")) == Some("user/message") && direct_prompt(get(event, "data"))
            })
            .map(|event| py::collapse(&text_content(get(event, "data"))))
            .find(|title| !title.is_empty())
    })
}

fn model(x: &Session) -> Value {
    for event in x.events.iter().rev() {
        match s(get(event, "type")) {
            Some("request/context") | Some("model/selection") => {
                let value = get(get(event, "data"), "model");
                if s(value).is_some_and(|m| !m.is_empty()) {
                    return value.clone();
                }
            }
            Some("assistant/message") => {
                let value = get(get(get(event, "data"), "message"), "source");
                if s(get(value, "model")).is_some() {
                    return get(value, "model").clone();
                }
            }
            _ => {}
        }
    }
    Value::Null
}

/// The current ordered surface, with compaction/replacement operations applied by event sequence.
fn surface(x: &Session) -> std::io::Result<Vec<Value>> {
    let mut surface: Vec<Value> = Vec::new();
    for event in &x.events {
        match s(get(event, "type")) {
            Some(
                "system/message" | "developer/message" | "user/message" | "assistant/message"
                | "tool/result",
            ) => {}
            _ => continue,
        }
        let operation = get(event, "surfaceOp");
        if s(operation) == Some("append") {
            surface.push(event.clone());
            continue;
        }
        if s(get(operation, "op")) != Some("replace") {
            return Err(invalid("DSH surface event has no valid operation"));
        }
        let start_seq = get(operation, "startSeq").as_u64();
        let end_seq = get(operation, "endSeq").as_u64();
        let (Some(start_seq), Some(end_seq)) = (start_seq, end_seq) else {
            return Err(invalid("DSH replacement has no sequence range"));
        };
        let start = surface
            .iter()
            .position(|item| get(item, "seq").as_u64() == Some(start_seq));
        let end = surface
            .iter()
            .position(|item| get(item, "seq").as_u64() == Some(end_seq));
        let (Some(start), Some(end)) = (start, end) else {
            return Err(invalid("DSH replacement names a missing surface node"));
        };
        if end < start {
            return Err(invalid("DSH replacement range is backwards"));
        }
        surface.splice(start..=end, [event.clone()]);
    }
    Ok(surface)
}

/// A session as the session list shows it.
pub fn summary(home: &str, x: &Session) -> Summary {
    let prompts = x
        .events
        .iter()
        .filter(|event| {
            s(get(event, "type")) == Some("user/message") && direct_prompt(get(event, "data"))
        })
        .count();
    Summary {
        agent: Agent::Dsh,
        id: x.id.clone(),
        when: updated(x),
        entrypoint: json!(if s(get(&x.header, "origin")) == Some("subagent") {
            "subagent"
        } else {
            "cli"
        }),
        branch: Value::Null,
        cwd: s(get(&x.header, "cwd"))
            .filter(|cwd| !cwd.is_empty())
            .map(String::from),
        canonical: true,
        size: x.size,
        has_messages: prompts > 0,
        title: title(x).unwrap_or_else(|| "(untitled)".into()),
        continued_in: Value::Null,
        model: model(x),
        profile: home.to_string(),
        path: x.dir.clone(),
        open: None,
        bound_to: None,
    }
}

/// Whether another process holds this session's POSIX writer lease. Windows uses a named semaphore.
pub fn session_open(dir: &str) -> bool {
    crate::find::lock_held(&ospath::join(dir, "session.lock"))
}

/// A conversation as hatcheck shows and converts it, from DSH's current surface.
pub fn read_conversation(home: &str, id: &str) -> std::io::Result<Conversation> {
    let x = session(home, id).ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("no session {id} in {home}"),
        )
    })?;
    conversation(&x)
}

fn conversation(x: &Session) -> std::io::Result<Conversation> {
    let (mut turns, mut calls) = (Vec::new(), 0);
    for event in surface(x)? {
        let data = get(&event, "data");
        let when = stamp_ms(get(&event, "time"));
        match s(get(&event, "type")) {
            Some("user/message") => {
                let kind = s(get(get(data, "source"), "kind"));
                if kind == Some("user") || kind == Some("compact-checkpoint") {
                    conv::add_turn(&mut turns, "user", &text_content(data), &when);
                }
            }
            Some("assistant/message") => {
                let message = get(data, "message");
                conv::add_turn(&mut turns, "assistant", &text_content(message), &when);
                if let Some(parts) = get(message, "content").as_array() {
                    for part in parts
                        .iter()
                        .filter(|part| s(get(part, "type")) == Some("tool-call"))
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
            Some("tool/result") => {
                let output = text_content(get(data, "message"));
                if !output.is_empty() {
                    conv::add_turn(&mut turns, "assistant", &conv::result_text(&output), &when);
                }
            }
            _ => {}
        }
    }
    Ok(Conversation {
        agent: Agent::Dsh,
        id: x.id.clone(),
        cwd: s(get(&x.header, "cwd"))
            .filter(|cwd| !cwd.is_empty())
            .map(String::from),
        model: model(x),
        turns,
        calls,
        title: title(x),
        started: stamp_ms(get(&x.header, "createdAt")),
        branch: Value::Null,
        reasoning: "DeepSeek Harness's reasoning",
        left_out: "DeepSeek Harness's reasoning, images, files, injected context, replaced history and subagent sessions",
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn fixture() -> Vec<u8> {
        let rows = [
            json!({"type":"session","version":4,"id":"session-abc","createdAt":1792871597050i64,"cwd":"/work","isSeeded":false}),
            json!({"type":"user/message","seq":0,"time":1792871598000i64,"surfaceOp":"append","data":{"role":"user","id":"u1","source":{"kind":"user"},"content":[{"type":"text","text":"Fix the parser"}]}}),
            json!({"type":"user/message","seq":1,"time":1792871598000i64,"surfaceOp":"append","data":{"role":"user","id":"u2","source":{"kind":"runtime-context"},"content":[{"type":"text","text":"injected"}]}}),
            json!({"type":"session/title","seq":2,"time":1792871599000i64,"data":{"title":"Parser fix","messageSeqs":[0],"source":{"kind":"fallback"}}}),
            json!({"type":"request/context","seq":3,"time":1792871599000i64,"data":{"provider":"deepseek","model":"deepseek-flash"}}),
            json!({"type":"assistant/message","seq":4,"time":1792871600000i64,"surfaceOp":"append","data":{"turn":1,"step":1,"message":{"role":"assistant","id":"a1","source":{"kind":"model","provider":"deepseek","model":"deepseek-flash"},"content":[{"type":"reasoning","text":"secret"},{"type":"text","text":"I'll look."},{"type":"tool-call","id":"c1","name":"read_file","arguments":"{\"path\":\"src/main.rs\"}"}]},"stream":[]}}),
            json!({"type":"tool/result","seq":5,"time":1792871601000i64,"surfaceOp":"append","data":{"turn":1,"step":1,"message":{"role":"tool","id":"t1","toolCallId":"c1","source":{"kind":"tool","callId":"c1"},"content":[{"type":"text","text":"fn main() {}"}]}}}),
        ];
        rows.iter()
            .map(|row| row.to_string() + "\n")
            .collect::<String>()
            .into_bytes()
    }

    fn parsed(data: &[u8], compressed: bool) -> Session {
        let decoded = decode(data, compressed).unwrap();
        let (header, events) = parse(&decoded, 4).unwrap();
        Session {
            id: "abc".into(),
            full_id: "session-abc".into(),
            dir: "/h/.dsh/sessions/--work--/session-abc".into(),
            log: "/h/.dsh/sessions/--work--/session-abc/session.v4.jsonl".into(),
            header,
            events,
            size: data.len() as u64,
            mtime: 0.0,
        }
    }

    #[test]
    fn reads_direct_messages_tools_and_metadata() {
        let x = parsed(&fixture(), false);
        assert_eq!(x.id, "abc");
        assert_eq!(title(&x).as_deref(), Some("Parser fix"));
        assert_eq!(model(&x), json!("deepseek-flash"));
        assert!(summary("/h/.dsh", &x).has_messages);
        let conv = conversation(&x).unwrap();
        assert_eq!(conv.turns.len(), 2);
        assert_eq!(conv.turns[0].1, ["Fix the parser"]);
        assert!(!conv
            .turns
            .iter()
            .any(|turn| turn.1.join("\n").contains("injected")));
        assert!(!conv
            .turns
            .iter()
            .any(|turn| turn.1.join("\n").contains("secret")));
        assert!(conv.turns[1].1[0].contains("I'll look."));
        assert!(conv.turns[1].1[1].contains("read_file"));
        assert!(conv.turns[1].1[2].contains("fn main() {}"));
        assert_eq!(conv.calls, 1);
    }

    #[test]
    fn reads_concatenated_zstd_frames() {
        let data = fixture();
        let split = data.iter().position(|b| *b == b'\n').unwrap() + 1;
        let mut compressed = zstd::stream::encode_all(&data[..split], 3).unwrap();
        compressed.extend(zstd::stream::encode_all(&data[split..], 3).unwrap());
        let x = parsed(&compressed, true);
        assert_eq!(title(&x).as_deref(), Some("Parser fix"));
        assert_eq!(conversation(&x).unwrap().calls, 1);
    }

    #[test]
    fn applies_surface_replacements() {
        let mut data = fixture();
        let replacement = json!({"type":"user/message","seq":6,"time":1792871602000i64,"surfaceOp":{"op":"replace","startSeq":0,"endSeq":1},"sourceEventSeqs":[0,1],"data":{"role":"user","id":"u3","source":{"kind":"compact-checkpoint"},"content":[{"type":"text","text":"checkpoint"}]}});
        data.write_all((replacement.to_string() + "\n").as_bytes())
            .unwrap();
        let conv = conversation(&parsed(&data, false)).unwrap();
        assert_eq!(conv.turns[0].1, ["checkpoint"]);
        assert!(!conv
            .turns
            .iter()
            .any(|turn| turn.1.join("\n").contains("Fix the parser")));
    }

    #[test]
    fn canonical_generations_and_ids() {
        assert_eq!(generation("session.jsonl"), Some((0, false)));
        assert_eq!(generation("session.v4.jsonl.zstd"), Some((4, true)));
        assert_eq!(generation("session.v04.jsonl"), None);
        assert_eq!(generation("session.v0.jsonl"), None);
        assert_eq!(
            dir_id("/h/.dsh/sessions/p/session-abc").as_deref(),
            Some("abc")
        );
        assert_eq!(home_of("/h/.dsh/sessions/p/session-abc"), "/h/.dsh");
    }
}
