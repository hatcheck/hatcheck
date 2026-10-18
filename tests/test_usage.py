"""hatcheck usage against a throwaway home, plus its platform helpers."""
import time
import unittest
from datetime import datetime
from unittest import mock

from helpers import NOW, FakeHome, iso

SCRIPT = "usage"
FAR = (NOW + 30 * 86400) * 1000  # a token expiry well in the future, in ms


def limits():
    return {"limits": [
        {"kind": "session", "group": "session", "percent": 40, "severity": "normal", "resets_at": iso(NOW + 2 * 3600)},
        {"kind": "weekly_scoped", "group": "weekly", "percent": 52, "severity": "warning",
         "resets_at": iso(NOW + 4 * 86400), "scope": {"model": {"display_name": "Fable"}}}],
        "iguana_necktie": {"utilization": 100, "resets_at": iso(NOW + 20 * 86400), "limit_dollars": 100,
                           "used_dollars": 106.08}}


class CachedUsage(unittest.TestCase):
    def setUp(self):
        self.h = FakeHome()
        self.addCleanup(self.h.cleanup)

    def test_rows_bars_and_the_login_file(self):
        # The login comes from .credentials.json, as on Linux and Windows.
        self.h.profile("work", "acct-w", usage=limits(), saved_at=NOW - 120,
                       oauth={"accessToken": "x", "subscriptionType": "team",
                              "rateLimitTier": "default_claude_max_5x", "expiresAt": FAR})
        code, out = self.h.run(SCRIPT, "--cached")
        self.assertEqual(code, 0, out)
        self.assertIn("work  team max 5x · cached 2m ago (--cached)", out)
        self.assertRegex(out, r"session \(5h\) +60% ██████░░░░  ████░░░░░░ resets \S+ \(in 1h5\dm\)")
        self.assertRegex(out, r"week \(Fable\) +48% ████▊░░░░░  █████▊░░░░ resets \S+ \S+ \(in 3d23h\)")
        self.assertIn("iguana_necktie", out)
        self.assertIn("$106.08 of $100 used", out)
        self.assertNotIn("@", out)  # no email unless --email

    def test_shared_account_shown_once(self):
        self.h.profile("a", "acct-1", usage=limits())
        self.h.profile("b", "acct-1")
        code, out = self.h.run(SCRIPT, "--cached")
        self.assertIn("~/.claude-a, ~/.claude-b".replace("/", __import__("os").sep), out)

    def test_not_signed_in(self):
        self.h.profile("solo")
        code, out = self.h.run(SCRIPT, "--cached")
        self.assertIn("no data (--cached)", out)
        self.assertNotIn("Sorted by Fable", out)  # no account has a Fable limit

    def test_sorted_by_fable(self):
        def fable(used, days, weekly_used=None):
            rows = [{"kind": "weekly_scoped", "group": "weekly", "percent": used, "severity": "normal",
                     "resets_at": iso(NOW + days * 86400), "scope": {"model": {"display_name": "Fable"}}}]
            if weekly_used is not None:
                rows.append({"kind": "weekly_all", "group": "weekly", "percent": weekly_used, "severity": "normal",
                             "resets_at": iso(NOW + 5 * 86400)})
            return {"limits": rows}
        h = self.h
        names = {}
        names["pro"] = h.profile("default", "acct-0", usage={"limits": [
            {"kind": "weekly_all", "group": "weekly", "percent": 10, "resets_at": iso(NOW + 86400)}]})
        names["later"] = h.profile("later", "acct-1", usage=fable(50, 3))
        names["soon"] = h.profile("soon", "acct-2", usage=fable(80, 1))
        names["spent"] = h.profile("spent", "acct-3", usage=fable(100, 0.5))
        # Fable left, but the weekly limit for all models, which Fable counts against too, is used up.
        names["blocked"] = h.profile("blocked", "acct-4", usage=fable(10, 0.2, weekly_used=100))
        # Saved before its Fable limit reset a day ago: full again, until it resets in 6 days.
        names["stale"] = h.profile("stale", "acct-5", usage=fable(95, -1), saved_at=NOW - 2 * 86400)
        code, out = h.run(SCRIPT, "--cached")
        self.assertEqual(code, 0, out)
        self.assertIn("Sorted by Fable: those with some left first, the soonest reset on top;", out)
        order = [line.split()[0] for line in out.splitlines() if line.startswith("~")]
        expected = ["soon", "later", "stale", "spent", "blocked", "pro"]
        self.assertEqual(order, [h.shown(names[n]) for n in expected])


if __name__ == "__main__":
    unittest.main()
