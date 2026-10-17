#!/usr/bin/env python3
"""hatcheck: Claude Code and Codex accounts on one computer, each kept apart. Lists
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
and so on. cl and cx run Claude Code and Codex with a profile.

The subcommands are the files in the hatcheck/ folder: this one, cli.py, runs them.
hatcheck serve is for the desktop app: it runs the commands it's sent as lines of JSON,
staying up between them (see serve below).
"""
import contextlib
import importlib.util
import io
import json
import os
import re
import sys
import traceback

HERE = os.path.dirname(os.path.realpath(__file__))
SUBCOMMANDS = ("sessions", "usage", "where", "move", "open", "show", "best", "bind", "unbind", "desktop",
               "provider")


def load(name):
    """A subcommand's module, from this folder, loaded once per process: hatcheck serve runs
    many commands in one, and the modules load their siblings under the same names."""
    key = "hatcheck_" + name
    if key in sys.modules:
        return sys.modules[key]
    sys.dont_write_bytecode = True  # leave no __pycache__ folder beside the scripts
    spec = importlib.util.spec_from_file_location(key, os.path.join(HERE, name + ".py"))
    module = sys.modules[key] = importlib.util.module_from_spec(spec)
    try:
        spec.loader.exec_module(module)
    except BaseException:
        del sys.modules[key]
        raise
    return module


def version():
    with open(os.path.join(HERE, "__init__.py"), encoding="utf-8") as fh:
        return re.search(r'__version__ = "([^"]+)"', fh.read()).group(1)


def serve():
    """hatcheck serve: run the command on each line of JSON read from stdin, such as
    {"id": 1, "args": ["usage", "--json"], "env": {"HATCHECK_TERMINAL": "iterm"}}, and answer
    each with a line of JSON on stdout, {"id": 1, "code": 0, "out": "...", "err": "..."},
    until stdin ends. env sets environment variables for that command alone (null unsets
    one). The process stays up between commands, so the desktop app pays for starting it once,
    and the modules and the session cache stay loaded."""
    # The requests and answers get pipes of their own. A program a command starts gets
    # nothing to read and has its output thrown away, so it can't take a request or garble
    # an answer.
    requests = os.fdopen(os.dup(0), "r", encoding="utf-8")
    answers = os.fdopen(os.dup(1), "w", encoding="utf-8", newline="\n")
    for fd, mode in ((0, os.O_RDONLY), (1, os.O_WRONLY)):
        null = os.open(os.devnull, mode)
        os.dup2(null, fd)
        os.close(null)
    for line in requests:
        if not line.strip():
            continue
        out, err, code, rid = io.StringIO(), io.StringIO(), 2, None
        try:
            request = json.loads(line)
            rid, args, env = request.get("id"), [str(a) for a in request["args"]], request.get("env") or {}
            env = {str(k): v for k, v in env.items()}
        except (ValueError, KeyError, TypeError, AttributeError):
            err.write("hatcheck serve: not a request: %s\n" % line.strip()[:200])
        else:
            if "--here" in args:  # it would turn this process into the session's terminal
                err.write("hatcheck serve can't run a command here (--here); run it in a terminal\n")
            else:
                code = serve_one(args, env, out, err)
        answers.write(json.dumps({"id": rid, "code": code, "out": out.getvalue(), "err": err.getvalue()}) + "\n")
        answers.flush()
    return 0


def serve_one(args, env, out, err):
    """One of serve's commands, its output going to out and err: its exit code."""
    saved, stdin = {k: os.environ.get(k) for k in env}, sys.stdin
    sys.stdin = io.StringIO()  # nothing to read: a command that would ask takes its default
    try:
        for k, v in env.items():
            if v is None:
                os.environ.pop(k, None)
            else:
                os.environ[k] = str(v)
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            try:
                code = main(args)
            except SystemExit as e:
                code = e.code
                if isinstance(code, str):  # sys.exit("message")
                    err.write(code + "\n")
                    code = 1
            except Exception:
                traceback.print_exc(file=err)
                code = 1
    finally:
        sys.stdin = stdin
        for k, v in saved.items():
            if v is None:
                os.environ.pop(k, None)
            else:
                os.environ[k] = v
        sessions = sys.modules.get("hatcheck_sessions")
        if sessions is not None:
            sessions.save_session_cache()  # now rather than at exit: the app can quit any time
    return code if isinstance(code, int) else (0 if code is None else 1)


def main(argv=None):
    argv = sys.argv[1:] if argv is None else list(argv)
    # The name it was run by, for its messages: hat, the short name, or hatcheck. A link
    # or command named hat says so in sys.argv[0]; install.ps1's hat.cmd passes --as hat.
    name = os.path.splitext(os.path.basename(sys.argv[0]))[0]
    if len(argv) >= 2 and argv[0] == "--as":
        name, argv = argv[1], argv[2:]
    me = "hat" if name == "hat" else "hatcheck"
    if argv and argv[0] == "check":  # hat check: the whole name again, so the same as hat
        argv.pop(0)
    if argv == ["serve"]:
        return serve()
    if argv and argv[0] in SUBCOMMANDS:
        command = argv.pop(0)
        prog = me + " " + command
    elif argv and argv[0] in ("-h", "--help", "help"):
        if me == "hat":
            print("hat is the short name for hatcheck.\n")
        print(__doc__.strip())
        return 0
    elif argv and argv[0] in ("-V", "--version"):
        print("%s %s" % (me, version()))
        return 0
    elif argv and not argv[0].startswith("-") and not re.fullmatch(r"\d+(\.\d+)?[hd]?", argv[0].lower()):
        sys.stderr.write("%s: there's no %r subcommand; see %s --help\n" % (me, argv[0], me))
        return 2
    else:  # options or HOURS: the session list
        command, prog = "sessions", me
    return load(command).main(argv, prog)


if __name__ == "__main__":
    sys.exit(main())
