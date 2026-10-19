"""Antigravity CLI (agy) in hatcheck: its conversations listed with their resume commands,
found, shown, converted for Claude Code, and a Claude Code session converted into a file for
agy to read; its usage through a fake agy. Against a throwaway home: no real agy folder, no
network."""
import json
import os
import re
import unittest

from helpers import NOW, WINDOWS, AgyOpen, FakeHome, iso, write

A = "aaaaaaaa-0000-4000-8000-000000000001"  # a Claude Code session
G = "c134893c-20f5-48ea-bc9b-7f7c9271bc4e"  # agy's conversations
P = "dd0ab9e0-cacb-4299-b352-5ee295ba8dcc"  # one from the Antigravity app
S = "9c48bb65-12f4-4efb-be5a-5140d828f6ec"  # one agy ran itself


class AgySessions(unittest.TestCase):
    def setUp(self):
        self.h = h = FakeHome()
        self.addCleanup(h.cleanup)
        self.claude = h.profile("default")
        self.root = h.agy_root()
        self.api = h.folder("code", "api")
        h.session(self.claude, self.api, A, "A Claude Code session", 3)
        h.agy_session(self.root, self.api, G, "Fix the uploader with agy", 1)
        h.agy_session(self.root, self.api, P, "From the app", 2, app="antigravity")
        h.agy_session(self.root, self.api, S, "A subagent", 1, parent=G)

    def test_listed_with_the_others(self):
        code, out = self.h.run("sessions", "--no-usage")
        self.assertEqual(code, 0, out)
        self.assertIn("Claude Code and Antigravity CLI sessions", out)
        self.assertIn("Fix the uploader with agy\n", out)  # no tag after the title: the command names the agent
        self.assertIn("agy --conversation " + G, out)
        self.assertLess(out.index(G), out.index(A))  # newest first, with Claude Code's
        self.assertNotIn(P, out)
        self.assertNotIn(S, out)
        self.assertIn("Hidden: 1 Antigravity app (show with --all)", out)
        self.assertIn(self.h.shown(self.root) + " (1)", out)
        code, out = self.h.run("sessions", "--no-usage", "--all")
        self.assertIn("via Antigravity app", out)
        self.assertNotIn(S, out)  # one agy ran itself is never listed

    def test_json(self):
        code, out = self.h.run("sessions", "--json", "--no-usage")
        self.assertEqual(code, 0, out)
        g = next(s for s in json.loads(out)["sessions"] if s["id"] == G)
        self.assertEqual((g["agent"], g["profile"], g["cwd"], g["entrypoint"], g["resume"]),
                         ("agy", self.root, self.api, "cli", "agy --conversation " + G))
        self.assertEqual((g["model"], g["branch"], g["open"]), (None, None, None))

    def test_where_show_and_open(self):
        h = self.h
        code, out = h.run("where", G[:8])
        self.assertEqual(code, 0, out)
        self.assertIn(os.path.join(self.root, "conversations", G + ".db"), out)
        self.assertIn(os.path.join(self.root, "brain", G), out)
        code, out = h.run("where", G[:8], "--json")
        copy = json.loads(out)["sessions"][0]
        self.assertEqual((copy["agent"], copy["id"], copy["profile"], copy["archived"]), ("agy", G, self.root, False))
        self.assertEqual(copy["data_folder"], os.path.join(self.root, "brain", G))
        code, out = h.run("show", G[:8])
        self.assertEqual(code, 0, out)
        self.assertIn('Session c134893c "Fix the uploader with agy" in %s, 2 turns' % h.shown(self.root), out)
        self.assertIn("[external_agent_tool_call: run_command]", out)
        self.assertIn("CommandLine: ls src", out)
        self.assertIn("main.py", out)  # the tool's output
        self.assertIn("Done: Fix the uploader with agy", out)
        self.assertNotIn("kept to itself", out)  # agy's thinking
        self.assertNotIn("checkpoint of agy", out)  # agy's own steps
        self.assertIn("Left out: Antigravity's thinking, images, artifacts and subagent conversations.", out)
        code, out = h.run("open", G[:8], "--print")
        self.assertEqual(out.splitlines(), ["cd " + h.shown(self.api), "agy --conversation " + G])

    @unittest.skipIf(WINDOWS, "agy's lock isn't checked on Windows")
    def test_an_open_conversation_is_refused(self):
        with AgyOpen(self.root, G):
            code, out = self.h.run("move", G[:8], "default")
        self.assertEqual(code, 1, out)
        self.assertIn("Antigravity session c134893c is open in agy; quit it first", out)
        # The lock file agy leaves behind doesn't count: only a lock that's held does.
        code, out = self.h.run("move", G[:8], "default", "--dry-run")
        self.assertEqual(code, 0, out)

    def test_converted_for_claude_code(self):
        h = self.h
        code, out = h.run("move", G[:8], "default")
        self.assertEqual(code, 0, out)
        self.assertIn('Converted Antigravity CLI session c134893c "Fix the uploader with agy" from %s into a Claude Code '
                      'session in %s:' % (h.shown(self.root), h.shown(self.claude)), out)
        self.assertIn("conversation               1 of your messages, with the replies", out)
        self.assertIn("tool calls                 1, as text with their output shortened", out)
        self.assertIn("The original stays in " + h.shown(self.root), out)
        new = re.search(r"claude --resume (\S+)", out).group(1)
        path = os.path.join(self.claude, "projects", re.sub(r"[^A-Za-z0-9]", "-", self.api), new + ".jsonl")
        self.assertTrue(os.path.exists(path), path)
        self.assertTrue(os.path.exists(os.path.join(self.root, "conversations", G + ".db")))  # kept
        with open(path, encoding="utf-8") as fh:
            text = fh.read()
        self.assertIn("[Imported from Antigravity CLI session " + G, text)
        self.assertIn("ls src", text)
        self.assertNotIn("kept to itself", text)

    def test_claude_code_converted_for_agy(self):
        h = self.h
        code, out = h.run("move", A[:8], "agy", "--json")
        self.assertEqual(code, 0, out)
        d = json.loads(out)
        self.assertEqual((d["action"], d["agent"], d["target_agent"], d["new_session"], d["target"]),
                         ("convert", "claude", "agy", None, self.root))
        self.assertTrue(d["file"].endswith(".md") and os.path.exists(d["file"]), d["file"])
        self.assertTrue(d["resume"].startswith("agy --add-dir "), d["resume"])
        self.assertIn(" -i ", d["resume"])
        self.assertIn(d["file"], d["resume"])
        with open(d["file"], encoding="utf-8") as fh:
            text = fh.read()
        self.assertTrue(text.startswith("# Conversation imported from Claude Code session " + A), text[:80])
        self.assertIn("## You", text)
        self.assertIn("A Claude Code session", text)
        self.assertIn("## Claude", text)
        self.assertTrue(os.path.exists(h.session(self.claude, self.api, A, "A Claude Code session", 3)))  # kept
        code, out = h.run("move", A[:8], "agy", "--dry-run")
        self.assertEqual(code, 0, out)
        self.assertIn('Would convert Claude Code session aaaaaaaa "A Claude Code session" from %s into a conversation for '
                      'Antigravity CLI in ' % h.shown(self.claude), out)
        self.assertIn("so the session starts when agy reads the file.", out)
        self.assertIn("Then start it with:", out)
        self.assertIn("agy --add-dir", out)
        self.assertIn("<new id>.md", out)
        code, out = h.run("fork", A[:8], "agy", "--json")  # a fork of one is the same conversion
        self.assertEqual(json.loads(out)["target_agent"], "agy")


