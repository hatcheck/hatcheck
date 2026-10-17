"""The Python package and the Homebrew formula: one version everywhere, and every command
they install points at code that's there."""
import os
import re
import unittest

from helpers import REPO, load


def text(*parts):
    with open(os.path.join(REPO, *parts), encoding="utf-8") as fh:
        return fh.read()


class Packaging(unittest.TestCase):
    def test_one_version(self):
        version = re.search(r'^version = "([^"]+)"', text("pyproject.toml"), re.M).group(1)
        self.assertEqual(re.search(r'__version__ = "([^"]+)"', text("hatcheck", "__init__.py")).group(1), version)
        self.assertEqual(load("cli").version(), version)
        self.assertIn("refs/tags/v%s.tar.gz" % version, text("packaging", "homebrew", "hatcheck.rb"))

    def test_one_license(self):
        self.assertTrue(text("LICENSE").startswith("MIT License\n\nCopyright (c) "))
        self.assertIn('license = "MIT"', text("pyproject.toml"))
        self.assertIn('license-files = ["LICENSE"]', text("pyproject.toml"))
        self.assertIn('license "MIT"', text("packaging", "homebrew", "hatcheck.rb"))
        self.assertIn("License: MIT\n", text("packaging", "winget", "yilab.Hatcheck.locale.en-US.yaml"))
        self.assertIn('license = "MIT"', text("desktop", "src-tauri", "Cargo.toml"))
        self.assertIn('"license": "MIT"', text("desktop", "package.json"))
        self.assertIn("## License", text("README.md"))

    def test_commands_point_at_functions(self):
        scripts = text("pyproject.toml").split("[project.scripts]")[1].split("\n[")[0]
        commands = dict(re.findall(r'^([\w-]+) = "hatcheck\.(\w+:\w+)"', scripts, re.M))
        self.assertEqual(sorted(commands), ["cl", "claude-k", "claude-z", "cx", "hat", "hatcheck"])
        for command, target in commands.items():
            module, function = target.split(":")
            self.assertTrue(callable(getattr(load(module), function, None)), command)

    def test_installers_make_the_same_commands(self):
        formula = text("packaging", "homebrew", "hatcheck.rb")
        for command in ("hatcheck", "hat", "claude-z", "claude-k"):
            self.assertIn('"%s"' % command, formula)
        self.assertIn('libexec/"cl", libexec/"cx"', formula)
        self.assertIn("hatcheck:hatcheck.py hat:hatcheck.py cl:cl cx:cx claude-z:cl claude-k:cl", text("install.sh"))


if __name__ == "__main__":
    unittest.main()
