"""hatcheck show --markdown: a session's conversation as a Markdown document. Against a
throwaway home: no real profiles, Keychain or network."""
import os
import re
import unittest

from helpers import NOW, FakeHome, iso, rec

A = "aaaaaaaa-0000-4000-8000-000000000001"
X = "019f0000-aaaa-7000-8000-000000000001"


class ShowMarkdown(unittest.TestCase):
    def setUp(self):
        self.h = h = FakeHome()
        self.addCleanup(h.cleanup)
        work = h.profile("work")
        base = {"cwd": h.folder("code"), "sessionId": A, "entrypoint": "cli", "gitBranch": "main"}
        talk = [rec(type="assistant", timestamp=iso(NOW - 200), message={"role": "assistant", "content": [
                    {"type": "tool_use", "id": "t1", "name": "Bash", "input": {"command": "ls `pwd`"}}]}, **base),
                rec(type="user", timestamp=iso(NOW - 190), message={"role": "user", "content": [
                    {"type": "tool_result", "tool_use_id": "t1", "content": "a\nb"}]}, **base),
                rec(type="assistant", timestamp=iso(NOW - 180), message={"role": "assistant", "content": [
                    {"type": "text", "text": "Two files: `a` and `b`."}]}, **base)]
        h.session(work, h.folder("code"), A, "Fix the login test", 3, extra=talk)
        h.codex_session(h.codex_home("default"), h.folder("code"), X, "Speed up the build", 20)

    def test_a_document_with_the_facts_the_turns_and_the_tool_calls_fenced(self):
        code, out = self.h.run("show", A[:8], "--markdown")
        self.assertEqual(code, 0, out)
        head, rest = out.split("\n", 1)
        self.assertEqual(head, "# Fix the login test")
        facts = rest.lstrip("\n").split("\n", 1)[0]
        sep = re.escape(os.sep)
        self.assertRegex(facts, r"^Claude Code session `" + A + r"` in `~" + sep + r"\.claude-work`, started from `~" + sep
                         + r"code`, on branch `main`, 2 turns, 1 tool call\. Exported by hatcheck \d+\.\d+\.\d+ on .+\. "
                         r"Left out: .+\.$")
        self.assertRegex(out, r"\n## You · \d{4}-\d\d-\d\d \d\d:\d\d\n\nFix the login test\n")
        self.assertRegex(out, r"\n## Claude · \d{4}-\d\d-\d\d \d\d:\d\d\n\nok\n")
        # The tool call and its result, in code blocks fenced longer than any backticks inside.
        self.assertIn("\n```text\n[external_agent_tool_call: Bash]\ncommand: ls `pwd`\n[/external_agent_tool_call]\n```\n", out)
        self.assertIn("\n```text\n[external_agent_tool_result]\na\nb\n[/external_agent_tool_result]\n```\n", out)
        self.assertIn("\nTwo files: `a` and `b`.\n", out)
        self.assertNotIn("\x1b[", out)  # raw output, no colours
        # The last N turns, said so; the brief, said so.
        code, out = self.h.run("show", A[:8], "--markdown", "--last", "1")
        self.assertEqual(code, 0, out)
        self.assertIn("the last 1 of 2 turns, 1 tool call.", out)
        self.assertEqual(len(re.findall(r"^## ", out, re.M)), 1)
        code, out = self.h.run("show", A[:8], "--markdown", "--brief")
        self.assertIn("A handover brief, as hatcheck move --brief hands one over", out)
        # Codex's, with its speaker and its tool calls as conv writes them.
        code, out = self.h.run("show", X[:13], "-m")
        self.assertEqual(code, 0, out)
        self.assertTrue(out.startswith("# Speed up the build\n\nCodex session `" + X + "` in `" + "~/.codex".replace("/", os.sep) + "`"), out[:120])
        self.assertIn("\n## Codex · ", out)
        self.assertIn("[external_agent_tool_call: exec_command]", out)
        self.assertNotIn("SECRET-REASONING", out)
        code, out = self.h.run("show", A[:8], "--markdown", "--json")
        self.assertEqual(code, 2, out)
        self.assertIn("not allowed with argument", out)


if __name__ == "__main__":
    unittest.main()
