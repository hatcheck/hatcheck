"""claude-recent-sessions.py against a throwaway home."""
import os
import re
import unittest

from helpers import NOW, WINDOWS, FakeHome, iso, rec

A = "aaaaaaaa-0000-4000-8000-000000000001"
B = "bbbbbbbb-0000-4000-8000-000000000002"
C = "cccccccc-0000-4000-8000-000000000003"
D = "dddddddd-0000-4000-8000-000000000004"
E = "eeeeeeee-0000-4000-8000-000000000005"
SCRIPT = "claude-recent-sessions.py"


def limits(session_used, weekly_used):
    return {"limits": [
        {"kind": "session", "group": "session", "percent": session_used, "severity": "normal",
         "resets_at": iso(NOW + 2 * 3600)},
        {"kind": "weekly_all", "group": "weekly", "percent": weekly_used, "severity": "normal",
         "resets_at": iso(NOW + 3 * 86400)}]}


class RecentSessions(unittest.TestCase):
    def setUp(self):
        self.h = FakeHome()
        self.addCleanup(self.h.cleanup)
        self.default = self.h.profile("default")
        self.work = self.h.profile("work")
        self.api = self.h.folder("code", "api")
        self.blog = self.h.folder("code", "blog")
        self.h.session(self.work, self.api, A, "Fix the flaky login test", 4)
        self.h.session(self.default, self.api, B, "Add rate limiting", 180)
        self.h.session(self.work, self.blog, C, "Draft a post", 600)
        self.h.session(self.default, self.blog, D, "Two days old", 48 * 60)
        self.h.session(self.default, self.blog, E, "From the Desktop app", 30, entrypoint="claude-desktop")

    def test_lists_recent_sessions_by_folder(self):
        code, out = self.h.run(SCRIPT, "--no-usage")
        self.assertEqual(code, 0, out)
        api, blog = "cd " + self.h.shown(self.api), "cd " + self.h.shown(self.blog)
        self.assertIn(api, out)
        self.assertIn(blog, out)
        self.assertLess(out.index(api), out.index(blog))  # folders A-Z
        self.assertIn("cl work --resume " + A, out)
        self.assertIn("claude --resume " + B, out)
        self.assertLess(out.index(A), out.index(B))  # newest first
        self.assertNotIn(D, out)  # older than 24 hours
        self.assertNotIn(E, out)  # Desktop sessions are hidden by default
        self.assertIn("Hidden: 1 Desktop (show with --all)", out)

    def test_hours_and_all(self):
        code, out = self.h.run(SCRIPT, "3d", "--all", "--no-usage")
        self.assertEqual(code, 0, out)
        self.assertIn(D, out)
        self.assertIn(E, out)
        self.assertIn("via claude-desktop", out)

    def test_newest_n_per_folder(self):
        code, out = self.h.run(SCRIPT, "-n", "1", "--no-usage")
        self.assertIn(A, out)
        self.assertNotIn(B, out)
        self.assertIn("+1  older session", out)

    def test_only_the_current_folder(self):
        code, out = self.h.run(SCRIPT, "--no-usage", cwd=self.api)
        self.assertEqual(code, 0, out)
        self.assertIn("in this folder", out)
        self.assertIn(A, out)
        self.assertNotIn(C, out)
        self.assertIn("1 in other folders (show with --all-folders)", out)
        code, out = self.h.run(SCRIPT, "--no-usage", "--all-folders", cwd=self.api)
        self.assertIn(C, out)
        self.assertNotIn("in this folder", out)

    def test_full_commands(self):
        code, out = self.h.run(SCRIPT, "--no-usage", "--full")
        if WINDOWS:
            expected = '$env:CLAUDE_CONFIG_DIR="$HOME\\.claude-work"; claude --resume %s; $env:CLAUDE_CONFIG_DIR=$null' % A
        else:
            expected = "CLAUDE_CONFIG_DIR=~/.claude-work claude --resume " + A
        self.assertIn(expected, out)

    def test_open_session_is_marked(self):
        self.h.register_open(self.work, A)
        code, out = self.h.run(SCRIPT, "--no-usage")
        self.assertIn("● open in pid %d (idle)" % os.getpid(), out)

    def test_open_session_with_another_start_time_is_not(self):
        self.h.register_open(self.work, A)
        path = os.path.join(self.work, "sessions", "%d.json" % os.getpid())
        with open(path, encoding="utf-8") as fh:
            text = fh.read()
        with open(path, "w", encoding="utf-8") as fh:  # a reused PID: same number, other process
            fh.write(re.sub(r'"(procStart|procStartFt)": "[^"]*"', r'"\1": "1"', text))
        code, out = self.h.run(SCRIPT, "--no-usage")
        self.assertNotIn("● open", out)


