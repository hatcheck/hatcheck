"""cl, cx, ck and claude-z, -k, -m and -d, the hatcheck command and the installers. The launchers are
the binaries built beside hatcheck, as every install has them; AsNames runs them through
hatcheck itself, as a link named cl does. A fake claude stands in for Claude Code, a fake
codex for Codex, and a fake kimi for Kimi Code CLI."""
import os
import subprocess
import unittest

from helpers import ENGINE, REPO, WINDOWS, FakeHome, in_terminal, write

ID = "abcdef12-0000-4000-8000-000000000001"
CODEX_ID = "019f0000-aaaa-7000-8000-000000000001"
POWERSHELL = ["powershell", "-NoLogo", "-NoProfile", "-ExecutionPolicy", "Bypass", "-File"]
SHOWN = ("CLAUDE_CONFIG_DIR", "ANTHROPIC_BASE_URL", "ANTHROPIC_AUTH_TOKEN", "ANTHROPIC_API_KEY", "ANTHROPIC_MODEL",
         "ANTHROPIC_DEFAULT_HAIKU_MODEL")
HATCHECK = [ENGINE]
EXE = ".exe" if WINDOWS else ""


def launcher(name):
    """A launcher's own binary, built beside hatcheck."""
    return os.path.join(os.path.dirname(ENGINE), name + EXE)


