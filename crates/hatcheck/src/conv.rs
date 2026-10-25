//! A session as a conversation: your messages and the replies, with each tool call as a
//! block of text with its input and the start of its output, in the format Codex's /import
//! gives Claude Code's tool calls. hatcheck show prints it; hatcheck move writes it into the
//! other agent's format.

use crate::agent::Agent;
use crate::facts::{self, codex_text};
use crate::ospath;
use crate::paths;
use crate::py::{self, get, s, truthy};
use serde_json::Value;

/// How much of each tool call's input and output to keep, as (lines, characters).
pub const CALL_CLIP: (usize, usize) = (30, 3000);
pub const RESULT_CLIP: (usize, usize) = (20, 2000);

/// How messages that Claude Code adds to a conversation itself start.
const CLAUDE_HARNESS: [&str; 8] = [
    "<command-name>",
    "<command-message>",
    "<command-args>",
    "<local-command-stdout>",
    "<local-command-stderr>",
    "<local-command-caveat>",
    "<task-notification>",
    "<system-reminder>",
];

/// A turn: who spoke, what was said (texts joined later with a blank line), and when.
pub type Turn = (String, Vec<String>, Value);

pub struct Conversation {
    pub agent: Agent,
    pub id: String,
    pub cwd: Option<String>,
    pub model: Value,
    pub turns: Vec<Turn>,
    pub calls: usize,
    pub title: Option<String>,
    pub started: Value,
    pub branch: Value,
    pub reasoning: &'static str,
    pub left_out: &'static str,
}

/// text cut to (lines, characters), saying how much was left out.
pub fn clip(text: &str, limits: (usize, usize)) -> String {
    let (lines, chars) = limits;
    let text = text.trim_matches('\n');
    let rows = py::splitlines(text);
    let joined = rows
        .iter()
        .take(lines)
        .copied()
        .collect::<Vec<_>>()
        .join("\n");
    let shown = py::head(&joined, chars);
    let hidden = rows.len() as i64 - py::splitlines(shown).len() as i64;
    if hidden > 0 {
        return format!(
            "{shown}\n… ({hidden} more line{})",
            if hidden != 1 { "s" } else { "" }
        );
    }
    let cut = shown.chars().count() < text.chars().count();
    format!("{shown}{}", if cut { " …" } else { "" })
}

/// A tool call as text, in the format Codex's /import gives Claude Code's tool calls.
pub fn call_text(name: &str, fields: &[(String, Value)]) -> String {
    let mut lines = vec![format!("[external_agent_tool_call: {name}]")];
    for (key, value) in fields {
        let value = match value {
            Value::String(text) => text.clone(),
            other => py::dumps_compact(other),
        };
        lines.push(format!("{key}: {}", clip(&value, CALL_CLIP)));
    }
    lines.push("[/external_agent_tool_call]".into());
    lines.join("\n")
}

pub fn result_text(text: &str) -> String {
    format!(
        "[external_agent_tool_result]\n{}\n[/external_agent_tool_result]",
        clip(text, RESULT_CLIP)
    )
}

// A conversation converted for an agent whose store hatcheck doesn't write (agy, zcode): a
// file for the agent to read, which hatcheck starts it on.

/// Where converted conversations are written: cache/imports/<id>.md.
pub fn imports_dir() -> String {
    ospath::join(&facts::cache_dir(), "imports")
}

pub fn import_path(id: &str) -> String {
    ospath::join(&imports_dir(), &format!("{id}.md"))
}

