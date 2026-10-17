#!/usr/bin/env python3
"""Run the tests of the subcommands the Rust port has against its binary: build it with
cargo, then run those tests with HATCHECK_ENGINE pointing at it.

    python3 tests/conformance.py [--release] [TEST ...]

A TEST names a module, class or method, such as test_usage or test_json.Json.test_usage;
without any, the ones in PORTED run. Tests of the Python code itself are skipped
(helpers.python_only), and the rest must pass for the Rust port as they pass for Python."""
import argparse
import os
import subprocess
import sys
import unittest

TESTS = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(TESTS)

# The tests of what the Rust port has: hatcheck sessions, usage and where.
PORTED = [
    "test_recent_sessions",
    "test_usage",
    "test_codex",
    "test_session_cache",
    "test_json.Json",
    "test_json.MoreJson.test_where",
]


def main():
    parser = argparse.ArgumentParser(description="Run the ported subcommands' tests against the Rust port.")
    parser.add_argument("--release", action="store_true", help="build and test the release binary")
    parser.add_argument("tests", nargs="*", help="tests to run (default: the ported ones)")
    args = parser.parse_args()
    profile = "release" if args.release else "debug"
    build = ["cargo", "build", "-p", "hatcheck"] + (["--release"] if args.release else [])
    if subprocess.run(build, cwd=REPO).returncode != 0:
        return 1
    engine = os.path.join(REPO, "target", profile, "hatcheck" + (".exe" if os.name == "nt" else ""))
    os.environ["HATCHECK_ENGINE"] = engine  # before helpers is imported, which reads it
    sys.path.insert(0, TESTS)
    suite = unittest.defaultTestLoader.loadTestsFromNames(args.tests or PORTED)
    result = unittest.TextTestRunner(verbosity=1).run(suite)
    return 0 if result.wasSuccessful() else 1


if __name__ == "__main__":
    sys.exit(main())
