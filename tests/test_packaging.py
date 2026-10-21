"""The packages: one version everywhere, one licence, and the same seven commands from every
way of installing them (the Rust binaries, which PyPI's wheels, the release archives and the
installers carry)."""
import os
import re
import tomllib
import unittest

from helpers import REPO

COMMANDS = ["hatcheck", "hat", "cl", "cx", "ck", "claude-z", "claude-k"]
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
        self.assertEqual(text("crates", "hatcheck", "LICENSE"), text("LICENSE"))  # the crate's copy, for crates.io
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

    def test_the_crate(self):
        crate = tomllib.loads(text("crates", "hatcheck", "Cargo.toml"))
        package = crate["package"]
        self.assertTrue(os.path.isfile(os.path.join(REPO, "crates", "hatcheck", package["readme"])))
        self.assertLessEqual(len(package["keywords"]), 5)  # crates.io's limit
        self.assertIn("command-line-utilities", package["categories"])
        # cargo binstall finds each archive cargo-dist builds, named and packed as it packs them.
        binstall = package["metadata"]["binstall"]
        targets = tomllib.loads(text("dist-workspace.toml"))["dist"]["targets"]
        for target in targets:
            spec = dict(binstall, **binstall["overrides"].get(target, {}))
            url = spec["pkg-url"].replace("{ repo }", "R").replace("{ version }", "1.2.3")
            url = url.replace("{ name }", "hatcheck").replace("{ target }", target)
            windows = "windows" in target
            self.assertEqual(url, "R/releases/download/v1.2.3/hatcheck-%s%s" % (target, ".zip" if windows else ".tar.xz"))
            self.assertEqual(spec["pkg-fmt"], "zip" if windows else "txz")
            self.assertEqual(spec["bin-dir"], "{ bin }{ binary-ext }" if windows else "{ name }-{ target }/{ bin }{ binary-ext }")

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
        # crates.io's trusted publisher names crates.yml and the crates-io environment.
        crates = text(".github", "workflows", "crates.yml")
        self.assertIn("environment: crates-io", crates)
        self.assertIn("id-token: write", crates)
        self.assertIn("rust-lang/crates-io-auth-action@v1", crates)
        self.assertIn("cargo publish -p hatcheck --locked", crates)


if __name__ == "__main__":
    unittest.main()


class InstallScript(unittest.TestCase):
    """install.sh --from DIR against a throwaway home: the links it makes, and the hat it holds back
    for another program of that name, but not for a hat that is hatcheck's own."""

    def setUp(self):
        if os.name == "nt":
            self.skipTest("install.sh is for macOS and Linux")
        from helpers import ENGINE, FakeHome
        self.h = FakeHome()
        self.addCleanup(self.h.cleanup)
        self.engine = ENGINE
        self.built = self.link_all(os.path.join(self.h.root, "built"))

    def link_all(self, folder, names=COMMANDS):
        os.makedirs(folder, exist_ok=True)
        for name in names:
            os.symlink(self.engine, os.path.join(folder, name))
        return folder

    def run_install(self, path_dirs, bin_dir):
        import subprocess
        env = dict(self.h.env(), PATH=os.pathsep.join(path_dirs + ["/bin", "/usr/bin"]))
        p = subprocess.run(["/bin/bash", os.path.join(REPO, "install.sh"), "--bin", bin_dir, "--from", self.built],
                           env=env, capture_output=True, text=True, encoding="utf-8")
        return p.returncode, p.stdout + p.stderr

    def test_hat_gives_way_to_another_program_only(self):
        h = self.h
        # A hat from another install of hatcheck is no reason to hold back.
        elsewhere = self.link_all(os.path.join(h.root, "elsewhere"), ["hat"])
        bin_dir = os.path.join(h.home, ".local", "bin")
        code, out = self.run_install([elsewhere], bin_dir)
        self.assertEqual(code, 0, out)
        self.assertIn("✅ hat -> ", out)
        for name in COMMANDS:
            self.assertEqual(os.readlink(os.path.join(bin_dir, name)), os.path.join(self.built, name))
        # Another program called hat: the short name gives way to it, the five others are linked.
        foreign = os.path.join(h.root, "foreign")
        os.makedirs(foreign)
        with open(os.path.join(foreign, "hat"), "w", encoding="utf-8") as fh:
            fh.write("#!/bin/sh\necho nope\n")
        os.chmod(os.path.join(foreign, "hat"), 0o755)
        other_bin = os.path.join(h.home, "other-bin")
        code, out = self.run_install([foreign], other_bin)
        self.assertEqual(code, 0, out)
        self.assertIn("Didn't add hat: %s has that name" % os.path.join(foreign, "hat"), out)
        self.assertFalse(os.path.lexists(os.path.join(other_bin, "hat")))
        self.assertTrue(all(os.path.islink(os.path.join(other_bin, n)) for n in COMMANDS if n != "hat"))