USAGE_ARGS = ["-p", "/usage", "--output-format", "json"]  # what hatcheck runs agy with for a reading


class AgyUsage(unittest.TestCase):
    def setUp(self):
        self.h = h = FakeHome()
        self.addCleanup(h.cleanup)
        self.root = h.agy_root()
        h.agy_session(self.root, h.folder("code", "api"), G, "Fix the uploader with agy", 1)
        self.bin = h.fake_agy()

    def run_usage(self, *args):
        return self.h.run("usage", *args, PATH=self.bin + os.pathsep + self.h.env()["PATH"])

    def test_live_from_agy_then_cached(self):
        write(os.path.join(self.root, "fake-usage.json"), json.dumps({
            "groups": [{"group_name": "Gemini 4 Argon", "quota_info": {"remaining_fraction": 0.62, "reset_time": iso(NOW + 5 * 3600)}}],
            "plan_info": {"tier_display_name": "Pro"}}))
        code, out = self.run_usage()
        self.assertEqual(code, 0, out)
        self.assertIn(self.h.shown(self.root) + "  Antigravity Pro · live", out)
        self.assertRegex(out, r"Gemini 4 Argon +62% █\S* +resets \S+ \(in 4h5\dm\)")
        runs = self.h.agy_runs()
        self.assertEqual([r["args"] for r in runs], [["models"], USAGE_ARGS])  # signed in? only then /usage
        self.assertTrue(all(r["cwd"].endswith("agy-usage") for r in runs), runs)  # a folder of hatcheck's own
        code, out = self.run_usage("--cached")  # the reading hatcheck kept, since agy saves none
        self.assertEqual(code, 0, out)
        self.assertIn("Antigravity Pro · cached", out)
        self.assertIn("(--cached)", out)
        self.assertEqual(len(self.h.agy_runs()), 2)
        code, out = self.run_usage("--json")
        account = json.loads(out)["accounts"][0]
        self.assertEqual((account["agent"], account["profiles"], account["plan"], account["source"]),
                         ("agy", [self.root], "Antigravity Pro", "live"))
        self.assertEqual((account["limits"][0]["name"], account["limits"][0]["left_percent"], account["limits"][0]["per_model"]),
                         ("Gemini 4 Argon", 62, True))

    def test_no_quota_groups(self):
        code, out = self.run_usage()
        self.assertEqual(code, 0, out)
        self.assertIn("Antigravity · no data (agy's /usage reported no quota groups)", out)

    def test_signed_out_is_asked_for_nothing_else(self):
        # Asked for anything without a sign-in, agy's print mode opens the browser on Google's
        # sign-in page and waits; `agy models` refuses at once, so it's asked first.
        write(os.path.join(self.root, "fake-usage.json"), json.dumps({
            "groups": [{"group_name": "Gemini 4 Argon", "quota_info": {"remaining_fraction": 0.62, "reset_time": iso(NOW + 5 * 3600)}}],
            "plan_info": {"tier_display_name": "Pro"}}))
        code, out = self.run_usage()
        self.assertEqual(code, 0, out)
        self.assertIn("Antigravity Pro · live", out)
        write(os.path.join(self.root, "fake-signed-out"), "")
        code, out = self.run_usage()
        self.assertEqual(code, 0, out)
        self.assertIn("Antigravity Pro · cached", out)  # the last reading, and why there's no new one
        self.assertIn("(agy isn't signed in; run agy to sign in)", out)
        self.assertEqual([r["args"] for r in self.h.agy_runs()], [["models"], USAGE_ARGS, ["models"]])

    def test_without_agy(self):
        code, out = self.h.run("usage")
        self.assertIn("no data (agy isn't on your PATH)", out)


