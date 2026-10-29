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

    def test_one_started_on_an_import_is_filed_under_its_folder(self):
        # The printed command adds hatcheck's imports folder with --add-dir, which agy 1.4.7
        # lists first among the conversation's folders; it ran in the session's folder.
        h = self.h
        code, out = h.run("move", A[:8], "agy", "--json")
        imports = os.path.dirname(json.loads(out)["file"])
        started = "5cfa9396-6f85-45ec-91f2-82a298494bd2"
        h.agy_session(self.root, self.api, started, "Analyze Imported Conversation File", 1, added=[imports])
        code, out = h.run("sessions", "--json", "--no-usage")
        self.assertEqual(code, 0, out)
        s = next(s for s in json.loads(out)["sessions"] if s["id"] == started)
        self.assertEqual(s["cwd"], self.api)
        code, out = h.run("open", started[:8], "--print")
        self.assertEqual(out.splitlines(), ["cd " + h.shown(self.api), "agy --conversation " + started])


USAGE_ARGS = ["-p", "/usage", "--output-format", "json"]  # what hatcheck runs agy with for a reading


class AgyUsage(unittest.TestCase):
    def setUp(self):
        self.h = h = FakeHome()
        self.addCleanup(h.cleanup)
        self.root = h.agy_root()
        h.agy_session(self.root, h.folder("code", "api"), G, "Fix the uploader with agy", 1)
        self.bin = h.fake_agy()
        self.cache = os.path.join(h.root, "cache")  # where hatcheck keeps its reading of agy's usage

    def run_usage(self, *args):
        return self.h.run("usage", *args, PATH=self.bin + os.pathsep + self.h.env()["PATH"], HATCHECK_CACHE_DIR=self.cache)

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
        # A reading from the last five minutes is handed on: agy, the slowest to ask, isn't
        # asked by every run.
        code, out = self.run_usage("--json")
        account = json.loads(out)["accounts"][0]
        self.assertEqual((account["agent"], account["profiles"], account["plan"], account["source"], account["why"]),
                         ("agy", [self.root], "Antigravity Pro", "cached", "asked at most once in 5m"))
        self.assertEqual((account["limits"][0]["name"], account["limits"][0]["left_percent"], account["limits"][0]["per_model"]),
                         ("Gemini 4 Argon", 62, True))
        self.assertEqual(len(self.h.agy_runs()), 2)
        code, out = self.run_usage()
        self.assertIn("Antigravity Pro · cached <1m ago (asked at most once in 5m)", out)
        self.backdate_reading(6 * 60)  # six minutes old: agy is asked again
        code, out = self.run_usage()
        self.assertIn("Antigravity Pro · live", out)
        self.assertEqual(len(self.h.agy_runs()), 4)

    def test_the_buckets_agy_1_4_3_answers_with(self):
        # agy 1.4.3 puts each group's limits in buckets, each with a window and a remaining fraction.
        write(os.path.join(self.root, "fake-usage.json"), json.dumps({"groups": [
            {"name": "Gemini Models", "buckets": [
                {"id": "gemini-weekly", "name": "Weekly Limit Remaining", "window": "weekly", "remaining_fraction": 0.62,
                 "reset_time": iso(NOW + 6 * 86400)},
                {"id": "gemini-5h", "name": "Five Hour Limit Remaining", "window": "5h", "remaining_fraction": 1,
                 "reset_time": iso(NOW + 3600)}]},
            {"name": "Claude and GPT models", "buckets": [
                {"id": "3p-weekly", "name": "Weekly Limit Remaining", "window": "weekly", "remaining_fraction": 0.05,
                 "reset_time": iso(NOW + 5 * 86400)}]}]}))
        code, out = self.run_usage()
        self.assertEqual(code, 0, out)
        self.assertIn("Antigravity · live", out)
        self.assertRegex(out, r"Gemini Models weekly +62% █+[▏▎▍▌▋▊▉]?░+  █+[▏▎▍▌▋▊▉]?░+ resets \S+ \S+ \(in 5d23h\)")
        self.assertRegex(out, r"Gemini Models 5h +100% █{10}  █+[▏▎▍▌▋▊▉]?░+ resets \S+ \(in 59m\)")
        self.assertRegex(out, r"Claude and GPT models weekly +5% [█▏▎▍▌▋▊▉]░+")
        code, out = self.run_usage("--json")
        account = json.loads(out)["accounts"][0]
        self.assertEqual([(l["name"], l["left_percent"], l["window_seconds"]) for l in account["limits"]],
                         [("Gemini Models weekly", 62, 7 * 86400), ("Gemini Models 5h", 100, 5 * 3600),
                          ("Claude and GPT models weekly", 5, 7 * 86400)])

    def test_the_buckets_agy_1_4_10_answers_with(self):
        # agy 1.4.10 names one group with a bucket per model and effort tier, and the epoch as the
        # reset of a limit nothing was spent from: a row per model, the tiers folded while they
        # read the same, and no reset ahead for the epoch.
        later = iso(NOW + 3 * 3600)
        epoch = "1970-01-01T00:00:00Z"
        bucket = lambda id_, name, left, reset: {"id": id_, "name": name, "remaining_fraction": left, "reset_time": reset}  # noqa: E731
        write(os.path.join(self.root, "fake-usage.json"), json.dumps({"groups": [{"name": "All Models", "buckets": [
            bucket("gemini-4-argon-high", "Gemini 4 Argon (High)", 1, epoch),
            bucket("gemini-4-argon-medium", "Gemini 4 Argon (Medium)", 1, epoch),
            bucket("gemini-4-argon-low", "Gemini 4 Argon (Low)", 1, epoch),
            bucket("gemini-3.8-flash-high", "Gemini 3.8 Flash (High)", 1, epoch),
            bucket("gemini-3.8-flash-low", "Gemini 3.8 Flash (Low)", 1, epoch),
            bucket("claude-opus-5-5-max", "Claude Opus 5.5 (Max)", 0.25, later),
            bucket("claude-opus-5-5-high", "Claude Opus 5.5 (High)", 0.62, later),
            bucket("claude-opus-5-5-medium", "Claude Opus 5.5 (Medium)", 0.62, later)]}]}))
        code, out = self.run_usage()
        self.assertEqual(code, 0, out)
        self.assertRegex(out, r"\n  Gemini 4 Argon +100% █{10} *\n")  # nothing spent, no reset ahead
        self.assertRegex(out, r"\n  Gemini 3.8 Flash +100% █{10} *\n")
        self.assertRegex(out, r"\n  Claude Opus 5.5 \(Max\) +25% ")
        self.assertRegex(out, r"\n  Claude Opus 5.5 \(High, Medium\) +62% .*resets \S+ \(in 2h59m\)")
        self.assertNotIn("All Models", out)
        self.assertNotIn("after this snapshot", out)
        code, out = self.run_usage("--json")
        account = json.loads(out)["accounts"][0]
        self.assertEqual([(l["name"], l["left_percent"], l["resets_at"] is None) for l in account["limits"]],
                         [("Gemini 4 Argon", 100, True), ("Gemini 3.8 Flash", 100, True),
                          ("Claude Opus 5.5 (Max)", 25, False), ("Claude Opus 5.5 (High, Medium)", 62, False)])
        # The list's one-line comment folds the models that read the same into one meter.
        code, out = self.h.run("sessions", "--cached", PATH=self.bin + os.pathsep + self.h.env()["PATH"],
                               HATCHECK_CACHE_DIR=self.cache)
        self.assertEqual(code, 0, out)
        comment = next(line for line in out.splitlines() if "agy --conversation " + G in line)
        self.assertRegex(comment, r"# Gemini 4 Argon \+1 +100% █+ +Claude Opus 5.5 \(Max\) +25% .*Claude Opus 5.5 \(High, Medium\) +62% ")

    def test_two_hatchecks_ask_agy_once(self):
        """The app's engine and the CLI can ask at once; a second /usage at the same time gets
        a 503 from Google after a minute, so the second hatcheck waits for the first and takes
        the reading it saved."""
        import subprocess
        from helpers import argv_of
        write(os.path.join(self.root, "fake-usage.json"), json.dumps({
            "groups": [{"group_name": "Gemini 4 Argon", "quota_info": {"remaining_fraction": 0.5, "reset_time": iso(NOW + 3600)}}]}))
        write(os.path.join(self.root, "fake-slow"), "")
        env = dict(self.h.env(), PATH=self.bin + os.pathsep + self.h.env()["PATH"], HATCHECK_CACHE_DIR=self.cache)
        runs = [subprocess.Popen(argv_of("usage") + ["--json"], cwd=self.h.root, env=env, stdout=subprocess.PIPE,
                                 stderr=subprocess.PIPE, text=True, encoding="utf-8") for _ in range(2)]
        outs = [p.communicate(timeout=60) for p in runs]
        for p, (out, err) in zip(runs, outs):
            self.assertEqual(p.returncode, 0, out + err)
            account = json.loads(out)["accounts"][0]
            self.assertEqual(account["limits"][0]["left_percent"], 50, out)
            self.assertIn(account["source"], ("live", "cached"))
        self.assertEqual([r["args"] for r in self.h.agy_runs()], [["models"], USAGE_ARGS])  # asked once
        self.assertEqual(sorted(a["source"] for a in [json.loads(o)["accounts"][0] for o, _ in outs]), ["cached", "live"])
        cached = next(json.loads(o)["accounts"][0] for o, _ in outs if json.loads(o)["accounts"][0]["source"] == "cached")
        self.assertEqual(cached["why"], "asked at most once in 5m")

    def test_hatchecks_stdin_never_reaches_agy(self):
        """A status bar or the app's engine runs hatcheck with a pipe on stdin that stays open;
        agy's print mode waits on such a pipe to its end, so hatcheck gives it none (a guard of
        what run_within always did)."""
        import subprocess
        from helpers import argv_of
        write(os.path.join(self.root, "fake-usage.json"), json.dumps({
            "groups": [{"group_name": "Gemini 4 Argon", "quota_info": {"remaining_fraction": 0.5, "reset_time": iso(NOW + 3600)}}]}))
        r, w = os.pipe()  # held open for the whole run
        try:
            env = dict(self.h.env(), PATH=self.bin + os.pathsep + self.h.env()["PATH"], HATCHECK_CACHE_DIR=self.cache)
            p = subprocess.run(argv_of("usage"), stdin=r, cwd=self.h.root, env=env, capture_output=True, text=True,
                               encoding="utf-8", timeout=30)
        finally:
            os.close(w)
            os.close(r)
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertIn("Antigravity · live", p.stdout)

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
        self.backdate_reading(10 * 60)  # the reading is ten minutes old now: agy is asked again
        code, out = self.run_usage()
        self.assertEqual(code, 0, out)
        self.assertIn("Antigravity Pro · cached", out)  # the last reading, and why there's no new one
        self.assertIn("(agy isn't signed in; run agy to sign in)", out)
        self.assertEqual([r["args"] for r in self.h.agy_runs()], [["models"], USAGE_ARGS, ["models"]])

    def backdate_reading(self, seconds):
        """Make the reading hatcheck kept of agy's usage `seconds` older."""
        path = os.path.join(self.cache, "agy-usage", "last-usage.json")
        with open(path, encoding="utf-8") as fh:
            kept = json.load(fh)
        for entry in kept.values():
            entry["saved_at"] -= seconds
        write(path, json.dumps(kept))

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
        # After the best Claude Code account, the other agents' best: agy's folder with what its
        # /usage reported (a fake agy here), and the command that starts agy.
        h = self.h
        h.profile("default", "acct-1", usage={"limits": [
            {"kind": "weekly_scoped", "group": "weekly", "percent": 40, "severity": "normal",
             "resets_at": iso(NOW + 2 * 86400), "scope": {"model": {"display_name": "Fable"}}}]})
        write(os.path.join(self.root, "fake-usage.json"), json.dumps({
            "groups": [{"group_name": "Gemini 4 Argon", "quota_info": {"remaining_fraction": 0.62, "reset_time": iso(NOW + 5 * 3600)}}],
            "plan_info": {"tier_display_name": "Pro"}}))
        code, out = h.run("best", PATH=h.fake_agy() + os.pathsep + h.env()["PATH"])
        self.assertEqual(code, 0, out)
        self.assertIn("Best Claude Code account now: " + h.shown(self.claude), out)
        self.assertIn("Antigravity CLI: %s  Gemini 4 Argon 62%%" % h.shown(self.root), out)
        self.assertIn("→ agy", out)
        code, out = h.run("best", "--cached")  # the reading hatcheck kept, since agy saves none
        self.assertEqual(code, 0, out)
        self.assertIn("Antigravity CLI: %s  Gemini 4 Argon 62%%" % h.shown(self.root), out)
        self.assertIn("cached", out.split("Antigravity CLI:")[1])


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
