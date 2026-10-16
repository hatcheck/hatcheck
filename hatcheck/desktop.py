#!/usr/bin/env python3
"""Claude Desktop instances on macOS: list them, launch or bring one forward, or make one.

Usage:
    hatcheck desktop [--json]                         the instances, and which are running
    hatcheck desktop open NAME                        launch an instance, or bring it forward
    hatcheck desktop new NAME DISPLAY [HUE [SAT]]     make one, with its own data folder and icon

An instance is a wrapper app made by make-claude-instance.sh, such as
/Applications/Claude Work.app, which starts Claude Desktop with its own data folder,
~/.claude-instances/<name>, so it has its own login. NAME is the instance's name or
the wrapper's display name. HUE (0-255) and SAT recolor the icon, as
make-claude-instance.sh takes them; new needs that script, from this repo.
"""
import argparse
import glob
import json
import os
import shutil
import subprocess
import sys

HOME = os.path.expanduser("~")
INSTANCES = os.path.join(HOME, ".claude-instances")


def instances(folders=("/Applications", os.path.join(HOME, "Applications"))):
    """[{name, display, app, data_dir}] for the wrapper apps make-claude-instance.sh made."""
    found = []
    for app in sorted(path for folder in folders for path in glob.glob(os.path.join(folder, "Claude *.app"))):
        name = None
        try:
            with open(os.path.join(app, "Contents", "MacOS", "config.sh"), encoding="utf-8") as fh:
                for line in fh:
                    if line.startswith("INSTANCE_NAME="):
                        name = line.split("=", 1)[1].strip().strip('"\'')
        except OSError:
            continue  # not one of ours, such as Claude Code URL Handler.app
        if name:
            found.append({"name": name, "display": os.path.basename(app)[len("Claude "):-len(".app")], "app": app,
                          "data_dir": None if name == "default" else os.path.join(INSTANCES, name)})
    return found


def running(found):
    """Mark each instance running or not, from its main process: Claude Desktop's own process
    with that --user-data-dir (none for the default instance), not one of its helpers."""
    try:
        out = subprocess.run(["ps", "-axww", "-o", "command="], capture_output=True, text=True).stdout
    except OSError:
        out = ""
    mains = [line for line in out.splitlines() if "/Claude.app/Contents/MacOS/" in line and "--type=" not in line]
    for inst in found:
        if inst["data_dir"]:
            inst["running"] = any("--user-data-dir=" + inst["data_dir"] in line for line in mains)
        else:
            inst["running"] = any("--user-data-dir=" not in line for line in mains)
    return found


def instance_script(here=None, path=None):
    """make-claude-instance.sh from the hatcheck repo, or None: beside this code (a clone, or
    Homebrew's libexec), in the folder the hatcheck on the PATH really lives in (the clone
    install.sh linked), or on the PATH itself. The app's bundled engine carries no copy, so
    from the app it's one of the installed ones. `here` and `path` are for tests."""
    folders = [here or os.path.dirname(os.path.dirname(os.path.realpath(__file__)))]
    for name in ("hatcheck", "make-claude-instance.sh"):
        found = shutil.which(name, path=path)
        if found:
            folders.append(os.path.dirname(os.path.realpath(found)))
    for folder in folders:
        script = os.path.join(folder, "make-claude-instance.sh")
        if os.path.isfile(script):
            return script
    return None


def find(name, found):
    match = [i for i in found if name in (i["name"], i["display"])]
    if not match:
        sys.exit("No Claude Desktop instance named %s; hatcheck desktop lists them" % name)
    return match[0]


def main(argv=None, prog=None):
    parser = argparse.ArgumentParser(prog=prog, description="Claude Desktop instances on macOS: list them, launch "
                                                            "one, or make one.")
    parser.add_argument("action", nargs="?", choices=("open", "new"), help="open NAME, or new NAME DISPLAY [HUE [SAT]]")
    parser.add_argument("words", nargs="*", help=argparse.SUPPRESS)
    parser.add_argument("--json", action="store_true", help="list the instances as JSON")
    args = parser.parse_args(argv)
    if sys.platform != "darwin":
        sys.exit("Claude Desktop instances are macOS only for now")
    if args.action == "open":
        if len(args.words) != 1:
            parser.error("open takes the instance's name")
        inst = find(args.words[0], instances())
        # The wrapper's launcher brings a running instance forward instead of starting another.
        return subprocess.call(["open", "-a", inst["app"]])
    if args.action == "new":
        if not 2 <= len(args.words) <= 4:
            parser.error("new takes NAME DISPLAY [HUE [SAT]]")
        script = instance_script()
        if not script:
            sys.exit("hatcheck desktop new needs make-claude-instance.sh from the hatcheck repo: clone it and run its "
                     "install.sh, or brew install yilab/tap/hatcheck")
        return subprocess.call(["bash", script] + args.words)
    found = running(instances())
    if args.json:
        print(json.dumps({"version": 1, "instances": found}, ensure_ascii=False, indent=2))
        return
    if not found:
        print("No Claude Desktop instances yet. Make one with: hatcheck desktop new work Work 135")
        return
    for inst in found:
        print("  %-20s %-10s %s%s" % (inst["display"], inst["name"], inst["app"], "  ● running" if inst["running"] else ""))


if __name__ == "__main__":
    sys.exit(main())
