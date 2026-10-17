//! What hatcheck reads from session files, and the cache that spares reading them again.
//!
//! Claude Code's transcripts (projects/<folder>/<id>.jsonl) and Codex's session files
//! (sessions/YYYY/MM/DD/rollout-*.jsonl) are only ever appended to. The facts the session
//! list needs from each, its newest activity, titles, model and first prompt, are cached by
//! file in session-facts.json in hatcheck's cache folder, with the file's identity, size and
//! times; a file that only grew since is read from the last whole line the previous read
//! reached, and anything else is read again from the start. Python's hatcheck keeps its own
//! cache (sessions.json) beside this one.

use crate::ospath;
use crate::paths;
use crate::py::{self, get, s, truthy};
use crate::times;
use memchr::memmem;
use serde_json::{json, Map, Value};
use std::fs::File;
use std::sync::Mutex;

const CACHE_VERSION: u64 = 1;
const CACHE_FILE: &str = "session-facts.json";

/// Where hatcheck keeps what it can work out again: $HATCHECK_CACHE_DIR, or
/// ~/Library/Caches/hatcheck on macOS, %LOCALAPPDATA%\hatcheck\Cache on Windows and
/// $XDG_CACHE_HOME/hatcheck (~/.cache/hatcheck) elsewhere.
pub fn cache_dir() -> String {
    let var = |name: &str| std::env::var(name).ok().filter(|v| !v.is_empty());
    if let Some(dir) = var("HATCHECK_CACHE_DIR") {
        return dir;
    }
    let home = paths::home();
    if cfg!(target_os = "macos") {
        return ospath::join_all(&home, &["Library", "Caches", "hatcheck"]);
    }
    if ospath::WINDOWS {
        let base =
            var("LOCALAPPDATA").unwrap_or_else(|| ospath::join_all(&home, &["AppData", "Local"]));
        return ospath::join_all(&base, &["hatcheck", "Cache"]);
    }
    ospath::join(
        &var("XDG_CACHE_HOME").unwrap_or_else(|| ospath::join(&home, ".cache")),
        "hatcheck",
    )
}

struct Cache {
    files: Map<String, Value>,
    changed: bool,
}

static CACHE: Mutex<Option<Cache>> = Mutex::new(None);

fn with_cache<T>(f: impl FnOnce(&mut Cache) -> T) -> T {
    let mut guard = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    let cache = guard.get_or_insert_with(|| {
        let path = ospath::join(&cache_dir(), CACHE_FILE);
        let files = std::fs::read_to_string(path)
            .ok()
            .and_then(|t| serde_json::from_str::<Value>(&t).ok())
            .filter(|d| get(d, "version").as_u64() == Some(CACHE_VERSION))
            .and_then(|d| d.get("files").and_then(Value::as_object).cloned())
            .unwrap_or_default();
        Cache {
            files,
            changed: false,
        }
    });
    f(cache)
}

/// Write the cache back if it changed, without files that are gone or unused for 30 days.
pub fn save_cache() {
    let mut guard = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    let Some(cache) = guard.as_mut() else { return };
    if !cache.changed {
        return;
    }
    cache.changed = false;
    let now = times::now();
    let files: Map<String, Value> = cache
        .files
        .iter()
        .filter(|(path, e)| {
            now - get(e, "seen").as_f64().unwrap_or(0.0) < 30.0 * 86400.0 && ospath::exists(path)
        })
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    let dir = cache_dir();
    let path = ospath::join(&dir, CACHE_FILE);
    let tmp = format!("{path}.{}.tmp", std::process::id());
    let text = serde_json::to_string(&json!({"version": CACHE_VERSION, "files": files}))
        .unwrap_or_default();
    let written = std::fs::create_dir_all(&dir)
        .and_then(|()| std::fs::write(&tmp, text))
        .and_then(|()| std::fs::rename(&tmp, &path));
    if written.is_err() {
        let _ = std::fs::remove_file(&tmp); // only a cache: the next run reads the files again
    }
}

