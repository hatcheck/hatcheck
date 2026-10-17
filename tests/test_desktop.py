"""hatcheck desktop: the launcher apps it makes for Claude Desktop instances, how it lists
them (ours and the old script's), and how it removes them. Against a throwaway folder with a
stand-in Claude.app; nothing is launched."""
import os
import plistlib
import shutil
import subprocess
import sys
import tempfile
import unittest

from helpers import WINDOWS, FakeHome, load, python_only, write

MAC = sys.platform == "darwin"


def pillow():
    try:
        import PIL  # noqa: F401
        return True
    except ImportError:
        return False


@python_only
class Desktop(unittest.TestCase):
    def setUp(self):
        self.h = h = FakeHome()
        self.addCleanup(h.cleanup)
        self.d = load("desktop")
        self.apps = h.folder("Applications")
        self.claude = os.path.join(self.apps, "Claude.app")
        self.instances_dir = os.path.join(h.home, ".claude-instances")
        with open(self.plist_path(self.claude, make=True), "wb") as fh:
            plistlib.dump({"CFBundleIconFile": "electron.icns", "CFBundleExecutable": "Claude"}, fh)
        write(os.path.join(self.claude, "Contents", "Resources", "electron.icns"), "not really an icon\n")

    def plist_path(self, app, make=False):
        if make:
            os.makedirs(os.path.join(app, "Contents"), exist_ok=True)
        return os.path.join(app, "Contents", "Info.plist")

    def make(self, name, display, **kw):
        kw.setdefault("apps_dir", self.apps)
        kw.setdefault("claude_app", self.claude)
        kw.setdefault("instances_dir", self.instances_dir)
        return self.d.make_instance(name, display, **kw)

    def test_make_a_launcher(self):
        made = self.make("work", "Work")
        app = os.path.join(self.apps, "Claude Work.app")
        self.assertEqual(made, {"app": app, "data_dir": os.path.join(self.instances_dir, "work"), "recoloured": False})
        with open(self.plist_path(app), "rb") as fh:
            plist = plistlib.load(fh)
        self.assertEqual((plist["CFBundleExecutable"], plist["CFBundleIconFile"], plist["CFBundleIdentifier"]),
                         ("launch", "icon", "com.yilab.hatcheck.claude-work"))
        self.assertEqual((plist["HatcheckInstance"], plist["HatcheckDataDir"], plist["CFBundleName"]),
                         ("work", os.path.join(self.instances_dir, "work"), "Claude Work"))
        launcher = os.path.join(app, "Contents", "MacOS", "launch")
        with open(launcher, encoding="utf-8") as fh:
            script = fh.read()
        self.assertIn("DATA_DIR='%s'" % os.path.join(self.instances_dir, "work"), script)
        self.assertIn("CLAUDE_APP='%s'" % self.claude, script)
        self.assertIn("open -n -a", script)
        self.assertIn("first process whose unix id is", script)
        if not WINDOWS:
            self.assertTrue(os.access(launcher, os.X_OK))
            self.assertEqual(subprocess.run(["sh", "-n", launcher], capture_output=True).returncode, 0)
            if shutil.which("shellcheck"):
                p = subprocess.run(["shellcheck", launcher], capture_output=True, text=True)
                self.assertEqual(p.returncode, 0, p.stdout)
        # The stand-in icon can't be taken apart, so the stock one is used as it is.
        with open(os.path.join(app, "Contents", "Resources", "icon.icns"), encoding="utf-8") as fh:
            self.assertEqual(fh.read(), "not really an icon\n")
        self.assertFalse(os.path.exists(os.path.join(self.instances_dir, "work")))  # made on first launch

    def test_default_and_names(self):
        made = self.make("default", "Default")
        self.assertIsNone(made["data_dir"])
        with open(os.path.join(made["app"], "Contents", "MacOS", "launch"), encoding="utf-8") as fh:
            self.assertIn('DATA_DIR=""', fh.read())
        for name, display in (("bad name", "X"), ("a/b", "X"), ("ok", ""), ("ok", "a/b"), ("ok", ".hidden")):
            with self.assertRaises(ValueError):
                self.make(name, display)
        with self.assertRaises(FileNotFoundError):
            self.make("x", "X", claude_app=os.path.join(self.apps, "Nope.app"))

    def test_replace_list_and_remove(self):
        self.make("work", "Work")
        with self.assertRaises(FileExistsError):
            self.make("work", "Work")
        self.make("work", "Work", replace=True)
        # One the old script made, and an app that merely starts with Claude.
        old = os.path.join(self.apps, "Claude Old.app")
        write(os.path.join(old, "Contents", "MacOS", "config.sh"), 'INSTANCE_NAME="old"\n')
        write(os.path.join(old, "Contents", "Info.plist"), "<plist/>")
        write(os.path.join(self.apps, "Claude Code URL Handler.app", "Contents", "Info.plist"), "<plist/>")
        found = self.d.instances([self.apps], self.instances_dir)
        self.assertEqual([(i["name"], i["display"], i["legacy"]) for i in found],
                         [("old", "Old", True), ("work", "Work", False)])
        self.assertEqual(found[0]["data_dir"], os.path.join(self.instances_dir, "old"))
        os.makedirs(os.path.join(self.instances_dir, "work"))
        self.assertFalse(self.d.remove_instance(found[1]))
        self.assertFalse(os.path.exists(found[1]["app"]))
        self.assertTrue(os.path.isdir(os.path.join(self.instances_dir, "work")))  # kept without --data
        made = self.make("work", "Work")
        inst = next(i for i in self.d.instances([self.apps], self.instances_dir) if i["name"] == "work")
        self.assertTrue(self.d.remove_instance(inst, data=True))
        self.assertFalse(os.path.exists(made["app"]))
        self.assertFalse(os.path.exists(os.path.join(self.instances_dir, "work")))

    @unittest.skipUnless(MAC and shutil.which("iconutil") and pillow(), "needs macOS's iconutil and Pillow")
    def test_recoloured_icon(self):
        from PIL import Image
        with tempfile.TemporaryDirectory() as tmp:
            iconset = os.path.join(tmp, "icon.iconset")
            os.makedirs(iconset)
            for size in (16, 32, 128, 256, 512):
                Image.new("RGBA", (size, size), (230, 120, 40, 255)).save(os.path.join(iconset, "icon_%dx%d.png" % (size, size)))
            icns = os.path.join(self.claude, "Contents", "Resources", "electron.icns")
            p = subprocess.run(["iconutil", "-c", "icns", iconset, "-o", icns], capture_output=True, text=True)
            self.assertEqual(p.returncode, 0, p.stderr)
        made = self.make("blue", "Blue", hue=190)
        self.assertTrue(made["recoloured"])
        icon = os.path.join(made["app"], "Contents", "Resources", "icon.icns")
        with open(icon, "rb") as fh, open(icns, "rb") as stock:
            self.assertNotEqual(fh.read(), stock.read())
        with tempfile.TemporaryDirectory() as tmp:
            out = os.path.join(tmp, "out.iconset")
            subprocess.run(["iconutil", "-c", "iconset", icon, "-o", out], check=True)
            r, g, b, a = Image.open(os.path.join(out, "icon_32x32.png")).convert("RGBA").getpixel((5, 5))
            self.assertGreater(b, r)  # orange turned by 190 degrees: blue
            self.assertEqual(a, 255)
        self.assertFalse(self.make("plain", "Plain")["recoloured"])  # no turn asked for


if __name__ == "__main__":
    unittest.main()
