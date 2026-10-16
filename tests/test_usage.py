"""hatcheck usage against a throwaway home, plus its platform helpers."""
import time
import unittest
from datetime import datetime
from unittest import mock

from helpers import NOW, FakeHome, iso, load

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


class Helpers(unittest.TestCase):
    def setUp(self):
        self.u = load(SCRIPT)

    def test_clock(self):
        now = time.time()
        for hours in (0.5, 5, 30, 100, 400):
            t = now + hours * 3600
            d = datetime.fromtimestamp(t)
            hm = "%d:%02d%s" % (d.hour % 12 or 12, d.minute, "am" if d.hour < 12 else "pm")
            expected = hm if hours < 24 else d.strftime("%a ") + hm if hours < 144 else d.strftime("%b ") + str(d.day)
            self.assertEqual(self.u.clock(t, now), expected)

    def test_bar(self):
        self.assertEqual(self.u.bar(0), "░" * 10)
        self.assertEqual(self.u.bar(1), "█" * 10)
        self.assertEqual(self.u.bar(0.59), "█████▉░░░░")
        self.assertEqual(self.u.bar(0.001, 5), "▏░░░░")  # anything left shows
        self.assertEqual(self.u.bar(0.999, 5), "████▉")  # only exactly full looks full

    def test_urgency_colours_have_no_green(self):
        colours = {self.u.URGENCY_COLORS[min(int(f / 20.0 * 11), 10)] for f in range(21)}
        self.assertFalse(colours & {34, 35, 70, 106, 2, 10, 22, 28, 40, 46})

    def test_per_model_limits_are_named_in_violet(self):
        with mock.patch.dict("os.environ", {"TERM": "xterm-256color"}):
            self.assertEqual(self.u.label_color("weekly_scoped"), "38;5;134")
            self.assertEqual(self.u.label_color("weekly_all"), "")
            self.assertEqual(self.u.label_color("session", "2"), "2")
        with mock.patch.dict("os.environ", {"TERM": "xterm", "COLORTERM": ""}):
            self.assertEqual(self.u.label_color("weekly_scoped"), "35")  # magenta in 16 colours
        # Not a colour the time bars use, so the name doesn't read as one of them.
        self.assertNotIn(self.u.SCOPED_COLOR, self.u.URGENCY_COLORS)

    def test_window_lengths(self):
        self.assertEqual(self.u.window_seconds("session"), 5 * 3600)
        self.assertEqual(self.u.window_seconds("weekly_scoped"), 7 * 86400)
        self.assertEqual(self.u.window_seconds("seven_day_opus"), 7 * 86400)
        self.assertIsNone(self.u.window_seconds("iguana_necktie"))


if __name__ == "__main__":
    unittest.main()
