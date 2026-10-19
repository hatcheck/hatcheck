"""Shared test helpers: a throwaway home folder with Claude Code profiles, Codex homes and
their sessions, and a way to run hatcheck against it. Nothing here reads your real profiles,
the macOS Keychain or the network.

The tests run the hatcheck binary and the launchers built beside it: HATCHECK_ENGINE=path names
one, and without it they build target/debug/hatcheck with cargo and run that."""
import base64
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time
from datetime import datetime, timezone

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
WINDOWS = os.name == "nt"
sys.dont_write_bytecode = True  # leave no __pycache__ beside the tests


def built_engine():
    """target/debug/hatcheck, built now, so the tests run what's in the repo."""
    try:
        p = subprocess.run(["cargo", "build", "-q", "-p", "hatcheck"], cwd=REPO, capture_output=True, text=True)
    except FileNotFoundError:
        raise RuntimeError("The tests build hatcheck with cargo, which isn't on your PATH: install Rust, "
                           "or set HATCHECK_ENGINE to a hatcheck binary") from None
    if p.returncode != 0:
        raise RuntimeError("cargo build -p hatcheck failed:\n" + p.stdout + p.stderr)
    return os.path.join(REPO, "target", "debug", "hatcheck" + (".exe" if WINDOWS else ""))


ENGINE = os.environ.get("HATCHECK_ENGINE") or built_engine()  # the hatcheck under test
NOW = time.time()  # after the build, so it doesn't count against the times the tests check


def iso(t):
    return datetime.fromtimestamp(t, timezone.utc).isoformat()


def rec(**fields):
    """A transcript line written the way Claude Code writes them: compact JSON, raw UTF-8."""
    return json.dumps(fields, ensure_ascii=False, separators=(",", ":"))


def fake_jwt(claims):
    """A token shaped like the JWTs in Codex's auth.json, unsigned."""
    part = lambda d: base64.urlsafe_b64encode(json.dumps(d).encode()).decode().rstrip("=")  # noqa: E731
    return "%s.%s.x" % (part({"alg": "none"}), part(claims))


def write(path, text, mtime=None):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w", encoding="utf-8", newline="\n") as fh:
        fh.write(text)
    if mtime is not None:
        os.utime(path, (mtime, mtime))


def argv_of(script):
    """The command line that runs a hatcheck subcommand (usage, move) with the binary under test."""
    return [ENGINE, script]


INTERRUPT = object()  # for in_terminal: Ctrl+C at that prompt, as SIGINT

# The fake claude FakeHome.fake_claude installs (see there).
FAKE_CLAUDE = r'''
import json, os, sys, time
here = os.path.dirname(os.path.abspath(__file__))
home = os.environ.get("USERPROFILE") if os.name == "nt" else os.environ.get("HOME")
config_dir = os.environ.get("CLAUDE_CONFIG_DIR")
profile = config_dir or os.path.join(home, ".claude")
config = os.path.join(config_dir, ".claude.json") if config_dir else os.path.join(home, ".claude.json")
shown = {k: v for k, v in os.environ.items() if k.startswith(("ANTHROPIC_", "CLAUDE", "DISABLE_"))}
# A file per run: runs at once appending to one file can write over each other on Windows.
runs = os.path.join(here, "claude-runs")
os.makedirs(runs, exist_ok=True)
with open(os.path.join(runs, "%d-%d.json" % (time.time_ns(), os.getpid())), "w", encoding="utf-8") as fh:
    json.dump({"args": sys.argv[1:], "cwd": os.getcwd(), "env": shown}, fh)
if sys.argv[1:] == ["--version"]:
    print("2.1.303 (Claude Code)")
    sys.exit(0)
fake = os.path.join(profile, "fake-usage.json")
if "/usage" in sys.argv and os.path.exists(fake):
    with open(config, encoding="utf-8") as fh:
        data = json.load(fh)
    with open(fake, encoding="utf-8") as fh:
        usage = json.load(fh)
    data["cachedUsageUtilization"] = {"fetchedAtMs": int(time.time() * 1000),
                                      "accountUuid": (data.get("oauthAccount") or {}).get("accountUuid"),
                                      "utilization": usage}
    with open(config, "w", encoding="utf-8") as fh:
        json.dump(data, fh)
    text = "You are currently using your subscription to power your Claude Code usage\n\n" + "\n".join(
        "Current %s: %s%% used" % ("session" if r.get("kind") == "session" else "week", r.get("percent", 0))
        for r in usage.get("limits", []))
elif "/usage" in sys.argv and os.path.exists(os.path.join(profile, "stale-usage")):
    text = "Current session: 11% used · resets Oct 18 at 3:49am (UTC)"  # an old reading, nothing new
else:
    text = "Total cost:            $0.0000\nTotal duration (API):  0s"
print(json.dumps({"type": "result", "is_error": False, "subtype": "success", "result": text}))
'''


