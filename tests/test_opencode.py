"""OpenCode (opencode) in hatcheck: the data folder and database as opencode 2.1.4 keeps them,
read-only conversion out, the session data written for its own importer, and relocation skips.
Every store is a throwaway fixture; no real opencode, credentials, background service or network."""
import json
import os
import re
import sqlite3
import unittest

from helpers import NOW, WINDOWS, FakeHome, opencode_db, opencode_sid

O = opencode_sid(0x7f3a2b1c0d9e)
N = opencode_sid(0xd00000000001)
E = opencode_sid(0xe00000000001)
A = "aaaaaaaa-0000-4000-8000-000000000001"
R = opencode_sid(0xe00000000002)
TITLE = "Fix the uploader with OpenCode"
MODEL = "anthropic/claude-opus-5-5"
NO_USAGE = "opencode has no plan-usage command: its stats count tokens spent (2.1.4)"
COPY_REFUSED = ("OpenCode keeps its sessions in one database hatcheck doesn't write; convert session %s for another "
                "agent instead" % O[:8])
LEFT_OUT = ("Left out: OpenCode's reasoning, the contents of files attached or returned, and its own records (skills, "
            "what it adds to the conversation itself, and its agent, model and folder switches).")
HIDDEN = ("INJECTED OPENCODE CONTEXT", "OPENCODE REASONING")


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


def db_bytes(home):
    """The database's rows as text, which a WAL checkpoint doesn't change the way it changes the
    files' bytes."""
    db = sqlite3.connect(opencode_db(home))
    rows = []
    for table in ("project", "session_v2", "session_message", "credential", "account"):
        rows.append((table, db.execute("SELECT * FROM %s ORDER BY 1" % table).fetchall()))
    db.close()
    return rows


