#!/usr/bin/env python3
"""Copy a session to another profile, to carry on with it there under that
profile's account and plan limits: a Claude Code session to another ~/.claude*
profile, a Codex session to another ~/.codex* home, or either one over to the
other agent.

Usage:
    hatcheck move SESSION PROFILE [--from PROFILE] [--no-keep] [--fork] [--dry-run] [--json]

  SESSION   : the session ID, or the start of it, of a Claude Code or Codex session.
  PROFILE   : where it goes: NAME for ~/.claude-NAME, default for ~/.claude,
              codex for ~/.codex, codex:NAME for ~/.codex-NAME, or a path.
  --from    : the profile or home to take it from, when more than one has it. Without it,
              the copies are listed and you pick one, the one used last being the default;
              a script or --json gets that one.
  --no-keep : delete the original once the copy is checked, which moves it.
              Not when the session is converted, since that leaves things out.
  --fork    : copy it in as a new session with its own ID, as claude --fork-session
              and codex fork do, leaving a copy already in PROFILE as it is.
  --dry-run : show what would be copied, converted and deleted, and change nothing.
  --json    : say so as JSON, for scripts and the desktop app: what was or would be
              done, item by item, and the command that resumes it. Never asks.

When PROFILE already has a copy of the session, from an earlier move, the two are
compared. If the session went on from the copy there, that copy is brought up to
date, so a session can go back and forth between profiles. If it went on there
instead, that copy already has everything and nothing is copied. If it went on in
both, nothing is copied: --fork brings this one in as a new session, keeping both.

Between Claude Code profiles, the transcript, the folder of subagent transcripts
and saved tool output, the /rewind file snapshots and the plan file are copied
with their timestamps, and every copied file is checked. Paths in the
conversation that point at those files are changed to point at the copies,
except inside Claude's thinking, whose signatures would stop matching. Plan
files stay in the old profile even with --no-keep, since sessions can share
one. If the new account rejects the signatures on earlier thinking, Claude
Code drops those thinking blocks and carries on.

Between Codex homes signed in to the same ChatGPT account, the session file is
copied as it is, with its ID and its name if it has one.

Otherwise the session is converted: between Claude Code and Codex, and between
Codex homes on different accounts, because Codex encrypts its reasoning and
its summaries of compacted context for the account that wrote them. The copy
gets a new ID and the original is always kept. Your messages and the replies
come across as text, and each tool call as a block of text with its input and
the start of its output, in the format Codex's own /import gives Claude Code
sessions ([external_agent_tool_call: Bash] ... [/external_agent_tool_call]).
Reasoning is left out, and so are images, plans, /rewind snapshots and
subagent transcripts. The whole conversation before a compaction is carried
over rather than its summary; past about 100,000 tokens the oldest turns are
left out, so it fits the new agent's context window. A note at the start tells
the model where the session came from.

A session that's open in a terminal is refused: quit it first, so it stops
changing. Resume it with the command printed at the end, from the directory it
ran in. In a terminal it then asks whether to do that now: y starts Claude
Code or Codex in the new profile, from that directory. The account and its
plan limits, settings, permissions, MCP servers, plugins and project memory
are the new profile's.

This script loads sessions.py, which loads usage.py, so keep the three in one
folder.
"""
import argparse
import contextlib
import glob
import importlib.util
import io
import json
import os
import re
import shlex
import shutil
import signal
import subprocess
import sys
import time
import uuid
from datetime import datetime, timezone

HOME = os.path.expanduser("~")
TMP = ".hatcheck-move-tmp"  # suffix of a copy in progress
# Converting: how much of each tool call's input and output to keep, as (lines, characters),
# and how much conversation to carry, in characters (about 100,000 tokens).
CALL_CLIP, RESULT_CLIP, BUDGET = (30, 3000), (20, 2000), 400000
# How messages that Claude Code adds to a conversation itself start: slash commands and
# their output, background task notices, reminders.
CLAUDE_HARNESS = ("<command-name>", "<command-message>", "<command-args>", "<local-command-stdout>",
                  "<local-command-stderr>", "<local-command-caveat>", "<task-notification>", "<system-reminder>")
AGENT_NAMES = {"claude": "Claude Code", "codex": "Codex"}
# Fields that hold a session's own ID, changed in a --fork copy: Claude Code's sessionId, and
# Codex's id and session_id (in session_meta) and thread_id.
ID_KEYS = ("sessionId", "session_id", "thread_id", "id")


def load_sibling(name, file):
    """A script from this one's folder, loaded as a module."""
    sys.dont_write_bytecode = True  # leave no __pycache__ folder beside the scripts
    path = os.path.join(os.path.dirname(os.path.realpath(__file__)), file)
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def place(spec):
    """(agent, folder) for a PROFILE argument: NAME for ~/.claude-NAME, default for ~/.claude,
    codex for ~/.codex, codex:NAME for ~/.codex-NAME (claude:NAME spells out a Claude Code
    one), or a path, which is a Codex home when it's named .codex* or has Codex's sessions/."""
    if spec in ("default", "claude:default"):
        return "claude", os.path.join(HOME, ".claude")
    if spec in ("codex", "codex:default"):
        return "codex", os.path.join(HOME, ".codex")
    for agent in ("claude", "codex"):
        if spec.startswith(agent + ":") and len(spec) > len(agent) + 1:
            return agent, os.path.join(HOME, ".%s-%s" % (agent, spec[len(agent) + 1:]))
    if "/" in spec or os.sep in spec or spec.startswith("~") or re.match(r"[A-Za-z]:", spec):
        path = os.path.abspath(os.path.expanduser(spec))
        codex = os.path.basename(path).startswith(".codex") or (
            os.path.isdir(os.path.join(path, "sessions")) and not os.path.isdir(os.path.join(path, "projects")))
        return ("codex" if codex else "claude"), path
    return "claude", os.path.join(HOME, ".claude-" + spec)


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


def repoint(value, pairs, ids=None):
    """value with each old path changed to its new one, and with ids, (old, new), the
    session's ID changed where a field holds it. Thinking blocks are left alone: their
    signatures cover the text."""
    if isinstance(value, str):
        for old, new in pairs:
            value = value.replace(old, new)
        return value
    if isinstance(value, list):
        return [repoint(v, pairs, ids) for v in value]
    if isinstance(value, dict):
        if value.get("type") in ("thinking", "redacted_thinking"):
            return value
        return {k: ids[1] if ids and k in ID_KEYS and v == ids[0] else repoint(v, pairs, ids)
                for k, v in value.items()}
    return value


def copy_bytes(src, pairs, ids=None):
    """What the copy of file src holds: its bytes, with a transcript's paths repointed and,
    for --fork, the session's ID changed. Only lines holding an old path or the ID are
    rewritten; Claude Code and Codex write compact JSON, so re-serializing one changes
    nothing else."""
    data = read(src)
    # As the paths appear in the JSON text: on Windows with their backslashes doubled.
    olds = [json.dumps(old, ensure_ascii=False)[1:-1].encode() for old, _ in pairs] + ([ids[0].encode()] if ids else [])
    if not src.endswith(".jsonl") or not any(old in data for old in olds):
        return data
    lines = data.split(b"\n")
    for i, line in enumerate(lines):
        if any(old in line for old in olds):
            try:
                lines[i] = json.dumps(repoint(json.loads(line), pairs, ids), ensure_ascii=False,
                                      separators=(",", ":")).encode()
            except (ValueError, UnicodeError):
                pass  # a line that doesn't round-trip stays as it was
    return b"\n".join(lines)


