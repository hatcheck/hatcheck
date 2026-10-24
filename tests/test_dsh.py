"""DeepSeek Harness (dsh) in hatcheck: v4 raw and compressed logs, discovery, display,
conversion out, read-only targets and relocation. Against a throwaway home: no real dsh,
zstd executable, credentials or network."""
import json
import os
import re
import unittest
from unittest import mock

from helpers import DSH_COMPRESSED_ID, WINDOWS, DshOpen, FakeHome, write

D = "d54e0001-0000-4000-8000-000000000001"
N = "d54e0002-0000-4000-8000-000000000002"
S = "d54e0003-0000-4000-8000-000000000003"
E = "d54e0004-0000-4000-8000-000000000004"
F = "d54e0005-0000-4000-8000-000000000005"
X = "d54e0006-0000-4000-8000-000000000006"
M = "d54e0007-0000-4000-8000-000000000007"
R = "d54e0008-0000-4000-8000-000000000008"
A = "aaaaaaaa-0000-4000-8000-000000000001"
TITLE = "Fix the uploader with dsh"
NO_USAGE = "dsh has no plan-usage command or endpoint (0.2.1-rc.2)"
READ_ONLY = "DeepSeek Harness's session store isn't one hatcheck can write; choose another agent as the target"


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


def custom_command(path):
    if WINDOWS:
        return "$env:DSH_HOME='%s'; dsh web; $env:DSH_HOME=$null" % path.replace("'", "''")
    return "DSH_HOME=%s dsh web" % path