class AgyElsewhere(unittest.TestCase):
    def setUp(self):
        self.h = h = FakeHome()
        self.addCleanup(h.cleanup)
        self.claude = h.profile("default")
        self.root = h.agy_root()

    def test_relocate_leaves_agy_sessions_alone(self):
        h = self.h
        api, api2 = h.folder("code", "api"), h.folder("code", "api2")
        h.session(self.claude, api, A, "Old one", 70)
        h.agy_session(self.root, api, G, "In agy", 70)
        code, out = h.run("relocate", api, api2)
        self.assertEqual(code, 0, out)
        self.assertIn("Relocated 1 session from", out)
        self.assertIn("Left alone: Antigravity CLI session c134893c in %s keeps the folder in agy's database, which stays "
                      "as it was." % h.shown(self.root), out)
        api3, api4 = h.folder("code", "api3"), h.folder("code", "api4")
        h.agy_session(self.root, api3, P, "Only agy", 70)
        code, out = h.run("relocate", api3, api4)
        self.assertEqual(code, 1, out)
        self.assertIn("No sessions to move ran in %s (only Antigravity CLI's, which stay as they are)" % h.shown(api3), out)

    def test_bind_default_binds_agy_too(self):
        h = self.h
        api = h.folder("code", "api")
        code, out = h.run("bind", "default", cwd=api)
        self.assertEqual(code, 0, out)
        self.assertIn("Claude Code %s · Antigravity CLI %s." % (h.shown(self.claude), h.shown(self.root)), out)
        code, out = h.run("bind", "--agy", "default", cwd=api)
        self.assertEqual(code, 0, out)
        self.assertIn("Antigravity CLI " + h.shown(self.root), out)
        code, out = h.run("bind", "--agy", "nope", cwd=api)
        self.assertEqual(code, 1, out)
        self.assertIn("isn't an Antigravity CLI folder", out)

    def test_best_lists_agy(self):
        code, out = self.h.run("best", "--cached")
        self.assertEqual(code, 0, out)
        self.assertIn("Antigravity CLI, best first", out)
        self.assertIn(self.h.shown(self.root), out)
        self.assertIn("→ agy", out)


D = "b3ec2639-7291-473f-b317-d5aba5708a97"  # the Antigravity app's conversations
E = "e038266b-07c9-4a61-92e7-8d38f8d8d1c9"


