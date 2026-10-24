//! cl, cx, ck, claude-z and claude-k: Claude Code with a profile's folder (or on Z.ai or
//! Kimi), Codex with a home's, and Kimi Code CLI with a home's:
//!
//! ```text
//! cl [-z|-k] <profile> [claude args...]    Claude Code with ~/.claude-<profile>
//! claude-z [claude args...]                Claude Code on Z.ai, with ~/.claude or
//! claude-k [claude args...]                CLAUDE_CONFIG_DIR (claude-k: Kimi);
//!                                          claude-z reconfig sets up the key
//! cx <profile> [codex args...]             Codex with ~/.codex-<profile>
//! ck <profile> [kimi args...]              Kimi Code CLI with ~/.kimi-code-<profile>
//! ```
//!
//! Without a profile name, in a folder tied to one with hatcheck bind, they use that one.
//! The hatcheck binary runs as one of them when it's called by its name (a link named cl)
//! or with --as cl; the cl, cx, ck, claude-z and claude-k binaries are only this.

use crate::agent::Agent;
use crate::{ospath, out, paths, provider, py};

/// The launchers' names.
pub const NAMES: [&str; 5] = ["cl", "cx", "ck", "claude-z", "claude-k"];

/// Codex's own commands, which cx doesn't take for a home's name unless ~/.codex-<it> exists.
const CODEX_COMMANDS: [&str; 29] = [
    "agents",
    "exec",
    "e",
    "review",
    "login",
    "logout",
    "mcp",
    "plugin",
    "app-server",
    "remote-control",
    "app",
    "completion",
    "update",
    "doctor",
    "sandbox",
    "debug",
    "apply",
    "a",
    "resume",
    "queue",
    "archive",
    "delete",
    "migrate-rollouts",
    "unarchive",
    "fork",
    "cloud",
    "exec-server",
    "features",
    "help",
];

/// Kimi's own commands, which ck doesn't take for a home's name unless ~/.kimi-code-<it>
/// exists (kimi 2.2.0).
const KIMI_COMMANDS: [&str; 18] = [
    "export",
    "fork",
    "provider",
    "session",
    "acp",
    "web",
    "server",
    "rc",
    "remote",
    "login",
    "doctor",
    "vis",
    "install-desktop",
    "app",
    "migrate",
    "upgrade",
    "update",
    "help",
];

/// What changes in the agent's environment, in order: (name, value), or None to unset it.
type Changes = Vec<(String, Option<String>)>;

/// Run the launcher called name with args (without the program's name): its exit code, when
/// the agent didn't take this process's place.
pub fn main(name: &str, args: Vec<String>) -> i32 {
    out::setup_console();
    match name {
        "cx" => cx(args),
        "ck" => ck(args),
        "claude-z" => cl(args, "claude-z", Some("z")),
        "claude-k" => cl(args, "claude-k", Some("k")),
        _ => cl(args, "cl", None),
    }
}

/// The name this folder is bound to with hatcheck bind, for claude or codex.
fn bound(agent: Agent) -> Option<String> {
    let bindings = paths::load_bindings();
    let folder = paths::bound_folder(Some(&ospath::cwd()), &bindings)?;
    let name = py::s(py::get(bindings.get(&folder)?, agent.key()))?;
    (!name.is_empty()).then(|| name.to_string())
}

/// Point the agent at a bound name: default for its own folder, a path, or ~/<base>-NAME.
fn use_bound(changes: &mut Changes, variable: &str, base: &str, name: &str) {
    let value = if name == "default" {
        None
    } else if name.contains('/') || name.contains(ospath::SEP) {
        Some(name.to_string())
    } else {
        Some(ospath::join(&paths::home(), &format!("{base}-{name}")))
    };
    changes.push((variable.to_string(), value));
}

