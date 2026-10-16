"""hatcheck move with Codex: copying Codex sessions between homes on one account, and
converting sessions between Claude Code and Codex, or for a home on another account. Against a
throwaway home: no real Codex home, no network."""
import json
import os
import re
import subprocess
import sys
import unittest
from unittest import mock

from helpers import NOW, WINDOWS, FakeHome, iso, load, rec, write

A = "aaaaaaaa-0000-4000-8000-000000000001"
X = "019f0000-aaaa-7000-8000-000000000001"  # Codex IDs are version 7 UUIDs
MOVE = "move"
UUID = r"[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}"


def text_of(path):
    with open(path, encoding="utf-8") as fh:
        return fh.read()


def jsonl(path):
    return [json.loads(line) for line in text_of(path).splitlines() if line.strip()]


class Moves(unittest.TestCase):
    """hatcheck move with Codex: copies between homes on one account, conversions otherwise."""

    def setUp(self):
        self.h = h = FakeHome()
        self.addCleanup(h.cleanup)
        self.default = h.profile("default", "acct-c")
        self.work = h.profile("work", "acct-w")
        self.codex = h.codex_home("default", "acct-1")
        self.same = h.codex_home("same", "acct-1")
        self.other = h.codex_home("other", "acct-2")
        self.cwd = h.folder("code", "api")
        self.rollout = h.codex_session(self.codex, self.cwd, X, "Fix the uploader", 30, name="Uploader fix")
        base = {"cwd": self.cwd, "sessionId": A, "entrypoint": "cli", "gitBranch": "main"}
        thinking = rec(type="assistant", uuid="u2", parentUuid="u1", timestamp=iso(NOW - 900), message={
            "role": "assistant", "model": "claude-opus-5-5", "content": [
                {"type": "thinking", "thinking": "SECRET-THINKING", "signature": "SIG=="},
                {"type": "tool_use", "id": "t1", "name": "Bash", "input": {"command": "pytest -q", "description": "Run tests"}}]},
            **base)
        result = rec(type="user", uuid="u3", parentUuid="u2", timestamp=iso(NOW - 890), message={
            "role": "user", "content": [{"type": "tool_result", "tool_use_id": "t1", "content": "3 passed"}]}, **base)
        reply = rec(type="assistant", uuid="u4", parentUuid="u3", timestamp=iso(NOW - 880), message={
            "role": "assistant", "model": "claude-opus-5-5", "content": [{"type": "text", "text": "All 3 tests pass."}]},
            **base)
        prompt = rec(type="user", uuid="u1", parentUuid=None, timestamp=iso(NOW - 910),
                     message={"role": "user", "content": "Run the tests"}, **base)
        self.transcript = os.path.join(self.default, "projects", re.sub(r"[^A-Za-z0-9]", "-", self.cwd), A + ".jsonl")
        write(self.transcript, "\n".join([prompt, thinking, result, reply,
                                          rec(type="ai-title", aiTitle="Run the test suite", sessionId=A)]) + "\n",
              mtime=NOW - 880)

    def new_id(self, out):
        return re.search(r"new session +(%s)" % UUID, out).group(1)

    def test_claude_code_to_codex(self):
        with open(self.transcript, "rb") as fh:
            original = fh.read()
        code, out = self.h.run(MOVE, A[:8], "codex:work")
        self.assertEqual(code, 1, out)
        self.assertIn("isn't a Codex home. Sign in there first, e.g. with: cx work login", out)
        work = self.h.codex_home("work")
        code, out = self.h.run(MOVE, A[:8], "codex:work")
        self.assertEqual(code, 0, out)
        new = self.new_id(out)
        self.assertIn('Converted Claude Code session aaaaaaaa "Run the test suite" from %s into a Codex session in %s:'
                      % (self.h.shown(self.default), self.h.shown(work)), out)
        self.assertIn("cx work resume " + new, out)
        path, = [p for p in self.h_files(work) if new in p]
        records = jsonl(path)
        meta = records[0]["payload"]
        self.assertEqual((records[0]["type"], meta["id"], meta["cwd"], meta["source"]), ("session_meta", new, self.cwd, "cli"))
        items = [r["payload"] for r in records if r["type"] == "response_item"]
        self.assertEqual([i["role"] for i in items], ["user", "assistant"])
        self.assertTrue(items[0]["content"][0]["text"].startswith("[Imported from Claude Code session " + A))
        self.assertIn("Run the tests", items[0]["content"][0]["text"])
        answer = items[1]["content"][0]["text"]
        self.assertIn("[external_agent_tool_call: Bash]\ncommand: pytest -q\ndescription: Run tests\n"
                      "[/external_agent_tool_call]", answer)
        self.assertIn("[external_agent_tool_result]\n3 passed\n[/external_agent_tool_result]", answer)
        self.assertIn("All 3 tests pass.", answer)
        self.assertNotIn("SECRET-THINKING", text_of(path))
        events = [r["payload"]["type"] for r in records if r["type"] == "event_msg"]
        self.assertEqual(events, ["task_started", "user_message", "agent_message", "agent_message", "token_count",
                                  "task_complete"])
        with open(os.path.join(work, "session_index.jsonl"), encoding="utf-8") as fh:
            self.assertEqual(json.loads(fh.read())["thread_name"], "Run the test suite")
        with open(self.transcript, "rb") as fh:
            self.assertEqual(fh.read(), original)
        code, out = self.h.run("sessions", "--no-usage")
        self.assertIn("cx work resume " + new, out)

    def h_files(self, home):
        return [os.path.join(root, f) for root, _, files in os.walk(os.path.join(home, "sessions")) for f in files]

    def test_codex_to_claude_code(self):
        code, out = self.h.run(MOVE, X[:13], "work")
        self.assertEqual(code, 0, out)
        new = self.new_id(out)
        self.assertIn("cl work --resume " + new, out)
        self.assertIn("1 of your messages, with the replies", out)
        self.assertIn("tool calls                 2, as text", out)
        path = os.path.join(self.work, "projects", re.sub(r"[^A-Za-z0-9]", "-", self.cwd), new + ".jsonl")
        records = jsonl(path)
        messages = [r for r in records if r["type"] in ("user", "assistant")]
        self.assertEqual([m["type"] for m in messages], ["user", "assistant"])
        self.assertEqual([m["parentUuid"] for m in messages], [None, messages[0]["uuid"]])
        first = messages[0]["message"]["content"]
        self.assertTrue(first.startswith("[Imported from Codex session %s, started " % X), first)
        self.assertIn("Fix the uploader", first)
        self.assertNotIn("environment_context", first)  # what Codex adds itself stays behind
        answer = messages[1]["message"]["content"][0]["text"]
        self.assertIn("[external_agent_tool_call: exec_command]\ncmd: ls src\n[/external_agent_tool_call]", answer)
        self.assertIn("[external_agent_tool_result]\nmain.py\nutil.py\n[/external_agent_tool_result]", answer)
        self.assertIn("[external_agent_tool_call: apply_patch]\ninput: *** Begin Patch", answer)
        self.assertIn("Done: Fix the uploader", answer)
        self.assertNotIn("SECRET-REASONING", text_of(path))
        self.assertEqual((records[-2]["type"], records[-2]["subtype"], records[-2]["parentUuid"]),
                         ("system", "informational", messages[-1]["uuid"]))
        self.assertEqual(records[-1], {"type": "ai-title", "aiTitle": "Uploader fix", "sessionId": new})
        self.assertTrue(all(m["sessionId"] == new and m["cwd"] == self.cwd for m in messages))
        code, out = self.h.run("sessions", "--no-usage")
        self.assertIn("cl work --resume " + new, out)
        self.assertIn("Uploader fix", out)

    def test_codex_homes_on_one_account_copy(self):
        code, out = self.h.run(MOVE, X[:13], "codex:same", "--no-keep")
        self.assertEqual(code, 0, out)
        self.assertIn("Moved Codex session 019f0000", out)
        self.assertIn("same ChatGPT account", out)
        self.assertIn("cx same resume " + X, out)
        copy = os.path.join(self.same, os.path.relpath(self.rollout, self.codex))
        self.assertFalse(os.path.exists(self.rollout))
        self.assertIn("SECRET-REASONING", text_of(copy))  # as it was: the same account can read it
        with open(os.path.join(self.same, "session_index.jsonl"), encoding="utf-8") as fh:
            self.assertEqual(json.loads(fh.read())["thread_name"], "Uploader fix")
        code, out = self.h.run(MOVE, X[:13], "codex", "--no-keep")
        self.assertEqual(code, 0, out)
        self.assertIn("codex resume " + X, out)
        self.assertTrue(os.path.isfile(self.rollout))

    def test_codex_homes_on_other_accounts_convert(self):
        code, out = self.h.run(MOVE, X[:13], "codex:other", "--no-keep")
        self.assertEqual(code, 1, out)
        self.assertIn("--no-keep can't be used here", out)
        code, out = self.h.run(MOVE, X[:13], "codex:other", "--dry-run")
        self.assertEqual(code, 0, out)
        self.assertIn("Would convert", out)
        self.assertEqual(self.h_files(self.other), [])
        code, out = self.h.run(MOVE, X[:13], "codex:other")
        self.assertEqual(code, 0, out)
        self.assertIn("aren't signed in to the same ChatGPT account", out)
        new = self.new_id(out)
        path, = self.h_files(self.other)
        self.assertIn(new, path)
        self.assertNotIn("SECRET-REASONING", text_of(path))
        self.assertTrue(os.path.isfile(self.rollout))

    @unittest.skipIf(WINDOWS, "Codex's session lock is checked on macOS and Linux")
    def test_refuses_a_codex_session_that_is_open(self):
        import fcntl
        lock = os.path.join(self.codex, "thread-writer-locks", X + ".lock")
        write(lock, "")
        fd = os.open(lock, os.O_RDWR)
        self.addCleanup(os.close, fd)
        fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)  # as a running codex holds it
        code, out = self.h.run(MOVE, X[:13], "work")
        self.assertEqual(code, 1, out)
        self.assertIn("is open in codex; quit it first", out)
        fcntl.flock(fd, fcntl.LOCK_UN)
        code, out = self.h.run(MOVE, X[:13], "work")
        self.assertEqual(code, 0, out)

    def test_where_it_goes(self):
        m = load(MOVE)
        home = m.HOME
        self.assertEqual(m.place("work"), ("claude", os.path.join(home, ".claude-work")))
        self.assertEqual(m.place("default"), ("claude", os.path.join(home, ".claude")))
        self.assertEqual(m.place("codex"), ("codex", os.path.join(home, ".codex")))
        self.assertEqual(m.place("codex:work"), ("codex", os.path.join(home, ".codex-work")))
        self.assertEqual(m.place("claude:codex"), ("claude", os.path.join(home, ".claude-codex")))
        self.assertEqual(m.place(os.path.join(self.h.home, ".codex-same")), ("codex", self.same))