/// A file's identity, size and modification and change times: what says it hasn't changed.
/// The change time moves with every write, even when the size and the modification time are
/// put back afterwards. On Windows the creation time stands in for the identity.
fn stamp(m: &std::fs::Metadata) -> Value {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        json!([
            m.ino(),
            m.size(),
            m.mtime() as i128 * 1_000_000_000 + m.mtime_nsec() as i128,
            m.ctime() as i128 * 1_000_000_000 + m.ctime_nsec() as i128
        ])
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        json!([
            m.creation_time(),
            m.file_size(),
            m.last_write_time(),
            m.creation_time()
        ])
    }
}

/// A hash of a file's first and last 4 KB before end (FNV-1a), to tell that the file up to
/// end is still what was read.
fn fingerprint(data: &[u8], end: usize) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for part in [&data[..end.min(4096)], &data[end.saturating_sub(4096)..end]] {
        for b in part {
            h ^= u64::from(*b);
            h = h.wrapping_mul(0x100000001b3);
        }
    }
    format!("{h:016x}{end:x}")
}

type Reader = fn(&[u8], Option<&Value>, usize) -> Value;

/// reader's facts about a session file, and its size: as last read when the file hasn't
/// changed, read on from where that read stopped when it has only grown, or read whole.
/// An empty file gives (None, 0).
pub fn file_facts(path: &str, kind: &str, reader: Reader) -> std::io::Result<(Option<Value>, u64)> {
    let key = ospath::abspath(path);
    let file = File::open(path)?;
    let meta = file.metadata()?; // before reading, so a write after this shows next time
    if meta.len() == 0 {
        return Ok((None, 0));
    }
    let stamp = stamp(&meta);
    let entry = with_cache(|c| {
        c.files
            .get(&key)
            .filter(|e| s(get(e, "kind")) == Some(kind))
            .cloned()
    });
    if let Some(e) = &entry {
        if get(e, "stamp") == &stamp {
            let now = times::now();
            if now - get(e, "seen").as_f64().unwrap_or(0.0) > 86400.0 {
                with_cache(|c| {
                    if let Some(Value::Object(e)) = c.files.get_mut(&key) {
                        e.insert("seen".into(), json!(now as i64));
                        c.changed = true;
                    }
                });
            }
            return Ok((Some(get(e, "facts").clone()), meta.len()));
        }
    }
    // SAFETY: session files are only appended to; the map covers the file as it is now.
    let map = unsafe { memmap2::Mmap::map(&file)? };
    let data: &[u8] = &map;
    let mut old: Option<&Value> = None;
    let mut start = 0;
    if let Some(e) = &entry {
        let scanned = get(e, "scanned").as_u64().map(|n| n as usize);
        if let Some(scanned) = scanned {
            if get(e, "stamp").get(0) == stamp.get(0)
                && 0 < scanned
                && scanned <= data.len()
                && s(get(e, "check")) == Some(fingerprint(data, scanned).as_str())
            {
                old = Some(get(e, "facts"));
                start = scanned;
            }
        }
    }
    let facts = reader(data, old, start);
    let scanned = memchr::memrchr(b'\n', data).map_or(0, |i| i + 1); // a line still being written is read again
    let entry = json!({"kind": kind, "stamp": stamp, "scanned": scanned, "check": fingerprint(data, scanned),
                       "facts": facts, "seen": times::now() as i64});
    with_cache(|c| {
        c.files.insert(key, entry);
        c.changed = true;
    });
    Ok((Some(facts), meta.len()))
}

/// The lines of data, the last first, down to the one that starts at stop.
pub fn lines_backward(data: &[u8], stop: usize) -> impl Iterator<Item = &[u8]> {
    let mut end = data.len();
    if end > stop && data[end - 1] == b'\n' {
        end -= 1;
    }
    let mut done = end <= stop;
    std::iter::from_fn(move || {
        if done {
            return None;
        }
        let start = memchr::memrchr(b'\n', &data[stop..end]).map_or(stop, |i| stop + i + 1);
        let line = &data[start..end];
        if start <= stop {
            done = true;
        } else {
            end = start - 1;
            done = end <= stop;
        }
        Some(line)
    })
}

