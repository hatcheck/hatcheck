"""Shared test helpers: a throwaway home folder with Claude Code profiles and sessions,
and a way to run the scripts against it. Nothing here reads your real profiles, the
macOS Keychain or the network."""
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
NOW = time.time()
WINDOWS = os.name == "nt"
sys.dont_write_bytecode = True  # leave no __pycache__ beside the scripts


def iso(t):
    return datetime.fromtimestamp(t, timezone.utc).isoformat()


def rec(**fields):
    """A transcript line written the way Claude Code writes them: compact JSON, raw UTF-8."""
    return json.dumps(fields, ensure_ascii=False, separators=(",", ":"))


def write(path, text, mtime=None):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w", encoding="utf-8", newline="\n") as fh:
        fh.write(text)
    if mtime is not None:
        os.utime(path, (mtime, mtime))


def load(script):
    """One of the repo's scripts, imported as a module."""
    spec = importlib.util.spec_from_file_location(re.sub(r"\W", "_", script), os.path.join(REPO, script))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def start_token(pid):
    """What Claude Code records in sessions/<pid>.json for when a process started."""
    if WINDOWS:
        return {"procStartFt": str(load("claude-recent-sessions.py").windows_start_time(pid))}
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

    def session(self, profile, cwd, sid, title, minutes_ago=5, entrypoint="cli", extra=()):
        """A transcript in profile/projects/<cwd as Claude Code names it>/<sid>.jsonl."""
        when = NOW - minutes_ago * 60
        base = {"cwd": cwd, "sessionId": sid, "entrypoint": entrypoint, "gitBranch": "main"}
        lines = [rec(type="user", timestamp=iso(when - 30), message={"role": "user", "content": title}, **base),
                 rec(type="assistant", timestamp=iso(when),
                     message={"role": "assistant", "content": [{"type": "text", "text": "ok"}]}, **base)]
        lines += list(extra) + [rec(type="ai-title", aiTitle=title, sessionId=sid)]
        path = os.path.join(profile, "projects", re.sub(r"[^A-Za-z0-9]", "-", cwd), sid + ".jsonl")
        write(path, "\n".join(lines) + "\n", mtime=when)
        return path

    def register_open(self, profile, sid, pid=None):
        """Mark a session as open in a running process (this one by default)."""
        pid = pid or os.getpid()
        entry = dict({"pid": pid, "sessionId": sid, "status": "idle"}, **start_token(pid))
        write(os.path.join(profile, "sessions", "%d.json" % pid), json.dumps(entry))

    def env(self):
        env = {k: v for k, v in os.environ.items() if k not in ("CLAUDE_CONFIG_DIR", "SHELL", "NO_COLOR")}
        env.update(HOME=self.home, USERPROFILE=self.home, NO_COLOR="1", PYTHONIOENCODING="utf-8",
                   PYTHONDONTWRITEBYTECODE="1")
        if sys.platform == "darwin":
            env["PATH"] = "/bin"  # has ps, but not the security tool, so the real Keychain stays untouched
        return env

    def run(self, script, *args, cwd=None):
        """Run a repo script against this home: (exit code, stdout + stderr)."""
        p = subprocess.run([sys.executable, os.path.join(REPO, script)] + list(args), cwd=cwd or self.root,
                           env=self.env(), capture_output=True, text=True, encoding="utf-8")
        return p.returncode, p.stdout + p.stderr

    def shown(self, path):
        """How the scripts print a path in the home folder: ~/... (~\\... in PowerShell)."""
        return "~" + os.sep + os.path.relpath(path, self.home)