class OpencodeSessions(unittest.TestCase):
    def setUp(self):
        self.h = h = FakeHome()
        self.addCleanup(h.cleanup)
        self.home = h.opencode_home()
        self.api = h.folder("code", "api")
        self.db = h.opencode_session(self.home, self.api, O, TITLE, 1)

    def sessions(self, *args, **env):
        code, out = self.h.run("sessions", "--json", "--no-usage", *args, **env)
        self.assertEqual(code, 0, out)
        return json.loads(out)["sessions"]

    def test_listing_and_json_metadata(self):
        code, out = self.h.run("sessions", "--no-usage")
        self.assertEqual(code, 0, out)
        self.assertIn("OpenCode CLI sessions", out)
        self.assertIn(TITLE, out)
        self.assertIn("opencode --session ses_" + O, out)
        self.assertIn(self.h.shown(self.home) + " (1)", out)
        for text in HIDDEN:
            self.assertNotIn(text, out)
        listed = self.sessions()
        self.assertEqual([s["id"] for s in listed], [O])
        o = listed[0]
        self.assertEqual((o["agent"], o["profile"], o["title"], o["cwd"], o["entrypoint"], o["branch"]),
                         ("opencode", self.home, TITLE, self.api, "cli", None))
        self.assertEqual((o["resume"], o["model"], o["open"]), ("opencode --session ses_" + O, MODEL, None))

    def test_ids_with_or_without_the_prefix_where_show_open_and_resume(self):
        for query in (O, O[:8], "ses_" + O, "SES_" + O[:8].upper()):
            with self.subTest(query=query):
                code, out = self.h.run("where", query)
                self.assertEqual((code, out), (0, self.db + "\n"))
        code, out = self.h.run("where", O[:8], "--json")
        self.assertEqual(code, 0, out)
        found = json.loads(out)["sessions"][0]
        self.assertEqual((found["agent"], found["id"], found["profile"], found["file"], found["data_folder"],
                          found["archived"]), ("opencode", O, self.home, self.db, None, False))
        self.assertIn(json.dumps(self.db)[1:-1], out)

        code, out = self.h.run("show", O[:8])
        self.assertEqual(code, 0, out)
        self.assertIn('Session %s "%s" in %s, 2 turns' % (O[:8], TITLE, self.h.shown(self.home)), out)
        for text in (TITLE, "[image]", "I'll inspect it.", "[external_agent_tool_call: read]", "filePath: src/main.rs",
                     "fn main() {}", "[external_agent_tool_call: shell]", "command: ls", "Cargo.toml", "Done: " + TITLE,
                     LEFT_OUT):
            self.assertIn(text, out)
        for text in HIDDEN:
            self.assertNotIn(text, out)
        code, out = self.h.run("show", "ses_" + O, "--json")
        self.assertEqual(code, 0, out)
        shown = json.loads(out)
        self.assertEqual((shown["agent"], shown["id"], shown["title"], shown["cwd"], shown["model"],
                          shown["turns_total"], shown["tool_calls"]), ("opencode", O, TITLE, self.api, MODEL, 2, 2))
        text = "\n".join(turn["text"] for turn in shown["turns"])
        self.assertIn("[external_agent_tool_result]", text)
        self.assertNotIn("OPENCODE REASONING", text)

        expected = ["cd " + self.h.shown(self.api), "opencode --session ses_" + O]
        code, out = self.h.run("open", O[:8], "--print")
        self.assertEqual((code, out.splitlines()), (0, expected))
        code, out = self.h.run("resume", O[:8], "--print")
        self.assertEqual((code, out.splitlines()), (0, expected))

    def test_the_rows_title_wins_and_the_first_prompt_stands_in(self):
        self.h.opencode_session(self.home, self.api, N, "", 2, prompt="The first prompt")
        self.h.opencode_session(self.home, self.api, E, "Uploader fix", 3, prompt="The first prompt")
        titles = {s["id"]: s["title"] for s in self.sessions()}
        self.assertEqual((titles[N], titles[E], titles[O]), ("The first prompt", "Uploader fix", TITLE))
        code, out = self.h.run("show", E[:8], "--json")
        self.assertEqual(code, 0, out)
        self.assertEqual(json.loads(out)["title"], "Uploader fix")

    def test_an_archived_session_is_found_and_says_so(self):
        self.h.opencode_session(self.home, self.api, E, "An archived OpenCode session", 2, archived=True)
        self.assertIn(E, {s["id"] for s in self.sessions()})
        code, out = self.h.run("where", E[:8], "--json")
        self.assertEqual(code, 0, out)
        found = json.loads(out)["sessions"][0]
        self.assertEqual((found["profile"], found["file"], found["archived"]), (self.home, self.db, True))

    def test_child_sessions_are_hidden_and_an_empty_one_is_not_listed(self):
        self.h.opencode_session(self.home, self.api, N, "A subagent's child session", parent=O)
        self.h.opencode_session(self.home, self.api, E, "Emptied", empty=True)
        self.assertEqual({s["id"] for s in self.sessions()}, {O})
        self.assertEqual({s["id"] for s in self.sessions("--all")}, {O})
        code, out = self.h.run("show", N[:8], "--json")  # hidden from the list, still readable
        self.assertEqual(code, 0, out)
        self.assertEqual(json.loads(out)["id"], N)
        code, out = self.h.run("where", E[:8])
        self.assertEqual((code, out), (0, self.db + "\n"))  # a row, listed by where, with no conversation
        code, out = self.h.run("show", E[:8], "--json")
        self.assertEqual(code, 0, out)
        self.assertEqual(json.loads(out)["turns_total"], 0)

    def test_windows_style_cwd_and_json_escaping(self):
        cwd = 'C:\\work\\opencode "quoted"\\api'
        session = opencode_sid(48)
        self.h.opencode_session(self.home, cwd, session, 'Windows \\ paths and "quotes"')
        code, out = self.h.run("show", session[:8], "--json")
        self.assertEqual(code, 0, out)
        shown = json.loads(out)
        self.assertEqual((shown["cwd"], shown["title"]), (cwd, 'Windows \\ paths and "quotes"'))
        self.assertIn(json.dumps(cwd)[1:-1], out)


