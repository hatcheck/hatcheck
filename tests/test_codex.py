"""Codex in claude-recent-sessions and claude-usage: its sessions with their resume commands,
and its ChatGPT plan's limits. Against a throwaway home: no real Codex home, no network."""
import json
import os
import unittest
from unittest import mock

from helpers import NOW, WINDOWS, FakeHome, iso, load, rec, write

A = "aaaaaaaa-0000-4000-8000-000000000001"
X = "019f0000-aaaa-7000-8000-000000000001"  # Codex IDs are version 7 UUIDs
Y = "019f0000-bbbb-7000-8000-000000000002"
Z = "019f0000-cccc-7000-8000-000000000003"


def rate_limits(used_5h=40, used_week=10, limit_id="codex", name=None, plan="plus"):
    """What Codex records after each turn: one limit, with its 5-hour and weekly windows."""
    return {"limit_id": limit_id, "limit_name": name, "plan_type": plan,
            "primary": {"used_percent": used_5h, "window_minutes": 300, "resets_at": int(NOW + 2 * 3600)},
            "secondary": {"used_percent": used_week, "window_minutes": 10080, "resets_at": int(NOW + 3 * 86400)},
            "credits": {"has_credits": False, "unlimited": False, "balance": None}}


class CodexSessions(unittest.TestCase):
    def setUp(self):
        self.h = h = FakeHome()
        self.addCleanup(h.cleanup)
        self.claude = h.profile("default")
        self.codex = h.codex_home()
        self.work = h.codex_home("work")
        self.api = h.folder("code", "api")
        h.session(self.claude, self.api, A, "A Claude Code session", 3)
        h.codex_session(self.codex, self.api, X, "Fix the uploader", 1)
        h.codex_session(self.work, self.api, Y, "Add retries", 2, name="Retries for uploads")
        h.codex_session(self.codex, self.api, Z, "From the app", 4, source="vscode", originator="Codex Desktop")

    def test_listed_with_claude_code_sessions(self):
        code, out = self.h.run("claude-recent-sessions.py", "--no-usage")
        self.assertEqual(code, 0, out)
        self.assertIn("Claude Code and Codex CLI sessions", out)
        self.assertIn("codex resume " + X, out)
        self.assertIn("cx work resume " + Y, out)
        self.assertIn("claude --resume " + A, out)
        self.assertLess(out.index(X), out.index(Y))  # newest first, Claude Code's and Codex's together
        self.assertLess(out.index(Y), out.index(A))
        self.assertIn("Fix the uploader · codex · main", out)  # your prompt, not the context Codex adds
        self.assertIn("Retries for uploads · codex", out)  # a name from /rename wins
        self.assertNotIn(Z, out)
        self.assertIn("Hidden: 1 Codex app (show with --all)", out)
        self.assertIn("~/.codex-work (1)".replace("/", os.sep), out)

    def test_all_and_full(self):
        code, out = self.h.run("claude-recent-sessions.py", "--no-usage", "--all", "--full")
        self.assertIn(Z, out)
        self.assertIn("via Codex app", out)
        if WINDOWS:
            expected = '$env:CODEX_HOME="$HOME\\.codex-work"; codex resume %s; $env:CODEX_HOME=$null' % Y
        else:
            expected = "CODEX_HOME=~/.codex-work codex resume " + Y
        self.assertIn(expected, out)

    def test_sessions_codex_runs_itself_are_not_listed(self):
        self.h.codex_session(self.codex, self.api, "019f0000-dddd-7000-8000-000000000004", "Review this", 1,
                             source={"subagent": {"other": "guardian"}}, originator="codex_cli_rs")
        code, out = self.h.run("claude-recent-sessions.py", "--no-usage", "--all")
        self.assertNotIn("019f0000-dddd", out)
        self.assertNotIn("without messages", out)

    def test_usage_comment_from_codex_session_files(self):
        h = self.h
        h.codex_session(self.work, h.folder("code", "web"), "019f0000-eeee-7000-8000-000000000005", "Style it", 1,
                        rate_limits=rate_limits(40, 10))
        code, out = h.run("claude-recent-sessions.py", "--cached", cwd=h.folder("code", "web"))
        self.assertEqual(code, 0, out)
        self.assertRegex(out, r"cx work resume 019f0000-eeee-\S+ +# 5h 60% ███░░ ↻██░░░ 1h5\dm  week 90% ██████▎ ↻███░░░░ 2d23h")
        self.assertRegex(out, r"Usage from Codex's session files \(--cached\): ~.\.codex-work \dm ago")


