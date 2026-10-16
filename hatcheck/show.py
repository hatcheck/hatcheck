#!/usr/bin/env python3
"""Show a session's conversation, read-only.

Usage:
    hatcheck show SESSION [--from PROFILE] [--last N] [--json]

  SESSION : the session ID, or the start of it, of a Claude Code or Codex session.
  --from  : the profile or home to read it from when more than one has a copy
            (default: the copy used last).
  --last  : only the last N turns.
  --json  : print it as JSON, for scripts and the desktop app's preview.

Your messages and the replies, with each tool call as a block of text with its input
and the start of its output, as hatcheck move gives them when it converts a session.
It follows the conversation back from its newest message, so turns undone with
/rewind are left out, and so are Claude's thinking and Codex's reasoning.

This script loads sessions.py, open.py and move.py, so keep them in one folder.
"""
import argparse
import importlib.util
import json
import os
import sys


def load_sibling(name, file):
    sys.dont_write_bytecode = True  # leave no __pycache__ folder beside the scripts
    path = os.path.join(os.path.dirname(os.path.realpath(__file__)), file)
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def main(argv=None, prog=None):
    parser = argparse.ArgumentParser(prog=prog, description="Show a Claude Code or Codex session's conversation, "
                                                            "read-only.")
    parser.add_argument("session", help="the session ID, or the start of it")
    parser.add_argument("--from", dest="source", metavar="PROFILE",
                        help="the profile or home to read it from, when more than one has a copy")
    parser.add_argument("--last", metavar="N", type=int, help="only the last N turns")
    parser.add_argument("--json", action="store_true", help="print it as JSON")
    args = parser.parse_args(argv)
    args.all = True  # pick() reads it when no ID is given; an ID is always given here
    sessions = load_sibling("hatcheck_sessions", "sessions.py")
    move = load_sibling("hatcheck_move", "move.py")
    opener = load_sibling("hatcheck_open", "open.py")
    usage = sessions.load_usage_module()
    if usage:
        usage.setup_console()  # UTF-8 output on Windows
    s = opener.pick(args, sessions, move)
    conv = move.read_codex(s["path"], s["profile"], sessions) if s["agent"] == "codex" \
        else move.read_claude(s["path"], sessions)
    turns = conv["turns"][-args.last:] if args.last else conv["turns"]
    if args.json:
        print(json.dumps({
            "version": 1, "agent": s["agent"], "id": s["id"], "profile": os.path.abspath(s["profile"]),
            "title": s.get("title"), "cwd": conv.get("cwd") or s.get("cwd"), "model": conv.get("model"),
            "turns_total": len(conv["turns"]), "tool_calls": conv["calls"], "left_out": conv["left_out"],
            "turns": [{"role": role, "text": "\n\n".join(texts), "time": when} for role, texts, when in turns]},
            ensure_ascii=False, indent=2))
        return
    names = {"user": "You", "assistant": "Codex" if s["agent"] == "codex" else "Claude"}
    print('Session %s "%s" in %s, %d turns%s:' % (s["id"][:8], sessions.truncate(s.get("title") or "?", 60),
                                                 sessions.shell_path(s["profile"]), len(conv["turns"]),
                                                 ", the last %d" % len(turns) if len(turns) < len(conv["turns"]) else ""))
    for role, texts, when in turns:
        print("\n%s%s" % (names.get(role, role), "  (%s)" % when[:16].replace("T", " ") if when else ""))
        for line in "\n\n".join(texts).splitlines():
            print("  " + line)
    print("\nLeft out: %s." % conv["left_out"])


if __name__ == "__main__":
    sys.exit(main())
