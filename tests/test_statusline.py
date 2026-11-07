"""hatcheck statusline: the line Claude Code shows from what it hands over on stdin, the preview
from a profile's snapshot, and --install and --uninstall on a profile's settings.json. Against a
throwaway home: no real profiles, Keychain or network."""
import json
import os
import subprocess
import unittest

from helpers import NOW, FakeHome, argv_of, iso, write


def limits():
    return {"limits": [
        {"kind": "session", "group": "session", "percent": 40, "severity": "normal", "resets_at": iso(NOW + 2 * 3600)},
        {"kind": "weekly_scoped", "group": "weekly", "percent": 52, "severity": "warning",
         "resets_at": iso(NOW + 4 * 86400), "scope": {"model": {"display_name": "Fable"}}}]}


PAYLOAD = {"hook_event_name": "Status", "session_id": "s", "cwd": "/w",
           "model": {"id": "claude-opus-5-5", "display_name": "Opus 5.5"},
           "rate_limits": {"five_hour": {"used_percentage": 38, "resets_at": NOW + 2 * 3600 + 41 * 60},
                           "seven_day": {"used_percentage": 70, "resets_at": NOW + 3 * 86400}},
           "context_window": {"used_percentage": 23.4}, "cost": {"total_cost_usd": 1.234}}