class CodexUsage(unittest.TestCase):
    def setUp(self):
        self.h = FakeHome()
        self.addCleanup(self.h.cleanup)
        self.u = load("claude-usage.py")

    def test_snapshot_rows_and_shared_accounts(self):
        h = self.h
        one, two = h.codex_home("default", "acct-1"), h.codex_home("b", "acct-1")
        api = h.folder("code", "api")
        spark = rate_limits(0, 52, "codex_bengalfox", "GPT-5.3-Codex-Spark")
        h.codex_session(one, api, X, "Old", 30, rate_limits=rate_limits(90, 90))
        newest = [rec(timestamp=iso(NOW - 60), type="event_msg",
                      payload={"type": "token_count", "info": None, "rate_limits": spark})]
        h.codex_session(two, api, Y, "New", 2, rate_limits=rate_limits(40, 10), extra=newest)
        code, out = h.run("claude-usage.py", "--cached")
        self.assertEqual(code, 0, out)
        # From the newest limits Codex recorded, which are the Spark ones, a minute ago.
        self.assertIn("~/.codex, ~/.codex-b  ChatGPT plus · cached 1m ago (--cached)".replace("/", os.sep), out)
        self.assertRegex(out, r"session \(5h\) +60% ██████░░░░  ████░░░░░░ resets \S+ \(in 1h5\dm\)")
        self.assertRegex(out, r"week \(all models\) +90% ")
        self.assertRegex(out, r"week \(GPT-5\.3-Codex-Spark\) +48% ")
        self.assertNotIn("acct-1@example.com", out)
        code, out = h.run("claude-usage.py", "--cached", "--email")
        self.assertIn("acct-1@example.com · ChatGPT plus", out)

    def test_live_answer(self):
        body = {"plan_type": "plus", "email": "x", "rate_limit": {"limit_reached": False,
                "primary_window": {"used_percent": 20, "limit_window_seconds": 18000, "reset_at": NOW + 3600},
                "secondary_window": {"used_percent": 80, "limit_window_seconds": 604800, "reset_at": NOW + 86400}},
                "additional_rate_limits": [{"limit_name": "GPT-5.3-Codex-Spark", "metered_feature": "codex_bengalfox",
                                            "rate_limit": {"primary_window": {"used_percent": 95, "limit_window_seconds": 18000,
                                                                              "reset_at": NOW + 60}}}],
                "code_review_rate_limit": None, "credits": {"has_credits": True, "unlimited": False, "balance": "12.50"}}
        usage = self.u.codex_usage_body(body)
        labels = [(self.u.row_label(r), self.u.row_label(r, short=True), r["severity"], self.u.row_window(r))
                  for r in usage["limits"]]
        self.assertEqual(labels, [("session (5h)", "5h", "normal", 18000), ("week (all models)", "week", "warning", 604800),
                                  ("5h (GPT-5.3-Codex-Spark)", "Spark 5h", "critical", 18000)])
        self.assertEqual(self.u.extra_usage_text(usage), "credits: 12.50")
        free = self.u.codex_usage_body({"plan_type": "free", "rate_limit": {"primary_window": {
            "used_percent": 0, "limit_window_seconds": 2592000, "reset_at": NOW + 2592000}}})
        row = free["limits"][0]
        self.assertEqual((self.u.row_label(row), self.u.row_label(row, short=True)), ("month (30 days)", "month"))
        self.assertEqual(self.u.bar_cells(self.u.row_window(row)), 10)
        with mock.patch.dict("os.environ", {"TERM": "xterm-256color"}):
            self.assertEqual(self.u.label_color(usage["limits"][2]["kind"]), "38;5;134")  # violet, like Fable

    def test_logins(self):
        h = self.h
        home = h.codex_home("x", "acct-9", plan="pro", expires=NOW + 86400)
        login = self.u.read_codex_login(home)
        self.assertEqual((login["account_id"], login["user_id"], login["plan"], login["email"]),
                         ("acct-9", "user-acct-9", "pro", "acct-9@example.com"))
        key = h.codex_home("k")
        write(os.path.join(key, "auth.json"), json.dumps({"OPENAI_API_KEY": "sk-test", "tokens": None}))
        self.assertEqual(self.u.read_codex_login(key), {"api_key": True})
        self.assertIsNone(self.u.read_codex_login(h.codex_home("none")))
        # An expired token isn't sent anywhere; the session files are used instead.
        expired = h.codex_home("old", "acct-8")
        h.codex_session(expired, h.folder("a"), X, "x", 5, rate_limits=rate_limits())
        entry, = self.u.load_codex_accounts([expired])
        with mock.patch.object(self.u, "fetch_codex_usage", side_effect=AssertionError("asked the API")):
            usage, saved_at, why = self.u.codex_account_usage(entry)
        self.assertEqual(why, "token expired; run codex in this profile to renew it")
        self.assertAlmostEqual(saved_at, NOW - 300, delta=2)
        self.assertEqual(len(usage["limits"]), 2)


if __name__ == "__main__":
    unittest.main()
