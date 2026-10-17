#!/usr/bin/env python3
"""Find a Claude Code or Codex session by its ID, or the start of it.

Usage:
    hatcheck where SESSION [--json]

For Claude Code it prints the transcript in ~/.claude or a ~/.claude-* profile,
plus its data folder if it has one: the profile in the path says how to resume
it (claude, or cl <profile>), and the folder under projects/ is the directory it
ran in, with every non-alphanumeric character turned into -. For Codex it
prints the session file in ~/.codex or a ~/.codex-* home, under sessions/ by the
date it started, or under archived_sessions/: the home says how to resume it
(codex resume <id>, or cx <home> resume <id>). Exits 1 when nothing matches.
--json prints each copy as data: its agent, ID, profile or home, file, data
folder and whether it's archived.

This script loads sessions.py, so keep the two in one folder.
"""
import argparse
import glob
import importlib.util
import json
import os
import sys


def load_sessions():
    sys.dont_write_bytecode = True  # leave no __pycache__ folder beside the scripts
    path = os.path.join(os.path.dirname(os.path.realpath(__file__)), "sessions.py")
    if "hatcheck_sessions" in sys.modules:  # loaded once per process: hatcheck serve runs many commands in one
        return sys.modules["hatcheck_sessions"]
    spec = importlib.util.spec_from_file_location("hatcheck_sessions", path)
    module = sys.modules["hatcheck_sessions"] = importlib.util.module_from_spec(spec)
    try:
        spec.loader.exec_module(module)
    except BaseException:
        del sys.modules["hatcheck_sessions"]
        raise
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


def as_json(found, prefix, sessions):
    """The matches as --json gives them: one entry per copy of a session, with its agent,
    ID, profile or home, file, data folder (Claude Code) and whether it's archived (Codex)."""
    copies = []
    for path in found:
        if path.endswith(".jsonl") and os.sep + "projects" + os.sep in path:
            profile = os.path.dirname(os.path.dirname(os.path.dirname(path)))
            data = path[:-len(".jsonl")]
            copies.append({"agent": "claude", "id": os.path.basename(data), "profile": profile, "file": path,
                           "data_folder": data if os.path.isdir(data) else None, "archived": False})
        elif sessions.CODEX_FILE.search(path):
            archived = os.path.basename(os.path.dirname(path)) == "archived_sessions"
            home = os.path.dirname(os.path.dirname(path)) if archived else \
                os.path.dirname(os.path.dirname(os.path.dirname(os.path.dirname(os.path.dirname(path)))))
            copies.append({"agent": "codex", "id": sessions.CODEX_FILE.search(path).group(1), "profile": home,
                           "file": path, "data_folder": None, "archived": archived})
    return {"version": 1, "query": prefix, "sessions": copies}


def main(argv=None, prog=None):
    parser = argparse.ArgumentParser(prog=prog, description="Find a Claude Code or Codex session by its ID, "
                                                            "or the start of it.")
    parser.add_argument("session", help="the session ID, or the start of it")
    parser.add_argument("--json", action="store_true", help="print the matches as JSON")
    args = parser.parse_args(argv)
    sessions = load_sessions()
    found = find(args.session, sessions)
    if args.json:
        print(json.dumps(as_json(found, args.session, sessions), ensure_ascii=False, indent=2))
        return 0 if found else 1
    for path in found:
        print(path)
    return 0 if found else 1  # 1: nothing matched


if __name__ == "__main__":
    sys.exit(main())
