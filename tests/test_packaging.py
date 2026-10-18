"""The packages: one version everywhere, one licence, and the same six commands from every
way of installing them (the Rust binaries, which PyPI's wheels, the release archives and the
installers carry)."""
import os
import re
import unittest

from helpers import REPO

COMMANDS = ["hatcheck", "hat", "cl", "cx", "claude-z", "claude-k"]
CLI_WINGET = ("hatcheck.Hatcheck.CLI.yaml", "hatcheck.Hatcheck.CLI.installer.yaml", "hatcheck.Hatcheck.CLI.locale.en-US.yaml")


def text(*parts):
    with open(os.path.join(REPO, *parts), encoding="utf-8") as fh:
        return fh.read()


class Packaging(unittest.TestCase):
    def test_one_version(self):
        version = re.search(r'^version = "([^"]+)"', text("Cargo.toml"), re.M).group(1)  # --version prints it
        self.assertIn("version.workspace = true", text("crates", "hatcheck", "Cargo.toml"))
        self.assertIn('dynamic = ["version"]', text("pyproject.toml"))  # maturin takes Cargo's
        for name in CLI_WINGET:
            self.assertIn("PackageVersion: %s\n" % version, text("packaging", "winget", "cli", name))
        self.assertIn("/releases/download/v%s/hatcheck-x86_64-pc-windows-msvc.zip" % version,
                      text("packaging", "winget", "cli", "hatcheck.Hatcheck.CLI.installer.yaml"))

    def test_one_license(self):
        self.assertTrue(text("LICENSE").startswith("MIT License\n\nCopyright (c) "))
        self.assertIn('license = "MIT"', text("Cargo.toml"))
        self.assertIn('license = "MIT"', text("pyproject.toml"))
        self.assertIn('license-files = ["LICENSE"]', text("pyproject.toml"))
        self.assertIn("License: MIT\n", text("packaging", "winget", "hatcheck.Hatcheck.locale.en-US.yaml"))
        self.assertIn("License: MIT\n", text("packaging", "winget", "cli", "hatcheck.Hatcheck.CLI.locale.en-US.yaml"))
        self.assertIn('license = "MIT"', text("desktop", "src-tauri", "Cargo.toml"))
        self.assertIn('"license": "MIT"', text("desktop", "package.json"))
        self.assertIn("## License", text("README.md"))

    def test_every_install_has_the_same_commands(self):
        crate = text("crates", "hatcheck", "Cargo.toml")
        self.assertEqual(sorted(re.findall(r'^\[\[bin\]\]\nname = "([^"]+)"', crate, re.M)), sorted(COMMANDS))
        self.assertIn('bindings = "bin"', text("pyproject.toml"))  # the wheels' commands: all the binaries
        self.assertIn('COMMANDS="%s"' % " ".join(COMMANDS), text("install.sh"))
        self.assertIn("foreach ($name in %s)" % ", ".join('"%s"' % c for c in COMMANDS), text("install.ps1"))
        winget = text("packaging", "winget", "cli", "hatcheck.Hatcheck.CLI.installer.yaml")
        self.assertEqual(re.findall(r"RelativeFilePath: (\S+)", winget), [c + ".exe" for c in COMMANDS])
        self.assertEqual(re.findall(r"PortableCommandAlias: (\S+)", winget), COMMANDS)

    def test_release_workflows(self):
        # PyPI's trusted publisher names release.yml and the pypi environment; cargo-dist writes
        # v-release.yml, and only for tags starting with v, which leaves desktop-v tags alone.
        pypi = text(".github", "workflows", "release.yml")
        self.assertIn("environment: pypi", pypi)
        self.assertIn("id-token: write", pypi)
        self.assertIn("pypa/gh-action-pypi-publish", pypi)
        self.assertNotIn("workflow_call", pypi)
        dist = text("dist-workspace.toml")
        self.assertIn('tag-namespace = "v"', dist)
        self.assertIn('tap = "hatcheck/homebrew-tap"', dist)
        self.assertIn("- 'v**[0-9]+.[0-9]+.[0-9]+*'", text(".github", "workflows", "v-release.yml"))


if __name__ == "__main__":
    unittest.main()
