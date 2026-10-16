#!/usr/bin/env python3
"""List recent Claude Code CLI sessions from every ~/.claude* profile, and Codex
CLI sessions from every ~/.codex* home, with the command that resumes each one
and how much of its account's plan is left.

Usage:
    hatcheck [sessions] [HOURS] [-n N] [--all] [--all-folders] [--full]
                        [--cached] [--no-usage] [--json [--email]]

  HOURS         : how far back to look (default 24). Accepts 36, 1.5, 12h or 3d.
  -n N          : show only the newest N sessions per folder (default: all).
  --all         : also list sessions last used from Claude Desktop's Code tab or
                  the SDK (claude -p, apps driving Claude Code), and Codex
                  sessions from the Codex app, its IDE extension or codex exec,
                  hidden by default.
  --all-folders : list every folder, even when run from one that has sessions.
  --full        : print CLAUDE_CONFIG_DIR=~/.claude-<name> claude ... instead of
                  the cl shortcut (CODEX_HOME=~/.codex-<name> codex ... instead
                  of cx), for machines without them.
  --cached      : take plan usage from the snapshots Claude Code and Codex last
                  saved instead of asking the APIs.
  --no-usage    : leave plan usage out.
  --json        : print the sessions, and the accounts behind them, as JSON: the
                  same sessions in the same order, with absolute paths and times
                  in UTC. --email adds each account's email address.

Run from a folder that has sessions in the time window, it lists just that
folder's; anywhere else it lists every folder. Output is grouped by folder
(A-Z), each headed by the cd command to run first, with the newest session
first within each folder. Sessions in a
~/.claude-<name> profile resume with `cl <name> --resume <id>`. cl is in this
repo (install.sh puts it on your PATH) and works like this shell function:

    cl() { CLAUDE_CONFIG_DIR="$HOME/.claude-$1" claude "${@:2}"; }

Codex sessions resume with `codex resume <id>` in ~/.codex, and with
`cx <name> resume <id>` in a ~/.codex-<name> home; cx is in this repo too and
works like CODEX_HOME="$HOME/.codex-$1" codex "${@:2}". They're marked codex,
and their comment shows the ChatGPT plan's Codex limits.

A session whose newest reply came from Z.ai (a GLM model) or Kimi shows that
model, as in "on k3", and resumes there: claude-z or claude-k --resume <id> in
~/.claude, cl -z or cl -k <name> --resume <id> in a profile (all in this repo,
with hatcheck provider holding the keys). Use claude or cl <name> instead to take
it back to Anthropic.

Each resume command ends with a comment showing what's left of its profile's
plan limits (the Claude plan, also on a session that resumes on Z.ai or
Kimi), as a % and a bar, and after ↻ a bar of the time left until each one
resets and that time: the 5-hour session (5h), the weekly limit (week) and
per-model weekly limits such as Fable, named in violet, with their bar drawn in it,
to tell them from week.
Bars have a block per hour of the 5-hour window and per day of a weekly one,
as in ↻██▌░░░░ 2d12h. Time bars are coloured by urgency: blue with most of the
window to go, then gold and orange, red as the reset nears. A plan bar shorter than its time bar means that
limit is being used up faster than its window runs out. A command without the
comment uses the same account as the one above it, even through another
profile, so its usage is the same. The shell ignores the comment if you paste
the whole line (zsh needs setopt interactivecomments). The numbers come from
usage.py, which this script loads, so keep the two in one folder. When
an account can't be asked live, Claude Code's last saved snapshot is used and
listed at the bottom with its age; ? marks a limit that has reset since that
snapshot.

A profile is any ~/.claude* directory (plus $CLAUDE_CONFIG_DIR) that holds a
projects/ folder, so Desktop data dirs like ~/.claude-instances are skipped; a
Codex home is ~/.codex or any ~/.codex-* directory (plus $CODEX_HOME) that holds
a sessions/ folder. Codex sessions it runs itself, such as approval reviews,
aren't listed.
Each transcript records the entrypoint that wrote it; a session counts as
Desktop when its most recent activity came from Claude Desktop. Sessions still
open in a terminal are always listed, even when idle longer than HOURS, and
marked so you don't resume them twice. A session that ran in a folder tied to
another profile with hatcheck bind is marked "⚠ folder bound to ...".
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
from datetime import datetime, timezone

HOME = os.path.expanduser("~")
DEFAULT_PROFILE = os.path.realpath(os.path.join(HOME, ".claude"))
DEFAULT_HOURS = 24
SHORTCUT = "cl"  # cl <name> ... runs claude with ~/.claude-<name> (./cl, or a shell function)
DEFAULT_CODEX_HOME = os.path.realpath(os.path.join(HOME, ".codex"))
CODEX_SHORTCUT = "cx"  # cx <name> ... runs codex with ~/.codex-<name> (./cx)
# rollout-<local start time>-<session ID>.jsonl, Codex's name for a session file.
CODEX_FILE = re.compile(r"rollout-\d{4}-\d\d-\d\dT\d\d-\d\d-\d\d-"
                        r"([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})\.jsonl$")
# How messages that Codex adds to a conversation itself start: the environment, AGENTS.md,
# goals, interruptions, approval reviews.
CODEX_HARNESS = ("<environment_context>", "# AGENTS.md instructions", "<user_instructions>", "<goal_context>",
                 "<turn_aborted>", "The following is the Codex agent history", "<permissions instructions>",
                 "<recommended_plugins>")


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


def find_codex_homes():
    """Every Codex home: ~/.codex and ~/.codex-* dirs (and $CODEX_HOME) with a sessions/ folder."""
    candidates = glob.glob(os.path.join(HOME, ".codex")) + glob.glob(os.path.join(HOME, ".codex-*"))
    if os.environ.get("CODEX_HOME"):
        candidates.append(os.path.expanduser(os.environ["CODEX_HOME"]))
    homes = {}
    for d in candidates:
        if os.path.isdir(os.path.join(d, "sessions")):
            homes.setdefault(os.path.realpath(d), os.path.abspath(d))
    return sorted(homes.values(), key=lambda d: (os.path.realpath(d) != DEFAULT_CODEX_HOME, d))


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


def codex_session_files(home):
    """The session files of a Codex home, archived ones left out."""
    return [path for path in glob.glob(os.path.join(home, "sessions", "*", "*", "*", "rollout-*.jsonl"))
            if CODEX_FILE.search(path)]


def codex_names(home):
    """Session ID -> the name it was given with /rename or by Codex, from session_index.jsonl."""
    names = {}
    try:
        with open(os.path.join(home, "session_index.jsonl"), encoding="utf-8") as fh:
            for line in fh:
                try:
                    entry = json.loads(line)
                except ValueError:
                    continue
                if isinstance(entry, dict) and entry.get("id") and entry.get("thread_name"):
                    names[entry["id"]] = entry["thread_name"]  # the newest wins
    except (OSError, UnicodeError):
        pass
    return names


def codex_text(content):
    return "".join(c.get("text") or "" for c in content or []
                   if isinstance(c, dict) and c.get("type") in ("input_text", "output_text", "text"))


def codex_prompt(record):
    """What you typed, from a Codex record holding one of your messages, or None (also for
    the messages Codex adds itself)."""
    payload = record.get("payload") if isinstance(record.get("payload"), dict) else {}
    item = payload.get("item") if isinstance(payload.get("item"), dict) else {}
    kind, text = (record.get("type"), payload.get("type")), None
    if kind == ("event_msg", "item_completed") and item.get("type") == "UserMessage":
        text = codex_text(item.get("content"))
    elif kind == ("event_msg", "user_message"):
        text = payload.get("message")
    elif kind == ("response_item", "message") and payload.get("role") == "user":
        text = codex_text(payload.get("content"))
    if isinstance(text, str) and text.strip() and not text.lstrip().startswith(CODEX_HARNESS):
        return text
    return None


def codex_entrypoint(meta):
    """Where a Codex session was started from: cli for the terminal, as Claude Code's
    entrypoint says, or the app, IDE extension or codex exec; subagent for one Codex runs."""
    source = meta.get("source")
    if isinstance(source, dict) or meta.get("thread_source") == "subagent":
        return "subagent"
    if source == "cli":
        return "cli"
    if meta.get("originator") == "Codex Desktop":
        return "Codex app"
    return {"vscode": "Codex IDE", "exec": "codex exec"}.get(source, str(source or "?"))


def read_codex_session(path, names):
    """Summary of one Codex session file, like read_session's, or None when it holds no
    conversation."""
    with open(path, "rb") as fh:
        size = os.fstat(fh.fileno()).st_size
        if size == 0:
            return None
        mm = mmap.mmap(fh.fileno(), 0, access=mmap.ACCESS_READ)
    try:
        try:
            first = json.loads(mm.readline())
        except ValueError:
            return None
        meta = first.get("payload") if isinstance(first, dict) and first.get("type") == "session_meta" else None
        if not isinstance(meta, dict):
            return None
        last = None  # the newest record: the last activity
        for line in lines_backward(mm):
            try:
                record = json.loads(line)
            except ValueError:
                continue
            if isinstance(record, dict) and parse_time(record.get("timestamp")):
                last = record
                break
        context = last_record(mm, "turn_context") or {}
        prompt = title = None
        mm.seek(0)
        for n, line in enumerate(iter(mm.readline, b"")):
            if title or n > 5000:
                break
            if not any(k in line for k in (b'"UserMessage"', b'"user_message"', b'"role":"user"')):
                continue
            try:
                record = json.loads(line)
            except ValueError:
                continue
            text = codex_prompt(record) if isinstance(record, dict) else None
            prompt = prompt or text
            title = text if text and not text.lstrip().startswith("<") else None
    finally:
        mm.close()
    match = CODEX_FILE.search(path)
    sid = meta.get("id") or (match.group(1) if match else os.path.basename(path))
    cwd = meta.get("cwd")
    return {
        "agent": "codex",
        "id": sid,
        "when": parse_time((last or first).get("timestamp")) or os.path.getmtime(path),
        "entrypoint": codex_entrypoint(meta),
        "branch": (meta.get("git") or {}).get("branch") if isinstance(meta.get("git"), dict) else None,
        "cwd": cwd,
        "canonical": True,
        "size": size,
        "has_messages": prompt is not None,
        "title": " ".join((names.get(sid) or title or prompt or "(untitled)").split()),
        "continued_in": None,
        "model": ((context.get("payload") or {}) if isinstance(context.get("payload"), dict) else {}).get("model"),
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
    return claude_command(profile, "--resume " + session_id, full, provider)


def claude_command(profile, tail, full, provider=None):
    """The command that runs Claude Code with a profile, then tail: claude for ~/.claude,
    cl <name> for ~/.claude-<name>, claude-z or claude-k on Z.ai or Kimi."""
    command = "claude-" + provider if provider else "claude"  # claude-z and claude-k: see ./cl
    tail = " " + tail if tail else ""
    if os.path.realpath(profile) == DEFAULT_PROFILE:
        return command + tail
    parent, name = os.path.split(profile.rstrip("/" + os.sep))
    if not full and parent == HOME and name.startswith(".claude-"):
        short = name[len(".claude-"):]
        return "%s %s%s%s" % (SHORTCUT, "-%s " % provider if provider else "",
                              ps_quote(short) if powershell() else shlex.quote(short), tail)
    if powershell():
        # Set for this one command and cleared after, so plain claude in the same window
        # goes back to ~\.claude.
        rest = profile[len(HOME) + 1:] if profile.startswith(HOME + os.sep) else None
        value = '"$HOME\\%s"' % rest if rest is not None and not re.search(r'["$`]', rest) \
            else "'" + profile.replace("'", "''") + "'"
        return "$env:CLAUDE_CONFIG_DIR=%s; %s%s; $env:CLAUDE_CONFIG_DIR=$null" % (value, command, tail)
    return "CLAUDE_CONFIG_DIR=%s %s%s" % (shell_path(profile), command, tail)


def codex_resume_command(home, session_id, full):
    """The command that resumes a Codex session: codex resume <id>, with cx <name> for a
    ~/.codex-<name> home."""
    return codex_command(home, "resume " + session_id, full)


def codex_command(home, tail, full):
    """The command that runs Codex with a home, then tail: codex for ~/.codex, cx <name>
    for ~/.codex-<name>."""
    tail = " " + tail if tail else ""
    if os.path.realpath(home) == DEFAULT_CODEX_HOME:
        return "codex" + tail
    parent, name = os.path.split(home.rstrip("/" + os.sep))
    if not full and parent == HOME and name.startswith(".codex-"):
        short = name[len(".codex-"):]
        return "%s %s%s" % (CODEX_SHORTCUT, ps_quote(short) if powershell() else shlex.quote(short), tail)
    if powershell():
        rest = home[len(HOME) + 1:] if home.startswith(HOME + os.sep) else None
        value = '"$HOME\\%s"' % rest if rest is not None and not re.search(r'["$`]', rest) \
            else "'" + home.replace("'", "''") + "'"
        return "$env:CODEX_HOME=%s; codex%s; $env:CODEX_HOME=$null" % (value, tail)
    return "CODEX_HOME=%s codex%s" % (shell_path(home), tail)


def start_command(agent, profile, full=False):
    """The command that starts Claude Code or Codex with a profile or home."""
    return codex_command(profile, "", full) if agent == "codex" else claude_command(profile, "", full)


def iso_utc(t):
    """A time as ISO 8601 in UTC, for --json, or None."""
    return datetime.fromtimestamp(t, timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ") if t is not None else None


# Folder bindings (hatcheck bind): which profile and Codex home a folder's work belongs to.

def bindings_path():
    """~/.config/hatcheck/bindings.json (%APPDATA%\\hatcheck\\bindings.json on Windows):
    your own config, not the repos', so profile names don't end up in git."""
    base = os.environ.get("APPDATA") if os.name == "nt" else None
    return os.path.join(base or os.path.join(HOME, ".config"), "hatcheck", "bindings.json")


def load_bindings():
    """Bound folder -> {"claude": name, "codex": name}: default for ~/.claude or ~/.codex,
    a NAME for ~/.claude-NAME or ~/.codex-NAME, or a path."""
    try:
        with open(bindings_path(), encoding="utf-8") as fh:
            data = json.load(fh)
    except (OSError, ValueError):
        return {}
    folders = data.get("folders") if isinstance(data, dict) else None
    return {folder: b for folder, b in (folders or {}).items() if isinstance(b, dict)} \
        if isinstance(folders, dict) else {}


def profile_folder(agent, name):
    """The folder a bound name stands for: default for ~/.claude or ~/.codex, NAME for
    ~/.claude-NAME or ~/.codex-NAME, or a path as it is."""
    base = ".codex" if agent == "codex" else ".claude"
    if name == "default":
        return os.path.join(HOME, base)
    if "/" in name or os.sep in name:
        return name
    return os.path.join(HOME, base + "-" + name)


def bound_folder(path, bindings):
    """The deepest bound folder that holds path, path itself included, or None. Folders
    are compared once symlinks are resolved."""
    if not path or not bindings:
        return None
    keys = {os.path.normcase(os.path.realpath(folder)): folder for folder in bindings}
    path = os.path.normcase(os.path.realpath(path))
    while True:
        if path in keys:
            return keys[path]
        parent = os.path.dirname(path)
        if parent == path:
            return None
        path = parent


def bound_profile(path, agent, bindings):
    """The profile (or Codex home) the folder at path is bound to, or None."""
    folder = bound_folder(path, bindings)
    name = bindings[folder].get(agent) if folder else None
    return profile_folder(agent, name) if isinstance(name, str) and name else None


def same_folder(a, b):
    return os.path.normcase(os.path.realpath(a)) == os.path.normcase(os.path.realpath(b))


# Finding sessions, for hatcheck open and hatcheck best.

def summarize(agent, profile, path):
    """read_session or read_codex_session of a file, with its agent, profile and file
    added, or None."""
    try:
        s = read_codex_session(path, codex_names(profile)) if agent == "codex" else read_session(path)
    except (OSError, ValueError):
        return None
    if s:
        s.update(agent=agent, profile=profile, path=path)
        s.setdefault("open", None)
    return s


def session_copies(prefix):
    """(agent, session ID) -> [(profile or home, file)] for the sessions whose ID starts
    with prefix, in every Claude Code profile and Codex home, archived Codex ones too."""
    found = {}
    for profile in find_profiles():
        for path in sorted(glob.glob(os.path.join(profile, "projects", "*", glob.escape(prefix) + "*.jsonl"))):
            found.setdefault(("claude", os.path.basename(path)[:-len(".jsonl")]), []).append((profile, path))
    for home in find_codex_homes():
        paths = codex_session_files(home) + glob.glob(os.path.join(home, "archived_sessions", "rollout-*.jsonl"))
        for path in sorted(paths):
            m = CODEX_FILE.search(path)
            if m and m.group(1).startswith(prefix.lower()):
                found.setdefault(("codex", m.group(1)), []).append((home, path))
    return found


def codex_cwd(path):
    """The folder a Codex session started in, from its first record."""
    try:
        with open(path, encoding="utf-8", errors="replace") as fh:
            record = json.loads(fh.readline())
    except (OSError, ValueError):
        return None
    payload = record.get("payload") if isinstance(record, dict) else None
    return payload.get("cwd") if isinstance(payload, dict) else None


def newest_here(folder, everything=False):
    """The newest session started from folder, in any profile or home, or None. Only
    terminal sessions, as the list shows by default, unless everything."""
    here, best = os.path.realpath(folder), None

    def better(s):
        return s and s["has_messages"] and s["cwd"] and same_folder(s["cwd"], here) \
            and s["entrypoint"] != "subagent" and (everything or s["entrypoint"] == "cli") \
            and (best is None or s["when"] > best["when"])
    names = {encode_project(p) for p in (folder, here)}
    for profile in find_profiles():
        for name in names:
            pattern = glob.escape(name[:200]) + "*" if len(name) > 200 else glob.escape(name)
            for path in glob.glob(os.path.join(profile, "projects", pattern, "*.jsonl")):
                s = summarize("claude", profile, path)
                best = s if better(s) else best
    for home in find_codex_homes():
        for path in sorted(codex_session_files(home), key=os.path.getmtime, reverse=True):
            if best and os.path.getmtime(path) < best["when"]:
                break  # a file's mtime is never older than its last message
            cwd = codex_cwd(path)
            if cwd and same_folder(cwd, here):
                s = summarize("codex", home, path)
                if better(s):
                    best = s
                    break
    return best


def codex_session_open(home, sid):
    """Whether Codex has a session open: it holds a lock on thread-writer-locks/<id>.lock
    while it's writing the session. Not checked on Windows."""
    path = os.path.join(home, "thread-writer-locks", sid + ".lock")
    if os.name == "nt" or not os.path.isfile(path):
        return False
    import fcntl
    try:
        fd = os.open(path, os.O_RDWR)
    except OSError:
        return False
    try:
        fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
        fcntl.flock(fd, fcntl.LOCK_UN)
        return False
    except OSError:
        return True
    finally:
        os.close(fd)


