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
               if k not in ("CLAUDE_CONFIG_DIR", "CODEX_HOME", "DSH_HOME", "SHELL", "NO_COLOR", "TERM_PROGRAM", "TERMINAL")
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
sys.stdin.read()  # agy's print mode reads its stdin to the end first: hatcheck must hand it none
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
    if os.path.exists(os.path.join(root, "fake-slow")):
        import time
        time.sleep(2)
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


# Kimi Code CLI (kimi): a home as kimi 2.2.0 keeps it, with sessions in kimi's own shape.

def kimi_wd_key(cwd):
    """The folder kimi files a working directory's sessions under: wd_<name>_<sha256[:12]>."""
    import hashlib
    return "wd_%s_%s" % (os.path.basename(cwd), hashlib.sha256(cwd.encode("utf-8")).hexdigest()[:12])


def _kimi_home(self, name="default", signed_in=False, expires=None):
    """A Kimi home (~/.kimi-code or ~/.kimi-code-NAME), with an empty sessions/ folder; signed in,
    with the token file kimi's own store writes, when asked. The token has expired unless
    `expires` says otherwise, so nothing asks the API."""
    path = os.path.join(self.home, ".kimi-code" if name == "default" else ".kimi-code-" + name)
    os.makedirs(os.path.join(path, "sessions"), exist_ok=True)
    if signed_in:
        write(os.path.join(path, "credentials", "kimi-code.json"), json.dumps({
            "access_token": "kimi-test-token", "refresh_token": "x", "expires_at": int(expires or NOW - 3600),
            "scope": "", "token_type": "Bearer", "expires_in": 0}, indent=2) + "\n")
    return path


