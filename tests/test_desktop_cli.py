"""hatcheck desktop from the command line, against a throwaway home and a folder standing in
for /Applications (HATCHECK_APPS_DIR) with a stand-in Claude.app (HATCHECK_CLAUDE_APP), so it
runs for the Rust port as for Python. Nothing is launched."""
import json
import os
import plistlib
import shutil
import struct
import subprocess
import sys
import tempfile
import unittest
import zlib

from helpers import ENGINE, FakeHome, argv_of, write

MAC = sys.platform == "darwin"


def pillow():
    try:
        import PIL  # noqa: F401
        return True
    except ImportError:
        return False


def write_png(path, size, rgba):
    """A PNG of one colour, RGBA, written by hand so the test needs no Pillow."""
    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF)
    rows = b"".join(b"\x00" + bytes(rgba) * size for _ in range(size))
    data = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0))
    data += chunk(b"IDAT", zlib.compress(rows)) + chunk(b"IEND", b"")
    with open(path, "wb") as fh:
        fh.write(data)


def png_pixel(path, x, y):
    """The RGBA of one pixel of an 8-bit RGB or RGBA PNG, read by hand."""
    with open(path, "rb") as fh:
        data = fh.read()
    pos, idat, width, kind = 8, b"", 0, 6
    while pos < len(data):
        length, name = struct.unpack(">I4s", data[pos:pos + 8])
        body = data[pos + 8:pos + 8 + length]
        if name == b"IHDR":
            width, _, depth, kind = struct.unpack(">IIBB", body[:10])
            assert depth == 8 and kind in (2, 6), (depth, kind)
        elif name == b"IDAT":
            idat += body
        pos += 12 + length
    channels = 4 if kind == 6 else 3
    stride = width * channels
    raw, rows, prev = zlib.decompress(idat), [], bytearray(stride)
    for r in range(y + 1):
        line = raw[r * (stride + 1):(r + 1) * (stride + 1)]
        f, cur = line[0], bytearray(line[1:])
        for i in range(stride):
            a = cur[i - channels] if i >= channels else 0
            b, c = prev[i], prev[i - channels] if i >= channels else 0
            if f == 1:
                cur[i] = (cur[i] + a) & 255
            elif f == 2:
                cur[i] = (cur[i] + b) & 255
            elif f == 3:
                cur[i] = (cur[i] + (a + b) // 2) & 255
            elif f == 4:
                p = a + b - c
                pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                cur[i] = (cur[i] + (a if pa <= pb and pa <= pc else b if pb <= pc else c)) & 255
        rows.append(cur)
        prev = cur
    px = rows[y][x * channels:(x + 1) * channels]
    return tuple(px) + ((255,) if channels == 3 else ())


class DesktopCommand(unittest.TestCase):
    def setUp(self):
        self.h = h = FakeHome()
        self.addCleanup(h.cleanup)
        self.apps = h.folder("Apps")  # stands in for /Applications
        self.claude = os.path.join(self.apps, "Claude.app")
        os.makedirs(os.path.join(self.claude, "Contents", "Resources"))
        with open(os.path.join(self.claude, "Contents", "Info.plist"), "wb") as fh:
            plistlib.dump({"CFBundleIconFile": "electron.icns", "CFBundleExecutable": "Claude"}, fh)
        write(os.path.join(self.claude, "Contents", "Resources", "electron.icns"), "not really an icon\n")
        self.instances = os.path.join(h.home, ".claude-instances")

    def desktop(self, *args, claude=None):
        env = dict(self.h.env(), HATCHECK_APPS_DIR=self.apps, HATCHECK_CLAUDE_APP=claude or self.claude)
        if MAC and shutil.which("iconutil", path="/usr/bin"):
            # iconutil without the rest of /usr/bin, which has the security tool the Keychain answers.
            tools = os.path.join(self.h.root, "tools")
            if not os.path.exists(tools):
                os.makedirs(tools)
                os.symlink("/usr/bin/iconutil", os.path.join(tools, "iconutil"))
            env["PATH"] = env["PATH"] + os.pathsep + tools
        p = subprocess.run(argv_of("desktop") + list(args), env=env, cwd=self.h.root, capture_output=True, text=True,
                           encoding="utf-8")
        return p.returncode, p.stdout + p.stderr

    @unittest.skipIf(MAC, "macOS has them")
    def test_macos_only(self):
        self.assertEqual(self.desktop(), (1, "Claude Desktop instances are macOS only for now\n"))

    @unittest.skipUnless(MAC, "macOS only")
    def test_make_list_and_remove(self):
        code, out = self.desktop("new", "work", "Work")
        app, data = os.path.join(self.apps, "Claude Work.app"), os.path.join(self.instances, "work")
        self.assertEqual((code, out), (0, "Made %s.\nIts data folder is %s: sign in there once, on first launch.\n"
                                          "Open it from Launchpad, or with: hatcheck desktop open work\n" % (app, data)))
        with open(os.path.join(app, "Contents", "Info.plist"), "rb") as fh:
            plist = plistlib.load(fh)
        self.assertEqual({k: plist[k] for k in ("CFBundleIdentifier", "HatcheckInstance", "HatcheckDataDir", "CFBundleName",
                                                 "CFBundleExecutable", "CFBundleIconFile", "NSHighResolutionCapable")},
                         {"CFBundleIdentifier": "dev.hatcheck.hatcheck.claude-work", "HatcheckInstance": "work",
                          "HatcheckDataDir": data, "CFBundleName": "Claude Work", "CFBundleExecutable": "launch",
                          "CFBundleIconFile": "icon", "NSHighResolutionCapable": True})
        launcher = os.path.join(app, "Contents", "MacOS", "launch")
        with open(launcher, encoding="utf-8") as fh:
            script = fh.read()
        self.assertIn("DATA_DIR='%s'\nCLAUDE_APP='%s'\n" % (data, self.claude), script)
        self.assertTrue(os.access(launcher, os.X_OK))
        self.assertEqual(subprocess.run(["sh", "-n", launcher]).returncode, 0)
        with open(os.path.join(app, "Contents", "Resources", "icon.icns"), encoding="utf-8") as fh:
            self.assertEqual(fh.read(), "not really an icon\n")  # can't be taken apart: the stock one as it is
        code, out = self.desktop("--json")
        self.assertEqual(json.loads(out), {"version": 1, "instances": [
            {"name": "work", "display": "Work", "app": app, "data_dir": data, "legacy": False, "running": False}]})
        self.assertEqual(self.desktop(), (0, "  %-20s %-12s %s\n" % ("Work", "work", app)))
        self.assertEqual(self.desktop("new", "work", "Work"), (1, "%s exists already; --replace rebuilds it\n" % app))
        self.assertEqual(self.desktop("new", "work", "Work", "--replace")[0], 0)
        old = os.path.join(self.apps, "Claude Old.app")
        write(os.path.join(old, "Contents", "MacOS", "config.sh"), 'INSTANCE_NAME="old"\n')
        write(os.path.join(old, "Contents", "Info.plist"), "<plist/>")
        write(os.path.join(self.apps, "Claude Code URL Handler.app", "Contents", "Info.plist"), "<plist/>")
        code, out = self.desktop("--json")
        self.assertEqual([(i["name"], i["display"], i["legacy"], i["data_dir"]) for i in json.loads(out)["instances"]],
                         [("old", "Old", True, os.path.join(self.instances, "old")), ("work", "Work", False, data)])
        self.assertIn("(made by the old script; --replace rebuilds it)", self.desktop()[1])
        os.makedirs(data)
        self.assertEqual(self.desktop("remove", "work"),
                         (0, "Removed %s.\nIts data folder, %s, is kept; --data deletes it too.\n" % (app, data)))
        self.assertTrue(os.path.isdir(data))
        self.desktop("new", "work", "Work")
        self.assertEqual(self.desktop("remove", "Work", "--data"), (0, "Removed %s, and its data folder.\n" % app))
        self.assertFalse(os.path.exists(data))
        self.assertEqual(self.desktop("open", "nosuch"), (1, "No Claude Desktop instance named nosuch; hatcheck desktop lists them\n"))

    @unittest.skipUnless(MAC, "macOS only")
    def test_names_and_the_default_instance(self):
        for args, message in ((("bad name", "X"), "The name can have letters, digits, dots, underscores and dashes: not 'bad name'"),
                              (("ok", " "), "The display name can't be empty, start with a dot or hold a slash: not ''"),
                              (("ok", ".x"), "The display name can't be empty, start with a dot or hold a slash: not '.x'")):
            self.assertEqual(self.desktop("new", *args), (1, message + "\n"))
        missing = os.path.join(self.apps, "Nope.app")
        self.assertEqual(self.desktop("new", "x", "X", claude=missing),
                         (1, "Claude Desktop isn't at %s; install it from https://claude.ai/download\n" % missing))
        code, out = self.desktop("new", "default", "Default")
        self.assertIn("It runs Claude's built-in data folder", out)
        with open(os.path.join(self.apps, "Claude Default.app", "Contents", "MacOS", "launch"), encoding="utf-8") as fh:
            self.assertIn('DATA_DIR=""', fh.read())
        self.assertIn("argument --hue: invalid float value: 'abc'", self.desktop("new", "x", "X", "--hue", "abc")[1])

    @unittest.skipUnless(MAC and shutil.which("iconutil"), "needs macOS's iconutil")
    @unittest.skipUnless(ENGINE or pillow(), "Python's hatcheck needs Pillow to recolour")
    def test_recoloured_icon(self):
        with tempfile.TemporaryDirectory() as tmp:
            iconset = os.path.join(tmp, "icon.iconset")
            os.makedirs(iconset)
            for size in (16, 32, 128, 256, 512):
                write_png(os.path.join(iconset, "icon_%dx%d.png" % (size, size)), size, (230, 120, 40, 255))
            stock = os.path.join(self.claude, "Contents", "Resources", "electron.icns")
            p = subprocess.run(["iconutil", "-c", "icns", iconset, "-o", stock], capture_output=True, text=True)
            self.assertEqual(p.returncode, 0, p.stderr)
        code, out = self.desktop("new", "blue", "Blue", "--hue", "190")
        self.assertEqual(code, 0, out)
        self.assertIn("with a recoloured icon", out)
        icon = os.path.join(self.apps, "Claude Blue.app", "Contents", "Resources", "icon.icns")
        with tempfile.TemporaryDirectory() as tmp:
            out_set = os.path.join(tmp, "out.iconset")
            subprocess.run(["iconutil", "-c", "iconset", icon, "-o", out_set], check=True)
            # Orange turned 190 degrees, by Pillow's arithmetic: this blue, exactly.
            self.assertEqual(png_pixel(os.path.join(out_set, "icon_32x32.png"), 5, 5), (41, 121, 230, 255))


if __name__ == "__main__":
    unittest.main()