class DshSessions(unittest.TestCase):
    def setUp(self):
        self.h = h = FakeHome()
        self.addCleanup(h.cleanup)
        self.home = h.dsh_home()
        self.api = h.folder("code", "api")
        self.log = h.dsh_session(self.home, self.api, D, TITLE, 1, model="deepseek-chat",
                                 prompt="Please fix the uploader with dsh")
        self.folder = os.path.dirname(self.log)

    def sessions(self, *args, **env):
        code, out = self.h.run("sessions", "--json", "--no-usage", *args, **env)
        self.assertEqual(code, 0, out)
        return json.loads(out)["sessions"]

    def test_raw_listing_and_json(self):
        code, out = self.h.run("sessions", "--no-usage")
        self.assertEqual(code, 0, out)
        self.assertIn("DeepSeek Harness CLI sessions", out)
        self.assertIn(TITLE + "\n", out)
        self.assertIn("dsh web", out)
        self.assertIn(self.h.shown(self.home) + " (1)", out)
        self.assertNotIn("Draft:", out)
        listed = self.sessions()
        self.assertEqual([s["id"] for s in listed], [D])
        d = listed[0]
        self.assertEqual((d["agent"], d["profile"], d["title"], d["cwd"], d["entrypoint"]),
                         ("dsh", self.home, TITLE, self.api, "cli"))
        self.assertEqual((d["resume"], d["model"], d["branch"], d["open"]),
                         ("dsh web", "deepseek-chat", None, None))

    def test_compressed_listing_json_and_show(self):
        log = self.h.dsh_compressed_session(self.home)
        code, out = self.h.run("sessions", "--no-usage")
        self.assertEqual(code, 0, out)
        self.assertIn("Compressed DSH session\n", out)
        c = next(s for s in self.sessions() if s["id"] == DSH_COMPRESSED_ID)
        self.assertEqual((c["agent"], c["title"], c["model"], c["cwd"], c["resume"]),
                         ("dsh", "Compressed DSH session", "deepseek-compressed", "/work/dsh-compressed", "dsh web"))
        code, out = self.h.run("show", DSH_COMPRESSED_ID[:8], "--json")
        self.assertEqual(code, 0, out)
        shown = json.loads(out)
        self.assertIn("Decoded the second frame", shown["turns"][-1]["text"])
        self.assertEqual((shown["title"], shown["model"]), ("Compressed DSH session", "deepseek-compressed"))
        code, out = self.h.run("where", "session-" + DSH_COMPRESSED_ID[:8], "--json")
        self.assertEqual(code, 0, out)
        found = json.loads(out)["sessions"][0]
        self.assertEqual((found["file"], found["data_folder"]), (log, os.path.dirname(log)))

    def test_where_show_open_and_resume(self):
        for query in (D, D[:8], "session-" + D, "session-" + D[:8]):
            with self.subTest(query=query):
                code, out = self.h.run("where", query)
                self.assertEqual((code, out), (0, self.folder + "\n"))
        code, out = self.h.run("where", "session-" + D[:8], "--json")
        self.assertEqual(code, 0, out)
        found = json.loads(out)["sessions"][0]
        self.assertEqual((found["agent"], found["id"], found["profile"], found["file"], found["data_folder"],
                          found["archived"]), ("dsh", D, self.home, self.log, self.folder, False))

        code, out = self.h.run("show", D[:8])
        self.assertEqual(code, 0, out)
        self.assertIn('Session %s "%s" in %s, 2 turns' % (D[:8], TITLE, self.h.shown(self.home)), out)
        for text in ("Please fix the uploader with dsh", "[external_agent_tool_call: read_file]", "path: src/main.rs",
                     "fn main() {}", "Done: " + TITLE):
            self.assertIn(text, out)
        for text in ("INJECTED RUNTIME CONTEXT", "kept to itself", "Draft:", "deepseek-old"):
            self.assertNotIn(text, out)
        self.assertIn("Left out: DeepSeek Harness's reasoning, images, files, injected context, replaced history and "
                      "subagent sessions.", out)
        code, out = self.h.run("show", D[:8], "--json")
        self.assertEqual(code, 0, out)
        shown = json.loads(out)
        self.assertEqual((shown["agent"], shown["id"], shown["title"], shown["cwd"], shown["model"],
                          shown["tool_calls"]), ("dsh", D, TITLE, self.api, "deepseek-chat", 1))
        text = "\n".join(turn["text"] for turn in shown["turns"])
        self.assertIn("[external_agent_tool_result]", text)
        self.assertIn("fn main() {}", text)
        self.assertNotIn("INJECTED RUNTIME CONTEXT", text)
        self.assertNotIn("kept to itself", text)

        expected = ["cd " + self.h.shown(self.api), "dsh web"]
        code, out = self.h.run("open", D[:8], "--print")
        self.assertEqual((code, out.splitlines()), (0, expected))
        code, out = self.h.run("resume", D[:8], "--print")
        self.assertEqual((code, out.splitlines()), (0, expected))

    def test_dsh_home_is_scrubbed_then_discovered_and_used(self):
        custom = self.h.dsh_home(path=os.path.join(self.h.root, "elsewhere", "dsh-custom"))
        self.h.dsh_session(custom, self.api, N, "In a custom DSH home", 2)
        with mock.patch.dict(os.environ, {"DSH_HOME": custom}):
            self.assertNotIn(N, [s["id"] for s in self.sessions()])
        command = custom_command(custom)
        n = next(s for s in self.sessions(DSH_HOME=custom) if s["id"] == N)
        self.assertEqual((n["profile"], n["resume"]), (custom, command))
        code, out = self.h.run("open", N[:8], "--print", DSH_HOME=custom)
        self.assertEqual((code, out.splitlines()), (0, ["cd " + self.h.shown(self.api), command]))

    def test_compact_checkpoint_replaces_direct_and_injected_messages(self):
        self.h.dsh_session(self.home, self.api, E, "Compacted DSH history", compact=True,
                           prompt="Original prompt replaced by compaction")
        code, out = self.h.run("show", E[:8], "--json")
        self.assertEqual(code, 0, out)
        text = "\n".join(turn["text"] for turn in json.loads(out)["turns"])
        self.assertIn("Compacted checkpoint: Compacted DSH history", text)
        self.assertNotIn("Original prompt replaced by compaction", text)
        self.assertNotIn("INJECTED RUNTIME CONTEXT", text)
        self.assertIn("Done: Compacted DSH history", text)

    def test_subagent_is_not_listed(self):
        self.h.dsh_session(self.home, self.api, S, "A dsh subagent", origin="subagent")
        code, out = self.h.run("sessions", "--no-usage")
        self.assertEqual(code, 0, out)
        self.assertNotIn("A dsh subagent", out)
        self.assertEqual([s["id"] for s in self.sessions()], [D])
        code, out = self.h.run("show", S[:8], "--json")
        self.assertEqual(code, 0, out)
        self.assertEqual(json.loads(out)["id"], S)

    def test_future_newest_generation_does_not_fall_back(self):
        log = self.h.dsh_session(self.home, self.api, F, "Future DSH generation")
        with open(log, encoding="utf-8") as fh:
            v4 = fh.read()
        future = os.path.join(os.path.dirname(log), "session.v5.jsonl")
        write(future, v4.replace('"version":4', '"version":5', 1))
        self.assertEqual([s["id"] for s in self.sessions()], [D])
        code, out = self.h.run("where", F)
        self.assertEqual((code, out), (1, ""))

    def test_corrupt_and_mixed_latest_generations_are_not_read(self):
        corrupt = self.h.dsh_session(self.home, self.api, X, "Corrupt DSH log")
        write(corrupt, "{not json\n")
        mixed = self.h.dsh_session(self.home, self.api, M, "Ambiguous DSH log")
        write(os.path.join(os.path.dirname(mixed), "session.v4.jsonl.zstd"), "not a zstd frame\n")
        self.assertEqual([s["id"] for s in self.sessions()], [D])
        for sid in (X, M):
            code, out = self.h.run("where", sid)
            self.assertEqual((code, out), (1, ""))


