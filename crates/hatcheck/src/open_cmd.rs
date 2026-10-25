//! hatcheck open [SESSION] ...: resume a session in a new terminal window, already in its
//! folder, or open DSH's web UI there and choose the session; here (--here), or print the commands
//! (--print); --login PROFILE signs in to a profile or home instead. docs/commands.md's
//! `hatcheck open` section says which terminal opens and how.

use crate::agent::Agent;
use crate::args::{Count, Parser};
use crate::facts::Summary;
use crate::find;
use crate::ospath;
use crate::out;
use crate::paths;
use crate::py::{self, get, truthy};
use crate::sessions;
use crate::widths::truncate;
use std::process::{Command, Stdio};

/// What the picker was asked for: open's and show's arguments.
pub struct Pick<'a> {
    pub session: Option<&'a str>,
    pub source: Option<&'a str>,
    pub all: bool,
}

/// The session to open: by its ID or the start of it, or the newest started from this folder.
pub fn pick(args: &Pick) -> Result<Summary, String> {
    let Some(prefix) = args.session else {
        return find::newest_here(&ospath::cwd(), args.all).ok_or_else(|| {
            "No session was started from this folder. Give its ID (hatcheck open <id>), or list them with: hatcheck --all-folders".to_string()
        });
    };
    let mut copies = find::session_copies(prefix);
    if copies.is_empty() {
        return Err(format!("No session starting with {prefix}"));
    }
    if copies.len() > 1 {
        copies.sort_by(|a, b| a.0.cmp(&b.0));
        let lines: Vec<String> = copies
            .iter()
            .map(|((_, sid), places)| {
                format!(
                    "  {sid}  {}",
                    places
                        .iter()
                        .map(|(p, _)| paths::shell_path(p))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            })
            .collect();
        let mut shown: Vec<String> = lines.iter().take(10).cloned().collect();
        if lines.len() > 10 {
            shown.push("  …".into());
        }
        return Err(format!(
            "{prefix} matches {} sessions; give more of the ID:\n{}",
            copies.len(),
            shown.join("\n")
        ));
    }
    let ((agent, sid), mut places) = copies.remove(0);
    if let Some(source) = args.source {
        let (_, folder) = find::place(source);
        let mine: Vec<(String, String)> = places
            .iter()
            .filter(|(p, _)| paths::same_folder(p, &folder))
            .cloned()
            .collect();
        if mine.is_empty() {
            return Err(format!(
                "Session {} isn't in {}; it's in {}",
                py::head(&sid, 8),
                paths::shell_path(&folder),
                places
                    .iter()
                    .map(|(p, _)| paths::shell_path(p))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        places = mine;
    }
    let found: Vec<Summary> = places
        .iter()
        .filter_map(|(p, f)| crate::facts::summarize(agent, p, f))
        .collect();
    crate::facts::save_cache();
    let best = found.iter().fold(None::<&Summary>, |best, s| match best {
        Some(b) if (s.when, s.canonical, s.size) > (b.when, b.canonical, b.size) => Some(s),
        Some(b) => Some(b),
        None => Some(s),
    });
    best.cloned().ok_or_else(|| {
        format!(
            "Session {} holds no conversation to resume",
            py::head(&sid, 8)
        )
    })
}

/// Refuse a session that's open in a terminal, so it isn't resumed twice.
pub(crate) fn refuse_if_open(s: &Summary) -> Result<(), String> {
    let id8 = py::head(&s.id, 8);
    match s.agent {
        Agent::Codex => {
            return if find::codex_session_open(&s.profile, &s.id) {
                Err(format!(
                    "Codex session {id8} is open in codex; switch to that terminal, or quit it first"
                ))
            } else {
                Ok(())
            };
        }
        Agent::Agy => {
            return if crate::agy::session_open(&s.profile, &s.id) {
                Err(format!(
                    "Antigravity session {id8} is open in {}; switch to it, or quit it first",
                    crate::agy::holder(&s.profile)
                ))
            } else {
                Ok(())
            };
        }
        Agent::Dsh => {
            return if crate::dsh::session_open(&s.path) {
                Err(format!(
                    "DeepSeek Harness session {id8} is open in dsh; switch to it, or close it first"
                ))
            } else {
                Ok(())
            };
        }
        Agent::Mcode => {
            return if crate::mcode::session_open(&s.profile, &s.id) {
                Err(format!(
                    "MiniMax Code session {id8} is open or has an active writer; close it or let that work finish first"
                ))
            } else {
                Ok(())
            };
        }
        Agent::Qwen => {
            return if crate::qwen::session_open(&s.profile, &s.path, &s.id) {
                Err(format!(
                    "Qwen Code session {id8} is open in qwen; switch to it, or quit it first"
                ))
            } else {
                Ok(())
            };
        }
        Agent::Kimi | Agent::Zcode => return Ok(()), // nothing marks one of theirs open
        Agent::Claude => {}
    }
    let live = sessions::open_sessions(std::slice::from_ref(&s.profile));
    if let Some(entry) = live.get(&(ospath::realpath(&s.profile), s.id.clone())) {
        let status = get(entry, "status");
        let status = if truthy(status) {
            format!(" ({})", py::str_of(status))
        } else {
            String::new()
        };
        return Err(format!(
            "Session {id8} is open in pid {}{status}; switch to that terminal, or quit it first",
            py::str_of(get(entry, "pid"))
        ));
    }
    Ok(())
}

/// The command that resumes it, as the session list prints it: the full form when cl or cx
/// isn't on the PATH, so a new terminal can run it either way.
pub fn resume_line(s: &Summary, full: bool) -> String {
    resume_line_in(&paths::Shell::here(), s, full)
}

fn resume_line_in(shell: &paths::Shell, s: &Summary, full: bool) -> String {
    sessions::session_command_in(shell, s, full || !launcher_on_path(s.agent))
}

/// How commands are written for a terminal: as for this shell, but in PowerShell's words on
/// Windows for a terminal that runs PowerShell (wt, powershell), whatever this shell is: from
/// Git Bash, CLAUDE_CONFIG_DIR=... claude means nothing to PowerShell.
fn shell_for(terminal: &str) -> paths::Shell {
    let shell = paths::Shell::here();
    if ospath::WINDOWS && runs_powershell(terminal) {
        shell.for_powershell()
    } else {
        shell
    }
}

/// A terminal's name as terminal_argv knows it: Terminal.app is terminal, wt.exe is wt.
fn terminal_key(name: &str) -> String {
    let mut key = ospath::basename(name).to_lowercase();
    if key.ends_with(".app") || key.ends_with(".exe") {
        key.truncate(key.len() - 4);
    }
    key
}

/// Whether a terminal runs the command it's given with PowerShell.
fn runs_powershell(terminal: &str) -> bool {
    matches!(
        terminal_key(terminal).as_str(),
        "wt" | "powershell" | "pwsh"
    )
}

/// Whether the agent's launcher (cl, cx) is on the PATH, so a printed command may use it.
fn launcher_on_path(agent: Agent) -> bool {
    agent
        .launcher()
        .is_some_and(|name| ospath::which(name).is_some())
}

fn applescript(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

/// The terminal to open: --terminal, HATCHECK_TERMINAL, or this system's usual one.
pub fn pick_terminal(choice: Option<&str>) -> Option<String> {
    let var = |n: &str| std::env::var(n).ok().filter(|v| !v.is_empty());
    if let Some(name) = choice
        .filter(|c| !c.is_empty())
        .map(String::from)
        .or_else(|| var("HATCHECK_TERMINAL"))
    {
        return Some(name);
    }
    if cfg!(target_os = "macos") {
        return Some(
            match var("TERM_PROGRAM").as_deref() {
                Some("iTerm.app") => "iterm",
                Some("ghostty") => "ghostty",
                Some("WezTerm") => "wezterm",
                _ => "terminal",
            }
            .into(),
        );
    }
    if ospath::WINDOWS {
        return Some(
            if ospath::which("wt").is_some() {
                "wt"
            } else {
                "powershell"
            }
            .into(),
        );
    }
    [
        var("TERMINAL"),
        Some("x-terminal-emulator".into()),
        Some("gnome-terminal".into()),
        Some("konsole".into()),
    ]
    .into_iter()
    .flatten()
    .find(|name| ospath::which(name).is_some())
}

/// How to open terminal name in cwd running command: (argv, wait). wait: the program hands
/// the work to the terminal and returns, so its exit code tells whether that worked. A shell
/// stays open in the folder afterwards.
pub fn terminal_argv(name: &str, cwd: &str, command: &str, shell: &str) -> (Vec<String>, bool) {
    let key = terminal_key(name);
    let v = |items: &[&str]| items.iter().map(|s| s.to_string()).collect::<Vec<String>>();
    let here = format!("cd {} && {command}", paths::sh_quote(cwd));
    match key.as_str() {
        "terminal" | "apple_terminal" => {
            return (
                v(&[
                    "osascript",
                    "-e",
                    "tell application \"Terminal\"",
                    "-e",
                    &format!("do script {}", applescript(&here)),
                    "-e",
                    "activate",
                    "-e",
                    "end tell",
                ]),
                true,
            );
        }
        "iterm" | "iterm2" => {
            return (
                v(&[
                    "osascript",
                    "-e",
                    "tell application \"iTerm\"",
                    "-e",
                    "create window with default profile",
                    "-e",
                    &format!(
                        "tell current session of current window to write text {}",
                        applescript(&here)
                    ),
                    "-e",
                    "activate",
                    "-e",
                    "end tell",
                ]),
                true,
            );
        }
        "wt" | "powershell" | "pwsh" => {
            let ps = if ospath::which("pwsh").is_some() {
                "pwsh"
            } else {
                "powershell"
            };
            if key == "wt" {
                // wt reads ; as the end of its own command, so it's escaped
                return (
                    v(&[
                        "wt",
                        "-d",
                        cwd,
                        ps,
                        "-NoExit",
                        "-Command",
                        &command.replace(';', r"\;"),
                    ]),
                    false,
                );
            }
            return (
                v(&[
                    ps,
                    "-NoExit",
                    "-Command",
                    &format!(
                        "Set-Location -LiteralPath '{}'; {command}",
                        cwd.replace('\'', "''")
                    ),
                ]),
                false,
            );
        }
        _ => {}
    }
    let stay = format!("{here}; exec {} -l", paths::sh_quote(shell));
    let run = v(&[shell, "-l", "-i", "-c", &stay]); // a login shell, so the PATH is the usual one
    let with = |head: &[&str]| {
        (
            v(head).into_iter().chain(run.clone()).collect::<Vec<_>>(),
            false,
        )
    };
    match key.as_str() {
        "ghostty" if cfg!(target_os = "macos") => {
            // Ghostty starts from its app bundle on macOS
            let (argv, _) = with(&[
                "open",
                "-na",
                "Ghostty.app",
                "--args",
                &format!("--working-directory={cwd}"),
                "-e",
            ]);
            (argv, true)
        }
        "wezterm" => with(&["wezterm", "start", "--cwd", cwd, "--"]),
        "gnome-terminal" => with(&[
            "gnome-terminal",
            &format!("--working-directory={cwd}"),
            "--",
        ]),
        "konsole" => with(&["konsole", "--workdir", cwd, "-e"]),
        "kitty" => with(&["kitty", "--directory", cwd]),
        _ => with(&[name, "-e"]), // xterm, alacritty, x-terminal-emulator and the like
    }
}

/// An OS error as Python's str() of it reads: [Errno 13] Permission denied: 'path'.
fn os_error(e: &std::io::Error, path: &str) -> String {
    let text = e.to_string();
    let text = text.split(" (os error").next().unwrap_or(&text).to_string();
    match e.raw_os_error() {
        Some(code) => format!("[Errno {code}] {text}: '{path}'"),
        None => text,
    }
}

/// Open the terminal: None when that worked, else why not.
pub fn open_terminal(name: &str, cwd: &str, command: &str) -> Option<String> {
    let shell = std::env::var("SHELL")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "/bin/sh".into());
    let (argv, wait) = terminal_argv(name, cwd, command, &shell);
    let mut cmd = Command::new(&argv[0]);
    cmd.args(&argv[1..]);
    let result = if wait {
        cmd.stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        match cmd.spawn() {
            Ok(child) => {
                let out = wait_with_timeout(child, std::time::Duration::from_secs(60));
                match out {
                    Some(out) if out.status.success() => Ok(None),
                    Some(out) => {
                        let err = String::from_utf8_lossy(if out.stderr.is_empty() {
                            &out.stdout
                        } else {
                            &out.stderr
                        })
                        .trim()
                        .to_string();
                        Ok(Some(if err.is_empty() {
                            format!(
                                "{} exited with {}",
                                argv[0],
                                out.status.code().unwrap_or(-1)
                            )
                        } else {
                            err
                        }))
                    }
                    None => Ok(Some(format!(
                        "Command '{argv:?}' timed out after 60 seconds"
                    ))),
                }
            }
            Err(e) => Err(e),
        }
    } else {
        cmd.current_dir(cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        detach(&mut cmd);
        cmd.spawn().map(|_| None)
    };
    match result {
        Ok(why) => why,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            Some(format!("{} isn't installed", argv[0]))
        }
        Err(e) => Some(os_error(&e, &argv[0])),
    }
}

fn wait_with_timeout(
    mut child: std::process::Child,
    timeout: std::time::Duration,
) -> Option<std::process::Output> {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return child.wait_with_output().ok(),
            Ok(None) if std::time::Instant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(20))
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}

/// Start the terminal on its own: a session of its own on POSIX, a console of its own on Windows.
fn detach(cmd: &mut Command) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // SAFETY: setsid is async-signal-safe and touches nothing but the new process.
        unsafe {
            cmd.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0000_0010); // CREATE_NEW_CONSOLE
    }
}

/// Resume it in this terminal: the same command, run from the session's folder.
pub(crate) fn run_here(cwd: &str, command: &str) -> i32 {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let e = Command::new("/bin/sh")
            .arg("-c")
            .arg(command)
            .current_dir(cwd)
            .exec();
        out::fail(&os_error(&e, "/bin/sh"))
    }
    #[cfg(not(unix))]
    {
        // No exec on Windows: run it and leave Ctrl+C to the agent.
        #[cfg(windows)]
        unsafe {
            windows_sys::Win32::System::Console::SetConsoleCtrlHandler(None, 1);
        }
        let argv: Vec<String> = if paths::powershell() {
            vec![
                if ospath::which("pwsh").is_some() {
                    "pwsh"
                } else {
                    "powershell"
                }
                .into(),
                "-NoProfile".into(),
                "-Command".into(),
                command.into(),
            ]
        } else {
            vec![
                std::env::var("SHELL").unwrap_or_else(|_| "sh".into()),
                "-c".into(),
                command.into(),
            ]
        };
        match Command::new(&argv[0])
            .args(&argv[1..])
            .current_dir(cwd)
            .status()
        {
            Ok(status) => status.code().unwrap_or(1),
            Err(e) => out::fail(&os_error(&e, &argv[0])),
        }
    }
}

