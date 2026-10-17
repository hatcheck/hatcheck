"""hatcheck bind and unbind: folders tied to profiles, how the session list uses them, and
the warning hook in Claude Code's settings. cl and cx are in test_commands.py. Against a
throwaway home."""
import json
import os
import subprocess
import unittest

from helpers import WINDOWS, FakeHome, argv_of, write

S = "11111111-1111-4111-8111-111111111111"
T = "22222222-2222-4222-8222-222222222222"


class Bind(unittest.TestCase):
    def setUp(self):
        self.h = h = FakeHome()
        self.addCleanup(h.cleanup)
        self.work, self.personal = h.profile("work"), h.profile("personal")
        self.codex_work = h.codex_home("work")
        self.api = h.folder("code", "api")
        self.src = h.folder("code", "api", "src")

    def saved(self):
        base = os.path.join(self.h.home, "AppData", "Roaming") if WINDOWS else os.path.join(self.h.home, ".config")
        with open(os.path.join(base, "hatcheck", "bindings.json"), encoding="utf-8") as fh:
            return json.load(fh)

    def test_bind_which_list_and_unbind(self):
        h, shown = self.h, self.h.shown
        code, out = h.run("bind", "work", cwd=self.api)
        self.assertEqual(code, 0, out)
        self.assertIn("Bound %s and everything under it: Claude Code %s · Codex %s."
                      % (shown(self.api), shown(self.work), shown(self.codex_work)), out)
        self.assertIn("hatcheck bind --hook", out)
        self.assertEqual(self.saved(), {"version": 1, "folders": {os.path.realpath(self.api): {
            "claude": "work", "codex": "work"}}})
        code, out = h.run("bind", "--which", cwd=self.src)  # under it: the deepest bound folder wins
        self.assertEqual((code, out), (0, "work\n"))
        code, out = h.run("bind", "--which", "codex", cwd=self.src)
        self.assertEqual((code, out), (0, "work\n"))
        code, out = h.run("bind", "--which", cwd=h.root)
        self.assertEqual((code, out), (1, ""))
        code, out = h.run("bind", cwd=self.src)
        self.assertIn("This folder is under %s, which is bound to Claude Code %s" % (shown(self.api), shown(self.work)), out)
        code, out = h.run("bind", "personal", cwd=self.src)
        self.assertIn("Codex isn't bound: there's no %s" % shown(os.path.join(h.home, ".codex-personal")), out)
        code, out = h.run("bind", "--which", cwd=self.src)
        self.assertEqual(out, "personal\n")
        code, out = h.run("bind", "--list")
        self.assertIn(shown(self.api), out)
        self.assertIn(shown(self.src), out)
        code, out = h.run("bind", "nosuch", cwd=self.api)
        self.assertEqual(code, 1, out)
        self.assertIn("isn't a profile folder. Sign in there first, e.g. with: cl nosuch", out)
        code, out = h.run("bind", "work", "--codex", "default", cwd=self.api)
        self.assertIn("isn't a Codex home. Sign in there first, e.g. with: codex login", out)
        code, out = h.run("unbind", cwd=os.path.join(self.src, ".."))  # ~/code/api, spelled another way
        self.assertEqual(code, 0, out)
        self.assertIn("Unbound %s" % shown(self.api), out)
        code, out = h.run("unbind", cwd=self.api)
        self.assertEqual(code, 1, out)
        self.assertIn("isn't bound", out)
        self.assertEqual(list(self.saved()["folders"]), [os.path.realpath(self.src)])

    def test_list_marks_sessions_outside_the_binding(self):
        h = self.h
        h.session(self.personal, self.src, S, "Done under the other account", 5)
        h.session(self.work, self.src, T, "Done under the bound one", 10)
        h.run("bind", "work", cwd=self.api)
        code, out = h.run("sessions", "--no-usage")
        self.assertEqual(code, 0, out)
        wrong = next(line for line in out.splitlines() if "Done under the other account" in line)
        self.assertIn("⚠ folder bound to " + h.shown(self.work), wrong)
        right = next(line for line in out.splitlines() if "Done under the bound one" in line)
        self.assertNotIn("⚠", right)
        code, out = h.run("sessions", "--no-usage", "--json")
        bound = {s["id"]: s["bound_to"] for s in json.loads(out)["sessions"]}
        self.assertEqual(bound, {S: self.work, T: None})

    def hook(self, settings_profile, payload, codex=False):
        """What the hook prints for a session in settings_profile (a Codex home with codex),
        run the way Claude Code and Codex run it: its command line, through the shell."""
        with open(os.path.join(settings_profile, "hooks.json" if codex else "settings.json"), encoding="utf-8") as fh:
            entry = json.load(fh)["hooks"]["SessionStart"][-1]["hooks"][0]
        env = dict(self.h.env(), **{"CODEX_HOME" if codex else "CLAUDE_CONFIG_DIR": settings_profile})
        p = subprocess.run(entry["command"], shell=True, input=json.dumps(payload), env=env, capture_output=True,
                           text=True, encoding="utf-8")
        self.assertEqual(p.returncode, 0, p.stderr)
        return json.loads(p.stdout)["systemMessage"] if p.stdout.strip() else None

    def test_hook(self):
        h = self.h
        h.run("bind", "work", cwd=self.api)
        mine = {"model": "fable", "hooks": {"SessionStart": [{"hooks": [{"type": "command", "command": "echo hi"}]}]}}
        write(os.path.join(self.personal, "settings.json"), json.dumps(mine))
        other = h.codex_home("other")
        code, out = h.run("bind", "--hook")
        self.assertEqual(code, 0, out)
        self.assertEqual(out.count("  added"), 4)  # two profiles, two Codex homes
        self.assertIn("open /hooks in codex", out)
        with open(os.path.join(self.personal, "settings.json"), encoding="utf-8") as fh:
            settings = json.load(fh)
        self.assertEqual(settings["model"], "fable")
        self.assertEqual(settings["hooks"]["SessionStart"][0], mine["hooks"]["SessionStart"][0])
        self.assertTrue(settings["hooks"]["SessionStart"][1]["hooks"][0]["command"].endswith("bind --check"))
        code, out = h.run("bind", "--hook")
        self.assertEqual(out.count("already has it"), 4)

        payload = {"session_id": S, "cwd": self.src, "source": "resume", "hook_event_name": "SessionStart"}
        message = self.hook(self.personal, payload)
        self.assertIn("%s is bound to %s, but this session runs in %s"
                      % (h.shown(self.api), h.shown(self.work), h.shown(self.personal)), message)
        self.assertIn("hatcheck move 11111111 work, and resume it with cl work --resume " + S, message)
        self.assertIn("Quit and run cl work here instead", self.hook(self.personal, dict(payload, source="startup")))
        self.assertIsNone(self.hook(self.work, payload))  # the bound profile: nothing to say
        self.assertIsNone(self.hook(self.personal, dict(payload, cwd=h.root)))  # not a bound folder
        message = self.hook(other, payload, codex=True)
        self.assertIn("is bound to %s, but this session runs in %s" % (h.shown(self.codex_work), h.shown(other)), message)
        self.assertIn("hatcheck move 11111111 codex:work, and resume it with cx work resume " + S, message)
        self.assertIsNone(self.hook(self.codex_work, payload, codex=True))

        code, out = h.run("unbind", "--hook")
        self.assertEqual(code, 0, out)
        with open(os.path.join(self.personal, "settings.json"), encoding="utf-8") as fh:
            self.assertEqual(json.load(fh), mine)
        with open(os.path.join(self.work, "settings.json"), encoding="utf-8") as fh:
            self.assertEqual(json.load(fh), {})
        self.assertFalse(os.path.exists(os.path.join(other, "hooks.json")))  # it held only the hook

    def test_check_never_fails(self):
        p = subprocess.run(argv_of("bind") + ["--check"], input="not json",
                           env=self.h.env(), capture_output=True, text=True, encoding="utf-8")
        self.assertEqual((p.returncode, p.stdout), (0, ""))


if __name__ == "__main__":
    unittest.main()
