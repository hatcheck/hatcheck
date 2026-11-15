"""Opt-in deep doctor checks and stale-binding repair, never against real profiles."""
import json
import os
import plistlib
import shutil
import unittest

from helpers import WINDOWS, FakeHome, write

A = "aaaaaaaa-0000-4000-8000-000000000001"
B = "bbbbbbbb-0000-4000-8000-000000000002"


class DeepDoctor(unittest.TestCase):
    def setUp(self):
        self.h = h = FakeHome()
        self.addCleanup(h.cleanup)
        self.work = h.profile("work")
        self.api = h.folder("code", "api")

    def test_deep_findings_and_stale_binding_repair(self):
        self.h.session(self.work, self.api, A, "Readable", 5)
        broken = self.h.session(self.work, self.api, B, "Malformed", 4)
        write(broken, "{not json\n")
        code, out = self.h.run("bind", "work", self.api)
        self.assertEqual(code, 0, out)
        shutil.rmtree(self.api)
        code, out = self.h.run("doctor", "--deep", "--json")
        self.assertEqual(code, 1, out)
        report = json.loads(out)
        health = report["health"]
        kinds = {issue["kind"] for issue in health["issues"]}
        self.assertIn("stale-binding", kinds)
        self.assertIn("session-unreadable", kinds)
        self.assertIn("missing-binary", kinds)
        missing = {issue["message"].split(" (")[0]: issue["severity"] for issue in health["issues"] if issue["kind"] == "missing-binary"}
        self.assertEqual(missing["Claude Code"], "warning")
        self.assertEqual(missing["Codex"], "info")
        malformed = next(i for i in health["issues"] if i["kind"] == "session-unreadable")
        self.assertEqual(malformed["path"], broken)
        storage = next(row for row in health["storage"] if row["profile"] == self.work)
        self.assertGreater(storage["files"], 0)
        self.assertGreater(storage["bytes"], 0)
        self.assertTrue(health["processes_checked"])

        code, out = self.h.run("doctor", "--json")
        self.assertIsNone(json.loads(out)["health"])
        code, out = self.h.run("doctor", "--repair", "stale-bindings", "--json")
        self.assertEqual(code, 1, out)  # the malformed session and missing binaries remain
        repaired = json.loads(out)["health"]
        self.assertEqual(len(repaired["repaired"]), 1)
        self.assertNotIn("stale-binding", {issue["kind"] for issue in repaired["issues"]})
        bindings_path = os.path.join(os.path.dirname(self.h.config("")), "bindings.json")
        with open(bindings_path, encoding="utf-8") as fh:
            self.assertEqual(json.load(fh)["folders"], {})
        code, out = self.h.run("doctor", "--repair", "unknown")
        self.assertEqual(code, 2, out)

    @unittest.skipIf(WINDOWS, "POSIX process list and app bundle")
    def test_duplicate_processes_and_a_missing_launcher(self):
        data = self.h.folder(".claude-instances", "work")
        apps = self.h.folder("apps")
        app = os.path.join(apps, "Claude Work.app")
        info = os.path.join(app, "Contents", "Info.plist")
        os.makedirs(os.path.dirname(info))
        with open(info, "wb") as fh:
            plistlib.dump({"HatcheckInstance": "work", "HatcheckDataDir": data}, fh)
        launcher = os.path.join(app, "Contents", "MacOS", "launch")
        write(launcher, "#!/bin/sh\n")
        fakebin = self.h.folder("fakebin")
        process = "/Applications/Claude.app/Contents/MacOS/Claude --user-data-dir=%s" % data
        write(os.path.join(fakebin, "ps"), "#!/bin/sh\nprintf '%%s\\n' '%s' '%s'\n" % (process, process))
        os.chmod(os.path.join(fakebin, "ps"), 0o755)
        env = dict(HATCHECK_APPS_DIR=apps, PATH=fakebin + ":/bin")
        code, out = self.h.run("doctor", "--deep", "--json", **env)
        self.assertEqual(code, 1, out)
        health = json.loads(out)["health"]
        duplicates = [issue for issue in health["issues"] if issue["kind"] == "duplicate-instance"]
        self.assertEqual(len(duplicates), 1)
        self.assertIn("2 main processes", duplicates[0]["message"])
        self.assertNotIn("broken-launcher", {issue["kind"] for issue in health["issues"]})
        os.remove(launcher)
        code, out = self.h.run("doctor", "--deep", "--json", **env)
        kinds = {issue["kind"] for issue in json.loads(out)["health"]["issues"]}
        self.assertIn("broken-launcher", kinds)


if __name__ == "__main__":
    unittest.main()
