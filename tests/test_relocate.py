"""hatcheck relocate against a throwaway home: a project folder that was moved, and its
sessions brought along in every profile and home, the lines that name the folder rewritten,
the index and memory with them, and the binding; what it refuses; and the way back."""
import json
import os
import re
import unittest

from helpers import NOW, FakeHome, iso, rec, write

A = "aaaaaaaa-0000-4000-8000-000000000001"  # in code/api, with a data folder and an index entry
B = "bbbbbbbb-0000-4000-8000-000000000002"  # in code/api/sub: comes along too
C = "cccccccc-0000-4000-8000-000000000003"  # in code/api-old: named alike, stays
D = "dddddddd-0000-4000-8000-000000000004"  # in code/api, in the default profile
X = "019f0000-aaaa-7000-8000-000000000001"  # a Codex session in code/api
SCRIPT = "relocate"


def encoded(path):
    return re.sub(r"[^A-Za-z0-9]", "-", path)


class Relocate(unittest.TestCase):
    def setUp(self):
        self.h = h = FakeHome()
        self.addCleanup(h.cleanup)
        self.work = h.profile("work", "acct-w")
        self.default = h.profile("default", "acct-d")
        self.codex = h.codex_home("default")
        self.api = h.folder("code", "api")
        self.sub = h.folder("code", "api", "sub")
        self.old_api = h.folder("code", "api-old")
        self.a = h.session(self.work, self.api, A, "Fix the login test", 10)
        self.data = self.a[:-len(".jsonl")]
        write(os.path.join(self.data, "tool-results", "out.txt"), "big output\n", mtime=NOW - 600)
        self.project = os.path.dirname(self.a)
        write(os.path.join(self.project, "memory", "MEMORY.md"), "# Memory\n", mtime=NOW - 600)
        write(os.path.join(self.project, "sessions-index.json"), json.dumps({"version": 1, "entries": [
            {"sessionId": A, "fullPath": self.a, "projectPath": self.api, "messageCount": 2, "created": iso(NOW - 700),
             "modified": iso(NOW - 600), "gitBranch": "main", "isSidechain": False}]}), mtime=NOW - 600)
        self.b = h.session(self.work, self.sub, B, "In the sub folder", 20)
        self.c = h.session(self.work, self.old_api, C, "Another project", 30)
        self.d = h.session(self.default, self.api, D, "From the default profile", 40)
        self.x = h.codex_session(self.codex, self.api, X, "Speed up the build", 50)
        code, out = h.run("bind", "work", cwd=self.api)
        self.assertEqual(code, 0, out)
        with open(self.a, "rb") as fh:
            self.a_bytes = fh.read()
        with open(self.x, "rb") as fh:
            self.x_bytes = fh.read()
        # The folder was renamed, as the user does before running this.
        self.api2 = os.path.join(h.home, "code", "api2")
        os.rename(self.api, self.api2)

    def moved(self, profile, folder, sid):
        return os.path.join(profile, "projects", encoded(folder), sid + ".jsonl")

    def cwds(self, path):
        with open(path, encoding="utf-8") as fh:
            return [json.loads(line).get("cwd") for line in fh if '"cwd":"' in line]

    def test_dry_run_changes_nothing(self):
        code, out = self.h.run(SCRIPT, self.api, self.api2, "--dry-run")
        self.assertEqual(code, 0, out)
        self.assertIn("Would relocate 4 sessions from %s to %s:" % (self.h.shown(self.api), self.h.shown(self.api2)), out)
        self.assertIn("Nothing was changed", out)
        self.assertIn("Left alone: Claude Code session cccccccc in %s ran in %s, a folder named alike."
                      % (self.h.shown(self.work), self.h.shown(self.old_api)), out)
        self.assertTrue(os.path.exists(self.a) and os.path.exists(self.x))
        with open(self.a, "rb") as fh:
            self.assertEqual(fh.read(), self.a_bytes)
        self.assertFalse(os.path.exists(self.moved(self.work, self.api2, A)))

    def test_sessions_come_along(self):
        code, out = self.h.run(SCRIPT, self.api, self.api2)
        self.assertEqual(code, 0, out)
        self.assertIn("Relocated 4 sessions from", out)
        self.assertIn("projects/%s renamed projects/%s, with its memory and index" % (encoded(self.api), encoded(self.api2)), out)
        self.assertIn("1 session: the folder in the file rewritten", out)
        self.assertIn("To undo: hatcheck relocate", out)
        # A and its data folder, memory and index: the whole project folder renamed.
        new_a = self.moved(self.work, self.api2, A)
        self.assertTrue(os.path.isfile(new_a), new_a)
        self.assertFalse(os.path.exists(self.project))
        new_project = os.path.dirname(new_a)
        self.assertTrue(os.path.isfile(os.path.join(new_project, A, "tool-results", "out.txt")))
        self.assertTrue(os.path.isfile(os.path.join(new_project, "memory", "MEMORY.md")))
        with open(os.path.join(new_project, "sessions-index.json"), encoding="utf-8") as fh:
            entry = json.load(fh)["entries"][0]
        self.assertEqual((entry["fullPath"], entry["projectPath"], entry["messageCount"]), (new_a, self.api2, 2))
        # Every line that named the folder now names the new one, and nothing else changed.
        self.assertEqual(set(self.cwds(new_a)), {self.api2})
        with open(new_a, "rb") as fh:
            moved = fh.read()
        as_json = lambda p: json.dumps(p)[1:-1].encode()  # noqa: E731
        self.assertEqual(moved, self.a_bytes.replace(as_json(self.api), as_json(self.api2)))
        self.assertAlmostEqual(os.path.getmtime(new_a), NOW - 600, delta=2)  # timestamps kept
        # B, under it, went to the matching folder; C, named alike, stayed; D in the other profile moved.
        self.assertTrue(os.path.isfile(self.moved(self.work, os.path.join(self.api2, "sub"), B)))
        self.assertTrue(os.path.isfile(self.c))
        self.assertTrue(os.path.isfile(self.moved(self.default, self.api2, D)))
        # The Codex session stays where it is, with its folder rewritten in the lines that name it.
        self.assertTrue(os.path.isfile(self.x))
        with open(self.x, encoding="utf-8") as fh:
            records = [json.loads(line) for line in fh if line.strip()]
        meta = [r["payload"]["cwd"] for r in records if r["type"] in ("session_meta", "turn_context")]
        self.assertEqual(meta, [self.api2, self.api2])
        self.assertIn("<cwd>%s</cwd>" % self.api, json.dumps(records))  # what was said stays as it was
        # The binding moved with it.
        code, out = self.h.run("bind", cwd=self.api2)
        self.assertIn("work", out)
        # And the list resumes them from the new folder.
        code, out = self.h.run("sessions", "--no-usage", "--all-folders", "3d")
        self.assertIn("cd " + self.h.shown(self.api2), out)
        self.assertNotIn("cd " + self.h.shown(self.api) + "\n", out)
        self.assertIn("cl work --resume " + A, out)

    def test_the_way_back(self):
        self.h.run(SCRIPT, self.api, self.api2)
        os.rename(self.api2, self.api)
        code, out = self.h.run(SCRIPT, self.api2, self.api)
        self.assertEqual(code, 0, out)
        with open(self.a, "rb") as fh:
            self.assertEqual(fh.read(), self.a_bytes)  # byte for byte as it was
        with open(self.x, "rb") as fh:
            self.assertEqual(fh.read(), self.x_bytes)
        self.assertTrue(os.path.isfile(os.path.join(self.project, "memory", "MEMORY.md")))
        self.assertFalse(os.path.exists(os.path.join(self.work, "projects", encoded(self.api2))))

    def test_json(self):
        code, out = self.h.run(SCRIPT, self.api, self.api2, "--json", "--dry-run")
        self.assertEqual(code, 0, out)
        data = json.loads(out)
        self.assertEqual((data["version"], data["dry_run"], data["from"], data["to"]), (1, True, self.api, self.api2))
        self.assertEqual(sorted(s["id"] for s in data["sessions"]), sorted([A, B, D, X]))
        a = next(s for s in data["sessions"] if s["id"] == A)
        self.assertEqual((a["agent"], a["cwd_from"], a["cwd_to"], a["to"]), ("claude", self.api, self.api2, self.moved(self.work, self.api2, A)))
        self.assertEqual([s["id"] for s in data["skipped"]], [C])
        self.assertEqual(data["binding"]["to"], self.api2)
        self.assertIn("hatcheck relocate", data["undo"])

    def test_refusals(self):
        # An open session, one written a moment ago, and one already filed under the new folder.
        self.h.register_open(self.work, A)
        code, out = self.h.run(SCRIPT, self.api, self.api2)
        self.assertEqual(code, 1, out)
        self.assertIn("Nothing was moved:", out)
        self.assertIn("Claude Code session aaaaaaaa is open in pid", out)
        self.assertTrue(os.path.exists(self.a))
        os.remove(os.path.join(self.work, "sessions", "%d.json" % os.getpid()))
        os.utime(self.b, None)  # just written
        code, out = self.h.run(SCRIPT, self.api, self.api2)
        self.assertEqual(code, 1, out)
        self.assertIn("Claude Code session bbbbbbbb was written", out)
        os.utime(self.b, (NOW - 1200, NOW - 1200))
        self.h.session(self.work, self.api2, A, "Already here", 70)
        code, out = self.h.run(SCRIPT, self.api, self.api2)
        self.assertEqual(code, 1, out)
        self.assertIn("is already filed under %s too" % self.h.shown(self.api2), out)

    def test_folders_that_cannot_be(self):
        code, out = self.h.run(SCRIPT, self.api, os.path.join(self.h.home, "nowhere"))
        self.assertEqual(code, 1, out)
        self.assertIn("isn't a folder; move or rename the project there first", out)
        code, out = self.h.run(SCRIPT, self.api2, self.api2)
        self.assertIn("OLD and NEW are the same folder", out)
        code, out = self.h.run(SCRIPT, os.path.join(self.h.home, "code"), self.api2)
        self.assertIn("a folder can't move into itself", out)
        code, out = self.h.run(SCRIPT, os.path.join(self.h.home, "code", "nothing"), self.api2)
        self.assertEqual(code, 1, out)
        self.assertIn("No sessions ran in", out)


if __name__ == "__main__":
    unittest.main()
