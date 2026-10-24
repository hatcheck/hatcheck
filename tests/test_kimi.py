"""Kimi Code CLI (kimi) in hatcheck: its sessions listed with their resume commands, found,
shown, moved and forked between Kimi homes, converted for Claude Code, and a Claude Code
session converted into a session kimi resumes. Against a throwaway home: no real Kimi home,
no network."""
import json
import os
import re
import unittest

from helpers import FakeHome, kimi_wd_key, write

A = "aaaaaaaa-0000-4000-8000-000000000001"  # a Claude Code session
K = "0752ad88-cc62-4e00-9912-48e4146b7628"  # a Kimi session (kimi calls it session_<this>)
UUID = r"[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}"


def jsonl(path):
    with open(path, encoding="utf-8") as fh:
        return [json.loads(line) for line in fh if line.strip()]


def read_json(path):
    with open(path, encoding="utf-8") as fh:
        return json.load(fh)


class KimiSessions(unittest.TestCase):
    def setUp(self):
        self.h = h = FakeHome()
        self.addCleanup(h.cleanup)
        self.claude = h.profile("default")
        self.home = h.kimi_home()
        self.api = h.folder("code", "api")
        h.session(self.claude, self.api, A, "A Claude Code session", 3)
        self.folder = h.kimi_session(self.home, self.api, K, "Fix the uploader with kimi", 1)

    def test_listed_with_the_others(self):
        code, out = self.h.run("sessions", "--no-usage")
        self.assertEqual(code, 0, out)
        self.assertIn("Claude Code and Kimi Code CLI sessions", out)
        self.assertIn("Fix the uploader with kimi\n", out)
        self.assertIn("kimi --session session_" + K, out)  # the whole ID, as kimi names it
        self.assertLess(out.index(K), out.index(A))  # newest first, with Claude Code's
        self.assertIn(self.h.shown(self.home) + " (1)", out)

    def test_json(self):
        code, out = self.h.run("sessions", "--json", "--no-usage")
        self.assertEqual(code, 0, out)
        k = next(s for s in json.loads(out)["sessions"] if s["id"] == K)
        self.assertEqual((k["agent"], k["profile"], k["cwd"], k["entrypoint"], k["resume"], k["model"]),
                         ("kimi", self.home, self.api, "cli", "kimi --session session_" + K, "kimi-code/k3"))
        self.assertEqual((k["branch"], k["open"]), (None, None))

    def test_where_show_and_open(self):
        h = self.h
        code, out = h.run("where", K[:8])
        self.assertEqual(code, 0, out)
        self.assertEqual(out.strip(), self.folder)  # the session's folder
        code, out = h.run("where", "session_" + K[:8])  # pasted with kimi's prefix
        self.assertEqual(out.strip(), self.folder)
        code, out = h.run("where", K[:8], "--json")
        copy = json.loads(out)["sessions"][0]
        self.assertEqual((copy["agent"], copy["id"], copy["profile"], copy["data_folder"]), ("kimi", K, self.home, self.folder))
        self.assertEqual(copy["file"], os.path.join(self.folder, "agents", "main", "wire.jsonl"))
        code, out = h.run("show", K[:8])
        self.assertEqual(code, 0, out)
        self.assertIn('Session 0752ad88 "Fix the uploader with kimi" in %s, 2 turns' % h.shown(self.home), out)
        self.assertIn("[external_agent_tool_call: Bash]", out)
        self.assertIn("command: ls src", out)
        self.assertIn("main.py", out)  # the tool's output
        self.assertIn("Done: Fix the uploader with kimi", out)
        self.assertNotIn("kept to itself", out)  # kimi's thinking
        self.assertIn("Left out: Kimi's thinking, images and subagent conversations.", out)
        code, out = h.run("open", K[:8], "--print")
        self.assertEqual(out.splitlines(), ["cd " + h.shown(self.api), "kimi --session session_" + K])

    def test_converted_for_claude_code(self):
        h = self.h
        code, out = h.run("move", K[:8], "default")
        self.assertEqual(code, 0, out)
        self.assertIn('Converted Kimi Code CLI session 0752ad88 "Fix the uploader with kimi" from %s into a Claude Code '
                      'session in %s:' % (h.shown(self.home), h.shown(self.claude)), out)
        self.assertIn("conversation               1 of your messages, with the replies", out)
        self.assertIn("tool calls                 1, as text with their output shortened", out)
        self.assertIn("The original stays in " + h.shown(self.home), out)
        new = re.search(r"claude --resume (\S+)", out).group(1)
        path = os.path.join(self.claude, "projects", re.sub(r"[^A-Za-z0-9]", "-", self.api), new + ".jsonl")
        self.assertTrue(os.path.exists(path), path)
        self.assertTrue(os.path.isdir(self.folder))  # kept
        with open(path, encoding="utf-8") as fh:
            text = fh.read()
        self.assertIn("[Imported from Kimi Code CLI session " + K, text)
        self.assertIn("ls src", text)
        self.assertNotIn("kept to itself", text)

    def test_claude_code_converted_for_kimi(self):
        h = self.h
        code, out = h.run("move", A[:8], "kimi", "--dry-run")
        self.assertEqual(code, 0, out)
        self.assertIn('Would convert Claude Code session aaaaaaaa "A Claude Code session" from %s into a Kimi Code CLI '
                      'session in %s:' % (h.shown(self.claude), h.shown(self.home)), out)
        self.assertIn("Then resume it with:", out)
        self.assertEqual(len(os.listdir(os.path.join(self.home, "sessions"))), 1)  # nothing written
        code, out = h.run("move", A[:8], "kimi", "--json")
        self.assertEqual(code, 0, out)
        d = json.loads(out)
        self.assertEqual((d["action"], d["agent"], d["target_agent"], d["target"], d["prompts"]),
                         ("convert", "claude", "kimi", self.home, 1))
        new = d["new_session"]
        self.assertRegex(new, UUID)
        self.assertEqual(d["resume"], "kimi --session session_" + new)
        folder = os.path.join(self.home, "sessions", kimi_wd_key(self.api), "session_" + new)
        self.assertTrue(os.path.isdir(folder), folder)
        state = read_json(os.path.join(folder, "state.json"))
        self.assertEqual((state["id"], state["cwd"], state["title"]), ("session_" + new, self.api, "A Claude Code session"))
        self.assertEqual(state["agents"]["main"]["homedir"], os.path.join(folder, "agents", "main"))
        # The wire as kimi's own importer writes one: a prompt, the messages in the context it
        # rebuilds the model's view from, and the turn's end. Nothing else.
        events = jsonl(os.path.join(folder, "agents", "main", "wire.jsonl"))
        kinds = [e["type"] for e in events]
        self.assertEqual(kinds, ["metadata", "turn.prompt", "context.append_message", "context.append_message", "turn.ended"])
        user, reply = events[2]["message"], events[3]["message"]
        self.assertEqual((user["role"], user["toolCalls"], reply["role"], reply["toolCalls"]), ("user", [], "assistant", []))
        self.assertIn("A Claude Code session", user["content"][0]["text"])
        self.assertTrue(user["content"][0]["text"].startswith("[Imported from Claude Code session " + A))
        self.assertEqual(events[1]["input"], user["content"])
        index = jsonl(os.path.join(self.home, "session_index.jsonl"))
        mine = [e for e in index if e["sessionId"] == "session_" + new]
        self.assertEqual(mine, [{"sessionId": "session_" + new, "sessionDir": folder, "workDir": self.api}])
        self.assertTrue(os.path.exists(h.session(self.claude, self.api, A, "A Claude Code session", 3)))  # kept
        code, out = h.run("sessions", "--no-usage")  # and listed, like kimi's own
        self.assertIn("kimi --session session_" + new, out)

    def test_an_index_without_a_final_newline_gets_one_first(self):
        h = self.h
        index = os.path.join(self.home, "session_index.jsonl")
        with open(index, encoding="utf-8") as fh:
            text = fh.read()
        write(index, text.rstrip("\n"))  # as kimi has left it (MoonshotAI/kimi-code#1925)
        code, out = h.run("move", A[:8], "kimi", "--json")
        self.assertEqual(code, 0, out)
        lines = jsonl(index)  # every line still parses on its own
        self.assertEqual(len(lines), 2)


