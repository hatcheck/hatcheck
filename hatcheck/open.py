#!/usr/bin/env python3
"""Resume a session in a new terminal window, already in its folder.

Usage:
    hatcheck open [SESSION] [--from PROFILE] [--here | --print] [--terminal NAME] [--full] [--all]
    hatcheck open --login PROFILE [--here | --print] [--terminal NAME]

  SESSION    : the session ID, or the start of it, of a Claude Code or Codex session.
               Without it, the newest session started from this folder, in any
               profile or home.
  --from     : the profile or home to take it from when more than one has a copy,
               named as hatcheck move names them (default: the copy used last).
  --here     : resume it in this terminal instead.
  --print    : print the cd and resume commands instead, for scripts.
  --terminal : the terminal to open: terminal, iterm, ghostty or wezterm on macOS;
               wt or powershell on Windows; gnome-terminal, konsole, kitty, or any
               other that takes -e, on Linux. HATCHECK_TERMINAL sets the default.
  --full     : the full command, CLAUDE_CONFIG_DIR=... claude, rather than cl or cx.
  --all      : without SESSION, also consider sessions last used from Claude
               Desktop, the SDK, the Codex app or codex exec.
  --login    : instead of a session, sign in to a profile or Codex home in a new
               terminal: cl NAME starts Claude Code there, which asks you to sign in
               the first time; codex:NAME runs cx NAME login. Named as hatcheck move
               names them: NAME, default, codex or codex:NAME.

The command is the one the session list prints: claude or cl <name>, codex or cx
<name>, and claude-z, claude-k, cl -z or cl -k for a session whose last reply came
from Z.ai or Kimi. A session that's open in a terminal is refused, so it isn't
resumed twice.

The terminal: on macOS, Terminal, or the one this runs in when that's iTerm2,
Ghostty or WezTerm (from TERM_PROGRAM). Driving Terminal or iTerm2 needs macOS's
permission to control it, which it asks for once. On Windows, Windows Terminal
(wt), else a new PowerShell window. On Linux, $TERMINAL, else
x-terminal-emulator, GNOME Terminal or Konsole. A shell stays open in the folder
after the session ends.

This script loads sessions.py and move.py, so keep them in one folder.
"""
import argparse
import importlib.util
import os
import shlex
import shutil
import signal
import subprocess
import sys

# TERM_PROGRAM -> the terminal to open on macOS, when hatcheck runs in it.
MAC_TERMINALS = {"iTerm.app": "iterm", "ghostty": "ghostty", "WezTerm": "wezterm"}


def load_sibling(name, file):
    sys.dont_write_bytecode = True  # leave no __pycache__ folder beside the scripts
    path = os.path.join(os.path.dirname(os.path.realpath(__file__)), file)
    if name in sys.modules:  # loaded once per process: hatcheck serve runs many commands in one
        return sys.modules[name]
    spec = importlib.util.spec_from_file_location(name, path)
    module = sys.modules[name] = importlib.util.module_from_spec(spec)
    try:
        spec.loader.exec_module(module)
    except BaseException:
        del sys.modules[name]
        raise
    return module


def pick(args, sessions, move):
    """The session to open, as a summary from sessions.summarize."""
    show = sessions.shell_path
    if not args.session:
        s = sessions.newest_here(os.getcwd(), args.all)
        if not s:
            sys.exit("No session was started from this folder. Give its ID (hatcheck open <id>), "
                     "or list them with: hatcheck --all-folders")
        return s
    copies = sessions.session_copies(args.session)
    if not copies:
        sys.exit("No session starting with %s" % args.session)
    if len(copies) > 1:
        lines = ["  %s  %s" % (sid, ", ".join(show(p) for p, _ in where)) for (_, sid), where in sorted(copies.items())]
        sys.exit("%s matches %d sessions; give more of the ID:\n%s"
                 % (args.session, len(copies), "\n".join(lines[:10] + ["  …"] * (len(lines) > 10))))
    ((agent, sid), where), = copies.items()
    if args.source:
        _, folder = move.place(args.source)
        mine = [(p, f) for p, f in where if sessions.same_folder(p, folder)]
        if not mine:
            sys.exit("Session %s isn't in %s; it's in %s" % (sid[:8], show(folder), ", ".join(show(p) for p, _ in where)))
        where = mine
    found = [s for s in (sessions.summarize(agent, p, f) for p, f in where) if s]
    if not found:
        sys.exit("Session %s holds no conversation to resume" % sid[:8])
    return max(found, key=lambda s: (s["when"], s.get("canonical", True), s["size"]))


