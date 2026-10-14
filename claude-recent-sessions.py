#!/usr/bin/env python3
"""List recent Claude Code CLI sessions from every ~/.claude* profile, with the
command that resumes each one and how much of its profile's plan is left.

Usage:
    python3 claude-recent-sessions.py [HOURS] [-n N] [--all] [--full] [--cached] [--no-usage]

  HOURS      : how far back to look (default 24). Accepts 36, 1.5, 12h or 3d.
  -n N       : show only the newest N sessions per folder (default: all).
  --all      : also list sessions last used from Claude Desktop's Code tab or the
               SDK (claude -p, apps driving Claude Code). Hidden by default.
  --full     : print CLAUDE_CONFIG_DIR=~/.claude-<name> claude ... instead of the
               cl shortcut, for shells that don't define it.
  --cached   : take plan usage from the snapshot Claude Code last saved instead
               of asking the API.
  --no-usage : leave plan usage out.

Output is grouped by folder (A-Z), each headed by the cd command to run first,
with the newest session first within each folder. Sessions in a
~/.claude-<name> profile resume with `cl <name> --resume <id>`, using this zsh
function from ~/.zshrc:

    cl() { CLAUDE_CONFIG_DIR="$HOME/.claude-$1" claude "${@:2}"; }

After its age, each session line shows what's left of its profile's plan
limits: the 5-hour session (5h), the weekly limit (week) and per-model weekly
limits such as Fable. They come from claude-usage.py, which this script loads, so keep the
two in one folder. When an account can't be asked live, Claude Code's last
saved snapshot is used and listed at the bottom with its age; ? marks a limit
that has reset since that snapshot.

A profile is any ~/.claude* directory (plus $CLAUDE_CONFIG_DIR) that holds a
projects/ folder, so Desktop data dirs like ~/.claude-instances are skipped.
Each transcript records the entrypoint that wrote it; a session counts as
Desktop when its most recent activity came from Claude Desktop. Sessions still
open in a terminal are always listed, even when idle longer than HOURS, and
marked so you don't resume them twice.
"""
import argparse
import glob
import importlib.util
import json
import mmap
import os
import re
import shlex
import shutil
import subprocess
import sys
import time
import unicodedata
from datetime import datetime

HOME = os.path.expanduser("~")
DEFAULT_PROFILE = os.path.realpath(os.path.join(HOME, ".claude"))
DEFAULT_HOURS = 24
SHORTCUT = "cl"  # zsh function in ~/.zshrc: cl <name> ... runs claude with ~/.claude-<name>


def parse_count(text):
    if not text.isdigit() or int(text) < 1:
        raise argparse.ArgumentTypeError("expected a whole number of 1 or more")
    return int(text)


def parse_hours(text):
    m = re.fullmatch(r"(\d+(?:\.\d+)?)([hd]?)", text.strip().lower())
    if not m:
        raise argparse.ArgumentTypeError("expected hours like 24, 1.5, 12h or 3d")
    value = float(m.group(1))
    return value * 24 if m.group(2) == "d" else value


def find_profiles():
    """Every Claude Code config dir: ~/.claude* dirs with a projects/ folder."""
    candidates = glob.glob(os.path.join(HOME, ".claude*"))
    if os.environ.get("CLAUDE_CONFIG_DIR"):
        candidates.append(os.path.expanduser(os.environ["CLAUDE_CONFIG_DIR"]))
    profiles = {}
    for d in candidates:
        if os.path.isdir(os.path.join(d, "projects")):
            profiles.setdefault(os.path.realpath(d), os.path.abspath(d))
    return sorted(profiles.values(), key=lambda d: (os.path.realpath(d) != DEFAULT_PROFILE, d))


