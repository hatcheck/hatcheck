//! hatcheck show SESSION [--from PROFILE] [--last N] [--json] [--markdown]: a session's
//! conversation, read-only: your messages and the replies, with each tool call as a block of
//! text; --markdown writes it as a Markdown document, to keep or share.

use crate::args::{Count, Parser};
use crate::conv;
use crate::facts;
use crate::open_cmd::{pick, Pick};
use crate::ospath;
use crate::out;
use crate::paths;
use crate::py;
use crate::times;
use crate::widths::truncate;
use serde_json::{json, Value};

pub fn main(argv: &[String], prog: &str) -> i32 {
    let parser = Parser::new(
        prog,
        "Show a Claude Code, Codex, Antigravity CLI, Kimi Code CLI, ZCode, DeepSeek Harness, MiniMax Code, Qwen Code, GitHub Copilot CLI or OpenCode session's conversation, read-only.",
    )
    .positional("session", Count::One, "the session ID, or the start of it")
    .option(
        &["--from"],
        "source",
        "PROFILE",
        "the profile or home to read it from, when more than one has a copy",
    )
    .option(&["--last"], "last", "N", "only the last N turns")
    .flag(&["--brief"], "brief", "show the handover brief hatcheck move --brief hands over instead: the first prompt, the files and commands its tool calls named, and its last turns")
    .flag(&["-m", "--markdown"], "markdown", "print it as a Markdown document: a heading with the session's facts, then each turn under who said it and when, tool calls in code blocks")
    .flag(&["-j", "--json"], "json", "print it as JSON")
    .exclusive(&["markdown", "json"]);
    let args = match parser.parse(argv) {
        Ok(a) => a,
        Err(stop) => return stop.exit_code(),
    };
    let last = match args.value("last") {
        None => None,
        Some(text) => match py::int(text) {
            Some(n) => Some(n),
            None => {
                return parser
                    .error(&format!("argument --last: invalid int value: '{text}'"))
                    .exit_code()
            }
        },
    };
    out::setup_console();
    let s = match pick(&Pick {
        session: args.value("session"),
        source: args.value("source").filter(|s| !s.is_empty()),
        all: true,
    }) {
        Ok(s) => s,
        Err(message) => return out::fail(&message),
    };
    let conv = match conv::read_for(&s) {
        Ok(c) => c,
        Err(e) => return out::fail(&e.to_string()),
    };
    // With --brief, the handover brief's turns stand in for the conversation's.
    let brief = args.flag("brief");
    let shown: Vec<conv::Turn> = if brief {
        conv::brief_turns(&conv)
    } else {
        conv.turns.clone()
    };
    let total = shown.len();
    // turns[-N:] as Python slices it: the last N, all of them for 0, and from N on for -N.
    let turns: &[conv::Turn] = match last.filter(|n| *n != 0) {
        Some(n) if n > 0 => &shown[total.saturating_sub(n as usize)..],
        Some(n) => &shown[((-n) as usize).min(total)..],
        None => &shown,
    };
    if args.flag("json") {
        let title = if s.title.is_empty() {
            Value::Null
        } else {
            json!(s.title)
        };
        let cwd = conv.cwd.clone().or(s.cwd.clone());
        let data = json!({
            "version": 1, "agent": s.agent.key(), "id": s.id, "profile": ospath::abspath(&s.profile), "title": title,
            "cwd": cwd, "model": conv.model, "turns_total": total, "tool_calls": conv.calls, "left_out": conv.left_out,
            "brief": brief,
            "turns": turns.iter().map(|(role, texts, when)| json!({"role": role, "text": texts.join("\n\n"), "time": when})).collect::<Vec<_>>(),
        });
        out::println(&py::dumps(&data));
        return 0;
    }
    let assistant = s.agent.speaker();
    if args.flag("markdown") {
        out::print(&markdown(&s, &conv, turns, total, brief));
        return 0;
    }
    let title = truncate(if s.title.is_empty() { "?" } else { &s.title }, 60);
    let some = if turns.len() < total {
        format!(", the last {}", turns.len())
    } else {
        String::new()
    };
    let paint = out::Paint::new();
    out::heading(&format!(
        "Session {} \"{title}\" in {}, {total} turns{some}{}:",
        py::head(&s.id, 8),
        paths::shell_path(&s.profile),
        if brief { " as a handover brief" } else { "" }
    ));
    for (role, texts, when) in turns {
        let name = match role.as_str() {
            "user" => "You",
            "assistant" => assistant,
            other => other,
        };
        let at = match when {
            Value::String(w) if !w.is_empty() => {
                paint.muted(&format!("  ({})", py::head(w, 16).replace('T', " ")))
            }
            _ => String::new(),
        };
        out::println(&format!("\n{}{}", paint.accent(name), at));
        for line in py::splitlines(&texts.join("\n\n")) {
            out::println(&format!("  {line}"));
        }
    }
    out::println(&format!(
        "\n{}",
        paint.muted(&format!("Left out: {}.", conv.left_out))
    ));
    0
}

