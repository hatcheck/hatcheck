#!/usr/bin/env python3
"""List recent Claude Code CLI sessions from every ~/.claude* profile, with the
command that resumes each one and how much of its profile's plan is left.

Usage:
    python3 claude-recent-sessions.py [HOURS] [-n N] [--all] [--all-folders]
                                      [--full] [--cached] [--no-usage]

  HOURS         : how far back to look (default 24). Accepts 36, 1.5, 12h or 3d.
  -n N          : show only the newest N sessions per folder (default: all).
  --all         : also list sessions last used from Claude Desktop's Code tab or
                  the SDK (claude -p, apps driving Claude Code), hidden by
                  default.
  --all-folders : list every folder, even when run from one that has sessions.
  --full        : print CLAUDE_CONFIG_DIR=~/.claude-<name> claude ... instead of
                  the cl shortcut, for machines without it.
  --cached      : take plan usage from the snapshot Claude Code last saved
                  instead of asking the API.
  --no-usage    : leave plan usage out.

Run from a folder that has sessions in the time window, it lists just that
folder's; anywhere else it lists every folder. Output is grouped by folder
(A-Z), each headed by the cd command to run first, with the newest session
first within each folder. Sessions in a
~/.claude-<name> profile resume with `cl <name> --resume <id>`. cl is in this
repo (install.sh puts it on your PATH) and works like this shell function:

    cl() { CLAUDE_CONFIG_DIR="$HOME/.claude-$1" claude "${@:2}"; }

A session whose newest reply came from Z.ai (a GLM model) or Kimi shows that
model, as in "on k3", and resumes there: claude-z or claude-k --resume <id> in
~/.claude, cl -z or cl -k <name> --resume <id> in a profile (all in this repo,
with claude-provider holding the keys). Use claude or cl <name> instead to take
it back to Anthropic.

Each resume command ends with a comment showing what's left of its profile's
plan limits (the Claude plan, also on a session that resumes on Z.ai or
Kimi), as a % and a bar, and after ↻ a bar of the time left until each one
resets and that time: the 5-hour session (5h), the weekly limit (week) and
per-model weekly limits such as Fable. Bars have a block per hour of the 5-hour
window and per day of a weekly one, as in ↻██▌░░░░ 2d12h. Time bars
are coloured by urgency: blue with most of the window to go, then gold and
orange, red as the reset nears. A plan bar shorter than its time bar means that
limit is being used up faster than its window runs out. A command without the
comment uses the same account as the one above it, even through another
profile, so its usage is the same. The shell ignores the comment if you paste
the whole line (zsh needs setopt interactivecomments). The numbers come from
claude-usage.py, which this script loads, so keep the two in one folder. When
an account can't be asked live, Claude Code's last saved snapshot is used and
listed at the bottom with its age; ? marks a limit that has reset since that
snapshot.

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
SHORTCUT = "cl"  # cl <name> ... runs claude with ~/.claude-<name> (./cl, or a shell function)


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


def windows_start_time(pid):
    """When a Windows process started, as the FILETIME number Claude Code records
    (procStartFt), or None when it isn't running."""
    import ctypes
    from ctypes import wintypes
    kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
    kernel32.OpenProcess.restype = wintypes.HANDLE
    kernel32.OpenProcess.argtypes = (wintypes.DWORD, wintypes.BOOL, wintypes.DWORD)
    kernel32.GetProcessTimes.argtypes = (wintypes.HANDLE,) + (ctypes.POINTER(wintypes.FILETIME),) * 4
    kernel32.GetExitCodeProcess.argtypes = (wintypes.HANDLE, ctypes.POINTER(wintypes.DWORD))
    kernel32.CloseHandle.argtypes = (wintypes.HANDLE,)
    handle = kernel32.OpenProcess(0x1000, False, pid)  # PROCESS_QUERY_LIMITED_INFORMATION
    if not handle:
        return None
    try:
        created, exited, kernel, user = (wintypes.FILETIME() for _ in range(4))
        code = wintypes.DWORD()
        if not kernel32.GetProcessTimes(handle, ctypes.byref(created), ctypes.byref(exited),
                                        ctypes.byref(kernel), ctypes.byref(user)):
            return None
        if kernel32.GetExitCodeProcess(handle, ctypes.byref(code)) and code.value != 259:  # STILL_ACTIVE
            return None
        return created.dwHighDateTime << 32 | created.dwLowDateTime
    finally:
        kernel32.CloseHandle(handle)


