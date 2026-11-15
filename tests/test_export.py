"""Self-contained session exports and project/date batches, against throwaway homes."""
import json
import os
import unittest

from helpers import FakeHome

A = "aaaaaaaa-0000-4000-8000-000000000001"
B = "bbbbbbbb-0000-4000-8000-000000000002"
C = "cccccccc-0000-4000-8000-000000000003"
D = "dddddddd-0000-4000-8000-000000000004"
X = "019f0000-aaaa-7000-8000-000000000001"


class Export(unittest.TestCase):
    def setUp(self):
        self.h = h = FakeHome()
        self.addCleanup(h.cleanup)
        self.work = h.profile("work")
        self.other = h.profile("other")
        self.codex = h.codex_home("default")
        self.api = h.folder("code", "api")
        self.blog = h.folder("code", "blog")
        h.session(self.work, self.api, A, "Fix <login> & secrets", 5)
        h.session(self.other, self.blog, B, "Write a post", 5)
        h.session(self.work, self.api, C, "Desktop session", 2, entrypoint="claude-desktop")
        h.session(self.work, self.api, D, "Old session", 120)
        h.codex_session(self.codex, self.api, X, "Fix the uploader", 10)

    def test_markdown_to_stdout_has_source_destination_and_warning(self):
        code, out = self.h.run("export", A[:8], "--from", "work")
        self.assertEqual(code, 0, out)
        self.assertIn("# Fix <login> & secrets", out)
        self.assertIn("Destination: stdout. Format: markdown.", out)
        self.assertIn("may contain secrets or private source code", out)
        self.assertIn("## You", out)

    def test_json_document_and_operation_report(self):
        output = os.path.join(self.h.root, "exports", "session.json")
        code, out = self.h.run("export", A[:8], "--from", "work", "--format", "json",
                               "--output", output, "--json")
        self.assertEqual(code, 0, out)
        report = json.loads(out)
        self.assertEqual(len(report["exported"]), 1)
        self.assertEqual(report["exported"][0]["path"], output)
        with open(output, encoding="utf-8") as fh:
            document = json.load(fh)
        self.assertEqual(document["version"], 1)
        self.assertEqual(document["source"]["id"], A)
        self.assertEqual(document["source"]["profile"], self.work)
        self.assertEqual(document["export"], {"format": "json", "path": output})
        self.assertIn("private source code", document["warning"])
        self.assertTrue(document["turns"])
        code, out = self.h.run("export", A[:8], "--from", "work", "--format", "json", "--json")
        self.assertEqual(code, 2, out)

    def test_html_is_self_contained_and_escaped(self):
        output = os.path.join(self.h.root, "session.html")
        code, out = self.h.run("export", A[:8], "--from", "work", "--format", "html", "--output", output)
        self.assertEqual(code, 0, out)
        with open(output, encoding="utf-8") as fh:
            html = fh.read()
        self.assertIn("<!doctype html>", html)
        self.assertIn("Fix &lt;login&gt; &amp; secrets", html)
        self.assertNotIn("Fix <login> & secrets", html)
        self.assertNotIn("http://", html)
        self.assertNotIn("https://", html)
        self.assertIn("Destination: " + output, html)
        self.assertIn("may contain secrets or private source code", html)

    def test_project_date_batch_and_dry_run(self):
        output = os.path.join(self.h.root, "batch")
        code, out = self.h.run("export", "--project", self.api, "--hours", "1", "--format", "json",
                               "--output", output, "--dry-run", "--json")
        self.assertEqual(code, 0, out)
        planned = json.loads(out)
        self.assertTrue(planned["dry_run"])
        self.assertEqual({item["source"]["id"] for item in planned["exported"]}, {A, X})
        self.assertFalse(os.path.exists(output))
        code, out = self.h.run("export", "--project", self.api, "--hours", "1", "--format", "json",
                               "--output", output, "--json")
        self.assertEqual(code, 0, out)
        exported = json.loads(out)["exported"]
        self.assertEqual(len(exported), 2)
        for item in exported:
            self.assertTrue(os.path.isfile(item["path"]), item["path"])
            with open(item["path"], encoding="utf-8") as fh:
                self.assertEqual(json.load(fh)["export"]["path"], item["path"])

        every = os.path.join(self.h.root, "every")
        code, out = self.h.run("export", "--project", self.api, "--hours", "1", "--all",
                               "--output", every, "--json")
        self.assertEqual(code, 0, out)
        self.assertEqual({item["source"]["id"] for item in json.loads(out)["exported"]}, {A, C, X})

    def test_overwrite_requires_replace(self):
        output = os.path.join(self.h.root, "session.md")
        for replace in (False, True):
            args = ["export", A[:8], "--from", "work", "--output", output]
            if replace:
                args.append("--replace")
            code, out = self.h.run(*args)
            self.assertEqual(code, 0, out)
            self.assertTrue(os.path.isfile(output))
            if not replace:
                code, out = self.h.run(*args)
                self.assertEqual(code, 1, out)
                self.assertIn("already exists", out)

    def test_an_empty_batch_is_an_error(self):
        code, out = self.h.run("export", "--project", self.api, "--hours", "0.000001",
                               "--output", os.path.join(self.h.root, "none"), "--json")
        self.assertEqual(code, 1, out)
        self.assertIn("No sessions match", out)


if __name__ == "__main__":
    unittest.main()
