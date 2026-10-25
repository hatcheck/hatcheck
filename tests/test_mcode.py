"""MiniMax Code (mcode) in hatcheck: v3 SQLite catalogs, canonical v2 histories,
expiring writer leases, read-only conversion out, and relocation skips. Every store is a
throwaway fixture; no real mcode, credentials, runtime initialization or network."""
import json
import os
import re
import shutil
import sqlite3
import unittest
from unittest import mock

from helpers import NOW, WINDOWS, FakeHome, mcode_db, mcode_record, rec, write

M = "b8bde3ebadeb49d5be04757040a8a2b3"
N = "d000000000000000000000000000000001"
C = "c000000000000000000000000000000001"
L = "c000000000000000000000000000000002"
E = "e000000000000000000000000000000001"
A = "aaaaaaaa-0000-4000-8000-000000000001"
R = "e000000000000000000000000000000002"
TITLE = "Fix the uploader with MiniMax Code"
MODEL = "minimax/MiniMax-M3.1-Flash"
NO_USAGE = "mcode plan quota isn't wired into hatcheck (0.6.13)"
COPY_REFUSED = ("MiniMax Code's shared SQLite state, manifest paths and session assets aren't safe for hatcheck to "
                "copy; convert session %s for another agent instead" % M[:8])
PROMPT = ("Read %s, a conversation imported from another coding agent, and carry it on from where it ended. "
          "Check first whether its last action completed, and if it did, do not repeat it.")
OPEN = "MiniMax Code session %s is open or has an active writer; close it or let that work finish first" % M[:8]


def sid(n):
    return "%08x%024x" % (n, n)


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


def mcode_words(h, home):
    """What hatcheck prints before and after `mcode <tail>` to run mcode with a data home: nothing
    for ~/.minimax, MINIMAX_DATA_DIR= before it in sh, and in PowerShell's words the variable set
    before and cleared after, so a longer tail sits between them."""
    default = os.path.join(h.home, ".minimax")
    if os.path.realpath(home) == os.path.realpath(default):
        return "", ""
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
        return "$env:MINIMAX_DATA_DIR=%s; " % folder, "; $env:MINIMAX_DATA_DIR=$null"
    shown = h.shown(home) if os.path.commonpath([h.home, home]) == h.home else home
    return "MINIMAX_DATA_DIR=%s " % shown, ""


def resume_command(h, home, session=None):
    before, after = mcode_words(h, home)
    return before + "mcode" + (" --session mvs_" + session if session else "") + after


