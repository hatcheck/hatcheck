"""hatcheck provider: the settings behind claude-z, claude-k, cl -z and cl -k."""
import json
import os
import stat
import subprocess
import unittest

from helpers import WINDOWS, FakeHome, argv_of

SCRIPT = "provider"
# A Z.ai config as the claude-z repo's installer writes it, with models edited by hand.
ZAI = ('CZ_API_KEY="zai-test-key-1234"\nCZ_PERMISSION_MODE="acceptEdits"\nCZ_MODEL_OPUS="glm-5.3[1m]"\n'
       'CZ_MODEL_SONNET="glm-5.3-flash[1m]"\nCZ_MODEL_HAIKU="glm-4.7"\nCZ_MAX_TOKENS=""\n')


class Provider(unittest.TestCase):
    def setUp(self):
        self.h = FakeHome()
        self.addCleanup(self.h.cleanup)

    def env(self, provider):
        code, out = self.h.run(SCRIPT, "env", provider, "--json")
        self.assertEqual(code, 0, out)
        return json.loads(out)

    def setup_with(self, provider, answers):
        p = subprocess.run(argv_of(SCRIPT) + ["setup", provider], input=answers,
                           cwd=self.h.root, env=self.h.env(), capture_output=True, text=True, encoding="utf-8")
        return p.returncode, p.stdout + p.stderr

    def test_zai(self):
        self.h.provider_config("z", ZAI)
        env = self.env("z")
        self.assertEqual(env["ANTHROPIC_BASE_URL"], "https://api.z.ai/api/anthropic")
        self.assertEqual(env["ANTHROPIC_AUTH_TOKEN"], "zai-test-key-1234")
        # Unset, so no Anthropic credential goes to Z.ai and the claude.ai login can't take over.
        self.assertIsNone(env["ANTHROPIC_API_KEY"])
        self.assertIsNone(env["CLAUDE_CODE_OAUTH_TOKEN"])
        # Pinned, so a "model" in the profile's settings.json can't send a Claude model name.
        self.assertEqual(env["ANTHROPIC_MODEL"], "glm-5.3[1m]")
        self.assertEqual(env["ANTHROPIC_DEFAULT_FABLE_MODEL"], "glm-5.3[1m]")
        self.assertEqual(env["ANTHROPIC_DEFAULT_SONNET_MODEL"], "glm-5.3-flash[1m]")
        self.assertEqual(env["ANTHROPIC_DEFAULT_HAIKU_MODEL"], "glm-4.7")
        self.assertEqual(env["CLAUDE_CODE_AUTO_COMPACT_WINDOW"], "1000000")
        self.assertEqual(env["CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC"], "1")
        self.assertNotIn("CLAUDE_CODE_MAX_OUTPUT_TOKENS", env)

    def test_kimi(self):
        self.h.provider_config("k", 'CK_API_KEY="kimi-test-key"\nCK_MODEL=""\n')
        env = self.env("k")
        self.assertEqual(env["ANTHROPIC_BASE_URL"], "https://api.kimi.ai/coding/")
        self.assertEqual(env["ANTHROPIC_AUTH_TOKEN"], "kimi-test-key")  # never ANTHROPIC_API_KEY
        self.assertIsNone(env["ANTHROPIC_API_KEY"])
        for name in ("ANTHROPIC_MODEL", "ANTHROPIC_DEFAULT_FABLE_MODEL", "ANTHROPIC_DEFAULT_HAIKU_MODEL",
                     "CLAUDE_CODE_SUBAGENT_MODEL"):
            self.assertEqual(env[name], "k3.1-pro[1m]")
        self.assertEqual(env["CLAUDE_CODE_MAX_CONTEXT_TOKENS"], "1048576")
        self.h.provider_config("k", 'CK_API_KEY="kimi-test-key"\nCK_MODEL="k3"\nCK_MAX_TOKENS="32000"\n'
                                    'CK_BASE_URL="https://api.moonshot.ai/anthropic"\n')
        env = self.env("k")
        self.assertEqual(env["CLAUDE_CODE_AUTO_COMPACT_WINDOW"], "262144")
        self.assertEqual(env["CLAUDE_CODE_MAX_OUTPUT_TOKENS"], "32000")
        self.assertEqual(env["ANTHROPIC_BASE_URL"], "https://api.moonshot.ai/anthropic")

    def test_powershell_config_with_a_bom(self):
        path = self.h.provider_config("z", "")
        with open(path, "w", encoding="utf-8-sig") as fh:  # what Set-Content -Encoding UTF8 writes
            fh.write(ZAI.replace("\n", "\r\n"))
        self.assertEqual(self.env("z")["ANTHROPIC_AUTH_TOKEN"], "zai-test-key-1234")

    def test_not_set_up(self):
        code, out = self.h.run(SCRIPT, "env", "z")
        self.assertEqual(code, 1)
        self.assertIn("Z.ai isn't set up yet. Run: claude-z reconfig", out)
        self.h.provider_config("k", 'CK_API_KEY=""\n')
        code, out = self.h.run(SCRIPT, "env", "k")
        self.assertEqual(code, 1)
        self.assertIn("No Kimi API key in", out)

    @unittest.skipIf(WINDOWS, "needs a POSIX shell's eval")
    def test_shell_lines_survive_eval(self):
        key = "it's a $key `with` \"quotes\""  # only by editing the file by hand
        self.h.provider_config("k", "CK_API_KEY=%s\n" % key)
        script = 'eval "$("$@" env k)" && printf "%s|%s" "$ANTHROPIC_AUTH_TOKEN" "${ANTHROPIC_API_KEY-unset}"'
        env = dict(self.h.env(), ANTHROPIC_API_KEY="sk-ant-from-the-shell")
        p = subprocess.run(["sh", "-c", script, "sh"] + argv_of(SCRIPT), env=env,
                           capture_output=True, text=True)
        self.assertEqual(p.stdout, key + "|unset", p.stderr)

    def test_setup_keeps_other_lines(self):
        path = self.h.provider_config("z", "# mine\n" + ZAI)
        code, out = self.setup_with("z", "new-zai-key\n2\n64000\n")
        self.assertEqual(code, 0, out)
        self.assertIn("Enter keeps …1234", out)
        self.assertIn("Enter to keep glm-5.3[1m] (sonnet glm-5.3-flash[1m], haiku glm-4.7)", out)
        with open(path, encoding="utf-8") as fh:
            text = fh.read()
        self.assertEqual(text, '# mine\nCZ_API_KEY="new-zai-key"\nCZ_PERMISSION_MODE="acceptEdits"\n'
                               'CZ_MODEL_OPUS="glm-5.3-flash[1m]"\nCZ_MODEL_SONNET="glm-5.3-flash[1m]"\n'
                               'CZ_MODEL_HAIKU="glm-5.3-flash[1m]"\nCZ_MAX_TOKENS="64000"\n')
        if not WINDOWS:
            self.assertEqual(stat.S_IMODE(os.stat(path).st_mode), 0o600)

    def test_setup_enter_keeps_everything(self):
        path = self.h.provider_config("z", ZAI)
        code, out = self.setup_with("z", "\n\n\n")
        self.assertEqual(code, 0, out)
        with open(path, encoding="utf-8") as fh:
            self.assertEqual(fh.read(), ZAI)

    def test_setup_asks_again_after_a_bad_answer(self):
        path = self.h.provider_config("k", 'CK_API_KEY="old"\nCK_MAX_TOKENS="1000"\n')
        code, out = self.setup_with("k", "has space\nkimi-new\n9\n6\nlots\n-\n")
        self.assertEqual(code, 0, out)
        self.assertIn("paste just the key", out)
        self.assertIn("Type a number from the list", out)
        with open(path, encoding="utf-8") as fh:
            self.assertEqual(fh.read(), 'CK_API_KEY="kimi-new"\nCK_MAX_TOKENS=""\nCK_MODEL="kimi-for-coding"\n')

    def test_setup_needs_a_key(self):
        code, out = self.setup_with("k", "")
        self.assertEqual(code, 1)
        self.assertIn("Nothing saved: an API key is needed", out)

    def test_status(self):
        self.h.provider_config("z", ZAI)
        code, out = self.h.run(SCRIPT)
        self.assertEqual(code, 0, out)
        self.assertIn("Z.ai (claude-z, or cl -z <profile>):", out)
        self.assertIn("key …1234 · glm-5.3[1m] (sonnet glm-5.3-flash[1m], haiku glm-4.7)", out)
        self.assertNotIn("zai-test-key", out)
        self.assertIn("Kimi (claude-k, or cl -k <profile>): not set up; run claude-k reconfig", out)


if __name__ == "__main__":
    unittest.main()
