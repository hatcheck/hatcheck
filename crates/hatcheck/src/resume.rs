//! hatcheck resume [SESSION] ...: carry on the newest session started from this folder, or the
//! one named, in this terminal, after showing which and asking; hat r for short. docs/commands.md's
//! `hatcheck resume` section says what it shows, asks and runs.

use crate::args::{Count, Parser};
use crate::move_cmd::{interactive, read_answer};
use crate::open_cmd::{self, Pick};
use crate::ospath;
use crate::out;
use crate::paths;
use crate::py;
use crate::times;
use crate::widths::truncate;

pub fn main(argv: &[String], prog: &str) -> i32 {
    let parser = Parser::new(prog, "Carry on the newest session started from this folder, or the one named, in this terminal: shows which, and asks first. hatcheck open opens a new terminal window instead.")
        .positional("session", Count::Optional, "the session ID, or the start of it (default: the newest session started from this folder)")
        .option(&["--from"], "source", "PROFILE", "the profile or home to take it from, when more than one has a copy")
        .flag(&["-y", "--yes"], "yes", "don't ask")
        .flag(&["--print"], "print", "print the cd and resume commands instead")
        .exclusive(&["yes", "print"])
        .flag(&["--full"], "full", "print CLAUDE_CONFIG_DIR=... claude rather than cl, CODEX_HOME=... codex rather than cx, and KIMI_CODE_HOME=... kimi rather than ck")
        .flag(&["--all"], "all", "without SESSION, also consider sessions from Claude Desktop, the SDK, the Codex app or codex exec");
    let args = match parser.parse(argv) {
        Ok(a) => a,
        Err(stop) => return stop.exit_code(),
    };
    out::setup_console();
    if out::capturing() {
        // hatcheck serve: the agent would take the engine's place.
        return out::fail(&format!(
            "{prog} runs the agent in this terminal, so the app can't run it"
        ));
    }
    let s = match open_cmd::pick(&Pick {
        session: args.value("session").filter(|s| !s.is_empty()),
        source: args.value("source").filter(|s| !s.is_empty()),
        all: args.flag("all"),
    }) {
        Ok(s) => s,
        Err(message) => return out::fail(&message),
    };
    let id8 = py::head(&s.id, 8).to_string();
    let cwd = match s
        .cwd
        .as_deref()
        .filter(|c| !c.is_empty() && ospath::isdir(c))
    {
        Some(c) => c.to_string(),
        None => {
            let shown = s
                .cwd
                .as_deref()
                .filter(|c| !c.is_empty())
                .map_or("?".to_string(), paths::shell_path);
            return out::fail(&format!(
                "Session {id8} ran in {shown}, which isn't there any more"
            ));
        }
    };
    let command = open_cmd::resume_line(&s, args.flag("full"));
    if args.flag("print") {
        out::println(&format!("cd {}", paths::shell_path(&cwd)));
        out::println(&command);
        return 0;
    }
    if let Err(message) = open_cmd::refuse_if_open(&s) {
        return out::fail(&message);
    }
    let title = truncate(if s.title.is_empty() { "?" } else { &s.title }, 60);
    let here = paths::shell_path(&cwd);
    if args.flag("yes") {
        out::println(&format!(
            "Resuming session {id8} \"{title}\" here, in {here}:\n  {command}"
        ));
        return open_cmd::run_here(&cwd, &command);
    }
    if !interactive() {
        return out::fail(&format!(
            "Say --yes when there's no terminal to ask at, or --print for the commands: {prog} --yes"
        ));
    }
    // What would run, and where it comes from: the agent, its profile, and how long since the
    // session was last active, as the list shows it; and the folder's binding when the session
    // is in another profile, as the list flags it.
    out::println(&format!(
        "Session {id8} \"{title}\" · {} in {} · {} ago",
        s.agent.name(),
        paths::shell_path(&s.profile),
        times::ago(times::now() - s.when)
    ));
    out::println(&format!("  {command}"));
    let bound = paths::bound_profile(Some(&cwd), s.agent, &paths::load_bindings())
        .filter(|b| !paths::same_folder(b, &s.profile));
    if let Some(bound) = bound {
        out::println(&format!(
            "  ⚠ folder bound to {}",
            paths::shell_path(&bound)
        ));
    }
    let answer = read_answer(&format!("Resume it here, in {here}? [Y/n] "));
    if answer.is_none() {
        out::println("");
    }
    match answer
        .as_deref()
        .map(|x| py::strip(x).to_lowercase())
        .as_deref()
    {
        Some("" | "y" | "yes") => open_cmd::run_here(&cwd, &command),
        _ => out::fail("Nothing started."),
    }
}
