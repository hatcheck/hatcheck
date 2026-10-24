"""The shared human-output style: colour on a terminal, symbols without it, and raw output alone."""
import json
import subprocess
import unittest

from helpers import ENGINE, WINDOWS, FakeHome, argv_of, in_terminal

S1 = "11111111-1111-4111-8111-111111111111"
ZAI = ('CZ_API_KEY="zai-test-key-1234"\nCZ_MODEL_OPUS="glm-5.3[1m]"\n'
       'CZ_MODEL_SONNET="glm-5.3-flash[1m]"\nCZ_MODEL_HAIKU="glm-4.7"\nCZ_MAX_TOKENS=""\n')


def run(h, args, env, cwd=None):
    p = subprocess.run([ENGINE] + list(args), cwd=cwd or h.root, env=env,
                       capture_output=True, text=True, encoding="utf-8")
    return p.returncode, p.stdout + p.stderr


class OutputStyle(unittest.TestCase):
    def setUp(self):
        self.h = h = FakeHome()
        self.addCleanup(h.cleanup)
        self.default, self.b = h.profile("default"), h.profile("b")
        self.cwd = h.folder("code", "api")
        self.transcript = h.session(self.default, self.cwd, S1, "Fix the flaky login test", 180)
        self.colours = dict(h.env(), TERM="xterm-256color")
        self.colours.pop("NO_COLOR", None)

    @unittest.skipIf(WINDOWS, "needs a pseudo-terminal")
    def test_move_and_dry_run_use_the_shared_colours(self):
        code, dry = in_terminal(argv_of("move") + [S1[:8], "b", "--dry-run"], [], self.colours, self.h.root)
        self.assertEqual(code, 0, dry)
        self.assertIn("\x1b[34m◇ Would move session 11111111", dry)

        code, moved = in_terminal(argv_of("move") + [S1[:8], "b", "--keep"],
                                  [(b"[y/N] ", b"\n")], self.colours, self.h.root)
        self.assertEqual(code, 0, moved)
        self.assertIn("\x1b[32m✓ Copied session 11111111", moved)
        self.assertIn("\x1b[1mResume it with:\x1b[0m", moved)
        self.assertRegex(moved, r"\x1b\[36m[^\n]*cl b --resume " + S1)
        self.assertIn("\x1b[1mResume it now in ", moved)  # the prompt, on stderr, is bold too

    @unittest.skipIf(WINDOWS, "needs a pseudo-terminal")
    def test_no_color_keeps_symbols_but_removes_ansi(self):
        env = dict(self.h.env(), TERM="xterm-256color")  # NO_COLOR=1, as FakeHome sets it
        code, out = in_terminal(argv_of("move") + [S1[:8], "b", "--keep"],
                                [(b"[y/N] ", b"\n")], env, self.h.root)
        self.assertEqual(code, 0, out)
        self.assertIn("✓ Copied session 11111111", out)
        self.assertIn("Resume it with:", out)
        self.assertNotIn("\x1b[", out)

    @unittest.skipIf(WINDOWS, "needs a pseudo-terminal")
    def test_serve_answers_have_no_ansi_even_on_a_terminal(self):
        request = json.dumps({"id": 1, "args": ["move", S1[:8], "b", "--dry-run"]}).encode() + b"\n"
        code, out = in_terminal([ENGINE, "serve"], [(b"", request), (b'"code":', b"\x04")],
                                self.colours, self.h.root)
        self.assertEqual(code, 0, out)
        line = next(line for line in out.splitlines() if line.startswith('{"id":1,'))
        answer = json.loads(line)
        self.assertEqual(answer["code"], 0, answer)
        self.assertTrue(answer["out"].startswith("◇ Would move session 11111111"), answer)
        self.assertNotIn("\x1b[", answer["out"])
        self.assertNotIn("\x1b[", answer["err"])

    def test_pipes_json_and_raw_output_have_no_decoration(self):
        env = self.colours  # a pipe, rather than NO_COLOR, must suppress the escape codes here
        code, dry = run(self.h, ["move", S1[:8], "b", "--dry-run"], env)
        self.assertEqual(code, 0, dry)
        self.assertTrue(dry.startswith("◇ Would move session 11111111"), dry)
        self.assertNotIn("\x1b[", dry)

        code, data = run(self.h, ["move", S1[:8], "b", "--dry-run", "--json"], env)
        self.assertEqual(code, 0, data)
        report = json.loads(data)
        self.assertTrue(report["dry_run"])
        for text in (data,):
            self.assertNotIn("\x1b[", text)
            self.assertNotIn("✓", text)
            self.assertNotIn("◇", text)
            self.assertNotIn("🎩", text)

        self.h.provider_config("z", ZAI)
        for args, expected in [
            (["open", S1[:8], "--print"], "claude --resume " + S1),
            (["where", S1[:8]], self.transcript),
            (["provider", "env", "z"], "ANTHROPIC_AUTH_TOKEN"),
        ]:
            code, out = run(self.h, args, env)
            self.assertEqual(code, 0, out)
            self.assertIn(expected, out)
            self.assertNotIn("\x1b[", out)
            self.assertNotIn("🎩", out)

        code, out = run(self.h, ["bind", "b"], env, self.cwd)
        self.assertEqual(code, 0, out)
        self.assertTrue(out.startswith("✓ Bound "), out)
        code, out = run(self.h, ["bind", "--which", "claude"], env, self.cwd)
        self.assertEqual((code, out), (0, "b\n"))

    def test_hat_appears_only_in_help_and_the_session_heading(self):
        code, help_text = run(self.h, ["--help"], self.colours)
        self.assertEqual(code, 0, help_text)
        self.assertIn("🎩 hatcheck:", help_text)
        self.assertEqual(help_text.count("🎩"), 1)
        code, version = run(self.h, ["--version"], self.colours)
        self.assertEqual(code, 0, version)
        self.assertNotIn("🎩", version)
        code, sessions = run(self.h, ["--no-usage"], self.colours, self.cwd)
        self.assertEqual(code, 0, sessions)
        self.assertTrue(sessions.startswith("🎩 "), sessions)
        self.assertEqual(sessions.count("🎩"), 1)
        self.assertNotIn("\x1b[", sessions)


if __name__ == "__main__":
    unittest.main()
