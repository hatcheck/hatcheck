"""hatcheck move against a throwaway home."""
import contextlib
import glob
import io
import json
import os
import re
import select
import subprocess
import sys
import time
import types
import unittest

from helpers import NOW, WINDOWS, FakeHome, command, iso, load, rec, write

S1 = "11111111-1111-4111-8111-111111111111"
S2 = "22222222-2222-4222-8222-222222222222"
SCRIPT = "move"
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
            subagent = json.loads(fh.readline())["message"]["content"]  # JSON: Windows paths are escaped in the raw text
        self.assertIn(os.path.join(new, "tool-results", "out.txt"), subagent)
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
        self.assertNotIn("Resume it now", out)  # not a terminal: no question
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
        self.assertIn("%s has a different session with the ID 11111111" % self.h.shown(self.b), out)
        self.assertIn("nothing was copied", out)
        self.assertIn("hatcheck move 11111111 b --fork", out)
        with open(other, "rb") as fh:
            self.assertEqual(fh.read(), before)

    def go_on(self, profile, text):
        """The session carries on in a profile: a prompt and a reply added to its copy there,
        with a new saved tool output and /rewind snapshot."""
        data = self.data if profile == self.default else self.copy_of(profile)
        out_txt = os.path.join(data, "tool-results", text.replace(" ", "-") + ".txt")
        write(out_txt, text + "\n")
        write(os.path.join(profile, "file-history", S1, "abc@" + text.replace(" ", "-")), text + "\n")
        base = {"cwd": self.cwd, "sessionId": S1, "entrypoint": "cli"}
        with open(data + ".jsonl", "a", encoding="utf-8", newline="\n") as fh:
            fh.write(rec(type="user", timestamp=iso(NOW - 60), message={"role": "user", "content": text}, **base) + "\n")
            fh.write(rec(type="assistant", timestamp=iso(NOW - 50), message={"role": "assistant", "content": [
                {"type": "text", "text": "Saved to " + out_txt}]}, **base) + "\n")

    def test_round_trip_brings_the_older_copy_up_to_date(self):
        code, out = self.h.run(SCRIPT, S1[:8], "b")  # copied, so ~/.claude keeps the copy it had
        self.assertEqual(code, 0, out)
        self.go_on(self.b, "more in b")
        code, out = self.h.run(SCRIPT, S1[:8], "default")
        self.assertEqual(code, 0, out)
        self.assertEqual(out.count("(updated an older copy)"), 3)  # transcript, its folder, the snapshots
        self.assertRegex(out, r"plan +happy-test-plan\.md  \(already there\)")
        self.assertIn("%s had an older copy of this session, as it was on " % self.h.shown(self.default), out)
        lines = self.lines(self.transcript)
        self.assertEqual(lines[:len(self.original.split(b"\n")) - 1], self.original.split(b"\n")[:-1])
        reply = json.loads(lines[-2])["message"]["content"][0]["text"]
        self.assertEqual(reply, "Saved to " + os.path.join(self.data, "tool-results", "more-in-b.txt"))
        self.assertTrue(os.path.isfile(os.path.join(self.data, "tool-results", "more-in-b.txt")))
        self.assertTrue(os.path.isfile(os.path.join(self.default, "file-history", S1, "abc@more-in-b")))
        code, out = self.h.run(SCRIPT, S1[:8], "default")
        self.assertEqual(out.count("(already there)"), 4)

    def test_nothing_to_copy_when_the_copy_there_is_newer(self):
        self.h.run(SCRIPT, S1[:8], "b")
        self.go_on(self.b, "more in b")
        with open(self.copy_of(self.b) + ".jsonl", "rb") as fh:
            theirs = fh.read()
        code, out = self.h.run(SCRIPT, S1[:8], "b", "--dry-run", "--no-keep")
        self.assertEqual(code, 0, out)
        self.assertIn('%s already has session 11111111 "Fix the flaky login test", with everything in this copy and '
                      "more" % self.h.shown(self.b), out)
        self.assertIn("Would delete the older copy from %s." % self.h.shown(self.default), out)
        self.assertTrue(os.path.isfile(self.transcript))
        code, out = self.h.run(SCRIPT, S1[:8], "b", "--no-keep")
        self.assertEqual(code, 0, out)
        self.assertIn("Nothing to copy.", out)
        self.assertIn("cl b --resume " + S1, out)
        self.assertFalse(os.path.exists(self.transcript))
        self.assertFalse(os.path.exists(os.path.join(self.default, "file-history", S1)))
        self.assertTrue(os.path.isfile(self.plan))  # plans stay
        with open(self.copy_of(self.b) + ".jsonl", "rb") as fh:
            self.assertEqual(fh.read(), theirs)

    def test_went_on_in_both_then_fork(self):
        self.h.run(SCRIPT, S1[:8], "b")
        self.go_on(self.b, "more in b")
        self.go_on(self.default, "more here")
        with open(self.copy_of(self.b) + ".jsonl", "rb") as fh:
            theirs = fh.read()
        code, out = self.h.run(SCRIPT, S1[:8], "b")
        self.assertEqual(code, 1, out)
        self.assertIn("Session 11111111 went on in both %s and %s after " % (self.h.shown(self.default), self.h.shown(self.b)),
                      out)
        self.assertIn(": 1 new prompt in %s, 1 in %s." % (self.h.shown(self.default), self.h.shown(self.b)), out)
        self.assertIn("hatcheck move 11111111 b --fork", out)
        self.assertIn("  " + self.h.shown(self.copy_of(self.b) + ".jsonl"), out)
        code, out = self.h.run(SCRIPT, S1[:8], "b", "--fork")
        self.assertEqual(code, 0, out)
        new = re.search(r"new session +(\S+)", out).group(1)
        self.assertNotEqual(new, S1)
        self.assertIn("to %s as a new session:" % self.h.shown(self.b), out)
        self.assertIn("The copy of session 11111111 that was already in %s stays as it was." % self.h.shown(self.b), out)
        self.assertIn("cl b --resume " + new, out)
        with open(self.copy_of(self.b) + ".jsonl", "rb") as fh:
            self.assertEqual(fh.read(), theirs)
        fork = os.path.join(self.b, "projects", self.cwd_folder(), new)
        records = [json.loads(line) for line in self.lines(fork + ".jsonl") if line]
        self.assertEqual({r.get("sessionId") for r in records}, {new})
        self.assertEqual(records[-1]["message"]["content"][0]["text"],
                         "Saved to " + os.path.join(fork, "tool-results", "more-here.txt"))
        self.assertEqual(records[2]["message"]["content"][0], json.loads(self.thinking)["message"]["content"][0])
        self.assertTrue(os.path.isfile(os.path.join(fork, "tool-results", "out.txt")))
        self.assertTrue(os.path.isfile(os.path.join(fork, "subagents", "agent-1.jsonl")))
        self.assertTrue(os.path.isfile(os.path.join(self.b, "file-history", new, "abc@more-here")))
        self.assertTrue(os.path.isfile(self.transcript))  # the original stays

    def test_which_session_and_where(self):
        self.h.session(self.b, self.cwd, S2, "In b", minutes_ago=30)
        self.h.session(self.default, self.cwd, S2, "In default", minutes_ago=3)
        code, out = self.h.run(SCRIPT, S2[:8], "a2")  # piped, so no asking: the copy used last, and a note
        self.assertEqual(code, 0, out)
        self.assertIn("Session %s is in %s too. Taking the copy in %s, used last; --from picks another."
                      % (S2[:8], self.h.shown(self.b), self.h.shown(self.default)), out)
        self.assertIn('"In default"', out)
        code, out = self.h.run(SCRIPT, S2[:8], "a2", "--from", "b", "--fork")
        self.assertEqual(code, 0, out)
        self.assertIn('"In b"', out)
        code, out = self.h.run(SCRIPT, S2[:8], "a2", "--json")
        self.assertEqual(code, 0, out)
        report = json.loads(out[out.index("{"):])
        self.assertEqual(report["copies"], [self.default, self.b])
        self.assertEqual(report["source_choice"], "used last")

    def test_asks_which_copy_in_a_terminal(self):
        m, s = load("move"), load("sessions")
        self.h.session(self.b, self.cwd, S2, "In b", minutes_ago=30)
        self.h.session(self.default, self.cwd, S2, "In default", minutes_ago=3)
        where = {p: glob.glob(os.path.join(p, "projects", "*", S2 + ".jsonl")) for p in (self.b, self.default)}
        args = types.SimpleNamespace(json=False, report={})
        answers = iter(["", "2", "zzz", "b", "default"])
        was = m.ask, m.interactive
        m.ask, m.interactive = (lambda prompt: next(answers)), (lambda: True)
        self.addCleanup(setattr, m, "ask", was[0])
        self.addCleanup(setattr, m, "interactive", was[1])
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            picked = [m.choose_source(S2, where, "claude", s, s.shell_path, args) for _ in range(4)]
        self.assertEqual(picked, [self.default, self.b, self.b, self.default])  # Enter, 2, zzz then b, default
        text = out.getvalue()
        self.assertIn("Session %s is in 2 profiles:" % S2[:8], text)
        self.assertIn("used last", text.splitlines()[1])
        self.assertIn("2. ", text.splitlines()[2])
        self.assertEqual(text.count("Answer with a number from 1 to 2, or the profile's name."), 1)
        self.h.session(self.b, self.cwd, S2[:8] + "-9999-4999-8999-999999999999", "Same prefix")
        code, out = self.h.run(SCRIPT, S2[:8], "fresh", "--from", "b")
        self.assertIn("matches 2 sessions", out)
        code, out = self.h.run(SCRIPT, S1[:8], "nosuch")
        self.assertIn("isn't a profile folder", out)

    def test_session_last_on_a_provider(self):
        self.h.session(self.b, self.cwd, S2, "On Kimi", model="k3")
        code, out = self.h.run(SCRIPT, S2[:8], "a2")
        self.assertEqual(code, 0, out)
        self.assertIn("cl a2 --resume " + S2, out)
        self.assertIn("Its last reply came from k3; to carry on there", out)
        self.assertIn("cl -k a2 --resume " + S2, out)

    def test_same_account_and_fresh_target(self):
        code, out = self.h.run(SCRIPT, S1[:8], "a2")
        self.assertIn("same account", out)
        code, out = self.h.run(SCRIPT, S1[:8], "fresh", "--from", "default")
        self.assertEqual(code, 0, out)
        self.assertTrue(os.path.isfile(self.copy_of(self.fresh) + ".jsonl"))
        self.assertIn("cl fresh --resume " + S1, out)


