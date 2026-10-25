"""Qwen Code (qwen) in hatcheck: homes, transcripts as qwen 0.26.0 writes them, its registry and
sidecar for sessions open now, read-only conversion out, and relocation skips. Every home is a
throwaway fixture; no real qwen, credentials or network."""
import json
import os
import re
import subprocess
import sys
import unittest
from unittest import mock

from helpers import NOW, WINDOWS, FakeHome, qwen_project, rec, write

Q = "c6f30025-d241-4388-a849-acb3ea844c99"
N = "d0000000-0000-4000-8000-000000000001"
C = "c0000000-0000-4000-8000-000000000001"
E = "e0000000-0000-4000-8000-000000000001"
A = "aaaaaaaa-0000-4000-8000-000000000001"
R = "e0000000-0000-4000-8000-000000000002"
TITLE = "Fix the uploader with Qwen Code"
MODEL = "GLM-5.3"
NO_USAGE = "qwen has no plan-usage command or endpoint (0.26.0)"
READ_ONLY = "Qwen Code's session store isn't one hatcheck can write; choose another agent as the target"
OPEN = "Qwen Code session %s is open in qwen; switch to it, or quit it first" % Q[:8]
OPEN_MOVE = "Qwen Code session %s is open in qwen; quit it first, so it stops changing" % Q[:8]
LEFT_OUT = ("Left out: Qwen Code's thoughts, images and files, what qwen adds to the conversation itself, "
            "its own records and subagent sessions.")
HIDDEN = ("QWEN THOUGHT", "INJECTED QWEN CONTEXT", "SIDECHAIN QWEN TEXT")


def sid(n):
    return "%08x-0000-4000-8000-%012x" % (n, n)


def read_bytes(path):
    with open(path, "rb") as fh:
        return fh.read()


def tree_bytes(root):
    found = {}
    for folder, dirs, files in os.walk(root):
        dirs.sort()
        for name in sorted(files):
            path = os.path.join(folder, name)
            found[os.path.relpath(path, root)] = read_bytes(path)
    return found


def dead_pid():
    """The pid of a process that has exited."""
    p = subprocess.Popen([sys.executable, "-c", "pass"])
    p.wait()
    return p.pid


def resume_command(h, home, session=None):
    tail = " --resume " + session if session else ""
    default = os.path.join(h.home, ".qwen")
    if os.path.realpath(home) == os.path.realpath(default):
        return "qwen" + tail
    if WINDOWS:
        prefix = h.home + os.sep
        if home.startswith(prefix):
            rest = home[len(prefix):]
            if not any(c in rest for c in ('"', '$', '`')):
                folder = '"$HOME\\%s"' % rest
            else:
                folder = "'%s'" % home.replace("'", "''")
        else:
            folder = "'%s'" % home.replace("'", "''")
        return "$env:QWEN_HOME=%s; qwen%s; $env:QWEN_HOME=$null" % (folder, tail)
    shown = h.shown(home) if os.path.commonpath([h.home, home]) == h.home else home
    return "QWEN_HOME=%s qwen%s" % (shown, tail)