def refuse_if_open(s, sessions):
    if s["agent"] == "codex":
        if sessions.codex_session_open(s["profile"], s["id"]):
            sys.exit("Codex session %s is open in codex; switch to that terminal, or quit it first" % s["id"][:8])
        return
    live = sessions.open_sessions([s["profile"]]).get((os.path.realpath(s["profile"]), s["id"]))
    if live:
        sys.exit("Session %s is open in pid %s%s; switch to that terminal, or quit it first"
                 % (s["id"][:8], live.get("pid"), " (%s)" % live["status"] if live.get("status") else ""))


def resume_line(s, sessions, full):
    """The command that resumes it, as the session list prints it: the full form when cl
    or cx isn't on the PATH, so a new terminal can run it either way."""
    shortcut = sessions.CODEX_SHORTCUT if s["agent"] == "codex" else sessions.SHORTCUT
    return sessions.session_command(s, full or not shutil.which(shortcut))


def applescript(text):
    return '"%s"' % text.replace("\\", "\\\\").replace('"', '\\"')


def pick_terminal(args):
    """The terminal to open: --terminal, HATCHECK_TERMINAL, or this system's usual one."""
    name = args.terminal or os.environ.get("HATCHECK_TERMINAL")
    if name:
        return name
    if sys.platform == "darwin":
        return MAC_TERMINALS.get(os.environ.get("TERM_PROGRAM", ""), "terminal")
    if os.name == "nt":
        return "wt" if shutil.which("wt") else "powershell"
    for name in (os.environ.get("TERMINAL"), "x-terminal-emulator", "gnome-terminal", "konsole"):
        if name and shutil.which(name):
            return name
    return None


def terminal_argv(name, cwd, command, shell):
    """How to open terminal `name` in cwd running command: (argv, wait). wait: the
    program hands the work to the terminal and returns, so its exit code tells whether
    that worked. A shell stays open in the folder afterwards."""
    key = os.path.basename(name).lower()
    if key.endswith((".app", ".exe")):
        key = key[:-4]
    if key in ("terminal", "apple_terminal"):
        return ["osascript", "-e", 'tell application "Terminal"',
                "-e", "do script " + applescript("cd %s && %s" % (shlex.quote(cwd), command)),
                "-e", "activate", "-e", "end tell"], True
    if key in ("iterm", "iterm2"):
        return ["osascript", "-e", 'tell application "iTerm"', "-e", "create window with default profile",
                "-e", "tell current session of current window to write text "
                + applescript("cd %s && %s" % (shlex.quote(cwd), command)),
                "-e", "activate", "-e", "end tell"], True
    if key in ("wt", "powershell", "pwsh"):
        ps = "pwsh" if shutil.which("pwsh") else "powershell"
        if key == "wt":  # wt reads ; as the end of its own command, so it's escaped
            return ["wt", "-d", cwd, ps, "-NoExit", "-Command", command.replace(";", "\\;")], False
        return [ps, "-NoExit", "-Command", "Set-Location -LiteralPath '%s'; %s" % (cwd.replace("'", "''"), command)], False
    stay = "cd %s && %s; exec %s -l" % (shlex.quote(cwd), command, shlex.quote(shell))
    run = [shell, "-l", "-i", "-c", stay]  # a login shell, so the PATH is the usual one
    if key == "ghostty" and sys.platform == "darwin":  # Ghostty starts from its app bundle on macOS
        return ["open", "-na", "Ghostty.app", "--args", "--working-directory=" + cwd, "-e"] + run, True
    if key == "wezterm":
        return ["wezterm", "start", "--cwd", cwd, "--"] + run, False
    if key == "gnome-terminal":
        return ["gnome-terminal", "--working-directory=" + cwd, "--"] + run, False
    if key == "konsole":
        return ["konsole", "--workdir", cwd, "-e"] + run, False
    if key == "kitty":
        return ["kitty", "--directory", cwd] + run, False
    return [name, "-e"] + run, False  # xterm, alacritty, x-terminal-emulator and the like


def open_terminal(name, cwd, command):
    """Open the terminal; None when that worked, else why not."""
    shell = os.environ.get("SHELL") or "/bin/sh"
    argv, wait = terminal_argv(name, cwd, command, shell)
    try:
        if wait:
            p = subprocess.run(argv, capture_output=True, text=True, timeout=60)
            if p.returncode != 0:
                return (p.stderr or p.stdout).strip() or "%s exited with %d" % (argv[0], p.returncode)
            return None
        flags = {"creationflags": getattr(subprocess, "CREATE_NEW_CONSOLE", 0)} if os.name == "nt" \
            else {"start_new_session": True}
        subprocess.Popen(argv, cwd=cwd, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                         stderr=subprocess.DEVNULL, **flags)
        return None
    except FileNotFoundError:
        return "%s isn't installed" % argv[0]
    except (OSError, subprocess.TimeoutExpired) as e:
        return str(e)


