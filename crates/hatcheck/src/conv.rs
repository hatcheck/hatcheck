//! A session as a conversation: your messages and the replies, with each tool call as a
//! block of text with its input and the start of its output, in the format Codex's /import
//! gives Claude Code's tool calls. hatcheck show prints it; hatcheck move writes it into the
//! other agent's format. The port of read_claude, read_codex and their helpers in move.py.

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
    pub agent: &'static str,
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
        agent: "codex",
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
        agent: "claude",
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
    fn clips_as_move_does() {
        assert_eq!(clip("a\nb\nc", (2, 100)), "a\nb\n… (1 more line)");
        assert_eq!(clip("abcdef", (5, 3)), "abc …");
        assert_eq!(clip("\n\nabc\n", (5, 100)), "abc");
        assert_eq!(clip("1\n2\n3\n4", (1, 100)), "1\n… (3 more lines)");
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
