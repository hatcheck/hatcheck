//! hatcheck doctor [--json]: every copy of hatcheck's commands on the PATH, grouped by how
//! each was installed, which one runs and how to remove the rest; every copy of claude, codex
//! and agy, with its version and where it came from; and the hooks hatcheck bind --hook added
//! that run a hatcheck that has gone. It changes nothing; the exit status says whether there's
//! something to fix. docs/install.md's `hatcheck doctor` section describes it.

use crate::agent::Agent;
use crate::args::Parser;
use crate::bind;
use crate::cli;
use crate::ospath;
use crate::out;
use crate::paths;
use crate::py;
use crate::usage;
use serde_json::{json, Value};
use std::time::Duration;

/// hatcheck's commands, as the installers put them on the PATH.
pub const COMMANDS: [&str; 7] = ["hatcheck", "hat", "cl", "cx", "ck", "claude-z", "claude-k"];
/// How long a program may take to say its version.
const VERSION_TIMEOUT: Duration = Duration::from_secs(10);

/// How a copy of hatcheck got onto the PATH.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Install {
    Uv,
    Pipx,
    Homebrew,
    Cargo,
    /// The shell or PowerShell installer of a release, or winget.
    Installer,
    Winget,
    /// install.sh or install.ps1 from a clone: links to its build.
    Clone,
    /// A copy of claude-z or claude-k that the claude-z repo's installers made.
    ClaudeZ,
    /// Not hatcheck's: another program of the same name.
    Other,
}

impl Install {
    fn label(self) -> &'static str {
        match self {
            Install::Uv => "uv",
            Install::Pipx => "pipx",
            Install::Homebrew => "Homebrew",
            Install::Cargo => "cargo",
            Install::Installer => "the release installer",
            Install::Winget => "winget",
            Install::Clone => "a build",
            Install::ClaudeZ => "the claude-z repo's installer",
            Install::Other => "another program",
        }
    }

    fn key(self) -> &'static str {
        match self {
            Install::Uv => "uv",
            Install::Pipx => "pipx",
            Install::Homebrew => "homebrew",
            Install::Cargo => "cargo",
            Install::Installer => "installer",
            Install::Winget => "winget",
            Install::Clone => "clone",
            Install::ClaudeZ => "claude-z",
            Install::Other => "other",
        }
    }
}

/// One program found on the PATH.
#[derive(Clone, Debug)]
struct Found {
    name: String,
    path: String,
    /// What a link points to, as written in it.
    link: Option<String>,
    /// The path, then each link followed, down to the program itself.
    chain: Vec<String>,
    /// Where the program itself is: the links followed.
    real: String,
    runnable: bool,
}

/// path, then what it links to, and so on down to a file that isn't a link.
fn chain_of(path: &str) -> Vec<String> {
    let mut chain = vec![path.to_string()];
    let mut current = path.to_string();
    for _ in 0..16 {
        let Ok(target) = std::fs::read_link(&current) else {
            break;
        };
        let target = target.to_string_lossy().into_owned();
        let next = if ospath::isabs(&target) {
            target
        } else {
            ospath::normpath(&ospath::join(&ospath::dirname(&current), &target))
        };
        if chain.contains(&next) {
            break;
        }
        chain.push(next.clone());
        current = next;
    }
    chain
}

fn found(name: &str, path: &str, runnable: bool) -> Found {
    let chain = chain_of(path);
    Found {
        name: name.to_string(),
        path: path.to_string(),
        link: std::fs::read_link(path)
            .ok()
            .map(|t| t.to_string_lossy().into_owned()),
        real: if runnable {
            ospath::realpath(path)
        } else {
            chain.last().cloned().unwrap_or_else(|| path.to_string())
        },
        chain,
        runnable,
    }
}

fn find(name: &str) -> Vec<Found> {
    ospath::which_all(name)
        .into_iter()
        .map(|(path, runnable)| found(name, &path, runnable))
        .collect()
}