/// Whether a part of a turn is a tool call or its result, as conv writes them: shown in a
/// code block, so its lines aren't read as Markdown.
fn is_tool_block(text: &str) -> bool {
    let lead = py::lstrip(text);
    lead.starts_with("[external_agent_tool_call")
        || lead.starts_with("[external_agent_tool_result]")
}

/// A code fence longer than any run of backticks in the text, so the block closes where meant.
fn fence_for(text: &str) -> String {
    let longest = text.split(|c| c != '`').map(str::len).max().unwrap_or(0);
    "`".repeat(longest.max(2) + 1)
}

/// The conversation as a Markdown document: a heading with the session's facts, then each turn
/// under who said it and when, its tool calls and their results in code blocks.
pub fn markdown(
    s: &facts::Summary,
    conv: &conv::Conversation,
    turns: &[conv::Turn],
    total: usize,
    brief: bool,
) -> String {
    let title = if s.title.is_empty() {
        "Untitled session"
    } else {
        &s.title
    };
    let mut facts: Vec<String> = vec![format!(
        "{} session `{}` in `{}`",
        s.agent.name(),
        s.id,
        paths::shell_path(&s.profile)
    )];
    if let Some(cwd) = conv
        .cwd
        .as_deref()
        .or(s.cwd.as_deref())
        .filter(|c| !c.is_empty())
    {
        facts.push(format!("started from `{}`", paths::shell_path(cwd)));
    }
    if let Some(branch) = py::s(&s.branch).filter(|b| !b.is_empty()) {
        facts.push(format!("on branch `{branch}`"));
    }
    if let Some(model) = py::s(&conv.model).filter(|m| !m.is_empty()) {
        facts.push(format!("model `{model}`"));
    }
    let shown = if turns.len() < total {
        format!("the last {} of {total} turns", turns.len())
    } else {
        format!("{total} turn{}", if total == 1 { "" } else { "s" })
    };
    facts.push(format!(
        "{shown}, {} tool call{}",
        conv.calls,
        if conv.calls == 1 { "" } else { "s" }
    ));
    let mut text = format!("# {}\n\n{}.", py::collapse(title), facts.join(", "));
    if brief {
        text += " A handover brief, as hatcheck move --brief hands one over: the first prompt, the files and commands the tool calls named, and the last turns.";
    }
    text += &format!(
        " Exported by hatcheck {} on {}. Left out: {}.\n",
        crate::cli::version(),
        times::short_local(times::now()),
        conv.left_out
    );
    for (role, texts, when) in turns {
        let who = if role == "user" {
            "You"
        } else {
            s.agent.speaker()
        };
        let at = match when {
            Value::String(w) if !w.is_empty() => {
                format!(" · {}", py::head(w, 16).replace('T', " "))
            }
            _ => String::new(),
        };
        text += &format!("\n## {who}{at}\n");
        for part in texts {
            if is_tool_block(part) {
                let fence = fence_for(part);
                text += &format!("\n{fence}text\n{}\n{fence}\n", part.trim_matches('\n'));
            } else {
                text += &format!("\n{}\n", part.trim_matches('\n'));
            }
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_blocks_get_a_fence_longer_than_their_backticks() {
        assert!(is_tool_block(
            "[external_agent_tool_call: Bash]\ncommand: ls\n[/external_agent_tool_call]"
        ));
        assert!(is_tool_block(
            "  [external_agent_tool_result]\nok\n[/external_agent_tool_result]"
        ));
        assert!(!is_tool_block("Run `ls` first"));
        assert_eq!(fence_for("no backticks"), "```");
        assert_eq!(fence_for("has ``` inside"), "````");
        assert_eq!(fence_for("has `````` six"), "```````");
    }
}
