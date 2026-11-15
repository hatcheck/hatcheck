"""Hatcheck-owned session library metadata and saved filters, never written to agent files."""
import json
import os
import unittest

from helpers import FakeHome, write

A = "aaaaaaaa-0000-4000-8000-000000000001"


class Library(unittest.TestCase):
    def setUp(self):
        self.h = h = FakeHome()
        self.addCleanup(h.cleanup)
        self.work = h.profile("work")
        self.api = h.folder("code", "api")
        self.transcript = h.session(self.work, self.api, A, "Fix the login test", 5)

    def test_session_metadata_round_trip_without_touching_the_transcript(self):
        with open(self.transcript, "rb") as fh:
            before = fh.read()
        code, out = self.h.run("library", "--json", "set", A[:8], "--from", "work",
                               "--alias", "Login bug", "--favorite", "--tag", "z-last",
                               "--tag", "backend", "--archive")
        self.assertEqual(code, 0, out)
        entry = json.loads(out)
        self.assertEqual(entry["agent"], "claude")
        self.assertEqual(entry["profile"], self.work)
        self.assertEqual(entry["id"], A)
        self.assertEqual(entry["alias"], "Login bug")
        self.assertTrue(entry["favorite"])
        self.assertTrue(entry["archived"])
        self.assertEqual(entry["tags"], ["backend", "z-last"])
        with open(self.transcript, "rb") as fh:
            self.assertEqual(fh.read(), before)

        code, out = self.h.run("library", "--json")
        self.assertEqual(code, 0, out)
        library = json.loads(out)
        self.assertEqual(library["version"], 1)
        stored = library["sessions"][entry["key"]]
        self.assertEqual(stored["alias"], "Login bug")
        self.assertEqual(stored["tags"], ["backend", "z-last"])

        code, out = self.h.run("library", "--json", "set", A[:8], "--from", "work",
                               "--clear-alias", "--no-favorite", "--untag", "z-last", "--unarchive")
        self.assertEqual(code, 0, out)
        changed = json.loads(out)
        self.assertIsNone(changed["alias"])
        self.assertFalse(changed["favorite"])
        self.assertFalse(changed["archived"])
        self.assertEqual(changed["tags"], ["backend"])
        code, out = self.h.run("library", "clear", A[:8], "--from", "work")
        self.assertEqual(code, 0, out)
        self.assertIn("transcript is unchanged", out)
        code, out = self.h.run("library", "--json")
        self.assertEqual(json.loads(out)["sessions"], {})
        with open(self.transcript, "rb") as fh:
            self.assertEqual(fh.read(), before)

    def test_saved_filters(self):
        code, out = self.h.run("library", "--json", "save-filter", "Backend favorites",
                               "--query", "login api", "--agent", "claude", "--profile", self.work,
                               "--hours", "72", "--model", "fable", "--cost", "1-10",
                               "--tag", "backend", "--favorites", "--archived")
        self.assertEqual(code, 0, out)
        saved = json.loads(out)
        self.assertEqual(saved["name"], "Backend favorites")
        self.assertEqual(saved["filter"]["hours"], 72)
        self.assertEqual(saved["filter"]["cost"], "1-10")
        self.assertTrue(saved["filter"]["favorites"])
        self.assertTrue(saved["filter"]["archived"])
        code, out = self.h.run("library", "--json")
        self.assertIn("Backend favorites", json.loads(out)["filters"])
        code, out = self.h.run("library", "remove-filter", "Backend favorites")
        self.assertEqual(code, 0, out)
        code, out = self.h.run("library", "remove-filter", "Backend favorites")
        self.assertEqual(code, 1, out)
        code, out = self.h.run("library", "save-filter", "Bad", "--hours", "0")
        self.assertEqual(code, 2, out)
        code, out = self.h.run("library", "save-filter", "Bad", "--cost", "free")
        self.assertEqual(code, 2, out)

    def test_a_broken_library_is_an_error_not_empty_metadata(self):
        config = self.h.config("")
        write(os.path.join(os.path.dirname(config), "library.json"), "{")
        code, out = self.h.run("library", "--json")
        self.assertEqual(code, 1, out)
        self.assertIn("can't read", out)


if __name__ == "__main__":
    unittest.main()
