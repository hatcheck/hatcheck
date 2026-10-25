//! Antigravity CLI (agy) as hatcheck reads it. agy keeps everything under
//! ~/.gemini/antigravity-cli, a folder only HOME moves: an index of its conversations,
//! conversation_summaries.db (SQLite); one conversation per conversations/<id>.db, which holds
//! protobuf hatcheck neither reads nor writes; the readable transcript agy writes beside each
//! conversation, brain/<id>/.system_generated/logs/transcript_full.jsonl; and
//! presence/<id>.lock, which it holds while the conversation is open. The Antigravity app keeps
//! its own conversations the same way, beside it in ~/.gemini/antigravity, and each index
//! mentions some of the other's, so a conversation is read from the folder it belongs to. Its
//! usage comes from agy's own /usage, run headless. A session converted for agy is written as
//! a file for agy to read, since nothing hatcheck could write is a conversation agy would
//! resume. docs/agents.md's `Antigravity CLI` section says what is read, and that it was checked
//! against agy 1.4.2 and again 1.4.7.

use crate::agent::Agent;
use crate::conv::{Conversation, Turn};
use crate::facts::{self, Summary};
use crate::ospath;
use crate::paths;
use crate::py::{self, get, s, truthy, Num};
use crate::times;
use crate::usage;
use rusqlite::{Connection, OpenFlags};
use serde_json::{json, Value};
use std::time::Duration;

/// agy's folder, under the home folder, and the Antigravity app's beside it.
pub const ROOT: [&str; 2] = [".gemini", "antigravity-cli"];
pub const APP_ROOT: [&str; 2] = [".gemini", "antigravity"];
/// What agy's /usage is run with: print mode, with the answer as JSON. Its print mode answers
/// a slash command without starting a conversation (checked on agy 1.4.2 by counting its
/// conversations before and after).
pub const USAGE_ARGS: [&str; 4] = ["-p", "/usage", "--output-format", "json"];
/// How long that run may take before it's given up on.
const USAGE_TIMEOUT: Duration = Duration::from_secs(45);
/// What says whether agy is signed in without starting its sign-in: `agy models` lists the
/// models when it is, and when it isn't exits 1 at once with "Please sign in", opening nothing.
/// Print mode asked anything without a sign-in opens the browser on Google's sign-in page and
/// waits a minute for it, whatever stdin or NO_BROWSER say (probed on agy 1.4.2).
pub const SIGNED_IN_ARGS: [&str; 1] = ["models"];
const SIGNED_IN_TIMEOUT: Duration = Duration::from_secs(20);
/// The index file, and the folders the index points into.
const INDEX: &str = "conversation_summaries.db";
const CONVERSATIONS: &str = "conversations";
const BRAIN: &str = "brain";
const PRESENCE: &str = "presence";
/// app_data_dir in the index: the CLI's own conversations, and the Antigravity app's, which
/// the CLI lists too.
const CLI_APP: &str = "antigravity-cli";
const DESKTOP_APP: &str = "antigravity";

/// ~/.gemini/antigravity-cli.
pub fn default_root() -> String {
    ospath::join_all(&paths::home(), &ROOT)
}

/// Whether a folder is an Antigravity CLI data folder: it has the index, or conversations.
pub fn is_root(path: &str) -> bool {
    ospath::isfile(&ospath::join(path, INDEX)) || ospath::isdir(&ospath::join(path, CONVERSATIONS))
}

/// Every Antigravity CLI data folder with conversations: the default one, since only HOME
/// moves it. These are the folders with an account behind them.
pub fn find_roots() -> Vec<String> {
    let root = default_root();
    if is_root(&root) {
        vec![root]
    } else {
        Vec::new()
    }
}

/// ~/.gemini/antigravity: the Antigravity app's folder, laid out as agy's.
pub fn app_root() -> String {
    ospath::join_all(&paths::home(), &APP_ROOT)
}

