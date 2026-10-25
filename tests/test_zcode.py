"""ZCode (zcode) in hatcheck: its sessions read from its database and listed with their resume
commands, found, shown, converted for Claude Code, and a Claude Code session converted into a
file for zcode to read; its database is never written. Against a throwaway home: no real ZCode
folder, no network."""
import json
import os
import re
import sqlite3
import unittest

from helpers import FakeHome

A = "aaaaaaaa-0000-4000-8000-000000000001"  # a Claude Code session
Z = "1fb068a4-b6d4-4a9f-b2d3-6e428e094fbe"  # a ZCode session (zcode calls it sess_<this>)
S = "3415c270-762b-4d0c-8961-cd9dd26f5fe2"  # one zcode ran itself


class ZcodeSessions(unittest.TestCase):
    def setUp(self):
        self.h = h = FakeHome()
        self.addCleanup(h.cleanup)
        self.claude = h.profile("default")
        self.root = h.zcode_root()
        self.db = os.path.join(self.root, "cli", "db", "db.sqlite")
        self.api = h.folder("code", "api")
        h.session(self.claude, self.api, A, "A Claude Code session", 3)
        h.zcode_session(self.root, self.api, Z, "Fix the uploader with zcode", 1, artifacts=True)
        h.zcode_session(self.root, self.api, S, "A subagent", 1, parent=Z, task_type="subagent_child")

    def rows(self):
        db = sqlite3.connect(self.db)
        try:
            return db.execute("select id, title from session order by id").fetchall()
        finally:
            db.close()

    def test_listed_with_the_others(self):
        code, out = self.h.run("sessions", "--no-usage")
        self.assertEqual(code, 0, out)
        self.assertIn("Claude Code and ZCode CLI sessions", out)
        self.assertIn("Fix the uploader with zcode\n", out)
        self.assertIn("zcode --resume sess_" + Z, out)  # the whole ID, as zcode names it
        self.assertNotIn(S, out)  # one zcode ran itself is never listed
        self.assertIn(self.h.shown(self.root) + " (1)", out)

    def test_a_session_zcode_deleted_is_gone(self):
        # ZCode 0.17.0 keeps a deleted session's row, marked in a column it adds to the table.
        db = sqlite3.connect(self.db)
        db.execute("alter table session add column time_deleted integer")
        db.execute("update session set time_deleted = 1792881000000 where id = ?", ("sess_" + Z,))
        db.commit()
        db.close()
        code, out = self.h.run("sessions", "--no-usage")
        self.assertEqual(code, 0, out)
        self.assertNotIn(Z, out)
        code, out = self.h.run("where", Z[:8])
        self.assertEqual(code, 1, out)

    def test_json(self):
        code, out = self.h.run("sessions", "--json", "--no-usage")
        self.assertEqual(code, 0, out)
        z = next(s for s in json.loads(out)["sessions"] if s["id"] == Z)
        self.assertEqual((z["agent"], z["profile"], z["cwd"], z["entrypoint"], z["resume"]),
                         ("zcode", self.root, self.api, "cli", "zcode --resume sess_" + Z))
        self.assertEqual((z["model"], z["branch"], z["open"]), (None, None, None))

    def test_where_show_and_open(self):
        h = self.h
        artifacts = os.path.join(self.root, "cli", "artifacts", "sess_" + Z)
        code, out = h.run("where", Z[:8])
        self.assertEqual(code, 0, out)
        self.assertEqual(out.splitlines(), [self.db, artifacts])  # the database, and its folder of tool results
        code, out = h.run("where", "sess_" + Z[:8])  # pasted with zcode's prefix
        self.assertEqual(out.splitlines()[0], self.db)
        code, out = h.run("where", Z[:8], "--json")
        copy = json.loads(out)["sessions"][0]
        self.assertEqual((copy["agent"], copy["id"], copy["profile"], copy["file"], copy["data_folder"], copy["archived"]),
                         ("zcode", Z, self.root, self.db, artifacts, False))
        code, out = h.run("show", Z[:8])
        self.assertEqual(code, 0, out)
        self.assertIn('Session 1fb068a4 "Fix the uploader with zcode" in %s, 2 turns' % h.shown(self.root), out)
        self.assertIn("[external_agent_tool_call: Bash]", out)
        self.assertIn("command: ls src", out)
        self.assertIn("main.py", out)  # the tool's output
        self.assertIn("Done: Fix the uploader with zcode", out)
        self.assertNotIn("kept to itself", out)  # zcode's reasoning
        self.assertIn("Left out: ZCode's reasoning, images, checkpoints and subagent sessions.", out)
        code, out = h.run("show", Z[:8], "--json")
        self.assertEqual(json.loads(out)["model"], "GLM-5.3")
        code, out = h.run("open", Z[:8], "--print")
        self.assertEqual(out.splitlines(), ["cd " + h.shown(self.api), "zcode --resume sess_" + Z])

    def test_converted_for_claude_code(self):
        h = self.h
        before = self.rows()
        code, out = h.run("move", Z[:8], "default")
        self.assertEqual(code, 0, out)
        self.assertIn('Converted ZCode session 1fb068a4 "Fix the uploader with zcode" from %s into a Claude Code '
                      'session in %s:' % (h.shown(self.root), h.shown(self.claude)), out)
        self.assertIn("conversation               1 of your messages, with the replies", out)
        self.assertIn("tool calls                 1, as text with their output shortened", out)
        self.assertIn("The original stays in " + h.shown(self.root), out)
        new = re.search(r"claude --resume (\S+)", out).group(1)
        path = os.path.join(self.claude, "projects", re.sub(r"[^A-Za-z0-9]", "-", self.api), new + ".jsonl")
        self.assertTrue(os.path.exists(path), path)
        with open(path, encoding="utf-8") as fh:
            text = fh.read()
        self.assertIn("[Imported from ZCode session " + Z, text)
        self.assertIn("ls src", text)
        self.assertNotIn("kept to itself", text)
        self.assertEqual(self.rows(), before)  # the database is never written

    def test_claude_code_converted_for_zcode(self):
        h = self.h
        before = self.rows()
        code, out = h.run("move", A[:8], "zcode", "--json")
        self.assertEqual(code, 0, out)
        d = json.loads(out)
        self.assertEqual((d["action"], d["agent"], d["target_agent"], d["new_session"], d["target"]),
                         ("convert", "claude", "zcode", None, self.root))
        self.assertTrue(d["file"].endswith(".md") and os.path.exists(d["file"]), d["file"])
        self.assertTrue(d["resume"].startswith("zcode --attach "), d["resume"])
        self.assertIn(" --mode plan -p ", d["resume"])
        self.assertIn("imported from another coding agent", d["resume"])
        with open(d["file"], encoding="utf-8") as fh:
            text = fh.read()
        self.assertTrue(text.startswith("# Conversation imported from Claude Code session " + A), text[:80])
        self.assertIn("## You", text)
        self.assertIn("A Claude Code session", text)
        self.assertIn("## Claude", text)
        self.assertEqual(self.rows(), before)
        code, out = h.run("move", A[:8], "zcode", "--dry-run")
        self.assertEqual(code, 0, out)
        self.assertIn('Would convert Claude Code session aaaaaaaa "A Claude Code session" from %s into a conversation for '
                      'ZCode in ' % h.shown(self.claude), out)
        self.assertIn("so the session starts when zcode reads the file.", out)
        self.assertIn("Then start it with:", out)
        self.assertIn("zcode --attach", out)
        self.assertIn("Once it has answered, carry on in the terminal UI with:\n  zcode -c", out)
        self.assertIn("<new id>.md", out)
        code, out = h.run("fork", A[:8], "zcode", "--json")  # a fork of one is the same conversion
        self.assertEqual(json.loads(out)["target_agent"], "zcode")

    def test_a_zcode_session_stays_where_it_is(self):
        code, out = self.h.run("move", Z[:8], "zcode")
        self.assertEqual(code, 1, out)
        self.assertIn("is already in %s, and in no other profile" % self.h.shown(self.root), out)
        code, out = self.h.run("move", A[:8], os.path.join(self.h.home, ".zcode-nope"))
        self.assertEqual(code, 1, out)
        self.assertIn("isn't a profile folder", out)  # a path nothing marks is Claude Code's