def matches(src, dst, pairs, ids=None):
    """Whether dst holds exactly what copying src gives."""
    if not os.path.exists(dst) or os.path.isdir(src) != os.path.isdir(dst):
        return False
    names = files_in(src)
    return names == files_in(dst) and all(read(join(dst, n)) == copy_bytes(join(src, n), pairs, ids) for n in names)


def lines_of(data):
    """A JSONL file's lines, without the empty one after the last newline."""
    lines = data.split(b"\n")
    return lines[:-1] if lines[-1] == b"" else lines


def same_line(a, b):
    """Whether two lines hold the same record, even if one was written out differently."""
    if a == b:
        return True
    try:
        return json.loads(a) == json.loads(b)
    except (ValueError, UnicodeError):
        return False


def shared_lines(a, b):
    """How many lines two JSONL files start with in common."""
    n = 0
    while n < min(len(a), len(b)) and same_line(a[n], b[n]):
        n += 1
    return n


def standing(have, want, jsonl, mtimes):
    """How a file already in the profile (have) stands to what copying would put there
    (want): same; older, when want starts with all of it, so the session went on from it;
    newer, when it starts with all of want, so the session went on there; or diverged."""
    if have == want:
        return "same"
    if jsonl:  # Claude Code and Codex only ever add lines to these
        a, b = lines_of(have), lines_of(want)
        n = shared_lines(a, b)
        if n == len(a) == len(b):
            return "same"
        return "older" if n == len(a) else "newer" if n == len(b) else "diverged"
    if want.startswith(have):
        return "older"
    if have.startswith(want):
        return "newer"
    # Written over rather than added to, such as a plan: the one changed last is newer.
    return "older" if mtimes[0] < mtimes[1] else "newer" if mtimes[0] > mtimes[1] else "diverged"


def overall(states):
    """What standings add up to: diverged once either side has something the other lacks
    and the other way round too. new (nothing there yet) counts as older."""
    found = {"older" if s == "new" else s for s in states} - {"same"}
    if "diverged" in found or {"older", "newer"} <= found:
        return "diverged"
    return "older" if "older" in found else "newer" if "newer" in found else "same"


def item_standing(src, dst, pairs):
    """How what's at dst stands to a copy of the file or folder src: new when there's
    nothing there, otherwise its files compared as standing() does."""
    if not os.path.lexists(dst):
        return "new"
    if not os.path.exists(src):
        return "newer"
    if os.path.isdir(src) != os.path.isdir(dst):
        return "diverged"
    ours, theirs = set(files_in(src)), set(files_in(dst))
    states = ["older"] * len(ours - theirs) + ["newer"] * len(theirs - ours)
    for name in ours & theirs:
        a, b = join(src, name), join(dst, name)
        states.append(standing(read(b), copy_bytes(a, pairs), b.endswith(".jsonl"),
                               (os.path.getmtime(b), os.path.getmtime(a))))
    return overall(states)


def records_of(lines):
    for line in lines:
        try:
            record = json.loads(line)
        except (ValueError, UnicodeError):
            continue
        if isinstance(record, dict):
            yield record


def since_split(ours, theirs, agent, sessions):
    """For two copies of a session that each went on: how many lines they share, when they
    were last the same, and how many prompts each has had since (this one's, the other's)."""
    a, b = lines_of(ours), lines_of(theirs)
    n = shared_lines(a, b)
    when = next((r["timestamp"] for r in reversed(list(records_of(a[:n]))) if r.get("timestamp")), None)

    return n, when, count_prompts(a[n:], agent, sessions), count_prompts(b[n:], agent, sessions)


def count_prompts(lines, agent, sessions):
    """How many prompts these lines of a session hold."""
    if agent == "codex":  # each prompt is in a user_message event, and again as a response item
        return sum(1 for r in records_of(lines) if r.get("type") == "event_msg" and sessions.codex_prompt(r))
    return sum(1 for r in records_of(lines) if r.get("type") == "user" and not r.get("isMeta")
               and not r.get("isSidechain") and sessions.prompt_text(r))


def last_time(data):
    """The time of a session file's last record that has one."""
    return next((r["timestamp"] for r in reversed(list(records_of(lines_of(data)))) if r.get("timestamp")), None)


def time_text(stamp, sessions):
    t = sessions.parse_time(stamp) if isinstance(stamp, str) else None
    return time.strftime("%b %d %H:%M", time.localtime(t)) if t else "an earlier move"


ask = input  # the prompt for which copy to take; tests replace it


def interactive():
    return sys.stdin.isatty() and sys.stdout.isatty()


def copies_used_last_first(where, agent, sessions):
    """The copies of one session, {folder: [files]}, as dicts, the one written last first:
    folder, files, file (the transcript), data, when (its last record's time, if it has one)
    and time (that, or the file's modification time)."""
    copies = []
    for folder, files in where.items():
        path = max(files, key=os.path.getmtime)
        data = read(path)
        when = last_time(data)
        t = sessions.parse_time(when) if when else None
        copies.append({"folder": folder, "files": files, "file": path, "data": data, "when": when,
                       "time": t if t else os.path.getmtime(path)})
    return sorted(copies, key=lambda c: -c["time"])


def copy_lines(copies, agent, sessions, show):
    """One line per copy: where, when it was last written, its prompts, and how it stands to
    the first one, the one used last."""
    newest, width = copies[0], max(len(show(c["folder"])) for c in copies)
    lines = []
    for i, c in enumerate(copies):
        total = count_prompts(lines_of(c["data"]), agent, sessions)
        if i == 0:
            standing = "used last"
        else:
            _, split, mine, other = since_split(newest["data"], c["data"], agent, sessions)
            if mine == 0 and other == 0:
                standing = "the same as 1"
            elif other == 0:
                standing = "%d prompt%s behind 1" % (mine, "s" * (mine != 1))
            elif mine == 0:
                standing = "%d prompt%s beyond 1" % (other, "s" * (other != 1))
            else:
                standing = "went its own way after %s: %d prompt%s of its own" % (
                    time_text(split, sessions), other, "s" * (other != 1))
        lines.append("%-*s  %s  %4d prompt%s  %s" % (width, show(c["folder"]), time.strftime("%b %d %H:%M", time.localtime(c["time"])),
                                                      total, " " if total == 1 else "s", standing))
    return lines


def choose_source(sid, where, agent, sessions, show, args):
    """Which copy to take when several profiles have the session: the one used last, unless
    you pick another at the prompt. A script, a pipe or --json gets the one used last."""
    copies = copies_used_last_first(where, agent, sessions)
    args.report["copies"] = [c["folder"] for c in copies]
    args.report["source_choice"] = "used last"
    if args.json or not interactive():
        print("Session %s is in %s too. Taking the copy in %s, used last; --from picks another."
              % (sid[:8], ", ".join(show(c["folder"]) for c in copies[1:]), show(copies[0]["folder"])))
        return copies[0]["folder"]
    print("Session %s is in %d profiles:" % (sid[:8], len(copies)))
    for i, line in enumerate(copy_lines(copies, agent, sessions, show), 1):
        print("  %d. %s" % (i, line))
    while True:
        try:
            answer = ask("Which one? [1] ").strip()
        except (EOFError, KeyboardInterrupt):
            print()
            sys.exit("Nothing copied.")
        if not answer:
            return copies[0]["folder"]
        if answer.isdigit() and 1 <= int(answer) <= len(copies):
            return copies[int(answer) - 1]["folder"]
        for c in copies:
            folder = c["folder"]
            base = os.path.basename(folder)
            short = base.split("-", 1)[1] if "-" in base else "default"
            names = {show(folder), folder, base, short, "%s:%s" % (agent, short)}
            if answer in names:
                return folder
        args.report["source_choice"] = "asked"
        print("Answer with a number from 1 to %d, or the profile's name." % len(copies))


