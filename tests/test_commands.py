"""cl, cx, claude-z, claude-k, the hatcheck command and the installers: the sh versions on
macOS and Linux, the PowerShell ones on Windows. A fake claude stands in for Claude Code, and
a fake codex for Codex."""
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
        # and hatcheck, which cl asks for a provider's settings.
        self.bin = os.path.join(self.h.root, "fakebin")
        self.fake_bin(self.bin, hatcheck=os.path.join(REPO, "hatcheck.py"))
        self.h.provider_config("z", 'CZ_API_KEY="zai-test-key"\nCZ_MODEL_OPUS="glm-5.3[1m]"\n')

    def fake_bin(self, folder, **commands):
        """Fake claude and codex in folder, and a command for each of `commands`, name=script,
        that runs that repo script with Python."""
        if WINDOWS:
            lines = ["@echo off", "echo args=%*"] + [
                "if defined %s (echo %s=%%%s%%) else (echo %s=unset)" % (v, v, v, v) for v in SHOWN]
            write(os.path.join(folder, "claude.cmd"), "\r\n".join(lines) + "\r\n")
            write(os.path.join(folder, "codex.cmd"), "@echo off\r\necho args=%*\r\nif defined CODEX_HOME "
                  "(echo CODEX_HOME=%CODEX_HOME%) else (echo CODEX_HOME=unset)\r\n")
            for name, script in commands.items():
                write(os.path.join(folder, name.replace("_", "-") + ".cmd"), '@"%s" "%s" %%*\r\n' % (sys.executable, script))
            return
        lines = ["#!/bin/sh", 'echo "args=$*"'] + ['echo "%s=${%s-unset}"' % (v, v) for v in SHOWN]
        write(os.path.join(folder, "claude"), "\n".join(lines) + "\n")
        write(os.path.join(folder, "codex"), '#!/bin/sh\necho "args=$*"\necho "CODEX_HOME=${CODEX_HOME-unset}"\n')
        for name, script in commands.items():
            write(os.path.join(folder, name.replace("_", "-")), '#!/bin/sh\nexec "%s" "%s" "$@"\n' % (sys.executable, script))
        for name in os.listdir(folder):
            os.chmod(os.path.join(folder, name), 0o755)
        for name in ("claude-z", "claude-k"):  # how install.sh installs them
            os.symlink(os.path.join(REPO, "cl"), os.path.join(folder, name))

    def run_command(self, argv, extra_env=None, stdin="", fakes=True, bindir=None, cwd=None):
        env = dict(self.h.env(), **(extra_env or {}))
        env["PATH"] = ((bindir or self.bin) + os.pathsep if fakes else "") + (os.environ["PATH"] if WINDOWS else "/bin:/usr/bin")
        p = subprocess.run(argv, cwd=cwd or self.h.root, env=env, input=stdin, capture_output=True, text=True,
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
        return self.run_command([sys.executable, os.path.join(REPO, "hatcheck.py"), "where"] + list(args))

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
        # A Codex command first, with no ~/.codex-resume and no binding here: not a home's name.
        code, out = self.cx("resume", CODEX_ID)
        self.assertEqual(code, 2, out)
        self.assertIn("cx: resume is a Codex command, not a home's name", out)
        self.assertFalse(os.path.exists(os.path.join(self.h.home, ".codex-resume")))

    def test_cl_and_cx_use_the_folder_binding(self):
        code_dir = self.h.folder("code")
        hatcheck = [sys.executable, os.path.join(REPO, "hatcheck.py")]
        code, out = self.run_command(hatcheck + ["bind", "work"], cwd=code_dir)
        self.assertEqual(code, 0, out)
        inside = self.h.folder("code", "src")
        code, out = self.cl("--resume", ID, cwd=inside)
        self.assertEqual(code, 0, out)
        self.assertIn("cl: this folder is bound to work", out)
        self.assertIn("args=--resume " + ID, out)
        self.assertIn("CLAUDE_CONFIG_DIR=" + os.path.join(self.h.home, ".claude-work"), out)
        code, out = self.cx("resume", CODEX_ID, cwd=inside)  # resume is Codex's, not a profile
        self.assertEqual(code, 0, out)
        self.assertIn("args=resume " + CODEX_ID, out)
        self.assertIn("CODEX_HOME=" + os.path.join(self.h.home, ".codex-work"), out)
        code, out = self.cl("work", "--resume", ID, cwd=inside)  # a profile name still wins
        self.assertNotIn("bound to", out)
        code, out = self.cl("--resume", ID)  # outside the folder, as before
        self.assertEqual(code, 2, out)
        self.assertIn("hatcheck bind <profile>", out)
        # Bound to ~/.claude and ~/.codex: plain claude and codex.
        self.h.profile("default")
        self.h.codex_home()
        code, out = self.run_command(hatcheck + ["bind", "default"], cwd=inside)
        self.assertEqual(code, 0, out)
        code, out = self.cl("--resume", ID, cwd=inside)
        self.assertIn("CLAUDE_CONFIG_DIR=unset", out)
        code, out = self.cx(cwd=inside)
        self.assertEqual(code, 0, out)
        self.assertIn("CODEX_HOME=unset", out)

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

    def test_cl_needs_hatcheck_for_a_provider(self):
        bare = os.path.join(self.h.root, "barebin")  # claude, but no hatcheck
        self.fake_bin(bare)
        code, out = self.cl("-k", "work", bindir=bare)
        self.assertEqual(code, 1, out)
        self.assertIn("hatcheck isn't on your PATH", out)
        self.assertNotIn("args=", out)  # claude didn't run

    def test_hatcheck(self):
        code, out = self.run_command([sys.executable, os.path.join(REPO, "hatcheck.py"), "--help"])
        self.assertEqual(code, 0, out)
        for line in ("hatcheck [sessions] [HOURS]", "hatcheck usage", "hatcheck where SESSION",
                     "hatcheck move SESSION PROFILE", "hatcheck open [SESSION]", "hatcheck best [--for SESSION]",
                     "hatcheck bind PROFILE", "hatcheck unbind [FOLDER]", "hatcheck provider", "take --json",
                     "hatcheck show SESSION", "hatcheck desktop"):
            self.assertIn(line, out)
        code, out = self.run_command([sys.executable, os.path.join(REPO, "hatcheck.py"), "3d", "--no-usage"])
        self.assertEqual(code, 0, out)  # HOURS on its own: the session list
        self.assertIn("cl work --resume " + ID, out)
        code, out = self.run_command([sys.executable, os.path.join(REPO, "hatcheck.py"), "nope"])
        self.assertEqual(code, 2, out)
        self.assertIn("hatcheck: there's no 'nope' subcommand; see hatcheck --help", out)

    def test_hat_is_the_short_name(self):
        hatcheck = os.path.join(REPO, "hatcheck.py")
        code, out = self.run_command([sys.executable, hatcheck, "--as", "hat", "--help"])  # as hat.cmd runs it
        self.assertEqual(code, 0, out)
        self.assertIn("hat is the short name for hatcheck.", out)
        code, out = self.run_command([sys.executable, hatcheck, "--as", "hat", "usage", "--help"])
        self.assertIn("usage: hat usage", out)
        code, out = self.run_command([sys.executable, hatcheck, "--as", "hat", "nope"])
        self.assertIn("hat: there's no 'nope' subcommand; see hat --help", out)
        if not WINDOWS:  # a link named hat, as install.sh makes
            os.symlink(hatcheck, os.path.join(self.bin, "hat"))
            code, out = self.run_command(["hat", "where", "--help"])
            self.assertIn("usage: hat where", out)

    def test_where_finds_a_session_by_prefix(self):
        code, out = self.where(ID[:8])
        self.assertEqual(code, 0, out)
        found = sorted(out.split())
        self.assertEqual(found, sorted([self.transcript, self.transcript[:-len(".jsonl")]]))

    def test_where_finds_codex_sessions(self):
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
        names = ["hatcheck", "hat", "cl", "cx", "claude-z", "claude-k"]
        self.h.provider_config("k", 'CK_API_KEY="kimi-test-key"\n')
        if WINDOWS:
            # Launchers for the commands hatcheck replaced, made before the repo was renamed from
            # claude-multi-account: removed. One this script didn't make is left alone.
            old = "@echo off\r\nrem made by claude-multi-account install.ps1\r\npython old\\claude-usage.py %*\r\n"
            write(os.path.join(target, "claude-usage.cmd"), old)
            write(os.path.join(target, "claude-where.cmd"), "@echo off\r\necho mine\r\n")
            code, out = self.run_command(POWERSHELL + [os.path.join(REPO, "install.ps1"), "-Bin", target])
            self.assertEqual(code, 0, out)
            for name in names:
                self.assertTrue(os.path.isfile(os.path.join(target, name + ".cmd")), out)
            self.assertFalse(os.path.exists(os.path.join(target, "claude-usage.cmd")), out)
            self.assertIn("it's hatcheck usage now", out)
            self.assertTrue(os.path.isfile(os.path.join(target, "claude-where.cmd")), out)
            os.remove(os.path.join(target, "claude-where.cmd"))
            code, out = self.run_command(["cmd", "/c", os.path.join(target, "hatcheck.cmd"), "where", ID[:8]])
            self.assertIn(self.transcript, out)
            code, out = self.run_command(["cmd", "/c", os.path.join(target, "hatcheck.cmd"), "--no-usage"])
            self.assertIn("cl work --resume " + ID, out)
            code, out = self.run_command(["cmd", "/c", os.path.join(target, "hat.cmd"), "--help"])
            self.assertIn("hat is the short name for hatcheck.", out)
            code, out = self.run_command(["cmd", "/c", os.path.join(target, "claude-k.cmd"), "--version"])
            self.assertIn("ANTHROPIC_BASE_URL=https://api.kimi.com/coding/", out)
            self.assertIn("args=--version", out)
            write(os.path.join(target, "claude-where.cmd"), old)  # removed on uninstall too
            code, out = self.run_command(POWERSHELL + [os.path.join(REPO, "install.ps1"), "-Bin", target, "-Uninstall"])
            self.assertEqual(os.listdir(target), [], out)
        else:
            # A copy that the claude-z repo's install-z.sh left: replaced without --force.
            write(os.path.join(target, "claude-z"), '#!/usr/bin/env bash\nCONFIG_DIR="$HOME/.config/claude-z"\n')
            # Links to the commands hatcheck replaced: removed. A file this script didn't make stays.
            repo = os.path.realpath(REPO)  # as install.sh spells it
            os.symlink(os.path.join(repo, "claude-recent-sessions.py"), os.path.join(target, "claude-recent-sessions"))
            os.symlink(os.path.join(repo, "claude-usage.py"), os.path.join(target, "claude-usage"))
            write(os.path.join(target, "claude-where"), "#!/bin/sh\necho mine\n")
            code, out = self.run_command(["bash", os.path.join(REPO, "install.sh"), "--bin", target])
            self.assertEqual(code, 0, out)
            for name in names:
                self.assertTrue(os.path.islink(os.path.join(target, name)), out)
            self.assertIn("claude-z -> %s (replaced the copy from the claude-z repo" % os.path.join(REPO, "cl"), out)
            self.assertIn("claude-recent-sessions: it's hatcheck now", out)
            self.assertIn("claude-usage: it's hatcheck usage now", out)
            self.assertFalse(os.path.lexists(os.path.join(target, "claude-usage")), out)
            self.assertTrue(os.path.isfile(os.path.join(target, "claude-where")), out)
            os.remove(os.path.join(target, "claude-where"))
            code, out = self.run_command([os.path.join(target, "hatcheck"), "where", ID[:8]])
            self.assertIn(self.transcript, out)
            code, out = self.run_command([os.path.join(target, "hat"), "--help"])
            self.assertIn("hat is the short name for hatcheck.", out)
            code, out = self.run_command([os.path.join(target, "claude-k"), "--version"])
            self.assertIn("ANTHROPIC_BASE_URL=https://api.kimi.com/coding/", out)
            code, out = self.run_command(["bash", os.path.join(REPO, "install.sh"), "--bin", target, "--uninstall"])
            self.assertEqual(os.listdir(target), [], out)

    def test_installer_leaves_hat_to_another_command(self):
        target = os.path.join(self.h.root, "installed")
        if WINDOWS:
            write(os.path.join(self.bin, "hat.cmd"), "@echo off\r\necho another hat\r\n")  # on the PATH
            install = POWERSHELL + [os.path.join(REPO, "install.ps1"), "-Bin", target]
            force, made = install + ["-Force"], os.path.join(target, "hat.cmd")
        else:
            write(os.path.join(self.bin, "hat"), "#!/bin/sh\necho another hat\n")
            os.chmod(os.path.join(self.bin, "hat"), 0o755)
            install = ["bash", os.path.join(REPO, "install.sh"), "--bin", target]
            force, made = install + ["--force"], os.path.join(target, "hat")
        code, out = self.run_command(install)
        self.assertEqual(code, 0, out)  # only a short name: not having it isn't a failure
        self.assertIn("Didn't add hat: %s has that name" % os.path.join(self.bin, "hat" + (".cmd" if WINDOWS else "")), out)
        self.assertFalse(os.path.lexists(made), out)
        self.assertTrue(os.path.lexists(os.path.join(target, "hatcheck" + (".cmd" if WINDOWS else ""))), out)
        code, out = self.run_command(force)
        self.assertEqual(code, 0, out)
        self.assertTrue(os.path.lexists(made), out)



class PythonLaunchers(Commands):
    """The same, through hatcheck/launch.py: the cl, cx, claude-z and claude-k a package
    install (uv tool install, pipx) puts on the PATH, on every system."""

    def launch(self, name, args, kw):
        return self.run_command([sys.executable, os.path.join(REPO, "hatcheck", "launch.py"), name] + list(args), **kw)

    def cl(self, *args, **kw):
        return self.launch("cl", args, kw)

    def cx(self, *args, **kw):
        return self.launch("cx", args, kw)

    def claude_on(self, name, *args, **kw):
        return self.launch(name, args, kw)

    # About the hatcheck command, the installers or the sh and PowerShell launchers only.
    test_hatcheck = test_hat_is_the_short_name = test_installer = test_installer_leaves_hat_to_another_command = None
    test_where_finds_a_session_by_prefix = test_where_finds_codex_sessions = test_cl_needs_hatcheck_for_a_provider = None


if __name__ == "__main__":
    unittest.main()
