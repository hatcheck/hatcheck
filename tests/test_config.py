"""hatcheck's own config.toml: the list's defaults, which the command line undoes, the agents
hatcheck doesn't look for and the folders the list hides, and what a bad file does. Against a
throwaway home: no real config, profiles or network."""
import json
import os
import unittest

from helpers import FakeHome, rec

A = "aaaaaaaa-0000-4000-8000-000000000001"
B = "bbbbbbbb-0000-4000-8000-000000000002"
X = "019f0000-aaaa-7000-8000-000000000001"


class ConfigFile(unittest.TestCase):
    def setUp(self):
        self.h = h = FakeHome()
        self.addCleanup(h.cleanup)
        self.work = h.profile("work")
        self.api = h.folder("code", "api")
        self.blog = h.folder("code", "blog")
        h.session(self.work, self.api, A, "Fix the login test", 5,
                  extra=[rec(type="cost-state", sessionId=A, totalCostUSD=2.5)])
        h.session(self.work, self.blog, B, "Draft a post", 3)
        self.codex = h.codex_home("default", "acct-c")
        h.codex_session(self.codex, self.api, X, "Speed up the build", 20)

    def test_the_lists_defaults_come_from_the_file_and_the_command_line_undoes_them(self):
        path = self.h.config("# hatcheck's config\n[list]\ndetails = true\nby_cost = true\nhours = \"3d\"\n"
                             "per_folder = 1\nusage = false\n")
        code, out = self.h.run("sessions")
        self.assertEqual(code, 0, out)
        self.assertEqual((code, out), self.h.run("sessions", "3d", "--details", "--by-cost", "-n", "1", "--no-usage"))
        self.assertIn("in the last 72h", out)
        self.assertIn("costliest first", out)
        self.assertIn("Fix the login test · main · $2.50\n", out)
        self.assertIn("turns 2  prompts 1  calls 0", out)  # --details
        self.assertIn("+1  other session", out)  # -n 1: the Codex session in the same folder
        # Each setting has a flag that undoes it for one run; -n 0 lists them all.
        undone = self.h.run("sessions", "24", "--no-details", "--by-time", "-n", "0", "--usage", "--cached")
        os.remove(path)
        self.assertEqual(undone, self.h.run("sessions", "--cached"))
        self.assertIn("newest first", undone[1])
        code, out = self.h.run("sessions", "--details", "--no-details")
        self.assertEqual(code, 2)
        self.assertIn("not allowed with argument", out)

    def test_an_agent_left_out_is_looked_for_nowhere(self):
        self.h.config("[ignore]\nagents = [\"codex\"]\n")
        code, out = self.h.run("sessions", "--no-usage")
        self.assertEqual(code, 0, out)
        self.assertNotIn("codex resume", out)
        self.assertNotIn("Codex", out)  # not in the heading either
        self.assertIn("cl work --resume " + A, out)
        code, out = self.h.run("usage", "--cached")
        self.assertEqual(code, 0, out)
        self.assertNotIn(".codex", out)
        code, out = self.h.run("where", X)
        self.assertEqual(code, 1, out)  # not found: hatcheck doesn't look there

    def test_a_folder_left_out_is_hidden_until_all(self):
        self.h.config("[ignore]\nfolders = [\"~/code/bl*\"]\n")
        code, out = self.h.run("sessions", "--no-usage")
        self.assertEqual(code, 0, out)
        self.assertNotIn("Draft a post", out)
        self.assertIn("Fix the login test", out)
        self.assertIn("Hidden: 1 by config (show with --all)", out)
        code, out = self.h.run("sessions", "--no-usage", "--all")
        self.assertIn("Draft a post", out)
        data = json.loads(self.h.run("sessions", "--no-usage", "--json")[1])
        self.assertEqual(data["hidden"], {"by config": 1})
        self.assertEqual([s["id"] for s in data["sessions"]], [A, X])
        # A folder named without a wildcard covers every folder under it, and where still finds
        # a hidden session by its ID.
        self.h.config("[ignore]\nfolders = [\"~/code\"]\n")
        code, out = self.h.run("sessions", "--no-usage")
        self.assertIn("Hidden: 3 by config (show with --all)", out)
        self.assertIn("No sessions in this window", out)
        code, out = self.h.run("where", B)
        self.assertEqual(code, 0, out)

    def test_only_the_folders_listed_show_and_those_ignored_never_do(self):
        self.h.config("[list]\nfolders = [\"~/code/*\"]\n")  # both folders: nothing hidden
        code, out = self.h.run("sessions", "--no-usage")
        self.assertEqual(code, 0, out)
        self.assertIn("Fix the login test", out)
        self.assertIn("Draft a post", out)
        self.assertNotIn("Hidden", out)
        self.h.config("[list]\nfolders = [\"~/code/api\"]\n[ignore]\nfolders = [\"~/code/api\"]\n")  # ignore wins
        code, out = self.h.run("sessions", "--no-usage")
        self.assertIn("Hidden: 3 by config (show with --all)", out)
        self.h.config("[list]\nfolders = [\"~/code/api\"]\n")
        code, out = self.h.run("sessions", "--no-usage")
        self.assertIn("Fix the login test", out)
        self.assertNotIn("Draft a post", out)
        self.assertIn("Hidden: 1 by config (show with --all)", out)
        self.assertIn("Draft a post", self.h.run("sessions", "--no-usage", "--all")[1])
        self.h.config("[list]\nfolders = []\n")  # empty: every folder, as without the setting
        self.assertNotIn("Hidden", self.h.run("sessions", "--no-usage")[1])
        self.h.config("[list]\nfolders = [\"~/code/*\"]\n")
        self.assertIn("  list: folders ~/code/*", self.h.run("doctor")[1])

    def test_a_file_that_cant_be_read_stops_every_command(self):
        for text, message in [
            ("[list\ndetails = true\n", "config.toml, line 1: expected [table]"),
            ("[list]\ndetails = yes\n", "config.toml, line 2: 'yes' isn't a value"),
            ("[list]\ndetail = true\n", "config.toml: list.detail isn't a setting; [list] takes details, by_cost, hours, "
                                        "per_folder, full, all_folders, usage and folders"),
            ("[ignore]\nagents = [\"gemini\"]\n", "config.toml: ignore.agents: 'gemini' isn't an agent; they're claude, "
                                                  "codex, agy, kimi, zcode, dsh, mcode, qwen, copilot and opencode"),
        ]:
            self.h.config(text)
            for command in (("sessions", "--no-usage"), ("usage", "--cached"), ("where", A)):
                code, out = self.h.run(*command)
                self.assertEqual(code, 2, out)
                self.assertIn("hatcheck: ~/.config/hatcheck/config.toml" if os.name != "nt" else "config.toml", out)
                self.assertIn(message, out)
        self.h.config("[list]\n")  # a file that sets nothing changes nothing
        code, out = self.h.run("sessions", "--no-usage")
        self.assertEqual(code, 0, out)
        self.assertIn("Fix the login test", out)

    def test_hatcheck_config_names_another_file(self):
        other = os.path.join(self.h.root, "elsewhere.toml")
        with open(other, "w", encoding="utf-8") as fh:
            fh.write("[list]\ndetails = true\n")
        code, out = self.h.run("sessions", "--no-usage", HATCHECK_CONFIG=other)
        self.assertEqual(code, 0, out)
        self.assertIn("turns 2  prompts 1  calls 0", out)
        code, out = self.h.run("sessions", "--no-usage")
        self.assertNotIn("turns", out)

    def test_doctor_shows_what_the_file_sets(self):
        code, out = self.h.run("doctor")
        self.assertIn("Config: none (", out)
        self.assertIn("config.toml would set the list's defaults and what to leave out", out)
        self.h.config("[list]\ndetails = true\nby_cost = true\nper_folder = 3\n[ignore]\nagents = [\"codex\", \"agy\"]\n"
                      "folders = [\"~/orca/workspaces/*\"]\n")
        code, out = self.h.run("doctor")
        self.assertIn("Config: " + ("~/.config/hatcheck/config.toml" if os.name != "nt" else "config.toml"), out)
        self.assertIn("  list: details, by cost, 3 per folder", out)
        self.assertIn("  ignore: agents codex, agy; folders ~/orca/workspaces/*", out)
        data = json.loads(self.h.run("doctor", "--json")[1])["config"]
        self.assertTrue(data["found"])
        self.assertEqual((data["list"]["details"], data["list"]["hours"], data["list"]["per_folder"]), (True, None, 3))
        self.assertEqual(data["ignore"]["agents"], ["codex", "agy"])
        self.assertEqual(data["ignore"]["folders"], [os.path.join(self.h.home, "orca", "workspaces", "*")])


if __name__ == "__main__":
    unittest.main()