def copy(src, dst, pairs, ids=None):
    """Copy a file or folder with its timestamps, under a temporary name until it's whole."""
    def copy_file(a, b):
        with open(b, "wb") as fh:
            fh.write(copy_bytes(a, pairs, ids))
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


def bring_up_to_date(src, dst, pairs):
    """Turn an older copy of a file or folder into a copy of src: each file it lacks, or
    has less of, is written whole, under a temporary name until it's complete."""
    for name in files_in(src):
        a, b = join(src, name), join(dst, name)
        if not os.path.isfile(b) or read(b) != copy_bytes(a, pairs):
            copy(a, b, pairs)


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


def resume_now(target, sid, cwd, agent="claude"):
    """Start Claude Code or Codex on the session in its new profile, from the folder it ran in."""
    program = shutil.which("codex" if agent == "codex" else "claude")
    if not program:
        sys.exit("%s isn't on your PATH; once it is, resume it with the command above"
                 % ("codex" if agent == "codex" else "claude"))
    env, variable = dict(os.environ), "CODEX_HOME" if agent == "codex" else "CLAUDE_CONFIG_DIR"
    if os.path.realpath(target) == os.path.realpath(os.path.join(HOME, ".codex" if agent == "codex" else ".claude")):
        env.pop(variable, None)  # where plain claude or codex looks
    else:
        env[variable] = target  # spelled as cl and cx spell it: Claude Code files the login under this string
    argv = [program, "resume", sid] if agent == "codex" else [program, "--resume", sid]
    sys.stdout.flush()
    os.chdir(cwd)
    if os.name == "nt":  # no exec on Windows: run it and leave Ctrl+C to the agent
        signal.signal(signal.SIGINT, signal.SIG_IGN)
        sys.exit(subprocess.call(argv, env=env))
    os.execve(program, argv, env)


def lock_codex_session(home, sid):
    """Take the lock Codex holds on a session while it's open, so it doesn't change while
    it's copied: (True, release) when it's free, (False, None) when Codex has it open. Not
    checked on Windows."""
    path = os.path.join(home, "thread-writer-locks", sid + ".lock")
    if os.name == "nt" or not os.path.isfile(path):  # Codex makes the file when it opens the session
        return True, lambda: None
    import fcntl
    fd = os.open(path, os.O_RDWR)
    try:
        fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
    except OSError:
        os.close(fd)
        return False, None
    return True, lambda: os.close(fd)  # closing it lets go


def clip(text, limits):
    """text cut to (lines, characters), saying how much was left out."""
    lines, chars = limits
    text = text.strip("\n")
    rows = text.splitlines()
    shown = "\n".join(rows[:lines])[:chars]
    hidden = len(rows) - len(shown.splitlines())
    if hidden > 0:
        return "%s\n… (%d more line%s)" % (shown, hidden, "s" * (hidden != 1))
    return shown + (" …" if len(shown) < len(text) else "")


def call_text(name, fields):
    """A tool call as text, in the format Codex's /import gives Claude Code's tool calls."""
    lines = ["[external_agent_tool_call: %s]" % name]
    for key, value in fields:
        value = value if isinstance(value, str) else json.dumps(value, ensure_ascii=False)
        lines.append("%s: %s" % (key, clip(value, CALL_CLIP)))
    return "\n".join(lines + ["[/external_agent_tool_call]"])


def result_text(text):
    return "[external_agent_tool_result]\n%s\n[/external_agent_tool_result]" % clip(text, RESULT_CLIP)


def add_turn(turns, role, text, when):
    """Add text to the conversation, joining it to the turn before when that's the same role's."""
    if not isinstance(text, str) or not text.strip():
        return
    if turns and turns[-1][0] == role:
        turns[-1][1].append(text)
        turns[-1][2] = when or turns[-1][2]
    else:
        turns.append([role, [text], when])


def codex_call_text(p):
    kind = p.get("type")
    if kind == "custom_tool_call":  # apply_patch: the patch itself
        return call_text(p.get("name") or "tool", [("input", p.get("input") or "")])
    if kind == "web_search_call":
        return call_text("web_search", [("query", (p.get("action") or {}).get("query") or "")])
    if kind == "image_generation_call":
        return call_text("image_generation", [("prompt", p.get("revised_prompt") or "")])
    if kind == "local_shell_call":
        return call_text("shell", [("command", " ".join((p.get("action") or {}).get("command") or []))])
    if kind == "tool_search_call":
        return call_text("tool_search", [("arguments", p.get("arguments") or "")])
    try:
        args = json.loads(p.get("arguments") or "{}")
    except (TypeError, ValueError):
        args = p.get("arguments")
    # Leave out settings such as yield_time_ms; keep text, lists and objects.
    fields = [(k, v) for k, v in args.items() if not isinstance(v, (bool, int, float)) and v is not None] \
        if isinstance(args, dict) else [("input", args)]
    return call_text(p.get("name") or "tool", fields)


def codex_output(p):
    out = p.get("output")
    if isinstance(out, dict):
        out = out.get("content") or out.get("output") or json.dumps(out, ensure_ascii=False)
    if isinstance(out, str):
        try:
            parsed = json.loads(out)
            if isinstance(parsed, dict) and "output" in parsed:
                out = parsed["output"]
        except ValueError:
            pass
    if isinstance(out, list):
        out = "".join(c.get("text") or "" for c in out if isinstance(c, dict)) or json.dumps(out, ensure_ascii=False)
    out = str(out or "")
    # exec_command puts a header before the command's output; keep the exit code from it.
    m = re.search(r"\nOutput:\n", out)
    if m:
        code = re.search(r"exited with code (-?\d+)", out[:m.start()])
        out = ("(exit code %s)\n" % code.group(1) if code and code.group(1) != "0" else "") + out[m.end():]
    return out


def read_codex(path, home, sessions):
    """A Codex session as a conversation to convert: its turns as text, and what it was."""
    meta, model, turns, calls = None, None, [], 0
    with open(path, encoding="utf-8", errors="replace") as fh:
        for line in fh:
            try:
                record = json.loads(line)
            except ValueError:
                continue
            p = record.get("payload") if isinstance(record, dict) else None
            if not isinstance(p, dict):
                continue
            kind, when = record.get("type"), record.get("timestamp")
            if kind == "session_meta" and meta is None:
                meta = p
            elif kind == "turn_context":
                model = p.get("model") or model
            elif kind == "response_item" and p.get("type") == "message":
                text = sessions.codex_text(p.get("content"))
                if p.get("role") == "user" and not text.lstrip().startswith(sessions.CODEX_HARNESS):
                    if text.lstrip().startswith("<user_shell_command>"):  # a command you ran, with its output
                        text = clip(text, RESULT_CLIP)
                    add_turn(turns, "user", text, when)
                elif p.get("role") == "assistant":
                    add_turn(turns, "assistant", text, when)
            elif kind == "response_item" and p.get("type") in (
                    "function_call", "custom_tool_call", "local_shell_call", "web_search_call",
                    "image_generation_call", "tool_search_call"):
                add_turn(turns, "assistant", codex_call_text(p), when)
                calls += 1
            elif kind == "response_item" and str(p.get("type")).endswith("_output"):
                add_turn(turns, "assistant", result_text(codex_output(p)), when)
            # Reasoning and compaction summaries are encrypted, so they're left out; the
            # conversation before a compaction is still in the file.
    meta = meta or {}
    match = sessions.CODEX_FILE.search(path)
    sid = meta.get("id") or (match.group(1) if match else "?")
    summary = sessions.read_codex_session(path, sessions.codex_names(home)) or {}
    return {"agent": "codex", "id": sid, "cwd": meta.get("cwd"), "model": model, "turns": turns, "calls": calls,
            "title": summary.get("title"), "started": meta.get("timestamp"),
            "branch": (meta.get("git") or {}).get("branch") if isinstance(meta.get("git"), dict) else None,
            "reasoning": "Codex's reasoning", "left_out": "Codex's reasoning, images, goals and subagent sessions"}


