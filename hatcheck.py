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

`hatcheck SUBCOMMAND --help` lists a subcommand's options. hat is the short
name: hat usage is hatcheck usage, and so on. cl and cx, beside this script,
run Claude Code and Codex with a profile.

The subcommands are the files in the hatcheck/ folder beside this script.
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
    # The name it was run by, for its messages: hat, the short name, or hatcheck. A link
    # named hat says so in sys.argv[0]; install.ps1's hat.cmd passes --as hat.
    name = os.path.splitext(os.path.basename(sys.argv[0]))[0]
    if len(argv) >= 2 and argv[0] == "--as":
        name, argv = argv[1], argv[2:]
    me = "hat" if name == "hat" else "hatcheck"
    if argv and argv[0] in SUBCOMMANDS:
        command = argv.pop(0)
        prog = me + " " + command
    elif argv and argv[0] in ("-h", "--help", "help"):
        if me == "hat":
            print("hat is the short name for hatcheck.\n")
        print(__doc__.strip())
        return 0
    elif argv and not argv[0].startswith("-") and not re.fullmatch(r"\d+(\.\d+)?[hd]?", argv[0].lower()):
        sys.stderr.write("%s: there's no %r subcommand; see %s --help\n" % (me, argv[0], me))
        return 2
    else:  # options or HOURS: the session list
        command, prog = "sessions", me
    return load(command).main(argv, prog)


if __name__ == "__main__":
    sys.exit(main())
