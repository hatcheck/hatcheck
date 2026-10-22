"""hatcheck usage against a throwaway home: the cached numbers, the order, and the live path,
which runs Claude Code's /usage (a fake claude here) and reads the snapshot it saved. No real
profiles, no Keychain, no network."""
import json
import os
import unittest

from helpers import NOW, WINDOWS, FakeHome, iso, write

SCRIPT = "usage"
TEAM = {"organizationType": "claude_team", "userRateLimitTier": "default_claude_max_5x",
        "organizationRateLimitTier": "default_raven"}


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

    def test_rows_bars_and_the_plan(self):
        # The plan comes from the oauthAccount in the profile's .claude.json, never a login.
        self.h.profile("work", "acct-w", usage=limits(), saved_at=NOW - 120, plan=TEAM)
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
        # Past 90% used it goes behind the fresher accounts, though it resets before them.
        names["low"] = h.profile("low", "acct-6", usage=fable(95, 0.3))
        names["spent"] = h.profile("spent", "acct-3", usage=fable(100, 0.5))
        # Fable left, but the weekly limit for all models, which Fable counts against too, is used up.
        names["blocked"] = h.profile("blocked", "acct-4", usage=fable(10, 0.2, weekly_used=100))
        # Saved before its Fable limit reset a day ago: full again, until it resets in 6 days.
        names["stale"] = h.profile("stale", "acct-5", usage=fable(95, -1), saved_at=NOW - 2 * 86400)
        code, out = h.run(SCRIPT, "--cached")
        self.assertEqual(code, 0, out)
        self.assertIn("Sorted by Fable: those with some left first, the soonest reset on top, any past 90% used behind them;", out)
        order = [line.split()[0] for line in out.splitlines() if line.startswith("~")]
        expected = ["soon", "later", "stale", "low", "spent", "blocked", "pro"]
        self.assertEqual(order, [h.shown(names[n]) for n in expected])