class KimiMoves(unittest.TestCase):
    """Between Kimi homes a session is moved, copied or forked as a folder, with its index line."""

    def setUp(self):
        self.h = h = FakeHome()
        self.addCleanup(h.cleanup)
        self.home = h.kimi_home()
        self.work = h.kimi_home("work")
        self.api = h.folder("code", "api")
        self.folder = h.kimi_session(self.home, self.api, K, "Fix the uploader with kimi", 1)
        self.copy = os.path.join(self.work, "sessions", kimi_wd_key(self.api), "session_" + K)

    def indexed(self, home):
        path = os.path.join(home, "session_index.jsonl")
        return [e["sessionId"] for e in jsonl(path)] if os.path.exists(path) else []

    def test_moved_to_another_home(self):
        h = self.h
        code, out = h.run("move", K[:8], "kimi:work", "--dry-run")
        self.assertEqual(code, 0, out)
        self.assertIn("Would move Kimi session 0752ad88", out)
        self.assertTrue(os.path.isdir(self.folder) and not os.path.exists(self.copy))  # nothing changed
        code, out = h.run("move", K[:8], "kimi:work")
        self.assertEqual(code, 0, out)
        self.assertIn('Moved Kimi session 0752ad88 "Fix the uploader with kimi" from %s to %s:'
                      % (h.shown(self.home), h.shown(self.work)), out)
        self.assertRegex(out, r"session folder +\d+ files\n")
        self.assertIn("Deleted the original from %s, and its line in the index there." % h.shown(self.home), out)
        self.assertIn("ck work --session session_" + K, out)
        self.assertFalse(os.path.exists(self.folder))
        self.assertTrue(os.path.isdir(self.copy))
        state = read_json(os.path.join(self.copy, "state.json"))
        self.assertEqual(state["agents"]["main"]["homedir"], os.path.join(self.copy, "agents", "main"))
        self.assertEqual(state["id"], "session_" + K)
        self.assertNotIn("forkedFrom", state)
        self.assertEqual(self.indexed(self.work), ["session_" + K])
        self.assertEqual(self.indexed(self.home), [])
        code, out = h.run("sessions", "--no-usage", "--json")
        k = next(s for s in json.loads(out)["sessions"] if s["id"] == K)
        self.assertEqual(k["profile"], self.work)

    def test_a_wrong_line_for_it_in_the_index_there_is_put_right(self):
        # A line left for the session in the other home's index, naming a folder that's gone: kimi
        # resumes through it, so it must name the copy before the original is deleted.
        gone = os.path.join(self.work, "sessions", "wd_gone_000000000000", "session_" + K)
        index = os.path.join(self.work, "session_index.jsonl")
        line = lambda d, w: json.dumps({"sessionId": "session_" + K, "sessionDir": d, "workDir": w},  # noqa: E731
                                       separators=(",", ":"))
        other = json.dumps({"sessionId": "session_other", "sessionDir": "/x", "workDir": "/y"}, separators=(",", ":"))
        write(index, line(gone, "/nowhere") + "\n" + other + "\n")
        code, out = self.h.run("move", K[:8], "kimi:work")
        self.assertEqual(code, 0, out)
        self.assertEqual([(e["sessionId"], e.get("sessionDir")) for e in jsonl(index)],
                         [("session_other", "/x"), ("session_" + K, self.copy)])
        self.assertFalse(os.path.exists(self.folder))

    def test_a_bare_name_means_the_kimi_home_when_there_is_one(self):
        code, out = self.h.run("move", K[:8], "work")  # ~/.kimi-code-work exists, ~/.claude-work doesn't
        self.assertEqual(code, 0, out)
        self.assertIn("Moved Kimi session 0752ad88", out)
        self.assertTrue(os.path.isdir(self.copy))

    def test_copied_then_moved_over_a_copy(self):
        h = self.h
        code, out = h.run("move", K[:8], "kimi:work", "--keep")
        self.assertEqual(code, 0, out)
        self.assertIn("Copied Kimi session 0752ad88", out)
        self.assertIn("The original stays in " + h.shown(self.home), out)
        self.assertTrue(os.path.isdir(self.folder) and os.path.isdir(self.copy))
        self.assertEqual((self.indexed(self.home), self.indexed(self.work)), (["session_" + K], ["session_" + K]))
        code, out = h.run("move", K[:8], "kimi:work")  # the copy there is the same: nothing to copy, the original goes
        self.assertEqual(code, 0, out)
        self.assertIn("(already there)", out)
        self.assertFalse(os.path.exists(self.folder))
        self.assertEqual(self.indexed(self.work), ["session_" + K])

    def test_a_copy_that_went_on_is_never_replaced(self):
        h = self.h
        h.run("move", K[:8], "kimi:work", "--keep")
        wire = os.path.join(self.copy, "agents", "main", "wire.jsonl")
        with open(wire, "a", encoding="utf-8") as fh:  # the session went on in work
            fh.write(json.dumps({"type": "turn.prompt", "agentId": "main", "input": [{"type": "text", "text": "more"}],
                                 "origin": {"kind": "user"}, "promptId": "msg_2", "turnId": 1, "time": 1}) + "\n")
        code, out = h.run("move", K[:8], "kimi:work")
        self.assertEqual(code, 1, out)
        self.assertIn("already has Kimi session 0752ad88", out)
        self.assertIn("the session went on there. This copy is older, so nothing was moved.", out)
        self.assertTrue(os.path.isdir(self.folder))
        code, out = h.run("move", K[:8], "kimi", "--from", "kimi:work")  # the other way: brought up to date
        self.assertEqual(code, 0, out)
        self.assertIn("(updated an older copy)", out)
        self.assertIn("had an older copy of this session", out)
        with open(os.path.join(self.folder, "agents", "main", "wire.jsonl"), encoding="utf-8") as fh:
            self.assertIn('"text": "more"', fh.read())

    def test_forked_into_another_home(self):
        h = self.h
        code, out = h.run("fork", K[:8], "kimi:work")
        self.assertEqual(code, 0, out)
        self.assertIn('Forked Kimi session 0752ad88 "Fix the uploader with kimi" from %s to %s as a new session:'
                      % (h.shown(self.home), h.shown(self.work)), out)
        new = re.search(r"new session +(%s)" % UUID, out).group(1)
        self.assertNotEqual(new, K)
        self.assertIn("The original stays in " + h.shown(self.home), out)
        copy = os.path.join(self.work, "sessions", kimi_wd_key(self.api), "session_" + new)
        self.assertTrue(os.path.isdir(copy), copy)
        state = read_json(os.path.join(copy, "state.json"))
        self.assertEqual((state["id"], state["forkedFrom"]), ("session_" + new, "session_" + K))
        self.assertEqual(state["agents"]["main"]["homedir"], os.path.join(copy, "agents", "main"))
        self.assertTrue(os.path.isdir(self.folder))
        self.assertEqual(self.indexed(self.work), ["session_" + new])

    def test_a_home_that_is_not_there(self):
        code, out = self.h.run("move", K[:8], "kimi:nope")
        self.assertEqual(code, 1, out)
        self.assertIn("isn't a Kimi home. Sign in there first, e.g. with: ck nope login", out)