def open_sessions(profiles):
    """(profile, sessionId) -> registry entry for sessions whose claude process is alive.

    Claude Code writes <profile>/sessions/<pid>.json with when the process started,
    which guards against reused PIDs: as `ps -o lstart` prints it in UTC (procStart),
    or on Windows as a FILETIME number (procStartFt).
    """
    started = {}
    if os.name != "nt":
        try:
            out = subprocess.run(["ps", "-A", "-o", "pid=,lstart="], capture_output=True, text=True,
                                 env=dict(os.environ, TZ="UTC", LC_ALL="C")).stdout
        except OSError:
            out = ""
        for line in out.splitlines():
            pid, _, lstart = line.strip().partition(" ")
            started[pid] = " ".join(lstart.split())
    live = {}
    for profile in profiles:
        for path in glob.glob(os.path.join(profile, "sessions", "*.json")):
            try:
                with open(path, encoding="utf-8") as fh:
                    entry = json.load(fh)
            except (OSError, ValueError):
                continue
            pid = str(entry.get("pid"))
            if not entry.get("sessionId"):
                continue
            if os.name == "nt":
                ft = windows_start_time(int(pid)) if pid.isdigit() else None
                alive = ft is not None and str(entry.get("procStartFt") or ft) == str(ft)
            else:
                proc_start = " ".join(str(entry.get("procStart") or "").split())
                alive = pid in started and (not proc_start or proc_start == started[pid])
            if alive:
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


def last_model(mm):
    """The model behind the newest reply, skipping Claude Code's own <synthetic> ones."""
    for line in lines_backward(mm):
        if b'"type":"assistant"' not in line or b'"model":"' not in line:
            continue
        try:
            record = json.loads(line)
        except ValueError:
            continue
        model = (record.get("message") or {}).get("model")
        if record.get("type") == "assistant" and model and model != "<synthetic>" \
                and not record.get("isSidechain"):
            return model
    return None


def provider_of(model):
    """z or k when the model is served by Z.ai (GLM) or Kimi, else None."""
    model = (model or "").lower()
    if model.startswith("glm"):
        return "z"
    if model.startswith(("k2", "k3", "kimi")):
        return "k"
    return None


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
        model = last_model(mm)
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
        "model": model,
    }


def powershell():
    """Whether to print commands for PowerShell: on Windows, unless this runs in a POSIX
    shell such as Git Bash, which sets SHELL."""
    return os.name == "nt" and not os.environ.get("SHELL")


PS_SAFE = re.compile(r"[A-Za-z0-9._~:\\/-]+")  # needs no quotes in a PowerShell command


def ps_quote(text):
    return text if PS_SAFE.fullmatch(text) and not text.startswith("-") else "'" + text.replace("'", "''") + "'"


def shell_path(path):
    """path to print in a command, as ~/... when it's in the home folder."""
    if path == HOME:
        return "~"
    rest = path[len(HOME) + 1:] if path.startswith(HOME + os.sep) else None
    if powershell():
        # PowerShell only expands a ~ that isn't quoted.
        return "~\\" + rest if rest is not None and PS_SAFE.fullmatch(rest) else ps_quote(path)
    if os.sep != "/":  # a POSIX shell on Windows reads forward slashes
        rest, path = rest and rest.replace(os.sep, "/"), path.replace(os.sep, "/")
    return "~/" + shlex.quote(rest) if rest is not None else shlex.quote(path)


