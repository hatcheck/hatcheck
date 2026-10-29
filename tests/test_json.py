"""--json for the session list and hatcheck usage: the same sessions and accounts as the
text shows, as data. Against a throwaway home: no real profiles, no network."""
import json
import os
import subprocess
import unittest

from helpers import NOW, FakeHome, argv_of, iso

A = "aaaaaaaa-0000-4000-8000-000000000001"
B = "bbbbbbbb-0000-4000-8000-000000000002"
X = "019f0000-aaaa-7000-8000-000000000001"


def fable(used, days):
    return {"limits": [
        {"kind": "session", "group": "session", "percent": 10, "severity": "normal", "resets_at": iso(NOW + 3600)},
        {"kind": "weekly_scoped", "group": "weekly", "percent": used, "severity": "normal",
         "resets_at": iso(NOW + days * 86400), "scope": {"model": {"display_name": "Fable"}}}]}


class Json(unittest.TestCase):
    def setUp(self):
        self.h = h = FakeHome()
        self.addCleanup(h.cleanup)
        self.work = h.profile("work", "acct-w", usage=fable(40, 2))
        # Saved two days ago, before its weekly limit reset a day ago.
        self.default = h.profile("default", "acct-d", saved_at=NOW - 2 * 86400, usage={"limits": [
            {"kind": "weekly_all", "group": "weekly", "percent": 70, "resets_at": iso(NOW - 86400)}]})
        self.api = h.folder("code", "api")
        h.session(self.work, self.api, A, "Fix the login test", 5, model="k3")
        h.session(self.default, self.api, B, "Write the docs", 10)
        h.codex_session(h.codex_home("default", "acct-c"), self.api, X, "Speed up the build", 20)

    def test_sessions(self):
        code, out = self.h.run("sessions", "--json", "--cached")
        self.assertEqual(code, 0, out)
        data = json.loads(out)
        self.assertEqual((data["version"], data["hours"], data["this_folder_only"]), (1, 24, False))
        self.assertEqual([s["id"] for s in data["sessions"]], [A, B, X])  # one folder, newest first
        a, b, x = data["sessions"]
        self.assertEqual((a["agent"], a["profile"], a["cwd"], a["model"]), ("claude", self.work, self.api, "k3"))
        self.assertIn("-k work --resume " + A, a["resume"])  # last on Kimi, as the text says
        self.assertRegex(a["last_active"], r"^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ$")
        self.assertIsNone(a["open"])
        self.assertIsNone(a["cost_usd"])  # no cost-state record: none known
        work = data["accounts"][a["account"]]
        self.assertEqual((work["agent"], work["profiles"], work["source"]), ("claude", [self.work], "cached"))
        row = next(r for r in work["limits"] if r["short"] == "Fable")
        self.assertEqual((row["used_percent"], row["left_percent"], row["per_model"], row["window_seconds"]),
                         (40, 60, True, 7 * 86400))
        self.assertAlmostEqual(row["pace"], 60 / 48, places=1)  # 60% left over two days
        self.assertEqual((work["fable"]["left"], work["fable"]["band"]), (True, "fresh"))
        self.assertNotIn("email", work)
        week = data["accounts"][b["account"]]["limits"][0]
        self.assertIsNone(week["left_percent"])  # reset since the snapshot: not known
        self.assertEqual((x["agent"], x["resume"]), ("codex", "codex resume " + X))
        code, out = self.h.run("sessions", "--json", "--no-usage")
        self.assertEqual(json.loads(out)["accounts"], [])
        code, out = self.h.run("sessions", "--json", "--no-usage", cwd=self.api)
        self.assertTrue(json.loads(out)["this_folder_only"])

    def test_usage(self):
        code, out = self.h.run("usage", "--json", "--cached")
        self.assertEqual(code, 0, out)
        data = json.loads(out)
        self.assertEqual(data["sorted_by"], "fable")
        self.assertEqual(data["accounts"][0]["profiles"], [self.work])  # has Fable left: first
        self.assertEqual(data["accounts"][-1]["agent"], "codex")
        self.assertTrue(all("email" not in a for a in data["accounts"]))
        code, out = self.h.run("usage", "--json", "--cached", "--email")
        self.assertTrue(all("email" in a for a in json.loads(out)["accounts"]))

    def test_errors_leave_stdout_empty(self):
        p = subprocess.run(argv_of("sessions") + ["--json", "nonsense"], env=self.h.env(),
                           capture_output=True, text=True, encoding="utf-8")
        self.assertEqual(p.returncode, 2)
        self.assertEqual(p.stdout, "")
        self.assertIn("expected hours", p.stderr)