class Standing(unittest.TestCase):
    """How a copy already in the profile compares with the one being copied."""

    def test_older_newer_and_diverged(self):
        m = load(SCRIPT)
        a = b'{"a":1,"b":"x"}\n{"c":2}\n'
        self.assertEqual(m.standing(a, a, True, (0, 0)), "same")
        self.assertEqual(m.standing(a, a + b'{"d":3}\n', True, (0, 0)), "older")
        self.assertEqual(m.standing(a + b'{"d":3}\n', a, True, (0, 0)), "newer")
        self.assertEqual(m.standing(a + b'{"d":3}\n', a + b'{"e":4}\n', True, (0, 0)), "diverged")
        self.assertEqual(m.standing(b'{"a": 1, "b": "x"}\n', a, True, (0, 0)), "older")  # the same record, spaced out
        self.assertEqual(m.standing(b"output", b"output and more", False, (9, 1)), "older")  # added to
        self.assertEqual(m.standing(b"# Plan\n", b"# Better plan\n", False, (1, 2)), "older")  # written over: the newer
        self.assertEqual(m.standing(b"# Better plan\n", b"# Plan\n", False, (2, 1)), "newer")
        self.assertEqual(m.overall(["same", "new"]), "older")
        self.assertEqual(m.overall(["older", "same", "newer"]), "diverged")
        self.assertEqual(m.overall(["same"]), "same")


