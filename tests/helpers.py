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
with open(os.path.join(here, "claude.log"), "a", encoding="utf-8") as fh:
    fh.write(json.dumps({"args": sys.argv[1:], "cwd": os.getcwd(), "env": shown}) + "\n")
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
        run is logged, with its arguments, folder and environment, in claude.log beside it.
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
        """What the fake claude was run with, one dict per run."""
        log = os.path.join(folder or os.path.join(self.root, "fakebin"), "claude.log")
        if not os.path.exists(log):
            return []
        with open(log, encoding="utf-8") as fh:
            return [json.loads(line) for line in fh if line.strip()]

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