class McodeSessions(unittest.TestCase):
    def setUp(self):
        self.h = h = FakeHome()
        self.addCleanup(h.cleanup)
        self.home = h.mcode_home()
        self.api = h.folder("code", "api")
        self.messages = h.mcode_session(self.home, self.api, M, TITLE, 1)
        self.folder = os.path.dirname(self.messages)

    def sessions(self, *args, **env):
        code, out = self.h.run("sessions", "--json", "--no-usage", *args, **env)
        self.assertEqual(code, 0, out)
        return json.loads(out)["sessions"]

    def test_listing_and_json_metadata(self):
        code, out = self.h.run("sessions", "--no-usage")
        self.assertEqual(code, 0, out)
        self.assertIn("MiniMax CLI sessions", out)
        self.assertIn(TITLE + "\n", out)
        self.assertIn("mcode --session mvs_" + M, out)
        self.assertIn(self.h.shown(self.home) + " (1)", out)
        for text in ("MCODE REASONING", "DISPLAY ROW FALLBACK", "OLD MCODE SNAPSHOT"):
            self.assertNotIn(text, out)
        listed = self.sessions()
        self.assertEqual([s["id"] for s in listed], [M])
        m = listed[0]
        self.assertEqual((m["agent"], m["profile"], m["title"], m["cwd"], m["entrypoint"]),
                         ("mcode", self.home, TITLE, self.api, "cli"))
        self.assertEqual((m["resume"], m["model"], m["branch"], m["open"]),
                         ("mcode --session mvs_" + M, MODEL, None, None))

    def test_empty_canonical_history_uses_catalog_title_and_archive(self):
        empty = self.h.mcode_session(self.home, self.api, E, "An empty MiniMax history", 2,
                                     archived=True, empty=True)
        self.assertEqual(read_bytes(empty), b"")
        self.assertEqual({s["id"] for s in self.sessions()}, {M, E})
        code, out = self.h.run("show", E[:8], "--json")
        self.assertEqual(code, 0, out)
        shown = json.loads(out)
        self.assertEqual((shown["agent"], shown["id"], shown["title"], shown["cwd"], shown["model"]),
                         ("mcode", E, "An empty MiniMax history", self.api, MODEL))
        self.assertEqual((shown["turns_total"], shown["tool_calls"], shown["turns"]), (0, 0, []))
        code, out = self.h.run("where", E[:8], "--json")
        self.assertEqual(code, 0, out)
        found = json.loads(out)["sessions"][0]
        self.assertEqual((found["file"], found["data_folder"], found["archived"]),
                         (empty, os.path.dirname(empty), True))

    def test_ids_where_show_open_and_resume(self):
        for query in (M, M[:8], "mvs_" + M, "mvs_" + M[:12]):
            with self.subTest(query=query):
                code, out = self.h.run("where", query)
                self.assertEqual((code, out), (0, self.folder + "\n"))
        code, out = self.h.run("where", "mvs_" + M[:8], "--json")
        self.assertEqual(code, 0, out)
        found = json.loads(out)["sessions"][0]
        self.assertEqual((found["agent"], found["id"], found["profile"], found["file"],
                          found["data_folder"], found["archived"]),
                         ("mcode", M, self.home, self.messages, self.folder, False))
        self.assertIn(json.dumps(self.messages)[1:-1], out)

        code, out = self.h.run("show", M[:8])
        self.assertEqual(code, 0, out)
        self.assertIn('Session %s "%s" in %s, 2 turns' % (M[:8], TITLE, self.h.shown(self.home)), out)
        for text in (TITLE, "I'll inspect it.", "[external_agent_tool_call: read_file]",
                     "path: src/main.rs", "fn main() {}", "Done: " + TITLE):
            self.assertIn(text, out)
        for text in ("MCODE REASONING", "MCODE SIGNATURE", "MCODE TURN CONFIG", "HIDDEN MCODE RECORD",
                     "CUSTOM MCODE RECORD", "PLUGIN MCODE RECORD", "OLD MCODE HISTORY ARTIFACT",
                     "DISPLAY ROW FALLBACK", "OLD MCODE SNAPSHOT"):
            self.assertNotIn(text, out)
        self.assertIn("Left out: MiniMax Code's reasoning and signatures, images, files, hidden and custom records, "
                      "UI-only notices, previous generations, tool-output artifacts and subagent sessions.", out)
        code, out = self.h.run("show", "mvs_" + M[:8], "--json")
        self.assertEqual(code, 0, out)
        shown = json.loads(out)
        self.assertEqual((shown["agent"], shown["id"], shown["title"], shown["cwd"], shown["model"],
                          shown["turns_total"], shown["tool_calls"]),
                         ("mcode", M, TITLE, self.api, MODEL, 2, 1))
        text = "\n".join(turn["text"] for turn in shown["turns"])
        self.assertIn("[external_agent_tool_result]", text)
        self.assertIn("fn main() {}", text)
        self.assertNotIn("MCODE REASONING", text)

        expected = ["cd " + self.h.shown(self.api), "mcode --session mvs_" + M]
        code, out = self.h.run("open", M[:8], "--print")
        self.assertEqual((code, out.splitlines()), (0, expected))
        code, out = self.h.run("resume", "mvs_" + M[:8], "--print")
        self.assertEqual((code, out.splitlines()), (0, expected))

    def test_compaction_summary_and_object_tool_arguments(self):
        extra = [
            mcode_record("msg-object-call", {"role": "assistant", "content": [
                {"type": "toolCall", "id": "call-2", "name": "Bash",
                 "arguments": {"command": "ls src", "recursive": True}}]}),
            mcode_record("msg-object-result", {"role": "toolResult", "toolCallId": "call-2",
                                               "content": "object tool output"}),
            mcode_record("msg-compaction", {"role": "compactionSummary", "summary": "Earlier MiniMax context"}),
        ]
        self.h.mcode_session(self.home, self.api, C, "Object arguments and compaction", extra=extra)
        code, out = self.h.run("show", C[:8], "--json")
        self.assertEqual(code, 0, out)
        shown = json.loads(out)
        self.assertEqual((shown["turns_total"], shown["tool_calls"]), (4, 2))
        text = "\n".join(turn["text"] for turn in shown["turns"])
        for expected in ("[external_agent_tool_call: Bash]", "command: ls src", "recursive: true",
                         "object tool output", "Earlier MiniMax context", "Done: Object arguments and compaction"):
            self.assertIn(expected, text)

    def test_derived_and_manifest_history_fallbacks(self):
        derived = self.h.mcode_session(self.home, self.api, sid(16), "UTC-derived MiniMax history",
                                       fallback="derived")
        manifest = self.h.mcode_session(self.home, self.api, sid(17), "Manifest MiniMax history",
                                        fallback="manifest")
        for session, path in ((sid(16), derived), (sid(17), manifest)):
            with self.subTest(session=session):
                code, out = self.h.run("where", session[:8], "--json")
                self.assertEqual(code, 0, out)
                found = json.loads(out)["sessions"][0]
                self.assertEqual((found["file"], found["data_folder"]), (path, os.path.dirname(path)))
                code, out = self.h.run("show", session[:8], "--json")
                self.assertEqual(code, 0, out)
                self.assertEqual(json.loads(out)["id"], session)

    def test_internal_and_subagent_filtering(self):
        subagents = [
            (sid(32), {"kind": "task"}),
            (sid(33), {"purpose": "local-task:abc"}),
            (sid(34), {"purpose": "local-background-task:abc"}),
            (sid(35), {"purpose": "team-plan:abc"}),
            (sid(36), {"agent_name": "explore"}),
            (sid(37), {"agent_name": "worker"}),
            (sid(38), {"agent_name": "verifier"}),
        ]
        internal = [(sid(40), "peek", {"kind": "peek"}), (sid(41), "cron", {"kind": "cron"}),
                    (sid(42), "channel", {"kind": "channel"}),
                    (sid(43), "internal", {"visibility": "hidden"})]
        for session, options in subagents:
            self.h.mcode_session(self.home, self.api, session, "Subagent " + session, **options)
        for session, _, options in internal:
            self.h.mcode_session(self.home, self.api, session, "Internal " + session, **options)
        branch = sid(44)
        self.h.mcode_session(self.home, self.api, branch, "An ordinary MiniMax branch",
                             session_type="branch", parent=M)
        self.assertEqual({s["id"] for s in self.sessions()}, {M, branch})
        code, out = self.h.run("sessions", "--no-usage")
        self.assertEqual(code, 0, out)
        self.assertIn("An ordinary MiniMax branch\n", out)
        self.assertNotIn("Subagent ", out)
        self.assertNotIn("Internal ", out)
        listed = {s["id"]: s["entrypoint"] for s in self.sessions("--all")}
        self.assertEqual(listed[M], "cli")
        self.assertEqual(listed[branch], "cli")
        for session, entrypoint, _ in internal:
            self.assertEqual(listed[session], entrypoint)
        for session, _ in subagents:
            self.assertNotIn(session, listed)
        code, out = self.h.run("show", subagents[0][0][:8], "--json")
        self.assertEqual(code, 0, out)
        self.assertEqual(json.loads(out)["id"], subagents[0][0])

    def test_active_stale_and_unknown_leases(self):
        self.h.mcode_lease(self.home, M, NOW - 300)
        self.assertIsNone(self.sessions()[0]["open"])
        code, out = self.h.run("open", M[:8], "--print")
        self.assertEqual(code, 0, out)

        self.h.mcode_lease(self.home, M, NOW + 3600)
        self.assertEqual(self.sessions()[0]["open"], {"pid": None, "status": None, "in": "mcode"})
        code, out = self.h.run("open", M[:8], HATCHECK_TERMINAL="false")
        self.assertEqual(code, 1, out)
        self.assertIn(OPEN, out)

        self.h.mcode_lease(self.home, M, NOW - 300)
        self.assertIsNone(self.sessions()[0]["open"])
        code, out = self.h.run("open", M[:8], "--print")
        self.assertEqual(code, 0, out)

        db = sqlite3.connect(mcode_db(self.home))
        db.execute("DROP TABLE local_runtime_session_locks")
        db.commit()
        db.close()
        code, out = self.h.run("open", M[:8], HATCHECK_TERMINAL="false")
        self.assertEqual(code, 1, out)
        self.assertIn(OPEN, out)

    def test_windows_style_cwd_and_json_escaping(self):
        cwd = 'C:\\work\\mini "quoted"\\api'
        session = sid(48)
        self.h.mcode_session(self.home, cwd, session, 'Windows \\ paths and "quotes"')
        code, out = self.h.run("show", session[:8], "--json")
        self.assertEqual(code, 0, out)
        shown = json.loads(out)
        self.assertEqual(shown["cwd"], cwd)
        self.assertEqual(shown["title"], 'Windows \\ paths and "quotes"')
        self.assertIn(json.dumps(cwd)[1:-1], out)
        code, out = self.h.run("where", session[:8], "--json")
        self.assertEqual(code, 0, out)
        found = json.loads(out)["sessions"][0]
        self.assertTrue(found["file"].endswith(os.sep + "messages.jsonl"), found["file"])
        self.assertIn(json.dumps(found["file"])[1:-1], out)