class DshUsageAndBindings(unittest.TestCase):
    def setUp(self):
        self.h = h = FakeHome()
        self.addCleanup(h.cleanup)
        self.home = h.dsh_home()
        self.work = h.dsh_home("work")
        self.claude = h.profile("default")
        self.codex = h.codex_home("default")
        self.api = h.folder("code", "api")

    def saved(self):
        base = os.path.join(self.h.home, "AppData", "Roaming") if WINDOWS else os.path.join(self.h.home, ".config")
        with open(os.path.join(base, "hatcheck", "bindings.json"), encoding="utf-8") as fh:
            return json.load(fh)["folders"][os.path.realpath(self.api)]

    def test_usage_says_why_there_is_none(self):
        code, out = self.h.run("usage")
        self.assertEqual(code, 0, out)
        self.assertIn("%s  DeepSeek · no data (%s)" % (self.h.shown(self.home), NO_USAGE), out)
        code, out = self.h.run("usage", "--json")
        self.assertEqual(code, 0, out)
        accounts = [a for a in json.loads(out)["accounts"] if a["agent"] == "dsh"]
        self.assertEqual({a["profiles"][0] for a in accounts}, {self.home, self.work})
        for account in accounts:
            self.assertEqual(len(account["profiles"]), 1)
            self.assertEqual((account["plan"], account["limits"], account["source"], account["why"]),
                             ("DeepSeek", [], "none", NO_USAGE))

    def test_bind_named_default_list_which_and_preserve(self):
        code, out = self.h.run("bind", "default", cwd=self.api)
        self.assertEqual(code, 0, out)
        self.assertIn("DeepSeek Harness " + self.h.shown(self.home), out)
        self.assertEqual(self.saved(), {"claude": "default", "codex": "default", "dsh": "default"})
        code, out = self.h.run("bind", "--dsh", "work", cwd=self.api)
        self.assertEqual(code, 0, out)
        named = self.saved()
        code, out = self.h.run("bind", "--which", "dsh", cwd=self.api)
        self.assertEqual((code, out), (0, "work\n"))
        code, out = self.h.run("bind", cwd=self.api)
        self.assertIn("DeepSeek Harness " + self.h.shown(self.work), out)
        code, out = self.h.run("bind", "--list")
        self.assertEqual(code, 0, out)
        self.assertIn(self.h.shown(self.api), out)
        self.assertIn("DeepSeek Harness " + self.h.shown(self.work), out)
        code, out = self.h.run("bind", "--dsh", "default", cwd=self.api)
        self.assertEqual(code, 0, out)
        code, out = self.h.run("bind", "--which", "dsh", cwd=self.api)
        self.assertEqual((code, out), (0, "default\n"))
        self.assertEqual(named, {"claude": "default", "codex": "default", "dsh": "work"})
        self.assertEqual(self.saved(), {"claude": "default", "codex": "default", "dsh": "default"})


