"""Session-owned backups before same-agent moves, and reversing a move. Throwaway homes only."""
import json
import os
import unittest

from helpers import WINDOWS, FakeHome, write

S = "11111111-1111-4111-8111-111111111111"
X = "019f0000-aaaa-7000-8000-000000000001"
K = "0752ad88-cc62-4e00-9912-48e4146b7628"


def read(path):
    with open(path, "rb") as fh:
        return fh.read()


class Backups(unittest.TestCase):
    def setUp(self):
        self.h = h = FakeHome()
        self.addCleanup(h.cleanup)
        self.api = h.folder("code", "api")
        self.backups = os.path.join(os.path.dirname(h.config("")), "backups")

    def manifest(self, report):
        self.assertIn("backup", report)
        path = os.path.join(report["backup"]["path"], "manifest.json")
        self.assertTrue(os.path.isfile(path), path)
        with open(path, encoding="utf-8") as fh:
            return json.load(fh)

    def test_claude_dry_run_real_backup_and_undo(self):
        a, b = self.h.profile("a"), self.h.profile("b")
        transcript = self.h.session(a, self.api, S, "Back me up", 5)
        data = transcript[:-len(".jsonl")]
        write(os.path.join(data, "tool-results", "out.txt"), "tool output\n")
        original = read(transcript)
        code, out = self.h.run("move", S[:8], "b", "--from", "a", "--backup", "--dry-run", "--json")
        self.assertEqual(code, 0, out)
        planned = json.loads(out)["backup"]
        self.assertTrue(planned["dry_run"])
        self.assertGreaterEqual(planned["files"], 2)
        self.assertFalse(os.path.exists(self.backups))
        self.assertEqual(read(transcript), original)

        code, out = self.h.run("move", S[:8], "b", "--from", "a", "--backup", "--json")
        self.assertEqual(code, 0, out)
        report = json.loads(out)
        self.assertFalse(os.path.exists(transcript))
        manifest = self.manifest(report)
        transcript_item = next(i for i in manifest["items"] if i["source"] == transcript)
        self.assertEqual(read(transcript_item["backup"]), original)
        data_item = next(i for i in manifest["items"] if i["source"] == data)
        self.assertEqual(read(os.path.join(data_item["backup"], "tool-results", "out.txt")), b"tool output\n")

        code, out = self.h.run("move", S[:8], "a", "--from", "b", "--json")
        self.assertEqual(code, 0, out)
        self.assertEqual(read(transcript), original)

    def test_codex_backup_contains_only_the_session_file(self):
        a = self.h.codex_home("a", "same-account")
        b = self.h.codex_home("b", "same-account")
        self.assertTrue(os.path.isdir(b))
        rollout = self.h.codex_session(a, self.api, X, "Back up Codex", 5)
        original = read(rollout)
        code, out = self.h.run("move", X[:12], "codex:b", "--from", "codex:a", "--backup", "--json")
        self.assertEqual(code, 0, out)
        manifest = self.manifest(json.loads(out))
        self.assertEqual(len(manifest["items"]), 1)
        item = manifest["items"][0]
        self.assertEqual(item["source"], rollout)
        self.assertEqual(read(item["backup"]), original)
        self.assertFalse(os.path.exists(rollout))

    def test_kimi_backup_contains_the_whole_session_folder(self):
        home = self.h.kimi_home()
        self.h.kimi_home("other")
        folder = self.h.kimi_session(home, self.api, K, "Back up Kimi", 5)
        wire = os.path.join(folder, "agents", "main", "wire.jsonl")
        original = read(wire)
        code, out = self.h.run("move", K[:8], "kimi:other", "--from", "kimi", "--backup", "--json")
        self.assertEqual(code, 0, out)
        manifest = self.manifest(json.loads(out))
        item = next(i for i in manifest["items"] if i["source"] == folder)
        self.assertEqual(read(os.path.join(item["backup"], "agents", "main", "wire.jsonl")), original)
        self.assertFalse(os.path.exists(folder))

    def test_backup_is_refused_when_the_original_is_kept_or_converted(self):
        a, b = self.h.profile("a"), self.h.profile("b")
        self.h.codex_home("default", "account")
        transcript = self.h.session(a, self.api, S, "Keep me", 5)
        code, out = self.h.run("move", S[:8], "codex", "--from", "a", "--backup", "--json")
        self.assertEqual(code, 1, out)
        self.assertIn("already keeps it", out)
        code, out = self.h.run("move", S[:8], "b", "--from", "a", "--keep", "--backup")
        self.assertEqual(code, 1, out)
        self.assertIn("already keeps it", out)
        self.assertTrue(os.path.exists(transcript))
        self.assertFalse(os.path.exists(self.backups))

    @unittest.skipIf(WINDOWS, "POSIX symlinks")
    def test_backup_skips_symlinks_instead_of_copying_their_targets(self):
        a, b = self.h.profile("a"), self.h.profile("b")
        transcript = self.h.session(a, self.api, S, "Linked tool output", 5)
        data = transcript[:-len(".jsonl")]
        os.makedirs(data)
        outside = os.path.join(self.h.root, "outside.txt")
        write(outside, "not session-owned\n")
        os.symlink(outside, os.path.join(data, "linked.txt"))
        code, out = self.h.run("move", S[:8], "b", "--from", "a", "--backup", "--json")
        self.assertEqual(code, 0, out)
        report = json.loads(out)
        self.assertEqual(report["backup"]["skipped"], 1)
        manifest = self.manifest(report)
        item = next(i for i in manifest["items"] if i["source"] == data)
        self.assertEqual(item["skipped"], 1)
        self.assertFalse(os.path.lexists(os.path.join(item["backup"], "linked.txt")))
        self.assertEqual(read(outside), b"not session-owned\n")

    def test_an_open_session_is_refused_before_a_backup_is_made(self):
        a, b = self.h.profile("a"), self.h.profile("b")
        transcript = self.h.session(a, self.api, S, "Still open", 5)
        self.h.register_open(a, S)
        code, out = self.h.run("move", S[:8], "b", "--from", "a", "--backup")
        self.assertEqual(code, 1, out)
        self.assertIn("is open", out)
        self.assertTrue(os.path.exists(transcript))
        self.assertFalse(os.path.exists(self.backups))


if __name__ == "__main__":
    unittest.main()
