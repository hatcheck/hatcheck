"""GitHub Copilot CLI (copilot) in hatcheck: homes, sessions as copilot 1.0.103 writes them, the
inuse file a running copilot keeps, conversion out, the interchange file written for its own
importer, and relocation skips. Every home is a throwaway fixture; no real copilot, credentials
or network."""
import json
import os
import re
import subprocess
import sys
import unittest
from unittest import mock

from helpers import NOW, WINDOWS, FakeHome, rec, write

C = "50b1f120-6353-4117-b918-fb4553e92eb4"
N = "d0000000-0000-4000-8000-000000000001"
E = "e0000000-0000-4000-8000-000000000001"
A = "aaaaaaaa-0000-4000-8000-000000000001"
R = "e0000000-0000-4000-8000-000000000002"
TITLE = "Fix the uploader with Copilot"
MODEL = "mai-code-1.1-flash"
NO_USAGE = "copilot's /usage runs only in a session, and hatcheck makes no request (1.0.103)"
OPEN = "GitHub Copilot CLI session %s is open in copilot; switch to it, or quit it first" % C[:8]
OPEN_MOVE = "GitHub Copilot CLI session %s is open in copilot; quit it first, so it stops changing" % C[:8]
LEFT_OUT = ("Left out: GitHub Copilot CLI's reasoning, what copilot adds to the conversation itself, its hooks' and "
            "its own records, and images and files.")
HIDDEN = ("INJECTED COPILOT CONTEXT", "COPILOT INTERNAL RECORD", "current_datetime")


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


def copilot_words(h, home):
    """What hatcheck prints before and after `copilot <tail>` to run copilot with a home: nothing
    for ~/.copilot, COPILOT_HOME= before it in sh, and in PowerShell's words the variable set
    before and cleared after."""
    default = os.path.join(h.home, ".copilot")
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
        return "$env:COPILOT_HOME=%s; " % folder, "; $env:COPILOT_HOME=$null"
    shown = h.shown(home) if os.path.commonpath([h.home, home]) == h.home else home
    return "COPILOT_HOME=%s " % shown, ""


def resume_command(h, home, session=None):
    before, after = copilot_words(h, home)
    return before + "copilot" + (" --resume " + session if session else "") + after


