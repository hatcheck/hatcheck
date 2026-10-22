"""hatcheck move against a throwaway home."""
import contextlib
import glob
import io
import json
import os
import re
import subprocess
import sys
import types
import unittest

from helpers import INTERRUPT, NOW, WINDOWS, FakeHome, argv_of, in_terminal, iso, rec, write

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
        self.assertIn("Would move session 11111111", out)
        self.assertIn("Would delete the original from %s; the plan file stays" % self.h.shown(self.default), out)
        self.assertFalse(os.path.exists(self.copy_of(self.b) + ".jsonl"))
        with open(self.transcript, "rb") as fh:
            self.assertEqual(fh.read(), self.original)
        self.assertTrue(os.path.isfile(os.path.join(self.default, "file-history", S1, "abc@v1")))

    def test_keep_copies(self):
        code, out = self.h.run(SCRIPT, S1[:8], "b", "--keep")
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
                     "now point to the copies", "The original stays in %s." % self.h.shown(self.default),
                     "cd " + self.h.shown(self.cwd), "cl b --resume " + S1):
            self.assertIn(text, out)
        self.assertNotIn("Resume it now", out)  # not a terminal: no question
        code, out = self.h.run(SCRIPT, S1, "b")  # all there already: a move only deletes the original
        self.assertEqual(code, 0, out)
        self.assertEqual(out.count("(already there)"), 4)
        self.assertIn("Deleted the original from %s; the plan file stays" % self.h.shown(self.default), out)
        self.assertFalse(os.path.exists(self.transcript))
        self.assertTrue(os.path.isfile(new + ".jsonl"))

    def test_move_and_back_restores_the_transcript(self):
        code, out = self.h.run(SCRIPT, S1, "b")
        self.assertEqual(code, 0, out)
        self.assertIn("Moved session 11111111", out)
        self.assertIn("Deleted the original from %s; the plan file stays" % self.h.shown(self.default), out)
        self.assertFalse(os.path.exists(self.transcript))
        self.assertFalse(os.path.exists(self.data))
        self.assertFalse(os.path.exists(os.path.join(self.default, "file-history", S1)))
        self.assertTrue(os.path.isfile(self.plan))  # plans stay: sessions can share one
        code, out = self.h.run(SCRIPT, S1, "default", "--no-keep")  # what a move once needed: still taken
        self.assertEqual(code, 0, out)
        self.assertIn("claude --resume " + S1, out)
        self.assertFalse(os.path.exists(self.copy_of(self.b) + ".jsonl"))
        with open(self.transcript, "rb") as fh:
            self.assertEqual(fh.read(), self.original)
        code, out = self.h.run(SCRIPT, "--help")
        self.assertIn("--keep", out)
        self.assertNotIn("--no-keep", out)
        self.assertNotIn("--fork", out)  # hatcheck fork now
        code, out = self.h.run("fork", "--help")
        self.assertEqual(code, 0, out)
        self.assertIn("--dry-run", out)
        self.assertNotIn("--keep", out)  # a fork always keeps the original

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
        self.assertIn("nothing was moved", out)
        self.assertIn("hatcheck fork 11111111 b", out)
        with open(other, "rb") as fh:
            self.assertEqual(fh.read(), before)
        with open(self.transcript, "rb") as fh:
            self.assertEqual(fh.read(), self.original)

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
        code, out = self.h.run(SCRIPT, S1[:8], "b", "--keep")  # copied, so ~/.claude keeps the copy it had
        self.assertEqual(code, 0, out)
        self.go_on(self.b, "more in b")
        code, out = self.h.run(SCRIPT, S1[:8], "default")
        self.assertEqual(code, 0, out)
        self.assertEqual(out.count("(updated an older copy)"), 3)  # transcript, its folder, the snapshots
        self.assertRegex(out, r"plan +happy-test-plan\.md  \(already there\)")
        self.assertIn("%s had an older copy of this session, as it was on " % self.h.shown(self.default), out)
        self.assertIn("Deleted the original from %s" % self.h.shown(self.b), out)  # once the copy here matched it
        self.assertFalse(os.path.exists(self.copy_of(self.b) + ".jsonl"))
        lines = self.lines(self.transcript)
        self.assertEqual(lines[:len(self.original.split(b"\n")) - 1], self.original.split(b"\n")[:-1])
        reply = json.loads(lines[-2])["message"]["content"][0]["text"]
        self.assertEqual(reply, "Saved to " + os.path.join(self.data, "tool-results", "more-in-b.txt"))
        self.assertTrue(os.path.isfile(os.path.join(self.data, "tool-results", "more-in-b.txt")))
        self.assertTrue(os.path.isfile(os.path.join(self.default, "file-history", S1, "abc@more-in-b")))

    def test_stops_when_the_copy_there_is_newer(self):
        self.h.run(SCRIPT, S1[:8], "b", "--keep")
        self.go_on(self.b, "more in b")
        with open(self.copy_of(self.b) + ".jsonl", "rb") as fh:
            theirs = fh.read()
        for args in (["--dry-run"], [], ["--no-keep"]):
            code, out = self.h.run(SCRIPT, S1[:8], "b", *args)
            self.assertEqual(code, 1, out)
            self.assertIn('%s already has session 11111111 "Fix the flaky login test", with everything in this copy and '
                          "more: the session went on there. This copy is older, so nothing was moved.\n"
                          % self.h.shown(self.b), out)
            self.assertIn("Resume it there with:\n  cd %s\n  cl b --resume %s\n" % (self.h.shown(self.cwd), S1), out)
            self.assertIn("To drop the older copy in %s, delete:\n  %s\n"
                          % (self.h.shown(self.default), self.h.shown(self.transcript)), out)
        code, out = self.h.run(SCRIPT, S1[:8], "b", "--keep")
        self.assertEqual(code, 1, out)
        self.assertIn("This copy is older, so nothing was copied.", out)
        p = subprocess.run(argv_of(SCRIPT) + [S1[:8], "b", "--json"], cwd=self.h.root, env=self.h.env(),
                           capture_output=True, text=True, encoding="utf-8")
        self.assertEqual((p.returncode, p.stdout), (1, ""))
        self.assertIn("nothing was moved", p.stderr)
        with open(self.transcript, "rb") as fh:
            self.assertEqual(fh.read(), self.original)  # nothing deleted here, nothing replaced there
        self.assertTrue(os.path.isfile(os.path.join(self.data, "tool-results", "out.txt")))
        self.assertTrue(os.path.isfile(os.path.join(self.default, "file-history", S1, "abc@v1")))
        with open(self.copy_of(self.b) + ".jsonl", "rb") as fh:
            self.assertEqual(fh.read(), theirs)

    def test_went_on_in_both_then_fork(self):
        self.h.run(SCRIPT, S1[:8], "b", "--keep")
        self.go_on(self.b, "more in b")
        self.go_on(self.default, "more here")
        with open(self.copy_of(self.b) + ".jsonl", "rb") as fh:
            theirs = fh.read()
        code, out = self.h.run(SCRIPT, S1[:8], "b")
        self.assertEqual(code, 1, out)
        self.assertIn("Session 11111111 went on in both %s and %s after " % (self.h.shown(self.default), self.h.shown(self.b)),
                      out)
        self.assertIn(": 1 new prompt in %s, 1 in %s." % (self.h.shown(self.default), self.h.shown(self.b)), out)
        self.assertIn("so nothing was moved", out)
        self.assertIn("hatcheck fork 11111111 b", out)
        self.assertIn("  " + self.h.shown(self.copy_of(self.b) + ".jsonl"), out)
        code, out = self.h.run(SCRIPT, S1[:8], "b", "--fork", "--dry-run")  # as before: hatcheck fork
        self.assertEqual(code, 0, out)
        self.assertIn("Would fork session 11111111", out)
        self.assertIn("The original stays in %s." % self.h.shown(self.default), out)
        code, out = self.h.run("fork", S1[:8], "b")
        self.assertEqual(code, 0, out)
        new = re.search(r"new session +(\S+)", out).group(1)
        self.assertNotEqual(new, S1)
        self.assertIn('Forked session 11111111 "Fix the flaky login test" from %s to %s as a new session:'
                      % (self.h.shown(self.default), self.h.shown(self.b)), out)
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
        self.assertIn("The original stays in %s." % self.h.shown(self.default), out)
        self.assertTrue(os.path.isfile(self.transcript))  # a fork keeps the original
        self.assertTrue(os.path.isfile(os.path.join(self.default, "file-history", S1, "abc@more-here")))

    def test_which_session_and_where(self):
        self.h.session(self.b, self.cwd, S2, "In b", minutes_ago=30)
        self.h.session(self.default, self.cwd, S2, "In default", minutes_ago=3)
        code, out = self.h.run(SCRIPT, S2[:8], "a2", "--keep")  # piped, so no asking: the copy used last, and a note
        self.assertEqual(code, 0, out)
        self.assertIn("Session %s is in %s too. Taking the copy in %s, used last; --from picks another."
                      % (S2[:8], self.h.shown(self.b), self.h.shown(self.default)), out)
        self.assertIn('"In default"', out)
        code, out = self.h.run("fork", S2[:8], "a2", "--from", "b")
        self.assertEqual(code, 0, out)
        self.assertIn('"In b"', out)
        code, out = self.h.run(SCRIPT, S2[:8], "a2", "--json")
        self.assertEqual(code, 0, out)
        report = json.loads(out[out.index("{"):])
        self.assertEqual(report["copies"], [self.default, self.b])
        self.assertEqual(report["source_choice"], "used last")

    @unittest.skipIf(WINDOWS, "needs a pseudo-terminal")
    def test_picks_a_copy_in_a_terminal(self):
        self.h.session(self.b, self.cwd, S2, "In b", minutes_ago=30)
        self.h.session(self.default, self.cwd, S2, "In default", minutes_ago=3)
        ask = b"Which one? [1] "

        def run(*replies, args=("--dry-run",)):
            return in_terminal(argv_of(SCRIPT) + [S2[:8], "a2"] + list(args), replies, self.h.env(), self.h.root)
        code, out = run((ask, b"zzz\n"), (ask, b"b\n"))
        self.assertEqual(code, 0, out)
        lines = out.splitlines()
        self.assertEqual(lines[0], "Session %s is in 2 profiles:" % S2[:8])
        self.assertRegex(lines[1], r"^  1\. %s +[A-Z][a-z]{2} \d\d \d\d:\d\d +1 prompt +used last$"
                         % re.escape(self.h.shown(self.default)))
        self.assertTrue(lines[2].startswith("  2. %s " % self.h.shown(self.b)), lines[2])
        self.assertEqual(out.count("Answer with a number from 1 to 2, or the profile's name."), 1)
        self.assertIn('Would move session %s "In b"' % S2[:8], out)
        code, out = run((ask, b"\n"))  # Enter: the one used last
        self.assertIn('Would move session %s "In default"' % S2[:8], out)
        code, out = run((ask, b"2\n"))
        self.assertIn('Would move session %s "In b"' % S2[:8], out)
        for keys in (b"\x04", INTERRUPT):  # the end of input, and Ctrl+C
            code, out = run((ask, keys))
            self.assertEqual(code, 1, out)
            self.assertTrue(out.rstrip().endswith("Nothing changed."), out)
        code, out = run((ask, b"default\n"), (b"[y/N] ", b"\n"), args=())
        self.assertEqual(code, 0, out)
        self.assertIn('Moved session %s "In default"' % S2[:8], out)
        self.assertIn("Resume it now in %s? [y/N]" % self.h.shown(self.a2), out)

    def test_more_than_one_match_and_no_such_profile(self):
        self.h.session(self.b, self.cwd, S2, "In b", minutes_ago=30)
        self.h.session(self.b, self.cwd, S2[:8] + "-9999-4999-8999-999999999999", "Same prefix")
        code, out = self.h.run(SCRIPT, S2[:8], "fresh", "--from", "b")
        self.assertEqual(code, 1, out)
        self.assertIn("%s matches 2 sessions; give more of the ID:\n  %s  %s\n" % (S2[:8], S2, self.h.shown(self.b)), out)
        code, out = self.h.run(SCRIPT, S1[:8], "nosuch")
        self.assertEqual(code, 1, out)
        self.assertIn("isn't a profile folder. Sign in there first, e.g. with: cl nosuch", out)

    def test_session_last_on_a_provider(self):
        self.h.session(self.b, self.cwd, S2, "On Kimi", model="k3")
        code, out = self.h.run(SCRIPT, S2[:8], "a2")
        self.assertEqual(code, 0, out)
        self.assertIn("cl a2 --resume " + S2, out)
        self.assertIn("Its last reply came from k3; to carry on there", out)
        self.assertIn("cl -k a2 --resume " + S2, out)

    def test_same_account_and_fresh_target(self):
        code, out = self.h.run(SCRIPT, S1[:8], "a2", "--keep")
        self.assertIn("same account", out)
        code, out = self.h.run(SCRIPT, S1[:8], "fresh", "--from", "default")
        self.assertEqual(code, 0, out)
        self.assertTrue(os.path.isfile(self.copy_of(self.fresh) + ".jsonl"))
        self.assertIn("cl fresh --resume " + S1, out)


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

    @unittest.skipIf(WINDOWS, "needs a pseudo-terminal")
    def test_asks_in_a_terminal(self):
        def run(args, keys, env=None):
            return in_terminal(argv_of(SCRIPT) + args, [(b"[y/N] ", keys)], env or self.env, self.h.root)
        code, out = run([S1[:8], "b", "--keep"], b"\n")
        self.assertEqual(code, 0, out)
        self.assertIn("Resume it now in %s? [y/N]" % self.h.shown(self.b), out)
        self.assertNotIn("args=", out)  # Enter means no
        code, out = run([S1[:8], "b", "--keep"], b"y\n")
        self.assertEqual(code, 0, out)
        self.expect(out, self.b)
        code, out = run([S1[:8], "b", "--keep"], INTERRUPT)  # Ctrl+C: no, as Enter is
        self.assertEqual(code, 0, out)
        self.assertNotIn("args=", out)
        # Back to ~/.claude, where plain claude looks: CLAUDE_CONFIG_DIR unset, even when it was set.
        code, out = run([S1[:8], "default", "--from", "b"], b"y\n", dict(self.env, CLAUDE_CONFIG_DIR=self.default))
        self.assertEqual(code, 0, out)
        self.expect(out, "unset")