class QwenSessions(unittest.TestCase):
    def setUp(self):
        self.h = h = FakeHome()
        self.addCleanup(h.cleanup)
        self.home = h.qwen_home()
        self.api = h.folder("code", "api")
        self.path = h.qwen_session(self.home, self.api, Q, TITLE, 1)

    def sessions(self, *args, **env):
        code, out = self.h.run("sessions", "--json", "--no-usage", *args, **env)
        self.assertEqual(code, 0, out)
        return json.loads(out)["sessions"]

    def test_listing_and_json_metadata(self):
        code, out = self.h.run("sessions", "--no-usage")
        self.assertEqual(code, 0, out)
        self.assertIn("Qwen CLI sessions", out)
        self.assertIn(TITLE, out)
        self.assertIn("qwen --resume " + Q, out)
        self.assertIn(self.h.shown(self.home) + " (1)", out)
        for text in HIDDEN:
            self.assertNotIn(text, out)
        listed = self.sessions()
        self.assertEqual([s["id"] for s in listed], [Q])
        q = listed[0]
        self.assertEqual((q["agent"], q["profile"], q["title"], q["cwd"], q["entrypoint"], q["branch"]),
                         ("qwen", self.home, TITLE, self.api, "cli", "main"))
        self.assertEqual((q["resume"], q["model"], q["open"]), ("qwen --resume " + Q, MODEL, None))

    def test_qwens_own_title_wins_over_the_first_prompt(self):
        self.h.qwen_session(self.home, self.api, C, "The first prompt", 2, custom_title="Uploader fix")
        titles = {s["id"]: s["title"] for s in self.sessions()}
        self.assertEqual(titles[C], "Uploader fix")
        self.assertEqual(titles[Q], TITLE)
        code, out = self.h.run("show", C[:8], "--json")
        self.assertEqual(code, 0, out)
        self.assertEqual(json.loads(out)["title"], "Uploader fix")

    def test_ids_where_show_open_and_resume(self):
        for query in (Q, Q[:8], Q[:8].upper()):
            with self.subTest(query=query):
                code, out = self.h.run("where", query)
                self.assertEqual((code, out), (0, self.path + "\n"))
        code, out = self.h.run("where", Q[:8], "--json")
        self.assertEqual(code, 0, out)
        found = json.loads(out)["sessions"][0]
        self.assertEqual((found["agent"], found["id"], found["profile"], found["file"], found["data_folder"],
                          found["archived"]), ("qwen", Q, self.home, self.path, None, False))
        self.assertIn(json.dumps(self.path)[1:-1], out)

        code, out = self.h.run("show", Q[:8])
        self.assertEqual(code, 0, out)
        self.assertIn('Session %s "%s" in %s, 2 turns' % (Q[:8], TITLE, self.h.shown(self.home)), out)
        for text in (TITLE, "I'll inspect it.", "[external_agent_tool_call: read_file]", "path: src/main.rs",
                     "fn main() {}", "Done: " + TITLE, LEFT_OUT):
            self.assertIn(text, out)
        for text in HIDDEN:
            self.assertNotIn(text, out)
        code, out = self.h.run("show", Q[:8], "--json")
        self.assertEqual(code, 0, out)
        shown = json.loads(out)
        self.assertEqual((shown["agent"], shown["id"], shown["title"], shown["cwd"], shown["model"],
                          shown["turns_total"], shown["tool_calls"]), ("qwen", Q, TITLE, self.api, MODEL, 2, 1))
        text = "\n".join(turn["text"] for turn in shown["turns"])
        self.assertIn("[external_agent_tool_result]", text)
        self.assertNotIn("QWEN THOUGHT", text)

        expected = ["cd " + self.h.shown(self.api), "qwen --resume " + Q]
        code, out = self.h.run("open", Q[:8], "--print")
        self.assertEqual((code, out.splitlines()), (0, expected))
        code, out = self.h.run("resume", Q[:8], "--print")
        self.assertEqual((code, out.splitlines()), (0, expected))

    def test_an_archived_session_is_found_and_says_so(self):
        path = self.h.qwen_session(self.home, self.api, E, "An archived Qwen session", 2, archived=True)
        self.assertTrue(path.endswith(os.path.join("chats", "archive", E + ".jsonl")))
        self.assertIn(E, {s["id"] for s in self.sessions()})
        code, out = self.h.run("where", E[:8], "--json")
        self.assertEqual(code, 0, out)
        found = json.loads(out)["sessions"][0]
        self.assertEqual((found["profile"], found["file"], found["archived"]), (self.home, path, True))

    def test_subagents_are_hidden_and_other_sources_wait_for_all(self):
        self.h.qwen_session(self.home, self.api, sid(32), "A subagent's transcript", parent=Q)
        self.h.qwen_session(self.home, self.api, sid(33), "An agent source", source="agent")
        self.h.qwen_session(self.home, self.api, sid(34), "A Telegram channel's session", source="channel")
        self.h.qwen_session(self.home, self.api, sid(35), "A plain terminal session", source="standalone")
        self.assertEqual({s["id"] for s in self.sessions()}, {Q, sid(35)})
        listed = {s["id"]: s["entrypoint"] for s in self.sessions("--all")}
        self.assertEqual(listed, {Q: "cli", sid(34): "channel", sid(35): "cli"})
        code, out = self.h.run("sessions", "--no-usage")
        self.assertEqual(code, 0, out)
        self.assertNotIn("Telegram", out)
        code, out = self.h.run("show", sid(32)[:8], "--json")  # hidden from the list, still readable
        self.assertEqual(code, 0, out)
        self.assertEqual(json.loads(out)["id"], sid(32))

    def test_open_from_the_registry_or_the_sidecar_of_a_running_process(self):
        self.assertIsNone(self.sessions()[0]["open"])
        self.h.qwen_registry(self.home, Q, self.api, dead_pid())
        self.assertIsNone(self.sessions()[0]["open"])
        self.h.qwen_registry(self.home, Q, self.api, os.getpid())
        self.assertEqual(self.sessions()[0]["open"], {"pid": None, "status": None, "in": "qwen"})
        code, out = self.h.run("open", Q[:8], HATCHECK_TERMINAL="false")
        self.assertEqual(code, 1, out)
        self.assertIn(OPEN, out)
        os.remove(os.path.join(self.home, "sessions", "%d.json" % os.getpid()))
        write(os.path.join(self.home, "sessions", "2026-planning-notes.json"), json.dumps({"sessionId": Q, "pid": os.getpid()}))
        self.assertIsNone(self.sessions()[0]["open"])  # not a record qwen would write
        self.h.qwen_sidecar(self.path, Q, self.api, dead_pid())
        self.assertIsNone(self.sessions()[0]["open"])
        self.h.qwen_sidecar(self.path, Q, self.api, os.getpid())
        self.assertEqual(self.sessions()[0]["open"], {"pid": None, "status": None, "in": "qwen"})
        code, out = self.h.run("open", Q[:8], HATCHECK_TERMINAL="false")
        self.assertEqual(code, 1, out)
        self.assertIn(OPEN, out)
        code, out = self.h.run("open", Q[:8], "--print")  # prints the commands without starting them
        self.assertEqual(code, 0, out)

    def test_windows_style_cwd_and_json_escaping(self):
        cwd = 'C:\\work\\qwen "quoted"\\api'
        session = sid(48)
        self.h.qwen_session(self.home, cwd, session, 'Windows \\ paths and "quotes"')
        code, out = self.h.run("show", session[:8], "--json")
        self.assertEqual(code, 0, out)
        shown = json.loads(out)
        self.assertEqual((shown["cwd"], shown["title"]), (cwd, 'Windows \\ paths and "quotes"'))
        self.assertIn(json.dumps(cwd)[1:-1], out)
        code, out = self.h.run("where", session[:8], "--json")
        self.assertEqual(code, 0, out)
        found = json.loads(out)["sessions"][0]
        self.assertTrue(found["file"].endswith(os.sep + session + ".jsonl"), found["file"])
        self.assertIn(json.dumps(found["file"])[1:-1], out)


