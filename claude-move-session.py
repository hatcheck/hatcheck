#!/usr/bin/env python3
"""Copy a Claude Code session to another ~/.claude* profile, to carry on with it
there under that profile's account and plan limits.

Usage:
    python3 claude-move-session.py SESSION PROFILE [--from PROFILE] [--no-keep] [--dry-run]

  SESSION   : the session ID, or the start of it.
  PROFILE   : where it goes: NAME for ~/.claude-NAME, default for ~/.claude, or a
              path.
  --from    : the profile to take it from, when more than one has it.
  --no-keep : delete the original once the copy is checked, which moves it.
  --dry-run : show what would be copied and deleted, and change nothing.

The transcript, the folder of subagent transcripts and saved tool output, the
/rewind file snapshots and the plan file are copied with their timestamps, and
every copied file is checked. Paths in the conversation that point at those
files are changed to point at the copies, except inside Claude's thinking,
whose signatures would stop matching. A session that's open in a terminal is
refused: quit it first, so it stops changing. Plan files stay in the old
profile even with --no-keep, since sessions can share one.

Resume it with the command printed at the end, from the directory it ran in.
The conversation comes across whole; the account and its plan limits,
settings, permissions, MCP servers, plugins and project memory are the new
profile's. If the new account rejects the signatures on earlier thinking,
Claude Code drops those thinking blocks and carries on.

This script loads claude-recent-sessions.py, which loads claude-usage.py, so
keep the three in one folder.
"""
import argparse
import glob
import importlib.util
import json
import os
import re
import shutil
import sys

HOME = os.path.expanduser("~")
TMP = ".claude-move-session-tmp"  # suffix of a copy in progress


def load_sibling(name, file):
    """A script from this one's folder, loaded as a module."""
    sys.dont_write_bytecode = True  # leave no __pycache__ folder beside the scripts
    path = os.path.join(os.path.dirname(os.path.realpath(__file__)), file)
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def profile_path(name):
    """NAME for ~/.claude-NAME, default for ~/.claude, or a path."""
    if name == "default":
        return os.path.join(HOME, ".claude")
    if "/" in name or os.sep in name or name.startswith("~") or re.match(r"[A-Za-z]:", name):
        return os.path.abspath(os.path.expanduser(name))
    return os.path.join(HOME, ".claude-" + name)


def join(base, name):
    return os.path.join(base, name) if name else base


def files_in(path):
    """The files in a folder, as paths relative to it, or [""] for a file."""
    if not os.path.isdir(path):
        return [""]
    return sorted(os.path.relpath(os.path.join(root, f), path) for root, _, files in os.walk(path) for f in files)


def read(path):
    with open(path, "rb") as fh:
        return fh.read()


def repoint(value, pairs):
    """value with each old path changed to its new one. Thinking blocks are left alone:
    their signatures cover the text."""
    if isinstance(value, str):
        for old, new in pairs:
            value = value.replace(old, new)
        return value
    if isinstance(value, list):
        return [repoint(v, pairs) for v in value]
    if isinstance(value, dict):
        if value.get("type") in ("thinking", "redacted_thinking"):
            return value
        return {k: repoint(v, pairs) for k, v in value.items()}
    return value


def copy_bytes(src, pairs):
    """What the copy of file src holds: its bytes, with a transcript's paths repointed.
    Only lines holding an old path are rewritten; Claude Code writes compact JSON, so
    re-serializing one changes nothing but the paths."""
    data = read(src)
    # As the paths appear in the JSON text: on Windows with their backslashes doubled.
    olds = [json.dumps(old, ensure_ascii=False)[1:-1].encode() for old, _ in pairs]
    if not src.endswith(".jsonl") or not any(old in data for old in olds):
        return data
    lines = data.split(b"\n")
    for i, line in enumerate(lines):
        if any(old in line for old in olds):
            try:
                lines[i] = json.dumps(repoint(json.loads(line), pairs), ensure_ascii=False,
                                      separators=(",", ":")).encode()
            except (ValueError, UnicodeError):
                pass  # a line that doesn't round-trip stays as it was
    return b"\n".join(lines)


def matches(src, dst, pairs):
    """Whether dst holds exactly what copying src gives."""
    if not os.path.exists(dst) or os.path.isdir(src) != os.path.isdir(dst):
        return False
    names = files_in(src)
    return names == files_in(dst) and all(read(join(dst, n)) == copy_bytes(join(src, n), pairs) for n in names)


