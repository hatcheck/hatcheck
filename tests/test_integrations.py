"""Raycast and Alfred integrations against the real CLI in a throwaway home."""
import json
import os
import subprocess
import sys
import unittest

from helpers import ENGINE, FakeHome

A = "aaaaaaaa-0000-4000-8000-000000000001"
X = "019f0000-aaaa-7000-8000-000000000001"
ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


class Integrations(unittest.TestCase):
    def setUp(self):
        self.h = h = FakeHome()
        self.addCleanup(h.cleanup)
        self.work = h.profile("work")
        self.codex = h.codex_home("default")
        self.api = h.folder("code", "api")
        h.session(self.work, self.api, A, "Fix the login test", 5)
        h.codex_session(self.codex, self.api, X, "Speed up the build", 10)
        self.env = dict(h.env(), HATCHECK_BIN=ENGINE,
                        PATH=os.path.dirname(sys.executable) + os.pathsep + h.env()["PATH"])

    def test_raycast_lists_real_sessions_and_filters_an_agent(self):
        script = os.path.join(ROOT, "integrations", "raycast", "hatcheck-sessions.sh")
        out = subprocess.run(["bash", script], capture_output=True, text=True, encoding="utf-8",
                             env=self.env, check=True).stdout
        self.assertIn("Fix the login test", out)
        self.assertIn("Speed up the build", out)
        self.assertIn(A, out)
        self.assertIn(X, out)
        out = subprocess.run(["bash", script, "claude"], capture_output=True, text=True,
                             encoding="utf-8", env=self.env, check=True).stdout
        self.assertIn("Fix the login test", out)
        self.assertNotIn("Speed up the build", out)

    def test_alfred_returns_actions_and_filters_words(self):
        script = os.path.join(ROOT, "integrations", "alfred", "hatcheck_sessions.py")
        out = subprocess.run([sys.executable, script], capture_output=True, text=True, encoding="utf-8",
                             env=self.env, check=True).stdout
        items = json.loads(out)["items"]
        self.assertEqual(len(items), 2)
        self.assertEqual(len({item["uid"] for item in items}), 2)
        claude = next(item for item in items if item["title"] == "Fix the login test")
        self.assertIn(" && ", claude["arg"])
        self.assertIn(A, claude["arg"])
        self.assertEqual(claude["mods"]["cmd"]["arg"], claude["variables"]["resume"])
        out = subprocess.run(["python3", script, "codex speed"], capture_output=True, text=True,
                             encoding="utf-8", env=self.env, check=True).stdout
        items = json.loads(out)["items"]
        self.assertEqual(len(items), 1)
        self.assertEqual(items[0]["title"], "Speed up the build")
        out = subprocess.run(["python3", script, "nothing-matches-this"], capture_output=True,
                             text=True, encoding="utf-8", env=self.env, check=True).stdout
        self.assertEqual(json.loads(out)["items"][0]["uid"], "empty")


if __name__ == "__main__":
    unittest.main()
