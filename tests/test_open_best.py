"""hatcheck open, hatcheck resume and hatcheck best against a throwaway home: which session
open picks and the command it runs or prints, the terminal it opens (a fake one), what resume
shows and asks before it runs, and how best ranks accounts and where it says to carry a
session. No real profiles, no network."""
import os
import subprocess
import sys
import tempfile
import time
import unittest

from helpers import ENGINE, NOW, WINDOWS, FakeHome, in_terminal, iso, write

A = "aaaaaaaa-0000-4000-8000-000000000001"
B = "bbbbbbbb-0000-4000-8000-000000000002"
X = "019f0000-aaaa-7000-8000-000000000001"


def run(h, args, cwd=None, **env):
    p = subprocess.run([ENGINE] + list(args), cwd=cwd or h.root,
                       env=dict(h.env(), **env), capture_output=True, text=True, encoding="utf-8")
    return p.returncode, p.stdout + p.stderr


class Open(unittest.TestCase):
    def setUp(self):
        self.h = h = FakeHome()
        self.addCleanup(h.cleanup)
        self.work, self.b = h.profile("work"), h.profile("b")
        self.codex = h.codex_home("default")
        self.api = h.folder("code", "api")
        h.session(self.work, self.api, A, "Fix the login test", 5)
        h.codex_session(self.codex, self.api, X, "Speed up the build", 20)

    def test_print(self):
        code, out = run(self.h, ["open", "--print", A[:8]])
        self.assertEqual(code, 0, out)
        cd, command = out.splitlines()
        self.assertEqual(cd, "cd " + self.h.shown(self.api))
        self.assertIn(".claude-work", command)  # cl isn't on the PATH here, so the full command
        self.assertTrue(command.rstrip().endswith("claude --resume %s" % A) or "claude --resume %s;" % A in command)
        code, out = run(self.h, ["open", "--print", X[:13]])
        self.assertEqual(out.splitlines()[1], "codex resume " + X)

    def test_newest_session_in_this_folder(self):
        code, out = run(self.h, ["open", "--print"], cwd=self.api)
        self.assertIn("--resume " + A, out)  # 5 minutes ago, ahead of Codex's 20
        self.h.codex_session(self.codex, self.api, "019f0000-bbbb-7000-8000-000000000002", "Newer in Codex", 1)
        code, out = run(self.h, ["open", "--print"], cwd=self.api)
        self.assertIn("codex resume 019f0000-bbbb", out)
        code, out = run(self.h, ["open", "--print"])
        self.assertEqual(code, 1, out)
        self.assertIn("No session was started from this folder", out)

    def test_copies_in_two_profiles(self):
        self.h.session(self.b, self.api, A, "Fix the login test", 1)  # carried on in b
        code, out = run(self.h, ["open", "--print", A[:8]])
        self.assertIn(".claude-b", out)
        code, out = run(self.h, ["open", "--print", A[:8], "--from", "work"])
        self.assertIn(".claude-work", out)
        code, out = run(self.h, ["open", "--print", A[:8], "--from", "nosuch"])
        self.assertIn("isn't in", out)

    def test_refuses_an_open_session(self):
        self.h.register_open(self.work, A)
        code, out = run(self.h, ["open", A[:8]], HATCHECK_TERMINAL="false")
        self.assertEqual(code, 1, out)
        self.assertIn("is open in pid", out)

    @unittest.skipIf(WINDOWS, "a POSIX shell and a fake terminal script")
    def test_new_terminal_and_here(self):
        bindir = os.path.join(self.h.root, "fakebin")
        record = os.path.join(self.h.root, "terminal-args")
        write(os.path.join(bindir, "fake-term"), '#!/bin/sh\nprintf "%%s\\n" "$@" > "%s"\n' % record)
        write(os.path.join(bindir, "claude"), '#!/bin/sh\necho "CLAUDE_CONFIG_DIR=$CLAUDE_CONFIG_DIR cwd=$(pwd) args=$*"\n')
        for name in ("fake-term", "claude"):
            os.chmod(os.path.join(bindir, name), 0o755)
        code, out = run(self.h, ["open", A[:8]], HATCHECK_TERMINAL=os.path.join(bindir, "fake-term"))
        self.assertEqual(code, 0, out)
        self.assertIn('Resuming session aaaaaaaa "Fix the login test" in a new fake-term window', out)
        for _ in range(100):  # the terminal starts on its own
            if os.path.exists(record) and os.path.getsize(record):
                break
            time.sleep(0.05)
        with open(record, encoding="utf-8") as fh:
            argv = fh.read().splitlines()
        self.assertEqual(argv[:5], ["-e", "/bin/sh", "-l", "-i", "-c"])
        self.assertEqual(argv[5], "cd %s && CLAUDE_CONFIG_DIR=~/.claude-work claude --resume %s; exec /bin/sh -l"
                         % (self.api, A))
        code, out = run(self.h, ["open", A[:8], "--here"], PATH=bindir + ":/bin:/usr/bin")
        self.assertEqual(code, 0, out)
        self.assertIn("CLAUDE_CONFIG_DIR=%s cwd=%s args=--resume %s" % (self.work, self.api, A), out)
        code, out = run(self.h, ["open", A[:8], "--terminal", os.path.join(bindir, "nosuch")])
        self.assertEqual(code, 1, out)
        self.assertIn("Couldn't open a terminal", out)
        self.assertIn("Run these instead:", out)