def claude_result(content):
    if isinstance(content, list):
        return "\n".join(b.get("text") or "" if b.get("type") == "text" else "[%s]" % b.get("type")
                         for b in content if isinstance(b, dict))
    return content if isinstance(content, str) else json.dumps(content, ensure_ascii=False)


def read_claude(path, sessions):
    """A Claude Code session as a conversation to convert. Follows the conversation back from
    its newest message, so turns undone with /rewind and other branches are left out, and
    past compactions to what came before them."""
    records, by_id = [], {}
    with open(path, encoding="utf-8", errors="replace") as fh:
        for line in fh:
            try:
                record = json.loads(line)
            except ValueError:
                continue
            if isinstance(record, dict):
                records.append(record)
                if record.get("uuid"):
                    by_id[record["uuid"]] = record
    node = next((r for r in reversed(records) if r.get("type") in ("user", "assistant")
                 and not r.get("isSidechain")), None)
    chain, seen = [], set()
    if node is not None and not node.get("uuid"):  # no links to follow: the messages in file order
        chain = [r for r in reversed(records) if r.get("type") in ("user", "assistant")]
        node = None
    while node is not None and id(node) not in seen:
        seen.add(id(node))
        chain.append(node)
        node = by_id.get(node.get("parentUuid") or node.get("logicalParentUuid"))
    turns, calls, model = [], 0, None
    for record in reversed(chain):
        if record.get("type") not in ("user", "assistant") or record.get("isMeta") \
                or record.get("isCompactSummary") or record.get("isSidechain"):
            continue
        message, when, role = record.get("message") or {}, record.get("timestamp"), record["type"]
        if role == "assistant" and message.get("model") not in (None, "<synthetic>"):
            model = message["model"]
        content = message.get("content")
        blocks = [{"type": "text", "text": content}] if isinstance(content, str) else \
            content if isinstance(content, list) else []
        for block in blocks:
            kind = block.get("type") if isinstance(block, dict) else None
            if kind == "text":
                text = block.get("text") or ""
                if not (role == "user" and text.lstrip().startswith(CLAUDE_HARNESS)):
                    add_turn(turns, role, text, when)
            elif kind in ("tool_use", "server_tool_use", "mcp_tool_use"):
                fields = block.get("input")
                fields = list(fields.items()) if isinstance(fields, dict) else [("input", fields)]
                add_turn(turns, "assistant", call_text(block.get("name") or kind, fields), when)
                calls += 1
            elif kind and kind.endswith("tool_result"):
                add_turn(turns, "assistant", result_text(claude_result(block.get("content"))), when)
            elif kind in ("image", "document"):
                add_turn(turns, role, "[%s]" % kind, when)
            # thinking and redacted_thinking are signed for Anthropic's API: left out
    messages = [r for r in reversed(chain) if r.get("type") in ("user", "assistant")]
    titles = [next((r.get(key) for r in reversed(records) if r.get("type") == kind and r.get(key)), None)
              for kind, key in (("custom-title", "customTitle"), ("ai-title", "aiTitle"), ("agent-name", "agentName"))]
    first_prompt = next((t for role, texts, _ in turns if role == "user" for t in texts), None)
    summary = sessions.read_session(path) or {}  # where to resume it from, as the session list says
    return {"agent": "claude", "id": os.path.basename(path)[:-len(".jsonl")],
            "cwd": summary.get("cwd") or next((r.get("cwd") for r in messages if r.get("cwd")), None),
            "model": model, "turns": turns, "calls": calls,
            "title": " ".join((next((t for t in titles if t), None) or first_prompt or "")[:200].split()) or None,
            "started": next((r.get("timestamp") for r in messages if r.get("timestamp")), None),
            "branch": next((r.get("gitBranch") for r in reversed(messages) if r.get("gitBranch")), None),
            "reasoning": "Claude's thinking",
            "left_out": "Claude's thinking, images, plans, /rewind snapshots and subagent transcripts"}


def fit(conv, target_agent):
    """The conversation's turns as the new agent should get them: a note about where they
    came from first, the oldest turns after the first left out past BUDGET, alternating
    between you and the assistant, ending on a reply. Also how many turns were left out."""
    turns = [[role, list(texts), when] for role, texts, when in conv["turns"]]
    sizes = [sum(len(t) for t in texts) for _, texts, _ in turns]
    dropped = 0
    if sum(sizes) > BUDGET and len(turns) > 2:
        start, total = len(turns), sizes[0]
        while start > 1 and total + sizes[start - 1] <= BUDGET:
            start -= 1
            total += sizes[start]
        dropped = start - 1
        gap = ["user", ["[%d earlier turn%s left out here, to fit the context window.]"
                        % (dropped, "s" * (dropped != 1))], turns[start - 1][2]]
        turns = turns[:1] + [gap] + turns[start:]
    started = (conv.get("started") or "")[:16].replace("T", " ")
    note = ("[Imported from %s session %s, started %s in %s%s. Its tool calls are shown as text, with their "
            "output shortened, and %s isn't included.]" % (
                AGENT_NAMES[conv["agent"]], conv["id"], started or "?", conv.get("cwd") or "?",
                " with " + conv["model"] if conv.get("model") else "", conv["reasoning"]))
    merged = []
    for role, texts, when in [["user", [note], conv.get("started")]] + turns:
        for text in texts:
            add_turn(merged, role, text, when)
    if merged[-1][0] == "user":
        add_turn(merged, "assistant", "(The %s session ended here.)" % AGENT_NAMES[conv["agent"]], merged[-1][2])
    return merged, dropped


def iso_ms(t):
    return datetime.fromtimestamp(t, timezone.utc).strftime("%Y-%m-%dT%H:%M:%S.") + "%03dZ" % (int(t * 1000) % 1000)


def uuid7(t):
    """A version 7 UUID for time t, the kind Codex gives its sessions: they sort by time."""
    ms = int(t * 1000) & ((1 << 48) - 1)
    rand = int.from_bytes(os.urandom(10), "big")
    return str(uuid.UUID(int=ms << 80 | 0x7 << 76 | (rand >> 62 & 0xFFF) << 64 | 0b10 << 62 | rand & ((1 << 62) - 1)))


def newest_value(paths, key, lines=50):
    """The first value of `key` in the newest of these JSONL files that has one."""
    dated = []
    for path in paths:
        try:
            dated.append((os.path.getmtime(path), path))
        except OSError:
            pass
    for _, path in sorted(dated, reverse=True)[:20]:
        try:
            with open(path, encoding="utf-8", errors="replace") as fh:
                for _, line in zip(range(lines), fh):
                    try:
                        record = json.loads(line)
                    except ValueError:
                        continue
                    value = record.get(key) if isinstance(record, dict) else None
                    if value is None and isinstance(record, dict) and isinstance(record.get("payload"), dict):
                        value = record["payload"].get(key)
                    if isinstance(value, str) and value:
                        return value
        except OSError:
            continue
    return None