def copy(src, dst, pairs):
    """Copy a file or folder with its timestamps, under a temporary name until it's whole."""
    def copy_file(a, b):
        with open(b, "wb") as fh:
            fh.write(copy_bytes(a, pairs))
        shutil.copystat(a, b)

    tmp = dst + TMP
    if os.path.isdir(tmp):
        shutil.rmtree(tmp)  # left by an interrupted run
    os.makedirs(os.path.dirname(dst), exist_ok=True)
    if os.path.isdir(src):
        shutil.copytree(src, tmp, copy_function=copy_file)
    else:
        copy_file(src, tmp)
    os.replace(tmp, dst)


def path_pairs(old, new):
    """(old, new), plus on Windows the same with forward slashes, which tools print too."""
    return [(old, new)] + ([(old.replace(os.sep, "/"), new.replace(os.sep, "/"))] if os.sep != "/" else [])


def plan_slugs(transcripts):
    """Names of the plan files (plans/<slug>.md) the transcripts use."""
    slugs = set()
    for path in transcripts:
        for line in read(path).split(b"\n"):
            if b'"slug":"' not in line:
                continue
            try:
                record = json.loads(line)
            except ValueError:
                continue
            slug = record.get("slug") if isinstance(record, dict) else None
            if isinstance(slug, str) and slug and "/" not in slug:
                slugs.add(slug)
    return slugs


def size_text(n):
    if n < 1024:
        return "%d bytes" % n
    return "%.1f KB" % (n / 1024.0) if n < 1024 ** 2 else "%.1f MB" % (n / 1024.0 ** 2)