def _kimi_session(self, home, cwd, sid, title, minutes_ago=5, model="kimi-code/k3", indexed=True, extra=()):
    """A session as kimi keeps one: its folder under the working directory's key, state.json,
    the main agent's wire with your prompt, a reply that thinks and calls a tool, the tool's
    output and the final reply, and its line in the home's index. Returns the folder."""
    when = NOW - minutes_ago * 60
    start = when - 60
    ms = lambda t: int(t * 1000)
    folder = os.path.join(home, "sessions", kimi_wd_key(cwd), "session_" + sid)
    main = os.path.join(folder, "agents", "main")
    pid = "msg_01TESTPROMPT"
    tool = "tool_TEST0001"

    def ev(etype, t, **fields):
        return json.dumps(dict({"type": etype}, **fields, time=ms(t)), separators=(",", ":"))  # compact, as kimi writes

    def appended(t, message, meta):
        return ev("agent.message.appended", t, message={"message": message, "meta": meta})

    lines = [
        json.dumps({"type": "metadata", "protocol_version": "1.5", "created_at": ms(start)}, separators=(",", ":")),
        ev("runtime.set_binding", start, workspaceId=kimi_wd_key(cwd), runtimeId="local", agentId="main"),
        ev("profile.bind", start, agentId="main", modelAlias=model, profileName="agent", thinkingEffort="high",
           systemPrompt="You are Kimi Code CLI.", environmentDisclosure={"cwd": cwd}, agentsMdPaths=[],
           activeToolNames=["Read", "Bash"], disallowedTools=[], subagents=[]),
        ev("permission.set_mode", start, agentId="main", mode="default"),
        appended(start + 1, {"role": "user", "content": [{"type": "text", "text": title}]},
                 {"source": "input", "promptId": pid, "origin": {"kind": "user"}, "tracked": True,
                  "createdAt": iso(start + 1), "userMessageId": pid}),
        json.dumps({"turnId": 0, "queueItemId": pid, "type": "agent.turn.started", "time": ms(start + 1), "kind": "event"}, separators=(",", ":")),
        ev("turn.prompt", start + 1, agentId="main", input=[{"type": "text", "text": title}], origin={"kind": "user"},
           promptId=pid, turnId=0),
        ev("context.append_message", start + 1, agentId="main",
           message={"role": "user", "content": [{"type": "text", "text": title}], "id": pid, "toolCalls": [], "origin": {"kind": "user"}}),
        ev("llm.request", start + 2, agentId="main", kind="loop", provider="openai", model=model.split("/")[-1], modelAlias=model,
           thinkingEffort="high", thinkingKeep="all", maxTokens=1048576, toolSelect=False, messageCount=2, turnStep="0.1"),
        appended(start + 3, {"role": "assistant", "content": [{"type": "think", "think": "kept to itself", "reasoningKey": "reasoning_content"}],
                             "toolCalls": [{"type": "function", "id": tool, "name": "Bash", "arguments": json.dumps({"command": "ls src"})}]},
                 {"model": {"provider": "agent-loop", "model": "agent-loop"}, "source": "llm",
                  "usage": {"inputOther": 10, "output": 5, "inputCacheRead": 0, "inputCacheCreation": 0},
                  "finish": {"finishReason": "tool_calls", "rawFinishReason": "tool_calls"}, "messageId": "chatcmpl-test1"}),
        appended(start + 4, {"role": "tool", "content": [{"type": "text", "text": "main.py\nutil.py\n"}], "toolCallId": tool},
                 {"source": "tool"}),
    ] + list(extra) + [
        appended(when, {"role": "assistant", "content": [{"type": "text", "text": "Done: " + title}], "toolCalls": []},
                 {"model": {"provider": "agent-loop", "model": "agent-loop"}, "source": "llm",
                  "usage": {"inputOther": 10, "output": 5, "inputCacheRead": 0, "inputCacheCreation": 0},
                  "finish": {"finishReason": "stop", "rawFinishReason": "stop"}, "messageId": "chatcmpl-test2"}),
        json.dumps({"turnId": 0, "outcome": "done", "type": "agent.turn.ended", "time": ms(when), "kind": "event"}, separators=(",", ":")),
        ev("turn.ended", when, agentId="main", turnId=0, reason="completed", durationMs=1000),
        ev("prompt.completed", when, agentId="main", promptId=pid, finishedAt=iso(when), reason="completed"),
    ]
    write(os.path.join(main, "wire.jsonl"), "\n".join(lines) + "\n", mtime=when)
    write(os.path.join(folder, "state.json"), json.dumps({
        "id": "session_" + sid, "version": 2, "cwd": cwd, "createdAt": ms(start), "updatedAt": ms(when), "archived": False,
        "agents": {"main": {"homedir": main, "type": "main"}}, "custom": {}, "lastPrompt": title, "title": title,
        "titleKind": "replaceable", "lastTurnReason": "completed", "isCustomTitle": False}, separators=(",", ":")),
        mtime=when)
    if indexed:
        with open(os.path.join(home, "session_index.jsonl"), "a", encoding="utf-8") as fh:
            fh.write(json.dumps({"sessionId": "session_" + sid, "sessionDir": folder, "workDir": cwd}, separators=(",", ":")) + "\n")
    return folder


FakeHome.kimi_home = _kimi_home
FakeHome.kimi_session = _kimi_session


# ZCode (zcode): its data folder as ZCode 0.16.9 keeps it, the session database with the
# tables hatcheck reads.

ZCODE_SCHEMA = [
    "CREATE TABLE session (id text primary key, project_id text not null, workspace_id text, parent_id text, slug text not null, "
    "directory text not null, path text, title text not null, version text not null, share_url text, summary_additions integer, "
    "summary_deletions integer, summary_files integer, summary_diffs text, revert text, permission text, time_created integer not null, "
    "time_updated integer not null, time_compacting integer, time_archived integer, task_type text not null default 'interactive', "
    "title_source text not null default 'first_input', title_message_id text, time_title_updated integer, trace_id text)",
    "CREATE TABLE message (id text primary key, session_id text not null references session(id) on delete cascade, "
    "time_created integer not null, time_updated integer not null, data text not null, sequence integer)",
    "CREATE TABLE part (id text primary key, message_id text not null references message(id) on delete cascade, session_id text not null, "
    "time_created integer not null, time_updated integer not null, data text not null, sequence integer)",
    "CREATE TABLE schema_migration (id text primary key, checksum text not null, app_version text, time_applied integer not null)",
]