/// The line holding byte i, parsed (None when it isn't JSON), and where it starts.
fn line_at(data: &[u8], i: usize) -> (Option<Value>, usize) {
    let start = memchr::memrchr(b'\n', &data[..i]).map_or(0, |j| j + 1);
    let stop = memchr::memchr(b'\n', &data[i..]).map_or(data.len(), |j| i + j);
    (serde_json::from_slice(&data[start..stop]).ok(), start)
}

/// The last top-level record of a type from stop on.
pub fn last_record(data: &[u8], rtype: &str, stop: usize) -> Option<Value> {
    let key = format!("\"type\":\"{rtype}\"");
    let finder = memmem::FinderRev::new(key.as_bytes());
    let mut end = data.len();
    loop {
        let i = stop + finder.rfind(&data[stop..end])?;
        let (record, start) = line_at(data, i);
        if let Some(r) = record {
            if s(get(&r, "type")) == Some(rtype) {
                return Some(r);
            }
        }
        if start <= stop {
            return None;
        }
        end = start;
    }
}

/// The record types read_session wants the newest of, and the field it reads from each.
const LAST_TYPES: [(&str, &str); 6] = [
    ("relocated", "relocatedCwd"),
    ("continued-in", "continuedInSessionId"),
    ("custom-title", "customTitle"),
    ("ai-title", "aiTitle"),
    ("agent-name", "agentName"),
    ("last-prompt", "lastPrompt"),
];

/// Field -> value, from the newest record of each type in LAST_TYPES that appears from stop
/// on; types that don't appear are left out. One pass over the file.
pub fn last_fields(data: &[u8], stop: usize) -> Map<String, Value> {
    let mut starts: Vec<Vec<usize>> = vec![Vec::new(); LAST_TYPES.len()];
    let key = b"\"type\":\"";
    for i in memmem::find_iter(&data[stop..], key) {
        let after = &data[stop + i + key.len()..];
        for (k, (rtype, _)) in LAST_TYPES.iter().enumerate() {
            if after.starts_with(rtype.as_bytes()) && after.get(rtype.len()) == Some(&b'"') {
                starts[k].push(stop + i);
            }
        }
    }
    let mut found = Map::new();
    for (k, (rtype, field)) in LAST_TYPES.iter().enumerate() {
        let mut below: Option<usize> = None;
        for &i in starts[k].iter().rev() {
            if below.is_some_and(|b| i >= b) {
                continue;
            }
            let (record, start) = line_at(data, i);
            below = Some(start);
            if let Some(r) = record {
                if r.is_object() && s(get(&r, "type")) == Some(rtype) {
                    found.insert(field.to_string(), get(&r, field).clone());
                    break;
                }
            }
        }
    }
    found
}

/// The model behind the newest reply from stop on, skipping Claude Code's own <synthetic> ones.
pub fn last_model(data: &[u8], stop: usize) -> Value {
    for line in lines_backward(data, stop) {
        if memmem::find(line, b"\"type\":\"assistant\"").is_none()
            || memmem::find(line, b"\"model\":\"").is_none()
        {
            continue;
        }
        let Ok(record) = serde_json::from_slice::<Value>(line) else {
            continue;
        };
        let model = get(get(&record, "message"), "model");
        if s(get(&record, "type")) == Some("assistant")
            && truthy(model)
            && s(model) != Some("<synthetic>")
            && !truthy(get(&record, "isSidechain"))
        {
            return model.clone();
        }
    }
    Value::Null
}

/// What you typed, from a Claude Code record holding a message: its text, unless it's one of
/// the wrappers slash commands and the system add, such as <command-name>.
pub fn prompt_text(record: &Value) -> Option<String> {
    let mut content = get(get(record, "message"), "content");
    if let Value::Array(items) = content {
        content = items
            .iter()
            .find(|c| c.is_object() && s(get(c, "type")) == Some("text"))
            .map_or(py::null(), |c| get(c, "text"));
    }
    let text = s(content)?;
    (!py::strip(text).is_empty() && !py::lstrip(text).starts_with('<')).then(|| text.to_string())
}