def in_terminal(argv, replies, env, cwd):
    """Run argv on a pseudo-terminal, as someone at one would: each (prompt, keys) in replies
    is typed once that prompt shows, in order. keys are bytes (b"y\\n"; b"\\x04" ends the input)
    or INTERRUPT. Returns the exit code and everything it showed, with \\n line ends. Not on
    Windows."""
    import pty
    import select
    import signal
    master, slave = pty.openpty()
    p = subprocess.Popen(argv, stdin=slave, stdout=slave, stderr=slave, env=env, cwd=cwd)
    os.close(slave)
    out, seen, replies, deadline = b"", 0, list(replies), time.time() + 30
    while time.time() < deadline:
        ready, _, _ = select.select([master], [], [], 0.2)
        if ready:
            try:
                chunk = os.read(master, 4096)
            except OSError:  # Linux, once the other end has closed
                chunk = b""
            if not chunk:
                break
            out += chunk
        elif p.poll() is not None:
            break
        if replies and replies[0][0] in out[seen:]:
            prompt, keys = replies.pop(0)
            seen = out.index(prompt, seen) + len(prompt)
            time.sleep(0.1)  # so it's reading by now
            if keys is INTERRUPT:
                p.send_signal(signal.SIGINT)
            else:
                os.write(master, keys)
    p.wait(timeout=10)
    os.close(master)
    return p.returncode, out.decode("utf-8", "replace").replace("\r\n", "\n")


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


def start_token(pid):
    """What Claude Code records in sessions/<pid>.json for when a process started."""
    if WINDOWS:
        return {"procStartFt": str(windows_start_time(pid))}
    out = subprocess.run(["ps", "-o", "lstart=", "-p", str(pid)], capture_output=True, text=True,
                         env=dict(os.environ, TZ="UTC", LC_ALL="C")).stdout
    return {"procStart": out.strip()}