class AntigravityApp(unittest.TestCase):
    """The Antigravity app's folder beside agy's: each index mentions some of the other's
    conversations, and each is read from the folder it belongs to."""

    def setUp(self):
        self.h = h = FakeHome()
        self.addCleanup(h.cleanup)
        self.claude = h.profile("default")
        self.cli = h.agy_root()
        self.app = h.agy_root(app=True)
        self.api = h.folder("code", "api")
        h.session(self.claude, self.api, A, "A Claude Code session", 3)
        h.agy_session(self.cli, self.api, G, "With agy", 1)
        # agy's index mentions an app conversation, as it was in July; the app's index has it as it is.
        h.agy_session(self.cli, self.api, D, "From the app, as agy's index lists it", 50, app="antigravity", transcript=False)
        h.agy_session(self.app, self.api, D, "From the app", 2)
        # And the app's index mentions one of agy's.
        h.agy_session(self.app, self.api, G, "With agy, as the app's index lists it", 40, app="antigravity-cli", transcript=False)
        h.agy_session(self.app, self.api, E, "Another in the app", 4)

    def test_each_from_its_own_folder(self):
        h = self.h
        code, out = h.run("sessions", "--no-usage")
        self.assertEqual(code, 0, out)
        self.assertIn("With agy\n", out)
        self.assertNotIn("index lists it", out)
        self.assertNotIn(D, out)
        self.assertIn("Hidden: 2 Antigravity app (show with --all)", out)
        self.assertIn(h.shown(self.cli) + " (1)", out)
        self.assertNotIn(h.shown(self.app) + " (", out)  # named only when one of its conversations is listed
        code, out = h.run("sessions", "--no-usage", "--all")
        self.assertEqual(code, 0, out)
        self.assertIn("From the app · via Antigravity app", out)
        self.assertIn("Another in the app · via Antigravity app", out)
        self.assertNotIn("index lists it", out)
        self.assertEqual(out.count("agy --conversation " + G), 1)
        self.assertIn(h.shown(self.app) + " (2)", out)
        lines = out.splitlines()
        k = next(i for i, line in enumerate(lines) if "From the app · via" in line)
        self.assertEqual(lines[k + 1].strip(), "agy")  # agy imports the app's from its picker; the line starts it

    def test_json(self):
        code, out = self.h.run("sessions", "--json", "--no-usage", "--all")
        d = next(s for s in json.loads(out)["sessions"] if s["id"] == D)
        self.assertEqual((d["agent"], d["profile"], d["entrypoint"], d["resume"], d["title"]),
                         ("agy", self.app, "Antigravity app", "agy", "From the app"))

    def test_where_show_and_convert(self):
        h = self.h
        code, out = h.run("where", D[:8])
        self.assertEqual(code, 0, out)
        self.assertEqual(out.splitlines(), [os.path.join(self.app, "conversations", D + ".db"), os.path.join(self.app, "brain", D)])
        code, out = h.run("show", D[:8], "--last", "1")
        self.assertEqual(code, 0, out)
        self.assertIn('Session b3ec2639 "From the app" in %s, 2 turns' % h.shown(self.app), out)
        code, out = h.run("move", D[:8], "default", "--dry-run")
        self.assertEqual(code, 0, out)
        self.assertIn('Would convert Antigravity CLI session b3ec2639 "From the app" from %s into a Claude Code session in %s:'
                      % (h.shown(self.app), h.shown(self.claude)), out)
        code, out = h.run("move", A[:8], self.app)
        self.assertEqual(code, 1, out)
        self.assertIn("is the Antigravity app's folder, which only the app writes; convert the session for agy instead: hatcheck move %s agy" % A[:8], out)

    def test_usage_asks_agy_once_for_both(self):
        h = self.h
        fakebin = h.fake_agy()
        write(os.path.join(self.cli, "fake-usage.json"), json.dumps({
            "groups": [{"group_name": "Gemini 4 Argon", "quota_info": {"remaining_fraction": 0.5, "reset_time": iso(NOW + 3600)}}]}))
        code, out = h.run("sessions", "--all", PATH=fakebin + os.pathsep + h.env()["PATH"])
        self.assertEqual(code, 0, out)
        self.assertEqual([r["args"] for r in h.agy_runs()], [["models"], USAGE_ARGS])
        self.assertNotRegex(out, r"antigravity  ")  # the app's folder is no account of its own
        code, out = h.run("usage", PATH=fakebin + os.pathsep + h.env()["PATH"])
        self.assertEqual(out.count("Antigravity"), 1, out)


if __name__ == "__main__":
    unittest.main()


@unittest.skipIf(WINDOWS, "agy's lock isn't checked on Windows")
class AgyOpenInTheList(unittest.TestCase):
    def test_marked_open_from_the_lock(self):
        h = FakeHome()
        self.addCleanup(h.cleanup)
        root = h.agy_root()
        api = h.folder("code", "api")
        h.agy_session(root, api, G, "Still going", 30 * 60)  # older than the window
        code, out = h.run("sessions", "--no-usage")
        self.assertNotIn(G, out)
        with AgyOpen(root, G):
            code, out = h.run("sessions", "--no-usage")
            self.assertEqual(code, 0, out)
            self.assertIn("Still going · ● open in agy", out)
            code, out = h.run("sessions", "--no-usage", "--json")
            g = next(s for s in json.loads(out)["sessions"] if s["id"] == G)
            self.assertEqual(g["open"], {"pid": None, "status": None, "in": "agy"})