/// Whether a folder is the Antigravity app's: <home>/.gemini/antigravity.
pub fn is_app_root(path: &str) -> bool {
    let trimmed = path.trim_end_matches(ospath::is_sep);
    ospath::basename(trimmed) == APP_ROOT[1]
        && ospath::basename(&ospath::dirname(trimmed)) == APP_ROOT[0]
}

/// What holds a folder's conversations open: agy, or the Antigravity app for its folder.
pub fn holder(root: &str) -> &'static str {
    if is_app_root(root) {
        "the Antigravity app"
    } else {
        "agy"
    }
}

/// Every folder with conversations to list: agy's, then the app's, when each has some.
pub fn find_stores() -> Vec<String> {
    let mut stores = find_roots();
    let app = app_root();
    if is_root(&app) {
        stores.push(app);
    }
    stores
}

/// The folder a conversation belongs to, by the app_data_dir its index row names: the app's
/// or agy's, beside the folder whose index listed it.
fn owner(root: &str, row: &Row) -> String {
    let beside = ospath::dirname(root.trim_end_matches(ospath::is_sep));
    let app = match row.app.as_str() {
        "" => CLI_APP,
        other => other,
    };
    ospath::join(&beside, app)
}

/// The file a conversation is kept in, and the folder of what agy wrote beside it.
pub fn conversation_path(root: &str, id: &str) -> String {
    ospath::join_all(root, &[CONVERSATIONS, &format!("{id}.db")])
}

pub fn brain_path(root: &str, id: &str) -> String {
    ospath::join_all(root, &[BRAIN, id])
}

/// The transcript agy writes as it goes, one step per line, whole (transcript.jsonl beside it
/// is cut short).
pub fn transcript_path(root: &str, id: &str) -> String {
    ospath::join_all(
        &brain_path(root, id),
        &[".system_generated", "logs", "transcript_full.jsonl"],
    )
}

/// Whether agy has the conversation open: it holds a lock on presence/<id>.lock while it does,
/// and leaves the file behind after, so the lock is what's checked. Not checked on Windows.
pub fn session_open(root: &str, id: &str) -> bool {
    crate::find::lock_held(&ospath::join_all(root, &[PRESENCE, &format!("{id}.lock")]))
}

/// One conversation as the index lists it.
#[derive(Clone, Debug, Default)]
pub struct Row {
    pub id: String,
    pub title: String,
    pub preview: String,
    /// When it last changed, and when you last typed into it, as seconds (0 when unknown).
    pub modified: f64,
    pub last_input: f64,
    /// The folders it ran in, the first being the one it started in.
    pub workspaces: Vec<String>,
    /// antigravity-cli for the CLI's own, antigravity for the Antigravity app's.
    pub app: String,
    /// The conversation that started this one, for a subagent's.
    pub parent: String,
    pub depth: i64,
    pub agent_name: String,
    pub steps: i64,
}

fn text(v: rusqlite::types::Value) -> String {
    match v {
        rusqlite::types::Value::Text(t) => t,
        rusqlite::types::Value::Integer(n) => n.to_string(),
        rusqlite::types::Value::Real(x) => x.to_string(),
        _ => String::new(),
    }
}

fn int(v: rusqlite::types::Value) -> i64 {
    match v {
        rusqlite::types::Value::Integer(n) => n,
        rusqlite::types::Value::Real(x) => x as i64,
        rusqlite::types::Value::Text(t) => t.trim().parse().unwrap_or(0),
        _ => 0,
    }
}

/// The path in a file:// URI, as agy records a workspace: file:///Users/me/code/api, with
/// spaces and the like percent-encoded; on Windows file:///C:/code/api.
pub fn uri_path(uri: &str) -> Option<String> {
    let rest = uri.strip_prefix("file://")?;
    let bytes = rest.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = |b: u8| (b as char).to_digit(16);
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    let mut path = String::from_utf8_lossy(&out).into_owned();
    if ospath::WINDOWS {
        // /C:/code/api -> C:\code\api
        if path.len() >= 3 && path.starts_with('/') && path.as_bytes()[2] == b':' {
            path.remove(0);
        }
        path = path.replace('/', "\\");
    }
    Some(path)
}