class CopilotSessions(unittest.TestCase):
    def setUp(self):
        self.h = h = FakeHome()
        self.addCleanup(h.cleanup)
        self.home = h.copilot_home()
        self.api = h.folder("code", "api")
        self.path = h.copilot_session(self.home, self.api, C, TITLE, 1)

    def sessions(self, *args, **env):
        code, out = self.h.run("sessions", "--json", "--no-usage", *args, **env)
        self.assertEqual(code, 0, out)
        return json.loads(out)["sessions"]

    def test_listing_and_json_metadata(self):
        code, out = self.h.run("sessions", "--no-usage")
        self.assertEqual(code, 0, out)
        self.assertIn("Copilot CLI sessions", out)
        self.assertIn(TITLE, out)
        self.assertIn("copilot --resume " + C, out)
        self.assertIn(self.h.shown(self.home) + " (1)", out)
        for text in HIDDEN:
            self.assertNotIn(text, out)
        listed = self.sessions()
        self.assertEqual([s["id"] for s in listed], [C])
        c = listed[0]
        self.assertEqual((c["agent"], c["profile"], c["title"], c["cwd"], c["entrypoint"], c["branch"]),
                         ("copilot", self.home, TITLE, self.api, "cli", "main"))
        self.assertEqual((c["resume"], c["model"], c["open"]), ("copilot --resume " + C, MODEL, None))

    def test_copilots_own_name_wins_over_the_first_prompt(self):
        self.h.copilot_session(self.home, self.api, N, "The first prompt", 2, name="Uploader fix")
        titles = {s["id"]: s["title"] for s in self.sessions()}
        self.assertEqual(titles[N], "Uploader fix")
        self.assertEqual(titles[C], TITLE)
        code, out = self.h.run("show", N[:8], "--json")
        self.assertEqual(code, 0, out)
        self.assertEqual(json.loads(out)["title"], "Uploader fix")

    def test_ids_where_show_open_and_resume(self):
        for query in (C, C[:8], C[:8].upper()):
            with self.subTest(query=query):
                code, out = self.h.run("where", query)
                self.assertEqual((code, out), (0, self.path + "\n"))
        code, out = self.h.run("where", C[:8], "--json")
        self.assertEqual(code, 0, out)
        found = json.loads(out)["sessions"][0]
        self.assertEqual((found["agent"], found["id"], found["profile"], found["file"], found["data_folder"],
                          found["archived"]), ("copilot", C, self.home, self.path, os.path.dirname(self.path), False))
        self.assertIn(json.dumps(self.path)[1:-1], out)

        code, out = self.h.run("show", C[:8])
        self.assertEqual(code, 0, out)
        self.assertIn('Session %s "%s" in %s, 4 turns' % (C[:8], TITLE, self.h.shown(self.home)), out)
        for text in (TITLE, "I'll inspect it.", "[external_agent_tool_call: view]", "path: src/main.rs",
                     "fn main() {}", "Done: " + TITLE, "!ls", "Cargo.toml", LEFT_OUT):
            self.assertIn(text, out)
        for text in HIDDEN:
            self.assertNotIn(text, out)
        code, out = self.h.run("show", C[:8], "--json")
        self.assertEqual(code, 0, out)
        shown = json.loads(out)
        self.assertEqual((shown["agent"], shown["id"], shown["title"], shown["cwd"], shown["model"],
                          shown["turns_total"], shown["tool_calls"]), ("copilot", C, TITLE, self.api, MODEL, 4, 1))
        text = "\n".join(turn["text"] for turn in shown["turns"])
        self.assertIn("[external_agent_tool_result]", text)
        self.assertNotIn("COPILOT INTERNAL RECORD", text)

        expected = ["cd " + self.h.shown(self.api), "copilot --resume " + C]
        code, out = self.h.run("open", C[:8], "--print")
        self.assertEqual((code, out.splitlines()), (0, expected))
        code, out = self.h.run("resume", C[:8], "--print")
        self.assertEqual((code, out.splitlines()), (0, expected))

    def test_a_session_copilot_never_spoke_in_is_not_one(self):
        path = self.h.copilot_session(self.home, self.api, E, "Never spoken in", 2, events=False)
        self.assertFalse(os.path.exists(path))
        self.assertEqual({s["id"] for s in self.sessions()}, {C})
        code, out = self.h.run("where", E[:8])
        self.assertEqual((code, out), (1, ""))

    def test_other_clients_wait_for_all(self):
        self.h.copilot_session(self.home, self.api, sid(33), "From the Copilot app", client="github/copilot-app")
        self.assertEqual({s["id"] for s in self.sessions()}, {C})
        listed = {s["id"]: s["entrypoint"] for s in self.sessions("--all")}
        self.assertEqual(listed, {C: "cli", sid(33): "github/copilot-app"})
        code, out = self.h.run("sessions", "--no-usage")
        self.assertEqual(code, 0, out)
        self.assertNotIn("From the Copilot app", out)

    def test_open_while_an_inuse_file_names_a_running_process(self):
        self.assertIsNone(self.sessions()[0]["open"])
        self.h.copilot_inuse(self.home, C, dead_pid())
        self.assertIsNone(self.sessions()[0]["open"])  # a crash's leftover
        self.h.copilot_inuse(self.home, C, os.getpid())
        self.assertEqual(self.sessions()[0]["open"], {"pid": None, "status": None, "in": "copilot"})
        code, out = self.h.run("open", C[:8], HATCHECK_TERMINAL="false")
        self.assertEqual(code, 1, out)
        self.assertIn(OPEN, out)
        code, out = self.h.run("open", C[:8], "--print")  # prints the commands without starting them
        self.assertEqual(code, 0, out)

    def test_windows_style_cwd_and_json_escaping(self):
        cwd = 'C:\\work\\copilot "quoted"\\api'
        session = sid(48)
        self.h.copilot_session(self.home, cwd, session, 'Windows \\ paths and "quotes"')
        code, out = self.h.run("show", session[:8], "--json")
        self.assertEqual(code, 0, out)
        shown = json.loads(out)
        self.assertEqual((shown["cwd"], shown["title"]), (cwd, 'Windows \\ paths and "quotes"'))
        self.assertIn(json.dumps(cwd)[1:-1], out)
        code, out = self.h.run("where", session[:8], "--json")
        self.assertEqual(code, 0, out)
        found = json.loads(out)["sessions"][0]
        self.assertTrue(found["file"].endswith(os.sep + session + os.sep + "events.jsonl"), found["file"])
        self.assertIn(json.dumps(found["file"])[1:-1], out)