def _zcode_root(self):
    """ZCode's data folder, ~/.zcode, with an empty session database."""
    import sqlite3
    root = os.path.join(self.home, ".zcode")
    os.makedirs(os.path.join(root, "cli", "db"), exist_ok=True)
    os.makedirs(os.path.join(root, "cli", "artifacts"), exist_ok=True)
    db = sqlite3.connect(os.path.join(root, "cli", "db", "db.sqlite"))
    for statement in ZCODE_SCHEMA:
        db.execute(statement)
    db.execute("INSERT INTO schema_migration VALUES ('0022_backfilled_session_reasoning', 'x', '0.16.5', ?)", (int(NOW * 1000),))
    db.commit()
    db.close()
    return root


def _zcode_session(self, root, cwd, sid, title, minutes_ago=5, parent=None, task_type="interactive", artifacts=False):
    """A session as ZCode keeps one: its row, your message, and a reply made of reasoning, a tool
    call with its output, and text, as part rows. `parent` makes it one zcode ran itself."""
    import sqlite3
    when = NOW - minutes_ago * 60
    start = when - 60
    ms = lambda t: int(t * 1000)
    full = "sess_" + sid
    db = sqlite3.connect(os.path.join(root, "cli", "db", "db.sqlite"))
    db.execute("INSERT INTO session (id, project_id, parent_id, slug, directory, title, version, time_created, time_updated, task_type) "
               "VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
               (full, "proj_" + re.sub(r"[^a-z0-9]+", "-", cwd.lower()).strip("-"), parent and "sess_" + parent, full, cwd, title,
                "0.16.5", ms(start), ms(when), task_type))
    model = {"providerID": "builtin:zai-coding-plan", "modelID": "GLM-5.3"}

    def message(mid, t, data):
        db.execute("INSERT INTO message (id, session_id, time_created, time_updated, data) VALUES (?, ?, ?, ?, ?)",
                   (mid, full, ms(t), ms(t), json.dumps(data)))

    def part(pid, mid, t, data, sequence):
        db.execute("INSERT INTO part (id, message_id, session_id, time_created, time_updated, data, sequence) VALUES (?, ?, ?, ?, ?, ?, ?)",
                   (pid, mid, full, ms(t), ms(t), json.dumps(data), sequence))

    message("msg_u1_" + sid[:8], start, {"role": "user", "time": {"created": ms(start)}, "agent": "zcode-agent", "model": model})
    part("prt_u1_" + sid[:8], "msg_u1_" + sid[:8], start, {"type": "text", "text": title}, 1)
    message("msg_a1_" + sid[:8], when, dict({"role": "assistant", "time": {"created": ms(start + 1), "completed": ms(when)},
                                              "parentID": "msg_u1_" + sid[:8], "agent": "zcode-agent", "path": {"cwd": cwd, "root": cwd},
                                              "finish": "stop"}, **model))
    part("prt_a0_" + sid[:8], "msg_a1_" + sid[:8], start + 1, {"type": "step-start"}, 1)
    part("prt_a1_" + sid[:8], "msg_a1_" + sid[:8], start + 2, {"type": "reasoning", "text": "kept to itself"}, 2)
    part("prt_a2_" + sid[:8], "msg_a1_" + sid[:8], start + 3, {"type": "tool", "callID": "call_test1", "tool": "Bash",
         "state": {"status": "completed", "input": {"command": "ls src", "description": "List"}, "output": "main.py\nutil.py\n"}}, 3)
    part("prt_a3_" + sid[:8], "msg_a1_" + sid[:8], when, {"type": "text", "text": "Done: " + title,
         "time": {"start": ms(start + 4), "end": ms(when)}}, 4)
    part("prt_a4_" + sid[:8], "msg_a1_" + sid[:8], when, {"type": "step-finish", "reason": "stop", "cost": 0}, 5)
    db.commit()
    db.close()
    if artifacts:
        write(os.path.join(root, "cli", "artifacts", full, "call_test1-tool-result-x.json"), json.dumps({"output": "main.py\n"}))
    return full