class QwenBadFiles(unittest.TestCase):
    def setUp(self):
        self.h = h = FakeHome()
        self.addCleanup(h.cleanup)
        self.home = h.qwen_home()
        self.api = h.folder("code", "api")

    def ids(self):
        code, out = self.h.run("sessions", "--json", "--no-usage")
        self.assertEqual(code, 0, out)
        return {s["id"] for s in json.loads(out)["sessions"]}

    def test_only_transcripts_count_and_a_torn_tail_is_not_read(self):
        path = self.h.qwen_session(self.home, self.api, Q, TITLE)
        chats = os.path.dirname(path)
        write(os.path.join(chats, "notes.jsonl"), rec(type="user", message={"parts": [{"text": "NOT A SESSION"}]}) + "\n")
        self.h.qwen_sidecar(path, Q, self.api, dead_pid())
        write(os.path.join(chats, Q + ".pr.json"), "{}")
        self.assertEqual(self.ids(), {Q})
        code, out = self.h.run("where", "notes")
        self.assertEqual((code, out), (1, ""))

        torn = sid(115)
        path = self.h.qwen_session(self.home, self.api, torn, "A torn Qwen tail")
        with open(path, "a", encoding="utf-8") as fh:
            fh.write(json.dumps({"uuid": "x", "parentUuid": "00000006-0000-4000-8000-000000000006", "type": "assistant",
                                 "message": {"parts": [{"text": "TORN TAIL MUST NOT APPEAR"}]}})[:-12])
        code, out = self.h.run("show", torn[:8], "--json")
        self.assertEqual(code, 0, out)
        self.assertEqual(json.loads(out)["turns_total"], 2)
        self.assertNotIn("TORN TAIL", out)

    def test_a_file_with_no_records_is_not_a_session(self):
        path = self.h.qwen_session(self.home, self.api, E, "Emptied")
        write(path, "\n")
        self.assertEqual(self.ids(), set())
        code, out = self.h.run("where", E[:8])
        self.assertEqual((code, out), (1, ""))