def write_claude(conv, turns, profile, cwd, sessions):
    """Write a converted conversation as a new Claude Code session in `profile`: its ID and file."""
    sid, now = str(uuid.uuid4()), time.time()
    version = newest_value(glob.glob(os.path.join(profile, "projects", "*", "*.jsonl")), "version") or \
        newest_value([p for d in sessions.find_profiles() for p in glob.glob(os.path.join(d, "projects", "*", "*.jsonl"))],
                     "version") or "2.1.0"
    base = {"isSidechain": False, "userType": "external", "entrypoint": "cli", "cwd": cwd, "sessionId": sid,
            "version": version, "gitBranch": conv.get("branch") or ""}
    entries, parent = [], None
    for n, (role, texts, when) in enumerate(turns):
        me = str(uuid.uuid4())
        entry = dict(base, parentUuid=parent, type=role, uuid=me, timestamp=when or iso_ms(now))
        text = "\n\n".join(texts)
        if role == "user":
            entry["message"] = {"role": "user", "content": text}
        else:
            entry["requestId"] = "req_imported_%d" % n
            entry["message"] = {"model": conv.get("model") or AGENT_NAMES[conv["agent"]], "id": "msg_imported_%d" % n,
                                "type": "message", "role": "assistant", "content": [{"type": "text", "text": text}],
                                "stop_reason": "end_turn", "stop_sequence": None,
                                "usage": {"input_tokens": 0, "output_tokens": 0}}
        entries.append(entry)
        parent = me
    # Shown when it's resumed; also dates the session to now, so it's listed as recent.
    entries.append(dict(base, parentUuid=parent, type="system", subtype="informational", isMeta=False,
                        uuid=str(uuid.uuid4()), timestamp=iso_ms(now), level="notice",
                        content="Converted from %s session %s with hatcheck move"
                        % (AGENT_NAMES[conv["agent"]], conv["id"])))
    if conv.get("title"):
        entries.append({"type": "ai-title", "aiTitle": conv["title"], "sessionId": sid})
    path = os.path.join(profile, "projects", sessions.encode_project(cwd), sid + ".jsonl")
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path + TMP, "w", encoding="utf-8", newline="\n") as fh:
        fh.write("".join(json.dumps(e, ensure_ascii=False, separators=(",", ":")) + "\n" for e in entries))
    os.replace(path + TMP, path)
    return sid, path


def codex_provider(home):
    """The model_provider a Codex home's config.toml sets, or openai."""
    try:
        with open(os.path.join(home, "config.toml"), encoding="utf-8") as fh:
            for line in fh:
                if line.lstrip().startswith("["):
                    break  # past the top-level settings
                m = re.match(r'\s*model_provider\s*=\s*["\']([^"\']+)["\']', line)
                if m:
                    return m.group(1)
    except OSError:
        pass
    return "openai"


def write_codex(conv, turns, home, cwd, sessions):
    """Write a converted conversation as a new Codex session in `home`, the way Codex's own
    /import writes a Claude Code session: its ID and file."""
    now = time.time()
    sid, stamp = uuid7(now), iso_ms(now)
    homes = [home] + [h for h in sessions.find_codex_homes() if h != home]
    meta = {"id": sid, "timestamp": stamp, "cwd": cwd, "originator": "codex-tui",
            "cli_version": newest_value([p for h in homes for p in sessions.codex_session_files(h)], "cli_version")
            or "0.155.1", "source": "cli", "model_provider": codex_provider(home), "history_mode": "legacy"}
    records = [{"timestamp": stamp, "type": "session_meta", "payload": meta},
               {"timestamp": stamp, "type": "event_msg", "payload": {
                   "type": "task_started", "turn_id": "external-import-turn-1", "started_at": int(now),
                   "model_context_window": None, "collaboration_mode_kind": "default"}}]
    for role, texts, when in turns:
        text, when = "\n\n".join(texts), when or stamp
        if role == "user":
            records.append({"timestamp": when, "type": "event_msg", "payload": {
                "type": "user_message", "message": text, "local_images": [], "local_audio": [], "text_elements": []}})
            records.append({"timestamp": when, "type": "response_item", "payload": {
                "type": "message", "role": "user", "content": [{"type": "input_text", "text": text}]}})
        else:
            records.append({"timestamp": when, "type": "event_msg", "payload": {
                "type": "agent_message", "message": text, "phase": None, "memory_citation": None}})
            records.append({"timestamp": when, "type": "response_item", "payload": {
                "type": "message", "role": "assistant", "content": [{"type": "output_text", "text": text}]}})
    zero = {"input_tokens": 0, "cached_input_tokens": 0, "output_tokens": 0, "reasoning_output_tokens": 0,
            "total_tokens": 0}
    records += [
        {"timestamp": stamp, "type": "event_msg", "payload": {
            "type": "agent_message", "message": "<EXTERNAL SESSION IMPORTED>", "phase": None, "memory_citation": None}},
        {"timestamp": stamp, "type": "event_msg", "payload": {
            "type": "token_count", "info": {"total_token_usage": zero, "last_token_usage": zero,
                                            "model_context_window": None}, "rate_limits": None}},
        {"timestamp": stamp, "type": "event_msg", "payload": {
            "type": "task_complete", "turn_id": "external-import-turn-1", "last_agent_message": None,
            "started_at": int(now), "completed_at": int(now)}}]
    local = time.localtime(now)
    path = os.path.join(home, "sessions", time.strftime("%Y", local), time.strftime("%m", local),
                        time.strftime("%d", local), "rollout-%s-%s.jsonl" % (time.strftime("%Y-%m-%dT%H-%M-%S", local), sid))
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path + TMP, "w", encoding="utf-8", newline="\n") as fh:
        fh.write("".join(json.dumps(r, ensure_ascii=False, separators=(",", ":")) + "\n" for r in records))
    os.replace(path + TMP, path)
    if conv.get("title"):
        name_session(home, sid, conv["title"])
    return sid, path


def name_session(home, sid, name):
    """Give a Codex session a name, as /rename does: a line in the home's session_index.jsonl."""
    stamp = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%S.%fZ")
    with open(os.path.join(home, "session_index.jsonl"), "a", encoding="utf-8", newline="\n") as fh:
        fh.write(json.dumps({"id": sid, "thread_name": name, "updated_at": stamp}, ensure_ascii=False) + "\n")


def delete_originals(items):
    """--no-keep: delete what was copied from the old profile, except plan files."""
    for _, src, _, deleted in items:
        if deleted and os.path.isdir(src):
            shutil.rmtree(src)
        elif deleted and os.path.exists(src):
            os.remove(src)


def read_info(sessions, transcripts):
    """What the session list knows about a session, from the newest of its transcripts."""
    summaries = []
    for path in transcripts:
        try:
            summaries.append(sessions.read_session(path))
        except (OSError, ValueError):
            pass
    return max(filter(None, summaries), key=lambda s: (s["when"], s["canonical"], s["size"]), default=None)


def went_on_in_both(sessions, agent, sid, source, target, ours, theirs, again, paths):
    """Stop: the session went on in both places after one was copied from the other.
    ours and theirs: the two transcripts, when it's they that differ that way."""
    show = sessions.shell_path
    shared = since_split(ours, theirs, agent, sessions) if ours is not None and theirs is not None else None
    if shared and shared[0] == 0:
        first = "%s has a different session with the ID %s, with nothing in common with this one" % (show(target), sid[:8])
    elif shared:
        _, when, mine, other = shared
        first = "Session %s went on in both %s and %s after %s" % (
            sid[:8], show(source), show(target), time_text(when, sessions))
        if mine or other:
            first += ": %d new prompt%s in %s, %d in %s" % (mine, "s" * (mine != 1), show(source), other, show(target))
    else:
        first = "The copies of session %s in %s and %s each have files the other lacks" % (
            sid[:8], show(source), show(target))
    sys.exit("%s. Copying this one over would lose what's only in %s, so nothing was copied.\n"
             "To keep both, bring this one in as a new session with its own ID:\n  %s\n"
             "Or delete the copy in %s first, to replace it:\n%s"
             % (first, show(target), again("--fork"), show(target), "\n".join("  " + show(p) for p in paths)))


