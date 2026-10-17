"""The cache of what hatcheck reads from session files: a repeat run lists the same, a file
that grew is read only from where the last read stopped, and a file that was rewritten or
replaced, or a cache that can't be read, is read again from the start."""
import json
import os
import sys
import unittest

from helpers import ENGINE, NOW, WINDOWS, FakeHome, iso, load, python_only, rec, write

S1 = "11111111-1111-4111-8111-111111111111"
X = "019f0000-aaaa-7000-8000-000000000001"


class SessionCache(unittest.TestCase):
    def setUp(self):
        self.h = h = FakeHome()
        self.addCleanup(h.cleanup)
        self.profile = h.profile("work", "acct-1")
        self.cwd = h.folder("code", "api")
        self.path = h.session(self.profile, self.cwd, S1, "Fix the uploader")

    def listed(self):
        code, out = self.h.run("sessions", "--json", "--no-usage", "--all-folders")
        self.assertEqual(code, 0, out)
        return json.loads(out)["sessions"]

    def cache_file(self):
        if WINDOWS:
            folder = os.path.join(self.h.home, "AppData", "Local", "hatcheck", "Cache")
        elif sys.platform == "darwin":
            folder = os.path.join(self.h.home, "Library", "Caches", "hatcheck")
        else:
            folder = os.path.join(self.h.home, ".cache", "hatcheck")
        # The Rust port keeps its own cache file beside the Python one.
        return os.path.join(folder, "session-facts.json" if ENGINE else "sessions.json")

    def append(self, *lines):
        with open(self.path, "a", encoding="utf-8", newline="\n") as fh:
            fh.write("".join(line + "\n" for line in lines))

    def activity(self, minutes_ago, text, branch="main"):
        return rec(type="user", timestamp=iso(NOW - minutes_ago * 60), cwd=self.cwd, sessionId=S1,
                   entrypoint="cli", gitBranch=branch, message={"role": "user", "content": text})

    def test_a_repeat_run_lists_the_same(self):
        first = self.listed()
        with open(self.cache_file(), encoding="utf-8") as fh:
            self.assertIn(os.path.abspath(self.path), json.load(fh)["files"])
        self.assertEqual(self.listed(), first)
        self.assertEqual(first[0]["title"], "Fix the uploader")

    def test_what_was_appended_shows(self):
        self.listed()
        self.append(rec(type="custom-title", customTitle="Retry uploads", sessionId=S1), self.activity(1, "go on", "fix"))
        s, = self.listed()
        self.assertEqual((s["title"], s["branch"], s["last_active"]), ("Retry uploads", "fix", iso(NOW - 60)[:19] + "Z"))

    @python_only
    def test_only_what_was_appended_is_read(self):
        s = load("sessions")
        self.assertEqual(s.read_session(self.path)["title"], "Fix the uploader")
        size = os.path.getsize(self.path)
        self.append(rec(type="custom-title", customTitle="Retry uploads", sessionId=S1), self.activity(1, "go on"))
        starts, real = [], s.last_fields
        s.last_fields = lambda mm, stop=0: starts.append(stop) or real(mm, stop)
        self.assertEqual(s.read_session(self.path)["title"], "Retry uploads")
        self.assertEqual(starts, [size])
        self.assertEqual(s.read_session(self.path)["title"], "Retry uploads")
        self.assertEqual(starts, [size])  # unchanged since: not read at all

    def test_a_line_still_being_written(self):
        self.append(self.activity(2, "more"))
        line = rec(type="custom-title", customTitle="Half written", sessionId=S1)
        with open(self.path, "a", encoding="utf-8", newline="\n") as fh:
            fh.write(line[:20])
        self.assertEqual(self.listed()[0]["title"], "Fix the uploader")
        with open(self.path, "a", encoding="utf-8", newline="\n") as fh:
            fh.write(line[20:] + "\n")
        self.assertEqual(self.listed()[0]["title"], "Half written")

    def test_rewritten_with_its_size_and_time_put_back(self):
        self.listed()
        st = os.stat(self.path)
        with open(self.path, encoding="utf-8") as fh:
            text = fh.read()
        write(self.path, text.replace("Fix the uploader", "Fix the importer"))  # the same length
        os.utime(self.path, ns=(st.st_atime_ns, st.st_mtime_ns))
        self.assertEqual(os.path.getsize(self.path), st.st_size)
        self.assertEqual(self.listed()[0]["title"], "Fix the importer")

    def test_replaced_by_another_file(self):
        self.listed()
        other = self.h.session(self.h.profile("b", "acct-2"), self.cwd, S1, "Something else entirely, and longer")
        with open(other, encoding="utf-8") as fh:
            text = fh.read()
        write(self.path + ".tmp", text)
        os.replace(self.path + ".tmp", self.path)
        titles = sorted(s["title"] for s in self.listed())
        self.assertEqual(titles, ["Something else entirely, and longer"] * 2)

    def test_a_cache_that_cant_be_read(self):
        self.listed()
        write(self.cache_file(), "{not json")
        self.assertEqual(self.listed()[0]["title"], "Fix the uploader")
        with open(self.cache_file(), encoding="utf-8") as fh:
            self.assertEqual(json.load(fh)["version"], 1)

    def test_codex(self):
        home = self.h.codex_home("work", "acct-9")
        path = self.h.codex_session(home, self.cwd, X, "Speed up the build", minutes_ago=30)
        codex = lambda: next(s for s in self.listed() if s["agent"] == "codex")  # noqa: E731
        self.assertEqual((codex()["model"], codex()["title"]), ("gpt-5.5", "Speed up the build"))
        with open(path, "a", encoding="utf-8", newline="\n") as fh:
            fh.write(rec(timestamp=iso(NOW - 120), type="turn_context", payload={"cwd": self.cwd, "model": "gpt-6"}) + "\n")
            fh.write(rec(timestamp=iso(NOW - 60), type="event_msg", payload={"type": "user_message", "message": "more"}) + "\n")
        s = codex()
        self.assertEqual((s["model"], s["title"], s["last_active"]), ("gpt-6", "Speed up the build", iso(NOW - 60)[:19] + "Z"))


if __name__ == "__main__":
    unittest.main()