class FakeHome:
    def __init__(self):
        # A short name: Windows paths top out at 260 characters, and project folders repeat the path.
        self.root = os.path.realpath(tempfile.mkdtemp(prefix="cma-"))
        self.home = os.path.join(self.root, "home")
        os.makedirs(self.home)

    def cleanup(self):
        shutil.rmtree(self.root, ignore_errors=True)

    def profile(self, name="default", account=None, projects=True, usage=None, saved_at=None, plan=None):
        """A profile folder (~/.claude or ~/.claude-NAME), signed in to `account`, with an
        optional usage snapshot. `plan` adds fields to the oauthAccount Claude Code keeps in
        .claude.json, such as organizationType and userRateLimitTier."""
        path = os.path.join(self.home, ".claude" if name == "default" else ".claude-" + name)
        os.makedirs(os.path.join(path, "projects") if projects else path, exist_ok=True)
        config = {"oauthAccount": dict({"accountUuid": account}, **(plan or {}))} if account else {}
        if usage is not None:
            config["cachedUsageUtilization"] = {"fetchedAtMs": (saved_at or NOW) * 1000,
                                                "accountUuid": account, "utilization": usage}
        write(self.config_path(name), json.dumps(config))
        return path

    def config_path(self, name="default"):
        """The profile's .claude.json: beside ~/.claude for the default profile, inside the
        folder for the others, as Claude Code keeps it."""
        if name == "default":
            return os.path.join(self.home, ".claude.json")
        return os.path.join(self.home, ".claude-" + name, ".claude.json")

    def fake_claude(self, folder=None):
        """A fake `claude` on a folder of its own, for the live usage path: `claude -p /usage`
        saves the profile's fake-usage.json as the usage snapshot in its .claude.json, the way
        Claude Code saves what Anthropic told it, and prints a result as Claude Code does; a
        profile without that file gets a cost summary, as one that isn't signed in does. Every
        run is logged, with its arguments, folder and environment, in claude-runs/ beside it.
        Returns the folder, to put first on the PATH."""
        folder = folder or os.path.join(self.root, "fakebin")
        script = os.path.join(folder, "claude.py")
        write(script, FAKE_CLAUDE)
        if WINDOWS:
            write(os.path.join(folder, "claude.cmd"), '@"%s" "%%~dp0claude.py" %%*\r\n' % sys.executable)
        else:
            wrapper = os.path.join(folder, "claude")
            # ${0%/*}, not dirname: the tests' PATH is /bin alone on macOS, which has no dirname.
            write(wrapper, '#!/bin/sh\nexec "%s" -I "${0%%/*}/claude.py" "$@"\n' % sys.executable)
            os.chmod(wrapper, 0o755)
        return folder

    def claude_runs(self, folder=None):
        """What the fake claude was run with, one dict per run, in the order they started."""
        runs = os.path.join(folder or os.path.join(self.root, "fakebin"), "claude-runs")
        if not os.path.isdir(runs):
            return []
        found = []
        for name in sorted(os.listdir(runs), key=lambda n: [int(part) for part in n[:-len(".json")].split("-")]):
            with open(os.path.join(runs, name), encoding="utf-8") as fh:
                found.append(json.load(fh))
        return found

    def folder(self, *parts):
        path = os.path.join(self.home, *parts)
        os.makedirs(path, exist_ok=True)
        return path

    def session(self, profile, cwd, sid, title, minutes_ago=5, entrypoint="cli", extra=(), model=None):
        """A transcript in profile/projects/<cwd as Claude Code names it>/<sid>.jsonl, its
        reply from `model` when given."""
        when = NOW - minutes_ago * 60
        base = {"cwd": cwd, "sessionId": sid, "entrypoint": entrypoint, "gitBranch": "main"}
        reply = dict({"model": model} if model else {}, role="assistant", content=[{"type": "text", "text": "ok"}])
        lines = [rec(type="user", timestamp=iso(when - 30), message={"role": "user", "content": title}, **base),
                 rec(type="assistant", timestamp=iso(when), message=reply, **base)]
        lines += list(extra) + [rec(type="ai-title", aiTitle=title, sessionId=sid)]
        path = os.path.join(profile, "projects", re.sub(r"[^A-Za-z0-9]", "-", cwd), sid + ".jsonl")
        write(path, "\n".join(lines) + "\n", mtime=when)
        return path

    def codex_home(self, name="default", account=None, plan="plus", expires=None):
        """A Codex home (~/.codex or ~/.codex-NAME), signed in with ChatGPT to `account` when
        given. Its token has expired unless `expires` says otherwise, so nothing asks the API."""
        path = os.path.join(self.home, ".codex" if name == "default" else ".codex-" + name)
        os.makedirs(os.path.join(path, "sessions"), exist_ok=True)
        if account:
            auth = {"chatgpt_account_id": account, "chatgpt_user_id": "user-" + account, "chatgpt_plan_type": plan}
            claims = {"https://api.openai.com/auth": auth, "exp": int(expires or NOW - 3600)}
            write(os.path.join(path, "auth.json"), json.dumps({"OPENAI_API_KEY": None, "last_refresh": iso(NOW), "tokens": {
                "access_token": fake_jwt(claims), "account_id": account, "refresh_token": "x",
                "id_token": fake_jwt(dict(claims, email=account + "@example.com"))}}))
        return path

    def codex_session(self, home, cwd, sid, prompt, minutes_ago=5, source="cli", originator="codex_cli_rs",
                      rate_limits=None, name=None, extra=()):
        """A Codex session file, the way Codex writes one: the instructions it adds itself, your
        prompt, a command it ran and its answer, and the rate limits it was told about."""
        when = NOW - minutes_ago * 60
        start = when - 60

        def line(t, kind, payload):
            return rec(timestamp=iso(t), type=kind, payload=payload)

        lines = [
            line(start, "session_meta", {"id": sid, "timestamp": iso(start), "cwd": cwd, "originator": originator,
                                         "cli_version": "0.155.1", "source": source, "model_provider": "openai",
                                         "git": {"branch": "main"}}),
            line(start, "response_item", {"type": "message", "role": "developer", "content": [
                {"type": "input_text", "text": "<permissions instructions>sandboxed</permissions instructions>"}]}),
            line(start, "response_item", {"type": "message", "role": "user", "content": [
                {"type": "input_text", "text": "<environment_context><cwd>%s</cwd></environment_context>" % cwd}]}),
            line(start, "turn_context", {"cwd": cwd, "model": "gpt-5.5"}),
            line(start + 1, "response_item", {"type": "message", "role": "user", "content": [
                {"type": "input_text", "text": prompt}]}),
            line(start + 2, "response_item", {"type": "reasoning", "summary": [], "content": None,
                                              "encrypted_content": "gAAAA-SECRET-REASONING"}),
            line(start + 3, "response_item", {"type": "function_call", "name": "exec_command", "call_id": "c1",
                                              "arguments": json.dumps({"cmd": "ls src", "yield_time_ms": 1000})}),
            line(start + 4, "response_item", {"type": "function_call_output", "call_id": "c1", "output":
                                              "Chunk ID: 1\nProcess exited with code 0\nOutput:\nmain.py\nutil.py\n"}),
            line(start + 5, "response_item", {"type": "custom_tool_call", "name": "apply_patch", "call_id": "c2",
                                              "input": "*** Begin Patch\n*** Update File: src/main.py\n+print(1)\n*** End Patch"}),
            line(start + 6, "response_item", {"type": "custom_tool_call_output", "call_id": "c2",
                                              "output": json.dumps({"output": "Success. Updated src/main.py"})}),
        ] + list(extra) + [
            line(when, "response_item", {"type": "message", "role": "assistant", "content": [
                {"type": "output_text", "text": "Done: " + prompt}], "phase": "final_answer"}),
        ]
        if rate_limits is not None:
            lines.append(line(when, "event_msg", {"type": "token_count", "info": None, "rate_limits": rate_limits}))
        local = time.localtime(start)
        path = os.path.join(home, "sessions", *time.strftime("%Y %m %d", local).split(),
                            "rollout-%s-%s.jsonl" % (time.strftime("%Y-%m-%dT%H-%M-%S", local), sid))
        write(path, "\n".join(lines) + "\n", mtime=when)
        if name:
            with open(os.path.join(home, "session_index.jsonl"), "a", encoding="utf-8") as fh:
                fh.write(json.dumps({"id": sid, "thread_name": name, "updated_at": iso(when)}) + "\n")
        return path

    def register_open(self, profile, sid, pid=None):
        """Mark a session as open in a running process (this one by default)."""
        pid = pid or os.getpid()
        entry = dict({"pid": pid, "sessionId": sid, "status": "idle"}, **start_token(pid))
        write(os.path.join(profile, "sessions", "%d.json" % pid), json.dumps(entry))

    def provider_config(self, provider, text):
        """hatcheck provider's config for z or k, where it looks for it on this OS."""
        path = (os.path.join(self.home, "AppData", "Roaming", "claude-" + provider, "config") if WINDOWS
                else os.path.join(self.home, ".config", "claude-" + provider, "config"))
        write(path, text)
        return path

    def env(self):
        env = {k: v for k, v in os.environ.items()
               if k not in ("CLAUDE_CONFIG_DIR", "CODEX_HOME", "SHELL", "NO_COLOR", "TERM_PROGRAM", "TERMINAL")
               and not k.startswith(("ANTHROPIC_", "CLAUDE_CODE_", "HATCHECK_"))}
        env.update(HOME=self.home, USERPROFILE=self.home, NO_COLOR="1", PYTHONIOENCODING="utf-8",
                   PYTHONDONTWRITEBYTECODE="1", APPDATA=os.path.join(self.home, "AppData", "Roaming"),
                   LOCALAPPDATA=os.path.join(self.home, "AppData", "Local"),
                   XDG_CACHE_HOME=os.path.join(self.home, ".cache"))  # caches stay in this home too
        if sys.platform == "darwin":
            env["PATH"] = "/bin"  # has ps, but not the security tool, so the real Keychain stays untouched
        return env

    def run(self, script, *args, cwd=None, **extra_env):
        """Run a hatcheck subcommand (usage) or another repo script against this home:
        (exit code, stdout + stderr). extra_env adds to or overrides the environment."""
        p = subprocess.run(argv_of(script) + list(args), cwd=cwd or self.root,
                           env=dict(self.env(), **extra_env), capture_output=True, text=True, encoding="utf-8")
        return p.returncode, p.stdout + p.stderr

    def shown(self, path):
        """How the scripts print a path in the home folder: ~/... (~\\... in PowerShell)."""
        return "~" + os.sep + os.path.relpath(path, self.home)