class McodeBadHistories(unittest.TestCase):
    def setUp(self):
        self.h = h = FakeHome()
        self.addCleanup(h.cleanup)
        self.home = h.mcode_home()
        self.api = h.folder("code", "api")

    def ids(self):
        code, out = self.h.run("sessions", "--json", "--no-usage")
        self.assertEqual(code, 0, out)
        return {s["id"] for s in json.loads(out)["sessions"]}

    def set_history(self, session, value):
        db = sqlite3.connect(mcode_db(self.home))
        db.execute("UPDATE local_runtime_sessions SET history_relative_dir = ? WHERE session_id = ?",
                   (value, "mvs_" + session))
        db.commit()
        db.close()

    def assert_not_found(self, session):
        code, out = self.h.run("where", session)
        self.assertEqual((code, out), (1, ""))

    def test_unsupported_column_versions_and_missing_canonical_history(self):
        unsupported = sid(64)
        self.h.mcode_session(self.home, self.api, unsupported, "Future mcode catalog", columnar_version=4)
        missing = sid(65)
        path = self.h.mcode_session(self.home, self.api, missing, "No canonical mcode history")
        os.remove(path)
        self.assertTrue(os.path.isfile(os.path.join(os.path.dirname(path), "display.jsonl")))
        self.assertEqual(self.ids(), set())
        self.assert_not_found(unsupported)
        self.assert_not_found(missing)

    def test_unsafe_and_ambiguous_histories_are_skipped(self):
        unsafe = ["../outside", "/tmp/mcode", "C:/mcode", "a\\b\\c\\d", "a/b/c", "a//c/d", " a/b/c/d"]
        sessions = []
        for n, history in enumerate(unsafe, 80):
            session = sid(n)
            self.h.mcode_session(self.home, self.api, session, "Unsafe mcode history " + session)
            self.set_history(session, history)
            sessions.append(session)
        ambiguous = sid(96)
        original = self.h.mcode_session(self.home, self.api, ambiguous, "Ambiguous mcode manifests",
                                        fallback="manifest")
        duplicate = os.path.join(self.home, "v2", "sessions", "1999", "01", "02", "duplicate-manifest")
        shutil.copytree(os.path.dirname(original), duplicate)
        self.assertEqual(self.ids(), set())
        for session in sessions + [ambiguous]:
            with self.subTest(session=session):
                self.assert_not_found(session)

    def test_manifest_identity_is_required(self):
        for n, field, value in ((100, "schemaVersion", 2), (101, "sessionId", "mvs_somebody-else"),
                                (102, "createdAtMs", 1)):
            session = sid(n)
            path = self.h.mcode_session(self.home, self.api, session, "Wrong mcode manifest identity",
                                        fallback="manifest")
            manifest_path = os.path.join(os.path.dirname(path), "manifest.json")
            with open(manifest_path, encoding="utf-8") as fh:
                manifest = json.load(fh)
            manifest[field] = value
            write(manifest_path, json.dumps(manifest))
            with self.subTest(field=field):
                self.assert_not_found(session)
        self.assertEqual(self.ids(), set())

    def test_malformed_envelopes_fail_and_a_torn_tail_is_not_read(self):
        cases = []
        for n, content, error in (
            (112, "{not json}\n", "line 1 column"),
            (113, rec(message_id="msg", message={"role": "user", "content": "missing turn"}) + "\n",
             "invalid mcode history envelope"),
            (114, rec(message_id="msg", turn_id="turn", message="not an object") + "\n",
             "invalid mcode history message"),
        ):
            session = sid(n)
            path = self.h.mcode_session(self.home, self.api, session, "Malformed mcode history " + session)
            write(path, content)
            cases.append((session, error))
        for session, error in cases:
            with self.subTest(session=session):
                code, out = self.h.run("show", session[:8], "--json")
                self.assertEqual(code, 1, out)
                self.assertIn(error, out)

        torn = sid(115)
        path = self.h.mcode_session(self.home, self.api, torn, "A torn mcode tail")
        complete = mcode_record("whole", {"role": "user", "content": "The last complete message"})
        partial = mcode_record("torn", {"role": "assistant", "content": "TORN TAIL MUST NOT APPEAR"})
        write(path, rec(**complete) + "\n" + json.dumps(partial)[:-12])
        code, out = self.h.run("show", torn[:8], "--json")
        self.assertEqual(code, 0, out)
        shown = json.loads(out)
        self.assertEqual(shown["turns_total"], 1)
        self.assertEqual(shown["turns"][0]["text"], "The last complete message")
        self.assertNotIn("TORN TAIL", out)