def move_claude(args, sessions, usage, sid, source, transcripts, target, again):
    """Copy (or with --no-keep, move) a Claude Code session to another profile, or with
    --fork copy it in as a new session. Returns the session to resume and the directory
    to resume it from."""
    show = sessions.shell_path
    live = sessions.open_sessions([source]).get((os.path.realpath(source), sid))
    if live:
        sys.exit("Session %s is open in pid %s%s; quit it first, so it stops changing"
                 % (sid[:8], live.get("pid"), " (%s)" % live["status"] if live.get("status") else ""))
    new_sid = str(uuid.uuid4()) if args.fork else sid
    ids = (sid, new_sid) if args.fork else None

    # What to copy: (what, original, copy, deleted with --no-keep), and the paths to repoint.
    # Also what only the copy already in the profile has, which makes that copy newer.
    items, pairs = [], []
    for path in transcripts:
        old = path[:-len(".jsonl")]
        new = os.path.join(target, "projects", os.path.basename(os.path.dirname(path)), new_sid)
        pairs += path_pairs(old, new)  # covers <id>.jsonl and the <id>/ folder beside it
        items.append(("transcript", path, new + ".jsonl", True))
        if os.path.isdir(old) or os.path.isdir(new):
            items.append(("subagents and tool output", old, new, True))
    for sub, what in (("file-history", "/rewind file snapshots"), ("session-env", "session environment")):
        old, new = os.path.join(source, sub, sid), os.path.join(target, sub, new_sid)
        if any(os.path.isdir(d) and files_in(d) for d in (old, new)):
            items.append((what, old, new, True))
            pairs += path_pairs(old, new)
    for slug in sorted(plan_slugs(transcripts)):
        old, new = os.path.join(source, "plans", slug + ".md"), os.path.join(target, "plans", slug + ".md")
        if os.path.isfile(old):
            items.append(("plan", old, new, False))  # kept: sessions can share a plan
            pairs += path_pairs(old, new)

    states = {dst: item_standing(src, dst, pairs) for _, src, dst, _ in items}
    if args.fork:  # its own new paths; but a plan already there is the copy there's too: left as it is
        states.update({dst: "kept" for what, _, dst, _ in items if what == "plan" and states[dst] not in ("new", "same")})
    verdict = overall(s for s in states.values() if s != "kept")
    transcript = next((src, dst) for what, src, dst, _ in items if what == "transcript")
    theirs = read(transcript[1]) if os.path.isfile(transcript[1]) else None
    if verdict == "diverged":
        split = states[transcript[1]] == "diverged"
        went_on_in_both(sessions, "claude", sid, source, target, copy_bytes(transcript[0], pairs) if split else None,
                        theirs, again,
                        [dst for what, _, dst, _ in items if os.path.lexists(dst) and (what != "plan" or states[dst] == "diverged")])

    args.report.update(agent="claude", session=sid, source=source, target=target, fork=bool(args.fork),
                       new_session=new_sid if args.fork else None)
    if verdict == "newer":  # it went on there: nothing here that the copy there lacks
        info = read_info(sessions, [transcript[1]])
        args.report.update(action="nothing", reason="the copy there is newer", deleted_original=bool(args.no_keep),
                           cwd=info["cwd"] if info else None, resume=sessions.resume_command(target, sid, False))
        print('%s already has session %s "%s", with everything in this copy and more: the session went on '
              "there. Nothing to copy." % (show(target), sid[:8], sessions.truncate(info["title"], 60) if info else "?"))
        if args.no_keep and not args.dry_run:
            delete_originals(items)
        print("%s the older copy from %s." % ("Would delete" if args.dry_run else "Deleted", show(source)) if args.no_keep
              else "Kept the older copy in %s; --no-keep deletes it." % show(source))
        print("Resume it with:")
        if info and info["cwd"]:
            print("  cd " + show(info["cwd"]))
        print("  " + sessions.resume_command(target, sid, False))
        return sid, info["cwd"] if info else None

    repointed = any(read(join(src, n)) != copy_bytes(join(src, n), pairs, ids)
                    for _, src, _, _ in items for n in files_in(src) if join(src, n).endswith(".jsonl"))
    details = {}  # for the report, worked out before --no-keep deletes the originals
    for what, src, _, _ in items:
        files = len(files_in(src))
        details[src] = (os.path.basename(src) if what == "plan" else size_text(os.path.getsize(src))
                        if os.path.isfile(src) else "%d file%s" % (files, "s" * (files != 1)))
    info = read_info(sessions, transcripts)
    older_since = last_time(theirs) if states[transcript[1]] == "older" and theirs is not None else None
    had_it = args.fork and any(os.path.isfile(os.path.join(target, "projects", os.path.basename(os.path.dirname(t)),
                                                           sid + ".jsonl")) for t in transcripts)  # left as it is

    if not args.dry_run:
        for _, src, dst, _ in items:
            if states[dst] == "new":
                copy(src, dst, pairs, ids)
            elif states[dst] == "older":
                bring_up_to_date(src, dst, pairs)
        wrong = [dst for _, src, dst, _ in items if states[dst] != "kept" and not matches(src, dst, pairs, ids)]
        if wrong:
            sys.exit("These copies don't match their originals, so nothing was deleted:\n"
                     + "\n".join("  " + show(w) for w in wrong))
        if args.no_keep:
            delete_originals(items)

    args.report.update(
        action="fork" if args.fork else "update" if "older" in states.values() else "move" if args.no_keep else "copy",
        title=info["title"] if info else None, deleted_original=bool(args.no_keep), repointed=repointed,
        items=[{"what": what, "from": src, "to": dst, "state": states[dst], "detail": details[src]}
               for what, src, dst, _ in items],
        cwd=info["cwd"] if info else None, resume=sessions.resume_command(target, new_sid, False))
    heading = ("Would move" if args.dry_run else "Moved") if args.no_keep else (
        "Would copy" if args.dry_run else "Copied")
    title = sessions.truncate(info["title"], 60) if info else "(no conversation)"
    print('%s session %s "%s" from %s to %s%s:' % (heading, sid[:8], title, show(source), show(target),
                                                   " as a new session" if args.fork else ""))
    notes = {"same": "  (already there)", "older": "  (%s an older copy)" % ("would update" if args.dry_run else "updated"),
             "kept": "  (the one there stays: the copy there uses it too)"}
    for what, src, dst, _ in items:
        print("  %-26s %s%s" % (what, details[src], notes.get(states[dst], "")))
    if args.fork:
        print("  %-26s %s" % ("new session", new_sid))
    if repointed:
        print("Paths in the conversation to these files %s to the copies." % ("would point" if args.dry_run else "now point"))
    if "older" in states.values():
        print("%s had an older copy of this session%s; it %s up to date." % (
            show(target), ", as it was on " + time_text(older_since, sessions) if older_since else "",
            "would be brought" if args.dry_run else "is now"))
    if had_it:
        print("The copy of session %s that was already in %s stays as it was." % (sid[:8], show(target)))
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
    print("  " + sessions.resume_command(target, new_sid, False))
    provider = sessions.provider_of(info["model"]) if info else None
    if provider:
        print("Its last reply came from %s; to carry on there rather than on this profile's Claude plan:\n  %s"
              % (info["model"], sessions.resume_command(target, new_sid, False, provider)))
    return new_sid, info["cwd"] if info else None