def open_sessions(profiles):
    """(profile, sessionId) -> registry entry for sessions whose claude process is alive.

    Claude Code writes <profile>/sessions/<pid>.json with the process start time
    (UTC, `ps -o lstart` format); matching it guards against reused PIDs.
    """
    try:
        out = subprocess.run(["ps", "-A", "-o", "pid=,lstart="], capture_output=True, text=True,
                             env=dict(os.environ, TZ="UTC", LC_ALL="C")).stdout
    except OSError:
        out = ""
    started = {}
    for line in out.splitlines():
        pid, _, lstart = line.strip().partition(" ")
        started[pid] = " ".join(lstart.split())
    live = {}
    for profile in profiles:
        for path in glob.glob(os.path.join(profile, "sessions", "*.json")):
            try:
                with open(path) as fh:
                    entry = json.load(fh)
            except (OSError, ValueError):
                continue
            pid = str(entry.get("pid"))
            proc_start = " ".join(str(entry.get("procStart") or "").split())
            if entry.get("sessionId") and pid in started and (not proc_start or proc_start == started[pid]):
                live[(os.path.realpath(profile), entry["sessionId"])] = entry
    return live


def lines_backward(mm):
    end = len(mm)
    if end and mm[end - 1:end] == b"\n":
        end -= 1
    while end > 0:
        start = mm.rfind(b"\n", 0, end) + 1
        yield mm[start:end]
        end = start - 1


def last_record(mm, rtype):
    """Last top-level record of the given type, found without parsing the whole file."""
    key = b'"type":"' + rtype.encode() + b'"'
    end = len(mm)
    while True:
        i = mm.rfind(key, 0, end)
        if i < 0:
            return None
        start = mm.rfind(b"\n", 0, i) + 1
        stop = mm.find(b"\n", i)
        try:
            record = json.loads(mm[start:stop if stop >= 0 else len(mm)])
            if record.get("type") == rtype:
                return record
        except ValueError:
            pass
        if start == 0:
            return None
        end = start


def prompt_text(record):
    content = (record.get("message") or {}).get("content")
    if isinstance(content, list):
        content = next((c.get("text") for c in content
                        if isinstance(c, dict) and c.get("type") == "text"), None)
    # Skip slash-command and system wrappers such as <command-name> or <local-command-caveat>.
    if isinstance(content, str) and content.strip() and not content.lstrip().startswith("<"):
        return content
    return None


def first_cwd_and_prompt(mm, want_prompt):
    cwd = prompt = None
    mm.seek(0)
    for n, line in enumerate(iter(mm.readline, b"")):
        if cwd and (prompt or not want_prompt or n > 2000):
            break
        if b'"cwd":"' not in line:
            continue
        try:
            record = json.loads(line)
        except ValueError:
            continue
        cwd = cwd or record.get("cwd")
        if want_prompt and not prompt and record.get("type") == "user" and not record.get("isMeta"):
            prompt = prompt_text(record)
    return cwd, prompt


def parse_time(stamp):
    try:
        return datetime.fromisoformat(stamp.replace("Z", "+00:00")).timestamp()
    except (AttributeError, ValueError):
        return None


def encode_project(path):
    # Claude Code names projects/<folder> after the cwd with every non-alphanumeric
    # character replaced by "-", truncating (and hashing) very long names.
    return re.sub(r"[^A-Za-z0-9]", "-", path)


def resume_dir(folder, candidates):
    """The directory `claude --resume` must run from: the one that maps to the transcript's folder."""
    paths = []
    for c in candidates:
        if c:
            paths += [c, os.path.realpath(c)]
    for p in paths:
        enc = encode_project(p)
        if folder == enc or (len(enc) > 200 and folder.startswith(enc[:200])):
            return p
    return paths[0] if paths else None