class StatusLine(unittest.TestCase):
    def setUp(self):
        self.h = FakeHome()
        self.addCleanup(self.h.cleanup)
        self.work = self.h.profile("work", "acct-w", usage=limits(), saved_at=NOW - 120)
        self.home = self.h.profile("home")

    def run_line(self, payload, profile=None, **extra_env):
        """hatcheck statusline as Claude Code runs it: the payload on stdin, CLAUDE_CONFIG_DIR set."""
        env = dict(self.h.env(), **extra_env)
        if profile:
            env["CLAUDE_CONFIG_DIR"] = profile
        p = subprocess.run(argv_of("statusline"), input=json.dumps(payload) if payload is not None else "",
                           env=env, capture_output=True, text=True, encoding="utf-8")
        return p.returncode, p.stdout + p.stderr

    def test_the_line_from_what_claude_code_hands_over(self):
        code, out = self.run_line(PAYLOAD, self.work)
        self.assertEqual(code, 0, out)
        # Claude Code's own 5h and weekly numbers, and the snapshot's per-model limit while it's fresh.
        # The minutes to a reset tick down while the suite runs, so they're matched loosely.
        self.assertRegex(out, r"^work · Opus 5\.5 · 5h 62% ↻2h4\dm · wk 30% ↻(3d0h|2d23h) · Fable 48% ↻(4d0h|3d23h) · ctx 23% · \$1\.23\n$")
        # The default profile, and a stale snapshot adds nothing.
        default = self.h.profile("default", "acct-d", usage=limits(), saved_at=NOW - 7200)
        code, out = self.run_line(PAYLOAD, default)
        self.assertRegex(out, r"^default · Opus 5\.5 · 5h 62% ↻2h4\dm · wk 30% ↻(3d0h|2d23h) · ctx 23% · \$1\.23\n$")
        code, out = self.run_line(PAYLOAD)
        self.assertTrue(out.startswith("default · "), out)
        # Without NO_COLOR the line has colours, since Claude Code renders them: the profile
        # stands out, and a limit that's low is red.
        env = self.h.env()
        del env["NO_COLOR"]
        low = dict(PAYLOAD, rate_limits={"five_hour": {"used_percentage": 95, "resets_at": NOW + 600}})
        p = subprocess.run(argv_of("statusline"), input=json.dumps(low), env=dict(env, CLAUDE_CONFIG_DIR=self.home),
                           capture_output=True, text=True, encoding="utf-8")
        self.assertEqual(p.returncode, 0, p.stderr)
        self.assertTrue(p.stdout.startswith("\x1b[1;36mhome\x1b[0m"), p.stdout)
        self.assertIn("5h \x1b[31m5%\x1b[0m", p.stdout)

    def test_the_preview_from_the_snapshot(self):
        code, out = self.run_line(None, self.work)  # nothing on stdin: the preview
        self.assertEqual(code, 0, out)
        preview = r"^work · 5h 60% ↻(2h0m|1h59m) · Fable 48% ↻(4d0h|3d23h) · cached [23]m ago\n"
        self.assertRegex(out, preview + "$")
        code, out = self.h.run("statusline", "--show", "work")
        self.assertEqual(code, 0, out)
        self.assertRegex(out, preview)
        code, out = self.h.run("statusline", "home")
        self.assertEqual(code, 0, out)
        self.assertTrue(out.startswith("home · usage ?\n"), out)
        code, out = self.h.run("statusline", "--show", "nope")
        self.assertEqual(code, 1, out)
        self.assertIn("There's no ~/.claude-nope", out)
        self.h.codex_home("default")
        code, out = self.h.run("statusline", "--show", "codex")
        self.assertEqual(code, 1, out)
        self.assertIn("~/.codex is Codex's; the status line is Claude Code's", out)

    def test_install_and_uninstall_touch_only_hatchecks_setting(self):
        write(os.path.join(self.work, "settings.json"), json.dumps({"model": "opus"}))
        code, out = self.h.run("statusline", "--install", "work", "home")
        self.assertEqual(code, 0, out)
        self.assertIn("✓ Added to ~/.claude-work/settings.json: statusLine runs ", out)
        self.assertRegex(out, r" statusline\n  Now, in a session there: work · 5h 60% ↻(2h0m|1h59m)")
        self.assertIn("✓ Added to ~/.claude-home/settings.json: statusLine runs ", out)
        with open(os.path.join(self.work, "settings.json"), encoding="utf-8") as fh:
            settings = json.load(fh)
        self.assertEqual(settings["model"], "opus")  # the rest as it was
        self.assertEqual(settings["statusLine"]["type"], "command")
        self.assertTrue(settings["statusLine"]["command"].endswith(" statusline"), settings)
        self.assertIn("hatcheck", settings["statusLine"]["command"])
        # Again: already there, nothing rewritten.
        before = os.stat(os.path.join(self.work, "settings.json")).st_mtime_ns
        code, out = self.h.run("statusline", "--install", "work")
        self.assertEqual(code, 0, out)
        self.assertIn("✓ Already in ~/.claude-work/settings.json", out)
        self.assertEqual(os.stat(os.path.join(self.work, "settings.json")).st_mtime_ns, before)
        # One somebody else set is left as it is, and said so.
        write(os.path.join(self.home, "settings.json"), json.dumps({"statusLine": {"type": "command", "command": "my-line.sh"}}))
        code, out = self.h.run("statusline", "--install", "home")
        self.assertEqual(code, 1, out)
        self.assertIn("⚠ ~/.claude-home/settings.json has a status line of its own (my-line.sh): left as it is.", out)
        code, out = self.h.run("statusline", "--uninstall", "home")
        self.assertEqual(code, 1, out)
        self.assertIn("⚠ ~/.claude-home/settings.json has a status line that isn't hatcheck's (my-line.sh): left as it is.", out)
        with open(os.path.join(self.home, "settings.json"), encoding="utf-8") as fh:
            self.assertEqual(json.load(fh)["statusLine"]["command"], "my-line.sh")
        code, out = self.h.run("statusline", "--uninstall", "--all")
        self.assertEqual(code, 1, out)  # home's wasn't hatcheck's
        self.assertIn("✓ Took hatcheck's status line out of ~/.claude-work/settings.json", out)
        with open(os.path.join(self.work, "settings.json"), encoding="utf-8") as fh:
            self.assertEqual(json.load(fh), {"model": "opus"})
        code, out = self.h.run("statusline", "--uninstall", "work")
        self.assertEqual(code, 0, out)
        self.assertIn("~/.claude-work/settings.json has no status line.", out)
        # Named wrongly, or not at all.
        code, out = self.h.run("statusline", "--install")
        self.assertEqual(code, 2, out)
        self.assertIn("name a profile (work, default, a path), or --all for every one", out)
        code, out = self.h.run("statusline", "--install", "codex")
        self.assertEqual(code, 1, out)
        self.assertIn("~/.codex is Codex's; the status line is Claude Code's", out)
        write(os.path.join(self.work, "settings.json"), "{broken")
        code, out = self.h.run("statusline", "--install", "work")
        self.assertEqual(code, 1, out)
        self.assertIn("~/.claude-work/settings.json: can't read its settings.json (not valid JSON)", out)


if __name__ == "__main__":
    unittest.main()
