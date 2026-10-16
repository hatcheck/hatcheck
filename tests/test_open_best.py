"""hatcheck open and hatcheck best against a throwaway home: which session open picks and
the command it runs or prints, the terminal it opens (a fake one), and how best ranks
accounts and where it says to carry a session. No real profiles, no network."""
import os
import subprocess
import sys
import tempfile
import time
import unittest

from helpers import NOW, REPO, WINDOWS, FakeHome, iso, load, write

A = "aaaaaaaa-0000-4000-8000-000000000001"
B = "bbbbbbbb-0000-4000-8000-000000000002"
X = "019f0000-aaaa-7000-8000-000000000001"


def run(h, args, cwd=None, **env):
    p = subprocess.run([sys.executable, os.path.join(REPO, "hatcheck.py")] + list(args), cwd=cwd or h.root,
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

    def test_terminal_commands(self):
        o = load("open")
        cwd, command, shell = "/w/my app", "cl work --resume 1", "/bin/zsh"
        stay = "cd '/w/my app' && cl work --resume 1; exec /bin/zsh -l"
        argv, wait = o.terminal_argv("terminal", cwd, command, shell)
        self.assertEqual((argv[0], wait), ("osascript", True))
        self.assertIn('do script "cd \'/w/my app\' && cl work --resume 1"', argv)
        argv, _ = o.terminal_argv("iTerm.app", cwd, command, shell)
        self.assertIn('tell current session of current window to write text "cd \'/w/my app\' && cl work --resume 1"',
                      argv)
        if sys.platform == "darwin":
            self.assertEqual(o.terminal_argv("ghostty", cwd, command, shell)[0],
                             ["open", "-na", "Ghostty.app", "--args", "--working-directory=/w/my app", "-e",
                              shell, "-l", "-i", "-c", stay])
        self.assertEqual(o.terminal_argv("wezterm", cwd, command, shell),
                         (["wezterm", "start", "--cwd", cwd, "--", shell, "-l", "-i", "-c", stay], False))
        self.assertEqual(o.terminal_argv("gnome-terminal", cwd, command, shell)[0][:3],
                         ["gnome-terminal", "--working-directory=/w/my app", "--"])
        self.assertEqual(o.terminal_argv("konsole", cwd, command, shell)[0][:4], ["konsole", "--workdir", cwd, "-e"])
        self.assertEqual(o.terminal_argv("xterm", cwd, command, shell)[0], ["xterm", "-e", shell, "-l", "-i", "-c", stay])
        argv, _ = o.terminal_argv("wt", "C:\\w", "$env:X=1; claude; $env:X=$null", shell)
        self.assertEqual(argv[:3], ["wt", "-d", "C:\\w"])
        self.assertEqual(argv[-1], "$env:X=1\\; claude\\; $env:X=$null")  # wt's own separator, escaped

    @unittest.skipUnless(sys.platform == "darwin" and os.path.exists("/usr/bin/osacompile"), "macOS AppleScript")
    def test_terminal_applescript_compiles(self):
        argv, _ = load("open").terminal_argv("terminal", '/w/a "b"', "cl work --resume 1", "/bin/zsh")
        lines = [argv[i + 1] for i, a in enumerate(argv) if a == "-e"]
        with tempfile.TemporaryDirectory() as tmp:
            p = subprocess.run(["/usr/bin/osacompile", "-o", os.path.join(tmp, "x.scpt"), "-e"] + lines[:1]
                               + sum((["-e", line] for line in lines[1:]), []), capture_output=True, text=True)
            self.assertEqual(p.returncode, 0, p.stderr)


def fable(used, days, weekly_used=None):
    rows = [{"kind": "session", "group": "session", "percent": 20, "severity": "normal", "resets_at": iso(NOW + 3600)},
            {"kind": "weekly_scoped", "group": "weekly", "percent": used, "severity": "normal",
             "resets_at": iso(NOW + days * 86400), "scope": {"model": {"display_name": "Fable"}}}]
    if weekly_used is not None:
        rows.append({"kind": "weekly_all", "group": "weekly", "percent": weekly_used, "severity": "normal",
                     "resets_at": iso(NOW + 5 * 86400)})
    return {"limits": rows}


class Best(unittest.TestCase):
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

    def test_ranking(self):
        code, out = run(self.h, ["best", "--cached"])
        self.assertEqual(code, 0, out)
        lines = [line for line in out.splitlines() if "→" in line]
        self.assertEqual([line.split()[1] for line in lines],
                         [self.h.shown(p) for p in (self.soon, self.later, self.spent, self.pro)])
        self.assertIn("Fable 20% left, resets", lines[0])
        self.assertTrue(lines[0].rstrip().endswith("→ cl soon"))
        self.assertIn("Fable 0% left, back", lines[2])
        self.assertIn("5h 10%", lines[3])  # the tightest limit first, for an account without Fable
        self.assertIn("Advice only", out)

    def test_where_to_carry_a_session(self):
        self.h.session(self.spent, self.api, A, "Out of Fable here", 5)
        code, out = run(self.h, ["best", "--for", A[:8], "--cached"])
        self.assertEqual(code, 0, out)
        self.assertIn('Session aaaaaaaa "Out of Fable here" is in %s: Fable 0%% left' % self.h.shown(self.spent), out)
        self.assertIn("Carry it to %s: Fable 20%% left" % self.h.shown(self.soon), out)
        self.assertIn("hatcheck move aaaaaaaa soon --from spent", out)
        self.h.session(self.soon, self.api, B, "Already in the best place", 5)
        code, out = run(self.h, ["best", "--for", B[:8], "--cached"])
        self.assertIn("Its own account is the best one now", out)
        self.assertIn("cl soon --resume " + B, out)

    def test_blocked_by_the_weekly_limit(self):
        blocked = self.h.profile("blocked", "acct-5", usage=fable(10, 0.2, weekly_used=100))
        code, out = run(self.h, ["best", "--cached"])
        line = next(line for line in out.splitlines() if self.h.shown(blocked) + " " in line)
        self.assertIn("(the weekly limit blocks Fable)", line)


if __name__ == "__main__":
    unittest.main()