class OpencodeDiscovery(unittest.TestCase):
    def setUp(self):
        self.h = h = FakeHome()
        self.addCleanup(h.cleanup)
        self.api = h.folder("code", "api")

    def ids(self, **env):
        code, out = self.h.run("sessions", "--json", "--no-usage", **env)
        self.assertEqual(code, 0, out)
        return {s["id"]: s for s in json.loads(out)["sessions"] if s["agent"] == "opencode"}

    def test_the_default_data_folder_and_xdg_data_home(self):
        self.assertEqual(self.ids(), {})
        home = self.h.opencode_home()
        self.assertEqual(home, os.path.join(self.h.home, ".local", "share", "opencode"))
        self.h.opencode_session(home, self.api, O, "Default OpenCode store")
        self.h.folder(".opencode", "bin")  # the installer's folder, not a data folder
        self.h.folder(".opencode-work")  # a name and nothing more: no data folder is made for it
        listed = self.ids()
        self.assertEqual(set(listed), {O})
        self.assertEqual((listed[O]["profile"], listed[O]["resume"]), (home, "opencode --session ses_" + O))
        # XDG_DATA_HOME moves the data folder, as it moves plain opencode's.
        data = self.h.folder("xdg-data")
        custom = self.h.opencode_home(path=os.path.join(data, "opencode"))
        self.h.opencode_session(custom, self.api, N, "XDG_DATA_HOME OpenCode store")
        listed = self.ids(XDG_DATA_HOME=data)
        self.assertEqual(set(listed), {N})
        self.assertEqual((listed[N]["profile"], listed[N]["resume"]), (custom, "opencode --session ses_" + N))
        code, out = self.h.run("open", "--login", "opencode", "--print")
        self.assertEqual(code, 0, out)
        self.assertEqual(out.splitlines()[-1], "opencode auth login")

    def test_a_claude_profile_is_not_an_opencode_store(self):
        home = self.h.opencode_home()
        self.h.opencode_session(home, self.api, O, "An OpenCode session")
        profile = self.h.profile("default")
        self.h.session(profile, self.api, A, "A Claude Code session")
        code, out = self.h.run("sessions", "--json", "--no-usage")
        self.assertEqual(code, 0, out)
        agents = {s["id"]: s["agent"] for s in json.loads(out)["sessions"]}
        self.assertEqual(agents, {O: "opencode", A: "claude"})


class OpencodeUsageAndBindings(unittest.TestCase):
    def setUp(self):
        self.h = h = FakeHome()
        self.addCleanup(h.cleanup)
        self.home = h.opencode_home()
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
        self.assertIn("%s  OpenCode · no data (%s)" % (self.h.shown(self.home), NO_USAGE), out)
        code, out = self.h.run("usage", "--json")
        self.assertEqual(code, 0, out)
        accounts = [a for a in json.loads(out)["accounts"] if a["agent"] == "opencode"]
        self.assertEqual([a["profiles"] for a in accounts], [[self.home]])
        self.assertEqual((accounts[0]["plan"], accounts[0]["limits"], accounts[0]["source"], accounts[0]["why"]),
                         ("OpenCode", [], "none", NO_USAGE))

    def test_bind_default_explicit_which_and_preservation(self):
        code, out = self.h.run("bind", "default", cwd=self.api)
        self.assertEqual(code, 0, out)
        self.assertIn("OpenCode " + self.h.shown(self.home), out)
        self.assertEqual(self.saved(), {"claude": "default", "codex": "default", "opencode": "default"})
        code, out = self.h.run("bind", "--which", "opencode", cwd=self.api)
        self.assertEqual((code, out), (0, "default\n"))
        code, out = self.h.run("bind", "--codex", "default", cwd=self.api)
        self.assertEqual(code, 0, out)
        self.assertEqual(self.saved()["opencode"], "default")
        code, out = self.h.run("bind", "--opencode", self.home, cwd=self.api)
        self.assertEqual(code, 0, out)
        self.assertEqual(self.saved()["opencode"], os.path.realpath(self.home))
        code, out = self.h.run("bind", "--list")
        self.assertEqual(code, 0, out)
        self.assertIn("OpenCode " + self.h.shown(self.home), out)
        empty = self.h.folder("not-an-opencode-store")
        code, out = self.h.run("bind", "--opencode", empty, cwd=self.api)
        self.assertEqual(code, 1, out)
        self.assertIn("isn't an OpenCode data folder", out)


