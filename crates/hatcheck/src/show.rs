//! hatcheck show SESSION [--from PROFILE] [--last N] [--json]: a session's conversation,
//! read-only: your messages and the replies, with each tool call as a block of text.

use crate::agent::Agent;
use crate::args::{Count, Parser};
use crate::conv;
use crate::open_cmd::{pick, Pick};
use crate::ospath;
use crate::out;
use crate::paths;
use crate::py;
use crate::widths::truncate;
use serde_json::{json, Value};

pub fn main(argv: &[String], prog: &str) -> i32 {
    let parser = Parser::new(
        prog,
        "Show a Claude Code, Codex, Antigravity CLI, Kimi Code CLI, ZCode, DeepSeek Harness, MiniMax Code or Qwen Code session's conversation, read-only.",
    )
    .positional("session", Count::One, "the session ID, or the start of it")
    .option(
        &["--from"],
        "source",
        "PROFILE",
        "the profile or home to read it from, when more than one has a copy",
    )
    .option(&["--last"], "last", "N", "only the last N turns")
    .flag(&["--json"], "json", "print it as JSON");
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
    let conv = match s.agent {
        Agent::Codex => conv::read_codex(&s.path, &s.profile),
        Agent::Claude => conv::read_claude(&s.path),
        Agent::Agy => crate::agy::read_conversation(&s.profile, &s.id),
        Agent::Kimi => crate::kimi::read_conversation(&s.profile, &s.id),
        Agent::Zcode => crate::zcode::read_conversation(&s.profile, &s.id),
        Agent::Dsh => crate::dsh::read_conversation(&s.profile, &s.id),
        Agent::Mcode => crate::mcode::read_conversation(&s.profile, &s.id),
        Agent::Qwen => crate::qwen::read_conversation(&s.profile, &s.id),
    };
    let conv = match conv {
        Ok(c) => c,
        Err(e) => return out::fail(&e.to_string()),
    };
    let total = conv.turns.len();
    // turns[-N:] as Python slices it: the last N, all of them for 0, and from N on for -N.
    let turns: &[conv::Turn] = match last.filter(|n| *n != 0) {
        Some(n) if n > 0 => &conv.turns[total.saturating_sub(n as usize)..],
        Some(n) => &conv.turns[((-n) as usize).min(total)..],
        None => &conv.turns,
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
            "turns": turns.iter().map(|(role, texts, when)| json!({"role": role, "text": texts.join("\n\n"), "time": when})).collect::<Vec<_>>(),
        });
        out::println(&py::dumps(&data));
        return 0;
    }
    let assistant = s.agent.speaker();
    let title = truncate(if s.title.is_empty() { "?" } else { &s.title }, 60);
    let some = if turns.len() < total {
        format!(", the last {}", turns.len())
    } else {
        String::new()
    };
    let paint = out::Paint::new();
    out::heading(&format!(
        "Session {} \"{title}\" in {}, {total} turns{some}:",
        py::head(&s.id, 8),
        paths::shell_path(&s.profile)
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
