"""hatcheck serve, which the desktop app keeps running: it answers each line of JSON with
one, sets environment variables for one command only, and keeps its cache on disk as it goes."""
import json
import os
import subprocess
import sys
import unittest

from helpers import ENGINE, FakeHome

S1 = "11111111-1111-4111-8111-111111111111"
S2 = "22222222-2222-4222-8222-222222222222"
CACHE_FILE = "session-facts.json"


class Serve(unittest.TestCase):
    def setUp(self):
        self.h = h = FakeHome()
        self.addCleanup(h.cleanup)
        self.work = h.profile("work", "acct-1")
        self.cwd = h.folder("code", "api")
        h.session(self.work, self.cwd, S1, "Fix the uploader")
        self.p = subprocess.Popen([ENGINE, "serve"], cwd=self.h.root,
                                  env=h.env(), stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                  text=True, encoding="utf-8")
        self.addCleanup(self.stop)

    def stop(self):
        if self.p.poll() is None:
            self.p.kill()
        self.p.wait(timeout=10)
        for stream in (self.p.stdin, self.p.stdout, self.p.stderr):
            stream.close()

    def ask(self, request):
        self.p.stdin.write((request if isinstance(request, str) else json.dumps(request)) + "\n")
        self.p.stdin.flush()
        return json.loads(self.p.stdout.readline())

    def test_answers_each_request(self):
        self.assertEqual(self.ask({"id": 1, "args": ["--version"]})["out"].split()[0], "hatcheck")
        listed = self.ask({"id": 2, "args": ["--json", "--no-usage", "--all-folders"]})
        self.assertEqual((listed["id"], listed["code"]), (2, 0))
        self.assertEqual([s["id"] for s in json.loads(listed["out"])["sessions"]], [S1])
        self.assertEqual(self.ask({"id": 3, "args": ["nope"]})["code"], 2)
        self.assertEqual(self.ask("not json"), {"id": None, "code": 2, "out": "",
                                                "err": "hatcheck serve: not a request: not json\n"})
        refused = self.ask({"id": 4, "args": ["open", S1[:8], "--here"]})
        self.assertEqual(refused["code"], 2)
        self.assertIn("--here", refused["err"])
        gone = self.ask({"id": 5, "args": ["show", "00000000"]})  # sys.exit with a message
        self.assertEqual((gone["code"], gone["err"]), (1, "No session starting with 00000000\n"))
        named = self.ask({"id": 6, "args": ["--as", "cl", "work"]})  # a name only: cl never runs here
        self.assertEqual((named["code"], named["err"]), (2, "hatcheck: there's no 'work' subcommand; see hatcheck --help\n"))
        self.p.stdin.close()
        self.assertEqual(self.p.wait(timeout=30), 0)

    def test_environment_for_one_command(self):
        elsewhere = os.path.join(self.h.root, "elsewhere", ".claude-x")
        os.makedirs(os.path.join(elsewhere, "projects"))
        self.h.session(elsewhere, self.cwd, S2, "In a profile outside the home folder")

        def ids(env=None):
            answer = self.ask({"id": 1, "args": ["--json", "--no-usage", "--all-folders"], "env": env or {}})
            return sorted(s["id"] for s in json.loads(answer["out"])["sessions"])
        self.assertEqual(ids({"CLAUDE_CONFIG_DIR": elsewhere}), [S1, S2])
        self.assertEqual(ids(), [S1])  # put back after the one command
        self.assertEqual(ids({"CLAUDE_CONFIG_DIR": elsewhere, "NO_COLOR": None}), [S1, S2])
        self.assertEqual(json.loads(self.ask({"id": 2, "args": ["--json", "--no-usage"]})["out"])["sessions"][0]["id"],
                         S1)

    def test_the_cache_is_saved_as_it_goes(self):
        self.ask({"id": 1, "args": ["--json", "--no-usage", "--all-folders"]})
        if os.name == "nt":
            cache = os.path.join(self.h.home, "AppData", "Local", "hatcheck", "Cache", CACHE_FILE)
        elif sys.platform == "darwin":
            cache = os.path.join(self.h.home, "Library", "Caches", "hatcheck", CACHE_FILE)
        else:
            cache = os.path.join(self.h.home, ".cache", "hatcheck", CACHE_FILE)
        self.assertTrue(os.path.isfile(cache))  # while it's still running


if __name__ == "__main__":
    unittest.main()