class McodeDiscovery(unittest.TestCase):
    def setUp(self):
        self.h = h = FakeHome()
        self.addCleanup(h.cleanup)
        self.api = h.folder("code", "api")
        self.home = h.mcode_home()
        h.mcode_session(self.home, self.api, M, "Default MiniMax home")

    def ids(self, **env):
        code, out = self.h.run("sessions", "--json", "--no-usage", **env)
        self.assertEqual(code, 0, out)
        return {s["id"]: s for s in json.loads(out)["sessions"] if s["agent"] == "mcode"}

    def test_default_named_installer_and_legacy_discovery(self):
        work = self.h.mcode_home("work")
        self.h.mcode_session(work, self.api, N, "Named MiniMax home")
        installer = self.h.folder(".minimax-code", "bin")
        write(os.path.join(installer, "mcode"), "installer files are not a session store\n")
        self.h.folder(".minimax-incomplete")
        legacy = self.h.mcode_home(path=os.path.join(self.h.home, ".mavis"))
        self.h.mcode_session(legacy, self.api, L, "Legacy mavis data")

        listed = self.ids()
        self.assertEqual(set(listed), {M, N})
        self.assertEqual((listed[M]["profile"], listed[M]["resume"]),
                         (self.home, "mcode --session mvs_" + M))
        self.assertEqual((listed[N]["profile"], listed[N]["resume"]),
                         (work, resume_command(self.h, work, N)))
        code, out = self.h.run("open", N[:8], "--print")
        self.assertEqual((code, out.splitlines()),
                         (0, ["cd " + self.h.shown(self.api), resume_command(self.h, work, N)]))
        before = tree_bytes(work)
        code, out = self.h.run("open", "--login", "mcode:work", "--print")
        self.assertEqual(code, 0, out)
        self.assertEqual(out.splitlines()[-1], resume_command(self.h, work) + " login")
        self.assertEqual(tree_bytes(work), before)

    def test_override_precedence_and_environment_scrubbing(self):
        custom = self.h.mcode_home(path=os.path.join(self.h.root, "elsewhere", "minimax-custom"))
        legacy = self.h.mcode_home(path=os.path.join(self.h.root, "elsewhere", "mavis-custom"))
        self.h.mcode_session(custom, self.api, C, "MINIMAX_DATA_DIR MiniMax home")
        self.h.mcode_session(legacy, self.api, L, "MAVIS_DATA_DIR MiniMax home")
        with mock.patch.dict(os.environ, {"MINIMAX_DATA_DIR": custom, "MAVIS_DATA_DIR": legacy}):
            self.assertEqual(set(self.ids()), {M})
        self.assertEqual(set(self.ids(MAVIS_DATA_DIR=legacy)), {M, L})
        listed = self.ids(MINIMAX_DATA_DIR=custom, MAVIS_DATA_DIR=legacy)
        self.assertEqual(set(listed), {M, C})
        self.assertEqual(listed[C]["profile"], custom)
        self.assertEqual(listed[C]["resume"], resume_command(self.h, custom, C))
        code, out = self.h.run("open", C[:8], "--print", MINIMAX_DATA_DIR=custom, MAVIS_DATA_DIR=legacy)
        self.assertEqual((code, out.splitlines()),
                         (0, ["cd " + self.h.shown(self.api), resume_command(self.h, custom, C)]))
        empty = self.h.folder("not-an-mcode-store")
        self.assertEqual(set(self.ids(MINIMAX_DATA_DIR=empty)), {M})