/// From the top of a transcript: the first cwd and the first prompt, looked for in the first
/// 2000 lines, and whether the search was over before the end of the file.
fn head_facts(data: &[u8]) -> (Value, Value, bool) {
    let (mut cwd, mut prompt) = (Value::Null, Value::Null);
    for (n, line) in data.split_inclusive(|b| *b == b'\n').enumerate() {
        if truthy(&cwd) && (truthy(&prompt) || n > 2000) {
            return (cwd, prompt, true);
        }
        if memmem::find(line, b"\"cwd\":\"").is_none() {
            continue;
        }
        let Ok(record) = serde_json::from_slice::<Value>(line) else {
            continue;
        };
        if !record.is_object() {
            continue;
        }
        if !truthy(&cwd) {
            cwd = get(&record, "cwd").clone();
        }
        if !truthy(&prompt)
            && s(get(&record, "type")) == Some("user")
            && !truthy(get(&record, "isMeta"))
        {
            prompt = prompt_text(&record).map_or(Value::Null, Value::String);
        }
    }
    (cwd, prompt, false)
}

/// Walking back from the end to stop: the newest record that has an entrypoint and a
/// timestamp, and whether a user or assistant message turned up, where the walk ends.
fn newest_activity(data: &[u8], stop: usize) -> (Option<Value>, bool) {
    let mut last: Option<Value> = None;
    for line in lines_backward(data, stop) {
        if memmem::find(line, b"\"entrypoint\":\"").is_none() {
            continue;
        }
        let Ok(record) = serde_json::from_slice::<Value>(line) else {
            continue;
        };
        if !record.is_object() {
            continue;
        }
        if last.is_none() && truthy(get(&record, "entrypoint")) && truthy(get(&record, "timestamp"))
        {
            last = Some(record.clone());
        }
        if matches!(s(get(&record, "type")), Some("user" | "assistant")) {
            return (last, true);
        }
    }
    (last, false)
}

/// What read_session needs from a transcript, read from start on, with old the facts about
/// the file up to start.
pub fn claude_facts(data: &[u8], old: Option<&Value>, start: usize) -> Value {
    let (mut last, mut has_messages) = newest_activity(data, start);
    let mut fields = last_fields(data, start);
    let mut model = last_model(data, start);
    if let Some(old) = old {
        if !has_messages {
            if last.is_none() && !get(old, "last").is_null() {
                last = Some(get(old, "last").clone());
            }
            has_messages = truthy(get(old, "has_messages"));
        }
        let mut merged = get(old, "fields").as_object().cloned().unwrap_or_default();
        merged.extend(fields);
        fields = merged;
        if !truthy(&model) {
            model = get(old, "model").clone();
        }
    }
    let head = match old.map(|o| get(o, "head")) {
        Some(Value::Array(h)) if h.len() == 3 && truthy(&h[2]) => Value::Array(h.clone()),
        _ => {
            let (cwd, prompt, complete) = head_facts(data);
            json!([cwd, prompt, complete])
        }
    };
    let last = last.map_or(Value::Null, |l| {
        json!({"entrypoint": get(&l, "entrypoint"), "timestamp": get(&l, "timestamp"), "cwd": get(&l, "cwd"), "gitBranch": get(&l, "gitBranch")})
    });
    json!({"last": last, "has_messages": has_messages, "fields": fields, "model": model, "head": head})
}

/// A session as the list shows it.
#[derive(Clone, Debug)]
pub struct Summary {
    pub agent: &'static str,
    pub id: String,
    pub when: f64,
    pub entrypoint: Value,
    pub branch: Value,
    pub cwd: Option<String>,
    pub canonical: bool,
    pub size: u64,
    pub has_messages: bool,
    pub title: String,
    pub continued_in: Value,
    pub model: Value,
    pub profile: String,
    pub path: String,
    pub open: Option<Value>,
    pub bound_to: Option<String>,
}

impl Summary {
    pub fn entrypoint_text(&self) -> String {
        py::str_of(&self.entrypoint)
    }
}

/// The time a stamp names, or the file's modification time when it names none.
fn when_or_mtime(stamp: &Value, path: &str) -> f64 {
    s(stamp)
        .and_then(times::parse_time)
        .filter(|t| *t != 0.0)
        .unwrap_or_else(|| ospath::getmtime(path).unwrap_or(0.0))
}