def run_here(cwd, command, sessions):
    """Resume it in this terminal: the same command, run from the session's folder."""
    os.chdir(cwd)
    sys.stdout.flush()
    if os.name == "nt":  # no exec on Windows: run it and leave Ctrl+C to the agent
        signal.signal(signal.SIGINT, signal.SIG_IGN)
        if sessions.powershell():
            argv = ["pwsh" if shutil.which("pwsh") else "powershell", "-NoProfile", "-Command", command]
        else:
            argv = [os.environ.get("SHELL") or "sh", "-c", command]
        sys.exit(subprocess.call(argv))
    os.execv("/bin/sh", ["/bin/sh", "-c", command])


def login(args, sessions, move):
    """--login: a terminal in your home folder running cl NAME (or claude), or cx NAME login
    (or codex login), so signing in stays with Claude Code and Codex."""
    agent, folder = move.place(args.login)
    shortcut = sessions.CODEX_SHORTCUT if agent == "codex" else sessions.SHORTCUT
    command = sessions.start_command(agent, folder, args.full or not shutil.which(shortcut))
    if agent == "codex":
        command += " login"
    cwd = sessions.HOME
    if args.print:
        print("cd " + sessions.shell_path(cwd))
        print(command)
        return
    if agent == "codex":
        os.makedirs(folder, exist_ok=True)  # Codex won't start with a CODEX_HOME that doesn't exist
    if args.here:
        print("Signing in to %s here" % sessions.shell_path(folder))
        run_here(cwd, command, sessions)
    terminal = pick_terminal(args)
    why = open_terminal(terminal, cwd, command) if terminal else "no terminal found; set HATCHECK_TERMINAL"
    if why:
        sys.exit("Couldn't open a terminal (%s). Run this instead:\n  %s" % (why, command))
    print("Signing in to %s in a new %s window:\n  %s" % (sessions.shell_path(folder), os.path.basename(terminal), command))


def main(argv=None, prog=None):
    parser = argparse.ArgumentParser(prog=prog, description="Resume a Claude Code or Codex session in a new terminal "
                                                            "window, already in its folder.")
    parser.add_argument("session", nargs="?", help="the session ID, or the start of it (default: the newest session "
                                                   "started from this folder)")
    parser.add_argument("--from", dest="source", metavar="PROFILE",
                        help="the profile or home to take it from, when more than one has a copy")
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--here", action="store_true", help="resume it in this terminal instead")
    mode.add_argument("--print", action="store_true", help="print the cd and resume commands instead")
    parser.add_argument("--terminal", metavar="NAME", help="the terminal to open (default: HATCHECK_TERMINAL, or "
                                                           "this system's usual one)")
    parser.add_argument("--full", action="store_true", help="print CLAUDE_CONFIG_DIR=... claude rather than cl, and "
                                                            "CODEX_HOME=... codex rather than cx")
    parser.add_argument("--all", action="store_true", help="without SESSION, also consider sessions from Claude "
                                                           "Desktop, the SDK, the Codex app or codex exec")
    parser.add_argument("--login", metavar="PROFILE", help="sign in to a profile (NAME, default, codex or codex:NAME) "
                                                           "in a new terminal, instead of resuming a session")
    args = parser.parse_args(argv)
    if args.login and args.session:
        parser.error("give a SESSION or --login, not both")

    sessions = load_sibling("hatcheck_sessions", "sessions.py")
    move = load_sibling("hatcheck_move", "move.py")
    usage = sessions.load_usage_module()
    if usage:
        usage.setup_console()  # UTF-8 output on Windows
    show = sessions.shell_path
    if args.login:
        return login(args, sessions, move)
    s = pick(args, sessions, move)
    cwd = s.get("cwd")
    if not cwd or not os.path.isdir(cwd):
        sys.exit("Session %s ran in %s, which isn't there any more" % (s["id"][:8], show(cwd) if cwd else "?"))
    command = resume_line(s, sessions, args.full)
    if args.print:
        print("cd " + show(cwd))
        print(command)
        return
    refuse_if_open(s, sessions)
    title = sessions.truncate(s.get("title") or "?", 60)
    if args.here:
        print('Resuming session %s "%s" here, in %s' % (s["id"][:8], title, show(cwd)))
        run_here(cwd, command, sessions)
    terminal = pick_terminal(args)
    why = open_terminal(terminal, cwd, command) if terminal else "no terminal found; set HATCHECK_TERMINAL"
    if why:
        sys.exit("Couldn't open a terminal (%s). Run these instead:\n  cd %s\n  %s" % (why, show(cwd), command))
    print('Resuming session %s "%s" in a new %s window, in %s:\n  %s'
          % (s["id"][:8], title, os.path.basename(terminal), show(cwd), command))


if __name__ == "__main__":
    sys.exit(main())