def read_session(path):
    """Summary of one transcript, or None when it holds no conversation."""
    with open(path, "rb") as fh:
        size = os.fstat(fh.fileno()).st_size
        if size == 0:
            return None
        mm = mmap.mmap(fh.fileno(), 0, access=mmap.ACCESS_READ)
    try:
        # Walk back to the newest record (last activity), then on to the newest
        # user/assistant message; sessions with only slash commands like /usage have none.
        last, has_messages = None, False
        for line in lines_backward(mm):
            if b'"entrypoint":"' not in line:
                continue
            try:
                record = json.loads(line)
            except ValueError:
                continue
            if last is None and record.get("entrypoint") and record.get("timestamp"):
                last = record
            if record.get("type") in ("user", "assistant"):
                has_messages = True
                break
        if last is None:
            return None
        relocated = last_record(mm, "relocated") or {}
        continued = last_record(mm, "continued-in") or {}
        titles = [(last_record(mm, t) or {}).get(k) for t, k in
                  (("custom-title", "customTitle"), ("ai-title", "aiTitle"),
                   ("agent-name", "agentName"), ("last-prompt", "lastPrompt"))]
        start_cwd, first_prompt = first_cwd_and_prompt(mm, want_prompt=not any(titles))
    finally:
        mm.close()
    folder = os.path.basename(os.path.dirname(path))
    cwd = resume_dir(folder, [relocated.get("relocatedCwd"), start_cwd, last.get("cwd")])
    return {
        "id": os.path.basename(path)[:-len(".jsonl")],
        "when": parse_time(last["timestamp"]) or os.path.getmtime(path),
        "entrypoint": last["entrypoint"],
        "branch": last.get("gitBranch"),
        "cwd": cwd,
        # Older Claude Code versions filed some sessions under the symlinked path as
        # well; the copy under the resolved path is the one `--resume` reads.
        "canonical": bool(cwd) and encode_project(os.path.realpath(cwd)) == folder,
        "size": size,
        "has_messages": has_messages,
        "title": " ".join(next((t for t in titles + [first_prompt] if t), "(untitled)").split()),
        "continued_in": continued.get("continuedInSessionId"),
    }


def shell_path(path):
    if path == HOME or path.startswith(HOME + "/"):
        rest = path[len(HOME) + 1:]
        return "~/" + shlex.quote(rest) if rest else "~"
    return shlex.quote(path)


def resume_command(profile, session_id, full):
    if os.path.realpath(profile) == DEFAULT_PROFILE:
        return "claude --resume " + session_id
    name = os.path.basename(profile.rstrip("/"))
    if not full and os.path.dirname(profile.rstrip("/")) == HOME and name.startswith(".claude-"):
        return "%s %s --resume %s" % (SHORTCUT, shlex.quote(name[len(".claude-"):]), session_id)
    return "CLAUDE_CONFIG_DIR=%s claude --resume %s" % (shell_path(profile), session_id)