class McodeUsageAndBindings(unittest.TestCase):
    def setUp(self):
        self.h = h = FakeHome()
        self.addCleanup(h.cleanup)
        self.home = h.mcode_home()
        self.work = h.mcode_home("work")
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
        self.assertIn("%s  MiniMax · no data (%s)" % (self.h.shown(self.home), NO_USAGE), out)
        code, out = self.h.run("usage", "--json")
        self.assertEqual(code, 0, out)
        accounts = [a for a in json.loads(out)["accounts"] if a["agent"] == "mcode"]
        self.assertEqual({a["profiles"][0] for a in accounts}, {self.home, self.work})
        for account in accounts:
            self.assertEqual(len(account["profiles"]), 1)
            self.assertEqual((account["plan"], account["limits"], account["source"], account["why"]),
                             ("MiniMax", [], "none", NO_USAGE))

    def test_bind_default_named_which_and_preservation(self):
        code, out = self.h.run("bind", "default", cwd=self.api)
        self.assertEqual(code, 0, out)
        self.assertIn("MiniMax Code " + self.h.shown(self.home), out)
        self.assertEqual(self.saved(), {"claude": "default", "codex": "default", "mcode": "default"})
        code, out = self.h.run("bind", "--mcode", "work", cwd=self.api)
        self.assertEqual(code, 0, out)
        named = self.saved()
        self.assertEqual(named, {"claude": "default", "codex": "default", "mcode": "work"})
        code, out = self.h.run("bind", "--which", "mcode", cwd=self.api)
        self.assertEqual((code, out), (0, "work\n"))
        code, out = self.h.run("bind", "--codex", "default", cwd=self.api)
        self.assertEqual(code, 0, out)
        self.assertEqual(self.saved(), named)
        code, out = self.h.run("bind", cwd=self.api)
        self.assertIn("MiniMax Code " + self.h.shown(self.work), out)
        code, out = self.h.run("bind", "--list")
        self.assertEqual(code, 0, out)
        self.assertIn(self.h.shown(self.api), out)
        self.assertIn("MiniMax Code " + self.h.shown(self.work), out)
        code, out = self.h.run("bind", "--mcode", "default", cwd=self.api)
        self.assertEqual(code, 0, out)
        self.assertEqual(self.saved(), {"claude": "default", "codex": "default", "mcode": "default"})
        self.assertEqual(named["mcode"], "work")

    def test_named_auto_binding_custom_path_and_store_validation(self):
        self.h.profile("work")
        self.h.codex_home("work")
        code, out = self.h.run("bind", "work", cwd=self.api)
        self.assertEqual(code, 0, out)
        self.assertEqual(self.saved()["mcode"], "work")
        custom = self.h.mcode_home(path=os.path.join(self.h.root, "elsewhere", "bound-minimax"))
        code, out = self.h.run("bind", "--mcode", custom, cwd=self.api)
        self.assertEqual(code, 0, out)
        self.assertEqual(self.saved()["mcode"], custom)
        code, out = self.h.run("bind", "--which", "mcode", cwd=self.api)
        self.assertEqual((code, out), (0, custom + "\n"))

        self.h.profile("empty")
        self.h.codex_home("empty")
        self.h.mcode_home("real")
        self.h.folder(".minimax-empty")
        other = self.h.folder("code", "other")
        code, out = self.h.run("bind", "empty", cwd=other)
        self.assertEqual(code, 0, out)
        self.assertNotIn("mcode", self.saved(other))
        before = self.saved(other)
        code, out = self.h.run("bind", "--mcode", "empty", cwd=other)
        self.assertEqual(code, 1, out)
        self.assertIn("isn't a MiniMax Code data home", out)
        self.assertEqual(self.saved(other), before)


