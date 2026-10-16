"""How commands and paths are printed for each shell, checked on any OS by giving
claude-recent-sessions a Windows-style os module."""
import ntpath
import os
import shutil
import tempfile
import types
import unittest

from helpers import load

ID = "abcdef12-0000-4000-8000-000000000001"
CODEX_ID = "019f0000-aaaa-7000-8000-000000000001"


class PrintedCommands(unittest.TestCase):
    def windows(self, powershell=True):
        s = load("claude-recent-sessions.py")
        fake_os = types.SimpleNamespace(**{k: getattr(os, k) for k in dir(os) if not k.startswith("__")})
        fake_os.name, fake_os.sep, fake_os.path = "nt", "\\", ntpath
        s.os, s.HOME, s.DEFAULT_PROFILE = fake_os, "C:\\Users\\me", "C:\\Users\\me\\.claude"
        s.DEFAULT_CODEX_HOME = "C:\\Users\\me\\.codex"
        s.powershell = lambda: powershell
        return s

    def test_powershell(self):
        s = self.windows()
        self.assertEqual(s.shell_path("C:\\Users\\me"), "~")
        self.assertEqual(s.shell_path("C:\\Users\\me\\code\\api"), "~\\code\\api")
        self.assertEqual(s.shell_path("C:\\Users\\me\\My Code"), "'C:\\Users\\me\\My Code'")  # quoted: no ~
        self.assertEqual(s.shell_path("D:\\work\\api"), "D:\\work\\api")
        self.assertEqual(s.shell_path("D:\\it's"), "'D:\\it''s'")
        self.assertEqual(s.resume_command("C:\\Users\\me\\.claude", ID, False), "claude --resume " + ID)
        self.assertEqual(s.resume_command("C:\\Users\\me\\.claude-work", ID, False), "cl work --resume " + ID)
        self.assertEqual(s.resume_command("C:\\Users\\me\\.claude-work", ID, True),
                         '$env:CLAUDE_CONFIG_DIR="$HOME\\.claude-work"; claude --resume %s; '
                         '$env:CLAUDE_CONFIG_DIR=$null' % ID)
        self.assertEqual(s.resume_command("D:\\p\\.claude-x", ID, False),
                         "$env:CLAUDE_CONFIG_DIR='D:\\p\\.claude-x'; claude --resume %s; "
                         "$env:CLAUDE_CONFIG_DIR=$null" % ID)

    def test_posix_shell_on_windows(self):  # Git Bash
        s = self.windows(powershell=False)
        self.assertEqual(s.shell_path("C:\\Users\\me\\code\\api"), "~/code/api")
        self.assertEqual(s.shell_path("D:\\x y"), "'D:/x y'")
        self.assertEqual(s.resume_command("C:\\Users\\me\\.claude-work", ID, True),
                         "CLAUDE_CONFIG_DIR=~/.claude-work claude --resume " + ID)

    def test_providers(self):
        s = self.windows()
        self.assertEqual(s.resume_command("C:\\Users\\me\\.claude", ID, False, "z"), "claude-z --resume " + ID)
        self.assertEqual(s.resume_command("C:\\Users\\me\\.claude-work", ID, False, "k"), "cl -k work --resume " + ID)
        self.assertEqual(s.resume_command("C:\\Users\\me\\.claude-work", ID, True, "k"),
                         '$env:CLAUDE_CONFIG_DIR="$HOME\\.claude-work"; claude-k --resume %s; '
                         '$env:CLAUDE_CONFIG_DIR=$null' % ID)
        s = self.windows(powershell=False)
        self.assertEqual(s.resume_command("C:\\Users\\me\\.claude-work", ID, True, "z"),
                         "CLAUDE_CONFIG_DIR=~/.claude-work claude-z --resume " + ID)
        for model, provider in (("glm-5.3", "z"), ("GLM-4.7", "z"), ("k3", "k"), ("kimi-for-coding", "k"),
                                ("claude-opus-5-5", None), ("deepseek-v4", None), (None, None)):
            self.assertEqual(s.provider_of(model), provider, model)

    def test_codex(self):
        s = self.windows()
        self.assertEqual(s.codex_resume_command("C:\\Users\\me\\.codex", CODEX_ID, False), "codex resume " + CODEX_ID)
        self.assertEqual(s.codex_resume_command("C:\\Users\\me\\.codex-work", CODEX_ID, False),
                         "cx work resume " + CODEX_ID)
        self.assertEqual(s.codex_resume_command("C:\\Users\\me\\.codex-work", CODEX_ID, True),
                         '$env:CODEX_HOME="$HOME\\.codex-work"; codex resume %s; $env:CODEX_HOME=$null' % CODEX_ID)
        self.assertEqual(s.codex_resume_command("D:\\p\\.codex-x", CODEX_ID, False),
                         "$env:CODEX_HOME='D:\\p\\.codex-x'; codex resume %s; $env:CODEX_HOME=$null" % CODEX_ID)
        s = self.windows(powershell=False)
        self.assertEqual(s.codex_resume_command("C:\\Users\\me\\.codex-work", CODEX_ID, True),
                         "CODEX_HOME=~/.codex-work codex resume " + CODEX_ID)

    def test_posix(self):
        s = load("claude-recent-sessions.py")
        s.HOME, s.powershell = "/home/me", lambda: False
        if os.sep == "/":
            self.assertEqual(s.shell_path("/home/me/code/api"), "~/code/api")
            self.assertEqual(s.shell_path("/home/me/My Code"), "~/'My Code'")
            self.assertEqual(s.shell_path("/srv/x"), "/srv/x")

    def test_windows_paths_in_transcripts_are_repointed(self):
        m = load("claude-move-session.py")
        old, new = "C:\\Users\\me\\.claude\\projects\\X\\" + ID, "C:\\Users\\me\\.claude-b\\projects\\X\\" + ID
        line = ('{"type":"user","message":{"role":"user","content":"saved to %s\\\\tool-results\\\\a.txt"}}'
                % old.replace("\\", "\\\\"))
        folder = tempfile.mkdtemp()
        self.addCleanup(shutil.rmtree, folder)
        path = os.path.join(folder, ID + ".jsonl")
        with open(path, "w", encoding="utf-8") as fh:
            fh.write(line + "\n")
        out = m.copy_bytes(path, [(old, new)]).decode()
        self.assertIn(new.replace("\\", "\\\\"), out)
        self.assertNotIn(old.replace("\\", "\\\\"), out)


if __name__ == "__main__":
    unittest.main()
