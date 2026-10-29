"""hatcheck sessions (plain hatcheck) against a throwaway home."""
import json
import os
import re
import unittest
from unittest import mock

from helpers import NOW, WINDOWS, FakeHome, iso, rec

A = "aaaaaaaa-0000-4000-8000-000000000001"
B = "bbbbbbbb-0000-4000-8000-000000000002"
C = "cccccccc-0000-4000-8000-000000000003"
D = "dddddddd-0000-4000-8000-000000000004"
E = "eeeeeeee-0000-4000-8000-000000000005"
F = "ffffffff-0000-4000-8000-000000000006"
G = "abababab-0000-4000-8000-000000000007"
H = "cdcdcdcd-0000-4000-8000-000000000008"
SCRIPT = "sessions"


def limits(session_used, weekly_used):
    return {"limits": [
        {"kind": "session", "group": "session", "percent": session_used, "severity": "normal",
         "resets_at": iso(NOW + 2 * 3600)},
        {"kind": "weekly_all", "group": "weekly", "percent": weekly_used, "severity": "normal",
         "resets_at": iso(NOW + 3 * 86400)}]}


class RecentSessions(unittest.TestCase):
    def setUp(self):
        self.h = FakeHome()
        self.addCleanup(self.h.cleanup)
        self.default = self.h.profile("default")
        self.work = self.h.profile("work")
        self.api = self.h.folder("code", "api")
        self.blog = self.h.folder("code", "blog")
        self.h.session(self.work, self.api, A, "Fix the flaky login test", 4)
        self.h.session(self.default, self.api, B, "Add rate limiting", 180)
        self.h.session(self.work, self.blog, C, "Draft a post", 600)
        self.h.session(self.default, self.blog, D, "Two days old", 48 * 60)
        self.h.session(self.default, self.blog, E, "From the Desktop app", 30, entrypoint="claude-desktop")

    def test_lists_recent_sessions_by_folder(self):
        code, out = self.h.run(SCRIPT, "--no-usage")
        self.assertEqual(code, 0, out)
        api, blog = "cd " + self.h.shown(self.api), "cd " + self.h.shown(self.blog)
        self.assertIn(api, out)
        self.assertIn(blog, out)
        self.assertLess(out.index(api), out.index(blog))  # folders A-Z
        self.assertIn("cl work --resume " + A, out)
        self.assertIn("claude --resume " + B, out)
        self.assertLess(out.index(A), out.index(B))  # newest first
        self.assertNotIn(D, out)  # older than 24 hours
        self.assertNotIn(E, out)  # Desktop sessions are hidden by default
        self.assertIn("Hidden: 1 Desktop (show with --all)", out)

    def test_hours_and_all(self):
        code, out = self.h.run(SCRIPT, "3d", "--all", "--no-usage")
        self.assertEqual(code, 0, out)
        self.assertIn(D, out)
        self.assertIn(E, out)
        self.assertIn("via claude-desktop", out)

    def test_cost_when_the_agent_wrote_one_down(self):
        # Claude Code appends cost-state records with the session's running total; the newest
        # is the total, as Claude Code takes it when the session is resumed.
        folder = self.h.folder("code", "cost")
        cost = lambda sid, usd: rec(type="cost-state", sessionId=sid, totalCostUSD=usd, totalAPIDuration=1146609,  # noqa: E731
                                    startTime=int((NOW - 600) * 1000), hasUnknownModelCost=False,
                                    modelUsage={"claude-sonnet-5": {"inputTokens": 6551, "outputTokens": 83883, "costUSD": usd}})
        self.h.session(self.work, folder, F, "Costly", 6, extra=[cost(F, 1.5), cost(F, 5.0095065)])
        self.h.session(self.work, folder, G, "Tiny", 7, extra=[cost(G, 0.0042)])
        self.h.session(self.work, folder, H, "Free", 8, extra=[cost(H, 0)])
        code, out = self.h.run(SCRIPT, "--no-usage")
        self.assertEqual(code, 0, out)
        self.assertIn("Costly · main · $5.01\n", out)
        self.assertIn("Tiny · main · <$0.01\n", out)
        self.assertIn("Free · main\n", out)  # priced at nothing: not shown
        self.assertIn("Fix the flaky login test · main\n", out)  # no record at all
        code, out = self.h.run(SCRIPT, "--no-usage", "--json")
        self.assertEqual(code, 0, out)
        costs = {s["id"]: s["cost_usd"] for s in json.loads(out)["sessions"]}
        self.assertEqual((costs[F], costs[G], costs[H], costs[A]), (5.0095065, 0.0042, None, None))

    def test_details_and_by_cost(self):
        # Claude Code's cost-state says what the total is made of, a model at a time, with the
        # durations and the lines changed: --details shows it, and --by-cost puts the costliest first.
        folder = self.h.folder("code", "cost")
        state = rec(type="cost-state", sessionId=F, totalCostUSD=5.0095065, totalAPIDuration=1146609,
                    totalDuration=18300000, totalToolDuration=80567, totalLinesAdded=130, totalLinesRemoved=7,
                    startTime=int((NOW - 600) * 1000), hasUnknownModelCost=False,
                    modelUsage={"claude-haiku-4-5": {"inputTokens": 1092, "outputTokens": 137, "thinkingTokens": 0,
                                                     "cacheReadInputTokens": 0, "cacheCreationInputTokens": 0,
                                                     "webSearchRequests": 0, "costUSD": 0.001777},
                                "claude-sonnet-5": {"inputTokens": 6551, "outputTokens": 83883, "thinkingTokens": 42938,
                                                    "cacheReadInputTokens": 12077475, "cacheCreationInputTokens": 696121,
                                                    "webSearchRequests": 0, "costUSD": 5.0077}})
        self.h.session(self.work, folder, F, "Costly", 6, extra=[state])
        self.h.session(self.work, folder, G, "Cheap", 2, extra=[rec(type="cost-state", sessionId=G, totalCostUSD=0.25)])
        code, out = self.h.run(SCRIPT, "--no-usage", "--by-cost", "--details")
        self.assertEqual(code, 0, out)
        self.assertIn("costliest first", out)
        cost, api, blog = ("cd " + self.h.shown(f) for f in (folder, self.api, self.blog))
        self.assertIn(cost + "  # $5.26 in 2 sessions\n", out)  # what the folder's sessions cost together
        self.assertLess(out.index(cost), out.index(api))  # the costliest folder first, then A-Z
        self.assertLess(out.index(api), out.index(blog))
        self.assertLess(out.index("Costly"), out.index("Cheap"))  # the costliest session first, not the newest
        # The columns line up across the whole list: numbers right-aligned, · for none, and the
        # kinds no row has (web searches here) left out.
        self.assertIn("        turns 2  prompts 1  calls 0  API 19m  wall 5h05m  tools 1m  lines +130 -7\n", out)
        self.assertIn("        " + " " * 21 + "cost    in    out  think   read   write\n", out)
        self.assertIn("        claude-sonnet-5     $5.01  6.6k  83.9k  42.9k  12.1m  696.1k\n", out)
        self.assertIn("        claude-haiku-4-5  $0.0018  1.1k    137      ·      ·       ·\n", out)
        self.assertIn("        turns 2  prompts 1  calls 0\n", out)  # a session without a cost-state record
        code, out = self.h.run(SCRIPT, "--no-usage", "--by-cost", "--details", "--json")
        self.assertEqual(code, 0, out)
        data = json.loads(out)
        self.assertEqual(data["sorted_by"], "cost")
        self.assertEqual([s["id"] for s in data["sessions"]][:2], [F, G])
        costly = data["sessions"][0]
        self.assertEqual(costly["turns"], {"total": 2, "prompts": 1, "tool_calls": 0})
        sonnet = next(m for m in costly["spend"]["models"] if m["model"] == "claude-sonnet-5")
        self.assertEqual((sonnet["usd"], sonnet["input"], sonnet["cache_read"]), (5.0077, 6551, 12077475))
        self.assertEqual((costly["spend"]["api_seconds"], costly["spend"]["lines_added"]), (1146.609, 130))
        self.assertIsNone(next(s for s in data["sessions"] if s["id"] == A)["spend"])
        plain = json.loads(self.h.run(SCRIPT, "--no-usage", "--json")[1])
        self.assertEqual(plain["sorted_by"], "time")
        self.assertNotIn("spend", plain["sessions"][0])

    def test_short_flags_bundle_as_pythons_do(self):
        # -dc is --details --by-cost, -qA the quick list of every folder, and -n ends a bundle.
        self.h.session(self.work, self.api, F, "Costly", 6, extra=[rec(type="cost-state", sessionId=F, totalCostUSD=2.5)])
        long_form = self.h.run(SCRIPT, "--no-usage", "--details", "--by-cost", "--all-folders", "-n", "1", cwd=self.api)
        self.assertEqual(self.h.run(SCRIPT, "-qdcAn1", cwd=self.api), long_form)
        self.assertEqual(self.h.run(SCRIPT, "-q", "-d", "-c", "-A", "-n1", cwd=self.api), long_form)
        code, out = long_form
        self.assertEqual(code, 0, out)
        self.assertIn("Costly · main · $2.50\n", out)
        self.assertIn("+2  other sessions", out)  # -n 1 under --by-cost: the cheaper and the unpriced
        short, long_json = (json.loads(self.h.run(SCRIPT, *flags)[1]) for flags in (("-qj",), ("--no-usage", "--json")))
        short.pop("generated_at"), long_json.pop("generated_at")  # a second may pass between the runs
        self.assertEqual(short, long_json)
        code, out = self.h.run(SCRIPT, "-qx")  # a letter that's no flag, as argparse reports it
        self.assertEqual(code, 2)
        self.assertIn("unrecognized arguments: -x", out)

    def test_newest_n_per_folder(self):
        code, out = self.h.run(SCRIPT, "-n", "1", "--no-usage")
        self.assertIn(A, out)
        self.assertNotIn(B, out)
        self.assertIn("+1  older session", out)

    def test_only_the_current_folder(self):
        code, out = self.h.run(SCRIPT, "--no-usage", cwd=self.api)
        self.assertEqual(code, 0, out)
        self.assertIn("in this folder", out)
        self.assertIn(A, out)
        self.assertNotIn(C, out)
        self.assertIn("1 in other folders (show with --all-folders)", out)
        code, out = self.h.run(SCRIPT, "--no-usage", "--all-folders", cwd=self.api)
        self.assertIn(C, out)
        self.assertNotIn("in this folder", out)

    def test_full_commands(self):
        code, out = self.h.run(SCRIPT, "--no-usage", "--full")
        if WINDOWS:
            expected = '$env:CLAUDE_CONFIG_DIR="$HOME\\.claude-work"; claude --resume %s; $env:CLAUDE_CONFIG_DIR=$null' % A
        else:
            expected = "CLAUDE_CONFIG_DIR=~/.claude-work claude --resume " + A
        self.assertIn(expected, out)

    def test_open_session_is_marked(self):
        self.h.register_open(self.work, A)
        code, out = self.h.run(SCRIPT, "--no-usage")
        self.assertIn("● open in pid %d (idle)" % os.getpid(), out)

    def test_open_session_with_another_start_time_is_not(self):
        self.h.register_open(self.work, A)
        path = os.path.join(self.work, "sessions", "%d.json" % os.getpid())
        with open(path, encoding="utf-8") as fh:
            text = fh.read()
        with open(path, "w", encoding="utf-8") as fh:  # a reused PID: same number, other process
            fh.write(re.sub(r'"(procStart|procStartFt)": "[^"]*"', r'"\1": "1"', text))
        code, out = self.h.run(SCRIPT, "--no-usage")
        self.assertNotIn("● open", out)


