"""cl, claude-where and the installers: the sh versions on macOS and Linux, the
PowerShell ones on Windows. A fake claude stands in for Claude Code."""
import os
import subprocess
import unittest

from helpers import REPO, WINDOWS, FakeHome, write

ID = "abcdef12-0000-4000-8000-000000000001"
POWERSHELL = ["powershell", "-NoLogo", "-NoProfile", "-ExecutionPolicy", "Bypass", "-File"]


class Commands(unittest.TestCase):
    def setUp(self):
        self.h = FakeHome()
        self.addCleanup(self.h.cleanup)
        work = self.h.profile("work")
        self.transcript = self.h.session(work, self.h.folder("code"), ID, "A session")
        os.makedirs(self.transcript[:-len(".jsonl")])  # its data folder
        # A fake claude that prints the profile it would use and its arguments.
        self.bin = os.path.join(self.h.root, "fakebin")
        if WINDOWS:
            write(os.path.join(self.bin, "claude.cmd"), "@echo CLAUDE_CONFIG_DIR=%CLAUDE_CONFIG_DIR% args=%*\r\n")
        else:
            write(os.path.join(self.bin, "claude"), '#!/bin/sh\necho "CLAUDE_CONFIG_DIR=$CLAUDE_CONFIG_DIR args=$*"\n')
            os.chmod(os.path.join(self.bin, "claude"), 0o755)

    def run_command(self, argv):
        env = self.h.env()
        env["PATH"] = self.bin + os.pathsep + (os.environ["PATH"] if WINDOWS else "/bin:/usr/bin")
        p = subprocess.run(argv, cwd=self.h.root, env=env, capture_output=True, text=True, encoding="utf-8")
        return p.returncode, p.stdout + p.stderr

    def cl(self, *args):
        if WINDOWS:
            return self.run_command(POWERSHELL + [os.path.join(REPO, "cl.ps1")] + list(args))
        return self.run_command(["sh", os.path.join(REPO, "cl")] + list(args))

    def where(self, *args):
        if WINDOWS:
            return self.run_command(POWERSHELL + [os.path.join(REPO, "claude-where.ps1")] + list(args))
        return self.run_command(["sh", os.path.join(REPO, "claude-where")] + list(args))

    def test_cl_runs_claude_with_the_profile(self):
        code, out = self.cl("work", "--resume", ID)
        self.assertEqual(code, 0, out)
        self.assertIn("CLAUDE_CONFIG_DIR=%s args=--resume %s" % (os.path.join(self.h.home, ".claude-work"), ID), out)

    def test_cl_needs_a_profile(self):
        code, out = self.cl()
        self.assertEqual(code, 2)
        self.assertIn("usage: cl <profile>", out)

    def test_claude_where_finds_a_session_by_prefix(self):
        code, out = self.where(ID[:8])
        self.assertEqual(code, 0, out)
        found = sorted(out.split())
        self.assertEqual(found, sorted([self.transcript, self.transcript[:-len(".jsonl")]]))

    def test_installer(self):
        target = os.path.join(self.h.root, "installed")
        names = ["cl", "claude-where", "claude-recent-sessions", "claude-usage", "claude-move-session"]
        if WINDOWS:
            code, out = self.run_command(POWERSHELL + [os.path.join(REPO, "install.ps1"), "-Bin", target])
            self.assertEqual(code, 0, out)
            for name in names:
                self.assertTrue(os.path.isfile(os.path.join(target, name + ".cmd")), out)
            code, out = self.run_command(["cmd", "/c", os.path.join(target, "claude-where.cmd"), ID[:8]])
            self.assertIn(self.transcript, out)
            code, out = self.run_command(["cmd", "/c", os.path.join(target, "claude-recent-sessions.cmd"), "--no-usage"])
            self.assertIn("cl work --resume " + ID, out)
            code, out = self.run_command(POWERSHELL + [os.path.join(REPO, "install.ps1"), "-Bin", target, "-Uninstall"])
            self.assertEqual(os.listdir(target), [], out)
        else:
            code, out = self.run_command(["bash", os.path.join(REPO, "install.sh"), "--bin", target])
            self.assertEqual(code, 0, out)
            for name in names:
                self.assertTrue(os.path.islink(os.path.join(target, name)), out)
            code, out = self.run_command([os.path.join(target, "claude-where"), ID[:8]])
            self.assertIn(self.transcript, out)
            code, out = self.run_command(["bash", os.path.join(REPO, "install.sh"), "--bin", target, "--uninstall"])
            self.assertEqual(os.listdir(target), [], out)


if __name__ == "__main__":
    unittest.main()
