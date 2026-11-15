"""Pinned project workspaces and explicitly started fresh sessions, in a throwaway home."""
import json
import os
import unittest

from helpers import WINDOWS, FakeHome, write


class Workspaces(unittest.TestCase):
    def setUp(self):
        self.h = h = FakeHome()
        self.addCleanup(h.cleanup)
        self.api = h.folder("code", "api")
        self.work = h.profile("work")
        self.other = h.profile("other")

    def test_pin_list_rename_and_remove(self):
        code, out = self.h.run("workspaces", "add", self.api, "--name", "API")
        self.assertEqual(code, 0, out)
        self.assertIn('Pinned workspace "API"', out)
        code, out = self.h.run("bind", "work", self.api)
        self.assertEqual(code, 0, out)
        code, out = self.h.run("workspaces", "--json")
        self.assertEqual(code, 0, out)
        data = json.loads(out)
        self.assertEqual(data["version"], 1)
        self.assertEqual(len(data["workspaces"]), 1)
        workspace = data["workspaces"][0]
        self.assertEqual(workspace["name"], "API")
        self.assertEqual(workspace["folder"], os.path.realpath(self.api))
        self.assertTrue(workspace["exists"])
        self.assertEqual(workspace["bindings"]["claude"], self.work)

        code, out = self.h.run("workspaces", "add", self.api, "--name", "Backend")
        self.assertEqual(code, 0, out)
        code, out = self.h.run("workspaces", "--json")
        workspaces = json.loads(out)["workspaces"]
        self.assertEqual(len(workspaces), 1)
        self.assertEqual(workspaces[0]["name"], "Backend")

        code, out = self.h.run("workspaces", "remove", self.api)
        self.assertEqual(code, 0, out)
        self.assertIn("Its folder is still there", out)
        self.assertTrue(os.path.isdir(self.api))
        code, out = self.h.run("workspaces", "--json")
        self.assertEqual(json.loads(out)["workspaces"], [])

    def test_a_missing_folder_is_unknown_not_deleted(self):
        code, out = self.h.run("workspaces", "add", self.api, "--name", "API")
        self.assertEqual(code, 0, out)
        os.rmdir(self.api)
        code, out = self.h.run("workspaces", "--json")
        self.assertEqual(code, 0, out)
        workspace = json.loads(out)["workspaces"][0]
        self.assertFalse(workspace["exists"])
        code, out = self.h.run("workspaces", "remove", self.api)
        self.assertEqual(code, 0, out)

    def test_a_broken_store_is_an_error_not_an_empty_list(self):
        config = self.h.config("")
        write(os.path.join(os.path.dirname(config), "workspaces.json"), "{")
        code, out = self.h.run("workspaces", "--json")
        self.assertNotEqual(code, 0)
        self.assertIn("can't read", out)

    def test_new_session_prints_the_explicit_agent_and_folder(self):
        code, out = self.h.run("open", "--new", "work", "--cwd", self.api, "--print")
        self.assertEqual(code, 0, out)
        cd, command = out.splitlines()
        self.assertEqual(cd, "cd " + self.h.shown(self.api))
        self.assertIn(".claude-work", command)
        self.assertNotIn("--resume", command)
        fresh = os.path.join(self.h.home, ".codex-fresh")
        code, out = self.h.run("open", "--new", "codex:fresh", "--cwd", self.api, "--print")
        self.assertEqual(code, 0, out)
        self.assertIn("codex", out.splitlines()[1])
        self.assertFalse(os.path.exists(fresh))

    def test_new_session_refuses_a_binding_conflict(self):
        code, out = self.h.run("bind", "other", self.api)
        self.assertEqual(code, 0, out)
        code, out = self.h.run("open", "--new", "work", "--cwd", self.api, "--print")
        self.assertEqual(code, 1, out)
        self.assertIn("is bound to", out)
        self.assertIn(self.h.shown(self.other), out)
        code, out = self.h.run("open", "--new", "other", "--cwd", self.api, "--print")
        self.assertEqual(code, 0, out)
        self.assertIn(".claude-other", out)

    def test_new_session_needs_a_real_project_folder(self):
        missing = os.path.join(self.h.home, "missing")
        code, out = self.h.run("open", "--new", "work", "--cwd", missing, "--print")
        self.assertEqual(code, 1, out)
        self.assertIn("isn't there", out)
        code, out = self.h.run("open", "--cwd", self.api, "--print")
        self.assertEqual(code, 2, out)
        self.assertIn("--cwd requires --new", out)

    @unittest.skipIf(WINDOWS, "a POSIX shell and fake claude")
    def test_new_session_here_runs_the_agent_without_a_resume_id(self):
        bindir = os.path.join(self.h.root, "fakebin")
        write(os.path.join(bindir, "claude"),
              '#!/bin/sh\necho "CLAUDE_CONFIG_DIR=$CLAUDE_CONFIG_DIR cwd=$(pwd) args=$*"\n')
        os.chmod(os.path.join(bindir, "claude"), 0o755)
        code, out = self.h.run("open", "--new", "work", "--cwd", self.api, "--here",
                               PATH=bindir + ":/bin:/usr/bin")
        self.assertEqual(code, 0, out)
        self.assertIn("Starting a new Claude Code session", out)
        self.assertIn("CLAUDE_CONFIG_DIR=%s cwd=%s args=" % (self.work, self.api), out)
        self.assertNotIn("--resume", out)


if __name__ == "__main__":
    unittest.main()
