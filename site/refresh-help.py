#!/usr/bin/env python3
"""Refresh the help texts index.html shows, from the hatcheck binary: each block between
<!-- help:NAME --> and <!-- /help:NAME --> becomes what `hatcheck NAME --help` prints
(hatcheck: `hatcheck --help`; version: the version), HTML-escaped. Run at a release, after
`cargo build --release`:

    python3 site/refresh-help.py [path/to/hatcheck]

Not uploaded with the page: .assetsignore names it."""
import html
import os
import re
import subprocess
import sys

here = os.path.dirname(os.path.abspath(__file__))
binary = sys.argv[1] if len(sys.argv) > 1 else os.path.join(here, "..", "target", "release", "hatcheck")
page = os.path.join(here, "index.html")
text = open(page, encoding="utf-8").read()


def printed(args):
    env = dict(os.environ, NO_COLOR="1", COLUMNS="80")  # wrapped as in an 80-column terminal
    return subprocess.run([binary] + args, capture_output=True, text=True, encoding="utf-8",
                          check=True, env=env).stdout.rstrip("\n")


def fill(match):
    name = match.group(1)
    if name == "version":
        body = printed(["--version"]).split()[-1]
    elif name == "hatcheck":
        body = printed(["--help"])
    else:
        body = printed([name, "--help"])
    return "<!-- help:%s -->%s<!-- /help:%s -->" % (name, html.escape(body, quote=False), name)


new, count = re.subn(r"<!-- help:([a-z]+) -->.*?<!-- /help:\1 -->", fill, text, flags=re.S)
if count == 0:
    sys.exit("index.html has no help blocks")
if new == text:
    print("%d help blocks already match %s" % (count, binary))
else:
    open(page, "w", encoding="utf-8").write(new)
    print("refreshed %d help blocks from %s" % (count, binary))
