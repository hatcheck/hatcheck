#!/usr/bin/env python3
"""Claude Desktop instances on macOS: one Claude Desktop per account, each with its own data
folder and login, behind a launcher app of its own colour.

Usage:
    hatcheck desktop [--json]                      the instances, and which are running
    hatcheck desktop new NAME DISPLAY [--hue DEG] [--saturation X] [--replace]
    hatcheck desktop open NAME                     start an instance, or bring it forward
    hatcheck desktop remove NAME [--data]          delete its launcher; --data deletes its
                                                   data folder too, login and all

Claude Desktop is an Electron app, and Electron's --user-data-dir runs it with another
data folder: its own login, settings, sessions and MCP servers. new makes a launcher
app, /Applications/Claude DISPLAY.app, that starts Claude Desktop with
~/.claude-instances/NAME as that folder, or brings the running one forward rather
than starting a second: Claude Desktop keeps no single-instance lock per folder, and
only the first process on a folder gets its login. So give every account its own
instance, the personal one too, and leave Claude's built-in folder to the stock
Claude icon. NAME default makes a launcher for that built-in folder, for those who
want one; don't also use the stock icon then.

The launcher's icon is Claude's with its hues turned by --hue degrees and its
saturation scaled by --saturation, when Pillow is installed (pip install Pillow);
the stock icon otherwise. The colour is on the launcher only: a running instance
shows Claude's own icon and name in the Dock, since they all run the one
/Applications/Claude.app. docs/desktop-instances.md has the whole description.

Launchers made by the make-claude-instance.sh script this replaced are listed and
opened too; new --replace rebuilds one.
"""
import argparse
import glob
import json
import os
import plistlib
import re
import shutil
import subprocess
import sys
import tempfile

HOME = os.path.expanduser("~")
INSTANCES = os.path.join(HOME, ".claude-instances")
CLAUDE_APP = "/Applications/Claude.app"
APPS_DIR = "/Applications"
APP_FOLDERS = (APPS_DIR, os.path.join(HOME, "Applications"))
NAME_OK = re.compile(r"[A-Za-z0-9._-]+")

# The launcher, a shell script: it starts Claude Desktop with the instance's data folder, or
# brings the running one forward. %(name)s, %(data_dir)s and %(claude_app)s are filled in.
LAUNCHER = r'''#!/bin/sh
# The launcher of the Claude Desktop instance %(name)s, made by hatcheck desktop new. It starts
# Claude Desktop with this instance's data folder, or brings the one already running forward:
# a second process on a folder that has one would come up signed out, so it never starts one.
DATA_DIR=%(data_dir)s
CLAUDE_APP=%(claude_app)s

# Process IDs of Claude Desktop's processes on this data folder: its main process, or with
# "window", its windows (Electron's renderer helpers, which get the folder on their command
# line too). An empty DATA_DIR means Claude's built-in folder: the processes with no folder.
processes() {
    ps -axww -o pid=,args= | awk -v app="$CLAUDE_APP/Contents/MacOS/" -v dir="$DATA_DIR" -v want="$1" '
        function has(line, option) {
            return index(line, option " ") > 0 || substr(line, length(line) - length(option) + 1) == option
        }
        index($0, app) == 0 { next }
        dir != "" && !has($0, "--user-data-dir=" dir) { next }
        dir == "" && index($0, "--user-data-dir=") > 0 { next }
        want == "main" && index($0, "--type=") > 0 { next }
        want == "window" && index($0, "--type=renderer") == 0 { next }
        { print $1 }'
}

start() {
    if [ -n "$DATA_DIR" ]; then
        mkdir -p "$DATA_DIR" || exit 1
        exec open -n -a "$CLAUDE_APP" --args --user-data-dir="$DATA_DIR"
    fi
    exec open -n -a "$CLAUDE_APP"
}

main=$(processes main | head -n 1)
[ -n "$main" ] || start
# To macOS every instance is the same app, so it's picked out by its process ID. The first time,
# macOS asks to let this launcher control System Events.
osascript -e "tell application \"System Events\" to set frontmost of (first process whose unix id is $main) to true" >/dev/null 2>&1
# Closed with Cmd-W, the window is gone but the process lives on. Coming forward usually brings
# the window back; when it hasn't after a moment, start a fresh one. The windowless process is
# left alone: it may be in the middle of a reply.
if [ -z "$(processes window)" ]; then
    sleep 1.5
    [ -n "$(processes window)" ] || start
fi
exit 0
'''