class Resume(unittest.TestCase):
    """hatcheck resume (hat r): the newest session started from this folder, carried on in
    this terminal once the question is answered."""
    def setUp(self):
        self.h = h = FakeHome()
        self.addCleanup(h.cleanup)
        self.work, self.b = h.profile("work"), h.profile("b")
        self.api = h.folder("code", "api")
        h.session(self.work, self.api, A, "Fix the login test", 5)
        self.bindir = os.path.join(h.root, "fakebin")
        write(os.path.join(self.bindir, "claude"), '#!/bin/sh\necho "CLAUDE_CONFIG_DIR=$CLAUDE_CONFIG_DIR cwd=$(pwd) args=$*"\n')
        os.chmod(os.path.join(self.bindir, "claude"), 0o755)
        self.path = self.bindir + ":/bin:/usr/bin"
        self.ran = "CLAUDE_CONFIG_DIR=%s cwd=%s args=--resume %s" % (self.work, self.api, A)

    def test_print(self):
        code, out = run(self.h, ["resume", "--print"], cwd=self.api)
        self.assertEqual(code, 0, out)
        self.assertEqual(out.splitlines()[0], "cd " + self.h.shown(self.api))
        self.assertIn("claude --resume " + A, out)
        code, out = run(self.h, ["resume", "--print"])
        self.assertEqual(code, 1, out)
        self.assertIn("No session was started from this folder", out)

    @unittest.skipIf(WINDOWS, "a POSIX shell and a pseudo-terminal")
    def test_shows_the_session_and_asks(self):
        env = dict(self.h.env(), PATH=self.path)
        code, out = in_terminal([ENGINE, "resume"], [(b"[Y/n] ", b"\n")], env, self.api)  # Enter: yes
        self.assertEqual(code, 0, out)
        self.assertIn('Session aaaaaaaa "Fix the login test" \u00b7 Claude Code in %s \u00b7 5m ago' % self.h.shown(self.work), out)
        self.assertIn("claude --resume " + A, out)
        self.assertIn("Resume it here, in %s? [Y/n]" % self.h.shown(self.api), out)
        self.assertIn(self.ran, out)
        code, out = in_terminal([ENGINE, "r"], [(b"[Y/n] ", b"n\n")], env, self.api)  # hat r, declined
        self.assertEqual(code, 1, out)
        self.assertIn("Nothing started.", out)
        self.assertNotIn("args=--resume", out)
        # The folder bound to another profile is flagged, as the list flags the session.
        code, out = run(self.h, ["bind", "b"], cwd=self.api)
        self.assertEqual(code, 0, out)
        code, out = in_terminal([ENGINE, "r"], [(b"[Y/n] ", b"n\n")], env, self.api)
        self.assertIn("\u26a0 folder bound to " + self.h.shown(self.b), out)

    @unittest.skipIf(WINDOWS, "a POSIX shell")
    def test_without_a_terminal_to_ask_at(self):
        code, out = run(self.h, ["resume"], cwd=self.api, PATH=self.path)
        self.assertEqual(code, 1, out)
        self.assertIn("Say --yes when there's no terminal to ask at", out)
        self.assertNotIn("args=--resume", out)
        code, out = run(self.h, ["resume", "--yes"], cwd=self.api, PATH=self.path)
        self.assertEqual(code, 0, out)
        self.assertIn('Resuming session aaaaaaaa "Fix the login test" here, in %s' % self.h.shown(self.api), out)
        self.assertIn(self.ran, out)

    def test_refuses_an_open_session(self):
        self.h.register_open(self.work, A)
        code, out = run(self.h, ["resume", "--yes", A[:8]], PATH=self.path)
        self.assertEqual(code, 1, out)
        self.assertIn("is open in pid", out)
        self.assertNotIn("args=--resume", out)