class CopilotBadFiles(unittest.TestCase):
    def setUp(self):
        self.h = h = FakeHome()
        self.addCleanup(h.cleanup)
        self.home = h.copilot_home()
        self.api = h.folder("code", "api")

    def ids(self):
        code, out = self.h.run("sessions", "--json", "--no-usage")
        self.assertEqual(code, 0, out)
        return {s["id"] for s in json.loads(out)["sessions"]}

    def test_only_session_folders_count_and_a_torn_tail_is_not_read(self):
        self.h.copilot_session(self.home, self.api, C, TITLE)
        locks = os.path.join(self.home, "session-state", ".session-operation-locks")
        write(os.path.join(locks, C + ".lock"), "")
        write(os.path.join(self.home, "session-state", "notes", "events.jsonl"),
              rec(type="user.message", data={"content": "NOT A SESSION"}) + "\n")
        self.assertEqual(self.ids(), {C})
        code, out = self.h.run("where", "notes")
        self.assertEqual((code, out), (1, ""))

        torn = sid(115)
        path = self.h.copilot_session(self.home, self.api, torn, "A torn Copilot tail")
        with open(path, "a", encoding="utf-8") as fh:
            fh.write(json.dumps({"type": "assistant.message", "data": {"content": "TORN TAIL MUST NOT APPEAR"},
                                 "id": "x", "timestamp": "2026-10-25T20:00:00.000Z"})[:-12])
        code, out = self.h.run("show", torn[:8], "--json")
        self.assertEqual(code, 0, out)
        self.assertEqual(json.loads(out)["turns_total"], 4)
        self.assertNotIn("TORN TAIL", out)

    def test_a_folder_with_no_messages_is_listed_only_while_open(self):
        path = self.h.copilot_session(self.home, self.api, E, "Emptied")
        write(path, rec(type="session.start", data={"sessionId": E, "context": {"cwd": self.api}},
                        id="s", timestamp="2026-10-25T20:00:00.000Z", parentId=None) + "\n")
        self.assertEqual(self.ids(), set())
        self.h.copilot_inuse(self.home, E, os.getpid())
        self.assertEqual(self.ids(), {E})