class Commands(unittest.TestCase):
    def setUp(self):
        self.h = FakeHome()
        self.addCleanup(self.h.cleanup)
        work = self.h.profile("work")
        self.transcript = self.h.session(work, self.h.folder("code"), ID, "A session")
        os.makedirs(self.transcript[:-len(".jsonl")])  # its data folder
        self.rollout = self.h.codex_session(self.h.codex_home("work"), self.h.folder("code"), CODEX_ID, "A Codex session")
        # A fake claude that prints the profile and provider it would use, and its arguments.
        self.bin = os.path.join(self.h.root, "fakebin")
        self.fake_bin(self.bin)
        self.h.provider_config("z", 'CZ_API_KEY="zai-test-key"\nCZ_MODEL_OPUS="glm-5.3[1m]"\n')

    def fake_bin(self, folder):
        """Fake claude and codex in folder, which print what they were given."""
        if WINDOWS:
            lines = ["@echo off", "echo args=%*"] + [
                "if defined %s (echo %s=%%%s%%) else (echo %s=unset)" % (v, v, v, v) for v in SHOWN]
            write(os.path.join(folder, "claude.cmd"), "\r\n".join(lines) + "\r\n")
            write(os.path.join(folder, "codex.cmd"), "@echo off\r\necho args=%*\r\nif defined CODEX_HOME "
                  "(echo CODEX_HOME=%CODEX_HOME%) else (echo CODEX_HOME=unset)\r\n")
            write(os.path.join(folder, "kimi.cmd"), "@echo off\r\necho args=%*\r\nif defined KIMI_CODE_HOME "
                  "(echo KIMI_CODE_HOME=%KIMI_CODE_HOME%) else (echo KIMI_CODE_HOME=unset)\r\n")
            return
        lines = ["#!/bin/sh", 'echo "args=$*"'] + ['echo "%s=${%s-unset}"' % (v, v) for v in SHOWN]
        write(os.path.join(folder, "claude"), "\n".join(lines) + "\n")
        write(os.path.join(folder, "codex"), '#!/bin/sh\necho "args=$*"\necho "CODEX_HOME=${CODEX_HOME-unset}"\n')
        write(os.path.join(folder, "kimi"), '#!/bin/sh\necho "args=$*"\necho "KIMI_CODE_HOME=${KIMI_CODE_HOME-unset}"\n')
        for name in os.listdir(folder):
            os.chmod(os.path.join(folder, name), 0o755)

    def run_command(self, argv, extra_env=None, stdin="", fakes=True, bindir=None, cwd=None):
        env = dict(self.h.env(), **(extra_env or {}))
        env["PATH"] = ((bindir or self.bin) + os.pathsep if fakes else "") + (os.environ["PATH"] if WINDOWS else "/bin:/usr/bin")
        p = subprocess.run(argv, cwd=cwd or self.h.root, env=env, input=stdin, capture_output=True, text=True,
                           encoding="utf-8")
        return p.returncode, p.stdout + p.stderr

    def cl(self, *args, **kw):
        return self.run_command([launcher("cl")] + list(args), **kw)

    def cx(self, *args, **kw):
        return self.run_command([launcher("cx")] + list(args), **kw)

    def ck(self, *args, **kw):
        return self.run_command([launcher("ck")] + list(args), **kw)

    def claude_on(self, name, *args, **kw):
        """claude-z, claude-k, claude-m or claude-d."""
        return self.run_command([launcher(name)] + list(args), **kw)

    def where(self, *args):
        return self.run_command(HATCHECK + ["where"] + list(args))

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
            self.assertIn("usage: cl [-z|-k|-m|-D] <profile>", out)

    def test_cl_with_a_provider(self):
        self.h.provider_config("k", 'CK_API_KEY="kimi-test-key"\n')
        code, out = self.cl("-k", "work", "--resume", ID, extra_env={"ANTHROPIC_API_KEY": "sk-ant-from-the-shell"})
        self.assertEqual(code, 0, out)
        for line in ("args=--effort max --resume " + ID, "CLAUDE_CONFIG_DIR=" + os.path.join(self.h.home, ".claude-work"),
                     "ANTHROPIC_BASE_URL=https://api.kimi.ai/coding/", "ANTHROPIC_AUTH_TOKEN=kimi-test-key",
                     "ANTHROPIC_API_KEY=unset", "ANTHROPIC_MODEL=k3.1-pro[1m]"):
            self.assertIn(line, out)

    def test_minimax_and_deepseek(self):
        self.h.provider_config("m", 'CM_API_KEY="minimax-test-key"\n')
        self.h.provider_config("d", 'CD_API_KEY="deepseek-test-key"\n')
        code, out = self.cl("-m", "work", "--resume", ID)
        self.assertEqual(code, 0, out)
        for line in ("args=--effort max --resume " + ID, "CLAUDE_CONFIG_DIR=" + os.path.join(self.h.home, ".claude-work"),
                     "ANTHROPIC_BASE_URL=https://api.minimax.io/anthropic", "ANTHROPIC_AUTH_TOKEN=minimax-test-key",
                     "ANTHROPIC_API_KEY=unset", "ANTHROPIC_MODEL=MiniMax-M3.1-Flash[1m]"):
            self.assertIn(line, out)
        code, out = self.claude_on("claude-d", "-p", "hi")
        self.assertEqual(code, 0, out)
        for line in ("args=--effort max -p hi", "CLAUDE_CONFIG_DIR=unset",
                     "ANTHROPIC_BASE_URL=https://api.deepseek.com/anthropic", "ANTHROPIC_AUTH_TOKEN=deepseek-test-key",
                     "ANTHROPIC_MODEL=deepseek-v4-pro[1m]", "ANTHROPIC_DEFAULT_HAIKU_MODEL=deepseek-flash"):
            self.assertIn(line, out)
        code, out = self.cl("-D", "work", "-d")
        self.assertEqual(code, 0, out)
        self.assertIn("ANTHROPIC_BASE_URL=https://api.deepseek.com/anthropic", out)
        self.assertIn("args=--effort max -d", out)
        # -d stays Claude Code's --debug, so a debug run in a bound folder is the profile's own,
        # on its Claude plan, and goes to no other provider.
        api = self.h.folder("code", "api")
        self.run_command(HATCHECK + ["bind", "work"], cwd=api)
        code, out = self.cl("-d", "--resume", ID, cwd=api)
        self.assertEqual(code, 0, out)
        for line in ("args=-d --resume " + ID, "CLAUDE_CONFIG_DIR=" + os.path.join(self.h.home, ".claude-work"),
                     "ANTHROPIC_BASE_URL=unset"):
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
        hatcheck = HATCHECK
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

    def test_ck_runs_kimi_with_the_home(self):
        home = os.path.join(self.h.home, ".kimi-code-fresh")
        code, out = self.ck("fresh", "--session", "session_" + ID)
        self.assertEqual(code, 0, out)
        self.assertIn("args=--session session_" + ID, out)
        self.assertIn("KIMI_CODE_HOME=" + home, out)
        self.assertTrue(os.path.isdir(home))  # made, so ck fresh login starts a new home
        for args in ((), ("--help",)):
            code, out = self.ck(*args)
            self.assertEqual(code, 2, out)
            self.assertIn("usage: ck <profile> [kimi args...]", out)
        # A Kimi command first, with no ~/.kimi-code-login and no binding here: not a home's name.
        code, out = self.ck("login")
        self.assertEqual(code, 2, out)
        self.assertIn("ck: login is a Kimi command, not a home's name", out)
        self.assertFalse(os.path.exists(os.path.join(self.h.home, ".kimi-code-login")))
        # Bound to a home: the name can be left out, and a Kimi command comes first.
        self.h.kimi_home("work")
        code_dir = self.h.folder("code")
        code, out = self.run_command(HATCHECK + ["bind", "work"], cwd=code_dir)
        self.assertEqual(code, 0, out)
        self.assertIn("Kimi Code CLI " + self.h.shown(os.path.join(self.h.home, ".kimi-code-work")), out)
        code, out = self.ck("login", cwd=code_dir)
        self.assertEqual(code, 0, out)
        self.assertIn("ck: this folder is bound to work", out)
        self.assertIn("args=login", out)
        self.assertIn("KIMI_CODE_HOME=" + os.path.join(self.h.home, ".kimi-code-work"), out)

    @unittest.skipIf(WINDOWS, "the runner's PATH could have a kimi on it")
    def test_ck_without_kimi(self):
        code, out = self.ck("work", fakes=False)
        self.assertEqual(code, 1, out)
        self.assertIn("kimi isn't on your PATH", out)

    @unittest.skipIf(WINDOWS, "the runner's PATH could have a codex on it")
    def test_cx_without_codex(self):
        code, out = self.cx("work", fakes=False)
        self.assertEqual(code, 1, out)
        self.assertIn("codex isn't on your PATH", out)

    def test_claude_z_and_k_keep_the_config_dir(self):
        code, out = self.claude_on("claude-z", "--resume", ID)
        self.assertEqual(code, 0, out)
        for line in ("args=--effort max --resume " + ID, "CLAUDE_CONFIG_DIR=unset", "ANTHROPIC_MODEL=glm-5.3[1m]",
                     "ANTHROPIC_BASE_URL=https://api.z.ai/api/anthropic", "ANTHROPIC_AUTH_TOKEN=zai-test-key"):
            self.assertIn(line, out)
        mine = os.path.join(self.h.home, ".claude-work")
        code, out = self.claude_on("claude-z", "-p", "hi", extra_env={"CLAUDE_CONFIG_DIR": mine})
        self.assertIn("CLAUDE_CONFIG_DIR=" + mine, out)

    def test_provider_not_set_up(self):
        # Off a terminal there's no one to ask, so it says how to set it up.
        code, out = self.claude_on("claude-k")
        self.assertEqual(code, 1, out)
        self.assertIn("Kimi isn't set up yet. Run: claude-k reconfig", out)
        self.assertNotIn("args=", out)  # claude didn't run

    def test_effort(self):
        # max unless the config names another, auto leaves it to Claude Code, and one typed wins.
        self.h.provider_config("k", 'CK_API_KEY="kimi-test-key"\n')
        for args, shown in ((["-p", "hi"], "args=--effort max -p hi"),
                            (["--effort", "low", "-p", "hi"], "args=--effort low -p hi"),
                            (["--effort=high", "-p", "hi"], "args=--effort=high -p hi"),
                            (["mcp", "list"], "args=--effort max mcp list")):  # before Claude Code's commands
            code, out = self.claude_on("claude-k", *args)
            self.assertEqual(code, 0, out)
            self.assertIn(shown + "\n", out)
        for saved, shown in (("high", "args=--effort high -p hi"), ("auto", "args=-p hi")):
            self.h.provider_config("k", 'CK_API_KEY="kimi-test-key"\nCK_EFFORT="%s"\n' % saved)
            code, out = self.cl("-k", "work", "-p", "hi")
            self.assertIn(shown + "\n", out)

    def test_models_no_longer_offered_run_as_saved_off_a_terminal(self):
        self.h.provider_config("z", 'CZ_API_KEY="zai-test-key"\nCZ_MODEL_OPUS="glm-5.3[1m]"\nCZ_MODEL_HAIKU="glm-4.7"\n')
        code, out = self.claude_on("claude-z", "-p", "hi")
        self.assertEqual(code, 0, out)
        self.assertIn("claude-z: glm-5.3[1m] (haiku glm-4.7) isn't among the models offered now; "
                      "claude-z reconfig picks one", out)
        self.assertIn("ANTHROPIC_DEFAULT_HAIKU_MODEL=glm-4.7", out)

    @unittest.skipIf(WINDOWS, "a pseudo-terminal")
    def test_set_up_on_a_terminal_the_first_time_and_when_models_are_no_longer_offered(self):
        env = dict(self.h.env(), PATH=self.bin + os.pathsep + "/bin:/usr/bin")
        # The first time: the key, then Enter for the flagship, max effort and Claude Code's limit.
        code, out = in_terminal([launcher("claude-d"), "-p", "hi"],
                                [(b"API key: ", b"deepseek-typed-key\n"), (b"Enter for 1, deepseek-v4-pro[1m]: ", b"\n"),
                                 (b"(Enter keeps max): ", b"\n"), (b"Max output tokens", b"\n")],
                                env, self.h.root)
        self.assertEqual(code, 0, out)
        self.assertIn("Claude Code on DeepSeek isn't set up yet: the key and model first.", out)
        self.assertNotIn("deepseek-typed-key", out.split("args=")[0])  # not echoed
        self.assertIn("Saved. Starting Claude Code on DeepSeek; claude-d reconfig changes these.", out)
        for line in ("args=--effort max -p hi", "ANTHROPIC_AUTH_TOKEN=deepseek-typed-key",
                     "ANTHROPIC_MODEL=deepseek-v4-pro[1m]", "ANTHROPIC_DEFAULT_HAIKU_MODEL=deepseek-flash"):
            self.assertIn(line, out)
        # Models no longer offered: the setup again, Enter keeping the key and taking the flagship.
        path = self.h.provider_config("z", 'CZ_API_KEY="zai-test-key"\nCZ_MODEL_OPUS="glm-5.3[1m]"\n'
                                           'CZ_MODEL_SONNET="glm-5.3-flash[1m]"\nCZ_MODEL_HAIKU="glm-4.7"\n')
        code, out = in_terminal([launcher("cl"), "-z", "work", "--resume", ID],
                                [(b"(Enter keeps \xe2\x80\xa6-key): ", b"\n"), (b"Enter for 1, glm-5.3[1m]: ", b"\n"),
                                 (b"(Enter keeps max): ", b"\n"), (b"Max output tokens", b"\n")],
                                env, self.h.root)
        self.assertEqual(code, 0, out)
        self.assertIn("Claude Code on Z.ai: glm-5.3[1m] (sonnet glm-5.3-flash[1m], haiku glm-4.7), saved in "
                      "~/.config/claude-z/config, isn't among the models offered now. Choose again", out)
        for line in ("args=--effort max --resume " + ID, "CLAUDE_CONFIG_DIR=" + os.path.join(self.h.home, ".claude-work"),
                     "ANTHROPIC_AUTH_TOKEN=zai-test-key", "ANTHROPIC_MODEL=glm-5.3[1m]",
                     "ANTHROPIC_DEFAULT_HAIKU_MODEL=glm-5.3-flash[1m]"):
            self.assertIn(line, out)
        with open(path, encoding="utf-8") as fh:
            self.assertIn('CZ_MODEL_HAIKU="glm-5.3-flash[1m]"\nCZ_EFFORT="max"\n', fh.read())
        # Set up and current: no questions.
        code, out = in_terminal([launcher("claude-z"), "-p", "hi"], [], env, self.h.root)
        self.assertEqual(code, 0, out)
        self.assertTrue(out.startswith("args=--effort max -p hi"), out)

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

    def test_hatcheck(self):
        code, out = self.run_command(HATCHECK + ["--help"])
        self.assertEqual(code, 0, out)
        for line in ("hatcheck [sessions] [HOURS]", "hatcheck usage", "hatcheck where SESSION",
                     "hatcheck move [SESSION] [PROFILE]", "hatcheck fork [SESSION] [PROFILE]", "hatcheck open [SESSION]",
                     "hatcheck best [--for SESSION]",
                     "hatcheck bind PROFILE", "hatcheck unbind [FOLDER]", "hatcheck provider", "take --json",
                     "hatcheck show SESSION", "hatcheck desktop"):
            self.assertIn(line, out)
        code, out = self.run_command(HATCHECK + ["3d", "--no-usage", "--all-folders"])
        self.assertEqual(code, 0, out)  # HOURS on its own: the session list
        self.assertIn("cl work --resume " + ID, out)
        code, out = self.run_command(HATCHECK + ["nope"])
        self.assertEqual(code, 2, out)
        self.assertIn("hatcheck: there's no 'nope' subcommand; see hatcheck --help", out)

    def test_hat_is_the_short_name(self):
        hat = HATCHECK + ["--as", "hat"]  # as hat.cmd runs it
        code, out = self.run_command(hat + ["--help"])
        self.assertEqual(code, 0, out)
        self.assertIn("hat is the short name for hatcheck.", out)
        code, out = self.run_command(hat + ["usage", "--help"])
        self.assertIn("usage: hat usage", out)
        code, out = self.run_command(hat + ["fork", "--help"])
        self.assertIn("usage: hat fork", out)
        # Cut to its first letters: the one it starts, or of two, the one used more.
        for short, name in (("m", "move"), ("f", "fork"), ("b", "best"), ("bi", "bind"), ("u", "usage"),
                            ("un", "unbind"), ("s", "show"), ("se", "search"), ("r", "resume"), ("rel", "relocate")):
            code, out = self.run_command(hat + [short, "--help"])
            self.assertEqual(code, 0, out)
            self.assertIn("usage: hat %s " % name, out)
        code, out = self.run_command(HATCHECK + ["w", ID[:8]])  # hatcheck where, as w
        self.assertEqual(code, 0, out)
        self.assertEqual(sorted(out.split()), sorted([self.transcript, self.transcript[:-len(".jsonl")]]))
        code, out = self.run_command(hat + ["nope"])
        self.assertIn("hat: there's no 'nope' subcommand; see hat --help", out)
        code, out = self.run_command(hat + ["check", "3d", "--no-usage", "--all-folders"])
        self.assertEqual(code, 0, out)  # hat check: hat again, so the session list
        self.assertIn("cl work --resume " + ID, out)
        code, out = self.run_command(hat + ["check", "usage", "--help"])
        self.assertIn("usage: hat usage", out)
        code, out = self.run_command(hat + ["check", "nope"])
        self.assertIn("hat: there's no 'nope' subcommand; see hat --help", out)
        if not WINDOWS:  # a link named hat, as install.sh makes
            os.symlink(HATCHECK[-1], os.path.join(self.bin, "hat"))
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
        names = ["hatcheck", "hat", "cl", "cx", "ck", "claude-z", "claude-k", "claude-m", "claude-d"]
        built = os.path.realpath(os.path.dirname(ENGINE))  # as the installers spell it
        self.h.provider_config("k", 'CK_API_KEY="kimi-test-key"\n')
        if WINDOWS:
            # Launchers for the commands hatcheck replaced, made before the repo was renamed from
            # claude-multi-account: removed. One this script didn't make is left alone. One it
            # made for the Python hatcheck is brought up to date.
            old = "@echo off\r\nrem made by claude-multi-account install.ps1\r\npython old\\claude-usage.py %*\r\n"
            write(os.path.join(target, "claude-usage.cmd"), old)
            write(os.path.join(target, "claude-where.cmd"), "@echo off\r\necho mine\r\n")
            write(os.path.join(target, "hatcheck.cmd"),
                  "@echo off\r\nrem made by hatcheck install.ps1\r\npython \"%s\\hatcheck.py\" %%*\r\n" % REPO)
            code, out = self.run_command(POWERSHELL + [os.path.join(REPO, "install.ps1"), "-Bin", target, "-From", built])
            self.assertEqual(code, 0, out)
            for name in names:
                with open(os.path.join(target, name + ".cmd"), encoding="ascii") as fh:
                    self.assertIn('"%s" %%*' % os.path.join(built, name + ".exe"), fh.read(), out)
            self.assertFalse(os.path.exists(os.path.join(target, "claude-usage.cmd")), out)
            self.assertIn("it's hatcheck usage now", out)
            self.assertTrue(os.path.isfile(os.path.join(target, "claude-where.cmd")), out)
            os.remove(os.path.join(target, "claude-where.cmd"))
            code, out = self.run_command(["cmd", "/c", os.path.join(target, "hatcheck.cmd"), "where", ID[:8]])
            self.assertIn(self.transcript, out)
            code, out = self.run_command(["cmd", "/c", os.path.join(target, "hatcheck.cmd"), "--all-folders", "--no-usage"])
            self.assertIn("cl work --resume " + ID, out)
            code, out = self.run_command(["cmd", "/c", os.path.join(target, "hat.cmd"), "--help"])
            self.assertIn("hat is the short name for hatcheck.", out)
            code, out = self.run_command(["cmd", "/c", os.path.join(target, "claude-k.cmd"), "--version"])
            self.assertIn("ANTHROPIC_BASE_URL=https://api.kimi.ai/coding/", out)
            self.assertIn("args=--version", out)
            write(os.path.join(target, "claude-where.cmd"), old)  # removed on uninstall too
            code, out = self.run_command(POWERSHELL + [os.path.join(REPO, "install.ps1"), "-Bin", target, "-Uninstall"])
            self.assertEqual(os.listdir(target), [], out)
        else:
            # A copy that the claude-z repo's install-z.sh left: replaced without --force.
            write(os.path.join(target, "claude-z"), '#!/usr/bin/env bash\nCONFIG_DIR="$HOME/.config/claude-z"\n')
            # Links to the commands hatcheck replaced: removed. A file this script didn't make stays.
            # Its own link to the Python hatcheck: replaced without --force.
            repo = os.path.realpath(REPO)  # as install.sh spells it
            os.symlink(os.path.join(repo, "claude-recent-sessions.py"), os.path.join(target, "claude-recent-sessions"))
            os.symlink(os.path.join(repo, "claude-usage.py"), os.path.join(target, "claude-usage"))
            os.symlink(os.path.join(repo, "hatcheck.py"), os.path.join(target, "hatcheck"))
            write(os.path.join(target, "claude-where"), "#!/bin/sh\necho mine\n")
            code, out = self.run_command(["bash", os.path.join(REPO, "install.sh"), "--bin", target, "--from", built])
            self.assertEqual(code, 0, out)
            for name in names:
                self.assertEqual(os.readlink(os.path.join(target, name)), os.path.join(built, name), out)
            self.assertIn("claude-z -> %s (replaced the copy from the claude-z repo" % os.path.join(built, "claude-z"), out)
            self.assertIn("hatcheck -> %s (it linked to %s)" % (os.path.join(built, "hatcheck"),
                                                                os.path.join(repo, "hatcheck.py")), out)
            self.assertIn("claude-recent-sessions: it's hatcheck now", out)
            self.assertIn("claude-usage: it's hatcheck usage now", out)
            self.assertFalse(os.path.lexists(os.path.join(target, "claude-usage")), out)
            self.assertTrue(os.path.isfile(os.path.join(target, "claude-where")), out)
            os.remove(os.path.join(target, "claude-where"))
            code, out = self.run_command(["bash", os.path.join(REPO, "install.sh"), "--bin", target, "--from", built])
            self.assertEqual(out.count("(already linked)"), len(names), out)
            code, out = self.run_command([os.path.join(target, "hatcheck"), "where", ID[:8]])
            self.assertIn(self.transcript, out)
            code, out = self.run_command([os.path.join(target, "hat"), "--help"])
            self.assertIn("hat is the short name for hatcheck.", out)
            code, out = self.run_command([os.path.join(target, "claude-k"), "--version"])
            self.assertIn("ANTHROPIC_BASE_URL=https://api.kimi.ai/coding/", out)
            code, out = self.run_command(["bash", os.path.join(REPO, "install.sh"), "--bin", target, "--uninstall"])
            self.assertEqual(os.listdir(target), [], out)
            code, out = self.run_command(["bash", os.path.join(REPO, "install.sh"), "--bin", target])
            self.assertEqual(code, 1, out)  # the runner's PATH here has no cargo to build with
            self.assertIn("builds hatcheck with cargo, which isn't on your PATH", out)

    def test_installer_leaves_hat_to_another_command(self):
        target = os.path.join(self.h.root, "installed")
        built = os.path.dirname(ENGINE)
        if WINDOWS:
            write(os.path.join(self.bin, "hat.cmd"), "@echo off\r\necho another hat\r\n")  # on the PATH
            install = POWERSHELL + [os.path.join(REPO, "install.ps1"), "-Bin", target, "-From", built]
            force, made = install + ["-Force"], os.path.join(target, "hat.cmd")
        else:
            write(os.path.join(self.bin, "hat"), "#!/bin/sh\necho another hat\n")
            os.chmod(os.path.join(self.bin, "hat"), 0o755)
            install = ["bash", os.path.join(REPO, "install.sh"), "--bin", target, "--from", built]
            force, made = install + ["--force"], os.path.join(target, "hat")
        code, out = self.run_command(install)
        self.assertEqual(code, 0, out)  # only a short name: not having it isn't a failure
        self.assertIn("Didn't add hat: %s has that name" % os.path.join(self.bin, "hat" + (".cmd" if WINDOWS else "")), out)
        self.assertFalse(os.path.lexists(made), out)
        self.assertTrue(os.path.lexists(os.path.join(target, "hatcheck" + (".cmd" if WINDOWS else ""))), out)
        code, out = self.run_command(force)
        self.assertEqual(code, 0, out)
        self.assertTrue(os.path.lexists(made), out)