def fable(used, days, weekly_used=None):
    rows = [{"kind": "session", "group": "session", "percent": 20, "severity": "normal", "resets_at": iso(NOW + 3600)},
            {"kind": "weekly_scoped", "group": "weekly", "percent": used, "severity": "normal",
             "resets_at": iso(NOW + days * 86400), "scope": {"model": {"display_name": "Fable"}}}]
    if weekly_used is not None:
        rows.append({"kind": "weekly_all", "group": "weekly", "percent": weekly_used, "severity": "normal",
                     "resets_at": iso(NOW + 5 * 86400)})
    return {"limits": rows}


class Best(unittest.TestCase):
    """hatcheck best: the best account in full, the move command that carries a session to it,
    and the ranking after it."""
    def setUp(self):
        self.h = h = FakeHome()
        self.addCleanup(h.cleanup)
        self.soon = h.profile("soon", "acct-1", usage=fable(80, 1))
        self.later = h.profile("later", "acct-2", usage=fable(50, 3))
        self.spent = h.profile("spent", "acct-3", usage=fable(100, 0.5))
        self.pro = h.profile("pro", "acct-4", usage={"limits": [
            {"kind": "session", "group": "session", "percent": 90, "resets_at": iso(NOW + 3600)},
            {"kind": "weekly_all", "group": "weekly", "percent": 30, "resets_at": iso(NOW + 2 * 86400)}]})
        self.api = h.folder("code", "api")

    def ranked(self, out):
        """The ranking's lines, after the account shown in full."""
        return [line for line in out.splitlines() if "\u2192" in line]

    def test_the_best_account_in_full(self):
        code, out = run(self.h, ["best", "--cached"])
        self.assertEqual(code, 0, out)
        # soon, as hatcheck usage shows it: the heading, then its rows with their bars.
        self.assertIn("Best Claude Code account now: %s  " % self.h.shown(self.soon), out)
        self.assertRegex(out, r"session \(5h\) +80% ████████░░")
        self.assertRegex(out, r"week \(Fable\) +20% ██░░░░░░░░")
        # No session here: how to start there, and how to carry one.
        self.assertIn("Start there with:\n  cl soon\n", out)
        self.assertIn("Carry a session there with:\n  hatcheck move <session> soon\n", out)
        # Then the rest, in the ranking's order, a line each, without soon again.
        lines = self.ranked(out)
        self.assertEqual([line.split()[1] for line in lines], [self.h.shown(p) for p in (self.later, self.spent, self.pro)])
        self.assertTrue(lines[0].lstrip().startswith("2. "), lines[0])
        self.assertIn("Fable 50% left, resets", lines[0])
        self.assertRegex(lines[0], r", 0\.7%/h")  # 50% left over 3 days: what it can spend an hour
        self.assertIn("Fable 0% left, back", lines[1])
        self.assertNotIn("%/h", lines[1])
        self.assertRegex(lines[2], r"5h 10%, 10\.0%/h")  # the tightest limit first, for an account without Fable
        self.assertIn("Then:", out)
        self.assertIn("Advice only", out)

    def test_nearly_out_goes_behind_fresher_accounts(self):
        low = self.h.profile("low", "acct-6", usage=fable(95, 0.3))  # resets soonest, but 95% used
        code, out = run(self.h, ["best", "--cached"])
        self.assertIn("Best Claude Code account now: %s  " % self.h.shown(self.soon), out)
        lines = self.ranked(out)
        self.assertEqual([line.split()[1] for line in lines],
                         [self.h.shown(p) for p in (self.later, low, self.spent, self.pro)])
        self.assertIn("Fable 5% left, resets", lines[1])

    def test_wait_answers_at_once_with_room(self):
        code, out = run(self.h, ["best", "--cached", "--wait"])
        self.assertEqual(code, 0, out)
        self.assertNotIn("waiting", out)
        self.assertIn("Best Claude Code account now: " + self.h.shown(self.soon), out)
        # For one profile that has room now: the same.
        code, out = run(self.h, ["best", "--cached", "--wait", "later"])
        self.assertEqual(code, 0, out)
        self.assertNotIn("waiting", out)

    def test_wait_sleeps_until_the_reset(self):
        h = FakeHome()
        self.addCleanup(h.cleanup)
        # Both out of Fable: one back in a second, the other in a day.
        soon = h.profile("soon", "acct-1", usage={"limits": [
            {"kind": "weekly_scoped", "group": "weekly", "percent": 100, "resets_at": iso(time.time() + 1),
             "scope": {"model": {"display_name": "Fable"}}}]})
        h.profile("later", "acct-2", usage=fable(100, 1))
        started = time.time()
        code, out = run(h, ["best", "--cached", "--wait"])
        self.assertEqual(code, 0, out)
        self.assertGreater(time.time() - started, 1)
        self.assertIn("No room yet; waiting for a Claude Code account until", out)
        # Its reset passed while waiting, so the snapshot's limit counts as full again.
        self.assertIn("Best Claude Code account now: " + h.shown(soon), out)
        self.assertEqual(out.count("waiting"), 1)

    def test_wait_with_nothing_to_wait_for(self):
        h = FakeHome()
        self.addCleanup(h.cleanup)
        h.profile("solo", "acct-1")  # no usage at all
        code, out = run(h, ["best", "--cached", "--wait"])
        self.assertEqual(code, 1, out)
        self.assertIn("No Claude Code account has room, and none says when it will again", out)
        code, out = run(h, ["best", "--cached", "--wait", "nowhere"])
        self.assertEqual(code, 1, out)
        self.assertIn("has no sessions or usage to go by", out)

    def test_where_to_carry_a_session(self):
        self.h.session(self.spent, self.api, A, "Out of Fable here", 5)
        code, out = run(self.h, ["best", "--for", A[:8], "--cached"])
        self.assertEqual(code, 0, out)
        self.assertIn('Session aaaaaaaa "Out of Fable here" is in %s: Fable 0%% left' % self.h.shown(self.spent), out)
        self.assertIn("Best Claude Code account now: %s  " % self.h.shown(self.soon), out)
        self.assertRegex(out, r"week \(Fable\) +20% ██░░░░░░░░")
        self.assertIn("Carry it there with:\n  hatcheck move aaaaaaaa soon", out)
        self.assertIn("(hatcheck fork aaaaaaaa soon keeps the original)", out)
        self.assertIn("(where it is)", next(line for line in self.ranked(out) if self.h.shown(self.spent) in line))
        # From the session's folder, without --for: the same session, and the shorter command,
        # which takes the newest session started here as hatcheck move does.
        code, out = run(self.h, ["best", "--cached"], cwd=self.api)
        self.assertEqual(code, 0, out)
        self.assertIn('Session aaaaaaaa "Out of Fable here", the newest started from this folder, is in %s' % self.h.shown(self.spent), out)
        self.assertIn("Carry it there with:\n  hatcheck move soon", out)
        self.assertIn("(hatcheck fork soon keeps the original)", out)
        # A copy in two profiles: --from says which one, the one used last.
        self.h.session(self.later, self.api, A, "Out of Fable here", 2)
        code, out = run(self.h, ["best", "--for", A[:8], "--cached"])
        self.assertIn("is in %s: Fable 50%% left" % self.h.shown(self.later), out)
        self.assertIn("hatcheck move aaaaaaaa soon --from later", out)
        # A session in the best account already: the resume command instead.
        self.h.session(self.soon, self.api, B, "Already in the best place", 5)
        code, out = run(self.h, ["best", "--for", B[:8], "--cached"])
        self.assertEqual(code, 0, out)
        self.assertIn("Best Claude Code account now: %s  " % self.h.shown(self.soon), out)
        self.assertIn("Its own account is the best one now; resume it there with:", out)
        self.assertIn("cd " + self.h.shown(self.api), out)
        self.assertIn("cl soon --resume " + B, out)
        self.assertNotIn("hatcheck move", out.split("Advice only")[0])

    def test_blocked_by_the_weekly_limit(self):
        blocked = self.h.profile("blocked", "acct-5", usage=fable(10, 0.2, weekly_used=100))
        code, out = run(self.h, ["best", "--cached"])
        line = next(line for line in out.splitlines() if self.h.shown(blocked) + " " in line)
        self.assertIn("(the weekly limit blocks Fable)", line)

    def test_no_account_has_room(self):
        h = FakeHome()
        self.addCleanup(h.cleanup)
        spent = h.profile("spent", "acct-3", usage=fable(100, 0.5))
        h.profile("gone", "acct-4", usage=fable(100, 2))
        code, out = run(h, ["best", "--cached"])
        self.assertEqual(code, 1, out)
        self.assertIn("No Claude Code account has room now:\n", out)
        self.assertEqual([line.split()[1] for line in self.ranked(out)], [h.shown(spent), h.shown(h.profile("gone"))])
        self.assertIn("Fable 0% left, back", out)
        self.assertNotIn("Best Claude Code account", out)


if __name__ == "__main__":
    unittest.main()
