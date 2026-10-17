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
"""
import importlib.util
import os
import re
import sys

HERE = os.path.dirname(os.path.realpath(__file__))
SUBCOMMANDS = ("sessions", "usage", "where", "move", "open", "show", "best", "bind", "unbind", "desktop",
               "provider")


def load(name):
    """A subcommand's module, from this folder."""
    sys.dont_write_bytecode = True  # leave no __pycache__ folder beside the scripts
    spec = importlib.util.spec_from_file_location("hatcheck_" + name, os.path.join(HERE, name + ".py"))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def version():
    with open(os.path.join(HERE, "__init__.py"), encoding="utf-8") as fh:
        return re.search(r'__version__ = "([^"]+)"', fh.read()).group(1)


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
