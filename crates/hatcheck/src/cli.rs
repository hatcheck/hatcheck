//! The hatcheck command: picks the subcommand and runs it, or the launcher it's called as.

use crate::{
    best, bind, desktop, doctor, move_cmd, open_cmd, out, provider, relocate, resume, sessions,
    show, usage, where_cmd,
};

pub const SUBCOMMANDS: &[&str] = &[
    "sessions", "usage", "where", "move", "fork", "open", "resume", "show", "best", "bind",
    "unbind", "desktop", "provider", "relocate", "doctor",
];

/// The letters two subcommands start with, each given to the one used more. The other takes two
/// letters, bi, un or do, or three, rel; sessions, the default, needs no word at all.
const SHARED_LETTERS: &[(&str, &str)] = &[
    ("b", "best"),
    ("s", "show"),
    ("u", "usage"),
    ("d", "desktop"),
    ("r", "resume"),
];

/// The subcommands a word could name: its whole name, a letter in SHARED_LETTERS, or else each
/// name the word is the start of, so hat m is hat move.
fn subcommands_named(word: &str) -> Vec<&'static str> {
    if let Some(&name) = SUBCOMMANDS.iter().find(|n| **n == word) {
        return vec![name];
    }
    if let Some(&(_, name)) = SHARED_LETTERS.iter().find(|(letter, _)| *letter == word) {
        return vec![name];
    }
    if word.is_empty() {
        return Vec::new();
    }
    SUBCOMMANDS
        .iter()
        .copied()
        .filter(|n| n.starts_with(word))
        .collect()
}

const HELP: &str = "hatcheck: Claude Code, Codex, Antigravity CLI, Kimi Code CLI and ZCode accounts
on one computer, each kept apart. Lists recent sessions with the command that
resumes each one and what's left of each plan, and carries sessions between
accounts and agents.

Usage:
    hatcheck [sessions] [HOURS] ...    recent sessions in every profile and home, with
                                       resume commands and plan usage (the default)
    hat check ...                      the same as hat (or hatcheck): the name in two words
    hatcheck usage ...                 plan usage left for every profile and home
    hatcheck where SESSION             where a session is stored, by its ID or the start of it
    hatcheck move [SESSION] [PROFILE]  move a session to another profile or home, or convert
                                       it for another of the agents; with nothing, the
                                       newest session started here, to this folder's
                                       Claude Code profile, after asking
    hatcheck fork [SESSION] [PROFILE]  fork a session into another profile or home: a new
                                       session with its own ID, and the original stays
    hatcheck relocate OLD NEW          bring the sessions of a project folder that was moved
                                       or renamed along to its new place
    hatcheck open [SESSION] ...        resume a session in a new terminal, in its folder
    hatcheck resume [SESSION]          carry on the newest session started here, in this
                                       terminal, after asking; hat r for short
    hatcheck show SESSION              a session's conversation, read-only
    hatcheck best [--for SESSION]      which account to use now, or to carry a session to;
                                       --wait sleeps until one has room
    hatcheck bind PROFILE ...          tie a folder to a profile, for cl, cx, ck and the list
    hatcheck unbind [FOLDER]           untie it again
    hatcheck desktop ...               Claude Desktop instances on macOS: list, open, make one
    hatcheck provider ...              the Z.ai and Kimi keys and models behind claude-z,
                                       claude-k, cl -z and cl -k
    hatcheck doctor                    every copy of hatcheck and the agents on your PATH,
                                       which runs, and how to remove the rest

hatcheck, usage, where, show, desktop, move, fork, relocate and doctor take --json,
for scripts, status bars and the desktop app.

`hatcheck SUBCOMMAND --help` lists a subcommand's options, and hatcheck --version
says which version this is. hat is the short name: hat usage is hatcheck usage,
and so on. cl, cx and ck run Claude Code, Codex and Kimi Code CLI with a profile.

A subcommand can be cut to its first letters: hat m is hat move, hat f is hat
fork. Where two share a letter, it goes to the one used more: b is best, s is
show, u is usage, d is desktop and r is resume, so bind, unbind and doctor take
two letters, bi, un and do, and relocate three, rel.";

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

/// Run hatcheck with argv (without the program's name), called as argv0: as a launcher when
/// that's cl, cx, ck, claude-z or claude-k.
pub fn main(argv0: &str, argv: Vec<String>) -> i32 {
    run(argv0, argv, true)
}