class QwenDiscovery(unittest.TestCase):
    def setUp(self):
        self.h = h = FakeHome()
        self.addCleanup(h.cleanup)
        self.api = h.folder("code", "api")
        self.home = h.qwen_home()
        h.qwen_session(self.home, self.api, Q, "Default Qwen home")

    def ids(self, **env):
        code, out = self.h.run("sessions", "--json", "--no-usage", **env)
        self.assertEqual(code, 0, out)
        return {s["id"]: s for s in json.loads(out)["sessions"] if s["agent"] == "qwen"}

    def test_default_named_and_custom_homes(self):
        work = self.h.qwen_home("work")
        self.h.qwen_session(work, self.api, N, "Named Qwen home")
        self.h.folder(".qwen-empty")  # never run: no installation_id, no chats
        listed = self.ids()
        self.assertEqual(set(listed), {Q, N})
        self.assertEqual((listed[Q]["profile"], listed[Q]["resume"]), (self.home, "qwen --resume " + Q))
        self.assertEqual((listed[N]["profile"], listed[N]["resume"]), (work, resume_command(self.h, work, N)))
        code, out = self.h.run("open", N[:8], "--print")
        self.assertEqual((code, out.splitlines()), (0, ["cd " + self.h.shown(self.api), resume_command(self.h, work, N)]))
        before = tree_bytes(work)
        code, out = self.h.run("open", "--login", "qwen:work", "--print")
        self.assertEqual(code, 0, out)
        self.assertEqual(out.splitlines()[-1], resume_command(self.h, work))
        self.assertEqual(tree_bytes(work), before)

        custom = self.h.qwen_home(path=os.path.join(self.h.root, "elsewhere", "qwen-custom"))
        self.h.qwen_session(custom, self.api, C, "QWEN_HOME Qwen home")
        with mock.patch.dict(os.environ, {"QWEN_HOME": custom}):
            self.assertEqual(set(self.ids()), {Q, N})  # the tests' environment is scrubbed
        listed = self.ids(QWEN_HOME=custom)
        self.assertEqual(set(listed), {Q, N, C})
        self.assertEqual((listed[C]["profile"], listed[C]["resume"]), (custom, resume_command(self.h, custom, C)))
        empty = self.h.folder("not-a-qwen-home")
        self.assertEqual(set(self.ids(QWEN_HOME=empty)), {Q, N})

    def test_a_claude_profile_is_not_a_qwen_home(self):
        profile = self.h.profile("default")
        self.h.session(profile, self.api, A, "A Claude Code session")
        code, out = self.h.run("sessions", "--json", "--no-usage")
        self.assertEqual(code, 0, out)
        agents = {s["id"]: s["agent"] for s in json.loads(out)["sessions"]}
        self.assertEqual(agents, {Q: "qwen", A: "claude"})
        code, out = self.h.run("where", A[:8], "--json")
        self.assertEqual(code, 0, out)
        self.assertEqual([s["agent"] for s in json.loads(out)["sessions"]], ["claude"])


