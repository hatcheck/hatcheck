"""cl, cx, claude-z, claude-k, claude-where and the installers: the sh versions on macOS
and Linux, the PowerShell ones on Windows. A fake claude stands in for Claude Code, and a
fake codex for Codex."""
import os
import subprocess
import sys
import unittest

from helpers import REPO, WINDOWS, FakeHome, write

ID = "abcdef12-0000-4000-8000-000000000001"
CODEX_ID = "019f0000-aaaa-7000-8000-000000000001"
POWERSHELL = ["powershell", "-NoLogo", "-NoProfile", "-ExecutionPolicy", "Bypass", "-File"]
SHOWN = ("CLAUDE_CONFIG_DIR", "ANTHROPIC_BASE_URL", "ANTHROPIC_AUTH_TOKEN", "ANTHROPIC_API_KEY", "ANTHROPIC_MODEL")


class Commands(unittest.TestCase):
    def setUp(self):
        self.h = FakeHome()
        self.addCleanup(self.h.cleanup)
        work = self.h.profile("work")
        self.transcript = self.h.session(work, self.h.folder("code"), ID, "A session")
        os.makedirs(self.transcript[:-len(".jsonl")])  # its data folder
        self.rollout = self.h.codex_session(self.h.codex_home("work"), self.h.folder("code"), CODEX_ID, "A Codex session")
        # A fake claude that prints the profile and provider it would use, and its arguments;
        # and claude-provider, which cl asks for a provider's settings.
        self.bin = os.path.join(self.h.root, "fakebin")
        provider = os.path.join(REPO, "claude-provider.py")
        if WINDOWS:
            lines = ["@echo off", "echo args=%*"] + [
                "if defined %s (echo %s=%%%s%%) else (echo %s=unset)" % (v, v, v, v) for v in SHOWN]
            write(os.path.join(self.bin, "claude.cmd"), "\r\n".join(lines) + "\r\n")
            write(os.path.join(self.bin, "claude-provider.cmd"), '@"%s" "%s" %%*\r\n' % (sys.executable, provider))
            write(os.path.join(self.bin, "codex.cmd"), "@echo off\r\necho args=%*\r\nif defined CODEX_HOME "
                  "(echo CODEX_HOME=%CODEX_HOME%) else (echo CODEX_HOME=unset)\r\n")
        else:
            lines = ["#!/bin/sh", 'echo "args=$*"'] + ['echo "%s=${%s-unset}"' % (v, v) for v in SHOWN]
            write(os.path.join(self.bin, "claude"), "\n".join(lines) + "\n")
            write(os.path.join(self.bin, "claude-provider"), '#!/bin/sh\nexec "%s" "%s" "$@"\n' % (sys.executable, provider))
            write(os.path.join(self.bin, "codex"), '#!/bin/sh\necho "args=$*"\necho "CODEX_HOME=${CODEX_HOME-unset}"\n')
            for name in ("claude", "claude-provider", "codex"):
                os.chmod(os.path.join(self.bin, name), 0o755)
            for name in ("claude-z", "claude-k"):  # how install.sh installs them
                os.symlink(os.path.join(REPO, "cl"), os.path.join(self.bin, name))
        self.h.provider_config("z", 'CZ_API_KEY="zai-test-key"\nCZ_MODEL_OPUS="glm-5.3[1m]"\n')

    def run_command(self, argv, extra_env=None, stdin="", fakes=True):
        env = dict(self.h.env(), **(extra_env or {}))
        env["PATH"] = (self.bin + os.pathsep if fakes else "") + (os.environ["PATH"] if WINDOWS else "/bin:/usr/bin")
        p = subprocess.run(argv, cwd=self.h.root, env=env, input=stdin, capture_output=True, text=True,
                           encoding="utf-8")
        return p.returncode, p.stdout + p.stderr

    def cl(self, *args, **kw):
        if WINDOWS:
            return self.run_command(POWERSHELL + [os.path.join(REPO, "cl.ps1")] + list(args), **kw)
        return self.run_command(["sh", os.path.join(REPO, "cl")] + list(args), **kw)

    def cx(self, *args, **kw):
        if WINDOWS:
            return self.run_command(POWERSHELL + [os.path.join(REPO, "cx.ps1")] + list(args), **kw)
        return self.run_command(["sh", os.path.join(REPO, "cx")] + list(args), **kw)

    def claude_on(self, name, *args, **kw):
        """claude-z or claude-k, run the way their installed launchers run them."""
        if WINDOWS:
            return self.run_command(POWERSHELL + [os.path.join(REPO, "cl.ps1"), "--as", name] + list(args), **kw)
        return self.run_command([name] + list(args), **kw)

    def where(self, *args):
        if WINDOWS:
            return self.run_command(POWERSHELL + [os.path.join(REPO, "claude-where.ps1")] + list(args))
        return self.run_command(["sh", os.path.join(REPO, "claude-where")] + list(args))

    def test_cl_runs_claude_with_the_profile(self):
        code, out = self.cl("work", "--resume", ID)
        self.assertEqual(code, 0, out)
        self.assertIn("args=--resume " + ID, out)
        self.assertIn("CLAUDE_CONFIG_DIR=" + os.path.join(self.h.home, ".claude-work"), out)
        self.assertIn("ANTHROPIC_BASE_URL=unset", out)

    def test_cl_needs_a_profile(self):
        for args in ((), ("--resume", ID), ("-k",)):
            code, out = self.cl(*args)
            self.assertEqual(code, 2, out)
            self.assertIn("usage: cl [-z|-k] <profile>", out)

    def test_cl_with_a_provider(self):
        self.h.provider_config("k", 'CK_API_KEY="kimi-test-key"\n')
        code, out = self.cl("-k", "work", "--resume", ID, extra_env={"ANTHROPIC_API_KEY": "sk-ant-from-the-shell"})
        self.assertEqual(code, 0, out)
        for line in ("args=--resume " + ID, "CLAUDE_CONFIG_DIR=" + os.path.join(self.h.home, ".claude-work"),
                     "ANTHROPIC_BASE_URL=https://api.kimi.com/coding/", "ANTHROPIC_AUTH_TOKEN=kimi-test-key",
                     "ANTHROPIC_API_KEY=unset", "ANTHROPIC_MODEL=k3[1m]"):
            self.assertIn(line, out)

    def test_cx_runs_codex_with_the_home(self):
        home = os.path.join(self.h.home, ".codex-fresh")
        code, out = self.cx("fresh", "resume", CODEX_ID)
        self.assertEqual(code, 0, out)
        self.assertIn("args=resume " + CODEX_ID, out)
        self.assertIn("CODEX_HOME=" + home, out)
        self.assertTrue(os.path.isdir(home))  # made: Codex won't start without it
        for args in ((), ("--help",)):
            code, out = self.cx(*args)
            self.assertEqual(code, 2, out)
            self.assertIn("usage: cx <profile> [codex args...]", out)

    @unittest.skipIf(WINDOWS, "the runner's PATH could have a codex on it")
    def test_cx_without_codex(self):
        code, out = self.cx("work", fakes=False)
        self.assertEqual(code, 1, out)
        self.assertIn("codex isn't on your PATH", out)

    def test_claude_z_and_k_keep_the_config_dir(self):
        code, out = self.claude_on("claude-z", "--resume", ID)
        self.assertEqual(code, 0, out)
        for line in ("args=--resume " + ID, "CLAUDE_CONFIG_DIR=unset", "ANTHROPIC_MODEL=glm-5.3[1m]",
                     "ANTHROPIC_BASE_URL=https://api.z.ai/api/anthropic", "ANTHROPIC_AUTH_TOKEN=zai-test-key"):
            self.assertIn(line, out)
        mine = os.path.join(self.h.home, ".claude-work")
        code, out = self.claude_on("claude-z", "-p", "hi", extra_env={"CLAUDE_CONFIG_DIR": mine})
        self.assertIn("CLAUDE_CONFIG_DIR=" + mine, out)

    def test_provider_not_set_up(self):
        code, out = self.claude_on("claude-k")
        self.assertEqual(code, 1, out)
        self.assertIn("Kimi isn't set up yet. Run: claude-k reconfig", out)
        self.assertNotIn("args=", out)  # claude didn't run

    def test_reconfig(self):
        code, out = self.claude_on("claude-k", "reconfig")  # nothing typed
        self.assertEqual(code, 1, out)
        self.assertIn("Get an API key from the Kimi Code Console", out)
        self.assertIn("Nothing saved: an API key is needed", out)
        if not WINDOWS:
            code, out = self.claude_on("claude-k", "reconfig", stdin="kimi-typed-key\n\n\n")
            self.assertEqual(code, 0, out)
            code, out = self.claude_on("claude-k")
            self.assertIn("ANTHROPIC_AUTH_TOKEN=kimi-typed-key", out)

    def test_claude_where_finds_a_session_by_prefix(self):
        code, out = self.where(ID[:8])
        self.assertEqual(code, 0, out)
        found = sorted(out.split())
        self.assertEqual(found, sorted([self.transcript, self.transcript[:-len(".jsonl")]]))

    def test_claude_where_finds_codex_sessions(self):
        code, out = self.where(CODEX_ID[:13])
        self.assertEqual(code, 0, out)
        self.assertEqual(out.split(), [self.rollout])
        archived = os.path.join(self.h.home, ".codex", "archived_sessions",
                                "rollout-2026-01-02T03-04-05-019f0000-bbbb-7000-8000-000000000002.jsonl")
        write(archived, "{}\n")
        code, out = self.where("019f0000-bbbb")
        self.assertEqual(out.split(), [archived])
        code, out = self.where("2026")  # the start time in a file name isn't the ID
        self.assertEqual(code, 1, out)

    def test_installer(self):
        target = os.path.join(self.h.root, "installed")
        names = ["cl", "cx", "claude-where", "claude-recent-sessions", "claude-usage", "claude-move-session",
                 "claude-provider", "claude-z", "claude-k"]
        self.h.provider_config("k", 'CK_API_KEY="kimi-test-key"\n')
        if WINDOWS:
            # Made before the repo was renamed from claude-multi-account: still its own, so replaced
            # without -Force.
            old = "@echo off\r\nrem made by claude-multi-account install.ps1\r\npython old\\claude-usage.py %*\r\n"
            write(os.path.join(target, "claude-usage.cmd"), old)
            code, out = self.run_command(POWERSHELL + [os.path.join(REPO, "install.ps1"), "-Bin", target])
            self.assertEqual(code, 0, out)
            for name in names:
                self.assertTrue(os.path.isfile(os.path.join(target, name + ".cmd")), out)
            with open(os.path.join(target, "claude-usage.cmd")) as f:
                self.assertIn("rem made by hatcheck install.ps1", f.read())
            code, out = self.run_command(["cmd", "/c", os.path.join(target, "claude-where.cmd"), ID[:8]])
            self.assertIn(self.transcript, out)
            code, out = self.run_command(["cmd", "/c", os.path.join(target, "claude-recent-sessions.cmd"), "--no-usage"])
            self.assertIn("cl work --resume " + ID, out)
            code, out = self.run_command(["cmd", "/c", os.path.join(target, "claude-k.cmd"), "--version"])
            self.assertIn("ANTHROPIC_BASE_URL=https://api.kimi.com/coding/", out)
            self.assertIn("args=--version", out)
            write(os.path.join(target, "claude-where.cmd"), old)  # an old one is removed too
            code, out = self.run_command(POWERSHELL + [os.path.join(REPO, "install.ps1"), "-Bin", target, "-Uninstall"])
            self.assertEqual(os.listdir(target), [], out)
        else:
            # A copy that the claude-z repo's install-z.sh left: replaced without --force.
            write(os.path.join(target, "claude-z"), '#!/usr/bin/env bash\nCONFIG_DIR="$HOME/.config/claude-z"\n')
            code, out = self.run_command(["bash", os.path.join(REPO, "install.sh"), "--bin", target])
            self.assertEqual(code, 0, out)
            for name in names:
                self.assertTrue(os.path.islink(os.path.join(target, name)), out)
            self.assertIn("claude-z -> %s (replaced the copy from the claude-z repo" % os.path.join(REPO, "cl"), out)
            code, out = self.run_command([os.path.join(target, "claude-where"), ID[:8]])
            self.assertIn(self.transcript, out)
            code, out = self.run_command([os.path.join(target, "claude-k"), "--version"])
            self.assertIn("ANTHROPIC_BASE_URL=https://api.kimi.com/coding/", out)
            code, out = self.run_command(["bash", os.path.join(REPO, "install.sh"), "--bin", target, "--uninstall"])
            self.assertEqual(os.listdir(target), [], out)


if __name__ == "__main__":
    unittest.main()