def version():
    try:
        with open(os.path.join(os.path.dirname(os.path.realpath(__file__)), "__init__.py"), encoding="utf-8") as fh:
            return re.search(r'__version__ = "([^"]+)"', fh.read()).group(1)
    except (OSError, AttributeError):
        return "0"


def read_plist(path):
    try:
        with open(path, "rb") as fh:
            data = plistlib.load(fh)
    except (OSError, ValueError, plistlib.InvalidFileException):
        return None
    return data if isinstance(data, dict) else None


def legacy_name(app):
    """The instance name a launcher made by make-claude-instance.sh keeps, in its config.sh."""
    try:
        with open(os.path.join(app, "Contents", "MacOS", "config.sh"), encoding="utf-8") as fh:
            for line in fh:
                if line.startswith("INSTANCE_NAME="):
                    return line.split("=", 1)[1].strip().strip("\"'") or None
    except OSError:
        pass
    return None


def instances(folders=APP_FOLDERS, instances_dir=INSTANCES):
    """[{name, display, app, data_dir, legacy}] for the launcher apps: ours, and those the
    old script made."""
    found = []
    for app in sorted(path for folder in folders for path in glob.glob(os.path.join(folder, "Claude *.app"))):
        plist = read_plist(os.path.join(app, "Contents", "Info.plist")) or {}
        name, legacy = plist.get("HatcheckInstance"), False
        if not isinstance(name, str) or not name:
            name, legacy = legacy_name(app), True
        if not name:
            continue  # another app that starts with Claude, such as Claude Code URL Handler.app
        data_dir = plist.get("HatcheckDataDir") if not legacy else None
        if not data_dir:
            data_dir = None if name == "default" else os.path.join(instances_dir, name)
        found.append({"name": name, "display": os.path.basename(app)[len("Claude "):-len(".app")], "app": app,
                      "data_dir": data_dir, "legacy": legacy})
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
            option = "--user-data-dir=" + inst["data_dir"]
            inst["running"] = any(option + " " in line or line.endswith(option) for line in mains)
        else:
            inst["running"] = any("--user-data-dir=" not in line for line in mains)
    return found


def find(name, found):
    match = [i for i in found if name in (i["name"], i["display"])]
    if not match:
        sys.exit("No Claude Desktop instance named %s; hatcheck desktop lists them" % name)
    return match[0]


def sh_quote(text):
    return "'" + text.replace("'", "'\\''") + "'"


def stock_icon(claude_app):
    """Claude Desktop's own icon file, from its Info.plist."""
    plist = read_plist(os.path.join(claude_app, "Contents", "Info.plist")) or {}
    name = plist.get("CFBundleIconFile") or "icon.icns"
    if not name.endswith(".icns"):
        name += ".icns"
    path = os.path.join(claude_app, "Contents", "Resources", name)
    return path if os.path.isfile(path) else None