FakeHome.zcode_root = _zcode_root
FakeHome.zcode_session = _zcode_session


# DeepSeek Harness (dsh): v4 session logs as dsh 0.2.1-rc.2 writes them.

DSH_COMPRESSED_ID = "d54a1f70-3b2c-4f0e-9a8b-7c6d5e4f3a2b"
# Two independent frames from zstd 1.5.7's ZSTD_compress at level 3: the v4 header for
# session-d54a1f70..., then its events (including "Compressed DSH session" and "Decoded the
# second frame"). Embedding them keeps zstd executables and Python packages out of the tests.
DSH_ZSTD_FRAMES = (
    "KLUv/SCbDQQAgsgcHlCFWAekogD2MF2ggRZZIV5VR1p7rv3H5+DGzCI6DIGA7wHKZ73+hG2crN/9fBBOVIVVwzCdyIii02rLLe1wCrK44xka6CH4GZ3MIDCR7E0F0SlGgYv6kkxFhEnUcvKArKnSxom9eYWTE06z+/XznwVkgAMAhBRC9LlF29IE",
    "KLUv/WDgAeUJAEaRNCFAi6oDe3m1IkT3GjrVyJaOSR/vEucReU2/oZUiFcPw6ZorACkALACPUTk5GnypiGzUByG0YAgsikWCoViSz7T1QcmTq4+PoFTPIOS1cfIjw7ohl5GBS3nCKmdwveF0vAvfR/K+74PLZ7LXJF+ygQtrPnZQuiFZrWI+Zi/MyYldd1eEO4PMtGSpA+dUBIIeyTq5w6fA0SBwYixu2yIOc0KmH2yqV4VdNYc5IY9kRl2u+pxRMqKwB9eVybOmAGX0UfGrTzaL0EDAPrXaVJUBJL/GPyogYKIhdOkBDzKyQl6HQlCIwSBFAM8NEh+A/75NoraA9jZkS4A6AkNL6Iy72wtyIvzua1PbAu0oZoBpJYkcijqdDWdcWkYIe6ADbBPwOidbMv2ZbNu/PDkKCMoHkXJo1WDf4VUTkaM=",
)


def _dsh_home(self, name="default", path=None):
    """A DSH home (~/.dsh or ~/.dsh-NAME), or a custom home for DSH_HOME, with sessions/."""
    path = path or os.path.join(self.home, ".dsh" if name == "default" else ".dsh-" + name)
    os.makedirs(os.path.join(path, "sessions"), exist_ok=True)
    return path