class CopilotDiscovery(unittest.TestCase):
    def setUp(self):
        self.h = h = FakeHome()
        self.addCleanup(h.cleanup)
        self.api = h.folder("code", "api")
        self.home = h.copilot_home()
        h.copilot_session(self.home, self.api, C, "Default Copilot home")

    def ids(self, **env):
        code, out = self.h.run("sessions", "--json", "--no-usage", **env)
        self.assertEqual(code, 0, out)
        return {s["id"]: s for s in json.loads(out)["sessions"] if s["agent"] == "copilot"}

    def test_default_named_and_custom_homes(self):
        work = self.h.copilot_home("work")
        self.h.copilot_session(work, self.api, N, "Named Copilot home")
        self.h.folder(".copilot-empty")  # never run: no config.json, no session-state
        listed = self.ids()
        self.assertEqual(set(listed), {C, N})
        self.assertEqual((listed[C]["profile"], listed[C]["resume"]), (self.home, "copilot --resume " + C))
        self.assertEqual((listed[N]["profile"], listed[N]["resume"]), (work, resume_command(self.h, work, N)))
        code, out = self.h.run("open", N[:8], "--print")
        self.assertEqual((code, out.splitlines()), (0, ["cd " + self.h.shown(self.api), resume_command(self.h, work, N)]))
        before = tree_bytes(work)
        code, out = self.h.run("open", "--login", "copilot:work", "--print")
        self.assertEqual(code, 0, out)
        self.assertEqual(out.splitlines()[-1], resume_command(self.h, work) + " login")
        self.assertEqual(tree_bytes(work), before)

        custom = self.h.copilot_home(path=os.path.join(self.h.root, "elsewhere", "copilot-custom"))
        self.h.copilot_session(custom, self.api, E, "COPILOT_HOME Copilot home")
        with mock.patch.dict(os.environ, {"COPILOT_HOME": custom}):
            self.assertEqual(set(self.ids()), {C, N})  # the tests' environment is scrubbed
        listed = self.ids(COPILOT_HOME=custom)
        self.assertEqual(set(listed), {C, N, E})
        self.assertEqual((listed[E]["profile"], listed[E]["resume"]), (custom, resume_command(self.h, custom, E)))
        empty = self.h.folder("not-a-copilot-home")
        self.assertEqual(set(self.ids(COPILOT_HOME=empty)), {C, N})

    def test_a_claude_profile_is_not_a_copilot_home(self):
        profile = self.h.profile("default")
        self.h.session(profile, self.api, A, "A Claude Code session")
        code, out = self.h.run("sessions", "--json", "--no-usage")
        self.assertEqual(code, 0, out)
        agents = {s["id"]: s["agent"] for s in json.loads(out)["sessions"]}
        self.assertEqual(agents, {C: "copilot", A: "claude"})
        code, out = self.h.run("where", A[:8], "--json")
        self.assertEqual(code, 0, out)
        self.assertEqual([s["agent"] for s in json.loads(out)["sessions"]], ["claude"])


class CopilotUsageAndBindings(unittest.TestCase):
    def setUp(self):
        self.h = h = FakeHome()
        self.addCleanup(h.cleanup)
        self.home = h.copilot_home()
        self.work = h.copilot_home("work")
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
        self.assertIn("%s  Copilot · no data (%s)" % (self.h.shown(self.home), NO_USAGE), out)
        code, out = self.h.run("usage", "--json")
        self.assertEqual(code, 0, out)
        accounts = [a for a in json.loads(out)["accounts"] if a["agent"] == "copilot"]
        self.assertEqual({a["profiles"][0] for a in accounts}, {self.home, self.work})
        for account in accounts:
            self.assertEqual((account["plan"], account["limits"], account["source"], account["why"]),
                             ("Copilot", [], "none", NO_USAGE))

    def test_bind_default_named_which_and_preservation(self):
        code, out = self.h.run("bind", "default", cwd=self.api)
        self.assertEqual(code, 0, out)
        self.assertIn("GitHub Copilot CLI " + self.h.shown(self.home), out)
        self.assertEqual(self.saved(), {"claude": "default", "codex": "default", "copilot": "default"})
        code, out = self.h.run("bind", "--copilot", "work", cwd=self.api)
        self.assertEqual(code, 0, out)
        named = self.saved()
        self.assertEqual(named, {"claude": "default", "codex": "default", "copilot": "work"})
        code, out = self.h.run("bind", "--which", "copilot", cwd=self.api)
        self.assertEqual((code, out), (0, "work\n"))
        code, out = self.h.run("bind", "--codex", "default", cwd=self.api)
        self.assertEqual(code, 0, out)
        self.assertEqual(self.saved(), named)
        code, out = self.h.run("bind", "--list")
        self.assertEqual(code, 0, out)
        self.assertIn("GitHub Copilot CLI " + self.h.shown(self.work), out)
        self.h.profile("work")
        self.h.codex_home("work")
        other = self.h.folder("code", "other")
        code, out = self.h.run("bind", "work", cwd=other)
        self.assertEqual(code, 0, out)
        self.assertEqual(self.saved(other)["copilot"], "work")
        self.h.folder(".copilot-never")
        code, out = self.h.run("bind", "--copilot", "never", cwd=other)
        self.assertEqual(code, 1, out)
        self.assertIn("isn't a GitHub Copilot CLI home", out)
        self.assertIn("COPILOT_HOME=", out)
        self.assertEqual(self.saved(other)["copilot"], "work")