/// Run the agent with args, in place of this process where the system allows.
fn run(me: &str, program: &str, args: &[String], changes: &Changes) -> i32 {
    let Some(path) = ospath::which(program) else {
        let what = match program {
            "codex" => "Codex",
            "kimi" => "Kimi Code CLI",
            _ => "Claude Code",
        };
        return out::fail(&format!(
            "{me}: {program} isn't on your PATH; install {what} first"
        ));
    };
    let mut command = std::process::Command::new(&path);
    command.args(args);
    for (name, value) in changes {
        match value {
            Some(v) => command.env(name, v),
            None => command.env_remove(name),
        };
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let e = command.arg0(program).exec(); // returns only when it couldn't start
        out::fail(&format!("{me}: couldn't run {path}: {e}"))
    }
    #[cfg(not(unix))]
    {
        // No exec on Windows: run it and leave Ctrl+C to the agent.
        #[cfg(windows)]
        // SAFETY: no handler, only the flag that has this process ignore Ctrl+C.
        unsafe {
            windows_sys::Win32::System::Console::SetConsoleCtrlHandler(None, 1);
        }
        match command.status() {
            Ok(status) => status.code().unwrap_or(1),
            Err(e) => out::fail(&format!("{me}: couldn't run {path}: {e}")),
        }
    }
}

/// cl [-z|-k] <profile> [claude args...], and claude-z and claude-k (provider z or k).
fn cl(mut args: Vec<String>, me: &str, mut provider: Option<&'static str>) -> i32 {
    let mut changes = Changes::new();
    if let Some(key) = provider {
        if args.first().is_some_and(|a| a == "reconfig") {
            return provider::setup(provider::provider(key).expect("z and k are providers"));
        }
    } else {
        if matches!(args.first().map(String::as_str), Some("-z" | "-k")) {
            provider = Some(if args.remove(0) == "-z" { "z" } else { "k" });
        }
        if args
            .first()
            .is_none_or(|a| a.is_empty() || a.starts_with('-'))
        {
            let Some(name) = bound(Agent::Claude) else {
                out::error(
                    "usage: cl [-z|-k] <profile> [claude args...]  (uses ~/.claude-<profile>)\n   \
                     or tie this folder to a profile with: hatcheck bind <profile>\n",
                );
                return 2;
            };
            use_bound(&mut changes, "CLAUDE_CONFIG_DIR", ".claude", &name);
            out::eprint(&format!("cl: this folder is bound to {name}\n"));
        } else {
            // Spelled as ./cl spells it: Claude Code files the login under this string.
            let name = args.remove(0);
            let dir = ospath::join(&paths::home(), &format!(".claude-{name}"));
            changes.push(("CLAUDE_CONFIG_DIR".into(), Some(dir)));
        }
    }
    if let Some(key) = provider {
        let p = provider::provider(key).expect("z and k are providers");
        let path = provider::config_path(p);
        let Some(values) = provider::read_config(&path).0 else {
            return out::fail(&format!(
                "{} isn't set up yet. Run: claude-{key} reconfig",
                p.name
            ));
        };
        let api_key = values
            .get(&format!("{}API_KEY", p.prefix))
            .and_then(serde_json::Value::as_str)
            .map(py::strip)
            .unwrap_or("");
        if api_key.is_empty() {
            return out::fail(&format!(
                "No {} API key in {}. Run: claude-{key} reconfig",
                p.name,
                provider::shown(&path)
            ));
        }
        for (name, value) in provider::environment(p, &values) {
            changes.push((name.to_string(), value));
        }
    }
    run(me, "claude", &args, &changes)
}

