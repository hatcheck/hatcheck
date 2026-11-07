"""The usage history hatcheck keeps of its own readings, and the trend it reads from them in
hatcheck usage and best: how fast a limit is being used up, and whether it runs out before it
resets. Against a throwaway home: no real profiles, Keychain or network."""
import json
import os
import sys
import unittest

from helpers import NOW, FakeHome, iso


def limits(session, fable):
    return {"limits": [
        {"kind": "session", "group": "session", "percent": session, "severity": "normal", "resets_at": iso(NOW + 2 * 3600)},
        {"kind": "weekly_scoped", "group": "weekly", "percent": fable, "severity": "warning",
         "resets_at": iso(NOW + 4 * 86400), "scope": {"model": {"display_name": "Fable"}}}]}


class History(unittest.TestCase):
    def setUp(self):
        self.h = FakeHome()
        self.addCleanup(self.h.cleanup)

    def history_path(self):
        home = self.h.home
        if sys.platform == "darwin":
            return os.path.join(home, "Library", "Caches", "hatcheck", "usage-history.jsonl")
        if os.name == "nt":
            return os.path.join(home, "AppData", "Local", "hatcheck", "Cache", "usage-history.jsonl")
        return os.path.join(home, ".cache", "hatcheck", "usage-history.jsonl")

    def lines(self):
        with open(self.history_path(), encoding="utf-8") as fh:
            return [json.loads(line) for line in fh if line.strip()]

    def test_readings_are_kept_and_read_as_a_trend(self):
        h = self.h
        # A snapshot from two hours ago: one reading of each limit, at the snapshot's time.
        work = h.profile("work", "acct-w", usage=limits(40, 20), saved_at=NOW - 7200)
        code, out = h.run("usage", "--cached")
        self.assertEqual(code, 0, out)
        self.assertNotIn("using", out)  # one reading says nothing about a pace
        rows = self.lines()
        self.assertEqual(len(rows), 2)
        self.assertEqual(sorted((r["label"], r["used"]) for r in rows), [("session (5h)", 40), ("week (Fable)", 20)])
        self.assertEqual(rows[0]["account"], "claude:" + work)
        self.assertAlmostEqual(rows[0]["t"], NOW - 7200, delta=1)
        self.assertAlmostEqual(rows[0]["resets_at"], NOW + 2 * 3600, delta=1)
        # The same snapshot again: nothing written twice.
        h.run("usage", "--cached")
        self.assertEqual(len(self.lines()), 2)
        # A new snapshot, now: 32 points of Fable in two hours is 16%/h, out in 3h, before the
        # reset in 4 days; 20 points of the session limit is 10%/h, and 40 left lasts past the
        # reset in 2h.
        h.profile("work", "acct-w", usage=limits(60, 52), saved_at=NOW)
        code, out = h.run("usage", "--cached")
        self.assertEqual(code, 0, out)
        self.assertEqual(len(self.lines()), 4)
        self.assertRegex(out, r"week \(Fable\).*resets .* · using 16%/h over (2h00m|1h59m), runs out \S+")
        self.assertRegex(out, r"session \(5h\).*resets .* · using 10%/h over (2h00m|1h59m), lasts to the reset")
        data = json.loads(h.run("usage", "--cached", "--json")[1])
        trends = {l["name"]: l["trend"] for l in data["accounts"][0]["limits"]}
        fable = trends["week (Fable)"]
        self.assertEqual((fable["pace_percent_per_hour"], fable["readings"], fable["before_reset"]), (16.0, 2, True))
        self.assertAlmostEqual(fable["over_seconds"], 7200, delta=2)
        self.assertTrue(fable["runs_out_at"].endswith("Z"))
        session = trends["session (5h)"]
        self.assertEqual((session["pace_percent_per_hour"], session["runs_out_at"], session["before_reset"]), (10.0, None, False))
        # best says the same of the limit it leads with.
        code, out = h.run("best", "--cached", cwd=h.folder("code"))
        self.assertEqual(code, 0, out)
        self.assertRegex(out, r"using 16%/h over (2h00m|1h59m), runs out ")
        # The list's --json carries it too, for the accounts behind the sessions listed.
        h.session(work, h.folder("code"), "aaaaaaaa-0000-4000-8000-000000000001", "A session")
        data = json.loads(h.run("sessions", "--all-folders", "--cached", "--json")[1])
        self.assertEqual(data["accounts"][0]["limits"][1]["trend"]["pace_percent_per_hour"], 16.0)

    def test_without_readings_the_json_says_null(self):
        self.h.profile("work", "acct-w", usage=limits(40, 20), saved_at=NOW - 60)
        data = json.loads(self.h.run("usage", "--cached", "--json")[1])
        self.assertEqual([l["trend"] for l in data["accounts"][0]["limits"]], [None, None])


if __name__ == "__main__":
    unittest.main()
