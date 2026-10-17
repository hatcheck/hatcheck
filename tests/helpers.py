"""Shared test helpers: a throwaway home folder with Claude Code profiles, Codex homes and
their sessions, and a way to run the scripts against it. Nothing here reads your real
profiles, the macOS Keychain or the network."""
import atexit
import base64
import importlib.util
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
SUBCOMMANDS = ("sessions", "usage", "where", "move", "open", "show", "best", "bind", "unbind", "desktop",
               "provider")  # in hatcheck/
NOW = time.time()
WINDOWS = os.name == "nt"
sys.dont_write_bytecode = True  # leave no __pycache__ beside the scripts
# Code a test runs in this process keeps its cache here, not in your cache folder; the
# scripts a test runs keep theirs in its fake home (FakeHome.env).
os.environ["HATCHECK_CACHE_DIR"] = tempfile.mkdtemp(prefix="cma-cache-")
atexit.register(shutil.rmtree, os.environ["HATCHECK_CACHE_DIR"], True)


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


def command(script):
    """How to run a repo script, after the Python interpreter: a hatcheck subcommand by its
    name (usage) through hatcheck.py, or another script by its file name."""
    if script in SUBCOMMANDS:
        return [os.path.join(REPO, "hatcheck.py"), script]
    return [os.path.join(REPO, script)]


def load(script):
    """One of the repo's scripts, imported as a module: a module of the hatcheck/ folder by its
    name (usage, cli), or another script by its file name."""
    package = os.path.join(REPO, "hatcheck", script + ".py")
    path = package if os.path.isfile(package) else os.path.join(REPO, script)
    spec = importlib.util.spec_from_file_location(re.sub(r"\W", "_", script), path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def start_token(pid):
    """What Claude Code records in sessions/<pid>.json for when a process started."""
    if WINDOWS:
        return {"procStartFt": str(load("sessions").windows_start_time(pid))}
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

    def profile(self, name="default", account=None, projects=True, usage=None, saved_at=None, oauth=None):
        """A profile folder (~/.claude or ~/.claude-NAME), signed in to `account`, with an
        optional usage snapshot and an optional .credentials.json login."""
        path = os.path.join(self.home, ".claude" if name == "default" else ".claude-" + name)
        os.makedirs(os.path.join(path, "projects") if projects else path, exist_ok=True)
        config = {"oauthAccount": {"accountUuid": account}} if account else {}
        if usage is not None:
            config["cachedUsageUtilization"] = {"fetchedAtMs": (saved_at or NOW) * 1000,
                                                "accountUuid": account, "utilization": usage}
        write(os.path.join(self.home, ".claude.json") if name == "default" else os.path.join(path, ".claude.json"),
              json.dumps(config))
        if oauth:
            write(os.path.join(path, ".credentials.json"), json.dumps({"claudeAiOauth": oauth}))
        return path

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

    def run(self, script, *args, cwd=None):
        """Run a hatcheck subcommand (usage) or another repo script against this home:
        (exit code, stdout + stderr)."""
        p = subprocess.run([sys.executable] + command(script) + list(args), cwd=cwd or self.root,
                           env=self.env(), capture_output=True, text=True, encoding="utf-8")
        return p.returncode, p.stdout + p.stderr

    def shown(self, path):
        """How the scripts print a path in the home folder: ~/... (~\\... in PowerShell)."""
        return "~" + os.sep + os.path.relpath(path, self.home)