class McodeConversion(unittest.TestCase):
    def setUp(self):
        self.h = h = FakeHome()
        self.addCleanup(h.cleanup)
        self.home = h.mcode_home()
        self.claude = h.profile("default")
        self.api = h.folder("code", "api")
        self.messages = h.mcode_session(self.home, self.api, M, TITLE)

    def claude_transcript(self, session):
        project = re.sub(r"[^A-Za-z0-9]", "-", self.api)
        return os.path.join(self.claude, "projects", project, session + ".jsonl")

    def test_convert_out_dry_run_move_and_fork_with_a_stale_lease(self):
        self.h.mcode_lease(self.home, M, NOW - 300)
        original = tree_bytes(self.home)
        before = tree_bytes(self.claude)
        code, out = self.h.run("move", M[:8], "default", "--dry-run")
        self.assertEqual(code, 0, out)
        self.assertIn('Would convert MiniMax Code session %s "%s"' % (M[:8], TITLE), out)
        self.assertEqual(tree_bytes(self.home), original)
        self.assertEqual(tree_bytes(self.claude), before)

        code, out = self.h.run("move", M[:8], "default", "--json")
        self.assertEqual(code, 0, out)
        moved = json.loads(out)
        self.assertEqual((moved["action"], moved["agent"], moved["session"], moved["target_agent"],
                          moved["target"], moved["prompts"], moved["tool_calls"]),
                         ("convert", "mcode", M, "claude", self.claude, 1, 1))
        new = moved["new_session"]
        self.assertTrue(new)
        path = self.claude_transcript(new)
        self.assertTrue(os.path.isfile(path), path)
        text = read_bytes(path).decode("utf-8")
        for expected in ("[Imported from MiniMax Code session " + M, TITLE, "read_file", "fn main() {}"):
            self.assertIn(expected, text)
        for excluded in ("MCODE REASONING", "MCODE SIGNATURE", "HIDDEN MCODE RECORD", "OLD MCODE SNAPSHOT"):
            self.assertNotIn(excluded, text)
        self.assertEqual(tree_bytes(self.home), original)

        code, out = self.h.run("fork", M[:8], "default", "--json")
        self.assertEqual(code, 0, out)
        forked = json.loads(out)
        self.assertEqual((forked["action"], forked["agent"], forked["target_agent"]),
                         ("convert", "mcode", "claude"))
        self.assertNotIn(forked["new_session"], (None, M, new))
        self.assertTrue(os.path.isfile(self.claude_transcript(forked["new_session"])))
        self.assertEqual(tree_bytes(self.home), original)

    def test_active_lease_refuses_conversion_without_writes(self):
        self.h.mcode_lease(self.home, M, NOW + 3600)
        original = tree_bytes(self.home)
        before = tree_bytes(self.claude)
        for command in ("move", "fork"):
            for extra in (("--dry-run",), ()):
                with self.subTest(command=command, extra=extra):
                    code, out = self.h.run(command, M[:8], "default", *extra)
                    self.assertEqual(code, 1, out)
                    self.assertIn(OPEN, out)
                    self.assertEqual(tree_bytes(self.home), original)
                    self.assertEqual(tree_bytes(self.claude), before)

    def test_same_agent_copies_are_refused_without_changes(self):
        work = self.h.mcode_home("work")
        before, target_before = tree_bytes(self.home), tree_bytes(work)
        for command in ("move", "fork"):
            with self.subTest(command=command):
                code, out = self.h.run(command, M[:8], "mcode:work")
                self.assertEqual(code, 1, out)
                self.assertIn(COPY_REFUSED, out)
                self.assertEqual(tree_bytes(self.home), before)
                self.assertEqual(tree_bytes(work), target_before)

    def test_a_claude_code_session_converted_for_mcode_is_a_file_mcode_starts_on(self):
        source = self.h.session(self.claude, self.api, A, "A Claude Code session")
        source_bytes = read_bytes(source)
        work = self.h.mcode_home("work")
        before, work_before = tree_bytes(self.home), tree_bytes(work)
        code, out = self.h.run("move", A[:8], "mcode", "--json")
        self.assertEqual(code, 0, out)
        d = json.loads(out)
        self.assertEqual((d["action"], d["agent"], d["target_agent"], d["new_session"], d["target"], d["paste"]),
                         ("convert", "claude", "mcode", None, self.home, None))
        self.assertTrue(d["file"].endswith(".md") and os.path.exists(d["file"]), d["file"])
        self.assertTrue(d["resume"].startswith("mcode "), d["resume"])  # the prompt is the task mcode starts on
        self.assertIn(PROMPT % d["file"], d["resume"].replace("'\\''", "'").replace("''", "'"))
        with open(d["file"], encoding="utf-8") as fh:
            text = fh.read()
        self.assertTrue(text.startswith("# Conversation imported from Claude Code session " + A), text[:80])
        self.assertIn("A Claude Code session", text)
        self.assertEqual(read_bytes(source), source_bytes)  # the original stays
        self.assertEqual(tree_bytes(self.home), before)  # and mcode's store isn't written
        code, out = self.h.run("fork", A[:8], "mcode:work", "--dry-run")
        self.assertEqual(code, 0, out)
        self.assertIn('Would convert Claude Code session %s "A Claude Code session" from %s into a conversation for '
                      'MiniMax Code in ' % (A[:8], self.h.shown(self.claude)), out)
        self.assertIn("so the session starts when mcode reads the file.", out)
        self.assertIn("Then start it with:", out)
        # The start command: in PowerShell's words the variable is set before it and cleared after.
        before_words, after_words = mcode_words(self.h, work)
        line = next(l for l in out.splitlines() if "mcode " in l and "<new id>.md" in l)
        self.assertTrue(line.startswith("  %smcode " % before_words) and line.endswith(after_words), line)
        self.assertEqual(tree_bytes(work), work_before)


class McodeRelocate(unittest.TestCase):
    def test_skip_and_dry_run_preserve_every_mcode_byte(self):
        h = FakeHome()
        self.addCleanup(h.cleanup)
        home, claude = h.mcode_home(), h.profile("default")
        old = h.folder("code", "api")
        new = os.path.join(h.home, "code", "api2")
        messages = h.mcode_session(home, old, R, "An mcode session left in place", 70)
        transcript = h.session(claude, old, A, "A Claude session that moves", 70)
        os.rename(old, new)
        before = tree_bytes(home)
        skipped = ("Left alone: MiniMax Code session %s in %s keeps shared SQLite state and absolute manifest paths "
                   "with related assets, which stay as they were." % (R[:8], h.shown(home)))
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
        self.assertTrue(os.path.isfile(messages))
        moved = os.path.join(claude, "projects", re.sub(r"[^A-Za-z0-9]", "-", new), A + ".jsonl")
        self.assertTrue(os.path.isfile(moved), moved)


if __name__ == "__main__":
    unittest.main()