# Antigravity CLI (agy): a data folder as agy 1.4.2 keeps it, and a fake agy for its /usage.

# The index agy keeps of its conversations, as agy 1.4.2 creates it.
AGY_SCHEMA = ('CREATE TABLE `conversation_summaries` (`conversation_id` text,`title` text NOT NULL DEFAULT "",'
              '`preview` text NOT NULL DEFAULT "",`step_count` integer NOT NULL DEFAULT 0,'
              '`last_modified_time` datetime NOT NULL,`workspace_uris` text NOT NULL,`status` text NOT NULL DEFAULT "",'
              '`source` text NOT NULL DEFAULT "",`project_id` text NOT NULL DEFAULT "",`agent_name` text NOT NULL DEFAULT "",'
              '`parent_conversation_id` text NOT NULL DEFAULT "",`nesting_depth` integer NOT NULL DEFAULT 0,'
              '`battle_id` text NOT NULL DEFAULT "",`winning_conversation_id` text NOT NULL DEFAULT "",'
              '`not_fully_idle` numeric NOT NULL DEFAULT false,`killed` numeric NOT NULL DEFAULT false,'
              '`last_user_input_time` datetime NOT NULL,`last_user_input_step_index` integer NOT NULL DEFAULT -1,'
              '`app_data_dir` text NOT NULL DEFAULT "", raw_summary BLOB, group_id TEXT NOT NULL DEFAULT \'\','
              'PRIMARY KEY (`conversation_id`))')

