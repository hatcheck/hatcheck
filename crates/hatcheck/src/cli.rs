//! The hatcheck command: picks the subcommand and runs it, as hatcheck/cli.py does.

use crate::{out, sessions, usage, where_cmd};

pub const SUBCOMMANDS: &[&str] = &[
    "sessions", "usage", "where", "move", "open", "show", "best", "bind", "unbind", "desktop",
    "provider",
];
/// The subcommands the Rust port has so far; the rest are still Python's (hatcheck/).
pub const PORTED: &[&str] = &["sessions", "usage", "where"];

const HELP: &str =
    "hatcheck: Claude Code and Codex accounts on one computer, each kept apart. Lists
recent sessions with the command that resumes each one and what's left of each
plan, and carries sessions between accounts and agents.

Usage:
    hatcheck [sessions] [HOURS] ...    recent sessions in every profile and home, with
                                       resume commands and plan usage (the default)
    hat check ...                      the same as hat (or hatcheck): the name in two words
    hatcheck usage ...                 plan usage left for every profile and home
    hatcheck where SESSION             where a session is stored, by its ID or the start of it
    hatcheck move SESSION PROFILE ...  copy a session to another profile or home, or convert
                                       it between Claude Code and Codex
    hatcheck open [SESSION] ...        resume a session in a new terminal, in its folder
    hatcheck show SESSION              a session's conversation, read-only
    hatcheck best [--for SESSION]      which account to use now, or to carry a session to
    hatcheck bind PROFILE ...          tie a folder to a profile, for cl, cx and the list
    hatcheck unbind [FOLDER]           untie it again
    hatcheck desktop ...               Claude Desktop instances on macOS: list, open, make one
    hatcheck provider ...              the Z.ai and Kimi keys and models behind claude-z,
                                       claude-k, cl -z and cl -k

hatcheck, usage, where, show, desktop and move take --json, for scripts, status bars
and the desktop app.

`hatcheck SUBCOMMAND --help` lists a subcommand's options, and hatcheck --version
says which version this is. hat is the short name: hat usage is hatcheck usage,
and so on. cl and cx run Claude Code and Codex with a profile.";

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Whether a word is HOURS for the session list: 36, 1.5, 12h or 3d.
fn is_hours(word: &str) -> bool {
    let w = word.to_lowercase();
    let w = w.strip_suffix(['h', 'd']).unwrap_or(&w);
    let (whole, frac) = w.split_once('.').unwrap_or((w, ""));
    !whole.is_empty()
        && whole.bytes().all(|b| b.is_ascii_digit())
        && (w.contains('.') <= (!frac.is_empty() && frac.bytes().all(|b| b.is_ascii_digit())))
}

/// Run hatcheck with argv (without the program's name), called as argv0.
pub fn main(argv0: &str, mut argv: Vec<String>) -> i32 {
    // The name it was run by, for its messages: hat, the short name, or hatcheck.
    let base = crate::ospath::basename(argv0);
    let mut name = base
        .rsplit_once('.')
        .map_or(base.as_str(), |(stem, ext)| {
            if ext.eq_ignore_ascii_case("exe") {
                stem
            } else {
                base.as_str()
            }
        })
        .to_string();
    if argv.len() >= 2 && argv[0] == "--as" {
        name = argv[1].clone();
        argv.drain(..2);
    }
    let me = if name == "hat" { "hat" } else { "hatcheck" };
    if argv.first().is_some_and(|w| w == "check") {
        argv.remove(0); // hat check: the whole name again, so the same as hat
    }
    let (command, prog) = match argv.first().map(String::as_str) {
        Some(c) if SUBCOMMANDS.contains(&c) => {
            let c = argv.remove(0);
            let prog = format!("{me} {c}");
            (c, prog)
        }
        Some("-h" | "--help" | "help") => {
            if me == "hat" {
                out::println("hat is the short name for hatcheck.\n");
            }
            out::println(HELP);
            return 0;
        }
        Some("-V" | "--version") => {
            out::println(&format!("{me} {}", version()));
            return 0;
        }
        Some(w) if !w.starts_with('-') && !is_hours(w) => {
            out::eprint(&format!(
                "{me}: there's no '{w}' subcommand; see {me} --help\n"
            ));
            return 2;
        }
        _ => ("sessions".to_string(), me.to_string()),
    };
    match command.as_str() {
        "sessions" => sessions::main(&argv, &prog),
        "usage" => usage::main(&argv, &prog),
        "where" => where_cmd::main(&argv, &prog),
        other => {
            out::eprint(&format!(
                "{me} {other} isn't in the Rust port yet; the Python hatcheck has it\n"
            ));
            2
        }
    }
}
