"""hatcheck search: the sessions whose title or conversation has every word, in every profile
and home, with the turns the words are in. Against a throwaway home: no real profiles, Keychain
or network."""
import json
import os
import unittest

from helpers import NOW, FakeHome, iso, rec

A = "aaaaaaaa-0000-4000-8000-000000000001"
B = "bbbbbbbb-0000-4000-8000-000000000002"
C = "cccccccc-0000-4000-8000-000000000003"
D = "dddddddd-0000-4000-8000-000000000004"
X = "019f0000-aaaa-7000-8000-000000000001"


class Search(unittest.TestCase):
    def setUp(self):
        self.h = h = FakeHome()
        self.addCleanup(h.cleanup)
        self.work = h.profile("work")
        self.home = h.profile("home")
        self.api = h.folder("code", "api")
        self.blog = h.folder("code", "blog")
        base = {"cwd": self.api, "sessionId": A, "entrypoint": "cli"}
        talk = [rec(type="user", timestamp=iso(NOW - 200), message={"role": "user", "content":
                    "Add a rate limiter to /login so a script can't hammer it"}, **base),
                rec(type="assistant", timestamp=iso(NOW - 190), message={"role": "assistant", "content": [
                    {"type": "text", "text": "The limiter keeps a Token Bucket per IP, refilled at 5 a second."}]}, **base),
                # What Claude Code adds itself isn't the conversation: a word only there doesn't count.
                rec(type="user", timestamp=iso(NOW - 180), message={"role": "user", "content":
                    "<system-reminder>The user mentioned a ZEBRA.</system-reminder>"}, **base)]
        h.session(self.work, self.api, A, "Harden the login endpoint", 3, extra=talk)
        h.session(self.home, self.blog, B, "Draft a post on rate limiting", 60 * 24 * 9)
        h.session(self.work, self.api, C, "From the app, on the rate limiter", 2, entrypoint="claude-desktop")
        self.codex = h.codex_home("default")
        h.codex_session(self.codex, self.api, X, "Speed up the build", 20, name="Build cache and rate limiter")

    def test_every_word_in_the_title_or_the_conversation_in_either_case(self):
        code, out = self.h.run("search", "RATE", "limiter", cwd=self.api)
        self.assertEqual(code, 0, out)
        self.assertEqual(out.splitlines()[0], 'Sessions with "RATE" and "limiter" (by folder, newest first)')
        self.assertIn("cd " + self.h.shown(self.api) + "\n", out)
        self.assertIn("Harden the login endpoint\n        cl work --resume " + A + "\n", out)
        self.assertIn("        You: Add a rate limiter to /login so a script can't hammer it\n", out)
        self.assertIn("        Claude: The limiter keeps a Token Bucket per IP, refilled at 5 a second.\n", out)
        self.assertIn("Build cache and rate limiter · main\n        codex resume " + X + "\n", out)  # the thread's name alone
        self.assertNotIn("Draft a post", out)  # limiter isn't in it
        self.assertIn("2 sessions in 1 folder, of 3 looked in; 1 more from other hosts may have them (1 Desktop), which --all searches.", out)
        code, out = self.h.run("search", "rate")
        self.assertEqual(code, 0, out)
        self.assertIn("Draft a post on rate limiting · main\n        cl home --resume " + B + "\n", out)
        self.assertIn("3 sessions in 2 folders", out)
        code, out = self.h.run("search", "bucket", "--json")
        self.assertEqual(code, 0, out)
        data = json.loads(out)
        self.assertEqual([s["id"] for s in data["sessions"]], [A])
        self.assertEqual(data["sessions"][0]["turns_matched"], 1)
        self.assertEqual(data["sessions"][0]["matches"][0]["role"], "assistant")
        self.assertIn("Token Bucket", data["sessions"][0]["matches"][0]["text"])
        self.assertEqual((data["searched"], data["matched"], data["more"], data["hidden"]), (3, 1, 0, {}))

    def test_a_word_only_in_what_the_agent_added_itself_doesnt_count(self):
        code, out = self.h.run("search", "zebra")
        self.assertEqual(code, 1, out)
        self.assertEqual(out, 'No session has "zebra" in its title or conversation, of 3 looked in.\n')
        code, out = self.h.run("search", "zebra", "--json")
        self.assertEqual(code, 1, out)
        self.assertEqual(json.loads(out)["sessions"], [])

    def test_sessions_from_other_hosts_arent_read_without_all(self):
        code, out = self.h.run("search", "rate", "--all")
        self.assertEqual(code, 0, out)
        self.assertIn("From the app, on the rate limiter · main · via claude-desktop\n        cl work --resume " + C, out)
        self.assertIn("4 sessions in 2 folders, of 4 looked in.", out)
        self.assertNotIn("--all searches", out)
        data = json.loads(self.h.run("search", "rate", "--json")[1])
        self.assertEqual(data["hidden"], {"Desktop": 1})
        self.assertNotIn("may have them", self.h.run("search", "bucket")[1])  # not in the Desktop one
        self.assertNotIn(C, [s["id"] for s in data["sessions"]])

    def test_here_since_from_and_max_narrow_it(self):
        code, out = self.h.run("search", "rate", "--here", cwd=self.api)
        self.assertEqual(code, 0, out)
        self.assertTrue(out.startswith('Sessions started from this folder with "rate"'), out)
        self.assertIn(A, out)
        self.assertNotIn(B, out)
        code, out = self.h.run("search", "rate", "--since", "1d")
        self.assertTrue(out.startswith('Sessions active in the last 24h with "rate"'), out)
        self.assertNotIn(B, out)  # nine days old
        code, out = self.h.run("search", "rate", "--from", "home")
        self.assertEqual(code, 0, out)
        self.assertIn(B, out)
        self.assertNotIn(A, out)
        self.assertNotIn(X, out)
        code, out = self.h.run("search", "rate", "--from", "codex")
        self.assertEqual(code, 0, out)
        self.assertIn(X, out)
        self.assertNotIn(A, out)
        code, out = self.h.run("search", "rate", "--max", "1")
        self.assertEqual(code, 0, out)
        self.assertIn(A, out)  # the newest
        self.assertNotIn(X, out)
        self.assertIn("3 sessions in 2 folders, of 3 looked in; the 2 oldest not shown, which --max 0 shows;", out)
        code, out = self.h.run("search", "rate", "-m", "x")
        self.assertEqual(code, 2, out)
        self.assertIn("argument -m/--max: expected a whole number (0 for every one)", out)
        code, out = self.h.run("search")
        self.assertEqual(code, 2, out)
        self.assertIn("the following arguments are required: words", out)

    def test_only_and_ignore_agents(self):
        code, out = self.h.run("search", "limiter", "--json", "-o", "claude")
        self.assertEqual(code, 0, out)
        data = json.loads(out)
        self.assertEqual([(s["agent"], s["id"]) for s in data["sessions"]], [("claude", A)])
        code, out = self.h.run("search", "limiter", "--json", "-i", "claude")
        self.assertEqual(code, 0, out)
        data = json.loads(out)
        self.assertEqual([(s["agent"], s["id"]) for s in data["sessions"]], [("codex", X)])
        self.assertEqual(data["hidden"], {})
        code, out = self.h.run("search", "rate", "--from", "codex", "-i", "codex")
        self.assertEqual(code, 1, out)
        self.assertIn("-o/--only or -i/--ignore leaves Codex out", out)

    def test_what_config_toml_leaves_out_isnt_looked_in(self):
        self.h.config("[ignore]\nfolders = [\"~/code/blog\"]\nagents = [\"codex\"]\n")
        code, out = self.h.run("search", "rate")
        self.assertEqual(code, 0, out)
        self.assertNotIn(B, out)
        self.assertNotIn(X, out)
        self.assertIn("1 session in 1 folder, of 1 looked in; 1 more from other hosts may have them (1 Desktop)", out)
        code, out = self.h.run("search", "rate", "--from", "codex")
        self.assertEqual(code, 1, out)
        self.assertIn("config.toml leaves Codex out", out)
        code, out = self.h.run("search", "rate", "--here", cwd=self.blog)
        self.assertEqual(code, 1, out)
        self.assertIn("config.toml leaves this folder out", out)

    def test_a_long_turn_is_shown_around_the_first_word(self):
        base = {"cwd": self.api, "sessionId": D, "entrypoint": "cli"}
        long = "word " * 200 + "the needle is here " + "more " * 200
        talk = [rec(type="user", timestamp=iso(NOW - 100), message={"role": "user", "content": long}, **base)]
        self.h.session(self.work, self.api, D, "A long one", 1, extra=talk)
        code, out = self.h.run("search", "needle", "--max", "1")
        self.assertEqual(code, 0, out)
        line = next(l for l in out.splitlines() if l.startswith("        You: "))
        # Cut between words, with some of what comes before the word for context.
        self.assertRegex(line, r"^        You: …(word )+the needle is here( more)+…$")
        self.assertLess(len(line), 110)
        self.assertEqual(self.h.run("search", "needle")[1].count(D), 1)


if __name__ == "__main__":
    unittest.main()
