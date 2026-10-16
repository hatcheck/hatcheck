#!/usr/bin/env python3
"""hatcheck: Claude Code and Codex accounts on one computer, each kept apart. Lists
recent sessions with the command that resumes each one and what's left of each
plan, and carries sessions between accounts and agents.

Usage:
    hatcheck [sessions] [HOURS] ...    recent sessions in every profile and home, with
                                       resume commands and plan usage (the default)
    hatcheck usage ...                 plan usage left for every profile and home
    hatcheck where SESSION             where a session is stored, by its ID or the start of it
    hatcheck move SESSION PROFILE ...  copy a session to another profile or home, or convert
                                       it between Claude Code and Codex
    hatcheck provider ...              the Z.ai and Kimi keys and models behind claude-z,
                                       claude-k, cl -z and cl -k

`hatcheck SUBCOMMAND --help` lists a subcommand's options. cl and cx, beside
this script, run Claude Code and Codex with a profile.

The subcommands are the files in the hatcheck/ folder beside this script. The
commands from before the rename (claude-recent-sessions, claude-usage,
claude-where, claude-move-session and claude-provider) are small scripts beside
it that run the matching subcommand, so older installs keep working.
"""
import importlib.util
import os
import re
import sys

SUBCOMMANDS = ("sessions", "usage", "where", "move", "provider")


def load(name):
    """A subcommand's module, from the hatcheck/ folder beside this script."""
    sys.dont_write_bytecode = True  # leave no __pycache__ folder beside the scripts
    path = os.path.join(os.path.dirname(os.path.realpath(__file__)), "hatcheck", name + ".py")
    spec = importlib.util.spec_from_file_location("hatcheck_" + name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def main(argv=None):
    argv = sys.argv[1:] if argv is None else list(argv)
    if argv and argv[0] in SUBCOMMANDS:
        command = argv.pop(0)
        prog = "hatcheck " + command
    elif argv and argv[0] in ("-h", "--help", "help"):
        print(__doc__.strip())
        return 0
    elif argv and not argv[0].startswith("-") and not re.fullmatch(r"\d+(\.\d+)?[hd]?", argv[0].lower()):
        sys.stderr.write("hatcheck: there's no %r subcommand; see hatcheck --help\n" % argv[0])
        return 2
    else:  # options or HOURS: the session list
        command, prog = "sessions", "hatcheck"
    return load(command).main(argv, prog)


if __name__ == "__main__":
    sys.exit(main())
