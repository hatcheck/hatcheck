"""hatcheck doctor: every copy of hatcheck's commands on the PATH, grouped by how each was
installed, the agents' copies, and hooks that run a hatcheck that's gone. Against a throwaway
home with copies of the binary under test laid out as uv and Homebrew lay them out."""
import json
import os
import re
import unittest

from helpers import ENGINE, WINDOWS, FakeHome, write

SIX = ["hatcheck", "hat", "cl", "cx", "claude-z", "claude-k"]


class Doctor(unittest.TestCase):
    def setUp(self):
        self.h = FakeHome()
        self.addCleanup(self.h.cleanup)

    def link(self, path, target):
        os.makedirs(os.path.dirname(path), exist_ok=True)
        os.symlink(target, path)

    def uv_install(self):
        """~/.local/bin/<name> -> ~/.local/share/uv/tools/hatcheck/bin/<name>, each the binary under test."""
        h = self.h
        tools = os.path.join(h.home, ".local", "share", "uv", "tools", "hatcheck", "bin")
        for name in SIX:
            self.link(os.path.join(tools, name), ENGINE)
            self.link(os.path.join(h.home, ".local", "bin", name), os.path.join(tools, name))
        return os.path.join(h.home, ".local", "bin")

    def brew_install(self):
        """<brew>/bin/<name> -> ../Cellar/hatcheck/0.3.0/bin/<name>, as Homebrew links a formula."""
        brew = os.path.join(self.h.root, "homebrew")
        for name in SIX:
            self.link(os.path.join(brew, "Cellar", "hatcheck", "0.3.0", "bin", name), ENGINE)
            self.link(os.path.join(brew, "bin", name), os.path.join("..", "Cellar", "hatcheck", "0.3.0", "bin", name))
        return os.path.join(brew, "bin")

    def doctor(self, path_dirs, *args):
        return self.h.run("doctor", *args, PATH=os.pathsep.join(list(path_dirs) + [self.h.env()["PATH"]]))

    @unittest.skipIf(WINDOWS, "lays the installs out with symlinks")
    def test_two_installs(self):
        uv, brew = self.uv_install(), self.brew_install()
        code, out = self.doctor([uv, brew])
        self.assertEqual(code, 1, out)  # something to fix
        self.assertRegex(out, r"^This is hatcheck \d+\.\d+\.\d+, built in ")
        self.assertIn("Installs of hatcheck on your PATH, the first of each name running:", out)
        self.assertRegex(out, r"1\. uv: ~/\.local/bin -> ~/\.local/share/uv/tools/hatcheck/bin +\d+\.\d+\.\d+ +all six commands; runs")
        self.assertRegex(out, r"2\. Homebrew: \S+/homebrew/bin -> \S+/Cellar/hatcheck/0\.3\.0/bin +\d+\.\d+\.\d+ +all six commands; shadowed")
        self.assertIn("To remove Homebrew's: brew uninstall hatcheck", out)
        self.assertIn("To fix: 2 installs of hatcheck are on your PATH; keep one.", out)
        code, out = self.doctor([brew, uv])  # the other way round
        self.assertRegex(out, r"1\. Homebrew: .*; runs")
        self.assertIn("To remove uv's: uv tool uninstall hatcheck", out)

    @unittest.skipIf(WINDOWS, "lays the install out with symlinks")
    def test_one_install_and_the_agents(self):
        h = self.h
        uv = self.uv_install()
        fakebin = h.fake_claude()
        code, out = self.doctor([uv, fakebin])
        self.assertEqual(code, 0, out)
        self.assertIn("Install of hatcheck on your PATH, the first of each name running:", out)
        self.assertRegex(out, r"1\. uv: .*; runs\n")
        self.assertNotIn("To remove", out)
        self.assertIn("The agents:", out)
        self.assertRegex(out, r"Claude Code +2\.1\.303 \(Claude Code\) +\S+/fakebin/claude +\(runs, and cl runs it\)")
        self.assertIn("Codex            not on your PATH; cx needs it", out)
        self.assertIn("Antigravity CLI  not on your PATH", out)
        self.assertIn("Hooks from hatcheck bind --hook: none.", out)
        self.assertIn("Nothing to fix.", out)

    @unittest.skipIf(WINDOWS, "lays the install out with symlinks")
    def test_a_link_to_nothing_and_a_hook_that_is_gone(self):
        h = self.h
        uv = self.uv_install()
        stale = os.path.join(h.root, "stale", "bin")
        self.link(os.path.join(stale, "agy"), os.path.join(h.root, "gone", "agy"))
        profile = h.profile("work", "acct-w")
        write(os.path.join(profile, "settings.json"), json.dumps({"hooks": {"SessionStart": [{"hooks": [
            {"type": "command", "command": "'%s' bind --check" % os.path.join(h.root, "gone", "hatcheck")}]}]}}))
        code, out = self.doctor([uv, stale])
        self.assertEqual(code, 1, out)
        self.assertRegex(out, r"Antigravity CLI +\S+/stale/bin/agy -> \S+/gone/agy  \(a link to a file that's gone\)")
        self.assertIn("%s Claude Code runs %s, which is gone: run hatcheck bind --hook to point it at this one"
                      % (h.shown(profile).ljust(24), os.path.join(h.root, "gone", "hatcheck")), out)
        self.assertIn("is a link to a file that's gone; a shell may find it first", out)
        self.assertIn("which is gone", out)

    def test_json(self):
        h = self.h
        dirs = [] if WINDOWS else [self.uv_install()]
        code, out = self.doctor(dirs + [h.fake_claude()], "--json")
        d = json.loads(out)
        self.assertEqual(d["version"], 1)
        self.assertEqual(d["this"]["version"], re.match(r"\S+ (\S+)", h.run("--version")[1]).group(1))
        self.assertEqual([a["agent"] for a in d["agents"]], ["claude", "codex", "agy"])
        claude = d["agents"][0]["copies"][0]
        self.assertEqual((claude["version"], claude["runs"], claude["runnable"]), ("2.1.303 (Claude Code)", True, True))
        self.assertEqual(d["agents"][1]["copies"], [])
        self.assertEqual(d["hooks"], [])
        if not WINDOWS:
            self.assertEqual(code, 0, out)
            self.assertEqual((d["installs"][0]["install"], d["installs"][0]["commands"], d["installs"][0]["runs"]), ("uv", SIX, SIX))
            self.assertEqual(d["installs"][0]["remove"], "uv tool uninstall hatcheck")
            self.assertEqual(d["problems"], [])


if __name__ == "__main__":
    unittest.main()