def codex_files(sessions, home, prefix):
    """[(session ID, file)] for the Codex sessions in a home whose ID starts with prefix,
    archived ones too."""
    paths = glob.glob(os.path.join(home, "sessions", "*", "*", "*", "rollout-*.jsonl")) + \
        glob.glob(os.path.join(home, "archived_sessions", "rollout-*.jsonl"))
    matches = ((sessions.CODEX_FILE.search(path), path) for path in sorted(paths))
    return [(m.group(1), path) for m, path in matches if m and m.group(1).startswith(prefix.lower())]


def codex_account(home, usage):
    """The ChatGPT account and user a Codex home is signed in to, or None."""
    login = usage.read_codex_login(home) if usage else None
    return (login["account_id"], login.get("user_id")) if login and login.get("account_id") else None


def copy_codex(args, sessions, sid, source, files, target, again):
    """Copy (or with --no-keep, move) a Codex session to another home on the same account,
    or with --fork copy it in as a new session. Returns the session to resume and the
    directory to resume it from."""
    show = sessions.shell_path
    src = max(files, key=os.path.getmtime)  # an archived copy and a live one: the newer
    name = sessions.codex_names(source).get(sid)
    info = sessions.read_codex_session(src, {sid: name} if name else {}) or {}
    theirs = sorted((p for s, p in codex_files(sessions, target, sid) if s == sid), key=os.path.getmtime)
    if args.fork:
        now = time.time()
        new_sid, local = uuid7(now), time.localtime(now)
        dst = os.path.join(target, "sessions", time.strftime("%Y", local), time.strftime("%m", local),
                           time.strftime("%d", local), "rollout-%s-%s.jsonl" % (time.strftime("%Y-%m-%dT%H-%M-%S", local), new_sid))
    else:  # the copy already there, wherever it is, archived or not
        new_sid, dst = sid, theirs[-1] if theirs else os.path.join(target, os.path.relpath(src, source))
    ids = (sid, new_sid) if args.fork else None
    free, release = lock_codex_session(source, sid)
    if not free:
        sys.exit("Codex session %s is open in codex; quit it first, so it stops changing" % sid[:8])
    try:
        state = item_standing(src, dst, [])
        if state == "diverged":
            went_on_in_both(sessions, "codex", sid, source, target, read(src), read(dst), again, [dst])
        args.report.update(agent="codex", session=sid, source=source, target=target, fork=bool(args.fork),
                           new_session=new_sid if args.fork else None, title=info.get("title"), cwd=info.get("cwd"))
        if state == "newer":  # it went on there
            args.report.update(action="nothing", reason="the copy there is newer", deleted_original=bool(args.no_keep),
                               resume=sessions.codex_resume_command(target, sid, False))
            print('%s already has Codex session %s "%s", with everything in this copy and more: the session went on '
                  "there. Nothing to copy." % (show(target), sid[:8], sessions.truncate(info.get("title") or "?", 60)))
            if args.no_keep and not args.dry_run:
                os.remove(src)
            print("%s the older copy from %s." % ("Would delete" if args.dry_run else "Deleted", show(source))
                  if args.no_keep else "Kept the older copy in %s; --no-keep deletes it." % show(source))
            print("Resume it with:")
            if info.get("cwd"):
                print("  cd " + show(info["cwd"]))
            print("  " + sessions.codex_resume_command(target, sid, False))
            return sid, info.get("cwd")
        older_since = last_time(read(dst)) if state == "older" else None
        size = size_text(os.path.getsize(src))
        args.report.update(
            action="fork" if args.fork else "update" if state == "older" else "move" if args.no_keep else "copy",
            deleted_original=bool(args.no_keep), items=[{"what": "session file", "from": src, "to": dst, "state": state,
                                                         "detail": size}],
            resume=sessions.codex_resume_command(target, new_sid, False))
        if not args.dry_run:
            if state in ("new", "older"):
                copy(src, dst, [], ids)
            if read(dst) != copy_bytes(src, [], ids):
                sys.exit("The copy doesn't match the original, so nothing was deleted:\n  " + show(dst))
            if name and sessions.codex_names(target).get(new_sid) != name:
                name_session(target, new_sid, name)
            if args.no_keep:
                os.remove(src)
    finally:
        release()
    heading = ("Would move" if args.dry_run else "Moved") if args.no_keep else ("Would copy" if args.dry_run else "Copied")
    print('%s Codex session %s "%s" from %s to %s%s:' % (heading, sid[:8], sessions.truncate(info.get("title") or "?", 60),
                                                         show(source), show(target), " as a new session" if args.fork else ""))
    print("  %-26s %s%s" % ("session file", size, {
        "same": "  (already there)", "older": "  (%s an older copy)" % ("would update" if args.dry_run else "updated")
    }.get(state, "")))
    if name:
        print("  %-26s %s" % ("name", name))
    if args.fork:
        print("  %-26s %s" % ("new session", new_sid))
    if state == "older":
        print("%s had an older copy of this session%s; it %s up to date." % (
            show(target), ", as it was on " + time_text(older_since, sessions) if older_since else "",
            "would be brought" if args.dry_run else "is now"))
    if args.fork and theirs:
        print("The copy of session %s that was already in %s stays as it was." % (sid[:8], show(target)))
    print("Both homes are signed in to the same ChatGPT account, so its plan limits stay the same.")
    if args.no_keep:
        print("%s the original from %s." % ("Would delete" if args.dry_run else "Deleted", show(source)))
    else:
        print("Kept the original in %s; --no-keep deletes it once the copy is checked." % show(source))
    print("Then resume it with:" if args.dry_run else "Resume it with:")
    if info.get("cwd"):
        print("  cd " + show(info["cwd"]))
    print("  " + sessions.codex_resume_command(target, new_sid, False))
    return new_sid, info.get("cwd")


def convert(args, sessions, agent, sid, source, files, target_agent, target, why):
    """Convert a session for the other agent, or for a Codex home on another account, into
    a new session in `target`. Returns the new session's ID and the directory to resume it from."""
    show = sessions.shell_path
    path = max(files, key=os.path.getmtime)
    release = lambda: None  # noqa: E731
    if agent == "codex":
        free, release = lock_codex_session(source, sid)
        if not free:
            sys.exit("Codex session %s is open in codex; quit it first, so it stops changing" % sid[:8])
    try:
        conv = read_codex(path, source, sessions) if agent == "codex" else read_claude(path, sessions)
    finally:
        release()
    if not conv["turns"]:
        sys.exit("%s session %s holds no conversation to convert" % (AGENT_NAMES[agent], sid[:8]))
    turns, dropped = fit(conv, target_agent)
    cwd = conv.get("cwd") or HOME
    new_sid = None
    if not args.dry_run:
        write = write_codex if target_agent == "codex" else write_claude
        new_sid, new_path = write(conv, turns, target, cwd, sessions)
    args.report.update(
        action="convert", agent=agent, session=sid, source=source, target=target, target_agent=target_agent,
        new_session=new_sid, title=conv.get("title"), why=why, cwd=cwd,
        prompts=sum(1 for role, _, _ in conv["turns"] if role == "user"), tool_calls=conv["calls"],
        left_out=conv["left_out"], turns_dropped=dropped,
        resume=(sessions.codex_resume_command if target_agent == "codex" else sessions.resume_command)(
            target, new_sid or "<new id>", False))
    print('%s %s session %s "%s" from %s into a %s session in %s:' % (
        "Would convert" if args.dry_run else "Converted", AGENT_NAMES[agent], sid[:8],
        sessions.truncate(conv.get("title") or "?", 60), show(source), AGENT_NAMES[target_agent], show(target)))
    if why:
        print("  (" + why + ")")
    prompts = sum(1 for role, _, _ in conv["turns"] if role == "user")
    print("  %-26s %d of your messages, with the replies" % ("conversation", prompts))
    print("  %-26s %d, as text with their output shortened" % ("tool calls", conv["calls"]))
    print("  %-26s %s" % ("left out", conv["left_out"]))
    if dropped:
        print("  %-26s the oldest %d turns after the first are left out, to fit the context window"
              % ("too long", dropped))
    if new_sid:
        print("  %-26s %s (%s)" % ("new session", new_sid, size_text(os.path.getsize(new_path))))
    print("The original stays in %s." % show(source))
    print("Then resume it with:" if args.dry_run else "Resume it with:")
    print("  cd " + show(cwd))
    if target_agent == "codex":
        print("  " + sessions.codex_resume_command(target, new_sid or "<new id>", False))
    else:
        print("  " + sessions.resume_command(target, new_sid or "<new id>", False))
    return new_sid, cwd


