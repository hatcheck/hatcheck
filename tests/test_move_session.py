"""claude-move-session.py against a throwaway home."""
import json
import os
import re
import unittest

from helpers import NOW, FakeHome, iso, rec, write

S1 = "11111111-1111-4111-8111-111111111111"
S2 = "22222222-2222-4222-8222-222222222222"
SCRIPT = "claude-move-session.py"
OLD = NOW - 3 * 3600


class MoveSession(unittest.TestCase):
    def setUp(self):
        self.h = h = FakeHome()
        self.addCleanup(h.cleanup)
        self.default = h.profile("default", "acct-1")
        self.a2 = h.profile("a2", "acct-1")
        self.b = h.profile("b", "acct-2")
        self.fresh = h.profile("fresh", "acct-3", projects=False)
        self.cwd = h.folder("code", "api")
        # S1 has everything a session can: saved tool output, a subagent, /rewind snapshots,
        # a plan, and paths to them in the conversation, one of them inside signed thinking.
        self.data = os.path.join(self.default, "projects", self.cwd_folder(), S1)
        self.plan = os.path.join(self.default, "plans", "happy-test-plan.md")
        out_txt = os.path.join(self.data, "tool-results", "out.txt")
        base = {"cwd": self.cwd, "sessionId": S1, "entrypoint": "cli"}
        self.thinking = rec(type="assistant", timestamp=iso(OLD - 30), message={"role": "assistant", "content": [
            {"type": "thinking", "thinking": "I should read " + out_txt, "signature": "SIG=="},
            {"type": "text", "text": "Reading " + out_txt}]}, **base)
        extra = [
            self.thinking,
            rec(type="user", timestamp=iso(OLD - 20), message={"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "t1", "content": "Output saved to " + out_txt}]}, **base),
            rec(type="user", timestamp=iso(OLD - 10), isCompactSummary=True, message={
                "role": "user", "content": "read the full transcript at " + self.data + ".jsonl — résumé ✓ 中文"}, **base),
            rec(type="system", timestamp=iso(OLD - 5), slug="happy-test-plan", content="Plan saved to " + self.plan, **base),
        ]
        self.transcript = h.session(self.default, self.cwd, S1, "Fix the flaky login test", 180, extra=extra)
        write(out_txt, "big output\n" * 100, mtime=OLD)
        write(os.path.join(self.data, "subagents", "agent-1.jsonl"),
              rec(type="user", message={"role": "user", "content": "see " + out_txt}) + "\n", mtime=OLD)
        write(os.path.join(self.default, "file-history", S1, "abc@v1"), "old contents\n", mtime=OLD)
        write(self.plan, "# Plan\n", mtime=OLD)
        with open(self.transcript, "rb") as fh:
            self.original = fh.read()

    def cwd_folder(self):
        return re.sub(r"[^A-Za-z0-9]", "-", self.cwd)  # how Claude Code names the projects/ folder

    def copy_of(self, profile):
        return os.path.join(profile, "projects", self.cwd_folder(), S1)

    def lines(self, path):
        with open(path, "rb") as fh:
            return fh.read().split(b"\n")

    def test_dry_run_changes_nothing(self):
        code, out = self.h.run(SCRIPT, S1[:8], "b", "--dry-run")
        self.assertEqual(code, 0, out)
        self.assertIn("Would copy session 11111111", out)
        self.assertFalse(os.path.exists(self.copy_of(self.b) + ".jsonl"))

    def test_copy(self):
        code, out = self.h.run(SCRIPT, S1[:8], "b")
        self.assertEqual(code, 0, out)
        new = self.copy_of(self.b)
        src, dst = self.lines(self.transcript), self.lines(new + ".jsonl")
        self.assertEqual(len(src), len(dst))
        changed = [i for i, (x, y) in enumerate(zip(src, dst)) if x != y]
        self.assertEqual(changed, [2, 3, 4, 5])  # only the lines holding the session's own paths
        thinking = json.loads(dst[2])["message"]["content"]
        self.assertEqual(thinking[0], json.loads(self.thinking)["message"]["content"][0])  # signed: untouched
        self.assertIn(os.path.join(new, "tool-results", "out.txt"), thinking[1]["text"])
        self.assertIn(os.path.join(new, "tool-results", "out.txt"), json.loads(dst[3])["message"]["content"][0]["content"])
        self.assertIn(new + ".jsonl — résumé ✓ 中文", json.loads(dst[4])["message"]["content"])
        self.assertIn(os.path.join(self.b, "plans", "happy-test-plan.md"), json.loads(dst[5])["content"])
        with open(os.path.join(new, "subagents", "agent-1.jsonl"), encoding="utf-8") as fh:
            self.assertIn(os.path.join(new, "tool-results", "out.txt"), fh.read())
        self.assertTrue(os.path.isfile(os.path.join(self.b, "file-history", S1, "abc@v1")))
        self.assertTrue(os.path.isfile(os.path.join(self.b, "plans", "happy-test-plan.md")))
        self.assertAlmostEqual(os.path.getmtime(new + ".jsonl"), os.path.getmtime(self.transcript), places=2)
        with open(self.transcript, "rb") as fh:
            self.assertEqual(fh.read(), self.original)  # the original is untouched
        for text in ('Copied session 11111111 "Fix the flaky login test" from %s to %s:'
                     % (self.h.shown(self.default), self.h.shown(self.b)),
                     "now point to the copies", "Kept the original", "cd " + self.h.shown(self.cwd),
                     "cl b --resume " + S1):
            self.assertIn(text, out)
        code, out = self.h.run(SCRIPT, S1, "b")
        self.assertEqual(code, 0, out)
        self.assertEqual(out.count("(already there)"), 4)

    def test_move_and_back_restores_the_transcript(self):
        code, out = self.h.run(SCRIPT, S1, "b", "--no-keep")
        self.assertEqual(code, 0, out)
        self.assertIn("Moved session 11111111", out)
        self.assertFalse(os.path.exists(self.transcript))
        self.assertFalse(os.path.exists(self.data))
        self.assertTrue(os.path.isfile(self.plan))  # plans stay: sessions can share one
        code, out = self.h.run(SCRIPT, S1, "default", "--no-keep")
        self.assertEqual(code, 0, out)
        self.assertIn("claude --resume " + S1, out)
        with open(self.transcript, "rb") as fh:
            self.assertEqual(fh.read(), self.original)

    def test_refuses_an_open_session(self):
        self.h.register_open(self.default, S1)
        code, out = self.h.run(SCRIPT, S1[:8], "b")
        self.assertEqual(code, 1)
        self.assertIn("is open in pid %d (idle)" % os.getpid(), out)

    def test_refuses_to_overwrite_a_different_copy(self):
        other = self.h.session(self.b, self.cwd, S1, "Something else with the same ID")
        with open(other, "rb") as fh:
            before = fh.read()
        code, out = self.h.run(SCRIPT, S1[:8], "b")
        self.assertEqual(code, 1)
        self.assertIn("already has a different copy", out)
        with open(other, "rb") as fh:
            self.assertEqual(fh.read(), before)

    def test_which_session_and_where(self):
        self.h.session(self.b, self.cwd, S2, "In b")
        self.h.session(self.default, self.cwd, S2, "In default")
        code, out = self.h.run(SCRIPT, S2[:8], "a2")
        self.assertEqual(code, 1)
        self.assertIn("pick one with --from", out)
        code, out = self.h.run(SCRIPT, S2[:8], "a2", "--from", "b")
        self.assertEqual(code, 0, out)
        self.assertIn('"In b"', out)
        self.h.session(self.b, self.cwd, S2[:8] + "-9999-4999-8999-999999999999", "Same prefix")
        code, out = self.h.run(SCRIPT, S2[:8], "fresh", "--from", "b")
        self.assertIn("matches 2 sessions", out)
        code, out = self.h.run(SCRIPT, S1[:8], "nosuch")
        self.assertIn("isn't a profile folder", out)

    def test_same_account_and_fresh_target(self):
        code, out = self.h.run(SCRIPT, S1[:8], "a2")
        self.assertIn("same account", out)
        code, out = self.h.run(SCRIPT, S1[:8], "fresh", "--from", "default")
        self.assertEqual(code, 0, out)
        self.assertTrue(os.path.isfile(self.copy_of(self.fresh) + ".jsonl"))
        self.assertIn("cl fresh --resume " + S1, out)


if __name__ == "__main__":
    unittest.main()