class CopilotConversion(unittest.TestCase):
    def setUp(self):
        self.h = h = FakeHome()
        self.addCleanup(h.cleanup)
        self.home = h.copilot_home()
        self.claude = h.profile("default")
        self.api = h.folder("code", "api")
        self.path = h.copilot_session(self.home, self.api, C, TITLE)

    def claude_transcript(self, session):
        project = re.sub(r"[^A-Za-z0-9]", "-", self.api)
        return os.path.join(self.claude, "projects", project, session + ".jsonl")

    def test_convert_out_dry_run_move_and_fork(self):
        original = tree_bytes(self.home)
        before = tree_bytes(self.claude)
        code, out = self.h.run("move", C[:8], "default", "--dry-run")
        self.assertEqual(code, 0, out)
        self.assertIn('Would convert GitHub Copilot CLI session %s "%s"' % (C[:8], TITLE), out)
        self.assertEqual(tree_bytes(self.home), original)
        self.assertEqual(tree_bytes(self.claude), before)

        code, out = self.h.run("move", C[:8], "default", "--json")
        self.assertEqual(code, 0, out)
        moved = json.loads(out)
        self.assertEqual((moved["action"], moved["agent"], moved["session"], moved["target_agent"], moved["target"],
                          moved["prompts"], moved["tool_calls"]), ("convert", "copilot", C, "claude", self.claude, 2, 1))
        new = moved["new_session"]
        self.assertTrue(new)
        path = self.claude_transcript(new)
        self.assertTrue(os.path.isfile(path), path)
        text = read_bytes(path).decode("utf-8")
        for expected in ("[Imported from GitHub Copilot CLI session " + C, TITLE, "view", "fn main() {}"):
            self.assertIn(expected, text)
        for excluded in HIDDEN:
            self.assertNotIn(excluded, text)
        self.assertEqual(tree_bytes(self.home), original)

        code, out = self.h.run("fork", C[:8], "default", "--json")
        self.assertEqual(code, 0, out)
        forked = json.loads(out)
        self.assertEqual((forked["action"], forked["agent"], forked["target_agent"]), ("convert", "copilot", "claude"))
        self.assertNotIn(forked["new_session"], (None, C, new))
        self.assertTrue(os.path.isfile(self.claude_transcript(forked["new_session"])))
        self.assertEqual(tree_bytes(self.home), original)

    def test_an_open_session_is_refused_without_writes(self):
        self.h.copilot_inuse(self.home, C, os.getpid())
        original = tree_bytes(self.home)
        before = tree_bytes(self.claude)
        for command in ("move", "fork"):
            for extra in (("--dry-run",), ()):
                with self.subTest(command=command, extra=extra):
                    code, out = self.h.run(command, C[:8], "default", *extra)
                    self.assertEqual(code, 1, out)
                    self.assertIn(OPEN_MOVE, out)
                    self.assertEqual(tree_bytes(self.home), original)
                    self.assertEqual(tree_bytes(self.claude), before)

    def test_copies_between_copilot_homes_are_refused_without_changes(self):
        work = self.h.copilot_home("work")
        before, target_before = tree_bytes(self.home), tree_bytes(work)
        for command in ("move", "fork"):
            with self.subTest(command=command):
                code, out = self.h.run(command, C[:8], "copilot:work")
                self.assertEqual(code, 1, out)
                self.assertIn("aren't copied between its homes; convert session %s for another agent instead" % C[:8], out)
                self.assertEqual(tree_bytes(self.home), before)
                self.assertEqual(tree_bytes(work), target_before)

    def test_a_claude_code_session_converted_for_copilot_is_a_file_its_importer_takes(self):
        source = self.h.session(self.claude, self.api, A, "A Claude Code session")
        source_bytes = read_bytes(source)
        work = self.h.copilot_home("work")
        before, work_before = tree_bytes(self.home), tree_bytes(work)
        code, out = self.h.run("move", A[:8], "copilot", "--json")
        self.assertEqual(code, 0, out)
        d = json.loads(out)
        self.assertEqual((d["action"], d["agent"], d["target_agent"], d["new_session"], d["target"], d["brief"]),
                         ("convert", "claude", "copilot", None, self.home, False))
        self.assertTrue(d["file"].endswith(".jsonl") and os.path.exists(d["file"]), d["file"])
        self.assertTrue(d["resume"].startswith("copilot sessions import "), d["resume"])
        self.assertTrue(d["resume"].endswith(os.path.basename(d["file"])), d["resume"])
        with open(d["file"], encoding="utf-8") as fh:
            lines = [json.loads(line) for line in fh]
        # The interchange form copilot's importer validates: a session header, then messages.
        header = lines[0]
        self.assertEqual((header["type"], header["version"], header["workspace"]["workingDirectory"],
                          header["workspace"]["branch"], header["source"]["application"]),
                         ("session", 1, self.api, "main", "hatcheck"))
        self.assertEqual(header["title"], "A Claude Code session")
        self.assertRegex(header["createdAt"], r"^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d\.\d{3}Z$")
        self.assertEqual([m["role"] for m in lines[1:]], ["user", "assistant"])
        self.assertIn("[Imported from Claude Code session " + A, lines[1]["content"][0]["text"])
        self.assertIn("A Claude Code session", lines[1]["content"][0]["text"])
        self.assertEqual(lines[2]["content"], [{"type": "text", "text": "ok"}])
        self.assertEqual(read_bytes(source), source_bytes)  # the original stays
        self.assertEqual(tree_bytes(self.home), before)  # and copilot's store isn't written
        code, out = self.h.run("fork", A[:8], "copilot:work", "--dry-run")
        self.assertEqual(code, 0, out)
        self.assertIn('Would convert Claude Code session %s "A Claude Code session" from %s into a session file for '
                      'GitHub Copilot CLI in ' % (A[:8], self.h.shown(self.claude)), out)
        self.assertIn("so the session exists once that has run", out)
        self.assertIn("Then import it with:", out)
        before_w, after_w = copilot_words(self.h, work)
        line = next(l for l in out.splitlines() if "copilot sessions import " in l)
        self.assertTrue(line.startswith("  %scopilot sessions import " % before_w) and line.endswith(after_w), line)
        self.assertIn("<new id>.jsonl", line)
        self.assertIn("which prints the ID of the session it makes; then resume it with:", out)
        self.assertIn("  %scopilot --resume <id>%s" % (before_w, after_w), out)
        self.assertEqual(tree_bytes(work), work_before)


class CopilotRelocate(unittest.TestCase):
    def test_skip_and_dry_run_preserve_every_copilot_byte(self):
        h = FakeHome()
        self.addCleanup(h.cleanup)
        home, claude = h.copilot_home(), h.profile("default")
        old = h.folder("code", "api")
        new = os.path.join(h.home, "code", "api2")
        events = h.copilot_session(home, old, R, "A copilot session left in place", 70)
        transcript = h.session(claude, old, A, "A Claude session that moves", 70)
        os.rename(old, new)
        before = tree_bytes(home)
        skipped = ("Left alone: GitHub Copilot CLI session %s in %s records the folder in its workspace file and its "
                   "events, which stay as they were." % (R[:8], h.shown(home)))
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
        self.assertTrue(os.path.isfile(events))
        moved = os.path.join(claude, "projects", re.sub(r"[^A-Za-z0-9]", "-", new), A + ".jsonl")
        self.assertTrue(os.path.isfile(moved), moved)


if __name__ == "__main__":
    unittest.main()