class ZcodeElsewhere(unittest.TestCase):
    def setUp(self):
        self.h = h = FakeHome()
        self.addCleanup(h.cleanup)
        self.claude = h.profile("default")
        self.root = h.zcode_root()

    def test_relocate_leaves_zcode_sessions_alone(self):
        h = self.h
        api, api2 = h.folder("code", "api"), h.folder("code", "api2")
        h.session(self.claude, api, A, "Old one", 70)
        h.zcode_session(self.root, api, Z, "In zcode", 70)
        code, out = h.run("relocate", api, api2)
        self.assertEqual(code, 0, out)
        self.assertIn("Relocated 1 session from", out)
        self.assertIn("Left alone: ZCode session 1fb068a4 in %s keeps the folder in zcode's database, which stays "
                      "as it was." % h.shown(self.root), out)
        api3, api4 = h.folder("code", "api3"), h.folder("code", "api4")
        h.zcode_session(self.root, api3, S, "Only zcode", 70)
        code, out = h.run("relocate", api3, api4)
        self.assertEqual(code, 1, out)
        self.assertIn("No sessions to move ran in %s (only ZCode's, which stay as they are)" % h.shown(api3), out)

    def test_bind_default_binds_zcode_too(self):
        h = self.h
        api = h.folder("code", "api")
        code, out = h.run("bind", "default", cwd=api)
        self.assertEqual(code, 0, out)
        self.assertIn("Claude Code %s · ZCode %s." % (h.shown(self.claude), h.shown(self.root)), out)
        code, out = h.run("bind", "--zcode", "default", cwd=api)
        self.assertEqual(code, 0, out)
        self.assertIn("ZCode " + h.shown(self.root), out)
        code, out = h.run("bind", "--zcode", "nope", cwd=api)
        self.assertEqual(code, 1, out)
        self.assertIn("isn't a ZCode data folder", out)

    def test_usage_says_why_there_is_none(self):
        code, out = self.h.run("usage")
        self.assertEqual(code, 0, out)
        self.assertIn("%s  ZCode · no data (zcode has no usage command; see z.ai/manage-apikey/subscription for the plan)"
                      % self.h.shown(self.root), out)
        code, out = self.h.run("usage", "--json")
        z = next(a for a in json.loads(out)["accounts"] if a["agent"] == "zcode")
        self.assertEqual((z["profiles"], z["plan"], z["source"], z["limits"]), ([self.root], "ZCode", "none", []))
        api = self.h.folder("code", "api")
        self.h.zcode_session(self.root, api, Z, "Fix the uploader with zcode", 1)
        code, out = self.h.run("sessions")
        self.assertIn("No usage data (zcode has no usage command; see z.ai/manage-apikey/subscription for the plan): "
                      + self.h.shown(self.root), out)

    def test_doctor_names_them(self):
        code, out = self.h.run("doctor")
        self.assertIn("Kimi Code CLI    not on your PATH", out)
        self.assertIn("ZCode            not on your PATH", out)


if __name__ == "__main__":
    unittest.main()