class QwenUsageAndBindings(unittest.TestCase):
    def setUp(self):
        self.h = h = FakeHome()
        self.addCleanup(h.cleanup)
        self.home = h.qwen_home()
        self.work = h.qwen_home("work")
        self.claude = h.profile("default")
        self.codex = h.codex_home("default")
        self.api = h.folder("code", "api")

    def saved(self, folder=None):
        base = os.path.join(self.h.home, "AppData", "Roaming") if WINDOWS else os.path.join(self.h.home, ".config")
        with open(os.path.join(base, "hatcheck", "bindings.json"), encoding="utf-8") as fh:
            return json.load(fh)["folders"][os.path.realpath(folder or self.api)]

    def test_usage_says_why_there_is_none(self):
        code, out = self.h.run("usage")
        self.assertEqual(code, 0, out)
        self.assertIn("%s  Qwen · no data (%s)" % (self.h.shown(self.home), NO_USAGE), out)
        code, out = self.h.run("usage", "--json")
        self.assertEqual(code, 0, out)
        accounts = [a for a in json.loads(out)["accounts"] if a["agent"] == "qwen"]
        self.assertEqual({a["profiles"][0] for a in accounts}, {self.home, self.work})
        for account in accounts:
            self.assertEqual((account["plan"], account["limits"], account["source"], account["why"]),
                             ("Qwen", [], "none", NO_USAGE))

    def test_bind_default_named_which_and_preservation(self):
        code, out = self.h.run("bind", "default", cwd=self.api)
        self.assertEqual(code, 0, out)
        self.assertIn("Qwen Code " + self.h.shown(self.home), out)
        self.assertEqual(self.saved(), {"claude": "default", "codex": "default", "qwen": "default"})
        code, out = self.h.run("bind", "--qwen", "work", cwd=self.api)
        self.assertEqual(code, 0, out)
        named = self.saved()
        self.assertEqual(named, {"claude": "default", "codex": "default", "qwen": "work"})
        code, out = self.h.run("bind", "--which", "qwen", cwd=self.api)
        self.assertEqual((code, out), (0, "work\n"))
        code, out = self.h.run("bind", "--codex", "default", cwd=self.api)
        self.assertEqual(code, 0, out)
        self.assertEqual(self.saved(), named)
        code, out = self.h.run("bind", "--list")
        self.assertEqual(code, 0, out)
        self.assertIn("Qwen Code " + self.h.shown(self.work), out)
        self.h.profile("work")
        self.h.codex_home("work")
        other = self.h.folder("code", "other")
        code, out = self.h.run("bind", "work", cwd=other)
        self.assertEqual(code, 0, out)
        self.assertEqual(self.saved(other)["qwen"], "work")
        self.h.folder(".qwen-never")
        code, out = self.h.run("bind", "--qwen", "never", cwd=other)
        self.assertEqual(code, 1, out)
        self.assertIn("isn't a Qwen Code home", out)
        self.assertIn("QWEN_HOME=", out)
        self.assertEqual(self.saved(other)["qwen"], "work")