def resume_command(profile, session_id, full, provider=None):
    """The command that resumes a session, on Z.ai or Kimi when provider is z or k."""
    command = "claude-" + provider if provider else "claude"  # claude-z and claude-k: see ./cl
    if os.path.realpath(profile) == DEFAULT_PROFILE:
        return "%s --resume %s" % (command, session_id)
    parent, name = os.path.split(profile.rstrip("/" + os.sep))
    if not full and parent == HOME and name.startswith(".claude-"):
        short = name[len(".claude-"):]
        return "%s %s%s --resume %s" % (SHORTCUT, "-%s " % provider if provider else "",
                                         ps_quote(short) if powershell() else shlex.quote(short), session_id)
    if powershell():
        # Set for this one command and cleared after, so plain claude in the same window
        # goes back to ~\.claude.
        rest = profile[len(HOME) + 1:] if profile.startswith(HOME + os.sep) else None
        value = '"$HOME\\%s"' % rest if rest is not None and not re.search(r'["$`]', rest) \
            else "'" + profile.replace("'", "''") + "'"
        return "$env:CLAUDE_CONFIG_DIR=%s; %s --resume %s; $env:CLAUDE_CONFIG_DIR=$null" % (value, command, session_id)
    return "CLAUDE_CONFIG_DIR=%s %s --resume %s" % (shell_path(profile), command, session_id)


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
    """profile -> text: each plan limit's short name, % left and a bar of it, then ↻, a
    bar of the time left until it resets and that time, coloured by urgency, in columns
    aligned across profiles. Bars have a block per hour of the 5-hour window, or per
    day of a weekly one."""
    meters = {}
    for profile, (usage, saved_at, _) in results.items():
        meters[profile] = []
        for row in (usage or {}).get("limits") or []:
            left = usage_mod.percent_left(row.get("percent"), row.get("resets_at"), saved_at, now)
            reset = usage_mod.parse_time(row.get("resets_at"))
            window = usage_mod.window_seconds(row.get("group") or row.get("kind"))
            blocks = (window // 86400 or window // 3600) if window else 5
            if left is None:  # reset since the snapshot
                pct, code, plan, until, until_code = "?", "2", "·" * blocks, "", ""
            else:
                pct, code = "%d%%" % left, usage_mod.SEVERITY_COLOR.get(row.get("severity"), "")
                plan, until, until_code = usage_mod.bar(left / 100.0, blocks), "", ""
                if reset is not None:
                    # Coloured by urgency; just the time when there's no window to draw a bar for.
                    to_go = (reset - now) / window if window else None  # fraction of the window left
                    time_bar = usage_mod.bar(to_go, blocks) + " " if window else ""
                    until = "↻" + time_bar + usage_mod.duration(reset - now)
                    until_code = usage_mod.urgency_color(to_go) if window else "2"
            meters[profile].append((usage_mod.row_label(row, short=True), pct, code, plan, until, until_code))
    widths = {}  # meter slot -> widest [label, % left, plan bar, time bar] in it
    for rows in meters.values():
        for i, (label, pct, _, plan, until, _) in enumerate(rows):
            w = widths.setdefault(i, [0, 0, 0, 0])
            w[:] = [max(n, len(text)) for n, text in zip(w, (label, pct, plan, until))]

    def padded(code, text, width, right=False):
        gap = " " * (width - len(text))
        text = paint(code, text) if text else ""
        return gap + text if right else text + gap

    cells = {}
    for profile, rows in meters.items():
        parts = []
        for i, (label, pct, code, plan, until, until_code) in enumerate(rows):
            label_w, pct_w, plan_w, until_w = widths[i]
            # Padded so the next meter lines up; the trailing gap is cut below.
            parts.append(" ".join((padded("2", label, label_w), padded(code, pct, pct_w, right=True),
                                   padded(code, plan, plan_w), padded(until_code, until, until_w))))
        cells[profile] = "  ".join(parts).rstrip() if parts else paint("2", "usage ?")
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
    parser.add_argument("--all-folders", action="store_true",
                        help="list every folder, even when run from a folder that has sessions")
    parser.add_argument("--full", action="store_true",
                        help="print CLAUDE_CONFIG_DIR=... claude instead of the %s shortcut" % SHORTCUT)
    parser.add_argument("--cached", action="store_true",
                        help="take plan usage from the snapshot Claude Code last saved instead of the API")
    parser.add_argument("--no-usage", action="store_true", help="leave plan usage out")
    args = parser.parse_args()

    usage_lib = load_usage_module()
    ansi = usage_lib.setup_console() if usage_lib else True  # UTF-8 and colours on Windows
    color = sys.stdout.isatty() and not os.environ.get("NO_COLOR") and ansi
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
    for _, s in found.values():
        if s["entrypoint"] != "cli" and not args.all:
            label = "Desktop" if s["entrypoint"] == "claude-desktop" else s["entrypoint"]
            hidden[label] = hidden.get(label, 0) + 1
            continue
        shown.append(s)
    # Run from a folder that has sessions, list only that folder's.
    try:
        here = os.path.realpath(os.getcwd())
    except OSError:  # the current directory was deleted
        here = None
    mine = [s for s in shown if s["cwd"] and os.path.realpath(s["cwd"]) == here]
    only_here = bool(mine) and not args.all_folders
    elsewhere = len(shown) - len(mine) if only_here else 0
    if only_here:
        shown = mine
    per_profile = dict.fromkeys(profiles, 0)
    for s in shown:
        per_profile[s["profile"]] += 1

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
    usage_mod = None if args.no_usage else usage_lib
    used_profiles = [p for p in profiles if any(s["profile"] == p for _, listed, _ in folders for s in listed)]
    results, account = {}, {}  # profile -> its account's usage, and a number for that account
    if usage_mod:
        for n, (entry, result) in enumerate(usage_mod.load_usage(used_profiles, args.cached)):
            for profile in entry["profiles"]:
                results[profile], account[profile] = result, n
    cells = usage_cells(usage_mod, results, now, paint) if results else {}
    # The usage comments line up after the longest resume command.
    command_width = max([len(resume_command(s["profile"], s["id"], args.full, provider_of(s["model"])))
                         for _, listed, _ in folders for s in listed] or [0])

    window = ("%g" % args.hours) + "h"
    print(paint("1", "Claude Code CLI sessions %sactive in the last %s or open now"
                % ("in this folder, " if only_here else "", window))
          + paint("2", " (%snewest first%s)" % ("" if only_here else "by folder, ",
                                                "; % = plan left, ↻ = hours/days until reset" if cells else "")))
    if not shown:
        print("  No sessions in this window. Try a longer one, e.g. %s 72" % os.path.basename(sys.argv[0]))

    previous = None  # (account, usage text) of the session row above, across folders
    for folder, listed, older in folders:
        missing = "" if os.path.isdir(folder) else "  " + paint("31", "(directory missing)")
        print(paint("1;36", "cd " + shell_path(folder)) + missing)
        for s in listed:
            # No profile here: the command shows it (plain `claude` means ~/.claude).
            meta = [(s["branch"], "2")] if s["branch"] else []
            if s["model"] and not s["model"].startswith("claude"):
                meta.append(("on " + s["model"], "35"))  # e.g. GLM or Kimi, through claude-z or claude-k
            if s["entrypoint"] != "cli":
                meta.append(("via " + s["entrypoint"], "33"))
            if s["open"]:
                entry = s["open"]
                meta.append(("● open in pid %s" % entry.get("pid")
                             + (" (%s)" % entry["status"] if entry.get("status") else ""), "32"))
            if s["continued_in"]:
                meta.append(("continued in " + s["continued_in"][:8], "2"))
            room = width - 8 - sum(3 + len(text) for text, _ in meta)
            print("  %s  %s%s" % (paint("2", ago(now - s["when"]).rjust(4)),
                                  paint("1", truncate(s["title"], max(room, 20))),
                                  "".join(paint("2", " · ") + paint(c, text) for text, c in meta)))
            command = resume_command(s["profile"], s["id"], args.full, provider_of(s["model"]))
            usage = cells.get(s["profile"])
            # Left out when the session above uses the same account, even through another
            # profile, so it would only repeat.
            if usage and (account.get(s["profile"]), usage) != previous:
                # A shell comment, so pasting the whole line still runs just the command.
                command = command.ljust(command_width) + "  " + paint("2", "#") + " " + usage
            previous = (account.get(s["profile"]), usage)
            print(" " * 8 + command)
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
    if elsewhere:
        notes.append("%d in other folders (show with --all-folders)" % elsewhere)
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