class Providers(unittest.TestCase):
    """Sessions whose newest reply came from Z.ai or Kimi resume there."""

    def setUp(self):
        self.h = FakeHome()
        self.addCleanup(self.h.cleanup)
        self.default, self.work = self.h.profile("default"), self.h.profile("work")
        api = self.h.folder("code", "api")
        self.h.session(self.work, api, A, "On Kimi", 1, model="k3")
        # Claude Code's own <synthetic> replies don't count.
        synthetic = rec(type="assistant", timestamp=iso(NOW - 90), cwd=api, sessionId=B, entrypoint="cli",
                        gitBranch="main", message={"model": "<synthetic>", "role": "assistant",
                                                   "content": [{"type": "text", "text": "x"}]})
        self.h.session(self.default, api, B, "On GLM", 2, model="glm-5.3", extra=[synthetic])
        self.h.session(self.work, api, C, "On Claude", 3, model="claude-opus-5-5")

    def test_commands_and_model(self):
        code, out = self.h.run(SCRIPT, "--no-usage")
        self.assertEqual(code, 0, out)
        self.assertIn("cl -k work --resume " + A, out)
        self.assertIn("claude-z --resume " + B, out)
        self.assertIn("cl work --resume " + C, out)
        self.assertIn("On Kimi · main · on k3", out)
        self.assertIn("On GLM · main · on glm-5.3", out)
        self.assertNotIn("on claude", out)

    def test_full_commands(self):
        code, out = self.h.run(SCRIPT, "--no-usage", "--full")
        if WINDOWS:
            expected = '$env:CLAUDE_CONFIG_DIR="$HOME\\.claude-work"; claude-k --resume %s; $env:CLAUDE_CONFIG_DIR=$null' % A
        else:
            expected = "CLAUDE_CONFIG_DIR=~/.claude-work claude-k --resume " + A
        self.assertIn(expected, out)
        self.assertIn("claude-z --resume " + B, out)