def _dsh_session(self, home, cwd, sid, title, minutes_ago=5, origin="cli", compact=False,
                 model="deepseek-reasoner", prompt=None):
    """A raw v4 DSH log: direct and injected user messages, an assistant's reasoning, text and
    tool call, the tool result and final reply, then superseded and current title/model records.
    With compact, a compact checkpoint replaces the first two surface events."""
    when = NOW - minutes_ago * 60
    start = when - 60
    ms = lambda t: int(t * 1000)  # noqa: E731
    full = "session-" + sid
    header = {"type": "session", "version": 4, "id": full, "createdAt": ms(start), "cwd": cwd,
              "origin": origin, "isSeeded": False}

    def event(seq, kind, data):
        return {"type": kind, "seq": seq, "time": ms(start + seq), "surfaceOp": "append", "data": data}

    def message(mid, role, kind, content, **extra):
        return dict({"role": role, "id": mid, "source": {"kind": kind}, "content": content}, **extra)

    text = lambda value: [{"type": "text", "text": value}]  # noqa: E731
    rows = [
        header,
        event(0, "user/message", message("u1", "user", "user", text(prompt or title))),
        event(1, "user/message", message("u2", "user", "runtime-context", text("INJECTED RUNTIME CONTEXT"))),
        event(2, "assistant/message", {"turn": 1, "step": 1, "message": {
            "role": "assistant", "id": "a1", "source": {"kind": "model", "provider": "deepseek", "model": "deepseek-previous"},
            "content": [{"type": "reasoning", "text": "kept to itself"}, text("I'll inspect it.")[0],
                        {"type": "tool-call", "id": "c1", "name": "read_file",
                         "arguments": json.dumps({"path": "src/main.rs"})}]}, "stream": []}),
        event(3, "tool/result", {"turn": 1, "step": 1, "message": message(
            "t1", "tool", "tool", text("fn main() {}\n"), toolCallId="c1")}),
        event(4, "assistant/message", {"turn": 1, "step": 2, "message": {
            "role": "assistant", "id": "a2", "source": {"kind": "model", "provider": "deepseek", "model": "deepseek-previous"},
            "content": text("Done: " + title)}, "stream": []}),
        {"type": "session/title", "seq": 5, "time": ms(start + 5),
         "data": {"title": "Draft: " + title, "messageSeqs": [0], "source": {"kind": "fallback"}}},
        {"type": "request/context", "seq": 6, "time": ms(start + 6),
         "data": {"provider": "deepseek", "model": "deepseek-old"}},
        {"type": "session/title", "seq": 7, "time": ms(start + 7),
         "data": {"title": title, "messageSeqs": [0], "source": {"kind": "generated"}}},
        {"type": "model/selection", "seq": 8, "time": ms(start + 8), "data": {"model": model}},
    ]
    if compact:
        rows.append({"type": "user/message", "seq": 9, "time": ms(start + 9),
                     "surfaceOp": {"op": "replace", "startSeq": 0, "endSeq": 1}, "sourceEventSeqs": [0, 1],
                     "data": message("u3", "user", "compact-checkpoint", text("Compacted checkpoint: " + title))})
    project = re.sub(r"[^A-Za-z0-9_.-]+", "-", cwd).strip("-") or "project"
    path = os.path.join(home, "sessions", project, full, "session.v4.jsonl")
    write(path, "\n".join(rec(**row) for row in rows) + "\n", mtime=when)
    return path


def _dsh_compressed_session(self, home, minutes_ago=5):
    """The two embedded frames above, as one session.v4.jsonl.zstd with a header and events."""
    folder = os.path.join(home, "sessions", "compressed", "session-" + DSH_COMPRESSED_ID)
    path = os.path.join(folder, "session.v4.jsonl.zstd")
    os.makedirs(folder, exist_ok=True)
    with open(path, "wb") as fh:
        for frame in DSH_ZSTD_FRAMES:
            fh.write(base64.b64decode(frame))
    when = NOW - minutes_ago * 60
    os.utime(path, (when, when))
    return path


class DshOpen:
    """Hold the flock dsh holds on session.lock while a session is open. Not on Windows."""

    def __init__(self, log):
        self.path = os.path.join(os.path.dirname(log), "session.lock")

    def __enter__(self):
        write(self.path, "")
        self.fh = open(self.path, "r+")
        if not WINDOWS:
            import fcntl
            fcntl.flock(self.fh, fcntl.LOCK_EX)
        return self

    def __exit__(self, *_):
        self.fh.close()


FakeHome.dsh_home = _dsh_home
FakeHome.dsh_session = _dsh_session
FakeHome.dsh_compressed_session = _dsh_compressed_session