/// The title: the first of these that's set, with its whitespace folded.
fn title_of(candidates: &[&Value]) -> String {
    let first = candidates.iter().find(|t| truthy(t));
    py::collapse(&first.map_or("(untitled)".to_string(), |t| py::str_of(t)))
}

/// Summary of one transcript, or None when it holds no conversation.
pub fn read_session(path: &str) -> std::io::Result<Option<Summary>> {
    let (facts, size) = file_facts(path, "claude", claude_facts)?;
    let Some(facts) = facts else { return Ok(None) };
    let last = get(&facts, "last");
    if last.is_null() {
        return Ok(None);
    }
    let fields = get(&facts, "fields");
    let head = get(&facts, "head");
    let titles: Vec<&Value> = ["customTitle", "aiTitle", "agentName", "lastPrompt"]
        .iter()
        .map(|k| get(fields, k))
        .collect();
    let (start_cwd, first_prompt) = (
        head.get(0).unwrap_or(py::null()),
        head.get(1).unwrap_or(py::null()),
    );
    let folder = ospath::basename(&ospath::dirname(path));
    let cwd = paths::resume_dir(
        &folder,
        &[
            s(get(fields, "relocatedCwd")),
            s(start_cwd),
            s(get(last, "cwd")),
        ],
    );
    let canonical = cwd
        .as_deref()
        .is_some_and(|c| !c.is_empty() && paths::encode_project(&ospath::realpath(c)) == folder);
    let mut candidates = titles;
    candidates.push(first_prompt);
    let name = ospath::basename(path);
    Ok(Some(Summary {
        agent: "claude",
        id: name.strip_suffix(".jsonl").unwrap_or(&name).to_string(),
        when: when_or_mtime(get(last, "timestamp"), path),
        entrypoint: get(last, "entrypoint").clone(),
        branch: get(last, "gitBranch").clone(),
        cwd,
        canonical,
        size,
        has_messages: truthy(get(&facts, "has_messages")),
        title: title_of(&candidates),
        continued_in: get(fields, "continuedInSessionId").clone(),
        model: get(&facts, "model").clone(),
        profile: String::new(),
        path: path.to_string(),
        open: None,
        bound_to: None,
    }))
}

/// How messages that Codex adds to a conversation itself start.
const CODEX_HARNESS: [&str; 8] = [
    "<environment_context>",
    "# AGENTS.md instructions",
    "<user_instructions>",
    "<goal_context>",
    "<turn_aborted>",
    "The following is the Codex agent history",
    "<permissions instructions>",
    "<recommended_plugins>",
];

pub fn codex_text(content: &Value) -> String {
    let Value::Array(items) = content else {
        return String::new();
    };
    items
        .iter()
        .filter(|c| {
            c.is_object()
                && matches!(
                    s(get(c, "type")),
                    Some("input_text" | "output_text" | "text")
                )
        })
        .filter_map(|c| s(get(c, "text")))
        .collect()
}

/// What you typed, from a Codex record holding one of your messages, or None (also for the
/// messages Codex adds itself).
pub fn codex_prompt(record: &Value) -> Option<String> {
    let payload = get(record, "payload");
    let payload = if payload.is_object() {
        payload
    } else {
        py::null()
    };
    let item = get(payload, "item");
    let item = if item.is_object() { item } else { py::null() };
    let kind = (s(get(record, "type")), s(get(payload, "type")));
    let text = match kind {
        (Some("event_msg"), Some("item_completed"))
            if s(get(item, "type")) == Some("UserMessage") =>
        {
            Some(codex_text(get(item, "content")))
        }
        (Some("event_msg"), Some("user_message")) => s(get(payload, "message")).map(String::from),
        (Some("response_item"), Some("message")) if s(get(payload, "role")) == Some("user") => {
            Some(codex_text(get(payload, "content")))
        }
        _ => None,
    }?;
    let lead = py::lstrip(&text);
    (!py::strip(&text).is_empty() && !CODEX_HARNESS.iter().any(|h| lead.starts_with(h)))
        .then_some(text)
}

