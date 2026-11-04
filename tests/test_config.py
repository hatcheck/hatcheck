"""hatcheck's own config.toml: the list's defaults, which the command line undoes, the agents
and folders no command shows whatever its flags, and what a bad file does. Against a throwaway
home: no real config, profiles or network."""
import json
import os
import subprocess
import unittest

from helpers import FakeHome, rec

A = "aaaaaaaa-0000-4000-8000-000000000001"
B = "bbbbbbbb-0000-4000-8000-000000000002"
C = "cccccccc-0000-4000-8000-000000000003"
D = "dddddddd-0000-4000-8000-000000000004"
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
        self.b_path = h.session(self.work, self.blog, B, "Draft a post", 3)
        self.codex = h.codex_home("default", "acct-c")
        h.codex_session(self.codex, self.api, X, "Speed up the build", 20)

    def test_the_lists_defaults_come_from_the_file_and_the_command_line_undoes_them(self):
        path = self.h.config("# hatcheck's config\n[list]\ndetails = true\nby_cost = true\nhours = \"3d\"\n"
                             "per_folder = 1\nusage = false\n")
        code, out = self.h.run("sessions", "--all-folders")
        self.assertEqual(code, 0, out)
        self.assertEqual((code, out), self.h.run("sessions", "--all-folders", "3d", "--details", "--by-cost", "-n", "1", "--no-usage"))
        self.assertIn("in the last 72h", out)
        self.assertIn("costliest first", out)
        self.assertIn("Fix the login test · main · $2.50\n", out)
        self.assertIn("turns 2  prompts 1  calls 0", out)  # --details
        self.assertIn("+1  other session", out)  # -n 1: the Codex session in the same folder
        # Each setting has a flag that undoes it for one run; -n 0 lists them all.
        undone = self.h.run("sessions", "--all-folders", "24", "--no-details", "--by-time", "-n", "0", "--usage", "--cached")
        os.remove(path)
        self.assertEqual(undone, self.h.run("sessions", "--all-folders", "--cached"))
        self.assertIn("newest first", undone[1])
        code, out = self.h.run("sessions", "--all-folders", "--details", "--no-details")
        self.assertEqual(code, 2)
        self.assertIn("not allowed with argument", out)

    def test_an_agent_left_out_is_looked_for_nowhere(self):
        self.h.config("[ignore]\nagents = [\"codex\"]\n")
        code, out = self.h.run("sessions", "--all-folders", "--no-usage")
        self.assertEqual(code, 0, out)
        self.assertNotIn("codex resume", out)
        self.assertNotIn("Codex", out)  # not in the heading either
        self.assertIn("cl work --resume " + A, out)
        code, out = self.h.run("usage", "--cached")
        self.assertEqual(code, 0, out)
        self.assertNotIn(".codex", out)
        code, out = self.h.run("where", X)
        self.assertEqual(code, 1, out)  # not found: hatcheck doesn't look there

    def test_a_folder_left_out_stays_out_whatever_the_flags(self):
        self.h.session(self.work, self.api, D, "From the app", 2, entrypoint="claude-desktop")
        self.h.config("[ignore]\nfolders = [\"~/code/bl*\"]\n")
        code, out = self.h.run("sessions", "--all-folders", "--no-usage")
        self.assertEqual(code, 0, out)
        self.assertNotIn("Draft a post", out)
        self.assertIn("Fix the login test", out)
        self.assertIn("Hidden: 1 Desktop (show with --all)", out)  # not counted: as if it weren't there
        code, out = self.h.run("sessions", "--all-folders", "--no-usage", "--all")
        self.assertIn("From the app", out)  # --all still shows what it shows
        self.assertNotIn("Draft a post", out)
        self.assertNotIn("Hidden", out)
        data = json.loads(self.h.run("sessions", "--no-usage", "--json", "--all")[1])
        self.assertEqual(data["hidden"], {})
        self.assertEqual(sorted(s["id"] for s in data["sessions"]), sorted([A, D, X]))
        # A folder named without a wildcard covers every folder under it.
        self.h.config("[ignore]\nfolders = [\"~/code\"]\n")
        code, out = self.h.run("sessions", "--all-folders", "--no-usage", "--all")
        self.assertIn("No sessions in this window", out)
        self.assertNotIn("Hidden", out)

    def test_no_command_finds_a_session_in_a_folder_left_out(self):
        self.h.config("[ignore]\nfolders = [\"~/code/blog\"]\n")
        self.assertEqual(self.h.run("where", B), (1, ""))
        code, out = self.h.run("where", B, "--json")
        self.assertEqual((code, json.loads(out)["sessions"]), (1, []))
        for command in (("show", B), ("open", B, "--print"), ("resume", B, "--print"), ("best", "--for", B, "--cached"),
                        ("move", B, "work", "--dry-run"), ("fork", B, "work", "--dry-run")):
            code, out = self.h.run(*command)
            self.assertEqual(code, 1, (command, out))
            self.assertIn("No session starting with " + B, out)
        self.assertEqual(self.h.run("show", A)[0], 0)  # the folders it doesn't leave out are as they were
        self.assertEqual(self.h.run("where", A)[0], 0)
        # relocate carries what it can see, and leaves the rest as it is, unmentioned.
        src = self.h.folder("src")
        code, out = self.h.run("relocate", os.path.join(self.h.home, "code"), src, "--json")
        self.assertEqual(code, 0, out)
        moved = json.loads(out)
        self.assertEqual(sorted(s["id"] for s in moved["sessions"]), sorted([A, X]))
        self.assertNotIn(B, out)
        self.assertTrue(os.path.exists(self.b_path))

    def test_a_folder_left_out_says_so_where_it_runs(self):
        self.h.config("[ignore]\nfolders = [\"~/code/blog\"]\n")
        left_out = "config.toml leaves %s out" % self.h.shown(self.blog)
        for flags in ((), ("-a",)):
            code, out = self.h.run("sessions", "--no-usage", *flags, cwd=self.blog)
            self.assertEqual(code, 0, out)
            self.assertTrue(out.startswith("🎩 hatcheck: "), out)  # the help, as from a folder with none
            self.assertIn(left_out + "; 2 sessions in other folders, which hatcheck sessions --all-folders lists.", out)
            self.assertNotIn("Draft a post", out)
            self.assertNotIn("A longer window", out)  # a longer one wouldn't bring them
        for command in (("open", "--print"), ("resume", "--print"), ("move", "work", "--dry-run")):
            code, out = self.h.run(*command, cwd=self.blog)
            self.assertEqual(code, 1, (command, out))
            self.assertIn("config.toml leaves this folder out", out)
        code, out = self.h.run("relocate", self.blog, self.h.folder("code", "blog2"))
        self.assertEqual(code, 1, out)
        self.assertIn(left_out + ", so hatcheck leaves its sessions as they are", out)
        self.assertTrue(os.path.exists(self.b_path))

    @unittest.skipIf(os.name == "nt", "a symbolic link needs more than a test's rights on Windows")
    def test_a_folder_named_through_a_link_is_left_out_too(self):
        # As /tmp leads to /private/tmp on macOS, where an agent started in /tmp says it ran.
        real = self.h.folder("real")
        os.symlink(real, os.path.join(self.h.home, "tmp"))
        self.h.session(self.work, os.path.join(real, "idea"), C, "Try an idea", 4)
        self.h.config("[ignore]\nfolders = [\"~/tmp\"]\n")
        code, out = self.h.run("sessions", "--all-folders", "--no-usage", "--all")
        self.assertEqual(code, 0, out)
        self.assertNotIn("Try an idea", out)
        self.assertIn("Fix the login test", out)
        self.assertEqual(self.h.run("where", C)[0], 1)

    def test_only_the_folders_listed_show_and_those_ignored_never_do(self):
        self.h.config("[list]\nfolders = [\"~/code/*\"]\n")  # both folders: nothing hidden
        code, out = self.h.run("sessions", "--all-folders", "--no-usage")
        self.assertEqual(code, 0, out)
        self.assertIn("Fix the login test", out)
        self.assertIn("Draft a post", out)
        self.assertNotIn("Hidden", out)
        self.h.config("[list]\nfolders = [\"~/code/api\"]\n[ignore]\nfolders = [\"~/code/api\"]\n")  # ignore wins
        code, out = self.h.run("sessions", "--all-folders", "--no-usage")
        self.assertIn("No sessions in this window", out)
        self.h.config("[list]\nfolders = [\"~/code/api\"]\n")
        for flags in ((), ("--all",)):
            code, out = self.h.run("sessions", "--all-folders", "--no-usage", *flags)
            self.assertIn("Fix the login test", out)
            self.assertNotIn("Draft a post", out)
            self.assertNotIn("Hidden", out)
        self.assertEqual(self.h.run("show", B)[0], 1)  # not among those listed: not there for any command
        self.h.config("[list]\nfolders = []\n")  # empty: every folder, as without the setting
        self.assertNotIn("Hidden", self.h.run("sessions", "--all-folders", "--no-usage")[1])
        self.h.config("[list]\nfolders = [\"~/code/*\"]\n")
        self.assertIn("  list: folders " + self.h.shown(os.path.join(self.h.home, "code", "*")), self.h.run("doctor")[1])

    def test_an_agent_left_out_isnt_reached_by_name_either(self):
        self.h.config("[ignore]\nagents = [\"codex\"]\n")
        for command in (("move", A, "codex", "--dry-run"), ("fork", A, "codex:other", "--dry-run"),
                        ("move", A, "work", "--from", "codex", "--dry-run"), ("show", A, "--from", "codex"),
                        ("open", "--login", "codex", "--print"), ("best", "--wait", "codex", "--cached"),
                        ("bind", "--codex", "default")):
            code, out = self.h.run(*command)
            self.assertEqual(code, 1, (command, out))
            self.assertIn("config.toml leaves Codex out", out)
        code, out = self.h.run("doctor")
        self.assertNotIn("  Codex ", out)  # not among the agents it checks
        self.assertIn("  ignore: agents codex", out)

    def codex_hook(self, home, payload):
        """What the hook in a Codex home prints for a session, run as Codex runs it."""
        with open(os.path.join(home, "hooks.json"), encoding="utf-8") as fh:
            entry = json.load(fh)["hooks"]["SessionStart"][-1]["hooks"][0]
        p = subprocess.run(entry["command"], shell=True, input=json.dumps(payload), env=dict(self.h.env(), CODEX_HOME=home),
                           capture_output=True, text=True, encoding="utf-8")
        self.assertEqual(p.returncode, 0, p.stderr)
        return p.stdout.strip()

    def test_a_hook_in_an_agent_left_out_is_quiet_and_not_said_to_be_gone(self):
        h = self.h
        other = h.codex_home("other")
        self.assertEqual(h.run("bind", "--codex", "default", cwd=self.api)[0], 0)
        self.assertEqual(h.run("bind", "--hook")[0], 0)
        payload = {"session_id": X, "cwd": self.api, "source": "startup", "hook_event_name": "SessionStart"}
        self.assertIn("is bound to", self.codex_hook(other, payload))
        h.config("[ignore]\nagents = [\"codex\"]\n")
        self.assertEqual(self.codex_hook(other, payload), "")
        unseen = "config.toml leaves Codex out, so its folders weren't looked in."
        code, out = h.run("unbind", "--hook")
        self.assertEqual(code, 0, out)
        self.assertIn("No profile, home or folder hatcheck looks in runs the hook now; ", out)
        self.assertIn(unseen, out)
        self.assertTrue(os.path.exists(os.path.join(other, "hooks.json")))  # not looked in, so left as it was
        self.assertIn(unseen, h.run("doctor")[1])

    def test_a_binding_a_launcher_still_uses_is_shown_though_its_agent_is_left_out(self):
        """cl, cx and ck don't read the file, so a folder's binding for an agent it leaves out still
        steers them: bind says so, as cx does. A part no launcher uses stays unshown."""
        h = self.h
        work = h.codex_home("work")
        self.assertEqual(h.run("bind", "--codex", "default", cwd=self.api)[0], 0)
        self.assertEqual(h.run("bind", "--codex", "work", cwd=self.blog)[0], 0)
        config = h.shown(h.config("[ignore]\nagents = [\"codex\"]\n"))
        used = "Codex %s, which %s leaves out but cx still uses" % (h.shown(self.codex), config)
        self.assertIn("This folder is bound to %s.\n" % used, h.run("bind", cwd=self.api)[1])
        out = h.run("--as", "cx", cwd=self.api, PATH=h.root)[1]  # no codex there: cx stops after saying so
        self.assertIn("cx: this folder is bound to default", out)
        code, out = h.run("bind", "--list")
        self.assertEqual(code, 0, out)
        self.assertIn("  Codex %s (cx only)\n" % h.shown(self.codex), out)
        self.assertIn("  Codex %s (cx only)\n" % h.shown(work), out)
        self.assertIn("%s leaves Codex out, but cx still uses its bindings.\n" % config, out)
        # Bound again, the folder keeps what Codex was bound to, untouched, and says so.
        code, out = h.run("bind", "work", cwd=self.api)
        self.assertEqual(code, 0, out)
        self.assertIn("It stays bound to %s.\n" % used, out)
        self.assertIn("This folder is bound to Claude Code %s, and to %s.\n" % (h.shown(self.work), used),
                      h.run("bind", cwd=self.api)[1])
        base = os.path.join(h.home, "AppData", "Roaming") if os.name == "nt" else os.path.join(h.home, ".config")
        bindings = os.path.join(base, "hatcheck", "bindings.json")
        with open(bindings, encoding="utf-8") as fh:
            self.assertEqual(json.load(fh)["folders"][self.api], {"claude": "work", "codex": "default"})
        code, out = h.run("unbind", cwd=self.blog)
        self.assertEqual(code, 0, out)
        self.assertIn("Unbound %s (it was bound to Codex %s, which %s leaves out but cx was using)."
                      % (h.shown(self.blog), h.shown(work), config), out)
        # qwen has no launcher, so nothing follows its binding while the file leaves it out.
        with open(bindings, "w", encoding="utf-8") as fh:
            json.dump({"version": 1, "folders": {self.blog: {"qwen": "default"}}}, fh)
        h.config("[ignore]\nagents = [\"qwen\"]\n")
        self.assertIn("This folder isn't bound.", h.run("bind", cwd=self.blog)[1])
        self.assertIn("No folders are bound.", h.run("bind", "--list")[1])

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
        code, out = self.h.run("sessions", "--all-folders", "--no-usage")
        self.assertEqual(code, 0, out)
        self.assertIn("Fix the login test", out)

    def test_hatcheck_config_names_another_file(self):
        other = os.path.join(self.h.root, "elsewhere.toml")
        with open(other, "w", encoding="utf-8") as fh:
            fh.write("[list]\ndetails = true\n")
        code, out = self.h.run("sessions", "--all-folders", "--no-usage", HATCHECK_CONFIG=other)
        self.assertEqual(code, 0, out)
        self.assertIn("turns 2  prompts 1  calls 0", out)
        code, out = self.h.run("sessions", "--all-folders", "--no-usage")
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
        self.assertIn("  ignore: agents codex, agy; folders " + self.h.shown(os.path.join(self.h.home, "orca", "workspaces", "*")),
                      out)
        data = json.loads(self.h.run("doctor", "--json")[1])["config"]
        self.assertTrue(data["found"])
        self.assertEqual((data["list"]["details"], data["list"]["hours"], data["list"]["per_folder"]), (True, None, 3))
        self.assertEqual(data["ignore"]["agents"], ["codex", "agy"])
        self.assertEqual(data["ignore"]["folders"], [os.path.join(self.h.home, "orca", "workspaces", "*")])


if __name__ == "__main__":
    unittest.main()
