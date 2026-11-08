"""hatcheck provider: the settings behind claude-z, -k, -m and -d, and cl -z, -k, -m and -D."""
import json
import os
import stat
import subprocess
import unittest

from helpers import WINDOWS, FakeHome, argv_of

SCRIPT = "provider"
# A Z.ai config as the claude-z repo's installer writes it, with models edited by hand: glm-4.7,
# which Z.ai serves with GLM-5.3-Flash now, isn't among the models claude-z offers.
ZAI = ('CZ_API_KEY="zai-test-key-1234"\nCZ_PERMISSION_MODE="acceptEdits"\nCZ_MODEL_OPUS="glm-5.3[1m]"\n'
       'CZ_MODEL_SONNET="glm-5.3-flash[1m]"\nCZ_MODEL_HAIKU="glm-4.7"\nCZ_MAX_TOKENS=""\n')
# One with the flagship as Z.ai's guide pairs it, as setup writes it.
ZAI_NOW = ('CZ_API_KEY="zai-test-key-1234"\nCZ_MODEL_OPUS="glm-5.3[1m]"\nCZ_MODEL_SONNET="glm-5.3[1m]"\n'
           'CZ_MODEL_HAIKU="glm-5.3-flash[1m]"\nCZ_EFFORT="max"\nCZ_MAX_TOKENS=""\n')


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
        # The launcher's own cap on a reply, GLM's 128K ceiling, where the config saves none:
        # Claude Code's default of 32,000, which GLM's thinking counts against, cut work short.
        self.assertEqual(env["CLAUDE_CODE_MAX_OUTPUT_TOKENS"], "128000")

    def test_kimi(self):
        self.h.provider_config("k", 'CK_API_KEY="kimi-test-key"\nCK_MODEL=""\n')
        env = self.env("k")
        self.assertEqual(env["ANTHROPIC_BASE_URL"], "https://api.kimi.ai/coding/")
        self.assertEqual(env["ANTHROPIC_AUTH_TOKEN"], "kimi-test-key")  # never ANTHROPIC_API_KEY
        self.assertIsNone(env["ANTHROPIC_API_KEY"])
        for name in ("ANTHROPIC_MODEL", "ANTHROPIC_DEFAULT_FABLE_MODEL", "ANTHROPIC_DEFAULT_HAIKU_MODEL",
                     "CLAUDE_CODE_SUBAGENT_MODEL"):
            self.assertEqual(env[name], "k3.1-pro[1m]")
        # Claude Code 2.1.319 assumes 200k for a model it doesn't know unless this names the window,
        # which doesn't cap a [1m] model's 1M; and it caps CLAUDE_CODE_AUTO_COMPACT_WINDOW at 1000000
        # with a warning, where Kimi's guide says 1048576, so that one isn't set.
        self.assertEqual(env["CLAUDE_CODE_MAX_CONTEXT_TOKENS"], "262144")
        self.assertNotIn("CLAUDE_CODE_AUTO_COMPACT_WINDOW", env)
        # No cap of the launcher's own on Kimi, which gives no figure: Claude Code's default.
        self.assertNotIn("CLAUDE_CODE_MAX_OUTPUT_TOKENS", env)
        self.h.provider_config("k", 'CK_API_KEY="kimi-test-key"\nCK_MODEL="k3-256k"\nCK_MAX_TOKENS="32000"\n'
                                    'CK_BASE_URL="https://api.moonshot.ai/anthropic"\n')
        env = self.env("k")
        self.assertEqual(env["CLAUDE_CODE_MAX_CONTEXT_TOKENS"], "262144")
        self.assertNotIn("CLAUDE_CODE_AUTO_COMPACT_WINDOW", env)
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
        code, out = self.setup_with("z", "new-zai-key\n2\nhigh\n64000\n")
        self.assertEqual(code, 0, out)
        self.assertIn("Enter keeps …1234", out)
        self.assertIn("Choose 1-2, or Enter for 1, glm-5.3[1m]: ", out)
        self.assertIn("(Enter keeps max)", out)
        with open(path, encoding="utf-8") as fh:
            text = fh.read()
        self.assertEqual(text, '# mine\nCZ_API_KEY="new-zai-key"\nCZ_PERMISSION_MODE="acceptEdits"\n'
                               'CZ_MODEL_OPUS="glm-5.3-flash[1m]"\nCZ_MODEL_SONNET="glm-5.3-flash[1m]"\n'
                               'CZ_MODEL_HAIKU="glm-5.3-flash[1m]"\nCZ_MAX_TOKENS="64000"\nCZ_EFFORT="high"\n')
        if not WINDOWS:
            self.assertEqual(stat.S_IMODE(os.stat(path).st_mode), 0o600)

    def test_setup_enter_keeps_everything(self):
        path = self.h.provider_config("z", ZAI_NOW)
        code, out = self.setup_with("z", "\n\n\n\n")
        self.assertEqual(code, 0, out)
        self.assertIn("Enter to keep glm-5.3[1m] (haiku glm-5.3-flash[1m])", out)
        with open(path, encoding="utf-8") as fh:
            self.assertEqual(fh.read(), ZAI_NOW)

    def test_enter_replaces_models_no_longer_offered_with_the_flagship(self):
        path = self.h.provider_config("z", ZAI)
        code, out = self.setup_with("z", "\n\n\n\n")
        self.assertEqual(code, 0, out)
        self.assertNotIn("Enter to keep", out)
        with open(path, encoding="utf-8") as fh:
            self.assertEqual(fh.read(), 'CZ_API_KEY="zai-test-key-1234"\nCZ_PERMISSION_MODE="acceptEdits"\n'
                                        'CZ_MODEL_OPUS="glm-5.3[1m]"\nCZ_MODEL_SONNET="glm-5.3[1m]"\n'
                                        'CZ_MODEL_HAIKU="glm-5.3-flash[1m]"\nCZ_MAX_TOKENS=""\nCZ_EFFORT="max"\n')

    def test_setup_asks_again_after_a_bad_answer(self):
        path = self.h.provider_config("k", 'CK_API_KEY="old"\nCK_MAX_TOKENS="1000"\n')
        code, out = self.setup_with("k", "has space\nkimi-new\n4\n\n9\n5\nmaximal\nAuto\nlots\n-\n")
        self.assertEqual(code, 0, out)
        self.assertIn("paste just the key", out)
        self.assertEqual(out.count("Type a number from the list"), 2)  # the plan, then the model
        self.assertIn("Type one of low, medium, high, xhigh, max or auto", out)
        with open(path, encoding="utf-8") as fh:
            self.assertEqual(fh.read(), 'CK_API_KEY="kimi-new"\nCK_MAX_TOKENS=""\nCK_PLAN="pro"\n'
                                        'CK_MODEL="kimi-for-coding[1m]"\nCK_EFFORT="auto"\n')

    def test_kimi_1m_models_keep_their_window_and_256k_ones_get_theirs(self):
        # Kimi's Claude Code guide, 2026-11-07: K3.1 Pro Highspeed has 1M on every plan that has
        # it, and K2.8 Preview on every plan; the [1m] in the name gives Claude Code the window.
        for choice, model in (("2", "k3.1-pro-highspeed[1m]"), ("5", "kimi-for-coding[1m]")):
            path = self.h.provider_config("k", 'CK_API_KEY="kimi-test-key"\n')
            code, out = self.setup_with("k", "\n\n%s\n\n\n" % choice)
            self.assertEqual(code, 0, out)
            with open(path, encoding="utf-8") as fh:
                self.assertIn('CK_MODEL="%s"' % model, fh.read())
            env = self.env("k")
            self.assertEqual(env["ANTHROPIC_MODEL"], model)
            self.assertEqual(env["CLAUDE_CODE_MAX_CONTEXT_TOKENS"], "262144")
            self.assertNotIn("CLAUDE_CODE_AUTO_COMPACT_WINDOW", env)
        # One saved without [1m] from an earlier menu is used as it is, read as a Plus plan's,
        # and said to be off the menu, so a launcher on a terminal asks again.
        self.h.provider_config("k", 'CK_API_KEY="kimi-test-key"\nCK_MODEL="k3.1-pro-highspeed"\n')
        env = self.env("k")
        self.assertEqual(env["ANTHROPIC_MODEL"], "k3.1-pro-highspeed")
        self.assertEqual(env["CLAUDE_CODE_MAX_CONTEXT_TOKENS"], "262144")
        code, out = self.h.run(SCRIPT)
        self.assertIn("key …-key · k3.1-pro-highspeed (not offered now) · effort max\n"
                      "  /model offers K3.1 Pro, K3, K3 256K and K2.8 Preview (plus plan)", out)

    def test_kimi_offers_each_plans_models(self):
        # Kimi's model list and its guide's table by plan, 2026-11-07: every model on Pro or
        # Allegretto and up, four with 256k on Plus or Moderato, and K2.8 Preview alone on the
        # legacy Andante plan; each once, so no model is listed with and without [1m].
        path = self.h.provider_config("k", 'CK_API_KEY="kimi-test-key"\n')
        code, out = self.setup_with("k", "\n\n4\n\n\n")
        self.assertEqual(code, 0, out)
        self.assertIn("Plan:\n  1) pro        Pro or Allegretto and up: 1M context, and the Highspeed models\n"
                      "  2) plus       Plus or Moderato: 256k context, 1M for K2.8 Preview\n"
                      "  3) andante    Andante (legacy): K2.8 Preview alone\n", out)
        self.assertIn("Choose 1-3, or Enter for 1, pro: ", out)
        self.assertIn("  1) k3.1-pro[1m]               K3.1 Pro: the flagship, 1M context\n"
                      "  2) k3.1-pro-highspeed[1m]     K3.1 Pro Highspeed: 2-3x faster at twice the quota, 1M context\n"
                      "  3) k3[1m]                     K3: the generation before, 1M context\n"
                      "  4) k3-256k                    K3 256K: K3 at 256k, for half k3's quota\n"
                      "  5) kimi-for-coding[1m]        K2.8 Preview: 1M context on every plan\n"
                      "  6) kimi-for-coding-highspeed  K2.7 Code Highspeed: 5-6x faster at 3x the quota, 256k, no effort setting\n",
                      out)
        self.assertIn("Choose 1-6, or Enter for 1, k3.1-pro[1m]: ", out)
        self.assertNotIn("k3.1-pro ", out)
        with open(path, encoding="utf-8") as fh:
            self.assertEqual(fh.read(), 'CK_API_KEY="kimi-test-key"\nCK_PLAN="pro"\nCK_MODEL="k3-256k"\n'
                                        'CK_EFFORT="max"\nCK_MAX_TOKENS=""\n')
        env = self.env("k")
        self.assertEqual(env["ANTHROPIC_MODEL"], "k3-256k")
        self.assertEqual(env["CLAUDE_CODE_MAX_CONTEXT_TOKENS"], "262144")
        code, out = self.h.run(SCRIPT)
        self.assertIn("key …-key · k3-256k · effort max\n"
                      "  /model offers K3.1 Pro, K3.1 Pro Highspeed, K3, K3 256K, K2.8 Preview and K2.7 Code Highspeed"
                      " (pro plan)", out)
        # Plus: the model saved stays offered, so Enter keeps it; 1 takes K3.1 Pro at 256k.
        code, out = self.setup_with("k", "\n2\n1\n\n\n")
        self.assertEqual(code, 0, out)
        self.assertIn("Choose 1-3, or Enter to keep pro: ", out)
        self.assertIn("  1) k3.1-pro                   K3.1 Pro: the flagship, 256k on this plan\n"
                      "  2) k3                         K3: the generation before, 256k on this plan, at twice K3 256K's quota\n"
                      "  3) k3-256k                    K3 256K: K3 at 256k, for half k3's quota\n"
                      "  4) kimi-for-coding[1m]        K2.8 Preview: 1M context on every plan\n", out)
        self.assertIn("Choose 1-4, or Enter to keep k3-256k: ", out)
        self.assertNotIn("highspeed", out)
        env = self.env("k")
        self.assertEqual(env["ANTHROPIC_MODEL"], "k3.1-pro")
        code, out = self.h.run(SCRIPT)
        self.assertIn("  /model offers K3.1 Pro, K3, K3 256K and K2.8 Preview (plus plan)", out)
        # Andante: one model, so nothing to choose; the model saved isn't it, so it's taken.
        code, out = self.setup_with("k", "\n3\n\n\n")
        self.assertEqual(code, 0, out)
        self.assertIn("Model: kimi-for-coding[1m]  K2.8 Preview: 1M context on every plan\n", out)
        self.assertEqual(out.count("Choose"), 1)
        with open(path, encoding="utf-8") as fh:
            self.assertIn('CK_PLAN="andante"\nCK_MODEL="kimi-for-coding[1m]"\n', fh.read())
        code, out = self.h.run(SCRIPT)
        self.assertIn("  /model offers K2.8 Preview (andante plan)", out)

    def test_setup_needs_a_key(self):
        code, out = self.setup_with("k", "")
        self.assertEqual(code, 1)
        self.assertIn("Nothing saved: an API key is needed", out)

    def test_status(self):
        self.h.provider_config("z", ZAI)
        code, out = self.h.run(SCRIPT)
        self.assertEqual(code, 0, out)
        self.assertIn("Z.ai (claude-z, or cl -z <profile>):", out)
        self.assertIn("key …1234 · glm-5.3[1m] (sonnet glm-5.3-flash[1m], haiku glm-4.7) (not offered now)"
                      " · effort max · max output 128000\n  /model offers GLM-5.3 and GLM-5.3-Flash\n", out)
        self.assertNotIn("zai-test-key", out)
        self.assertIn("Kimi (claude-k, or cl -k <profile>): not set up; run claude-k reconfig", out)
        self.assertIn("MiniMax (claude-m, or cl -m <profile>): not set up; run claude-m reconfig", out)
        self.assertIn("DeepSeek (claude-d, or cl -D <profile>): not set up; run claude-d reconfig", out)

    def test_minimax(self):
        # As MiniMax's Claude Code guide sets it, 2026-11-05: M3.1 Flash in every slot, and Claude
        # Code compacting at 512K of its 1M.
        self.h.provider_config("m", 'CM_API_KEY="minimax-test-key"\n')
        env = self.env("m")
        self.assertEqual(env["ANTHROPIC_BASE_URL"], "https://api.minimax.io/anthropic")
        self.assertEqual(env["ANTHROPIC_AUTH_TOKEN"], "minimax-test-key")
        self.assertIsNone(env["ANTHROPIC_API_KEY"])
        for name in ("ANTHROPIC_MODEL", "ANTHROPIC_DEFAULT_FABLE_MODEL", "ANTHROPIC_DEFAULT_OPUS_MODEL",
                     "ANTHROPIC_DEFAULT_SONNET_MODEL", "ANTHROPIC_DEFAULT_HAIKU_MODEL", "CLAUDE_CODE_SUBAGENT_MODEL"):
            self.assertEqual(env[name], "MiniMax-M3.1-Flash[1m]")
        self.assertEqual(env["CLAUDE_CODE_AUTO_COMPACT_WINDOW"], "524288")
        self.assertNotIn("CLAUDE_CODE_MAX_CONTEXT_TOKENS", env)
        self.assertEqual(env["CLAUDE_CODE_MAX_OUTPUT_TOKENS"], "128000")  # M3's and M3.1 Flash's ceiling
        # M3, the generation before, is the other model MiniMax's Anthropic-compatible API serves
        # (2026-11-08); the retired M2 series isn't offered.
        code, out = self.setup_with("m", "\n2\n\n\n")
        self.assertEqual(code, 0, out)
        self.assertIn("  1) MiniMax-M3.1-Flash[1m]     M3.1 Flash: 1M context, compacting at 512K (every M Plan tier)\n"
                      "  2) MiniMax-M3[1m]             M3: the generation before, 1M context\n", out)
        self.assertIn("Choose 1-2, or Enter for 1, MiniMax-M3.1-Flash[1m]: ", out)
        self.assertNotIn("M2", out)
        self.assertIn("Saved. Start Claude Code on MiniMax with claude-m, or with a profile: cl -m <profile>", out)
        env = self.env("m")
        for name in ("ANTHROPIC_MODEL", "ANTHROPIC_DEFAULT_OPUS_MODEL", "ANTHROPIC_DEFAULT_HAIKU_MODEL"):
            self.assertEqual(env[name], "MiniMax-M3[1m]")
        self.assertEqual(env["CLAUDE_CODE_AUTO_COMPACT_WINDOW"], "524288")
        code, out = self.h.run(SCRIPT)
        self.assertIn("key …-key · MiniMax-M3[1m] · effort max · max output 128000\n"
                      "  /model offers M3.1 Flash and M3\n", out)

    def test_deepseek(self):
        # As DeepSeek's Claude Code guide sets it, 2026-11-05, but with V4 Pro, the flagship, as
        # the default: plain deepseek-flash for the haiku tier and subagents, and compacting at 768K.
        self.h.provider_config("d", 'CD_API_KEY="deepseek-test-key"\n')
        env = self.env("d")
        self.assertEqual(env["ANTHROPIC_BASE_URL"], "https://api.deepseek.com/anthropic")
        self.assertEqual(env["ANTHROPIC_AUTH_TOKEN"], "deepseek-test-key")
        for name in ("ANTHROPIC_MODEL", "ANTHROPIC_DEFAULT_FABLE_MODEL", "ANTHROPIC_DEFAULT_OPUS_MODEL",
                     "ANTHROPIC_DEFAULT_SONNET_MODEL"):
            self.assertEqual(env[name], "deepseek-v4-pro[1m]")
        self.assertEqual(env["ANTHROPIC_DEFAULT_HAIKU_MODEL"], "deepseek-flash")
        self.assertEqual(env["CLAUDE_CODE_SUBAGENT_MODEL"], "deepseek-flash")
        self.assertEqual(env["CLAUDE_CODE_AUTO_COMPACT_WINDOW"], "786432")
        # What fits beside the 768K compaction window within 1M; DeepSeek documents no ceiling.
        self.assertEqual(env["CLAUDE_CODE_MAX_OUTPUT_TOKENS"], "262144")
        code, out = self.setup_with("d", "\n2\n\n\n")
        self.assertEqual(code, 0, out)
        self.assertIn("Choose 1-2, or Enter for 1, deepseek-v4-pro[1m]: ", out)
        self.assertIn("Saved. Start Claude Code on DeepSeek with claude-d, or with a profile: cl -D <profile>", out)
        env = self.env("d")
        self.assertEqual(env["ANTHROPIC_MODEL"], "deepseek-flash[1m]")
        self.assertEqual(env["ANTHROPIC_DEFAULT_HAIKU_MODEL"], "deepseek-flash")
        code, out = self.h.run(SCRIPT)
        self.assertIn("DeepSeek (claude-d, or cl -D <profile>):", out)
        self.assertIn("key …-key · deepseek-flash[1m] (haiku deepseek-flash) · effort max", out)
        code, out = self.h.run(SCRIPT, "env", "q")
        self.assertEqual(code, 2, out)
        self.assertIn("invalid choice: 'q' (choose from 'z', 'k', 'm', 'd')", out)


if __name__ == "__main__":
    unittest.main()