def session_command(s, full):
    """The command that resumes a listed session, Claude Code's or Codex's."""
    if s.get("agent") == "codex":
        return codex_resume_command(s["profile"], s["id"], full)
    return resume_command(s["profile"], s["id"], full, provider_of(s["model"]))


def load_usage_module():
    """usage.py from this script's folder, loaded as a module, or None if it's missing."""
    path = os.path.join(os.path.dirname(os.path.realpath(__file__)), "usage.py")
    if not os.path.isfile(path):
        return None
    sys.dont_write_bytecode = True  # leave no __pycache__ folder beside the scripts
    spec = importlib.util.spec_from_file_location("hatcheck_usage", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def usage_cells(usage_mod, results, now, paint):
    """profile -> text: each plan limit's short name (per-model ones in violet), % left
    and a bar of it, then ↻, a bar of the time left until it resets and that time,
    coloured by urgency, in columns aligned across profiles. Bars have a block per hour
    of the 5-hour window, per day of a weekly one, and 10 for Codex's 30 days."""
    meters = {}
    for profile, (usage, saved_at, _) in results.items():
        meters[profile] = []
        for row in (usage or {}).get("limits") or []:
            left = usage_mod.percent_left(row.get("percent"), row.get("resets_at"), saved_at, now)
            reset = usage_mod.parse_time(row.get("resets_at"))
            window = usage_mod.row_window(row)
            blocks = usage_mod.bar_cells(window)
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
            # A per-model limit's bar is violet like its name; its % keeps the severity's colour.
            meters[profile].append((usage_mod.row_label(row, short=True), usage_mod.label_color(row.get("kind"), "2"),
                                    pct, code, plan, usage_mod.label_color(row.get("kind"), code), until, until_code))
    widths = {}  # meter slot -> widest [label, % left, plan bar, time bar] in it
    for rows in meters.values():
        for i, (label, _, pct, _, plan, _, until, _) in enumerate(rows):
            w = widths.setdefault(i, [0, 0, 0, 0])
            w[:] = [max(n, len(text)) for n, text in zip(w, (label, pct, plan, until))]

    def padded(code, text, width, right=False):
        gap = " " * (width - len(text))
        text = paint(code, text) if text else ""
        return gap + text if right else text + gap

    cells = {}
    for profile, rows in meters.items():
        parts = []
        for i, (label, label_code, pct, code, plan, plan_code, until, until_code) in enumerate(rows):
            label_w, pct_w, plan_w, until_w = widths[i]
            # Padded so the next meter lines up; the trailing gap is cut below.
            parts.append(" ".join((padded(label_code, label, label_w), padded(code, pct, pct_w, right=True),
                                   padded(plan_code, plan, plan_w), padded(until_code, until, until_w))))
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


def sessions_json(args, now, folders, loaded, account, hidden, elsewhere, only_here, usage_mod):
    """The list as --json gives it: the same sessions in the same order, with the accounts
    behind them. Paths are absolute and times are in UTC; version goes up only when a field
    changes meaning or goes away."""
    out = {"version": 1, "generated_at": iso_utc(now), "hours": args.hours, "this_folder_only": only_here,
           "sessions": [], "accounts": [], "hidden": hidden, "other_folders": elsewhere,
           "older_not_listed": {folder: older for folder, _, older in folders if older}}
    for _, listed, _ in folders:
        for s in listed:
            live = s.get("open")
            out["sessions"].append({
                "agent": s.get("agent") or "claude", "id": s["id"], "profile": os.path.abspath(s["profile"]),
                "account": account.get(s["profile"]), "title": s["title"], "cwd": s["cwd"],
                "last_active": iso_utc(s["when"]), "entrypoint": s["entrypoint"], "branch": s["branch"],
                "model": s["model"],
                "open": {"pid": live.get("pid"), "status": live.get("status")} if live else None,
                "continued_in": s["continued_in"],
                "bound_to": os.path.abspath(s["bound_to"]) if s["bound_to"] else None,
                "resume": session_command(s, args.full)})
    if loaded:
        out["accounts"] = [usage_mod.account_json(entry, result, now, args.email) for entry, result in loaded]
    return out


def main(argv=None, prog=None):
    parser = argparse.ArgumentParser(
        prog=prog, description="List recent Claude Code CLI sessions across all ~/.claude* profiles, and Codex CLI "
                    "sessions across all ~/.codex* homes, with the command that resumes each one.")
    parser.add_argument("hours", nargs="?", type=parse_hours, default=DEFAULT_HOURS,
                        help="how far back to look (default %d); accepts 36, 1.5, 12h or 3d" % DEFAULT_HOURS)
    parser.add_argument("-n", "--per-folder", metavar="N", type=parse_count,
                        help="show only the newest N sessions per folder (default: all)")
    parser.add_argument("--all", action="store_true",
                        help="also list sessions last used from Claude Desktop or the SDK, and Codex "
                             "sessions from the Codex app, its IDE extension or codex exec")
    parser.add_argument("--all-folders", action="store_true",
                        help="list every folder, even when run from a folder that has sessions")
    parser.add_argument("--full", action="store_true",
                        help="print CLAUDE_CONFIG_DIR=... claude instead of the %s shortcut, and "
                             "CODEX_HOME=... codex instead of %s" % (SHORTCUT, CODEX_SHORTCUT))
    parser.add_argument("--cached", action="store_true",
                        help="take plan usage from the snapshots Claude Code and Codex last saved "
                             "instead of the APIs")
    parser.add_argument("--no-usage", action="store_true", help="leave plan usage out")
    parser.add_argument("--json", action="store_true",
                        help="print the sessions, and the accounts behind them, as JSON for scripts and apps")
    parser.add_argument("-e", "--email", action="store_true", help="with --json, include each account's email address")
    args = parser.parse_args(argv)

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
    codex_homes = find_codex_homes()
    for home in codex_homes:
        names = None  # read once a session in the window needs them
        for path in codex_session_files(home):
            try:
                if os.path.getmtime(path) < cutoff:
                    continue
                names = codex_names(home) if names is None else names
                session = read_codex_session(path, names)
            except (OSError, ValueError):
                unreadable += 1
                continue
            if session is not None and session["entrypoint"] == "subagent":
                continue  # one Codex ran itself, such as an approval review
            if session is None or not session["has_messages"]:
                empty += 1
                continue
            if session["when"] < cutoff:
                continue
            session["profile"], session["open"] = home, None
            found[("codex", os.path.realpath(home), session["id"])] = ((session["when"], True, session["size"]), session)

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
    per_profile = dict.fromkeys(profiles + codex_homes, 0)
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
    used = {s["profile"] for _, listed, _ in folders for s in listed}
    used_profiles, used_homes = [p for p in profiles if p in used], [h for h in codex_homes if h in used]
    results, account = {}, {}  # profile or home -> its account's usage, and a number for that account
    loaded = usage_mod.load_usage(used_profiles, args.cached, used_homes) if usage_mod else []
    for n, (entry, result) in enumerate(loaded):
        for profile in entry["profiles"]:
            results[profile], account[profile] = result, n
    bindings = load_bindings()
    for _, listed, _ in folders:
        for s in listed:
            want = bound_profile(s["cwd"], s.get("agent") or "claude", bindings)
            s["bound_to"] = want if want and not same_folder(want, s["profile"]) else None
    if args.json:
        print(json.dumps(sessions_json(args, now, folders, loaded, account, hidden, elsewhere, only_here, usage_mod),
                         ensure_ascii=False, indent=2))
        return
    cells = usage_cells(usage_mod, results, now, paint) if results else {}
    # The usage comments line up after the longest resume command.
    command_width = max([len(session_command(s, args.full)) for _, listed, _ in folders for s in listed] or [0])

    window = ("%g" % args.hours) + "h"
    print(paint("1", "%s CLI sessions %sactive in the last %s or open now"
                % ("Claude Code and Codex" if codex_homes else "Claude Code", "in this folder, " if only_here else "",
                   window))
          + paint("2", " (%snewest first%s)" % ("" if only_here else "by folder, ",
                                                "; % = plan left, ↻ = hours/days until reset" if cells else "")))
    if not shown:
        print("  No sessions in this window. Try a longer one, e.g. %s 72" % parser.prog)

    previous = None  # (account, usage text) of the session row above, across folders
    for folder, listed, older in folders:
        missing = "" if os.path.isdir(folder) else "  " + paint("31", "(directory missing)")
        print(paint("1;36", "cd " + shell_path(folder)) + missing)
        for s in listed:
            # No profile here: the command shows it (plain `claude` means ~/.claude).
            meta = [("codex", "36")] if s.get("agent") == "codex" else []
            meta += [(s["branch"], "2")] if s["branch"] else []
            if s.get("agent") != "codex" and s["model"] and not s["model"].startswith("claude"):
                meta.append(("on " + s["model"], "35"))  # e.g. GLM or Kimi, through claude-z or claude-k
            if s["entrypoint"] != "cli":
                meta.append(("via " + s["entrypoint"], "33"))
            if s["open"]:
                entry = s["open"]
                meta.append(("● open in pid %s" % entry.get("pid")
                             + (" (%s)" % entry["status"] if entry.get("status") else ""), "32"))
            if s["continued_in"]:
                meta.append(("continued in " + s["continued_in"][:8], "2"))
            if s["bound_to"]:  # hatcheck bind ties this folder to another profile
                meta.append(("⚠ folder bound to " + shell_path(s["bound_to"]), "33"))
            room = width - 8 - sum(3 + len(text) for text, _ in meta)
            print("  %s  %s%s" % (paint("2", ago(now - s["when"]).rjust(4)),
                                  paint("1", truncate(s["title"], max(room, 20))),
                                  "".join(paint("2", " · ") + paint(c, text) for text, c in meta)))
            command = session_command(s, args.full)
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
    for p in used_profiles + used_homes:
        if p in results:
            usage, saved_at, why = results[p]
            if usage is None:
                no_data.setdefault(why, []).append(shell_path(p))
            elif saved_at is not None:
                source = "Codex's session files" if p in used_homes else "Claude Code's last snapshot"
                snapshots.setdefault((source, why), []).append("%s %s ago" % (shell_path(p), ago(now - saved_at)))
    for (source, why), items in snapshots.items():
        print(paint("2", "Usage from %s (%s): %s" % (source, why, ", ".join(items))))
    for why, items in no_data.items():
        print(paint("2", "No usage data (%s): %s" % (why, ", ".join(items))))
    if used and not args.no_usage and not usage_mod:
        print(paint("2", "No usage data: usage.py isn't in this script's folder"))
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