def recolour(src, dst, hue, saturation):
    """Write the icon at src to dst with every hue turned by `hue` degrees and the saturation
    scaled by `saturation`. False, with nothing written, when that can't be done here: no Pillow,
    no iconutil, or an icon iconutil can't take apart."""
    try:
        from PIL import Image
    except ImportError:
        return False
    if not shutil.which("iconutil"):
        return False
    shift = int(round(hue / 360.0 * 256)) % 256
    with tempfile.TemporaryDirectory() as tmp:
        iconset = os.path.join(tmp, "icon.iconset")
        if subprocess.run(["iconutil", "-c", "iconset", src, "-o", iconset], capture_output=True).returncode != 0:
            return False
        for png in glob.glob(os.path.join(iconset, "*.png")):
            try:
                image = Image.open(png).convert("RGBA")
            except OSError:
                continue
            alpha = image.getchannel("A")
            h, s, v = image.convert("RGB").convert("HSV").split()
            h = h.point(lambda x: (x + shift) % 256)
            s = s.point(lambda x: max(0, min(255, int(round(x * saturation)))))
            out = Image.merge("HSV", (h, s, v)).convert("RGB")
            out.putalpha(alpha)
            out.save(png)
        made = subprocess.run(["iconutil", "-c", "icns", iconset, "-o", dst], capture_output=True).returncode == 0
    return made and os.path.isfile(dst)


def make_instance(name, display, hue=0.0, saturation=1.0, replace=False, apps_dir=APPS_DIR, claude_app=CLAUDE_APP,
                  instances_dir=INSTANCES):
    """Make the launcher app for an instance: {app, data_dir, recoloured}. Raises ValueError
    for a bad name or display, FileNotFoundError without Claude Desktop, and FileExistsError
    when the launcher exists and replace is off."""
    display = " ".join(display.split())
    if not NAME_OK.fullmatch(name):
        raise ValueError("The name can have letters, digits, dots, underscores and dashes: not %r" % name)
    if not display or "/" in display or display.startswith("."):
        raise ValueError("The display name can't be empty, start with a dot or hold a slash: not %r" % display)
    if not os.path.isdir(claude_app):
        raise FileNotFoundError("Claude Desktop isn't at %s; install it from https://claude.ai/download" % claude_app)
    app = os.path.join(apps_dir, "Claude %s.app" % display)
    if os.path.lexists(app) and not replace:
        raise FileExistsError("%s exists already; --replace rebuilds it" % app)
    data_dir = None if name == "default" else os.path.join(instances_dir, name)
    stock = stock_icon(claude_app)

    tmp = tempfile.mkdtemp(prefix="hatcheck-instance-")
    try:
        built = os.path.join(tmp, "Claude %s.app" % display)
        macos, resources = os.path.join(built, "Contents", "MacOS"), os.path.join(built, "Contents", "Resources")
        os.makedirs(macos)
        os.makedirs(resources)
        launcher = os.path.join(macos, "launch")
        with open(launcher, "w", encoding="utf-8", newline="\n") as fh:
            fh.write(LAUNCHER % {"name": name, "data_dir": sh_quote(data_dir) if data_dir else '""',
                                 "claude_app": sh_quote(claude_app)})
        os.chmod(launcher, 0o755)
        icon = os.path.join(resources, "icon.icns")
        recoloured = bool(stock) and (hue % 360 != 0 or saturation != 1.0) and recolour(stock, icon, hue, saturation)
        if stock and not os.path.isfile(icon):
            shutil.copyfile(stock, icon)
        plist = {
            "CFBundleName": "Claude " + display, "CFBundleDisplayName": "Claude " + display,
            "CFBundleIdentifier": "com.yilab.hatcheck.claude-" + name.replace("_", "-"),
            "CFBundleExecutable": "launch", "CFBundleIconFile": "icon", "CFBundlePackageType": "APPL",
            "CFBundleInfoDictionaryVersion": "6.0", "CFBundleShortVersionString": version(), "CFBundleVersion": version(),
            "LSMinimumSystemVersion": "11.0", "NSHighResolutionCapable": True,
            "HatcheckInstance": name, "HatcheckDataDir": data_dir or "",
        }
        with open(os.path.join(built, "Contents", "Info.plist"), "wb") as fh:
            plistlib.dump(plist, fh)
        if os.path.lexists(app):
            shutil.rmtree(app)
        os.makedirs(apps_dir, exist_ok=True)
        shutil.move(built, app)
    finally:
        shutil.rmtree(tmp, ignore_errors=True)
    return {"app": app, "data_dir": data_dir, "recoloured": recoloured}