/// The first line a program prints for --version, or None.
fn version_of(path: &str) -> Option<String> {
    let mut command = std::process::Command::new(path);
    command.arg("--version");
    let out = usage::output_within(command, VERSION_TIMEOUT).ok()?;
    let line = out.lines().next().map(str::trim).unwrap_or("");
    (!line.is_empty()).then(|| line.to_string())
}

/// The version a copy of hatcheck or hat prints, from "hatcheck 0.3.0".
fn hatcheck_version(path: &str) -> Option<String> {
    let line = version_of(path)?;
    let (name, version) = line.split_once(' ')?;
    (matches!(name, "hatcheck" | "hat") && !version.is_empty()).then(|| version.to_string())
}

/// The receipt the release installer leaves: ($XDG_CONFIG_HOME or ~/.config)/hatcheck/
/// hatcheck-receipt.json, %LOCALAPPDATA%\hatcheck on Windows.
fn receipt() -> Option<Value> {
    let home = paths::home();
    let mut places = Vec::new();
    if ospath::WINDOWS {
        let base = std::env::var("LOCALAPPDATA")
            .ok()
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| ospath::join_all(&home, &["AppData", "Local"]));
        places.push(ospath::join_all(
            &base,
            &["hatcheck", "hatcheck-receipt.json"],
        ));
    }
    if let Some(xdg) = std::env::var("XDG_CONFIG_HOME")
        .ok()
        .filter(|v| !v.is_empty())
    {
        places.push(ospath::join_all(
            &xdg,
            &["hatcheck", "hatcheck-receipt.json"],
        ));
    }
    places.push(ospath::join_all(
        &home,
        &[".config", "hatcheck", "hatcheck-receipt.json"],
    ));
    places
        .iter()
        .find_map(|p| std::fs::read_to_string(p).ok())
        .and_then(|t| serde_json::from_str(&t).ok())
}

/// A launcher install.ps1 made: the .exe it runs, from the .cmd file.
fn cmd_launcher_target(path: &str) -> Option<String> {
    if !path.to_lowercase().ends_with(".cmd") {
        return None;
    }
    let text = std::fs::read_to_string(path).ok()?;
    if !text.contains("made by hatcheck install.ps1")
        && !text.contains("made by claude-multi-account")
    {
        return None;
    }
    let line = text.lines().find(|l| l.contains(".exe"))?;
    let start = line.find('"')? + 1;
    let end = line[start..].find('"')? + start;
    Some(line[start..end].to_string())
}

/// Where a copy of one of hatcheck's commands came from, and the folder its install lives in
/// (the build or package folder, which groups the commands of one install): the first link in
/// the chain that says, since uv, pipx and Homebrew each link from a bin folder into their own.
fn classify(f: &Found, receipt: Option<&Value>) -> (Install, String) {
    let home = paths::home();
    let cargo_bin = std::env::var("CARGO_HOME")
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| ospath::join(&home, ".cargo"));
    let mut chain = f.chain.clone();
    if let Some(exe) = cmd_launcher_target(&f.path) {
        chain.push(exe);
    }
    for target in &chain {
        let lower = target.replace('\\', "/").to_lowercase();
        let folder = ospath::dirname(target);
        if lower.contains("/uv/tools/hatcheck/") {
            return (Install::Uv, folder);
        }
        if lower.contains("pipx/venvs/hatcheck/") {
            return (Install::Pipx, folder);
        }
        if lower.contains("/cellar/hatcheck/") || lower.contains("/linuxbrew/") {
            return (Install::Homebrew, folder);
        }
        if lower.contains("/winget/packages/hatcheck") || lower.contains("/winget/links/") {
            return (Install::Winget, folder);
        }
        if let Some(k) = lower.find("/target/") {
            if lower[k..].contains("/release/") || lower[k..].contains("/debug/") {
                return (Install::Clone, target[..k].to_string());
            }
        }
        if paths::same_folder(&folder, &ospath::join(&cargo_bin, "bin")) {
            return (Install::Cargo, folder);
        }
    }
    let folder = ospath::dirname(&f.real);
    if matches!(f.name.as_str(), "claude-z" | "claude-k") {
        if let Ok(text) = std::fs::read_to_string(&f.real) {
            if text.contains("CONFIG_DIR=\"$HOME/.config/claude-") {
                return (Install::ClaudeZ, folder);
            }
        }
    }
    if let Some(r) = receipt {
        let prefix = py::s(py::get(r, "install_prefix")).unwrap_or("");
        let listed = py::get(r, "binaries")
            .as_array()
            .is_some_and(|b| b.iter().any(|n| py::s(n) == Some(f.name.as_str())));
        if !prefix.is_empty() && listed && paths::same_folder(prefix, &ospath::dirname(&f.path)) {
            return (Install::Installer, folder);
        }
    }
    (Install::Other, folder)
}