/// Run a command for hatcheck serve: never as a launcher, which would put the agent in the
/// engine's place, so --as names it in messages only, as Python's --as does.
pub fn served(argv: Vec<String>) -> i32 {
    run("hatcheck", argv, false)
}

fn run(argv0: &str, mut argv: Vec<String>, launchers: bool) -> i32 {
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
    if launchers && crate::launch::NAMES.contains(&name.as_str()) {
        return crate::launch::main(&name, argv); // a link named cl, or --as cl
    }
    let me = if name == "hat" { "hat" } else { "hatcheck" };
    if argv.first().is_some_and(|w| w == "check") {
        argv.remove(0); // hat check: the whole name again, so the same as hat
    }
    if argv.len() == 1 && argv[0] == "serve" {
        return crate::serve::run();
    }
    let named = argv.first().map_or(Vec::new(), |w| subcommands_named(w));
    let (command, prog) = match (named.as_slice(), argv.first().map(String::as_str)) {
        ([c], _) => {
            argv.remove(0); // m, or move: the whole name from here on, in messages too
            (c.to_string(), format!("{me} {c}"))
        }
        (_, Some("-h" | "--help" | "help")) => {
            if me == "hat" {
                out::println("hat is the short name for hatcheck.\n");
            }
            out::println(HELP);
            return 0;
        }
        (_, Some("-V" | "--version")) => {
            out::println(&format!("{me} {}", version()));
            return 0;
        }
        ([first @ .., last], Some(w)) => {
            out::eprint(&format!(
                "{me}: '{w}' could be {} or {last}; give more of it\n",
                first.join(", ")
            ));
            return 2;
        }
        (_, Some(w)) if !w.starts_with('-') && !is_hours(w) => {
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
        "move" => move_cmd::main(&argv, &prog),
        "fork" => move_cmd::fork_main(&argv, &prog),
        "relocate" => relocate::main(&argv, &prog),
        "open" => open_cmd::main(&argv, &prog),
        "resume" => resume::main(&argv, &prog),
        "show" => show::main(&argv, &prog),
        "desktop" => desktop::main(&argv, &prog),
        "best" => best::main(&argv, &prog),
        "bind" => bind::main(&argv, &prog),
        "unbind" => bind::unbind_main(&argv, &prog),
        "provider" => provider::main(&argv, &prog),
        "doctor" => doctor::main(&argv, &prog),
        other => {
            // Not reached: every name in SUBCOMMANDS has its arm above (a test checks).
            out::eprint(&format!(
                "{me}: there's no '{other}' subcommand; see {me} --help\n"
            ));
            2
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_subcommand_runs() {
        for name in SUBCOMMANDS {
            let argv = vec![name.to_string(), "--help".into()];
            let (code, help, _) = out::captured(|| run("hatcheck", argv, false));
            assert_eq!(code, 0, "{name}");
            assert!(
                help.starts_with(&format!("usage: hatcheck {name}")),
                "{name}: {help}"
            );
        }
    }

    #[test]
    fn a_subcommand_can_be_cut_to_its_first_letters() {
        for name in SUBCOMMANDS {
            // A start that's one subcommand's alone names it; re, which relocate and resume
            // share, names both, so hat re is asked for more.
            for n in 2..=name.len() {
                let start = &name[..n];
                let alone = SUBCOMMANDS.iter().filter(|s| s.starts_with(start)).count() == 1;
                let named = subcommands_named(start);
                if alone {
                    assert_eq!(named, [*name], "{start}");
                } else {
                    assert!(named.len() > 1 && named.contains(name), "{start}");
                }
            }
            assert_eq!(subcommands_named(&name[..1]).len(), 1, "{name}");
        }
        assert_eq!(subcommands_named("re"), ["resume", "relocate"]);
        for (letter, name) in [
            ("b", "best"),
            ("s", "show"),
            ("u", "usage"),
            ("m", "move"),
            ("f", "fork"),
            ("w", "where"),
            ("o", "open"),
            ("d", "desktop"),
            ("p", "provider"),
            ("r", "resume"),
            ("res", "resume"),
            ("rel", "relocate"),
        ] {
            assert_eq!(subcommands_named(letter), [name], "{letter}");
        }
        assert!(subcommands_named("x").is_empty());
        assert!(subcommands_named("").is_empty());
        let argv = vec!["m".to_string(), "--help".into()];
        let (code, help, _) = out::captured(|| run("hat", argv, false));
        assert_eq!(code, 0);
        assert!(help.starts_with("usage: hat move"), "{help}");
    }
}