class QwenConversion(unittest.TestCase):
    def setUp(self):
        self.h = h = FakeHome()
        self.addCleanup(h.cleanup)
        self.home = h.qwen_home()
        self.claude = h.profile("default")
        self.api = h.folder("code", "api")
        self.path = h.qwen_session(self.home, self.api, Q, TITLE)

    def claude_transcript(self, session):
        project = re.sub(r"[^A-Za-z0-9]", "-", self.api)
        return os.path.join(self.claude, "projects", project, session + ".jsonl")

    def test_convert_out_dry_run_move_and_fork(self):
        original = tree_bytes(self.home)
        before = tree_bytes(self.claude)
        code, out = self.h.run("move", Q[:8], "default", "--dry-run")
        self.assertEqual(code, 0, out)
        self.assertIn('Would convert Qwen Code session %s "%s"' % (Q[:8], TITLE), out)
        self.assertEqual(tree_bytes(self.home), original)
        self.assertEqual(tree_bytes(self.claude), before)

        code, out = self.h.run("move", Q[:8], "default", "--json")
        self.assertEqual(code, 0, out)
        moved = json.loads(out)
        self.assertEqual((moved["action"], moved["agent"], moved["session"], moved["target_agent"], moved["target"],
                          moved["prompts"], moved["tool_calls"]), ("convert", "qwen", Q, "claude", self.claude, 1, 1))
        new = moved["new_session"]
        self.assertTrue(new)
        path = self.claude_transcript(new)
        self.assertTrue(os.path.isfile(path), path)
        text = read_bytes(path).decode("utf-8")
        for expected in ("[Imported from Qwen Code session " + Q, TITLE, "read_file", "fn main() {}"):
            self.assertIn(expected, text)
        for excluded in HIDDEN:
            self.assertNotIn(excluded, text)
        self.assertEqual(tree_bytes(self.home), original)

        code, out = self.h.run("fork", Q[:8], "default", "--json")
        self.assertEqual(code, 0, out)
        forked = json.loads(out)
        self.assertEqual((forked["action"], forked["agent"], forked["target_agent"]), ("convert", "qwen", "claude"))
        self.assertNotIn(forked["new_session"], (None, Q, new))
        self.assertTrue(os.path.isfile(self.claude_transcript(forked["new_session"])))
        self.assertEqual(tree_bytes(self.home), original)

    def test_an_open_session_is_refused_without_writes(self):
        self.h.qwen_registry(self.home, Q, self.api, os.getpid())
        original = tree_bytes(self.home)
        before = tree_bytes(self.claude)
        for command in ("move", "fork"):
            for extra in (("--dry-run",), ()):
                with self.subTest(command=command, extra=extra):
                    code, out = self.h.run(command, Q[:8], "default", *extra)
                    self.assertEqual(code, 1, out)
                    self.assertIn(OPEN_MOVE, out)
                    self.assertEqual(tree_bytes(self.home), original)
                    self.assertEqual(tree_bytes(self.claude), before)

    def test_qwen_targets_and_same_agent_copies_are_refused_without_changes(self):
        source = self.h.session(self.claude, self.api, A, "A Claude Code session")
        source_bytes = read_bytes(source)
        claude_before = tree_bytes(self.claude)
        work = self.h.qwen_home("work")
        before, target_before = tree_bytes(self.home), tree_bytes(work)
        for command in ("move", "fork"):
            for session in (A, Q):
                with self.subTest(command=command, source=session):
                    code, out = self.h.run(command, session[:8], "qwen:work")
                    self.assertEqual(code, 1, out)
                    self.assertIn(READ_ONLY, out)
                    self.assertEqual(tree_bytes(self.home), before)
                    self.assertEqual(tree_bytes(work), target_before)
                    self.assertEqual(tree_bytes(self.claude), claude_before)
                    self.assertEqual(read_bytes(source), source_bytes)


class QwenRelocate(unittest.TestCase):
    def test_skip_and_dry_run_preserve_every_qwen_byte(self):
        h = FakeHome()
        self.addCleanup(h.cleanup)
        home, claude = h.qwen_home(), h.profile("default")
        old = h.folder("code", "api")
        new = os.path.join(h.home, "code", "api2")
        transcript_q = h.qwen_session(home, old, R, "A qwen session left in place", 70)
        transcript = h.session(claude, old, A, "A Claude session that moves", 70)
        os.rename(old, new)
        before = tree_bytes(home)
        skipped = ("Left alone: Qwen Code session %s in %s is filed under the folder's name and records it on every "
                   "line, which stay as they were." % (R[:8], h.shown(home)))
        code, out = h.run("relocate", old, new, "--dry-run")
        self.assertEqual(code, 0, out)
        self.assertIn("Would relocate 1 session", out)
        self.assertIn(skipped, out)
        self.assertEqual(tree_bytes(home), before)
        self.assertTrue(os.path.isfile(transcript))
        code, out = h.run("relocate", old, new)
        self.assertEqual(code, 0, out)
        self.assertIn("Relocated 1 session", out)
        self.assertIn(skipped, out)
        self.assertEqual(tree_bytes(home), before)
        self.assertTrue(os.path.isfile(transcript_q))
        moved = os.path.join(claude, "projects", re.sub(r"[^A-Za-z0-9]", "-", new), A + ".jsonl")
        self.assertTrue(os.path.isfile(moved), moved)
        self.assertEqual(qwen_project(new), re.sub(r"[^A-Za-z0-9]", "-", new.lower() if WINDOWS else new))


if __name__ == "__main__":
    unittest.main()