# The fake agy FakeHome.fake_agy installs: `agy -p /usage --output-format json` answers with the
# root's fake-usage.json as the data, or no groups without it, as agy answers in print mode.
FAKE_AGY = r'''
import json, os, sys
here = os.path.dirname(os.path.abspath(__file__))
home = os.environ.get("USERPROFILE") if os.name == "nt" else os.environ.get("HOME")
root = os.path.join(home, ".gemini", "antigravity-cli")
with open(os.path.join(here, "agy.log"), "a", encoding="utf-8") as fh:
    fh.write(json.dumps({"args": sys.argv[1:], "cwd": os.getcwd()}) + "\n")
zero = {"input_tokens": 0, "output_tokens": 0, "thinking_tokens": 0, "cache_read_tokens": 0, "total_tokens": 0}
signed_out = os.path.exists(os.path.join(root, "fake-signed-out"))
if sys.argv[1:] == ["models"]:  # what hatcheck asks first: agy answers it only when signed in
    print("Fetching available models...", file=sys.stderr)
    if signed_out:
        print("Error: Please sign in to view available models. Launch the CLI without arguments to sign in.", file=sys.stderr)
        sys.exit(1)
    print("gemini-4-argon")
    sys.exit(0)
if "/usage" in sys.argv:
    if signed_out:  # the real agy opens the browser on Google's sign-in page here; hatcheck must never get this far
        print("Authentication required. Please visit the URL to log in:", file=sys.stderr)
        sys.exit(1)
    fake = os.path.join(root, "fake-usage.json")
    data = {"groups": []}
    if os.path.exists(fake):
        with open(fake, encoding="utf-8") as fh:
            data = json.load(fh)
    print(json.dumps({"conversation_id": "", "status": "SUCCESS", "response": "", "duration_seconds": 0, "num_turns": 0,
                      "usage": zero, "command": {"name": "usage", "data": data}}))
else:
    print(json.dumps({"conversation_id": "", "status": "ERROR", "response": "", "error": "the fake agy only answers /usage",
                      "duration_seconds": 0, "num_turns": 0, "usage": zero}))
    sys.exit(1)
'''


def agy_stamp(t):
    """A time as agy's index records it: 2026-10-19 13:31:31.244007+00:00."""
    return datetime.fromtimestamp(t, timezone.utc).strftime("%Y-%m-%d %H:%M:%S.%f+00:00")


def _agy_root(self, app=False):
    """Antigravity CLI's data folder, ~/.gemini/antigravity-cli, with an empty index; or, with
    app, the Antigravity app's beside it, ~/.gemini/antigravity, laid out the same way."""
    import sqlite3
    root = os.path.join(self.home, ".gemini", "antigravity" if app else "antigravity-cli")
    for sub in ("conversations", "brain", "presence"):
        os.makedirs(os.path.join(root, sub), exist_ok=True)
    db = sqlite3.connect(os.path.join(root, "conversation_summaries.db"))
    db.execute("PRAGMA journal_mode=WAL")
    db.execute(AGY_SCHEMA)
    db.commit()
    db.close()
    return root