def main(argv=None, prog=None):
    parser = argparse.ArgumentParser(
        prog=prog, description="Copy a Claude Code or Codex session to another profile, to carry on with it there, "
                    "or convert it for the other agent.")
    parser.add_argument("session", help="the session ID, or the start of it")
    parser.add_argument("profile", help="where it goes: NAME for ~/.claude-NAME, default for ~/.claude, "
                                        "codex for ~/.codex, codex:NAME for ~/.codex-NAME, or a path")
    parser.add_argument("--from", dest="source", metavar="PROFILE",
                        help="the profile or home to take it from, when more than one has it")
    parser.add_argument("--no-keep", action="store_true",
                        help="delete the original once the copy is checked, which moves it (not when converting)")
    parser.add_argument("--fork", action="store_true",
                        help="copy it in as a new session with its own ID, leaving a copy already in PROFILE as it is "
                             "(a converted session always gets one)")
    parser.add_argument("--dry-run", action="store_true",
                        help="show what would be copied, converted and deleted, and change nothing")
    parser.add_argument("--json", action="store_true",
                        help="say what was or would be done as JSON, and never ask to resume it")
    args = parser.parse_args(argv)
    args.report = {"version": 1, "dry_run": bool(args.dry_run)}
    if args.json:  # the text goes nowhere; the report is printed at the end
        usage = load_sibling("hatcheck_sessions", "sessions.py").load_usage_module()
        if usage:
            usage.setup_console()  # UTF-8 output on Windows, before stdout is set aside
        with contextlib.redirect_stdout(io.StringIO()):
            run(args, parser)
        print(json.dumps(args.report, ensure_ascii=False, indent=2))
        return
    run(args, parser)


def rerun_command(prog, args, sessions, *flags):
    """This command again, with flags added: what to run next. The session, profile and
    source are quoted for the shell the command is printed for; the flags stay bare, so
    PowerShell's quoting of words that start with - doesn't turn --fork into '--fork'."""
    quote = sessions.ps_quote if sessions.powershell() else shlex.quote
    words = [quote(args.session), quote(args.profile)]
    if args.source:
        words += ["--from", quote(args.source)]
    words += [flag for flag, on in (("--no-keep", args.no_keep), ("--dry-run", args.dry_run)) if on]
    return " ".join([prog] + words + list(flags))


def run(args, parser):
    """The move itself, saying what it does as it goes, and filling in args.report."""
    try:
        sessions = load_sibling("hatcheck_sessions", "sessions.py")
    except OSError:
        sys.exit("hatcheck move needs sessions.py in its folder")
    usage = sessions.load_usage_module()  # usage.py: accounts, and UTF-8 output on Windows
    if usage:
        usage.setup_console()
    show = sessions.shell_path
    target_agent, target = place(args.profile)
    if not os.path.isdir(target):
        if target_agent == "codex":
            name = os.path.basename(target)[len(".codex-"):] if os.path.basename(target).startswith(".codex-") else ""
            sys.exit("%s isn't a Codex home. Sign in there first, e.g. with: %s"
                     % (show(target), "cx %s login" % name if name else "codex login"))
        sys.exit("%s isn't a profile folder. Sign in there first, e.g. with: cl %s" % (show(target), args.profile))

    # Look in every profile and home but the target, or just in --from.
    if args.source:
        sources = [place(args.source)]
    else:
        sources = [(a, p) for a, ps in (("claude", sessions.find_profiles()), ("codex", sessions.find_codex_homes()))
                   for p in ps if os.path.realpath(p) != os.path.realpath(target)]
    found = {}  # (agent, session ID) -> {profile or home: [files]}
    for agent, folder in sources:
        if agent == "claude":
            for path in sorted(glob.glob(os.path.join(folder, "projects", "*", glob.escape(args.session) + "*.jsonl"))):
                key = ("claude", os.path.basename(path)[:-len(".jsonl")])
                found.setdefault(key, {}).setdefault(folder, []).append(path)
        else:
            for sid, path in codex_files(sessions, folder, args.session):
                found.setdefault(("codex", sid), {}).setdefault(folder, []).append(path)
    if not found:
        if args.source:
            sys.exit("No session starting with %s in %s" % (args.session, show(sources[0][1])))
        if glob.glob(os.path.join(target, "projects", "*", glob.escape(args.session) + "*.jsonl")) \
                or codex_files(sessions, target, args.session):
            sys.exit("Session %s is already in %s, and in no other profile" % (args.session, show(target)))
        sys.exit("No session starting with %s" % args.session)
    if len(found) > 1:
        lines = ["  %s  %s" % (sid, ", ".join(map(show, where))) for (_, sid), where in sorted(found.items())]
        sys.exit("%s matches %d sessions; give more of the ID:\n%s"
                 % (args.session, len(found), "\n".join(lines[:10] + ["  …"] * (len(lines) > 10))))
    ((agent, sid), where), = found.items()
    if len(where) > 1:
        source = choose_source(sid, where, agent, sessions, show, args)
    else:
        source, = where
    files = where[source]
    if agent == target_agent and os.path.realpath(source) == os.path.realpath(target):
        sys.exit("Session %s is already in %s" % (sid, show(target)))

    why = None
    if agent == "codex" and target_agent == "codex":
        accounts = (codex_account(source, usage), codex_account(target, usage))
        if None in accounts or accounts[0] != accounts[1]:
            why = ("%s and %s aren't signed in to the same ChatGPT account, and Codex encrypts its reasoning for "
                   "the account that wrote it, so the session is converted" % (show(source), show(target)))
    converting = agent != target_agent or why is not None
    if converting and args.no_keep:
        sys.exit("--no-keep can't be used here: the session would be converted, which leaves things out, so "
                 "the original is kept. Run it again without --no-keep.")

    def again(*flags):
        return rerun_command(parser.prog, args, sessions, *flags)

    if not converting and agent == "claude":
        resume_sid, cwd = move_claude(args, sessions, usage, sid, source, files, target, again)
    elif not converting:
        resume_sid, cwd = copy_codex(args, sessions, sid, source, files, target, again)
    else:
        if agent == "claude":
            live = sessions.open_sessions([source]).get((os.path.realpath(source), sid))
            if live:
                sys.exit("Session %s is open in pid %s%s; quit it first, so it stops changing"
                         % (sid[:8], live.get("pid"), " (%s)" % live["status"] if live.get("status") else ""))
        resume_sid, cwd = convert(args, sessions, agent, sid, source, files, target_agent, target, why)

    # In a terminal, offer to run that now. Never when piped, so scripts don't stop to ask.
    if not args.dry_run and resume_sid and cwd and os.path.isdir(cwd) and sys.stdin.isatty() and sys.stdout.isatty():
        try:
            answer = input("Resume it now in %s? [y/N] " % show(target))
        except (EOFError, KeyboardInterrupt):
            print()
            answer = ""
        if answer.strip().lower() in ("y", "yes"):
            resume_now(target, resume_sid, cwd, target_agent)


if __name__ == "__main__":
    main()