class Converting(unittest.TestCase):
    def setUp(self):
        self.m = load(MOVE)
        self.s = load("sessions")

    def test_follows_the_conversation_past_rewinds_and_compactions(self):
        def msg(uuid, parent, role, text, **extra):
            return dict({"type": role, "uuid": uuid, "parentUuid": parent, "timestamp": iso(NOW), "cwd": "/w",
                         "message": {"role": role, "content": text if role == "user" else [{"type": "text", "text": text}]}},
                        **extra)
        records = [
            msg("1", None, "user", "first"), msg("2", "1", "assistant", "one"),
            msg("3", "2", "user", "undone by /rewind"), msg("4", "3", "assistant", "dropped"),
            msg("5", "2", "user", "second"), msg("6", "5", "assistant", "two"),
            {"type": "system", "subtype": "compact_boundary", "uuid": "7", "parentUuid": None, "logicalParentUuid": "6"},
            msg("8", "7", "user", "Summary of the above", isCompactSummary=True),
            msg("9", "8", "user", "<command-name>/model</command-name>"),
            msg("10", "9", "user", "third"), msg("11", "10", "assistant", "three"),
        ]
        folder = self.tmp()
        path = os.path.join(folder, A + ".jsonl")
        write(path, "\n".join(json.dumps(r) for r in records) + "\n")
        conv = self.m.read_claude(path, self.s)
        texts = [(role, " ".join(t)) for role, t, _ in conv["turns"]]
        self.assertEqual(texts, [("user", "first"), ("assistant", "one"), ("user", "second"), ("assistant", "two"),
                                 ("user", "third"), ("assistant", "three")])
        self.assertEqual(conv["cwd"], "/w")

    def test_long_sessions_keep_the_first_prompt_and_the_newest_turns(self):
        turns = [["user" if n % 2 == 0 else "assistant", ["%d " % n + "x" * 1000], iso(NOW)] for n in range(10)]
        conv = {"agent": "codex", "id": X, "cwd": "/w", "model": None, "turns": turns, "started": iso(NOW),
                "reasoning": "Codex's reasoning"}
        with mock.patch.object(self.m, "BUDGET", 4500):
            fitted, dropped = self.m.fit(conv, "claude")
        self.assertEqual(dropped, 6)
        self.assertEqual([role for role, _, _ in fitted], ["user", "assistant", "user", "assistant"])
        first = "\n".join(fitted[0][1])
        self.assertIn("[Imported from Codex session", first)
        self.assertIn("0 x", first)
        self.assertIn("[6 earlier turns left out here, to fit the context window.]", first)
        self.assertIn("7 x", "".join(fitted[1][1]))
        self.assertTrue(fitted[-1][1][-1].startswith("9 x"))

    def tmp(self):
        import shutil
        import tempfile
        folder = tempfile.mkdtemp(prefix="cma-")
        self.addCleanup(shutil.rmtree, folder, True)
        return folder


