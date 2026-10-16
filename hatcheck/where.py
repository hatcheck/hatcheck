#!/usr/bin/env python3
"""Find a Claude Code or Codex session by its ID, or the start of it.

Usage:
    hatcheck where SESSION

For Claude Code it prints the transcript in ~/.claude or a ~/.claude-* profile,
plus its data folder if it has one: the profile in the path says how to resume
it (claude, or cl <profile>), and the folder under projects/ is the directory it
ran in, with every non-alphanumeric character turned into -. For Codex it
prints the session file in ~/.codex or a ~/.codex-* home, under sessions/ by the
date it started, or under archived_sessions/: the home says how to resume it
(codex resume <id>, or cx <home> resume <id>). Exits 1 when nothing matches.

This script loads sessions.py, so keep the two in one folder.
"""
import argparse
import glob
import importlib.util
import os
import sys


def load_sessions():
    sys.dont_write_bytecode = True  # leave no __pycache__ folder beside the scripts
    path = os.path.join(os.path.dirname(os.path.realpath(__file__)), "sessions.py")
    spec = importlib.util.spec_from_file_location("hatcheck_sessions", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def find(prefix, sessions):
    """The files (and Claude Code's data folders) of the sessions whose ID starts with prefix."""
    found = []
    for profile in sessions.find_profiles():
        found += sorted(glob.glob(os.path.join(profile, "projects", "*", glob.escape(prefix) + "*")))
    homes = [os.path.join(sessions.HOME, ".codex")] + sorted(glob.glob(os.path.join(sessions.HOME, ".codex-*")))
    if os.environ.get("CODEX_HOME"):
        homes.append(os.path.abspath(os.path.expanduser(os.environ["CODEX_HOME"])))
    seen = set()
    for home in homes:
        if os.path.realpath(home) in seen:
            continue
        seen.add(os.path.realpath(home))
        paths = glob.glob(os.path.join(home, "sessions", "*", "*", "*", "rollout-*.jsonl")) + \
            glob.glob(os.path.join(home, "archived_sessions", "rollout-*.jsonl"))
        for path in sorted(paths):
            match = sessions.CODEX_FILE.search(path)
            if match and match.group(1).startswith(prefix.lower()):
                found.append(path)
    return found


def main(argv=None, prog=None):
    parser = argparse.ArgumentParser(prog=prog, description="Find a Claude Code or Codex session by its ID, "
                                                            "or the start of it.")
    parser.add_argument("session", help="the session ID, or the start of it")
    args = parser.parse_args(argv)
    found = find(args.session, load_sessions())
    for path in found:
        print(path)
    return 0 if found else 1  # 1: nothing matched


if __name__ == "__main__":
    sys.exit(main())