class DshConversion(unittest.TestCase):
    def setUp(self):
        self.h = h = FakeHome()
        self.addCleanup(h.cleanup)
        self.home = h.dsh_home()
        self.claude = h.profile("default")
        self.api = h.folder("code", "api")
        self.log = h.dsh_session(self.home, self.api, D, TITLE)

    def claude_transcript(self, sid):
        project = re.sub(r"[^A-Za-z0-9]", "-", self.api)
        return os.path.join(self.claude, "projects", project, sid + ".jsonl")

    def test_convert_out_dry_run_move_and_fork_with_a_stale_lock(self):
        write(os.path.join(os.path.dirname(self.log), "session.lock"), "")
        original = read_bytes(self.log)
        before = tree_bytes(self.claude)
        code, out = self.h.run("move", D[:8], "default", "--dry-run")
        self.assertEqual(code, 0, out)
        self.assertIn('Would convert DeepSeek Harness session %s "%s"' % (D[:8], TITLE), out)
        self.assertEqual(tree_bytes(self.claude), before)
        self.assertEqual(read_bytes(self.log), original)

        code, out = self.h.run("move", D[:8], "default", "--json")
        self.assertEqual(code, 0, out)
        moved = json.loads(out)
        self.assertEqual((moved["action"], moved["agent"], moved["session"], moved["target_agent"], moved["target"],
                          moved["prompts"], moved["tool_calls"]), ("convert", "dsh", D, "claude", self.claude, 1, 1))
        new = moved["new_session"]
        self.assertTrue(new)
        path = self.claude_transcript(new)
        self.assertTrue(os.path.isfile(path), path)
        text = read_bytes(path).decode("utf-8")
        for expected in ("[Imported from DeepSeek Harness session " + D, TITLE, "read_file", "fn main() {}"):
            self.assertIn(expected, text)
        self.assertNotIn("INJECTED RUNTIME CONTEXT", text)
        self.assertNotIn("kept to itself", text)
        self.assertEqual(read_bytes(self.log), original)

        code, out = self.h.run("fork", D[:8], "default", "--json")
        self.assertEqual(code, 0, out)
        forked = json.loads(out)
        self.assertEqual((forked["action"], forked["agent"], forked["target_agent"]), ("convert", "dsh", "claude"))
        self.assertNotIn(forked["new_session"], (None, D, new))
        self.assertTrue(os.path.isfile(self.claude_transcript(forked["new_session"])))
        self.assertEqual(read_bytes(self.log), original)

    @unittest.skipIf(WINDOWS, "DSH's Windows named semaphore isn't detected")
    def test_held_lock_refuses_conversion_without_writes(self):
        before = tree_bytes(self.claude)
        with DshOpen(self.log):
            original = tree_bytes(self.home)
            for command in ("move", "fork"):
                code, out = self.h.run(command, D[:8], "default", "--dry-run")
                self.assertEqual(code, 1, out)
                self.assertIn("DeepSeek Harness session %s is open in dsh; close it first, so it stops changing" % D[:8], out)
                self.assertEqual(tree_bytes(self.home), original)
                self.assertEqual(tree_bytes(self.claude), before)

    def test_dsh_targets_and_same_agent_copies_are_refused_without_changes(self):
        source = self.h.session(self.claude, self.api, A, "A Claude Code session")
        source_bytes = read_bytes(source)
        work = self.h.dsh_home("work")
        before, target_before = tree_bytes(self.home), tree_bytes(work)
        for command in ("move", "fork"):
            for sid in (A, D):
                with self.subTest(command=command, source=sid):
                    code, out = self.h.run(command, sid[:8], "dsh:work")
                    self.assertEqual(code, 1, out)
                    self.assertIn(READ_ONLY, out)
                    self.assertEqual(tree_bytes(self.home), before)
                    self.assertEqual(tree_bytes(work), target_before)
                    self.assertEqual(read_bytes(source), source_bytes)


class DshRelocate(unittest.TestCase):
    def test_skip_and_dry_run_preserve_every_dsh_byte(self):
        h = FakeHome()
        self.addCleanup(h.cleanup)
        home, claude = h.dsh_home(), h.profile("default")
        old = h.folder("code", "api")
        new = os.path.join(h.home, "code", "api2")
        log = h.dsh_session(home, old, R, "A dsh session left in place", 70)
        transcript = h.session(claude, old, A, "A Claude session that moves", 70)
        os.rename(old, new)
        before = tree_bytes(home)
        skipped = ("Left alone: DeepSeek Harness session %s in %s keeps an immutable folder in its session header and "
                   "workspace state, which stay as they were." % (R[:8], h.shown(home)))
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
        self.assertTrue(os.path.isfile(log))
        moved = os.path.join(claude, "projects", re.sub(r"[^A-Za-z0-9]", "-", new), A + ".jsonl")
        self.assertTrue(os.path.isfile(moved), moved)


if __name__ == "__main__":
    unittest.main()