/// Where a Codex session was started from: cli for the terminal, or the app, IDE extension
/// or codex exec; subagent for one Codex runs itself.
pub fn codex_entrypoint(meta: &Value) -> String {
    let source = get(meta, "source");
    if source.is_object() || s(get(meta, "thread_source")) == Some("subagent") {
        return "subagent".into();
    }
    match s(source) {
        Some("cli") => return "cli".into(),
        _ if s(get(meta, "originator")) == Some("Codex Desktop") => return "Codex app".into(),
        Some("vscode") => return "Codex IDE".into(),
        Some("exec") => return "codex exec".into(),
        _ => {}
    }
    if truthy(source) {
        py::str_of(source)
    } else {
        "?".into()
    }
}

/// From the top of a Codex session: your first message, and the first that doesn't start
/// with <, which names the session, in the first 5000 lines; and whether the search was over
/// before the end of the file.
fn codex_head(data: &[u8]) -> (Value, Value, bool) {
    let (mut prompt, mut title): (Option<String>, Option<String>) = (None, None);
    for (n, line) in data.split_inclusive(|b| *b == b'\n').enumerate() {
        if title.is_some() || n > 5000 {
            return (
                prompt.map_or(Value::Null, Value::String),
                title.map_or(Value::Null, Value::String),
                true,
            );
        }
        if ![
            &b"\"UserMessage\""[..],
            b"\"user_message\"",
            b"\"role\":\"user\"",
        ]
        .iter()
        .any(|k| memmem::find(line, k).is_some())
        {
            continue;
        }
        let Ok(record) = serde_json::from_slice::<Value>(line) else {
            continue;
        };
        let text = if record.is_object() {
            codex_prompt(&record)
        } else {
            None
        };
        if prompt.is_none() {
            prompt = text.clone();
        }
        title = text.filter(|t| !py::lstrip(t).starts_with('<'));
    }
    (
        prompt.map_or(Value::Null, Value::String),
        title.map_or(Value::Null, Value::String),
        false,
    )
}

/// What read_codex_session needs from a Codex session file, read from start on with old for
/// what comes before.
pub fn codex_facts(data: &[u8], old: Option<&Value>, start: usize) -> Value {
    let (old, start) = match old {
        Some(o) if !get(o, "meta").is_null() => (Some(o), start),
        _ => (None, 0),
    };
    let meta = match old {
        Some(o) => get(o, "meta").clone(),
        None => {
            let first_line = data.split(|b| *b == b'\n').next().unwrap_or(&[]);
            let Ok(first) = serde_json::from_slice::<Value>(first_line) else {
                return json!({"meta": null});
            };
            let meta = get(&first, "payload");
            if !(first.is_object()
                && s(get(&first, "type")) == Some("session_meta")
                && meta.is_object())
            {
                return json!({"meta": null});
            }
            let git = get(meta, "git");
            json!({"id": get(meta, "id"), "cwd": get(meta, "cwd"), "source": get(meta, "source"),
                   "originator": get(meta, "originator"), "thread_source": get(meta, "thread_source"),
                   "branch": if git.is_object() { get(git, "branch").clone() } else { Value::Null },
                   "started": get(&first, "timestamp")})
        }
    };
    let mut last = Value::Null; // the newest record's time: the last activity
    for line in lines_backward(data, start) {
        let Ok(record) = serde_json::from_slice::<Value>(line) else {
            continue;
        };
        if record.is_object()
            && s(get(&record, "timestamp"))
                .and_then(times::parse_time)
                .is_some()
        {
            last = get(&record, "timestamp").clone();
            break;
        }
    }
    let model = match last_record(data, "turn_context", start) {
        Some(context) => {
            let payload = get(&context, "payload");
            if payload.is_object() {
                get(payload, "model").clone()
            } else {
                Value::Null
            }
        }
        None => old.map_or(Value::Null, |o| get(o, "model").clone()),
    };
    let head = match old.map(|o| get(o, "head")) {
        Some(Value::Array(h)) if h.len() == 3 && truthy(&h[2]) => Value::Array(h.clone()),
        _ => {
            let (prompt, title, complete) = codex_head(data);
            json!([prompt, title, complete])
        }
    };
    if last.is_null() {
        if let Some(o) = old {
            last = get(o, "last").clone();
        }
    }
    json!({"meta": meta, "last": last, "model": model, "head": head})
}