class UsageComments(unittest.TestCase):
    def setUp(self):
        self.h = FakeHome()
        self.addCleanup(self.h.cleanup)
        self.api = self.h.folder("code", "api")
        # work and work2 are signed in to one account; personal to another with the same numbers.
        for name, account in (("work", "acct-w"), ("work2", "acct-w"), ("personal", "acct-p")):
            profile = self.h.profile(name, account, usage=limits(40, 10), saved_at=NOW - 60)
            setattr(self, name, profile)

    def lines(self, out):
        return [l for l in out.splitlines() if re.match(r" {8}\S", l)]

    def test_bars_numbers_and_dedup_by_account(self):
        self.h.session(self.work, self.api, A, "One", 1)
        self.h.session(self.work2, self.api, B, "Two", 2)
        self.h.session(self.personal, self.api, C, "Three", 3)
        code, out = self.h.run(SCRIPT, "--cached")
        self.assertEqual(code, 0, out)
        first, second, third = self.lines(out)
        self.assertRegex(first, "cl work --resume " + A + r" +# 5h 60% ███░░ ↻██░░░ 1h5\dm  week 90% ██████▎ ↻███░░░░ 2d23h$")
        self.assertTrue(second.endswith("cl work2 --resume " + B), second)  # same account: left out
        self.assertIn("# 5h 60%", third)  # another account, though the numbers match
        self.assertIn("Usage from Claude Code's last snapshot (--cached)", out)

    def test_no_usage(self):
        self.h.session(self.work, self.api, A, "One", 1)
        code, out = self.h.run(SCRIPT, "--no-usage")
        self.assertNotIn("#", "".join(self.lines(out)))


if __name__ == "__main__":
    unittest.main()