def main():
    parser = argparse.ArgumentParser(
        description="Copy a Claude Code session to another ~/.claude* profile, to carry on with it there.")
    parser.add_argument("session", help="the session ID, or the start of it")
    parser.add_argument("profile", help="where it goes: NAME for ~/.claude-NAME, default for ~/.claude, or a path")
    parser.add_argument("--from", dest="source", metavar="PROFILE",
                        help="the profile to take it from, when more than one has it")
    parser.add_argument("--no-keep", action="store_true",
                        help="delete the original once the copy is checked, which moves it")
    parser.add_argument("--dry-run", action="store_true",
                        help="show what would be copied and deleted, and change nothing")
    args = parser.parse_args()

    try:
        sessions = load_sibling("claude_recent_sessions", "claude-recent-sessions.py")
    except OSError:
        sys.exit("claude-move-session needs claude-recent-sessions.py in its folder")
    usage = sessions.load_usage_module()  # claude-usage.py: accounts, and UTF-8 output on Windows
    if usage:
        usage.setup_console()
    show = sessions.shell_path
    target = profile_path(args.profile)
    if not os.path.isdir(target):
        sys.exit("%s isn't a profile folder. Sign in there first, e.g. with: cl %s" % (show(target), args.profile))

    # Look in every profile but the target, or just in --from.
    sources = [profile_path(args.source)] if args.source else [
        p for p in sessions.find_profiles() if os.path.realpath(p) != os.path.realpath(target)]
    found = {}  # session ID -> {profile: [transcripts]}
    for profile in sources:
        for path in sorted(glob.glob(os.path.join(profile, "projects", "*", glob.escape(args.session) + "*.jsonl"))):
            found.setdefault(os.path.basename(path)[:-len(".jsonl")], {}).setdefault(profile, []).append(path)
    if not found:
        if args.source:
            sys.exit("No session starting with %s in %s" % (args.session, show(sources[0])))
        if glob.glob(os.path.join(target, "projects", "*", glob.escape(args.session) + "*.jsonl")):
            sys.exit("Session %s is already in %s, and in no other profile" % (args.session, show(target)))
        sys.exit("No session starting with %s" % args.session)
    if len(found) > 1:
        lines = ["  %s  %s" % (sid, ", ".join(map(show, where))) for sid, where in sorted(found.items())]
        sys.exit("%s matches %d sessions; give more of the ID:\n%s"
                 % (args.session, len(found), "\n".join(lines[:10] + ["  …"] * (len(lines) > 10))))
    (sid, where), = found.items()
    if len(where) > 1:
        sys.exit("Session %s is in %s; pick one with --from" % (sid, " and ".join(map(show, where))))
    (source, transcripts), = where.items()
    if os.path.realpath(source) == os.path.realpath(target):
        sys.exit("Session %s is already in %s" % (sid, show(target)))
    live = sessions.open_sessions([source]).get((os.path.realpath(source), sid))
    if live:
        sys.exit("Session %s is open in pid %s%s; quit it first, so it stops changing"
                 % (sid[:8], live.get("pid"), " (%s)" % live["status"] if live.get("status") else ""))

    # What to copy: (what, original, copy, deleted with --no-keep), and the paths to repoint.
    items, pairs = [], []
    for path in transcripts:
        old = path[:-len(".jsonl")]
        new = os.path.join(target, "projects", os.path.basename(os.path.dirname(path)), sid)
        pairs += path_pairs(old, new)  # covers <id>.jsonl and the <id>/ folder beside it
        items.append(("transcript", path, new + ".jsonl", True))
        if os.path.isdir(old):
            items.append(("subagents and tool output", old, new, True))
    for sub, what in (("file-history", "/rewind file snapshots"), ("session-env", "session environment")):
        old, new = os.path.join(source, sub, sid), os.path.join(target, sub, sid)
        if os.path.isdir(old) and files_in(old):
            items.append((what, old, new, True))
            pairs += path_pairs(old, new)
    for slug in sorted(plan_slugs(transcripts)):
        old, new = os.path.join(source, "plans", slug + ".md"), os.path.join(target, "plans", slug + ".md")
        if os.path.isfile(old):
            items.append(("plan", old, new, False))  # kept: sessions can share a plan
            pairs += path_pairs(old, new)

    present, conflicts = set(), []
    for _, src, dst, _ in items:
        if os.path.lexists(dst):
            if matches(src, dst, pairs):
                present.add(dst)
            else:
                conflicts.append(dst)
    if conflicts:
        sys.exit("%s already has a different copy of this session. Delete it there first to replace it:\n%s"
                 % (show(target), "\n".join("  " + show(c) for c in conflicts)))
    repointed = any(read(join(src, n)) != copy_bytes(join(src, n), pairs)
                    for _, src, _, _ in items for n in files_in(src) if join(src, n).endswith(".jsonl"))
    details = {}  # for the report, worked out before --no-keep deletes the originals
    for what, src, _, _ in items:
        files = len(files_in(src))
        details[src] = (os.path.basename(src) if what == "plan" else size_text(os.path.getsize(src))
                        if os.path.isfile(src) else "%d file%s" % (files, "s" * (files != 1)))
    summaries = []
    for path in transcripts:
        try:
            summaries.append(sessions.read_session(path))
        except (OSError, ValueError):
            pass
    info = max(filter(None, summaries), key=lambda s: (s["when"], s["canonical"], s["size"]), default=None)

    if not args.dry_run:
        for _, src, dst, _ in items:
            if dst not in present:
                copy(src, dst, pairs)
        wrong = [dst for _, src, dst, _ in items if not matches(src, dst, pairs)]
        if wrong:
            sys.exit("These copies don't match their originals, so nothing was deleted:\n"
                     + "\n".join("  " + show(w) for w in wrong))
        if args.no_keep:
            for _, src, _, deleted in items:
                if deleted and os.path.isdir(src):
                    shutil.rmtree(src)
                elif deleted:
                    os.remove(src)

    heading = ("Would move" if args.dry_run else "Moved") if args.no_keep else (
        "Would copy" if args.dry_run else "Copied")
    title = sessions.truncate(info["title"], 60) if info else "(no conversation)"
    print('%s session %s "%s" from %s to %s:' % (heading, sid[:8], title, show(source), show(target)))
    for what, src, dst, _ in items:
        print("  %-26s %s%s" % (what, details[src], "  (already there)" if dst in present else ""))
    if repointed:
        print("Paths in the conversation to these files %s to the copies." % ("would point" if args.dry_run else "now point"))
    accounts = {usage.read_config(p)[0].get("accountUuid") for p in (source, target)} if usage else {None}
    if len(accounts) == 1 and None not in accounts:
        print("Both profiles are signed in to the same account, so its plan limits stay the same.")
    if args.no_keep:
        print("%s the original from %s%s." % (
            "Would delete" if args.dry_run else "Deleted", show(source),
            "; the plan file stays, since sessions can share one" if any(w == "plan" for w, _, _, _ in items) else ""))
    else:
        print("Kept the original in %s; --no-keep deletes it once the copy is checked." % show(source))
    print("Then resume it with:" if args.dry_run else "Resume it with:")
    if info and info["cwd"]:
        print("  cd " + show(info["cwd"]))
    print("  " + sessions.resume_command(target, sid, False))
    provider = sessions.provider_of(info["model"]) if info else None
    if provider:
        print("Its last reply came from %s; to carry on there rather than on this profile's Claude plan:\n  %s"
              % (info["model"], sessions.resume_command(target, sid, False, provider)))


if __name__ == "__main__":
    main()