/// Write the conversation as Markdown, each turn under who said it and when: the file.
pub fn write_import(conv: &Conversation, turns: &[Turn], id: &str) -> std::io::Result<String> {
    let mut text = format!(
        "# Conversation imported from {} session {}\n",
        conv.agent.name(),
        conv.id
    );
    for (role, texts, when) in turns {
        let who = if role == "user" {
            "You"
        } else {
            conv.agent.speaker()
        };
        let at = s(when)
            .filter(|w| !w.is_empty())
            .map_or(String::new(), |w| {
                format!(" ({})", py::head(w, 16).replace('T', " "))
            });
        text += &format!("\n## {who}{at}\n\n{}\n", texts.join("\n\n"));
    }
    let path = import_path(id);
    std::fs::create_dir_all(imports_dir())?;
    let tmp = format!("{path}.hatcheck-tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, &path)?;
    Ok(path)
}

/// Add text to the conversation, joining it to the turn before when that's the same role's.
pub fn add_turn(turns: &mut Vec<Turn>, role: &str, text: &str, when: &Value) {
    if py::strip(text).is_empty() {
        return;
    }
    if let Some(last) = turns.last_mut().filter(|t| t.0 == role) {
        last.1.push(text.to_string());
        if truthy(when) {
            last.2 = when.clone();
        }
    } else {
        turns.push((role.to_string(), vec![text.to_string()], when.clone()));
    }
}

// A handover brief: what another agent gets in place of the whole conversation with
// `hatcheck move --brief`, as Orca hands a task to a fresh agent with a bounded prompt built from
// the old transcript. hatcheck runs no model, so the brief is made from what the conversation
// says: its first prompt, the files and commands its tool calls named, and its last turns.

/// How much of a conversation's end a brief keeps: its last BRIEF_TURNS turns, within BRIEF_CHARS
/// characters, the newest cut to fit when it alone is longer.
pub const BRIEF_TURNS: usize = 6;
pub const BRIEF_CHARS: usize = 12_000;
/// How many files and commands a brief lists at most.
const BRIEF_FILES: usize = 40;
const BRIEF_COMMANDS: usize = 20;

/// The newest turn cut to room characters, its latest parts kept: what it ended on is where the
/// conversation carries on. When even its last part is too long, that part, cut.
pub fn cut_turn((role, texts, when): &Turn, room: usize) -> Turn {
    let mut kept: Vec<String> = Vec::new();
    let mut used = 0;
    for text in texts.iter().rev() {
        used += text.chars().count();
        if used > room {
            break;
        }
        kept.push(text.clone());
    }
    if kept.is_empty() {
        kept.push(clip(
            texts.last().map_or("", String::as_str),
            (usize::MAX, room),
        ));
    }
    kept.reverse();
    let left = texts.len() - kept.len();
    if left > 0 {
        kept.insert(
            0,
            format!(
                "[{left} earlier part{} of this turn left out here, to fit the context window.]",
                if left != 1 { "s" } else { "" }
            ),
        );
    }
    (role.clone(), kept, when.clone())
}

/// The files and commands the conversation's tool calls named, each in the order first seen:
/// the path-like and command fields of its `[external_agent_tool_call: …]` blocks.
pub fn touched(turns: &[Turn]) -> (Vec<String>, Vec<String>) {
    let (mut files, mut commands): (Vec<String>, Vec<String>) = (Vec::new(), Vec::new());
    let add = |list: &mut Vec<String>, value: &str| {
        if !value.is_empty() && !list.iter().any(|v| v == value) {
            list.push(value.to_string());
        }
    };
    for (role, texts, _) in turns {
        if role != "assistant" {
            continue;
        }
        for text in texts {
            let mut in_call = false;
            for line in text.lines() {
                if line.starts_with("[external_agent_tool_call: ") {
                    in_call = true;
                } else if line.starts_with("[/external_agent_tool_call]") {
                    in_call = false;
                } else if in_call {
                    let Some((key, value)) = line.split_once(": ") else {
                        continue;
                    };
                    match key {
                        "path" | "file_path" | "filePath" | "notebook_path" | "target_file"
                        | "file" | "absolute_path" => add(&mut files, value.trim()),
                        "command" | "cmd" => add(&mut commands, py::head(value.trim(), 160)),
                        _ => {}
                    }
                }
            }
        }
    }
    (files, commands)
}

/// The last turns of a conversation, as a brief hands them over: the last BRIEF_TURNS within
/// BRIEF_CHARS, the newest cut when it alone is longer. Also whether it was cut.
pub fn brief_tail(conv: &Conversation) -> (Vec<Turn>, bool) {
    let mut kept: Vec<Turn> = Vec::new();
    let mut used = 0;
    let mut cut = false;
    for turn in conv.turns.iter().rev() {
        let size: usize = turn.1.iter().map(|t| t.chars().count()).sum();
        if kept.is_empty() && size > BRIEF_CHARS {
            kept.push(cut_turn(turn, BRIEF_CHARS));
            cut = true;
            break;
        }
        if kept.len() >= BRIEF_TURNS || used + size > BRIEF_CHARS {
            break;
        }
        kept.push(turn.clone());
        used += size;
    }
    kept.reverse();
    (kept, cut)
}

/// The text of a handover brief: what the session was about, what its tool calls touched, and
/// that its last turns follow.
pub fn brief_text(conv: &Conversation) -> String {
    let prompts = conv.turns.iter().filter(|t| t.0 == "user").count();
    let title = conv
        .title
        .as_deref()
        .map(py::collapse)
        .filter(|t| !t.is_empty())
        .map_or(String::new(), |t| format!(" \"{t}\""));
    let first = conv
        .turns
        .iter()
        .find(|t| t.0 == "user")
        .map(|t| t.1.join("\n\n"))
        .unwrap_or_default();
    let (files, commands) = touched(&conv.turns);
    let (tail, _) = brief_tail(conv);
    let listed = |items: &[String], most: usize, each: &dyn Fn(&str) -> String| {
        let mut lines: Vec<String> = items.iter().take(most).map(|i| each(i)).collect();
        if items.len() > most {
            lines.push(format!("- … and {} more", items.len() - most));
        }
        lines.join("\n")
    };
    let mut text = format!(
        "Handover brief for {} session {}{}: {prompts} of the user's messages and {} tool calls, of which the last {} turn{} follow{} as they were. The rest is summarised here without a model: the first prompt, and what the tool calls named.",
        conv.agent.name(),
        conv.id,
        title,
        conv.calls,
        tail.len(),
        if tail.len() == 1 { "" } else { "s" },
        if tail.len() == 1 { "s" } else { "" },
    );
    text += &format!("\n\n## The task\n\n{}", clip(&first, (40, 2000)));
    if !files.is_empty() {
        text += &format!(
            "\n\n## Files the tool calls named\n\n{}",
            listed(&files, BRIEF_FILES, &|f| format!("- {f}"))
        );
    }
    if !commands.is_empty() {
        text += &format!(
            "\n\n## Commands it ran\n\n{}",
            listed(&commands, BRIEF_COMMANDS, &|c| format!("- `{c}`"))
        );
    }
    text += "\n\n## Where it ended\n\nCarry on from the last turn, which follows; ask before redoing anything it says is done.";
    text
}

/// The brief as turns: its text as the user's, then the last turns as they were.
pub fn brief_turns(conv: &Conversation) -> Vec<Turn> {
    let (tail, _) = brief_tail(conv);
    let mut turns = vec![(
        "user".to_string(),
        vec![brief_text(conv)],
        conv.started.clone(),
    )];
    turns.extend(tail);
    turns
}

/// A name for a tool call: the value as Python's "%s" prints it.
fn name_of(v: &Value, fallback: &str) -> String {
    if truthy(v) {
        py::str_of(v)
    } else {
        fallback.to_string()
    }
}

fn text_or_empty(v: &Value) -> Value {
    if truthy(v) {
        v.clone()
    } else {
        Value::String(String::new())
    }
}

fn codex_call_text(p: &Value) -> String {
    let kind = s(get(p, "type"));
    let field = |k: &str, v: Value| vec![(k.to_string(), v)];
    match kind {
        Some("custom_tool_call") => {
            return call_text(
                &name_of(get(p, "name"), "tool"),
                &field("input", text_or_empty(get(p, "input"))),
            )
        }
        Some("web_search_call") => {
            return call_text(
                "web_search",
                &field("query", text_or_empty(get(get(p, "action"), "query"))),
            )
        }
        Some("image_generation_call") => {
            return call_text(
                "image_generation",
                &field("prompt", text_or_empty(get(p, "revised_prompt"))),
            )
        }
        Some("local_shell_call") => {
            let words: Vec<String> = match get(get(p, "action"), "command") {
                Value::Array(items) => items.iter().map(py::str_of).collect(),
                _ => Vec::new(),
            };
            return call_text("shell", &field("command", Value::String(words.join(" "))));
        }
        Some("tool_search_call") => {
            return call_text(
                "tool_search",
                &field("arguments", text_or_empty(get(p, "arguments"))),
            )
        }
        _ => {}
    }
    let raw = get(p, "arguments");
    let args: Value = match raw {
        Value::String(text) => serde_json::from_str(if text.is_empty() { "{}" } else { text })
            .unwrap_or_else(|_| raw.clone()),
        Value::Null => Value::Object(Default::default()),
        other if !truthy(other) => Value::Object(Default::default()),
        other => other.clone(),
    };
    // Leave out settings such as yield_time_ms; keep text, lists and objects.
    let fields: Vec<(String, Value)> = match &args {
        Value::Object(o) => o
            .iter()
            .filter(|(_, v)| !(v.is_boolean() || v.is_number() || v.is_null()))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
        other => vec![("input".into(), other.clone())],
    };
    call_text(&name_of(get(p, "name"), "tool"), &fields)
}

fn codex_output(p: &Value) -> String {
    let mut out = get(p, "output").clone();
    if out.is_object() {
        let content = py::or(get(&out, "content"), get(&out, "output"));
        out = if truthy(content) {
            content.clone()
        } else {
            Value::String(py::dumps_compact(&out))
        };
    }
    if let Value::String(text) = &out {
        if let Ok(parsed) = serde_json::from_str::<Value>(text) {
            if parsed.as_object().is_some_and(|o| o.contains_key("output")) {
                out = get(&parsed, "output").clone();
            }
        }
    }
    if let Value::Array(items) = &out {
        let joined: String = items
            .iter()
            .filter(|c| c.is_object())
            .map(|c| {
                if truthy(get(c, "text")) {
                    py::str_of(get(c, "text"))
                } else {
                    String::new()
                }
            })
            .collect();
        out = Value::String(if joined.is_empty() {
            py::dumps_compact(&out)
        } else {
            joined
        });
    }
    let mut text = if truthy(&out) {
        py::str_of(&out)
    } else {
        String::new()
    };
    // exec_command puts a header before the command's output; keep the exit code from it.
    if let Some(m) = text.find("\nOutput:\n") {
        let header = &text[..m];
        // re.search(r"exited with code (-?\d+)"): the first such phrase followed by a number.
        let code = header
            .match_indices("exited with code ")
            .find_map(|(i, key)| {
                let rest = &header[i + key.len()..];
                let sign = if rest.starts_with('-') { "-" } else { "" };
                let digits: String = rest[sign.len()..]
                    .chars()
                    .take_while(char::is_ascii_digit)
                    .collect();
                (!digits.is_empty()).then(|| format!("{sign}{digits}"))
            });
        let prefix = code
            .filter(|c| c != "0")
            .map_or(String::new(), |c| format!("(exit code {c})\n"));
        text = prefix + &text[m + "\nOutput:\n".len()..];
    }
    text
}

/// The lines of a file as Python reads a text file: universal newlines, bad UTF-8 replaced.
fn text_lines(path: &str) -> std::io::Result<Vec<String>> {
    let bytes = std::fs::read(path)?;
    let text = String::from_utf8_lossy(&bytes);
    let mut lines = Vec::new();
    let mut start = 0;
    let b = text.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\n' || b[i] == b'\r' {
            lines.push(text[start..i].to_string());
            if b[i] == b'\r' && b.get(i + 1) == Some(&b'\n') {
                i += 1;
            }
            start = i + 1;
        }
        i += 1;
    }
    if start < b.len() {
        lines.push(text[start..].to_string());
    }
    Ok(lines)
}