if __name__ == "__main__":
    unittest.main()


class MoveWithoutArguments(unittest.TestCase):
    """hat move with nothing: the newest session started from this folder, to the folder's Claude
    Code profile, after asking; and claude as the name of ~/.claude."""

    def setUp(self):
        self.h = h = FakeHome()
        self.addCleanup(h.cleanup)
        self.default = h.profile("default", "acct-1")
        self.work = h.profile("work", "acct-2")
        self.api = h.folder("code", "api")
        h.session(self.work, self.api, S1, "Newest here", 1)
        h.session(self.work, self.api, S2, "Older here", 9)

    def test_dry_run_says_what_it_would_do(self):
        h = self.h
        code, out = h.run(SCRIPT, "--dry-run", cwd=self.api)
        self.assertEqual(code, 0, out)
        self.assertIn('Would move session 11111111 "Newest here" from %s to %s:' % (h.shown(self.work), h.shown(self.default)), out)
        self.assertNotIn("[y/N]", out)
        # Bound to a profile, the folder's sessions go there.
        h.run("bind", "work", cwd=self.api)
        h.session(self.default, self.api, "33333333-3333-4333-8333-333333333333", "In the default profile", 0)
        code, out = h.run(SCRIPT, "--dry-run", cwd=self.api)
        self.assertIn('Would move session 33333333 "In the default profile" from %s to %s:' % (h.shown(self.default), h.shown(self.work)), out)
        # claude names ~/.claude, as codex and agy name theirs.
        code, out = h.run(SCRIPT, S1[:8], "claude", "--dry-run", cwd=self.api)
        self.assertIn("from %s to %s:" % (h.shown(self.work), h.shown(self.default)), out)

    def test_a_profile_alone_takes_the_newest_session_here(self):
        # One word that names a profile is where it goes, and the session is the newest started
        # from this folder, as with nothing; a word that names no profile is a session's ID.
        h = self.h
        code, out = h.run(SCRIPT, "default", "--dry-run", cwd=self.api)
        self.assertEqual(code, 0, out)
        self.assertIn('Would move session 11111111 "Newest here" from %s to %s:' % (h.shown(self.work), h.shown(self.default)), out)
        code, out = h.run("fork", "default", "--dry-run", cwd=self.api)
        self.assertEqual(code, 0, out)
        self.assertIn('Would fork session 11111111 "Newest here" from %s to %s' % (h.shown(self.work), h.shown(self.default)), out)
        code, out = h.run(SCRIPT, "default", cwd=self.api)  # not a dry run: asked, so a script has to say both
        self.assertEqual(code, 1, out)
        self.assertIn("Say which session and where, when there's no terminal to ask: hatcheck move <session> <profile>", out)
        code, out = h.run(SCRIPT, "nosuch", "--dry-run", cwd=self.api)  # names no profile: a session's ID
        self.assertEqual(code, 1, out)
        self.assertIn("No session starting with nosuch", out)
        code, out = h.run(SCRIPT, "default", "--dry-run", cwd=h.folder("code", "quiet"))
        self.assertEqual(code, 1, out)
        self.assertIn("No session was started from this folder. Give its ID: hatcheck move <session> <profile>", out)
        # A word that starts a session's ID and names a profile that exists too is asked for more.
        hex_named = h.profile("1111")
        code, out = h.run(SCRIPT, "1111", "--dry-run", cwd=self.api)
        self.assertEqual(code, 1, out)
        self.assertIn("1111 starts a session's ID and names the profile %s; give more of the ID, or say both: "
                      "hatcheck move <session> 1111" % h.shown(hex_named), out)
        code, out = h.run(SCRIPT, "11111111", "1111", "--dry-run", cwd=self.api)  # both: as before
        self.assertEqual(code, 0, out)
        self.assertIn("to %s:" % h.shown(hex_named), out)

    def test_never_to_where_it_already_is(self):
        h = self.h
        quiet = h.folder("code", "quiet")
        h.session(self.default, quiet, "33333333-3333-4333-8333-333333333333", "Already home", 1)
        for args in ((), ("--dry-run",), ("33333333",)):
            code, out = h.run(SCRIPT, *args, cwd=quiet)
            self.assertEqual(code, 1, out)
            self.assertIn('Session 33333333%s is already in %s. Say where it goes: hatcheck move 33333333 <profile>'
                          % ('' if args == ("33333333",) else ' "Already home"', h.shown(self.default)), out)
        # Given both, the same answer as before.
        code, out = h.run(SCRIPT, "33333333", "default", cwd=quiet)
        self.assertEqual(code, 1, out)
        self.assertIn("Session 33333333 is already in %s, and in no other profile" % h.shown(self.default), out)

    def test_a_script_has_to_say_both(self):
        h = self.h
        for args in ((), (S1[:8],), ("--json",)):
            code, out = h.run(SCRIPT, *args, cwd=self.api)
            self.assertEqual(code, 1, out)
            self.assertIn("Say which session and where, when there's no terminal to ask: hatcheck move <session> <profile>", out)
        self.assertTrue(os.path.exists(os.path.join(self.work, "projects", re.sub(r"[^A-Za-z0-9]", "-", self.api), S1 + ".jsonl")))
        code, out = h.run(SCRIPT, cwd=h.folder("code", "quiet"))
        self.assertEqual(code, 1, out)
        self.assertIn("No session was started from this folder. Give its ID: hatcheck move <session> <profile>", out)

    @unittest.skipIf(WINDOWS, "needs a pseudo-terminal")
    def test_asks_in_a_terminal(self):
        h = self.h
        code, out = in_terminal(argv_of("fork"), [(b"? [y/N] ", b"n\n")], h.env(), self.api)
        self.assertEqual(code, 1, out)
        self.assertIn('Fork session 11111111 "Newest here" from %s to %s? [y/N]' % (h.shown(self.work), h.shown(self.default)), out)
        self.assertIn("Nothing changed.", out)
        code, out = in_terminal(argv_of(SCRIPT), [(b"? [y/N] ", b"y\n"), (b"Resume it now", b"n\n")], h.env(), self.api)
        self.assertEqual(code, 0, out)
        self.assertIn('Move session 11111111 "Newest here" from %s to %s? [y/N]' % (h.shown(self.work), h.shown(self.default)), out)
        self.assertIn("Moved session 11111111", out)
        self.assertTrue(os.path.exists(os.path.join(self.default, "projects", re.sub(r"[^A-Za-z0-9]", "-", self.api), S1 + ".jsonl")))
