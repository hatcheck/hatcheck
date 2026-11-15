"""Offline plan and spending insights from Hatcheck history and session cost records."""
import json
import os
import sys
import unittest

from helpers import NOW, WINDOWS, FakeHome, rec, write

A = "aaaaaaaa-0000-4000-8000-000000000001"
B = "bbbbbbbb-0000-4000-8000-000000000002"
C = "cccccccc-0000-4000-8000-000000000003"


class Insights(unittest.TestCase):
    def setUp(self):
        self.h = h = FakeHome()
        self.addCleanup(h.cleanup)
        self.work = h.profile("work")
        self.other = h.profile("other")
        self.api = h.folder("code", "api")
        self.blog = h.folder("code", "blog")
        h.session(self.work, self.api, A, "Costly API", 5, model="claude-sonnet-5",
                  extra=[rec(type="cost-state", sessionId=A, totalCostUSD=1.5)])
        h.session(self.other, self.blog, B, "Costly blog", 4, model="claude-opus-5",
                  extra=[rec(type="cost-state", sessionId=B, totalCostUSD=2.5)])
        h.session(self.work, self.api, C, "No cost recorded", 3, model="claude-sonnet-5")

    def history(self):
        if WINDOWS:
            folder = os.path.join(self.h.home, "AppData", "Local", "hatcheck", "Cache")
        elif sys.platform == "darwin":
            folder = os.path.join(self.h.home, "Library", "Caches", "hatcheck")
        else:
            folder = os.path.join(self.h.home, ".cache", "hatcheck")
        reset = NOW + 2 * 86400
        rows = []
        for t, used in [(NOW - 7200, 20), (NOW - 3600, 50), (NOW, 70)]:
            rows.append({"t": t, "account": "claude:" + self.work, "label": "week", "used": used,
                         "resets_at": reset, "window": 604800})
        rows.append({"t": NOW, "account": "claude:" + self.other, "label": "5h", "used": 40,
                     "resets_at": NOW + 3600, "window": 18000})
        write(os.path.join(folder, "usage-history.jsonl"),
              "\n".join(json.dumps(row, separators=(",", ":")) for row in rows) + "\n")

    def test_missing_plan_history_is_explicit_not_zero(self):
        code, out = self.h.run("insights", "--json")
        self.assertEqual(code, 0, out)
        report = json.loads(out)
        self.assertEqual(report["series"], [])
        self.assertEqual(report["weekly"]["session_count"], 3)
        self.assertEqual(report["weekly"]["costed_session_count"], 2)
        code, out = self.h.run("insights")
        self.assertEqual(code, 0, out)
        self.assertIn("No plan readings", out)

    def test_series_forecasts_and_spending(self):
        self.history()
        code, out = self.h.run("insights", "--json")
        self.assertEqual(code, 0, out)
        report = json.loads(out)
        self.assertEqual(report["days"], 7)
        self.assertEqual(len(report["series"]), 2)
        kinds = {forecast["kind"] for forecast in report["forecasts"]}
        self.assertEqual(kinds, {"runs_out", "unused"})
        weekly = report["weekly"]
        self.assertEqual(weekly, {"session_count": 3, "costed_session_count": 2, "cost_usd": 4.0})
        projects = {row["project"]: row for row in report["projects"]}
        self.assertEqual(projects[self.api]["cost_usd"], 1.5)
        self.assertEqual(projects[self.api]["session_count"], 2)
        self.assertEqual(projects[self.blog]["cost_usd"], 2.5)
        models = {row["model"]: row for row in report["models"]}
        self.assertEqual(models["claude-sonnet-5"]["cost_usd"], 1.5)
        self.assertEqual(models["claude-sonnet-5"]["session_count"], 2)
        profiles = {row["profile"]: row for row in report["profiles"]}
        self.assertEqual(profiles[self.work]["cost_usd"], 1.5)
        self.assertEqual(profiles[self.other]["cost_usd"], 2.5)
        code, out = self.h.run("insights")
        self.assertEqual(code, 0, out)
        self.assertIn("Usage insights, last 7 days", out)
        self.assertIn("runs out", out)
        self.assertIn("Sessions: 3", out)

    def test_days_and_unused_validation(self):
        for args in [("--days", "0"), ("--days", "31"), ("--unused", "101")]:
            code, out = self.h.run("insights", *args)
            self.assertEqual(code, 2, out)


if __name__ == "__main__":
    unittest.main()