/// cx <profile> [codex args...]
fn cx(mut args: Vec<String>) -> i32 {
    let mut changes = Changes::new();
    let home = paths::home();
    let first = args.first().cloned().unwrap_or_default();
    let named = !first.is_empty()
        && !first.starts_with('-')
        && (!CODEX_COMMANDS.contains(&first.as_str())
            || ospath::isdir(&ospath::join(&home, &format!(".codex-{first}"))));
    match if named { None } else { bound(Agent::Codex) } {
        Some(name) => {
            use_bound(&mut changes, "CODEX_HOME", ".codex", &name);
            out::eprint(&format!("cx: this folder is bound to {name}\n"));
        }
        None => {
            if !named {
                if !first.is_empty() && !first.starts_with('-') {
                    // a Codex command, with no home of that name
                    out::error(&format!(
                        "cx: {first} is a Codex command, not a home's name; run it with a home \
                         (cx <profile> {first} ...)\n"
                    ));
                }
                out::error(
                    "usage: cx <profile> [codex args...]  (uses ~/.codex-<profile>)\n   \
                     or tie this folder to a home with: hatcheck bind <profile>\n",
                );
                return 2;
            }
            let name = args.remove(0);
            let dir = ospath::join(&home, &format!(".codex-{name}"));
            changes.push(("CODEX_HOME".into(), Some(dir)));
        }
    }
    if ospath::which("codex").is_none() {
        return out::fail("cx: codex isn't on your PATH; install Codex first");
    }
    // The CODEX_HOME it runs with: the one set here, or the one it was given.
    let codex_home = match changes.iter().rev().find(|(k, _)| k == "CODEX_HOME") {
        Some((_, value)) => value.clone(),
        None => std::env::var("CODEX_HOME").ok(),
    };
    if let Some(dir) = codex_home.filter(|d| !d.is_empty()) {
        // Codex won't start with a CODEX_HOME that doesn't exist.
        if let Err(e) = std::fs::create_dir_all(&dir) {
            return out::fail(&format!("cx: couldn't make {dir}: {e}"));
        }
    }
    run("cx", "codex", &args, &changes)
}

/// ck <profile> [kimi args...]: Kimi Code CLI with ~/.kimi-code-<profile>, as cx runs Codex
/// with a home. The folder is made first, so a new home starts with `ck <name> login`.
fn ck(mut args: Vec<String>) -> i32 {
    let mut changes = Changes::new();
    let home = paths::home();
    let base = Agent::Kimi.base();
    let first = args.first().cloned().unwrap_or_default();
    let named = !first.is_empty()
        && !first.starts_with('-')
        && (!KIMI_COMMANDS.contains(&first.as_str())
            || ospath::isdir(&ospath::join(&home, &format!("{base}-{first}"))));
    match if named { None } else { bound(Agent::Kimi) } {
        Some(name) => {
            use_bound(&mut changes, "KIMI_CODE_HOME", base, &name);
            out::eprint(&format!("ck: this folder is bound to {name}\n"));
        }
        None => {
            if !named {
                if !first.is_empty() && !first.starts_with('-') {
                    // a Kimi command, with no home of that name
                    out::error(&format!(
                        "ck: {first} is a Kimi command, not a home's name; run it with a home \
                         (ck <profile> {first} ...)\n"
                    ));
                }
                out::error(
                    "usage: ck <profile> [kimi args...]  (uses ~/.kimi-code-<profile>)\n   \
                     or tie this folder to a home with: hatcheck bind <profile>\n",
                );
                return 2;
            }
            let name = args.remove(0);
            let dir = ospath::join(&home, &format!("{base}-{name}"));
            changes.push(("KIMI_CODE_HOME".into(), Some(dir)));
        }
    }
    if ospath::which("kimi").is_none() {
        return out::fail("ck: kimi isn't on your PATH; install Kimi Code CLI first");
    }
    // The KIMI_CODE_HOME it runs with: the one set here, or the one it was given.
    let kimi_home = match changes.iter().rev().find(|(k, _)| k == "KIMI_CODE_HOME") {
        Some((_, value)) => value.clone(),
        None => std::env::var("KIMI_CODE_HOME").ok(),
    };
    if let Some(dir) = kimi_home.filter(|d| !d.is_empty()) {
        if let Err(e) = std::fs::create_dir_all(&dir) {
            return out::fail(&format!("ck: couldn't make {dir}: {e}"));
        }
    }
    run("ck", "kimi", &args, &changes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_commands_as_cx_lists_them() {
        assert_eq!(CODEX_COMMANDS.len(), 29);
        assert!(CODEX_COMMANDS.contains(&"resume") && !CODEX_COMMANDS.contains(&"work"));
        assert_eq!(KIMI_COMMANDS.len(), 18);
        assert!(KIMI_COMMANDS.contains(&"login") && !KIMI_COMMANDS.contains(&"work"));
    }
}