class MoreJson(unittest.TestCase):
    """--json for hatcheck where, move and show, and what hatcheck open --login and
    hatcheck desktop give the desktop app."""

    def setUp(self):
        self.h = h = FakeHome()
        self.addCleanup(h.cleanup)
        self.work, self.b = h.profile("work"), h.profile("b")
        self.api = h.folder("code", "api")
        self.transcript = h.session(self.work, self.api, A, "Fix the login test", 5)
        os.makedirs(self.transcript[:-len(".jsonl")])  # its data folder
        self.codex = h.codex_home("default", "acct-c")
        h.codex_session(self.codex, self.api, X, "Speed up the build", 20)

    def test_where(self):
        code, out = self.h.run("where", A[:8], "--json")
        self.assertEqual(code, 0, out)
        copy, = json.loads(out)["sessions"]
        self.assertEqual((copy["agent"], copy["id"], copy["profile"], copy["file"], copy["data_folder"]),
                         ("claude", A, self.work, self.transcript, self.transcript[:-len(".jsonl")]))
        code, out = self.h.run("where", X[:13], "--json")
        copy, = json.loads(out)["sessions"]
        self.assertEqual((copy["agent"], copy["id"], copy["profile"], copy["archived"]), ("codex", X, self.codex, False))
        code, out = self.h.run("where", "ffff", "--json")
        self.assertEqual((code, json.loads(out)["sessions"]), (1, []))

    def test_move(self):
        code, out = self.h.run("move", A[:8], "b", "--dry-run", "--json")
        self.assertEqual(code, 0, out)
        report = json.loads(out)
        self.assertEqual((report["dry_run"], report["action"], report["deleted_original"], report["session"],
                          report["target"]), (True, "move", True, A, self.b))
        self.assertEqual([(i["what"], i["state"]) for i in report["items"]],
                         [("transcript", "new"), ("subagents and tool output", "new")])
        self.assertIn("b --resume " + A, report["resume"])
        self.assertFalse(os.path.exists(os.path.join(self.b, "projects")) and os.listdir(os.path.join(self.b, "projects")))
        code, out = self.h.run("fork", A[:8], "b", "--dry-run", "--json")
        self.assertEqual(code, 0, out)
        report = json.loads(out)
        self.assertEqual((report["action"], report["fork"], report["deleted_original"]), ("fork", True, False))
        self.assertRegex(report["new_session"], r"^[0-9a-f]{8}-[0-9a-f]{4}-4")
        self.assertIn("b --resume " + report["new_session"], report["resume"])
        code, out = self.h.run("move", A[:8], "b", "--keep", "--json")
        report = json.loads(out)
        self.assertEqual((code, report["action"], report["deleted_original"]), (0, "copy", False))
        code, out = self.h.run("move", A[:8], "b", "--json")
        report = json.loads(out)
        self.assertEqual((code, report["action"], report["deleted_original"]), (0, "move", True))
        self.assertEqual([i["state"] for i in report["items"]], ["same", "same"])
        self.assertFalse(os.path.exists(self.transcript))
        code, out = self.h.run("move", X[:13], "b", "--dry-run", "--json")
        report = json.loads(out)
        self.assertEqual((report["action"], report["target_agent"], report["new_session"]), ("convert", "claude", None))
        code, out = self.h.run("move", "ffff", "b", "--json")
        self.assertEqual(code, 1, out)
        self.assertIn("No session starting with ffff", out)  # on stderr; stdout stays empty

    def test_show(self):
        code, out = self.h.run("show", A[:8], "--json")
        self.assertEqual(code, 0, out)
        data = json.loads(out)
        self.assertEqual((data["id"], data["agent"], data["turns_total"]), (A, "claude", 2))
        self.assertEqual([(t["role"], t["text"]) for t in data["turns"]],
                         [("user", "Fix the login test"), ("assistant", "ok")])
        code, out = self.h.run("show", X[:13], "--last", "1")
        self.assertIn("Speed up the build", out.splitlines()[0])
        self.assertIn("Codex", out)
        self.assertIn("Done: Speed up the build", out)
        self.assertNotIn("SECRET-REASONING", out)

    def test_login_prints_without_making_anything(self):
        code, out = self.h.run("open", "--login", "codex:fresh", "--print")
        self.assertEqual(code, 0, out)
        self.assertTrue(out.splitlines()[1].endswith(" login"))
        self.assertFalse(os.path.exists(os.path.join(self.h.home, ".codex-fresh")))
        code, out = self.h.run("open", "--login", "work", "--print")
        self.assertIn(".claude-work", out)


if __name__ == "__main__":
    unittest.main()