class LiveUsage(unittest.TestCase):
    """Live numbers come from running Claude Code's /usage in the profile and reading the
    snapshot it saves, so hatcheck never reads a login."""

    def setUp(self):
        self.h = h = FakeHome()
        self.addCleanup(h.cleanup)
        self.bin = h.fake_claude()
        self.path = self.bin + os.pathsep + (os.environ["PATH"] if WINDOWS else "/bin")
        # work: signed in, and Anthropic answers (fake-usage.json is what it would say)
        self.work = h.profile("work", "acct-w", usage=limits(), saved_at=NOW - 3600, plan=TEAM)
        fresh = limits()
        fresh["limits"][0]["percent"] = 75
        write(os.path.join(self.work, "fake-usage.json"), json.dumps(fresh))

    def test_numbers_come_from_claude_code(self):
        code, out = self.h.run(SCRIPT, PATH=self.path, ANTHROPIC_AUTH_TOKEN="zai-key", CLAUDECODE="1",
                               CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC="1")
        self.assertEqual(code, 0, out)
        self.assertIn("work  team max 5x · live", out)
        self.assertRegex(out, r"session \(5h\) +25% ")  # the new reading, not the hour-old snapshot's 60%
        runs = self.h.claude_runs()
        self.assertEqual(len(runs), 1)
        run = runs[0]
        self.assertEqual(run["args"][:2], ["-p", "/usage"])
        self.assertIn("--no-session-persistence", run["args"])
        self.assertEqual(run["env"].get("CLAUDE_CONFIG_DIR"), self.work)
        self.assertEqual(run["env"].get("DISABLE_TELEMETRY"), "1")
        # Nothing of hatcheck's own surroundings: no provider key, no mark of a session it
        # runs in, and not the setting that would have /usage skip its request.
        for name in ("ANTHROPIC_AUTH_TOKEN", "CLAUDECODE", "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC"):
            self.assertNotIn(name, run["env"])
        self.assertEqual(os.path.basename(run["cwd"]), "claude-usage")  # hatcheck's own folder, in its cache
        self.assertTrue(run["cwd"].startswith(self.h.home), run["cwd"])
        # The snapshot Claude Code saved is what --cached shows next.
        code, out = self.h.run(SCRIPT, "--cached")
        self.assertRegex(out, r"session \(5h\) +25% ")

    def test_default_profile_runs_without_a_config_dir(self):
        default = self.h.profile("default", "acct-d", usage=limits(), saved_at=NOW - 120)  # old enough to ask
        write(os.path.join(default, "fake-usage.json"), json.dumps(limits()))
        code, out = self.h.run(SCRIPT, PATH=self.path, CLAUDE_CONFIG_DIR=self.work)
        self.assertEqual(code, 0, out)
        by_dir = {r["env"].get("CLAUDE_CONFIG_DIR"): r for r in self.h.claude_runs()}
        self.assertIn(None, by_dir)  # ~/.claude: its login is filed under no CLAUDE_CONFIG_DIR
        self.assertIn(self.work, by_dir)
        self.assertIn("~/.claude  ", out.replace(os.sep, "/"))
        self.assertEqual(out.count("· live"), 2)

    def test_nothing_new_keeps_the_snapshot(self):
        # stale: signed in, but /usage printed its old reading (offline); solo: not signed in.
        stale = self.h.profile("stale", "acct-s", usage=limits(), saved_at=NOW - 600, plan=TEAM)
        write(os.path.join(stale, "stale-usage"), "")
        solo = self.h.profile("solo")
        code, out = self.h.run(SCRIPT, PATH=self.path)
        self.assertEqual(code, 0, out)
        self.assertIn("stale  team max 5x · cached 10m ago (Claude Code's /usage couldn't fetch new numbers; offline, or rate limited?)", out)
        self.assertIn("solo  not signed in · no data (not signed in)", out)
        self.assertIn("work  team max 5x · live", out)

    def test_a_reading_under_a_minute_old_is_shown_again(self):
        # Claude Code's /usage shows a snapshot under a minute old again without asking
        # Anthropic (2.1.307), so hatcheck shows it as it is and doesn't run claude for it.
        fresh = self.h.profile("fresh", "acct-f", usage=limits(), plan=TEAM)  # saved just now
        write(os.path.join(fresh, "fake-usage.json"), json.dumps(limits()))
        code, out = self.h.run(SCRIPT, PATH=self.path)
        self.assertEqual(code, 0, out)
        self.assertIn("fresh  team max 5x · cached <1m ago (Claude Code's /usage reuses a reading under a minute old)", out)
        self.assertIn("work  team max 5x · live", out)  # its snapshot was an hour old: asked
        self.assertEqual([r["env"].get("CLAUDE_CONFIG_DIR") for r in self.h.claude_runs()], [self.work])
        code, out = self.h.run(SCRIPT, "--json", PATH=self.path)
        account = next(a for a in json.loads(out)["accounts"] if a["profiles"] == [fresh])
        self.assertEqual((account["source"], account["why"]),
                         ("cached", "Claude Code's /usage reuses a reading under a minute old"))
        # Within a minute of its run, work's new reading is shown again the same way.
        self.assertEqual(len(self.h.claude_runs()), 1)
        code, out = self.h.run(SCRIPT, PATH=self.path)
        self.assertIn("work  team max 5x · cached <1m ago (Claude Code's /usage reuses a reading under a minute old)", out)
        self.assertEqual(len(self.h.claude_runs()), 1)

    def test_without_claude_on_the_path(self):
        code, out = self.h.run(SCRIPT, PATH="/bin" if not WINDOWS else os.path.join(self.h.root, "empty"))
        self.assertEqual(code, 0, out)
        self.assertIn("work  team max 5x · cached 1h00m ago (claude isn't on your PATH)", out)
        self.assertEqual(self.h.claude_runs(), [])


if __name__ == "__main__":
    unittest.main()