class KimiUsage(unittest.TestCase):
    """hatcheck usage for Kimi homes: the token is sent nowhere when it has expired, and a home
    without one isn't signed in; what's shown says why."""

    def setUp(self):
        self.h = h = FakeHome()
        self.addCleanup(h.cleanup)

    def test_not_signed_in_and_expired(self):
        h = self.h
        home = h.kimi_home()
        work = h.kimi_home("work", signed_in=True)
        code, out = h.run("usage")
        self.assertEqual(code, 0, out)
        self.assertIn("%s  Kimi Code · no data (not signed in)" % h.shown(home), out)
        self.assertIn("%s  Kimi Code · no data (token expired; run kimi in this home to renew it)" % h.shown(work), out)
        code, out = h.run("usage", "--json")
        accounts = [a for a in json.loads(out)["accounts"] if a["agent"] == "kimi"]
        self.assertEqual([(a["profiles"], a["plan"], a["source"], a["why"]) for a in accounts],
                         [([home], "Kimi Code", "none", "not signed in"),
                          ([work], "Kimi Code", "none", "token expired; run kimi in this home to renew it")])
        code, out = h.run("usage", "--cached")
        self.assertIn("Kimi Code · no data (--cached)", out)

    def test_the_list_says_why_there_is_no_usage(self):
        h = self.h
        home = h.kimi_home(signed_in=True)
        api = h.folder("code", "api")
        h.kimi_session(home, api, K, "Fix the uploader with kimi", 1)
        code, out = h.run("sessions")
        self.assertEqual(code, 0, out)
        self.assertIn("kimi --session session_" + K, out)
        self.assertIn("No usage data (token expired; run kimi in this home to renew it): " + h.shown(home), out)


