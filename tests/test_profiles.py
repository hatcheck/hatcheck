"""hatcheck profiles: what each Claude Code profile, Codex home and Kimi home is set up with,
and what differs between two. Against a throwaway home: no real profiles, Keychain or network,
and nothing that could be a secret in what it prints."""
import json
import os
import unittest

from helpers import FakeHome, write

ENV_VALUE = "hunter2-env-value"
MCP_TOKEN = "hunter2-mcp-token"
CODEX_TOKEN = "hunter2-codex-token"
KIMI_KEY = "hunter2-kimi-key"


class Profiles(unittest.TestCase):
    def setUp(self):
        self.h = h = FakeHome()
        self.addCleanup(h.cleanup)
        self.work = h.profile("work")
        self.home = h.profile("home")
        j = lambda path, data: write(path, json.dumps(data))  # noqa: E731
        j(os.path.join(self.work, "settings.json"), {
            "model": "opus", "theme": "dark", "env": {"FOO": ENV_VALUE},
            "permissions": {"allow": ["Bash(npm run:*)", "Read"], "deny": []},
            "hooks": {"PreToolUse": [{"matcher": "Bash", "hooks": [{"type": "command", "command": "echo one"}]}]},
            "enabledPlugins": {"x@m": True}, "modelSettings": {"opus": {"effortLevel": "high"}}})
        write(os.path.join(self.work, "CLAUDE.md"), "Be brief.")
        write(os.path.join(self.work, "skills", "k2", "SKILL.md"), "---\nname: k2\n---\nK2.")
        write(os.path.join(self.work, "agents", "reviewer.md"), "Review.")
        write(os.path.join(self.work, "commands", "git", "commit.md"), "Commit.")
        write(os.path.join(self.work, "hooks", "notify.sh"), "#!/bin/sh\n")
        j(os.path.join(self.work, "plugins", "installed_plugins.json"), {"version": 2, "plugins": {"x@m": [{"version": "1.0.0"}]}})
        j(h.config_path("work"), {"mcpServers": {"github": {"command": "npx", "env": {"TOKEN": MCP_TOKEN}}}})
        j(os.path.join(self.home, "settings.json"), {
            "model": "sonnet", "theme": "dark", "env": {"FOO": ENV_VALUE + "-other"},
            "permissions": {"allow": ["Read"]}})
        write(os.path.join(self.home, "CLAUDE.md"), "Be brief and kind.")
        write(os.path.join(self.home, "skills", "k2", "SKILL.md"), "---\nname: k2\n---\nK2.")
        write(os.path.join(self.home, "skills", "pdf", "SKILL.md"), "PDFs.")
        j(h.config_path("home"), {"mcpServers": {"github": {"command": "npx", "env": {"TOKEN": MCP_TOKEN + "-other"}}}})
        self.codex = h.codex_home("default")
        write(os.path.join(self.codex, "config.toml"),
              'model = "gpt-5.5"\nmodel_reasoning_effort = "high"\n\n[mcp_servers.github]\ncommand = "npx"\n'
              'env = { TOKEN = "%s" }\n\n[projects."/Users/me/app.v2"]\ntrust_level = "trusted"\n' % CODEX_TOKEN)
        write(os.path.join(self.codex, "AGENTS.md"), "Terse.")
        write(os.path.join(self.codex, "prompts", "commit.md"), "Commit.")
        j(os.path.join(self.codex, "hooks.json"), {"hooks": {"SessionStart": [{"hooks": [{"type": "command", "command": "echo hi"}]}]}})
        self.codex_other = h.codex_home("other")
        write(os.path.join(self.codex_other, "config.toml"), 'model = "gpt-5.5"\n')
        self.kimi = h.kimi_home("default")
        write(os.path.join(self.kimi, "config.toml"),
              'default_model = "kimi-code/k3"\n\n[thinking]\nenabled = true\neffort = "high"\n\n'
              '[providers."managed:kimi-code"]\ntype = "kimi"\napi_key = "%s"\n' % KIMI_KEY)
        write(os.path.join(self.kimi, "hooks", "state.sh"), "#!/bin/sh\n")

    def assertNoSecret(self, text):
        for secret in (ENV_VALUE, MCP_TOKEN, CODEX_TOKEN, KIMI_KEY):
            self.assertNotIn(secret, text)

    def test_every_profile_and_home_is_summed_up(self):
        code, out = self.h.run("profiles")
        self.assertEqual(code, 0, out)
        self.assertNoSecret(out)
        self.assertEqual(out.splitlines()[0], "What each profile and home is set up with")
        self.assertIn("Claude Code\n  ~/.claude-home\n    model sonnet · theme dark\n"
                      "    CLAUDE.md 18 B · 2 skills · 1 MCP server · 1 permission · 1 env variable\n", out)
        self.assertIn("  ~/.claude-work\n    model opus · theme dark · 1 more setting\n"
                      "    CLAUDE.md 9 B · 1 skill · 1 agent · 1 command · 1 plugin · 1 hook · 1 hooks file · 1 MCP server"
                      " · 2 permissions · 1 env variable\n", out)
        self.assertIn("Codex\n  ~/.codex  (default)\n    model gpt-5.5 · model_reasoning_effort high\n"
                      "    AGENTS.md 6 B · 1 prompt · 1 hook · 1 MCP server · 1 trusted folder\n"
                      "  ~/.codex-other\n    model gpt-5.5\n    nothing set up yet\n", out)
        self.assertIn("Kimi Code CLI\n  ~/.kimi-code  (default)\n    default_model kimi-code/k3 · effort high · 1 more setting\n"
                      "    1 hooks file\n", out)
        self.assertTrue(out.endswith("hatcheck profiles A B shows what differs between two; nothing is changed.\n"), out)
        code, out = self.h.run("profiles", "--json")
        self.assertEqual(code, 0, out)
        self.assertNoSecret(out)
        data = json.loads(out)
        self.assertEqual(data["version"], 1)
        by_path = {p["path"]: p for p in data["profiles"]}
        work = by_path[self.work]
        self.assertEqual(work["agent"], "claude")
        kinds = sorted(set(i["kind"] for i in work["items"]))
        self.assertEqual(kinds, ["MCP server", "agent", "command", "env", "hook", "hooks file", "instructions",
                                 "permission", "plugin", "setting", "skill"])
        env = next(i for i in work["items"] if i["kind"] == "env")
        self.assertEqual((env["name"], env["shown"], env["path"]), ("FOO", None, None))
        skill = next(i for i in work["items"] if i["kind"] == "skill")
        self.assertEqual(skill["path"], os.path.join(self.work, "skills", "k2"))
        self.assertEqual(by_path[self.codex]["agent"], "codex")
        self.assertEqual(by_path[self.kimi]["agent"], "kimi")
        self.assertEqual(work["unread"], [])

    def test_two_profiles_compared(self):
        code, out = self.h.run("profiles", "work", "home")
        self.assertEqual(code, 1, out)  # they differ, as diff says it
        self.assertNoSecret(out)
        self.assertEqual(out.splitlines()[0], "~/.claude-work and ~/.claude-home, Claude Code profiles")
        self.assertIn("Only in ~/.claude-work:\n", out)
        for line in ("  agent reviewer", "  command git:commit", "  hooks file notify.sh", "  plugin x@m",
                     "  hook PreToolUse: [Bash] echo one", "  permission allow Bash(npm run:*)",
                     "  setting modelSettings.opus"):
            self.assertIn(line, out)
        self.assertIn("1.0.0, enabled", out)
        self.assertIn("effortLevel high", out)
        self.assertIn("~/.claude-work/agents/reviewer.md", out)
        self.assertIn("Only in ~/.claude-home:\n  skill pdf", out)
        self.assertIn("In both, not the same:\n", out)
        self.assertIn("  instructions CLAUDE.md  9 B in ~/.claude-work, 18 B in ~/.claude-home\n", out)
        self.assertIn("  MCP server github       defined differently\n", out)
        self.assertIn("  env FOO                 set differently\n", out)
        self.assertIn("  setting model           opus in ~/.claude-work, sonnet in ~/.claude-home\n", out)
        self.assertIn("The same in both: 1 skill, 1 permission and 1 setting.\n", out)
        self.assertIn("Nothing was changed: hatcheck reads a setup and never writes one.", out)
        code, out = self.h.run("profiles", "work", "home", "--json")
        self.assertEqual(code, 1, out)
        self.assertNoSecret(out)
        data = json.loads(out)
        self.assertEqual((data["a"]["path"], data["b"]["path"]), (self.work, self.home))
        self.assertEqual(sorted((i["kind"], i["name"]) for i in data["only_in_b"]), [("skill", "pdf")])
        self.assertEqual(sorted((d["kind"], d["name"]) for d in data["different"]),
                         [("MCP server", "github"), ("env", "FOO"), ("instructions", "CLAUDE.md"), ("setting", "model")])
        self.assertEqual((data["same"], data["identical"]), (3, False))
        # Codex homes: the config's tables, read without their values where they may hold keys.
        code, out = self.h.run("profiles", "codex", "codex:other")
        self.assertEqual(code, 1, out)
        self.assertNoSecret(out)
        self.assertIn("~/.codex and ~/.codex-other, Codex homes\nOnly in ~/.codex:\n", out)
        for line in ("  instructions AGENTS.md", "  prompt commit", "  hook SessionStart: echo hi", "  MCP server github",
                     "  trusted folder /Users/me/app.v2", "  setting model_reasoning_effort"):
            self.assertIn(line, out)
        self.assertIn("trusted\n", out)
        self.assertIn("The same in both: 1 setting.\n", out)

    def test_the_same_setup_says_so(self):
        same = self.h.profile("same")
        for name in ("settings.json", "CLAUDE.md"):
            with open(os.path.join(self.home, name), encoding="utf-8") as fh:
                write(os.path.join(same, name), fh.read())
        for skill in ("k2", "pdf"):
            with open(os.path.join(self.home, "skills", skill, "SKILL.md"), encoding="utf-8") as fh:
                write(os.path.join(same, "skills", skill, "SKILL.md"), fh.read())
        with open(self.h.config_path("home"), encoding="utf-8") as fh:
            write(self.h.config_path("same"), fh.read())
        code, out = self.h.run("profiles", "home", "same")
        self.assertEqual(code, 0, out)
        self.assertIn("The same setup in both: 1 instructions, 2 skills, 1 MCP server, 1 permission, 1 env variable and 2 settings.\n", out)
        self.assertNotIn("Only in", out)
        self.assertTrue(json.loads(self.h.run("profiles", "home", "same", "--json")[1])["identical"])

    def test_what_it_refuses(self):
        code, out = self.h.run("profiles", "work", "codex")
        self.assertEqual(code, 1, out)
        self.assertIn("~/.claude-work is Claude Code's and ~/.codex is Codex's: hatcheck compares two of one agent's", out)
        code, out = self.h.run("profiles", "work")
        self.assertEqual(code, 2, out)
        self.assertIn("give two profiles to compare, or none to see every one", out)
        code, out = self.h.run("profiles", "work", "work")
        self.assertEqual(code, 1, out)
        self.assertIn("are the same folder", out)
        code, out = self.h.run("profiles", "nope", "work")
        self.assertEqual(code, 1, out)
        self.assertIn("There's no ~/.claude-nope", out)
        self.h.config("[ignore]\nagents = [\"codex\"]\n")
        code, out = self.h.run("profiles")
        self.assertEqual(code, 0, out)
        self.assertNotIn("Codex", out)
        code, out = self.h.run("profiles", "codex", "codex:other")
        self.assertEqual(code, 1, out)
        self.assertIn("config.toml leaves Codex out", out)

    def test_a_file_that_cant_be_read_is_said_not_shown_as_absent(self):
        write(os.path.join(self.home, "settings.json"), "{not json")
        code, out = self.h.run("profiles")
        self.assertEqual(code, 0, out)
        self.assertIn("⚠ ~/.claude-home/settings.json: not valid JSON", out)
        code, out = self.h.run("profiles", "work", "home")
        self.assertIn("⚠ ~/.claude-home/settings.json: not valid JSON", out)
        data = json.loads(self.h.run("profiles", "--json")[1])
        home = next(p for p in data["profiles"] if p["path"] == self.home)
        self.assertEqual(home["unread"], [os.path.join(self.home, "settings.json").replace(self.h.home, "~") + ": not valid JSON"])


if __name__ == "__main__":
    unittest.main()