class AsNames(Commands):
    """The same, through the hatcheck binary itself, called as cl, cx, claude-z or claude-k
    (with --as, which is what a link of that name to it does)."""

    def launch(self, name, args, kw):
        return self.run_command([ENGINE, "--as", name] + list(args), **kw)

    def cl(self, *args, **kw):
        return self.launch("cl", args, kw)

    def cx(self, *args, **kw):
        return self.launch("cx", args, kw)

    def ck(self, *args, **kw):
        return self.launch("ck", args, kw)

    def claude_on(self, name, *args, **kw):
        return self.launch(name, args, kw)

    # About the hatcheck command or the installers only.
    test_hatcheck = test_hat_is_the_short_name = test_installer = test_installer_leaves_hat_to_another_command = None
    test_where_finds_a_session_by_prefix = test_where_finds_codex_sessions = None

    @unittest.skipIf(WINDOWS, "a symbolic link")
    def test_by_a_link(self):
        """A link to hatcheck named cl, or claude-k, is that launcher."""
        self.h.provider_config("k", 'CK_API_KEY="kimi-test-key"\n')
        links = os.path.join(self.h.root, "links")
        os.makedirs(links)
        for name in ("cl", "ck", "claude-k"):
            os.symlink(ENGINE, os.path.join(links, name))
        code, out = self.run_command([os.path.join(links, "cl"), "work", "--version"])
        self.assertEqual(code, 0, out)
        self.assertIn("args=--version", out)
        self.assertIn("CLAUDE_CONFIG_DIR=" + os.path.join(self.h.home, ".claude-work"), out)
        code, out = self.run_command([os.path.join(links, "claude-k"), "--version"])
        self.assertEqual(code, 0, out)
        self.assertIn("ANTHROPIC_BASE_URL=https://api.kimi.ai/coding/", out)
        code, out = self.run_command([os.path.join(links, "ck"), "work", "--version"])
        self.assertEqual(code, 0, out)
        self.assertIn("KIMI_CODE_HOME=" + os.path.join(self.h.home, ".kimi-code-work"), out)


if __name__ == "__main__":
    unittest.main()