fn open(root: &str) -> rusqlite::Result<Connection> {
    Connection::open_with_flags(
        ospath::join(root, INDEX),
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
}

const COLUMNS: &str = "conversation_id, title, preview, step_count, last_modified_time, \
                       last_user_input_time, workspace_uris, app_data_dir, \
                       parent_conversation_id, nesting_depth, agent_name";

/// A conversation's folders with the one it was started from first. The command hatcheck prints
/// for a converted session adds hatcheck's imports folder with --add-dir, which agy 1.4.7 lists
/// before the folder it was started from, so that one goes last.
fn started_in_first(mut folders: Vec<String>, imports: &str) -> Vec<String> {
    if folders.len() > 1 {
        if let Some(i) = folders.iter().position(|f| paths::same_folder(f, imports)) {
            let added = folders.remove(i);
            folders.push(added);
        }
    }
    folders
}

fn row_of(r: &rusqlite::Row) -> rusqlite::Result<Row> {
    let v = |i: usize| r.get::<_, rusqlite::types::Value>(i);
    let when =
        |i: usize| -> rusqlite::Result<f64> { Ok(times::parse_time(&text(v(i)?)).unwrap_or(0.0)) };
    let workspaces: Vec<String> = serde_json::from_str::<Value>(&text(v(6)?))
        .ok()
        .and_then(|w| w.as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(|u| s(u).and_then(uri_path))
        .collect();
    let workspaces = started_in_first(workspaces, &crate::conv::imports_dir());
    Ok(Row {
        id: text(v(0)?),
        title: text(v(1)?),
        preview: text(v(2)?),
        steps: int(v(3)?),
        modified: when(4)?,
        last_input: when(5)?,
        workspaces,
        app: text(v(7)?),
        parent: text(v(8)?),
        depth: int(v(9)?),
        agent_name: text(v(10)?),
    })
}

/// Every conversation of the folder's own in its index, the newest change first; none when
/// the index can't be read. A conversation the index lists for the folder beside it (agy's for
/// the app's, and the other way round) is left to that folder's index, when there is one.
pub fn rows(root: &str) -> Vec<Row> {
    let Ok(db) = open(root) else {
        return Vec::new();
    };
    let Ok(mut query) = db.prepare(&format!(
        "SELECT {COLUMNS} FROM conversation_summaries ORDER BY last_modified_time DESC"
    )) else {
        return Vec::new();
    };
    let Ok(found) = query.query_map([], row_of) else {
        return Vec::new();
    };
    found
        .flatten()
        .filter(|r| {
            let owner = owner(root, r);
            paths::same_folder(&owner, root) || !is_root(&owner)
        })
        .collect()
}

/// The conversations whose ID starts with prefix (IDs are lower-case hex).
pub fn rows_starting_with(root: &str, prefix: &str) -> Vec<Row> {
    let lower = prefix.to_lowercase();
    rows(root)
        .into_iter()
        .filter(|r| r.id.starts_with(&lower))
        .collect()
}

pub fn row(root: &str, id: &str) -> Option<Row> {
    rows(root).into_iter().find(|r| r.id == id)
}

/// How the session list files a conversation: cli for the CLI's own, Antigravity app for the
/// app's, subagent for one agy ran itself.
pub fn entrypoint(row: &Row) -> String {
    if row.depth > 0 || !row.parent.is_empty() {
        "subagent".into()
    } else if row.app == CLI_APP || row.app.is_empty() {
        "cli".into()
    } else if row.app == DESKTOP_APP {
        "Antigravity app".into()
    } else {
        row.app.clone()
    }
}

/// A conversation as the session list shows it.
pub fn summary(root: &str, row: &Row) -> Summary {
    let path = conversation_path(root, &row.id);
    let title = [&row.title, &row.preview]
        .into_iter()
        .map(|t| py::collapse(t))
        .find(|t| !t.is_empty())
        .unwrap_or_else(|| "(untitled)".into());
    let when = if row.modified != 0.0 {
        row.modified
    } else if row.last_input != 0.0 {
        row.last_input
    } else {
        ospath::getmtime(&path).unwrap_or(0.0)
    };
    Summary {
        agent: Agent::Agy,
        id: row.id.clone(),
        when,
        entrypoint: json!(entrypoint(row)),
        branch: Value::Null,
        cwd: row.workspaces.first().cloned(),
        canonical: true,
        size: std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0),
        has_messages: row.steps > 0 && title != "(untitled)",
        title,
        continued_in: Value::Null,
        model: Value::Null,
        profile: root.to_string(),
        path,
        open: None,
        bound_to: None,
    }
}

/// The ID in a conversation file's path, conversations/<id>.db.
pub fn file_id(path: &str) -> Option<&str> {
    let name = path.rsplit(['/', '\\']).next()?;
    let id = name.strip_suffix(".db")?;
    let hex = |c: char| c.is_ascii_hexdigit() || c == '-';
    (id.len() == 36 && id.chars().all(hex)).then_some(id)
}

/// The summary of the conversation kept at path, or None when the index doesn't list it.
pub fn summarize(root: &str, path: &str) -> Option<Summary> {
    let id = file_id(path)?;
    row(root, id).map(|r| summary(root, &r))
}

/// A conversation as hatcheck shows and converts it, from the transcript agy wrote: what you
/// typed, what the model said, each tool call as text with its arguments, and each tool's
/// output. agy's thinking is left out, as the other agents' reasoning is.
pub fn read_conversation(root: &str, id: &str) -> std::io::Result<Conversation> {
    let path = transcript_path(root, id);
    let data = std::fs::read(&path)?;
    let text = String::from_utf8_lossy(&data);
    let (mut turns, mut calls, mut started): (Vec<Turn>, usize, Value) =
        (Vec::new(), 0, Value::Null);
    for line in text.lines() {
        let Ok(step) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if !step.is_object() {
            continue;
        }
        let when = get(&step, "created_at");
        if started.is_null() && truthy(when) {
            started = when.clone();
        }
        let content = s(get(&step, "content")).unwrap_or("");
        match s(get(&step, "type")) {
            Some("USER_INPUT") => crate::conv::add_turn(&mut turns, "user", content, when),
            Some("PLANNER_RESPONSE") => {
                crate::conv::add_turn(&mut turns, "assistant", content, when);
                if let Value::Array(list) = get(&step, "tool_calls") {
                    for call in list.iter().filter(|c| c.is_object()) {
                        let name = s(get(call, "name")).unwrap_or("tool");
                        let fields: Vec<(String, Value)> = match get(call, "args") {
                            Value::Object(o) => {
                                o.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
                            }
                            Value::Null => Vec::new(),
                            other => vec![("input".into(), other.clone())],
                        };
                        crate::conv::add_turn(
                            &mut turns,
                            "assistant",
                            &crate::conv::call_text(name, &fields),
                            when,
                        );
                        calls += 1;
                    }
                }
            }
            // A tool's output, or what went wrong running it.
            Some("GENERIC") if s(get(&step, "source")) == Some("MODEL") => {
                let output = if content.is_empty() {
                    s(get(&step, "error")).unwrap_or("")
                } else {
                    content
                };
                crate::conv::add_turn(
                    &mut turns,
                    "assistant",
                    &crate::conv::result_text(output),
                    when,
                );
            }
            _ => {} // agy's own checkpoints and system messages
        }
    }
    let row = row(root, id).unwrap_or_default();
    let title = [&row.title, &row.preview]
        .into_iter()
        .map(|t| py::collapse(t))
        .find(|t| !t.is_empty());
    Ok(Conversation {
        agent: Agent::Agy,
        id: id.to_string(),
        cwd: row.workspaces.first().cloned(),
        model: Value::Null,
        turns,
        calls,
        title,
        started,
        branch: Value::Null,
        reasoning: "Antigravity's thinking",
        left_out: "Antigravity's thinking, images, artifacts and subagent conversations",
    })
}

// Usage: agy's own /usage, run headless, and hatcheck's copy of its last answer, since agy
// saves none.

/// Where agy is run for /usage: a folder of hatcheck's own, so whatever agy notes about the
/// folder it runs in is the same every time.
pub fn usage_run_dir() -> String {
    ospath::join(&facts::cache_dir(), "agy-usage")
}

/// How long a run waits for another hatcheck's agy run to finish: the other's two steps at
/// their longest.
const LOCK_WAIT: Duration = Duration::from_secs(70);
/// A reading hatcheck took this recently is handed on instead of asking agy again: agy's two
/// steps, the model list and /usage, each a request to Google, take five seconds or more
/// together, the longest of every account's reading, and its limits move slowly.
pub const FRESH: f64 = 5.0 * 60.0;

/// One agy run at a time across hatchecks: the lock on run.lock in agy's run folder, held while
/// agy is asked, and waited for up to LOCK_WAIT when another hatcheck holds it (the app's engine
/// and the CLI can ask at once). Two /usage runs at the same time made Google's quota service
/// answer the second with a 503, after a minute (probed on agy 1.4.3). The lock, or None when
/// it can't be made or the wait ran out (the run goes ahead without it), and whether another
/// hatcheck held it first.
pub fn take_run_lock() -> (Option<std::fs::File>, bool) {
    let run_dir = usage_run_dir();
    let _ = std::fs::create_dir_all(&run_dir);
    let Ok(file) = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(ospath::join(&run_dir, "run.lock"))
    else {
        return (None, false);
    };
    let deadline = std::time::Instant::now() + LOCK_WAIT;
    let mut waited = false;
    loop {
        match file.try_lock() {
            Ok(()) => return (Some(file), waited),
            Err(std::fs::TryLockError::WouldBlock) => waited = true,
            Err(_) => return (None, waited),
        }
        if std::time::Instant::now() >= deadline {
            return (None, waited);
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

/// Run agy's /usage for a data folder: the data it reported, with its quota groups, or why it
/// gave none. agy reads ~/.gemini/antigravity-cli, so only the default folder can be asked.
pub fn run_usage(root: &str) -> Result<Value, String> {
    let Some(agy) = ospath::which("agy") else {
        return Err("agy isn't on your PATH".into());
    };
    if !paths::same_folder(root, &default_root()) {
        return Err("agy reads only ~/.gemini/antigravity-cli".into());
    }
    let run_dir = usage_run_dir();
    let _ = std::fs::create_dir_all(&run_dir);
    signed_in(&agy, &run_dir)?;
    let mut command = std::process::Command::new(agy);
    command.args(USAGE_ARGS);
    if ospath::isdir(&run_dir) {
        command.current_dir(&run_dir);
    }
    let out = usage::output_within(command, USAGE_TIMEOUT).map_err(|e| format!("agy {e}"))?;
    let answer: Value =
        serde_json::from_str(out.trim()).map_err(|_| "agy's /usage gave no JSON".to_string())?;
    if s(get(&answer, "status")) == Some("ERROR") || truthy(get(&answer, "error")) {
        return Err(format!(
            "agy's /usage failed: {}",
            s(get(&answer, "error")).unwrap_or("?")
        ));
    }
    let data = get(get(&answer, "command"), "data");
    match get(data, "groups") {
        Value::Array(groups) if !groups.is_empty() => Ok(data.clone()),
        _ => Err("agy's /usage reported no quota groups".into()),
    }
}

/// Whether agy is signed in, from `agy models` (see SIGNED_IN_ARGS); the reason when it isn't,
/// or when agy couldn't say.
fn signed_in(agy: &str, run_dir: &str) -> Result<(), String> {
    let mut command = std::process::Command::new(agy);
    command.args(SIGNED_IN_ARGS);
    if ospath::isdir(run_dir) {
        command.current_dir(run_dir);
    }
    match usage::run_within(command, SIGNED_IN_TIMEOUT, usage::Keep::Stderr) {
        Ok((true, _)) => Ok(()),
        Ok((false, stderr)) if stderr.contains("sign in") => {
            Err("agy isn't signed in; run agy to sign in".into())
        }
        Ok((false, stderr)) => Err(format!(
            "agy couldn't list its models: {}",
            stderr
                .lines()
                .map(str::trim)
                .rfind(|l| !l.is_empty() && !l.starts_with("Fetching"))
                .unwrap_or("?")
        )),
        Err(e) => Err(format!("agy {e}")),
    }
}

/// A name for a quota group, window or model, from the names agy gives such things.
fn label_of(v: &Value, fallback: &str) -> String {
    [
        "display_name",
        "group_name",
        "name",
        "window_id",
        "group_id",
        "model_id",
        "id",
    ]
    .iter()
    .map(|k| s(get(v, k)).unwrap_or(""))
    .find(|t| !t.is_empty())
    .map_or(fallback.to_string(), String::from)
}

/// The length of a bucket's window, from the word agy 1.4.3 names it by.
fn window_seconds(window: &str) -> Value {
    match window.trim().to_lowercase().as_str() {
        "5h" => json!(5 * 3600),
        "weekly" | "week" | "7d" => json!(7 * 86400),
        "daily" | "24h" => json!(86400),
        "monthly" | "month" | "30d" => json!(30 * 86400),
        _ => Value::Null,
    }
}

/// agy's /usage answer as a usage body with limits[] rows like Claude's: one row per quota it
/// reports, each the share of a model group's quota used and when it resets. agy 1.4.3 names
/// groups, each with a name and `buckets`, each with a `window` (5h or weekly), a
/// remaining_fraction and a reset_time; agy 1.4.2 named the same with quota_info on the group,
/// on each of its windows or on each of its models, which is still read. A remaining_fraction
/// left out beside a reset_time means none left.
pub fn usage_body(data: &Value) -> Value {
    let mut rows = Vec::new();
    let groups = match get(data, "groups") {
        Value::Array(g) => g.iter().filter(|g| g.is_object()).collect::<Vec<_>>(),
        _ => Vec::new(),
    };
    for group in groups {
        let name = label_of(group, "quota");
        let mut meters: Vec<(String, &Value, Value)> = Vec::new(); // label, the meter, its window's length
        for (key, prefix) in [("buckets", true), ("windows", true), ("models", false)] {
            if let Value::Array(items) = get(group, key) {
                for item in items.iter().filter(|i| i.is_object()) {
                    // A bucket is named by its window, 5h or weekly, not its long name.
                    let window = s(get(item, "window")).unwrap_or("").to_string();
                    let part = if key == "buckets" && !window.is_empty() {
                        window.clone()
                    } else {
                        label_of(item, "")
                    };
                    let label = if part.is_empty() {
                        name.clone()
                    } else if prefix {
                        format!("{name} {part}")
                    } else {
                        part
                    };
                    meters.push((label, item, window_seconds(&window)));
                }
            }
        }
        if meters.is_empty() {
            meters.push((name.clone(), group, Value::Null));
        }
        for (label, meter, window) in meters {
            let quota = if get(meter, "quota_info").is_object() {
                get(meter, "quota_info")
            } else {
                meter
            };
            let reset = s(get(quota, "reset_time")).and_then(times::parse_time);
            let used = match (
                Num::of(get(quota, "used_percentage"))
                    .filter(|_| get(quota, "used_percentage").is_number()),
                Num::of(get(quota, "remaining_fraction"))
                    .filter(|_| get(quota, "remaining_fraction").is_number()),
            ) {
                (Some(u), _) => u.f(),
                (None, Some(r)) => 100.0 - r.f() * 100.0,
                (None, None) if reset.is_some() => 100.0, // the field left out: nothing left
                (None, None) => continue,
            };
            let used = py::round(used.clamp(0.0, 100.0)) as i64;
            let severity = if used >= 90 {
                "critical"
            } else if used >= 75 {
                "warning"
            } else {
                "normal"
            };
            rows.push(json!({"kind": "quota_scoped", "group": "quota", "label": label.clone(), "short": label,
                             "window_seconds": window, "percent": used,
                             "resets_at": reset.and_then(times::isoformat_utc).map_or(Value::Null, Value::String),
                             "severity": severity}));
        }
    }
    let tier = [
        get(get(data, "plan_info"), "tier_display_name"),
        get(data, "tier_display_name"),
        get(data, "user_tier"),
        get(data, "tier"),
    ]
    .into_iter()
    .find(|t| truthy(t))
    .cloned()
    .unwrap_or(Value::Null);
    json!({"limits": rows, "plan_type": tier})
}

/// The plan, as the usage names it: Antigravity, with its tier when /usage said one.
pub fn plan_text(usage: Option<&Value>) -> String {
    match usage.map(|u| get(u, "plan_type")).filter(|t| truthy(t)) {
        Some(tier) => format!("Antigravity {}", py::str_of(tier)),
        None => "Antigravity".into(),
    }
}

fn snapshot_path() -> String {
    ospath::join(&usage_run_dir(), "last-usage.json")
}

/// Keep the last reading, with when it was taken, so a later run without one can show it with
/// its age, as the other agents' own snapshots are shown.
pub fn save_snapshot(root: &str, body: &Value, now: f64) {
    let path = snapshot_path();
    let mut all: Value = std::fs::read_to_string(&path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .filter(Value::is_object)
        .unwrap_or_else(|| json!({}));
    all[root] = json!({"saved_at": now, "usage": body});
    let _ = std::fs::create_dir_all(usage_run_dir());
    let tmp = format!("{path}.{}.tmp", std::process::id());
    if std::fs::write(&tmp, py::dumps(&all))
        .and_then(|()| std::fs::rename(&tmp, &path))
        .is_err()
    {
        let _ = std::fs::remove_file(&tmp);
    }
}

/// (usage body, when it was read) of the last reading kept for a data folder.
pub fn read_snapshot(root: &str) -> Option<(Value, f64)> {
    let all: Value = serde_json::from_str(&std::fs::read_to_string(snapshot_path()).ok()?).ok()?;
    let entry = get(&all, root);
    let saved = Num::of(get(entry, "saved_at"))?.f();
    let body = get(entry, "usage");
    truthy(body).then(|| (body.clone(), saved))
}

// A conversation converted for agy: written as a file for agy to read (conv::write_import),
// since nothing hatcheck could write is a conversation agy would resume.

/// The first prompt agy gets with an imported conversation: read the file, then go on.
pub fn prompt(file: &str) -> String {
    format!("Read {file}, a conversation imported from another coding agent, and carry it on from where it ended.")
}

/// The command that starts agy on an imported conversation, from the session's folder: the
/// imports folder added to the workspace, so agy may read the file, and a first prompt that
/// has it carry on from there.
pub fn start_command(root: &str, file: &str, full: bool) -> String {
    paths::agy_command(
        root,
        &format!(
            "--add-dir {} -i {}",
            paths::shell_path(&crate::conv::imports_dir()),
            paths::quote(&prompt(file))
        ),
        full,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_uris_are_paths() {
        // As this system spells a path: with \ on Windows.
        let native = |p: &str| p.replace('/', ospath::SEP);
        assert_eq!(
            uri_path("file:///Users/me/code/api"),
            Some(native("/Users/me/code/api"))
        );
        assert_eq!(
            uri_path("file:///Users/me/my%20code"),
            Some(native("/Users/me/my code"))
        );
        if ospath::WINDOWS {
            assert_eq!(
                uri_path("file:///C:/code/api"),
                Some("C:\\code\\api".into())
            );
        }
        assert_eq!(uri_path("/Users/me"), None);
        assert_eq!(
            file_id("/r/conversations/c134893c-20f5-48ea-bc9b-7f7c9271bc4e.db"),
            Some("c134893c-20f5-48ea-bc9b-7f7c9271bc4e")
        );
        assert_eq!(file_id("/r/conversations/x.db"), None);
    }

    #[test]
    fn the_folder_started_from_comes_before_the_imports_folder() {
        // As agy 1.4.7 lists a conversation started with the command hatcheck prints for an import.
        let (imports, api) = ("/c/hatcheck/imports", "/code/api");
        let owned = |v: &[&str]| v.iter().map(|f| f.to_string()).collect::<Vec<_>>();
        let order = |v: &[&str]| started_in_first(owned(v), imports);
        assert_eq!(order(&[imports, api]), owned(&[api, imports]));
        assert_eq!(order(&[imports]), owned(&[imports]));
        assert_eq!(order(&[api]), owned(&[api]));
    }

    #[test]
    fn usage_groups_become_rows() {
        let later = times::isoformat_utc(1_800_000_000.0).unwrap();
        let data = json!({"groups": [
            {"group_name": "Gemini 4 Argon", "quota_info": {"remaining_fraction": 0.62, "reset_time": later}},
            {"group_name": "Gemini 3.8 Flash", "windows": [
                {"window_id": "5h", "remaining_fraction": 0.05, "reset_time": later},
                {"window_id": "week", "reset_time": later}]},
            {"group_name": "nothing known"}],
            "plan_info": {"tier_display_name": "Pro"}});
        let body = usage_body(&data);
        let rows = body["limits"].as_array().unwrap();
        let labels: Vec<&str> = rows.iter().map(|r| r["label"].as_str().unwrap()).collect();
        assert_eq!(
            labels,
            [
                "Gemini 4 Argon",
                "Gemini 3.8 Flash 5h",
                "Gemini 3.8 Flash week"
            ]
        );
        let used: Vec<i64> = rows
            .iter()
            .map(|r| r["percent"].as_i64().unwrap())
            .collect();
        assert_eq!(used, [38, 95, 100]); // a remaining_fraction left out beside a reset: none left
        assert_eq!(rows[1]["severity"], "critical");
        assert_eq!(rows[0]["resets_at"], later);
        assert_eq!(rows[0]["window_seconds"], Value::Null); // 1.4.2 named no window lengths
        assert_eq!(plan_text(Some(&body)), "Antigravity Pro");
        assert_eq!(plan_text(None), "Antigravity");
    }

    #[test]
    fn buckets_become_rows_with_their_windows() {
        // As agy 1.4.3 answered on 2026-10-20: each group's limits in buckets, named by window.
        let later = times::isoformat_utc(1_800_000_000.0).unwrap();
        let data = json!({"groups": [
            {"name": "Gemini Models", "description": "Models within this group: Gemini Flash, Gemini Pro", "buckets": [
                {"id": "gemini-weekly", "name": "Weekly Limit Remaining", "window": "weekly", "remaining_fraction": 0.9999, "reset_time": later},
                {"id": "gemini-5h", "name": "Five Hour Limit Remaining", "window": "5h", "remaining_fraction": 0.25, "reset_time": later}]},
            {"name": "Claude and GPT models", "buckets": [
                {"id": "3p-weekly", "name": "Weekly Limit Remaining", "window": "weekly", "remaining_fraction": 1, "reset_time": later}]}]});
        let body = usage_body(&data);
        let rows = body["limits"].as_array().unwrap();
        let labels: Vec<&str> = rows.iter().map(|r| r["label"].as_str().unwrap()).collect();
        assert_eq!(
            labels,
            [
                "Gemini Models weekly",
                "Gemini Models 5h",
                "Claude and GPT models weekly"
            ]
        );
        let used: Vec<i64> = rows
            .iter()
            .map(|r| r["percent"].as_i64().unwrap())
            .collect();
        assert_eq!(used, [0, 75, 0]);
        assert_eq!(rows[1]["severity"], "warning");
        assert_eq!(rows[0]["window_seconds"], 7 * 86400);
        assert_eq!(rows[1]["window_seconds"], 5 * 3600);
        assert_eq!(plan_text(Some(&body)), "Antigravity"); // 1.4.3 names no tier
    }
}