class KimiElsewhere(unittest.TestCase):
    def setUp(self):
        self.h = h = FakeHome()
        self.addCleanup(h.cleanup)
        self.claude = h.profile("default")
        self.home = h.kimi_home()

    def test_relocate_leaves_kimi_sessions_alone(self):
        h = self.h
        api, api2 = h.folder("code", "api"), h.folder("code", "api2")
        h.session(self.claude, api, A, "Old one", 70)
        h.kimi_session(self.home, api, K, "In kimi", 70)
        code, out = h.run("relocate", api, api2)
        self.assertEqual(code, 0, out)
        self.assertIn("Relocated 1 session from", out)
        self.assertIn("Left alone: Kimi Code CLI session 0752ad88 in %s is filed under a hash of the folder, which stays "
                      "as it was." % h.shown(self.home), out)
        api3, api4 = h.folder("code", "api3"), h.folder("code", "api4")
        h.kimi_session(self.home, api3, "11111111-2222-4333-8444-555555555555", "Only kimi", 70)
        code, out = h.run("relocate", api3, api4)
        self.assertEqual(code, 1, out)
        self.assertIn("No sessions to move ran in %s (only Kimi Code CLI's, which stay as they are)" % h.shown(api3), out)

    def test_bind_default_binds_kimi_too(self):
        h = self.h
        api = h.folder("code", "api")
        code, out = h.run("bind", "default", cwd=api)
        self.assertEqual(code, 0, out)
        self.assertIn("Claude Code %s · Kimi Code CLI %s." % (h.shown(self.claude), h.shown(self.home)), out)
        work = h.kimi_home("work")
        code, out = h.run("bind", "--kimi", "work", cwd=api)
        self.assertEqual(code, 0, out)
        self.assertIn("Kimi Code CLI " + h.shown(work), out)
        code, out = h.run("bind", "--kimi", "nope", cwd=api)
        self.assertEqual(code, 1, out)
        self.assertIn("isn't a Kimi home", out)


if __name__ == "__main__":
    unittest.main()