def remove_instance(inst, data=False):
    """Delete an instance's launcher app, and with data its data folder."""
    shutil.rmtree(inst["app"])
    if data and inst["data_dir"] and os.path.isdir(inst["data_dir"]):
        shutil.rmtree(inst["data_dir"])
        return True
    return False


def main(argv=None, prog=None):
    parser = argparse.ArgumentParser(prog=prog, description="Claude Desktop instances on macOS: one per account, "
                                                            "each with its own data folder and a launcher of its own colour.")
    parser.add_argument("--json", action="store_true", help="list the instances as JSON")
    sub = parser.add_subparsers(dest="action")
    new = sub.add_parser("new", help="make an instance's launcher, /Applications/Claude DISPLAY.app")
    new.add_argument("name", help="the instance: letters, digits, dots, underscores and dashes; default for "
                                  "Claude's built-in data folder")
    new.add_argument("display", help="what the launcher is called, as in Claude Work.app")
    new.add_argument("--hue", type=float, default=0.0, metavar="DEG",
                     help="turn the icon's hues by this many degrees: about 110 green, 190 blue, 240 purple, -10 red")
    new.add_argument("--saturation", type=float, default=1.0, metavar="X", help="scale the icon's saturation (1.0 keeps it)")
    new.add_argument("--replace", action="store_true", help="rebuild a launcher that exists")
    opener = sub.add_parser("open", help="start an instance, or bring it forward")
    opener.add_argument("name", help="the instance's name, or its display name")
    remove = sub.add_parser("remove", help="delete an instance's launcher")
    remove.add_argument("name", help="the instance's name, or its display name")
    remove.add_argument("--data", action="store_true", help="delete its data folder too: the login and everything in it")
    args = parser.parse_args(argv)
    if sys.platform != "darwin":
        sys.exit("Claude Desktop instances are macOS only for now")

    if args.action == "new":
        try:
            made = make_instance(args.name, args.display, args.hue, args.saturation, args.replace)
        except (ValueError, FileNotFoundError, FileExistsError) as e:
            sys.exit(str(e))
        print("Made %s%s." % (made["app"], ", with a recoloured icon" if made["recoloured"] else
                                           "" if args.hue % 360 == 0 and args.saturation == 1.0 else
                                           ", with the stock icon (recolouring needs Pillow: pip install Pillow)"))
        if made["data_dir"]:
            print("Its data folder is %s: sign in there once, on first launch." % made["data_dir"])
        else:
            print("It runs Claude's built-in data folder; don't also use the stock Claude icon.")
        print("Open it from Launchpad, or with: hatcheck desktop open %s" % args.name)
        return
    if args.action == "open":
        inst = find(args.name, instances())
        # The launcher brings a running instance forward instead of starting another.
        return subprocess.call(["open", "-a", inst["app"]])
    if args.action == "remove":
        inst = find(args.name, instances())
        data_gone = remove_instance(inst, args.data)
        print("Removed %s%s." % (inst["app"], ", and its data folder" if data_gone else ""))
        if inst["data_dir"] and not data_gone and os.path.isdir(inst["data_dir"]):
            print("Its data folder, %s, is kept; --data deletes it too." % inst["data_dir"])
        return

    found = running(instances())
    if args.json:
        print(json.dumps({"version": 1, "instances": found}, ensure_ascii=False, indent=2))
        return
    if not found:
        print("No Claude Desktop instances yet. Make one with: hatcheck desktop new work Work --hue 190")
        return
    for inst in found:
        print("  %-20s %-12s %s%s%s" % (inst["display"], inst["name"], inst["app"],
                                         "  ● running" if inst["running"] else "",
                                         "  (made by the old script; --replace rebuilds it)" if inst["legacy"] else ""))


if __name__ == "__main__":
    sys.exit(main())