class OpencodeConversion(unittest.TestCase):
    def setUp(self):
        self.h = h = FakeHome()
        self.addCleanup(h.cleanup)
        self.home = h.opencode_home()
        self.claude = h.profile("default")
        self.api = h.folder("code", "api")
        h.opencode_session(self.home, self.api, O, TITLE)

    def claude_transcript(self, session):
        project = re.sub(r"[^A-Za-z0-9]", "-", self.api)
        return os.path.join(self.claude, "projects", project, session + ".jsonl")

    def test_convert_out_dry_run_move_and_fork(self):
        original = db_bytes(self.home)
        before = tree_bytes(self.claude)
        code, out = self.h.run("move", O[:8], "default", "--dry-run")
        self.assertEqual(code, 0, out)
        self.assertIn('Would convert OpenCode session %s "%s"' % (O[:8], TITLE), out)
        self.assertEqual(db_bytes(self.home), original)
        self.assertEqual(tree_bytes(self.claude), before)

        code, out = self.h.run("move", O[:8], "default", "--json")
        self.assertEqual(code, 0, out)
        moved = json.loads(out)
        self.assertEqual((moved["action"], moved["agent"], moved["session"], moved["target_agent"], moved["target"],
                          moved["prompts"], moved["tool_calls"]), ("convert", "opencode", O, "claude", self.claude, 1, 2))
        new = moved["new_session"]
        self.assertTrue(new)
        path = self.claude_transcript(new)
        self.assertTrue(os.path.isfile(path), path)
        text = read_bytes(path).decode("utf-8")
        for expected in ("[Imported from OpenCode session " + O, TITLE, "read", "fn main() {}", "Cargo.toml"):
            self.assertIn(expected, text)
        for excluded in HIDDEN:
            self.assertNotIn(excluded, text)
        self.assertEqual(db_bytes(self.home), original)

        code, out = self.h.run("fork", "ses_" + O, "default", "--json")
        self.assertEqual(code, 0, out)
        forked = json.loads(out)
        self.assertEqual((forked["action"], forked["agent"], forked["target_agent"]), ("convert", "opencode", "claude"))
        self.assertNotIn(forked["new_session"], (None, O, new))
        self.assertTrue(os.path.isfile(self.claude_transcript(forked["new_session"])))
        self.assertEqual(db_bytes(self.home), original)

    def test_a_copy_into_its_own_store_is_refused_without_changes(self):
        before = db_bytes(self.home)
        for command in ("move", "fork"):
            with self.subTest(command=command):
                code, out = self.h.run(command, O[:8], "opencode")
                self.assertEqual(code, 1, out)
                self.assertIn("already in " + self.h.shown(self.home), out)
                self.assertEqual(db_bytes(self.home), before)

    def test_a_claude_code_session_converted_for_opencode_is_a_file_its_importer_takes(self):
        source = self.h.session(self.claude, self.api, A, "A Claude Code session", model="claude-opus-5-5")
        source_bytes = read_bytes(source)
        before = db_bytes(self.home)
        code, out = self.h.run("move", A[:8], "opencode", "--json")
        self.assertEqual(code, 0, out)
        d = json.loads(out)
        self.assertEqual((d["action"], d["agent"], d["target_agent"], d["new_session"], d["target"], d["brief"]),
                         ("convert", "claude", "opencode", None, self.home, False))
        self.assertTrue(d["file"].endswith(".json") and os.path.exists(d["file"]), d["file"])
        self.assertTrue(d["resume"].startswith("opencode session import "), d["resume"])
        self.assertTrue(d["resume"].endswith(os.path.basename(d["file"])), d["resume"])
        with open(d["file"], encoding="utf-8") as fh:
            data = json.load(fh)
        # The session data opencode's own export prints and its import takes.
        sid = os.path.basename(d["file"])[:-len(".json")]
        self.assertEqual(len(sid), 26)
        info = data["info"]
        self.assertEqual((info["id"], info["location"]["directory"], info["title"], info["cost"]),
                         ("ses_" + sid, self.api, "A Claude Code session", 0))
        self.assertEqual(info["tokens"], {"input": 0, "output": 0, "reasoning": 0, "cache": {"read": 0, "write": 0}})
        self.assertTrue(info["time"]["created"] <= info["time"]["updated"])
        self.assertEqual(info["metadata"]["imported_from"], "Claude Code session " + A)
        messages = data["messages"]
        self.assertEqual([m["type"] for m in messages], ["user", "assistant"])
        self.assertIn("[Imported from Claude Code session " + A, messages[0]["text"])
        self.assertTrue(messages[0]["id"].startswith("msg_") and len(messages[0]["id"]) == 30)
        self.assertEqual(messages[1]["content"], [{"type": "text", "text": "ok"}])
        self.assertEqual(messages[1]["model"], {"providerID": "claude", "id": "claude-opus-5-5"})
        self.assertEqual(messages[1]["agent"], "build")
        self.assertIn("completed", messages[1]["time"])
        self.assertEqual(read_bytes(source), source_bytes)  # the original stays
        self.assertEqual(db_bytes(self.home), before)  # and opencode's store isn't written
        code, out = self.h.run("fork", A[:8], "opencode", "--dry-run")
        self.assertEqual(code, 0, out)
        self.assertIn('Would convert Claude Code session %s "A Claude Code session" from %s into a session file for '
                      'OpenCode in ' % (A[:8], self.h.shown(self.claude)), out)
        self.assertIn("so the session exists once that has run", out)
        self.assertIn("Then import it with:", out)
        self.assertIn("  opencode session import ", out)
        self.assertIn("<new id>.json", out)
        self.assertIn("  opencode --session ses_<new id>", out)
        self.assertEqual(db_bytes(self.home), before)