def _agy_session(self, root, cwd, cid, title, minutes_ago=5, app=None, parent="", steps=5, transcript=True):
    """A conversation as agy keeps one: its row in the index, its conversation file (which
    hatcheck never reads), and the transcript agy writes beside it: your prompt, a reply with a
    tool call, the tool's output, a checkpoint of agy's own, and the final reply. `app` is the
    app_data_dir the row names: the folder's own by default, antigravity-cli or antigravity."""
    import sqlite3
    app = app or os.path.basename(root)
    when = NOW - minutes_ago * 60
    start = when - 60
    uri = "file://" + ("/" + cwd.replace("\\", "/") if WINDOWS else cwd)
    db = sqlite3.connect(os.path.join(root, "conversation_summaries.db"))
    db.execute("INSERT INTO conversation_summaries (conversation_id, title, preview, step_count, last_modified_time, "
               "last_user_input_time, workspace_uris, app_data_dir, parent_conversation_id, nesting_depth) "
               "VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
               (cid, title, title + "...", steps, agy_stamp(when), agy_stamp(start), json.dumps([uri]), app, parent,
                1 if parent else 0))
    db.commit()
    db.close()
    write(os.path.join(root, "conversations", cid + ".db"), "SQLite format 3\0" + "x" * 100, mtime=when)
    if not transcript:
        return cid

    def step(n, source, kind, t, **fields):
        return json.dumps(dict({"step_index": n, "source": source, "type": kind, "status": "DONE",
                                "created_at": iso(t)}, **fields))
    lines = [
        step(0, "USER_EXPLICIT", "USER_INPUT", start, content=title),
        step(1, "MODEL", "PLANNER_RESPONSE", start + 1, thinking="kept to itself",
             tool_calls=[{"name": "run_command", "args": {"CommandLine": "ls src", "Cwd": cwd}}]),
        step(2, "MODEL", "GENERIC", start + 2, content="Created At: %s\nOutput:\nmain.py\nutil.py\n" % iso(start + 2)),
        step(3, "SYSTEM", "CHECKPOINT", start + 3, content="a checkpoint of agy's own"),
        step(4, "MODEL", "PLANNER_RESPONSE", when, content="Done: " + title),
    ]
    write(os.path.join(root, "brain", cid, ".system_generated", "logs", "transcript_full.jsonl"),
          "\n".join(lines) + "\n", mtime=when)
    return cid


def _fake_agy(self, folder=None):
    """A fake `agy` on a folder of its own (the fake claude's by default), for the usage path:
    see FAKE_AGY. Returns the folder, to put first on the PATH."""
    folder = folder or os.path.join(self.root, "fakebin")
    script = os.path.join(folder, "agy.py")
    write(script, FAKE_AGY)
    if WINDOWS:
        write(os.path.join(folder, "agy.cmd"), '@"%s" "%%~dp0agy.py" %%*\r\n' % sys.executable)
    else:
        wrapper = os.path.join(folder, "agy")
        write(wrapper, '#!/bin/sh\nexec "%s" -I "${0%%/*}/agy.py" "$@"\n' % sys.executable)
        os.chmod(wrapper, 0o755)
    return folder


def _agy_runs(self, folder=None):
    """What the fake agy was run with, one dict per run."""
    log = os.path.join(folder or os.path.join(self.root, "fakebin"), "agy.log")
    if not os.path.exists(log):
        return []
    with open(log, encoding="utf-8") as fh:
        return [json.loads(line) for line in fh if line.strip()]


class AgyOpen:
    """Hold the lock agy holds on presence/<id>.lock while a conversation is open. Not on
    Windows, where hatcheck doesn't check it."""

    def __init__(self, root, cid):
        self.path = os.path.join(root, "presence", cid + ".lock")

    def __enter__(self):
        write(self.path, "")
        self.fh = open(self.path, "r+")
        if not WINDOWS:
            import fcntl
            fcntl.flock(self.fh, fcntl.LOCK_EX)
        return self

    def __exit__(self, *_):
        self.fh.close()


FakeHome.agy_root = _agy_root
FakeHome.agy_session = _agy_session
FakeHome.fake_agy = _fake_agy
FakeHome.agy_runs = _agy_runs
