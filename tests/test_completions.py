"""hatcheck completions: the scripts for zsh, bash, fish and PowerShell, and what they ask of
hatcheck as they run. Against a throwaway home: no real profiles, Keychain or network."""
import os
import shutil
import subprocess
import tempfile
import unittest

from helpers import FakeHome

A = "aaaaaaaa-0000-4000-8000-000000000001"
B = "bbbbbbbb-0000-4000-8000-000000000002"
X = "019f0000-aaaa-7000-8000-000000000001"


class Completions(unittest.TestCase):
    def setUp(self):
        self.h = FakeHome()
        self.addCleanup(self.h.cleanup)

    def test_each_shell_gets_a_script_the_shell_takes(self):
        for shell in ("zsh", "bash", "fish", "powershell"):
            code, out = self.h.run("completions", shell)
            self.assertEqual(code, 0, out)
            for word in ("search", "since", "profiles", "statusline", "hatcheck complete profiles",
                         "hatcheck complete sessions", "relocate", "desktop"):
                self.assertIn(word, out, (shell, word))
            self.assertNotIn("--no-details", out)  # hidden options stay hidden
            if shell == "zsh":
                self.assertTrue(out.startswith("#compdef hatcheck hat\n"), out[:80])
                self.assertIn("compdef _hatcheck hatcheck hat\n", out)
                self.assertIn("new", out)  # desktop's own subcommands
            if shell == "bash":
                self.assertTrue(out.endswith("complete -F _hatcheck hatcheck hat\n"), out[-80:])
            if shell == "fish":
                self.assertIn("complete -c hat ", out)
                self.assertIn("-n '__fish_seen_subcommand_from search' -l since", out)
            if shell == "powershell":
                self.assertIn("Register-ArgumentCompleter -Native -CommandName hatcheck, hat", out)
            # The shells that are here read the script without complaint.
            program = {"zsh": ["zsh", "-n"], "bash": ["bash", "-n"], "fish": ["fish", "-n"],
                       "powershell": ["pwsh", "-NoProfile", "-Command"]}[shell]
            env = self.h.env()
            found = shutil.which(program[0], path=env["PATH"])
            if not found:
                continue
            with tempfile.NamedTemporaryFile("w", suffix="." + shell, delete=False, encoding="utf-8") as fh:
                fh.write(out)
                path = fh.name
            self.addCleanup(os.remove, path)
            argv = program + ([path] if shell != "powershell" else
                              ["[scriptblock]::Create((Get-Content -Raw '%s')) | Out-Null" % path])
            p = subprocess.run(argv, env=env, capture_output=True, text=True)
            self.assertEqual(p.returncode, 0, (shell, p.stdout + p.stderr))
        code, out = self.h.run("completions", "nope")
        self.assertEqual(code, 2, out)
        self.assertIn("'nope' isn't one of zsh, bash, fish or powershell", out)
        code, out = self.h.run("completions")
        self.assertEqual(code, 2, out)

    def test_what_the_scripts_ask_for(self):
        h = self.h
        work = h.profile("work")
        home = h.profile("home")
        codex = h.codex_home("default")
        h.session(work, h.folder("code"), A, "Fix the login test", 5)
        h.session(home, h.folder("code"), B, "An old one, two weeks ago", 60 * 24 * 14)
        h.codex_session(codex, h.folder("code"), X, "Speed up the build", 20)
        code, out = h.run("complete", "profiles")
        self.assertEqual(code, 0, out)
        self.assertEqual(out, "home\t~/.claude-home\nwork\t~/.claude-work\ncodex\t~/.codex\n")
        code, out = h.run("complete", "sessions")
        self.assertEqual(code, 0, out)
        self.assertEqual(out, A + "\tFix the login test\n" + X + "\tSpeed up the build\n")  # the last week's, newest first
        code, out = h.run("complete")
        self.assertEqual(code, 2, out)
        self.assertIn("hatcheck complete: profiles or sessions", out)
        code, out = h.run("complete", "nope")
        self.assertEqual(code, 2, out)


if __name__ == "__main__":
    unittest.main()