class ResumeInCodex(unittest.TestCase):
    """The y in "Resume it now in ...? [y/N]" for a Codex home: codex resume <id>, with its CODEX_HOME."""

    def test_runs_codex_in_the_new_home(self):
        h = FakeHome()
        self.addCleanup(h.cleanup)
        cwd, bindir = h.folder("code", "api"), os.path.join(h.root, "fakebin")
        if WINDOWS:
            write(os.path.join(bindir, "codex.cmd"), "@echo off\r\nif defined CODEX_HOME (echo CODEX_HOME=%CODEX_HOME%"
                  " cwd=%CD% args=%*) else (echo CODEX_HOME=unset cwd=%CD% args=%*)\r\n")
        else:
            write(os.path.join(bindir, "codex"), '#!/bin/sh\necho "CODEX_HOME=${CODEX_HOME-unset} cwd=$(pwd) args=$*"\n')
            os.chmod(os.path.join(bindir, "codex"), 0o755)
        env = dict(h.env(), PATH=bindir + os.pathsep + (os.environ["PATH"] if WINDOWS else "/bin"),
                   CODEX_HOME=os.path.join(h.home, ".codex-elsewhere"))
        snippet = ("import sys; sys.path.insert(0, %r); from helpers import load; "
                   "load(%r).resume_now(*sys.argv[1:])" % (os.path.dirname(os.path.abspath(__file__)), MOVE))
        for home, shown in ((h.codex_home("work"), None), (h.codex_home(), "unset")):
            p = subprocess.run([sys.executable, "-c", snippet, home, X, cwd, "codex"], env=env, cwd=h.root,
                               capture_output=True, text=True, encoding="utf-8")
            self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
            line = "CODEX_HOME=%s cwd=%s args=resume %s" % (shown or home, cwd, X)
            self.assertIn(line.lower() if WINDOWS else line, p.stdout.lower() if WINDOWS else p.stdout)


if __name__ == "__main__":
    unittest.main()