/// One install of hatcheck: its kind, its folder, the commands of it on the PATH, its version.
#[derive(Clone, Debug)]
struct Group {
    install: Install,
    folder: String,
    /// The folder on the PATH its commands are in.
    bin: String,
    commands: Vec<String>,
    version: Option<String>,
    /// The commands of it that run: the first of their name on the PATH.
    runs: Vec<String>,
}

impl Group {
    fn remove_command(&self) -> Option<String> {
        let shell = paths::Shell::here();
        Some(match self.install {
            Install::Uv => "uv tool uninstall hatcheck".into(),
            Install::Pipx => "pipx uninstall hatcheck".into(),
            Install::Homebrew => "brew uninstall hatcheck".into(),
            Install::Cargo => "cargo uninstall hatcheck".into(),
            Install::Winget => "winget uninstall hatcheck.Hatcheck.CLI".into(),
            Install::Clone => {
                if paths::powershell() {
                    format!(
                        "cd {}; .\\install.ps1 -Uninstall",
                        shell.quote(&self.folder)
                    )
                } else {
                    format!(
                        "cd {} && ./install.sh --uninstall",
                        shell.shell_path(&self.folder)
                    )
                }
            }
            Install::Installer | Install::ClaudeZ => {
                let names: Vec<String> = self
                    .commands
                    .iter()
                    .map(|n| {
                        let name = if ospath::WINDOWS {
                            format!("{n}.exe")
                        } else {
                            n.clone()
                        };
                        shell.shell_path(&ospath::join(&self.bin, &name))
                    })
                    .collect();
                if paths::powershell() {
                    format!("Remove-Item {}", names.join(", "))
                } else {
                    format!("rm {}", names.join(" "))
                }
            }
            Install::Other => return None,
        })
    }
}

/// A folder as the text shows it: ~/..., or the path.
fn show(path: &str) -> String {
    paths::shell_path(path)
}

/// How a copy of an agent got onto the PATH, from its path and what it links to.
fn agent_origin(agent: Agent, f: &Found) -> &'static str {
    let lower = f.real.replace('\\', "/").to_lowercase();
    let first_line = std::fs::File::open(&f.real)
        .ok()
        .and_then(|file| {
            use std::io::Read;
            let mut head = [0u8; 64];
            let n = (&file).read(&mut head).ok()?;
            Some(String::from_utf8_lossy(&head[..n]).into_owned())
        })
        .unwrap_or_default();
    if lower.contains("/caskroom/") || lower.contains("/cellar/") || lower.contains("/linuxbrew/") {
        return "Homebrew";
    }
    if lower.contains("/winget/") {
        return "winget";
    }
    if lower.contains("/node_modules/") || first_line.contains("node") {
        return "npm";
    }
    match agent {
        Agent::Claude if lower.contains("/.claude/local/") => {
            "an old npm install in ~/.claude/local"
        }
        Agent::Claude if lower.contains("/.local/share/claude/versions/") => "the native installer",
        Agent::Codex if lower.contains("/.codex/packages/") => "the native installer",
        Agent::Agy if lower.contains("antigravity") && lower.contains(".app/") => {
            "the Antigravity app"
        }
        Agent::Agy if lower.ends_with("/.local/bin/agy") || lower.contains("/agy/bin/") => {
            "the agy installer"
        }
        Agent::Kimi if lower.contains("/.kimi-code/bin/") => "the Kimi Code installer",
        Agent::Zcode if lower.contains("zcode") && lower.contains(".app/") => "the ZCode app",
        _ => "",
    }
}

