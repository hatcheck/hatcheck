"""Build the hatcheck engine the app ships: the hatcheck CLI as one standalone binary, made
with PyInstaller, at src-tauri/binaries/hatcheck-<target triple>, where Tauri looks for its
sidecars. Users need no Python. PyInstaller runs through uvx, so it isn't installed.

    python3 scripts/build-engine.py [--target TRIPLE]

The CLI's scripts load each other by path, so PyInstaller can't see what they import; the
standard-library modules they import are listed for it here.
"""
import argparse
import ast
import glob
import os
import shutil
import subprocess
import sys
import tempfile

DESKTOP = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
REPO = os.path.dirname(DESKTOP)


def host_triple():
    out = subprocess.run(["rustc", "-vV"], capture_output=True, text=True, check=True).stdout
    return next(line.split(":", 1)[1].strip() for line in out.splitlines() if line.startswith("host:"))


def imports():
    """The standard-library modules the hatcheck/ scripts import."""
    names = set()
    for path in glob.glob(os.path.join(REPO, "hatcheck", "*.py")):
        with open(path, encoding="utf-8") as fh:
            tree = ast.parse(fh.read())
        for node in ast.walk(tree):
            if isinstance(node, ast.Import):
                names.update(a.name for a in node.names)
            elif isinstance(node, ast.ImportFrom) and node.module and not node.level:
                names.add(node.module)
    stdlib = getattr(sys, "stdlib_module_names", None)
    return sorted(n for n in names if stdlib is None or n.split(".")[0] in stdlib)


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--target", help="the Rust target triple to name it for (default: this machine's)")
    args = parser.parse_args()
    triple = args.target or host_triple()
    exe = ".exe" if "windows" in triple else ""
    out_dir = os.path.join(DESKTOP, "src-tauri", "binaries")
    os.makedirs(out_dir, exist_ok=True)
    with tempfile.TemporaryDirectory() as tmp:
        command = ["uvx", "--from", "pyinstaller", "pyinstaller", "--onefile", "--noconfirm", "--clean",
                   "--name", "hatcheck", "--distpath", os.path.join(tmp, "dist"), "--workpath", os.path.join(tmp, "build"),
                   "--specpath", tmp, "--add-data", os.path.join(REPO, "hatcheck") + os.pathsep + "hatcheck"]
        for name in imports():
            command += ["--hidden-import", name]
        command.append(os.path.join(REPO, "hatcheck.py"))
        subprocess.run(command, check=True)
        built = os.path.join(tmp, "dist", "hatcheck" + exe)
        target = os.path.join(out_dir, "hatcheck-%s%s" % (triple, exe))
        shutil.copy2(built, target)
    print("Built", os.path.relpath(target, DESKTOP))


if __name__ == "__main__":
    main()