/// --login: a terminal running cl, cx login, ck login, agy, zcode or dsh web, so signing in
/// stays with the agent. DSH's official web UI supplies its account flow; no login subcommand is invented.
fn login(profile: &str, here: bool, print: bool, full: bool, terminal: Option<&str>) -> i32 {
    let (agent, folder) = find::place(profile);
    let line = |shell: &paths::Shell| {
        let mut command = shell.start_command(agent, &folder, full || !launcher_on_path(agent));
        if matches!(agent, Agent::Codex | Agent::Kimi | Agent::Mcode) {
            command += " login";
        }
        command
    };
    let command = line(&paths::Shell::here());
    let cwd = paths::home();
    if print {
        out::println(&format!("cd {}", paths::shell_path(&cwd)));
        out::println(&command);
        return 0;
    }
    if matches!(agent, Agent::Codex | Agent::Kimi) {
        let _ = std::fs::create_dir_all(&folder); // Codex won't start with a CODEX_HOME that doesn't exist
    }
    if here {
        out::heading(&format!(
            "Signing in to {} here",
            paths::shell_path(&folder)
        ));
        return run_here(&cwd, &command);
    }
    let terminal = pick_terminal(terminal);
    let launched = terminal.as_deref().map(|t| line(&shell_for(t)));
    let why = match (&terminal, &launched) {
        (Some(t), Some(launched)) => open_terminal(t, &cwd, launched),
        _ => Some("no terminal found; set HATCHECK_TERMINAL".into()),
    };
    if let Some(why) = why {
        let command = out::Paint::stderr().command(&format!("  {command}"));
        return out::fail(&format!(
            "Couldn't open a terminal ({why}). Run this instead:\n{command}"
        ));
    }
    out::heading(&format!(
        "Signing in to {} in a new {} window:",
        paths::shell_path(&folder),
        ospath::basename(terminal.as_deref().unwrap_or(""))
    ));
    out::command(&format!("  {}", launched.unwrap_or(command)));
    0
}