class ResumeNow(unittest.TestCase):
    """The y in "Resume it now in ...? [y/N]": Claude Code in the new profile, from the session's folder."""

    def setUp(self):
        self.h = FakeHome()
        self.addCleanup(self.h.cleanup)
        self.default, self.b = self.h.profile("default", "acct-1"), self.h.profile("b", "acct-2")
        self.cwd = self.h.folder("code", "api")
        self.h.session(self.default, self.cwd, S1, "Fix the flaky login test", 180)
        # A fake claude that prints the profile it got, where it runs and its arguments.
        self.bin = os.path.join(self.h.root, "fakebin")
        if WINDOWS:
            write(os.path.join(self.bin, "claude.cmd"),
                  "@echo off\r\nif defined CLAUDE_CONFIG_DIR (echo CLAUDE_CONFIG_DIR=%CLAUDE_CONFIG_DIR% cwd=%CD% args=%*)"
                  " else (echo CLAUDE_CONFIG_DIR=unset cwd=%CD% args=%*)\r\n")
        else:
            write(os.path.join(self.bin, "claude"), '#!/bin/sh\necho "CLAUDE_CONFIG_DIR=${CLAUDE_CONFIG_DIR-unset} cwd=$(pwd) args=$*"\n')
            os.chmod(os.path.join(self.bin, "claude"), 0o755)
        self.env = dict(self.h.env(), PATH=self.bin + os.pathsep + (os.environ["PATH"] if WINDOWS else "/bin"))

    def expect(self, out, config_dir):
        line = "CLAUDE_CONFIG_DIR=%s cwd=%s args=--resume %s" % (config_dir, self.cwd, S1)
        self.assertIn(line.lower() if WINDOWS else line, out.lower() if WINDOWS else out)

    def test_runs_claude_in_the_new_profile(self):
        snippet = ("import sys; sys.path.insert(0, %r); from helpers import load; "
                   "load('move').resume_now(*sys.argv[1:])" % os.path.dirname(os.path.abspath(__file__)))
        for target, config_dir in ((self.b, self.b), (self.default, "unset")):
            env = dict(self.env, CLAUDE_CONFIG_DIR=os.path.join(self.h.home, ".claude-other"))
            p = subprocess.run([sys.executable, "-c", snippet, target, S1, self.cwd], env=env, cwd=self.h.root,
                               capture_output=True, text=True, encoding="utf-8")
            self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
            self.expect(p.stdout, config_dir)  # ~/.claude: CLAUDE_CONFIG_DIR unset, as plain claude has it

    @unittest.skipIf(WINDOWS, "needs a pseudo-terminal")
    def test_asks_in_a_terminal(self):
        import pty

        def run(answer):
            master, slave = pty.openpty()
            p = subprocess.Popen([sys.executable] + command(SCRIPT) + [S1[:8], "b"], stdin=slave,
                                 stdout=slave, stderr=slave, env=self.env, cwd=self.h.root)
            os.close(slave)
            out, sent, deadline = b"", False, time.time() + 30
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
                if not sent and b"[y/N]" in out:
                    os.write(master, answer + b"\n")
                    sent = True
            p.wait(timeout=10)
            os.close(master)
            return p.returncode, out.decode("utf-8", "replace").replace("\r\n", "\n")

        code, out = run(b"")
        self.assertEqual(code, 0, out)
        self.assertIn("Resume it now in %s? [y/N]" % self.h.shown(self.b), out)
        self.assertNotIn("args=", out)  # Enter means no
        code, out = run(b"y")
        self.assertEqual(code, 0, out)
        self.expect(out, self.b)


if __name__ == "__main__":
    unittest.main()