class Providers(unittest.TestCase):
    """Sessions whose newest reply came from Z.ai or Kimi resume there."""

    def setUp(self):
        self.h = FakeHome()
        self.addCleanup(self.h.cleanup)
        self.default, self.work = self.h.profile("default"), self.h.profile("work")
        api = self.h.folder("code", "api")
        self.h.session(self.work, api, A, "On Kimi", 1, model="k3")
        # Claude Code's own <synthetic> replies don't count.
        synthetic = rec(type="assistant", timestamp=iso(NOW - 90), cwd=api, sessionId=B, entrypoint="cli",
                        gitBranch="main", message={"model": "<synthetic>", "role": "assistant",
                                                   "content": [{"type": "text", "text": "x"}]})
        self.h.session(self.default, api, B, "On GLM", 2, model="glm-5.3", extra=[synthetic])
        self.h.session(self.work, api, C, "On Claude", 3, model="claude-opus-5-5")

    def test_commands_and_model(self):
        code, out = self.h.run(SCRIPT, "--no-usage")
        self.assertEqual(code, 0, out)
        self.assertIn("cl -k work --resume " + A, out)
        self.assertIn("claude-z --resume " + B, out)
        self.assertIn("cl work --resume " + C, out)
        self.assertIn("On Kimi · main · on k3", out)
        self.assertIn("On GLM · main · on glm-5.3", out)
        self.assertNotIn("on claude", out)

    def test_full_commands(self):
        code, out = self.h.run(SCRIPT, "--no-usage", "--full")
        if WINDOWS:
            expected = '$env:CLAUDE_CONFIG_DIR="$HOME\\.claude-work"; claude-k --resume %s; $env:CLAUDE_CONFIG_DIR=$null' % A
        else:
            expected = "CLAUDE_CONFIG_DIR=~/.claude-work claude-k --resume " + A
        self.assertIn(expected, out)
        self.assertIn("claude-z --resume " + B, out)


class UsageComments(unittest.TestCase):
    def setUp(self):
        self.h = FakeHome()
        self.addCleanup(self.h.cleanup)
        self.api = self.h.folder("code", "api")
        # work and work2 are signed in to one account; personal to another with the same numbers.
        for name, account in (("work", "acct-w"), ("work2", "acct-w"), ("personal", "acct-p")):
            profile = self.h.profile(name, account, usage=limits(40, 10), saved_at=NOW - 60)
            setattr(self, name, profile)

    def lines(self, out):
        return [l for l in out.splitlines() if re.match(r" {8}\S", l)]

    def test_bars_numbers_and_dedup_by_account(self):
        self.h.session(self.work, self.api, A, "One", 1)
        self.h.session(self.work2, self.api, B, "Two", 2)
        self.h.session(self.personal, self.api, C, "Three", 3)
        code, out = self.h.run(SCRIPT, "--cached")
        self.assertEqual(code, 0, out)
        first, second, third = self.lines(out)
        self.assertRegex(first, "cl work --resume " + A + r" +# 5h 60% ███░░ ↻██░░░ 1h5\dm  week 90% ██████▎ ↻███░░░░ 2d23h$")
        self.assertTrue(second.endswith("cl work2 --resume " + B), second)  # same account: left out
        self.assertIn("# 5h 60%", third)  # another account, though the numbers match
        self.assertIn("Usage from Claude Code's last snapshot (--cached)", out)

    def test_no_usage(self):
        self.h.session(self.work, self.api, A, "One", 1)
        code, out = self.h.run(SCRIPT, "--no-usage")
        self.assertNotIn("#", "".join(self.lines(out)))


if __name__ == "__main__":
    unittest.main()