pub fn main(argv: &[String], prog: &str) -> i32 {
    let parser = Parser::new(prog, "Resume a Claude Code, Codex, Antigravity CLI, Kimi Code CLI, ZCode, MiniMax Code or Qwen Code session in a new terminal window, already in its folder; for DeepSeek Harness, open dsh web there and choose the session in its session history.")
        .positional("session", Count::Optional, "the session ID, or the start of it (default: the newest session started from this folder)")
        .option(&["--from"], "source", "PROFILE", "the profile or home to take it from, when more than one has a copy")
        .flag(&["--here"], "here", "resume it in this terminal instead")
        .flag(&["--print"], "print", "print the cd and resume commands instead")
        .exclusive(&["here", "print"])
        .option(&["--terminal"], "terminal", "NAME", "the terminal to open (default: HATCHECK_TERMINAL, or this system's usual one)")
        .flag(&["--full"], "full", "print CLAUDE_CONFIG_DIR=... claude rather than cl, CODEX_HOME=... codex rather than cx, and KIMI_CODE_HOME=... kimi rather than ck")
        .flag(&["--all"], "all", "without SESSION, also consider sessions from Claude Desktop, the SDK, the Codex app or codex exec")
        .option(&["--login"], "login", "PROFILE", "sign in to a profile (NAME, default, codex, codex:NAME, agy, kimi, kimi:NAME, zcode, dsh, dsh:NAME, mcode, mcode:NAME, qwen or qwen:NAME) in a new terminal, instead of resuming a session");
    let args = match parser.parse(argv) {
        Ok(a) => a,
        Err(stop) => return stop.exit_code(),
    };
    let (here, print, full) = (args.flag("here"), args.flag("print"), args.flag("full"));
    if args.value("login").is_some_and(|l| !l.is_empty())
        && args.value("session").is_some_and(|s| !s.is_empty())
    {
        return parser
            .error("give a SESSION or --login, not both")
            .exit_code();
    }
    out::setup_console();
    if let Some(profile) = args.value("login").filter(|l| !l.is_empty()) {
        return login(profile, here, print, full, args.value("terminal"));
    }
    let s = match pick(&Pick {
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
    let command = resume_line(&s, full);
    if print {
        out::println(&format!("cd {}", paths::shell_path(&cwd)));
        out::println(&command);
        return 0;
    }
    if let Err(message) = refuse_if_open(&s) {
        return out::fail(&message);
    }
    let title = truncate(if s.title.is_empty() { "?" } else { &s.title }, 60);
    let opening = s.agent == Agent::Dsh;
    if here {
        out::heading(&if opening {
            format!("Opening DeepSeek Harness for session {id8} \"{title}\" here, in {}; choose it in DSH's web UI:", paths::shell_path(&cwd))
        } else {
            format!(
                "Resuming session {id8} \"{title}\" here, in {}",
                paths::shell_path(&cwd)
            )
        });
        return run_here(&cwd, &command);
    }
    let terminal = pick_terminal(args.value("terminal"));
    let launched = terminal
        .as_deref()
        .map(|t| resume_line_in(&shell_for(t), &s, full));
    let why = match (&terminal, &launched) {
        (Some(t), Some(launched)) => open_terminal(t, &cwd, launched),
        _ => Some("no terminal found; set HATCHECK_TERMINAL".into()),
    };
    if let Some(why) = why {
        let paint = out::Paint::stderr();
        let cd = paint.command(&format!("  cd {}", paths::shell_path(&cwd)));
        let command = paint.command(&format!("  {command}"));
        return out::fail(&format!(
            "Couldn't open a terminal ({why}). Run these instead:\n{cd}\n{command}"
        ));
    }
    out::heading(&if opening {
        format!("Opening DeepSeek Harness for session {id8} \"{title}\" in a new {} window, in {}; choose it in DSH's web UI:",
            ospath::basename(terminal.as_deref().unwrap_or("")), paths::shell_path(&cwd))
    } else {
        format!(
            "Resuming session {id8} \"{title}\" in a new {} window, in {}:",
            ospath::basename(terminal.as_deref().unwrap_or("")),
            paths::shell_path(&cwd)
        )
    });
    out::command(&format!("  {}", launched.unwrap_or(command)));
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminals_that_run_powershell() {
        for name in ["wt", "wt.exe", "powershell.exe", "pwsh"] {
            assert!(runs_powershell(name), "{name}");
        }
        for name in ["terminal", "Terminal.app", "iterm", "wezterm", "xterm"] {
            assert!(!runs_powershell(name), "{name}");
        }
    }

    #[test]
    fn terminal_commands() {
        let (cwd, command, shell) = ("/w/my app", "cl work --resume 1", "/bin/zsh");
        let stay = "cd '/w/my app' && cl work --resume 1; exec /bin/zsh -l";
        let (argv, wait) = terminal_argv("terminal", cwd, command, shell);
        assert_eq!((argv[0].as_str(), wait), ("osascript", true));
        assert!(argv.contains(&"do script \"cd '/w/my app' && cl work --resume 1\"".to_string()));
        let (argv, _) = terminal_argv("iTerm.app", cwd, command, shell);
        assert!(argv.contains(&"tell current session of current window to write text \"cd '/w/my app' && cl work --resume 1\"".to_string()));
        if cfg!(target_os = "macos") {
            assert_eq!(
                terminal_argv("ghostty", cwd, command, shell).0,
                [
                    "open",
                    "-na",
                    "Ghostty.app",
                    "--args",
                    "--working-directory=/w/my app",
                    "-e",
                    shell,
                    "-l",
                    "-i",
                    "-c",
                    stay
                ]
            );
        }
        assert_eq!(
            terminal_argv("wezterm", cwd, command, shell),
            (
                vec![
                    "wezterm".to_string(),
                    "start".into(),
                    "--cwd".into(),
                    cwd.into(),
                    "--".into(),
                    shell.into(),
                    "-l".into(),
                    "-i".into(),
                    "-c".into(),
                    stay.into()
                ],
                false
            )
        );
        assert_eq!(
            terminal_argv("gnome-terminal", cwd, command, shell).0[..3],
            ["gnome-terminal", "--working-directory=/w/my app", "--"]
        );
        assert_eq!(
            terminal_argv("konsole", cwd, command, shell).0[..4],
            ["konsole", "--workdir", cwd, "-e"]
        );
        assert_eq!(
            terminal_argv("xterm", cwd, command, shell).0,
            ["xterm", "-e", shell, "-l", "-i", "-c", stay]
        );
        let (argv, _) = terminal_argv("wt", "C:\\w", "$env:X=1; claude; $env:X=$null", shell);
        assert_eq!(argv[..3], ["wt", "-d", "C:\\w"]);
        assert_eq!(argv.last().unwrap(), r"$env:X=1\; claude\; $env:X=$null"); // wt's own separator, escaped
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn terminal_applescript_compiles() {
        let compiler = "/usr/bin/osacompile";
        if !std::path::Path::new(compiler).exists() {
            return;
        }
        let (argv, _) = terminal_argv("terminal", "/w/a \"b\"", "cl work --resume 1", "/bin/zsh");
        let dir = std::env::temp_dir().join(format!("hatcheck-osa-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut compile = Command::new(compiler);
        compile.arg("-o").arg(dir.join("x.scpt"));
        for (i, a) in argv.iter().enumerate() {
            if a == "-e" {
                compile.arg("-e").arg(&argv[i + 1]);
            }
        }
        let out = compile.output().unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}