const CODEX_CALLS: [&str; 6] = [
    "function_call",
    "custom_tool_call",
    "local_shell_call",
    "web_search_call",
    "image_generation_call",
    "tool_search_call",
];

/// A Codex session as a conversation: its turns as text, and what it was.
pub fn read_codex(path: &str, home: &str) -> std::io::Result<Conversation> {
    let (mut meta, mut model, mut turns, mut calls) = (Value::Null, Value::Null, Vec::new(), 0);
    for line in text_lines(path)? {
        let Ok(record) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let p = if record.is_object() {
            get(&record, "payload")
        } else {
            py::null()
        };
        if !p.is_object() {
            continue;
        }
        let (kind, when) = (s(get(&record, "type")), get(&record, "timestamp"));
        let ptype = s(get(p, "type"));
        if kind == Some("session_meta") && meta.is_null() {
            meta = p.clone();
        } else if kind == Some("turn_context") {
            if truthy(get(p, "model")) {
                model = get(p, "model").clone();
            }
        } else if kind == Some("response_item") && ptype == Some("message") {
            let text = codex_text(get(p, "content"));
            let role = s(get(p, "role"));
            if role == Some("user") && !facts::is_codex_harness(&text) {
                let text = if py::lstrip(&text).starts_with("<user_shell_command>") {
                    clip(&text, RESULT_CLIP)
                } else {
                    text
                };
                add_turn(&mut turns, "user", &text, when);
            } else if role == Some("assistant") {
                add_turn(&mut turns, "assistant", &text, when);
            }
        } else if kind == Some("response_item") && ptype.is_some_and(|t| CODEX_CALLS.contains(&t)) {
            add_turn(&mut turns, "assistant", &codex_call_text(p), when);
            calls += 1;
        } else if kind == Some("response_item") && py::str_of(get(p, "type")).ends_with("_output") {
            add_turn(
                &mut turns,
                "assistant",
                &result_text(&codex_output(p)),
                when,
            );
        }
        // Reasoning and compaction summaries are encrypted, so they're left out.
    }
    let id = if truthy(get(&meta, "id")) {
        py::str_of(get(&meta, "id"))
    } else {
        paths::codex_file_id(path).map_or("?".into(), String::from)
    };
    let summary = facts::read_codex_session(path, &facts::codex_names(home))
        .ok()
        .flatten();
    let git = get(&meta, "git");
    Ok(Conversation {
        agent: Agent::Codex,
        id,
        cwd: s(get(&meta, "cwd")).map(String::from),
        model,
        turns,
        calls,
        title: summary.map(|s| s.title),
        started: get(&meta, "timestamp").clone(),
        branch: if git.is_object() {
            get(git, "branch").clone()
        } else {
            Value::Null
        },
        reasoning: "Codex's reasoning",
        left_out: "Codex's reasoning, images, goals and subagent sessions",
    })
}