def load_usage_module():
    """claude-usage.py from this script's folder, loaded as a module, or None if it's missing."""
    path = os.path.join(os.path.dirname(os.path.realpath(__file__)), "claude-usage.py")
    if not os.path.isfile(path):
        return None
    sys.dont_write_bytecode = True  # leave no __pycache__ folder beside the scripts
    spec = importlib.util.spec_from_file_location("claude_usage", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def usage_cells(usage_mod, results, now, paint):
    """profile -> (text, width): each plan limit's short name and % left, in columns
    aligned across profiles."""
    meters = {}
    for profile, (usage, saved_at, _) in results.items():
        meters[profile] = [] if usage is None else [
            (usage_mod.row_label(row, short=True),
             usage_mod.percent_left(row.get("percent"), row.get("resets_at"), saved_at, now),
             row.get("severity"))
            for row in usage.get("limits") or []]
    pct = lambda left: "?" if left is None else "%d%%" % left
    label_w, pct_w = {}, {}
    for rows in meters.values():
        for i, (label, left, _) in enumerate(rows):
            label_w[i] = max(label_w.get(i, 0), len(label))
            pct_w[i] = max(pct_w.get(i, 0), len(pct(left)))
    cells = {}
    for profile, rows in meters.items():
        if not rows:
            cells[profile] = (paint("2", "usage ?"), len("usage ?"))
            continue
        parts = [paint("2", label.ljust(label_w[i])) + " "
                 + paint("2" if left is None else usage_mod.SEVERITY_COLOR.get(severity, ""),
                         pct(left).rjust(pct_w[i]))
                 for i, (label, left, severity) in enumerate(rows)]
        width = sum(label_w[i] + 1 + pct_w[i] for i in range(len(rows))) + 2 * (len(rows) - 1)
        cells[profile] = ("  ".join(parts), width)
    return cells


def ago(seconds):
    if seconds < 3600:
        return "%dm" % max(seconds // 60, 0)
    if seconds < 86400:
        return "%dh" % (seconds // 3600)
    return "%dd" % (seconds // 86400)


def char_width(c):
    return 2 if unicodedata.east_asian_width(c) in "WF" else 1


def truncate(text, cols):
    """Cut text to fit `cols` terminal columns; CJK characters take two."""
    if sum(map(char_width, text)) <= cols:
        return text
    out, used = [], 0
    for c in text:
        if used + char_width(c) > cols - 1:
            break
        out.append(c)
        used += char_width(c)
    return "".join(out) + "…"


def main():
    parser = argparse.ArgumentParser(
        description="List recent Claude Code CLI sessions across all ~/.claude* profiles, "
                    "with the command that resumes each one.")
    parser.add_argument("hours", nargs="?", type=parse_hours, default=DEFAULT_HOURS,
                        help="how far back to look (default %d); accepts 36, 1.5, 12h or 3d" % DEFAULT_HOURS)
    parser.add_argument("-n", "--per-folder", metavar="N", type=parse_count,
                        help="show only the newest N sessions per folder (default: all)")
    parser.add_argument("--all", action="store_true",
                        help="also list sessions last used from Claude Desktop or the SDK")
    parser.add_argument("--full", action="store_true",
                        help="print CLAUDE_CONFIG_DIR=... claude instead of the %s shortcut" % SHORTCUT)
    parser.add_argument("--cached", action="store_true",
                        help="take plan usage from the snapshot Claude Code last saved instead of the API")
    parser.add_argument("--no-usage", action="store_true", help="leave plan usage out")
    args = parser.parse_args()

    color = sys.stdout.isatty() and not os.environ.get("NO_COLOR")
    paint = (lambda code, s: "\033[%sm%s\033[0m" % (code, s) if code else s) if color \
        else (lambda code, s: s)
    width = shutil.get_terminal_size((100, 24)).columns

    now = time.time()
    cutoff = now - args.hours * 3600
    profiles = find_profiles()
    live = open_sessions(profiles)

    found, empty, unreadable = {}, 0, 0
    for profile in profiles:
        real = os.path.realpath(profile)
        for path in glob.glob(os.path.join(profile, "projects", "*", "*.jsonl")):
            key = (real, os.path.basename(path)[:-len(".jsonl")])
            try:
                # A file's mtime is never older than its last message, so this skips
                # old sessions without reading them.
                if os.path.getmtime(path) < cutoff and key not in live:
                    continue
                session = read_session(path)
            except (OSError, ValueError):
                unreadable += 1
                continue
            if session is None or not (session["has_messages"] or key in live):
                empty += 1
                continue
            # A session open in a terminal is listed even if its last message is older.
            if session["when"] < cutoff and key not in live:
                continue
            session["profile"] = profile
            session["open"] = live.get(key)
            rank = (session["when"], session["canonical"], session["size"])
            if key not in found or rank > found[key][0]:
                found[key] = (rank, session)

    shown, hidden = [], {}
    per_profile = dict.fromkeys(profiles, 0)
    for _, s in found.values():
        if s["entrypoint"] != "cli" and not args.all:
            label = "Desktop" if s["entrypoint"] == "claude-desktop" else s["entrypoint"]
            hidden[label] = hidden.get(label, 0) + 1
            continue
        per_profile[s["profile"]] += 1
        shown.append(s)

    groups = {}
    for s in shown:
        # Group by the exact path the transcript is filed under, not its realpath: an
        # old session filed under a symlinked path must be resumed from that path.
        groups.setdefault(s["cwd"] or HOME, []).append(s)
    folders = []
    for folder in sorted(groups, key=str.lower):
        sessions = sorted(groups[folder], key=lambda s: s["when"], reverse=True)
        listed = sessions[:args.per_folder]  # all of them when -n isn't given
        folders.append((folder, listed, len(sessions) - len(listed)))

    # Plan usage for the profiles behind the listed sessions, one request per account.
    usage_mod = None if args.no_usage else load_usage_module()
    used_profiles = [p for p in profiles if any(s["profile"] == p for _, listed, _ in folders for s in listed)]
    results = {}
    if usage_mod:
        for entry, result in usage_mod.load_usage(used_profiles, args.cached):
            results.update(dict.fromkeys(entry["profiles"], result))
    cells = usage_cells(usage_mod, results, now, paint) if results else {}
    cells_width = max([w for _, w in cells.values()] or [0])

    window = ("%g" % args.hours) + "h"
    print(paint("1", "Claude Code CLI sessions active in the last %s or open now" % window)
          + paint("2", " (by folder, newest first%s)" % ("; % = plan usage left" if cells else "")))
    if not shown:
        print("  No sessions in this window. Try a longer one, e.g. %s 72" % os.path.basename(sys.argv[0]))

    for folder, listed, older in folders:
        missing = "" if os.path.isdir(folder) else "  " + paint("31", "(directory missing)")
        print(paint("1;36", "cd " + shell_path(folder)) + missing)
        for s in listed:
            # No profile here: the command shows it (plain `claude` means ~/.claude).
            meta = [(s["branch"], "2")] if s["branch"] else []
            if s["entrypoint"] != "cli":
                meta.append(("via " + s["entrypoint"], "33"))
            if s["open"]:
                entry = s["open"]
                meta.append(("● open in pid %s" % entry.get("pid")
                             + (" (%s)" % entry["status"] if entry.get("status") else ""), "32"))
            if s["continued_in"]:
                meta.append(("continued in " + s["continued_in"][:8], "2"))
            # The usage cell sits right above the command, which names the profile.
            cell, cell_width = cells.get(s["profile"], ("", 0))
            usage_col = cell + " " * (cells_width - cell_width + 2) if cells else ""
            room = width - 8 - (cells_width + 2 if cells else 0) - sum(3 + len(text) for text, _ in meta)
            print("  %s  %s%s%s" % (paint("2", ago(now - s["when"]).rjust(4)), usage_col,
                                    paint("1", truncate(s["title"], max(room, 20))),
                                    "".join(paint("2", " · ") + paint(c, text) for text, c in meta)))
            print(" " * 8 + resume_command(s["profile"], s["id"], args.full))
        if older:
            print("  " + paint("2", ("+%d" % older).rjust(4) + "  older session" + "s" * (older > 1)))

    print(paint("2", "Profiles: " + ", ".join(
        "%s (%d)" % (shell_path(p), n) for p, n in per_profile.items())))
    snapshots, no_data = {}, {}
    for p in used_profiles:
        if p in results:
            usage, saved_at, why = results[p]
            if usage is None:
                no_data.setdefault(why, []).append(shell_path(p))
            elif saved_at is not None:
                snapshots.setdefault(why, []).append("%s %s ago" % (shell_path(p), ago(now - saved_at)))
    for why, items in snapshots.items():
        print(paint("2", "Usage from Claude Code's last snapshot (%s): %s" % (why, ", ".join(items))))
    for why, items in no_data.items():
        print(paint("2", "No usage data (%s): %s" % (why, ", ".join(items))))
    if used_profiles and not args.no_usage and not usage_mod:
        print(paint("2", "No usage data: claude-usage.py isn't in this script's folder"))
    notes = []
    if hidden:
        notes.append("Hidden: " + ", ".join("%d %s" % (n, k) for k, n in sorted(hidden.items()))
                     + " (show with --all)")
    if empty:
        notes.append("%d without messages skipped" % empty)
    if unreadable:
        notes.append("%d unreadable" % unreadable)
    if notes:
        print(paint("2", "; ".join(notes)))


if __name__ == "__main__":
    main()