/// Session ID -> the name it was given with /rename or by Codex, from session_index.jsonl.
pub fn codex_names(home: &str) -> Map<String, Value> {
    let mut names = Map::new();
    let Ok(text) = std::fs::read(ospath::join(home, "session_index.jsonl")) else {
        return names;
    };
    let Ok(text) = String::from_utf8(text) else {
        return names;
    };
    for line in text.lines() {
        let Ok(entry) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if entry.is_object() && truthy(get(&entry, "id")) && truthy(get(&entry, "thread_name")) {
            if let Some(id) = s(get(&entry, "id")) {
                names.insert(id.to_string(), get(&entry, "thread_name").clone());
                // the newest wins
            }
        }
    }
    names
}

/// Summary of one Codex session file, or None when it holds no conversation.
pub fn read_codex_session(
    path: &str,
    names: &Map<String, Value>,
) -> std::io::Result<Option<Summary>> {
    let (facts, size) = file_facts(path, "codex", codex_facts)?;
    let Some(facts) = facts else { return Ok(None) };
    let meta = get(&facts, "meta");
    if meta.is_null() {
        return Ok(None);
    }
    let head = get(&facts, "head");
    let (prompt, title) = (
        head.get(0).unwrap_or(py::null()),
        head.get(1).unwrap_or(py::null()),
    );
    let sid = if truthy(get(meta, "id")) {
        py::str_of(get(meta, "id"))
    } else {
        paths::codex_file_id(path).map_or_else(|| ospath::basename(path), String::from)
    };
    let name = names.get(&sid).unwrap_or(py::null());
    let stamp = if truthy(get(&facts, "last")) {
        get(&facts, "last")
    } else {
        get(meta, "started")
    };
    Ok(Some(Summary {
        agent: "codex",
        when: when_or_mtime(stamp, path),
        entrypoint: Value::String(codex_entrypoint(meta)),
        branch: get(meta, "branch").clone(),
        cwd: s(get(meta, "cwd")).map(String::from),
        canonical: true,
        size,
        has_messages: !prompt.is_null(),
        title: title_of(&[name, title, prompt]),
        continued_in: Value::Null,
        model: get(&facts, "model").clone(),
        id: sid,
        profile: String::new(),
        path: path.to_string(),
        open: None,
        bound_to: None,
    }))
}

/// read_session or read_codex_session of a file, with its agent, profile and file, or None.
pub fn summarize(agent: &str, profile: &str, path: &str) -> Option<Summary> {
    let s = if agent == "codex" {
        read_codex_session(path, &codex_names(profile))
    } else {
        read_session(path)
    };
    let mut s = s.ok().flatten()?;
    s.profile = profile.to_string();
    Some(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walks_lines_back_to_a_stop() {
        let data = b"one\ntwo\nthree\n";
        assert_eq!(
            lines_backward(data, 0).collect::<Vec<_>>(),
            vec![&b"three"[..], b"two", b"one"]
        );
        assert_eq!(
            lines_backward(data, 4).collect::<Vec<_>>(),
            vec![&b"three"[..], b"two"]
        );
        assert_eq!(lines_backward(b"x", 0).collect::<Vec<_>>(), vec![&b"x"[..]]);
        assert_eq!(lines_backward(b"", 0).count(), 0);
    }

    #[test]
    fn finds_the_newest_of_each_type_in_one_pass() {
        let data = b"{\"type\":\"ai-title\",\"aiTitle\":\"old\"}\n{\"type\":\"user\",\"x\":{\"type\":\"ai-title\"}}\n{\"type\":\"ai-title\",\"aiTitle\":\"new\"}\n{\"type\":\"custom-title\"}\n";
        let found = last_fields(data, 0);
        assert_eq!(found.get("aiTitle"), Some(&json!("new")));
        assert_eq!(found.get("customTitle"), Some(&Value::Null)); // there, without the field
        assert!(!found.contains_key("relocatedCwd"));
        let tail = last_fields(data, data.len());
        assert!(tail.is_empty());
    }
}