fn claude_result(content: &Value) -> String {
    match content {
        Value::Array(blocks) => blocks
            .iter()
            .filter(|b| b.is_object())
            .map(|b| {
                if s(get(b, "type")) == Some("text") {
                    if truthy(get(b, "text")) {
                        py::str_of(get(b, "text"))
                    } else {
                        String::new()
                    }
                } else {
                    format!("[{}]", py::str_of(get(b, "type")))
                }
            })
            .collect::<Vec<_>>()
            .join("\n"),
        Value::String(text) => text.clone(),
        other => py::dumps_compact(other),
    }
}

/// A Claude Code session as a conversation. Follows the conversation back from its newest
/// message, so turns undone with /rewind and other branches are left out, and past
/// compactions to what came before them.
pub fn read_claude(path: &str) -> std::io::Result<Conversation> {
    let mut records: Vec<Value> = Vec::new();
    let mut by_id: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for line in text_lines(path)? {
        let Ok(record) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if record.is_object() {
            if truthy(get(&record, "uuid")) {
                by_id.insert(py::str_of(get(&record, "uuid")), records.len());
            }
            records.push(record);
        }
    }
    let is_message = |r: &Value| matches!(s(get(r, "type")), Some("user" | "assistant"));
    let mut node = records
        .iter()
        .rposition(|r| is_message(r) && !truthy(get(r, "isSidechain")));
    let mut chain: Vec<usize> = Vec::new();
    if let Some(n) = node {
        if !truthy(get(&records[n], "uuid")) {
            // No links to follow: the messages in file order.
            chain = (0..records.len())
                .rev()
                .filter(|&k| is_message(&records[k]))
                .collect();
            node = None;
        }
    }
    let mut seen = std::collections::HashSet::new();
    while let Some(n) = node {
        if !seen.insert(n) {
            break;
        }
        chain.push(n);
        let r = &records[n];
        let parent = py::or(get(r, "parentUuid"), get(r, "logicalParentUuid"));
        node = if truthy(parent) {
            by_id.get(&py::str_of(parent)).copied()
        } else {
            None
        };
    }
    let (mut turns, mut calls, mut model) = (Vec::new(), 0, Value::Null);
    for &k in chain.iter().rev() {
        let record = &records[k];
        if !is_message(record)
            || truthy(get(record, "isMeta"))
            || truthy(get(record, "isCompactSummary"))
            || truthy(get(record, "isSidechain"))
        {
            continue;
        }
        let message = get(record, "message");
        let (when, role) = (
            get(record, "timestamp"),
            s(get(record, "type")).unwrap_or(""),
        );
        if role == "assistant"
            && !get(message, "model").is_null()
            && s(get(message, "model")) != Some("<synthetic>")
        {
            model = get(message, "model").clone();
        }
        let content = get(message, "content");
        let blocks: Vec<Value> = match content {
            Value::String(text) => vec![serde_json::json!({"type": "text", "text": text})],
            Value::Array(items) => items.clone(),
            _ => Vec::new(),
        };
        for block in &blocks {
            let kind = if block.is_object() {
                s(get(block, "type"))
            } else {
                None
            };
            match kind {
                Some("text") => {
                    let text = if truthy(get(block, "text")) {
                        py::str_of(get(block, "text"))
                    } else {
                        String::new()
                    };
                    let lead = py::lstrip(&text);
                    if !(role == "user" && CLAUDE_HARNESS.iter().any(|h| lead.starts_with(h))) {
                        add_turn(&mut turns, role, &text, when);
                    }
                }
                Some(k @ ("tool_use" | "server_tool_use" | "mcp_tool_use")) => {
                    let fields: Vec<(String, Value)> = match get(block, "input") {
                        Value::Object(o) => o.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
                        other => vec![("input".into(), other.clone())],
                    };
                    add_turn(
                        &mut turns,
                        "assistant",
                        &call_text(&name_of(get(block, "name"), k), &fields),
                        when,
                    );
                    calls += 1;
                }
                Some(k) if k.ends_with("tool_result") => {
                    add_turn(
                        &mut turns,
                        "assistant",
                        &result_text(&claude_result(get(block, "content"))),
                        when,
                    );
                }
                Some(k @ ("image" | "document")) => {
                    add_turn(&mut turns, role, &format!("[{k}]"), when)
                }
                _ => {} // thinking and redacted_thinking are signed for Anthropic's API: left out
            }
        }
    }
    let messages: Vec<&Value> = chain
        .iter()
        .rev()
        .map(|&k| &records[k])
        .filter(|r| is_message(r))
        .collect();
    let newest = |kind: &str, key: &str| {
        records
            .iter()
            .rev()
            .find(|r| s(get(r, "type")) == Some(kind) && truthy(get(r, key)))
            .map(|r| get(r, key).clone())
    };
    let titles = [
        newest("custom-title", "customTitle"),
        newest("ai-title", "aiTitle"),
        newest("agent-name", "agentName"),
    ];
    let first_prompt = turns
        .iter()
        .filter(|t| t.0 == "user")
        .flat_map(|t| t.1.first())
        .next()
        .cloned();
    let summary = facts::read_session(path).ok().flatten(); // where to resume it from, as the session list says
    let title_text = titles
        .iter()
        .flatten()
        .next()
        .map(py::str_of)
        .or(first_prompt)
        .unwrap_or_default();
    let title = py::collapse(py::head(&title_text, 200));
    let name = ospath::basename(path);
    let first_with = |key: &str| {
        messages
            .iter()
            .find(|r| truthy(get(r, key)))
            .map(|r| get(r, key).clone())
    };
    Ok(Conversation {
        agent: Agent::Claude,
        id: name.strip_suffix(".jsonl").unwrap_or(&name).to_string(),
        cwd: summary
            .and_then(|s| s.cwd)
            .filter(|c| !c.is_empty())
            .or_else(|| first_with("cwd").map(|c| py::str_of(&c))),
        model,
        turns,
        calls,
        title: (!title.is_empty()).then_some(title),
        started: first_with("timestamp").unwrap_or(Value::Null),
        branch: messages
            .iter()
            .rev()
            .find(|r| truthy(get(r, "gitBranch")))
            .map_or(Value::Null, |r| get(r, "gitBranch").clone()),
        reasoning: "Claude's thinking",
        left_out: "Claude's thinking, images, plans, /rewind snapshots and subagent transcripts",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn follows_the_conversation_past_rewinds_and_compactions() {
        fn msg(uuid: &str, parent: Option<&str>, role: &str, text: &str) -> serde_json::Value {
            let content = if role == "user" {
                serde_json::json!(text)
            } else {
                serde_json::json!([{"type": "text", "text": text}])
            };
            serde_json::json!({"type": role, "uuid": uuid, "parentUuid": parent, "timestamp": "2026-10-17T21:58:36Z", "cwd": "/w",
                               "message": {"role": role, "content": content}})
        }
        let mut summary = msg("8", Some("7"), "user", "Summary of the above");
        summary["isCompactSummary"] = serde_json::json!(true);
        let records = [
            msg("1", None, "user", "first"),
            msg("2", Some("1"), "assistant", "one"),
            msg("3", Some("2"), "user", "undone by /rewind"),
            msg("4", Some("3"), "assistant", "dropped"),
            msg("5", Some("2"), "user", "second"),
            msg("6", Some("5"), "assistant", "two"),
            serde_json::json!({"type": "system", "subtype": "compact_boundary", "uuid": "7", "parentUuid": null, "logicalParentUuid": "6"}),
            summary,
            msg(
                "9",
                Some("8"),
                "user",
                "<command-name>/model</command-name>",
            ),
            msg("10", Some("9"), "user", "third"),
            msg("11", Some("10"), "assistant", "three"),
        ];
        let dir = std::env::temp_dir().join(format!("hatcheck-rewinds-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("aaaaaaaa-0000-4000-8000-000000000001.jsonl");
        std::fs::write(
            &path,
            records
                .iter()
                .map(|r| r.to_string() + "\n")
                .collect::<String>(),
        )
        .unwrap();
        let conv = read_claude(&path.to_string_lossy());
        std::fs::remove_dir_all(&dir).unwrap();
        let conv = conv.unwrap();
        let texts: Vec<(String, String)> = conv
            .turns
            .iter()
            .map(|(role, t, _)| (role.clone(), t.join(" ")))
            .collect();
        let want = [
            ("user", "first"),
            ("assistant", "one"),
            ("user", "second"),
            ("assistant", "two"),
            ("user", "third"),
            ("assistant", "three"),
        ];
        assert_eq!(texts, want.map(|(r, t)| (r.to_string(), t.to_string())));
        assert_eq!(conv.cwd.as_deref(), Some("/w"));
    }

    #[test]
    fn clips_as_move_does() {
        assert_eq!(clip("a\nb\nc", (2, 100)), "a\nb\n… (1 more line)");
        assert_eq!(clip("abcdef", (5, 3)), "abc …");
        assert_eq!(clip("\n\nabc\n", (5, 100)), "abc");
        assert_eq!(clip("1\n2\n3\n4", (1, 100)), "1\n… (3 more lines)");
    }

    #[test]
    fn a_brief_keeps_the_task_what_was_touched_and_the_last_turns() {
        let mut turns: Vec<Turn> = Vec::new();
        for n in 1..=8 {
            add_turn(&mut turns, "user", &format!("Prompt {n}"), &Value::Null);
            let call = call_text(
                "Edit",
                &[
                    ("file_path".into(), Value::String(format!("src/file{n}.rs"))),
                    (
                        "command".into(),
                        Value::String(format!("cargo test {n}\nmore")),
                    ),
                ],
            );
            add_turn(
                &mut turns,
                "assistant",
                &format!("Reply {n}\n\n{call}"),
                &Value::Null,
            );
        }
        let conv = Conversation {
            agent: Agent::Claude,
            id: "abc".into(),
            cwd: Some("/work".into()),
            model: Value::Null,
            turns,
            calls: 8,
            title: Some("Fix  the\nuploader".into()),
            started: Value::Null,
            branch: Value::Null,
            reasoning: "",
            left_out: "",
        };
        let (tail, cut) = brief_tail(&conv);
        assert_eq!((tail.len(), cut), (BRIEF_TURNS, false));
        assert_eq!(tail[0].1[0], "Prompt 6");
        let (files, commands) = touched(&conv.turns);
        assert_eq!(files.len(), 8);
        assert_eq!(files[0], "src/file1.rs");
        assert_eq!(commands[0], "cargo test 1"); // a command's first line
        let text = brief_text(&conv);
        assert!(text.starts_with("Handover brief for Claude Code session abc \"Fix the uploader\": 8 of the user's messages and 8 tool calls, of which the last 6 turns follow as they were."), "{text}");
        assert!(text.contains("## The task\n\nPrompt 1"));
        assert!(text.contains("- src/file8.rs"));
        assert!(text.contains("- `cargo test 8`"));
        assert!(!text.contains("more"));
        let briefed = brief_turns(&conv);
        assert_eq!(briefed.len(), BRIEF_TURNS + 1);
        assert_eq!(briefed[0].0, "user");
        assert!(briefed[BRIEF_TURNS].1[0].starts_with("Reply 8"));
        // A newest turn longer than the budget is cut to its latest parts, alone.
        let mut long = Conversation {
            turns: Vec::new(),
            ..conv
        };
        add_turn(&mut long.turns, "user", "Start", &Value::Null);
        add_turn(
            &mut long.turns,
            "assistant",
            &"x".repeat(BRIEF_CHARS),
            &Value::Null,
        );
        add_turn(&mut long.turns, "assistant", "the end", &Value::Null);
        let (tail, cut) = brief_tail(&long);
        assert_eq!((tail.len(), cut), (1, true));
        assert_eq!(tail[0].1.last().map(String::as_str), Some("the end"));
    }

    #[test]
    fn tool_calls_as_text() {
        let text = call_text(
            "Bash",
            &[
                ("command".into(), Value::String("ls".into())),
                ("n".into(), serde_json::json!(3)),
            ],
        );
        assert_eq!(
            text,
            "[external_agent_tool_call: Bash]\ncommand: ls\nn: 3\n[/external_agent_tool_call]"
        );
        let out = codex_output(
            &serde_json::json!({"output": "Chunk ID: 1\nProcess exited with code 2\nOutput:\nmain.py\n"}),
        );
        assert_eq!(out, "(exit code 2)\nmain.py\n");
        let out = codex_output(&serde_json::json!({"output": "{\"output\": \"Success\"}"}));
        assert_eq!(out, "Success");
    }
}