/// Whether ~/.zshrc or ~/.bashrc defines the command as a shell function, which wins over any
/// program: the files that do.
fn shell_functions(name: &str) -> Vec<String> {
    let home = paths::home();
    [".zshrc", ".bashrc"]
        .iter()
        .map(|rc| ospath::join(&home, rc))
        .filter(|rc| {
            std::fs::read_to_string(rc).is_ok_and(|text| {
                text.lines().any(|line| {
                    let l = line.trim_start();
                    let l = l.strip_prefix("function ").map_or(l, str::trim_start);
                    l.strip_prefix(name)
                        .is_some_and(|rest| rest.trim_start().starts_with("()"))
                })
            })
        })
        .collect()
}

/// A copy of an agent on the PATH: the program, its version, and where it came from.
type AgentCopy = (Found, Option<String>, &'static str);

/// What the doctor found.
struct Report {
    me: (String, Install, String),
    groups: Vec<Group>,
    /// The commands of the six with no copy on the PATH at all.
    missing: Vec<String>,
    others: Vec<Found>,
    functions: Vec<(String, String)>,
    agents: Vec<(Agent, Vec<AgentCopy>)>,
    hooks: Vec<(Agent, String, String, bool)>,
    problems: Vec<String>,
}

fn examine() -> Report {
    let receipt = receipt();
    let mut groups: Vec<Group> = Vec::new();
    let mut others: Vec<Found> = Vec::new();
    let mut functions = Vec::new();
    let mut missing: Vec<String> = Vec::new();
    for name in COMMANDS {
        let copies = find(name);
        if copies.is_empty() {
            missing.push(name.to_string());
        }
        for (k, f) in copies.into_iter().enumerate() {
            if !f.runnable {
                others.push(f);
                continue;
            }
            let (install, folder) = classify(&f, receipt.as_ref());
            let bin = ospath::dirname(&f.path);
            if install == Install::Other {
                // A program of the name beside hatcheck, or hatcheck's own from a folder nothing
                // marks: the latter says so when asked.
                let ours =
                    matches!(name, "hatcheck" | "hat") && hatcheck_version(&f.path).is_some();
                let sibling =
                    !ours && hatcheck_version(&ospath::join(&folder, "hatcheck")).is_some();
                if !ours && !sibling {
                    others.push(f);
                    continue;
                }
            }
            let install = if install == Install::Other {
                // hatcheck's, in a folder nothing marks: the release installer's, most likely.
                Install::Installer
            } else {
                install
            };
            let group = match groups
                .iter_mut()
                .find(|g| g.install == install && paths::same_folder(&g.folder, &folder))
            {
                Some(g) => g,
                None => {
                    groups.push(Group {
                        install,
                        folder: folder.clone(),
                        bin: bin.clone(),
                        commands: Vec::new(),
                        version: None,
                        runs: Vec::new(),
                    });
                    groups.last_mut().expect("just pushed")
                }
            };
            group.commands.push(name.to_string());
            if k == 0 {
                group.runs.push(name.to_string());
            }
            if group.version.is_none() && matches!(name, "hatcheck" | "hat") {
                group.version = hatcheck_version(&f.path);
            }
        }
        for rc in shell_functions(name) {
            functions.push((name.to_string(), rc));
        }
    }
    for g in groups.iter_mut().filter(|g| g.version.is_none()) {
        let exe = if ospath::WINDOWS {
            "hatcheck.exe"
        } else {
            "hatcheck"
        };
        g.version = hatcheck_version(&ospath::join(&g.folder, exe));
    }
    let exe = std::env::current_exe()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    let me_found = found("hatcheck", &exe, true);
    let (mine, my_folder) = classify(&me_found, receipt.as_ref());
    let mine = match mine {
        Install::Other => groups
            .iter()
            .find(|g| paths::same_folder(&g.folder, &my_folder))
            .map_or(Install::Installer, |g| g.install),
        found => found,
    };
    let mut agents = Vec::new();
    for agent in Agent::ALL {
        let copies: Vec<AgentCopy> = find(agent.program())
            .into_iter()
            .map(|f| {
                let version = if f.runnable {
                    version_of(&f.path)
                } else {
                    None
                };
                let origin = agent_origin(agent, &f);
                (f, version, origin)
            })
            .collect();
        agents.push((agent, copies));
    }
    let hooks: Vec<(Agent, String, String, bool)> = bind::hook_runs()
        .into_iter()
        .map(|(agent, folder, program)| {
            let there = ospath::which(&program).is_some();
            (agent, folder, program, there)
        })
        .collect();
    let mut problems = Vec::new();
    if !groups.is_empty() && !missing.is_empty() {
        problems.push(format!(
            "{} {} on your PATH; the installer adds {}",
            missing.join(" and "),
            if missing.len() == 1 {
                "isn't"
            } else {
                "aren't"
            },
            if missing.len() == 1 { "it" } else { "them" }
        ));
    }
    if groups.len() > 1 {
        problems.push(format!(
            "{} installs of hatcheck are on your PATH; keep one",
            groups.len()
        ));
    }
    for f in &others {
        if f.runnable {
            if find(&f.name)
                .first()
                .is_some_and(|first| first.path == f.path)
            {
                problems.push(format!(
                    "{} is {}, another program, which runs ahead of hatcheck's",
                    f.name,
                    show(&f.path)
                ));
            }
        } else {
            problems.push(format!(
                "{} is a link to a file that's gone; a shell may find it first",
                show(&f.path)
            ));
        }
    }
    for (_, copies) in &agents {
        if let Some((f, _, _)) = copies.first().filter(|(f, _, _)| !f.runnable) {
            problems.push(format!(
                "{} is a link to a file that's gone; a shell may find it first",
                show(&f.path)
            ));
        }
    }
    for (agent, folder, program, there) in &hooks {
        if !there {
            problems.push(format!(
                "the hook in {} ({}) runs {}, which is gone",
                show(folder),
                agent.name(),
                show(program)
            ));
        }
    }
    Report {
        me: (exe, mine, my_folder),
        groups,
        missing,
        others,
        functions,
        agents,
        hooks,
        problems,
    }
}

fn as_json(r: &Report) -> Value {
    let groups: Vec<Value> = r
        .groups
        .iter()
        .map(|g| {
            json!({"install": g.install.key(), "folder": ospath::abspath(&g.folder), "bin": ospath::abspath(&g.bin),
                   "version": g.version, "commands": g.commands, "runs": g.runs,
                   "remove": g.remove_command()})
        })
        .collect();
    let others: Vec<Value> = r
        .others
        .iter()
        .map(|f| json!({"command": f.name, "path": ospath::abspath(&f.path), "runnable": f.runnable}))
        .collect();
    let agents: Vec<Value> = r
        .agents
        .iter()
        .map(|(agent, copies)| {
            json!({"agent": agent.key(), "program": agent.program(),
                   "copies": copies.iter().enumerate().map(|(k, (f, version, origin))| json!({
                       "path": ospath::abspath(&f.path), "target": f.link.as_ref().map(|_| ospath::abspath(&f.real)),
                       "version": version, "origin": if origin.is_empty() { Value::Null } else { json!(origin) },
                       "runs": k == 0 && f.runnable, "runnable": f.runnable})).collect::<Vec<_>>()})
        })
        .collect();
    let hooks: Vec<Value> = r
        .hooks
        .iter()
        .map(|(agent, folder, program, there)| {
            json!({"agent": agent.key(), "profile": ospath::abspath(folder), "runs": program, "exists": there})
        })
        .collect();
    json!({
        "version": 1,
        "this": {"path": ospath::abspath(&r.me.0), "version": cli::version(), "install": r.me.1.key(), "folder": ospath::abspath(&r.me.2)},
        "installs": groups, "missing": r.missing, "others": others,
        "shell_functions": r.functions.iter().map(|(n, rc)| json!({"command": n, "file": ospath::abspath(rc)})).collect::<Vec<_>>(),
        "agents": agents, "hooks": hooks, "problems": r.problems,
    })
}

fn describe(r: &Report) {
    let paint = out::Paint::new();
    let where_ = match r.me.1 {
        Install::Clone => format!("built in {}", show(&r.me.2)),
        other => format!("installed with {}", other.label()),
    };
    out::heading(&format!(
        "This is hatcheck {}, {where_}: {}",
        cli::version(),
        show(&r.me.0)
    ));
    if r.groups.is_empty() {
        out::println(&paint.p("33", "None of hatcheck's commands is on your PATH."));
    } else {
        out::heading(&format!(
            "Install{} of hatcheck on your PATH, the first of each name running:",
            if r.groups.len() == 1 { "" } else { "s" }
        ));
    }
    let n = |names: &[String]| {
        if names.len() == COMMANDS.len() {
            "all seven commands".to_string()
        } else {
            names.join(", ")
        }
    };
    let lines: Vec<(String, String, String, String)> = r
        .groups
        .iter()
        .map(|g| {
            let place = match g.install {
                Install::Clone => format!("{} in {}", show(&g.bin), show(&g.folder)),
                _ if !paths::same_folder(&g.bin, &g.folder) => {
                    format!("{} -> {}", show(&g.bin), show(&g.folder))
                }
                _ => show(&g.bin),
            };
            let standing = if g.runs.len() == g.commands.len() {
                "runs".to_string()
            } else if g.runs.is_empty() {
                "shadowed".to_string()
            } else {
                format!("runs {}", g.runs.join(", "))
            };
            (
                format!("{}: {place}", g.install.label()),
                g.version.clone().unwrap_or_else(|| "?".into()),
                n(&g.commands),
                standing,
            )
        })
        .collect();
    let width = |k: usize| {
        lines
            .iter()
            .map(|l| [&l.0, &l.1, &l.2, &l.3][k].chars().count())
            .max()
            .unwrap_or(0)
    };
    let (w0, w1) = (width(0), width(1));
    for (k, (place, version, commands, standing)) in lines.iter().enumerate() {
        let standing = if standing == "shadowed" {
            paint.p("33", standing)
        } else {
            paint.p("32", standing)
        };
        out::println(&format!(
            "  {}. {place:<w0$}  {version:<w1$}  {commands}; {standing}",
            k + 1
        ));
    }
    if !r.groups.is_empty() {
        for name in &r.missing {
            out::println(&paint.p("33", &format!("  {name} isn't on your PATH.")));
        }
    }
    for g in r.groups.iter().filter(|g| g.runs.len() < g.commands.len()) {
        if let Some(command) = g.remove_command() {
            out::println(&format!(
                "  To remove {}'s: {}",
                g.install.label(),
                paint.command(&command)
            ));
        }
    }
    for f in &r.others {
        if f.runnable {
            out::println(&paint.p(
                "33",
                &format!("  {} is also {}, another program.", f.name, show(&f.path)),
            ));
        } else {
            out::println(&paint.error(&format!(
                "  {} is a link to a file that's gone.",
                show(&f.path)
            )));
        }
    }
    for (name, rc) in &r.functions {
        out::println(&paint.p(
            "33",
            &format!(
                "  {} defines {name}() itself, which wins over the command; both do the same.",
                show(rc)
            ),
        ));
    }
    out::heading("The agents:");
    for (agent, copies) in &r.agents {
        let label = format!("{:<16}", agent.name());
        if copies.is_empty() {
            let needs = agent
                .launcher()
                .map_or(String::new(), |l| format!("; {l} needs it"));
            out::println(&format!(
                "  {label} {}",
                paint.p("33", &format!("not on your PATH{needs}"))
            ));
            continue;
        }
        for (k, (f, version, origin)) in copies.iter().enumerate() {
            let link = f.link.as_ref().map_or(String::new(), |l| {
                format!(" -> {}", if ospath::isabs(l) { show(l) } else { l.clone() })
            });
            let mut notes: Vec<String> = Vec::new();
            if !origin.is_empty() {
                notes.push(origin.to_string());
            }
            if !f.runnable {
                notes.push("a link to a file that's gone".into());
            } else if k == 0 {
                let running = match agent.launcher() {
                    Some(l) => format!("runs, and {l} runs it"),
                    None => "runs".into(),
                };
                notes.push(paint.p("32", &running));
            } else {
                notes.push(paint.p("33", "shadowed"));
            }
            let line = format!(
                "  {} {:<24} {}{link}  ({})",
                if k == 0 {
                    label.clone()
                } else {
                    " ".repeat(16)
                },
                version
                    .as_deref()
                    .unwrap_or(if f.runnable { "?" } else { "" }),
                show(&f.path),
                notes.join(", ")
            );
            if f.runnable {
                out::println(&line);
            } else {
                out::println(&paint.error(&line));
            }
        }
    }
    if r.hooks.is_empty() {
        out::heading("Hooks from hatcheck bind --hook: none.");
    } else {
        out::heading("Hooks from hatcheck bind --hook:");
        for (agent, folder, program, there) in &r.hooks {
            let state = if *there {
                String::new()
            } else {
                format!(
                    "{}: run {} to point it at this one",
                    paint.error(", which is gone"),
                    paint.command("hatcheck bind --hook")
                )
            };
            out::println(&format!(
                "  {:<24} {} runs {}{}",
                show(folder),
                agent.name(),
                show(program),
                state
            ));
        }
    }
    if r.problems.is_empty() {
        out::success("Nothing to fix.");
    } else {
        out::warn(&format!("To fix: {}.", r.problems.join("; ")));
    }
}

pub fn main(argv: &[String], prog: &str) -> i32 {
    let parser = Parser::new(prog, "Check how hatcheck and the agents are installed: every copy on the PATH, which one runs, and how to remove the rest. Changes nothing.")
        .flag(&["--json"], "json", "print what it found as JSON");
    let args = match parser.parse(argv) {
        Ok(a) => a,
        Err(stop) => return stop.exit_code(),
    };
    out::setup_console();
    let report = examine();
    if args.flag("json") {
        out::println(&py::dumps(&as_json(&report)));
    } else {
        describe(&report);
    }
    i32::from(!report.problems.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installs_are_told_by_their_paths() {
        let found = |name: &str, real: &str| Found {
            name: name.into(),
            path: format!("/home/me/.local/bin/{name}"),
            link: Some(real.into()),
            chain: vec![format!("/home/me/.local/bin/{name}"), real.into()],
            real: real.into(),
            runnable: true,
        };
        let kind = |name: &str, real: &str| classify(&found(name, real), None).0;
        assert_eq!(
            kind(
                "hatcheck",
                "/home/me/.local/share/uv/tools/hatcheck/bin/hatcheck"
            ),
            Install::Uv
        );
        assert_eq!(
            kind("cl", "/home/me/.local/pipx/venvs/hatcheck/bin/cl"),
            Install::Pipx
        );
        assert_eq!(
            kind("hat", "/opt/homebrew/Cellar/hatcheck/0.3.0/bin/hat"),
            Install::Homebrew
        );
        assert_eq!(
            kind("cx", "/home/me/code/hatcheck/target/release/cx"),
            Install::Clone
        );
        assert_eq!(
            classify(
                &found("cx", "/home/me/code/hatcheck/target/release/cx"),
                None
            )
            .1,
            "/home/me/code/hatcheck"
        );
        assert_eq!(kind("hatcheck", "/usr/bin/hatcheck"), Install::Other);
        let receipt =
            json!({"install_prefix": "/home/me/.local/bin", "binaries": ["hatcheck", "hat"]});
        assert_eq!(
            classify(&found("hat", "/home/me/.local/bin/hat"), Some(&receipt)).0,
            Install::Installer
        );
        assert_eq!(
            classify(&found("cl", "/home/me/.local/bin/cl"), Some(&receipt)).0,
            Install::Other
        );
    }

    #[test]
    fn hook_programs_are_unquoted() {
        assert_eq!(
            bind::hook_program("'/a b/hatcheck' bind --check"),
            "/a b/hatcheck"
        );
        assert_eq!(
            bind::hook_program("\"C:\\x\\hatcheck.exe\" bind --check codex"),
            "C:\\x\\hatcheck.exe"
        );
        assert_eq!(bind::hook_program("hatcheck bind --check"), "hatcheck");
    }
}