class OpencodeRelocate(unittest.TestCase):
    def test_skip_and_dry_run_preserve_the_store(self):
        h = FakeHome()
        self.addCleanup(h.cleanup)
        home, claude = h.opencode_home(), h.profile("default")
        old = h.folder("code", "api")
        new = os.path.join(h.home, "code", "api2")
        h.opencode_session(home, old, R, "An opencode session left in place", 70)
        transcript = h.session(claude, old, A, "A Claude session that moves", 70)
        os.rename(old, new)
        before = db_bytes(home)
        skipped = ("Left alone: OpenCode session %s in %s records the folder in opencode's database, which stays as "
                   "it was." % (R[:8], h.shown(home)))
        code, out = h.run("relocate", old, new, "--dry-run")
        self.assertEqual(code, 0, out)
        self.assertIn("Would relocate 1 session", out)
        self.assertIn(skipped, out)
        self.assertEqual(db_bytes(home), before)
        self.assertTrue(os.path.isfile(transcript))
        code, out = h.run("relocate", old, new)
        self.assertEqual(code, 0, out)
        self.assertIn("Relocated 1 session", out)
        self.assertIn(skipped, out)
        self.assertEqual(db_bytes(home), before)
        moved = os.path.join(claude, "projects", re.sub(r"[^A-Za-z0-9]", "-", new), A + ".jsonl")
        self.assertTrue(os.path.isfile(moved), moved)


if __name__ == "__main__":
    unittest.main()
