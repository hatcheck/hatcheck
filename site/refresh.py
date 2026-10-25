#!/usr/bin/env python3
"""Refresh what the pages show of hatcheck's own output, from the built binary. Run at a
release, after `cargo build --release`:

    python3 site/refresh.py [path/to/hatcheck]

commands.html: each block between <!-- help:NAME --> and <!-- /help:NAME --> becomes what
`hatcheck NAME --help` prints (hatcheck: `hatcheck --help`; version: the version), HTML-escaped,
wrapped as in an 80-column terminal.

index.html: each block between <!-- sample:NAME --> and <!-- /sample:NAME --> becomes a command
run in a demo home made with tests/helpers.py, never your profiles: `list` is `hatcheck` in a
folder with sessions from every agent, `best` is `hatcheck best` there, and `relocate` is
`hatcheck relocate` once ~/code/web, with sessions in two profiles, a Codex home, a Kimi home, a DSH home and
an mcode home, is renamed ~/code/site; the demo home's path reads /Users/me wherever it's printed in full, as
in the project folders' names. They
run on a
pseudo-terminal with 256 colours, so the output is coloured as in a terminal; its colour codes
become spans (classes b, d, r, g, y, bl, m, c for bold, dim and the basic colours, and a colour
for the 256-colour ones), styled in style.css. A fake claude answers the usage questions from
files, so nothing is asked over the network, and the snapshots it saves are backdated between
runs, so each sample reads live. The list's lines after the sessions (the profiles, where usage
came from) are left out.

Not uploaded with the pages: .assetsignore names it."""
import html
import os
import re
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(HERE)
BINARY = sys.argv[1] if len(sys.argv) > 1 else os.path.join(REPO, "target", "release", "hatcheck")


def printed(args):
    env = dict(os.environ, NO_COLOR="1", COLUMNS="80")
    return subprocess.run([BINARY] + args, capture_output=True, text=True, encoding="utf-8",
                          check=True, env=env).stdout.rstrip("\n")


def help_text(name):
    if name == "version":
        return printed(["--version"]).split()[-1]
    if name == "hatcheck":
        return printed(["--help"])
    return printed([name, "--help"])


# ---- the demo home, and the commands run in it on a terminal ----

def demo_home():
    """A home with sessions from every agent in ~/code/api: two Claude Code profiles with usage a
    fake claude answers, a Codex home with limits in its session, and Antigravity CLI's folder, a
    Kimi home, ZCode's folder, a DSH home and an mcode home with no usage, as when their agents aren't on the PATH. The
    snapshots are shaped as Claude Code saves them: a session limit and a Fable one for the team
    plan, a session limit and the week's for the Pro plan. ~/.claude-a is the Pro one, nearly out
    for the week, with the newest session, and ~/.claude-b the team one, so hatcheck best has
    somewhere better to point."""
    os.environ["HATCHECK_ENGINE"] = BINARY  # or the helpers build a debug engine
    sys.path.insert(0, os.path.join(REPO, "tests"))
    from helpers import NOW, FakeHome, iso, write

    sev = lambda used: "critical" if used >= 90 else "warning" if used >= 75 else "normal"

    def session(used):
        return {"kind": "session", "group": "session", "percent": used, "severity": sev(used),
                "resets_at": iso(NOW + 2 * 3600 + 40 * 60)}

    def week(used, days):
        return {"kind": "weekly_all", "group": "weekly", "percent": used, "severity": sev(used),
                "resets_at": iso(NOW + days * 86400)}

    def fable(used, days):
        return {"kind": "weekly_scoped", "group": "weekly", "percent": used, "severity": sev(used),
                "resets_at": iso(NOW + days * 86400), "scope": {"model": {"display_name": "Fable"}}}

    team_plan = {"limits": [session(38), fable(52, 4.2)]}
    pro_plan = {"limits": [session(80), week(91, 0.9)]}
    h = FakeHome()
    profile_b = h.profile("b", "acct-b", usage=team_plan, saved_at=NOW - 3600,
                     plan={"organizationType": "claude_team", "userRateLimitTier": "default_claude_max_5x"})
    profile_a = h.profile("a", "acct-a", usage=pro_plan, saved_at=NOW - 3600,
                     plan={"organizationType": "claude_pro"})
    write(os.path.join(profile_b, "fake-usage.json"), __import__("json").dumps(team_plan))
    write(os.path.join(profile_a, "fake-usage.json"), __import__("json").dumps(pro_plan))
    api = h.folder("code", "api")
    h.session(profile_a, api, "3f1c2a9e-8b7d-4c1e-9f2a-6d5e4c3b2a10", "Fix flaky login test", 4)
    h.session(profile_b, api, "9a8b7c6d-5e4f-4a3b-8c2d-1e0f9a8b7c6d", "Add rate limiting to the login endpoint", 35)
    codex = h.codex_home("default", "acct-codex")
    h.codex_session(codex, api, "019a8c1e-7d3f-7b21-9c4e-2f1a0b9c8d7e", "Add retries to the uploader", 50,
                    rate_limits={"limit_id": "codex", "limit_name": None, "plan_type": "plus",
                                 "primary": {"used_percent": 20, "window_minutes": 300, "resets_at": int(NOW + 3 * 3600)},
                                 "secondary": {"used_percent": 36, "window_minutes": 10080, "resets_at": int(NOW + 2 * 86400)},
                                 "credits": {"has_credits": False, "unlimited": False, "balance": None}})
    agy = h.agy_root()
    h.agy_session(agy, api, "c134893c-20f5-48ea-bc9b-7f7c9271bc4e", "Profile the slow search query", 70)
    kimi = h.kimi_home("default")
    h.kimi_session(kimi, api, "6d2f1b0a-3c4e-4f5a-9b8c-7d6e5f4a3b2c", "Write the migration for the audit table", 130)
    zcode = h.zcode_root()
    h.zcode_session(zcode, api, "1fb068a4-b6d4-4a9f-b2d3-6e428e094fbe", "Tidy the uploader's error handling", 200)
    dsh = h.dsh_home()
    h.dsh_session(dsh, api, "d54e0009-0000-4000-8000-000000000009", "Trace the missing webhook", 240)
    mcode = h.mcode_home()
    h.mcode_session(mcode, api, "c54e0009000040008000000000000009", "Review the retry backoff", 280)
    path = h.fake_claude() + os.pathsep + "/bin" + os.pathsep + "/usr/bin"
    env = dict(h.env(), PATH=path, TERM="xterm-256color", COLUMNS="150")
    env.pop("NO_COLOR", None)
    return h, api, env


def backdate(home):
    """Make each profile's saved usage an hour old, so the next run asks the fake claude again
    rather than show a reading under a minute old as cached."""
    import glob
    import json
    for path in glob.glob(os.path.join(home, ".claude*", ".claude.json")) + [os.path.join(home, ".claude.json")]:
        if not os.path.isfile(path):
            continue
        with open(path, encoding="utf-8") as fh:
            data = json.load(fh)
        if "cachedUsageUtilization" in data:
            data["cachedUsageUtilization"]["fetchedAtMs"] -= 3600 * 1000
            with open(path, "w", encoding="utf-8") as fh:
                json.dump(data, fh)


def on_terminal(argv, cwd, env):
    """What argv prints to a 256-colour terminal 150 columns wide, with its stderr dropped."""
    import fcntl
    import pty
    import struct
    import termios
    master, slave = pty.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 60, 150, 0, 0))
    p = subprocess.Popen(argv, cwd=cwd, env=env, stdin=subprocess.DEVNULL, stdout=slave, stderr=subprocess.DEVNULL)
    os.close(slave)
    out = b""
    while True:
        try:
            chunk = os.read(master, 65536)
        except OSError:  # the child has closed its end
            break
        if not chunk:
            break
        out += chunk
    p.wait()
    os.close(master)
    if p.returncode != 0:
        sys.exit("%s exited with %d" % (" ".join(argv), p.returncode))
    return out.decode("utf-8", "replace").replace("\r\n", "\n").rstrip("\n")


BASIC = {31: "r", 32: "g", 33: "y", 34: "bl", 35: "m", 36: "c", 91: "r", 92: "g", 93: "y", 94: "bl", 95: "m", 96: "c"}


def x256(n):
    """An xterm 256-colour index as #rrggbb."""
    if n < 16:
        return ["#000000", "#cd3131", "#0dbc79", "#e5e510", "#2472c8", "#bc3fbc", "#11a8cd", "#e5e5e5",
                "#666666", "#f14c4c", "#23d18b", "#f5f543", "#3b8eea", "#d670d6", "#29b8db", "#ffffff"][n]
    if n < 232:
        n -= 16
        parts = (n // 36, n // 6 % 6, n % 6)
        return "#%02x%02x%02x" % tuple(0 if v == 0 else 55 + v * 40 for v in parts)
    v = 8 + (n - 232) * 10
    return "#%02x%02x%02x" % (v, v, v)


def ansi_to_html(text):
    """The text's SGR colour codes as spans: bold and dim as classes b and d, the basic colours
    as classes, a 256-colour one as an inline colour."""
    out, bold, dim, fg = [], False, False, None
    pos = 0
    for m in re.finditer(r"\x1b\[([0-9;]*)m", text):
        chunk = text[pos:m.start()]
        pos = m.end()
        if chunk:
            classes = [c for c, on in (("b", bold), ("d", dim)) if on]
            style = ""
            if isinstance(fg, int):
                style = ' style="color:%s"' % x256(fg)
            elif fg:
                classes.append(fg)
            escaped = html.escape(chunk, quote=False)
            if classes or style:
                out.append('<span%s%s>%s</span>' % (' class="%s"' % " ".join(classes) if classes else "", style, escaped))
            else:
                out.append(escaped)
        codes = [int(c) for c in m.group(1).split(";") if c] or [0]
        i = 0
        while i < len(codes):
            c = codes[i]
            if c == 0:
                bold, dim, fg = False, False, None
            elif c == 1:
                bold = True
            elif c == 2:
                dim = True
            elif c == 22:
                bold = dim = False
            elif c == 38 and i + 2 < len(codes) and codes[i + 1] == 5:
                fg = codes[i + 2]
                i += 2
            elif c == 39:
                fg = None
            elif c in BASIC:
                fg = BASIC[c]
            i += 1
    out.append(html.escape(text[pos:], quote=False))
    return "".join(out)


def sample(name, home):
    h, api, env = home
    backdate(h.home)
    if name == "list":
        text = on_terminal([BINARY], api, env)
        lines = text.split("\n")
        plain = lambda line: re.sub(r"\x1b\[[0-9;]*m", "", line)
        cut = next((k for k, line in enumerate(lines)
                    if re.match(r"Profiles:|Usage from|No usage data|\d+ in other folders|Hidden:", plain(line))), len(lines))
        text = "\n".join(lines[:cut]).rstrip("\n")
        return "$ hatcheck\n" + ansi_to_html(text)
    if name == "best":
        return "$ hatcheck best\n" + ansi_to_html(on_terminal([BINARY, "best"], api, env))
    if name == "relocate":
        return relocate_sample(h, env)
    sys.exit("no such sample: " + name)


def relocate_sample(h, env):
    """A project folder renamed, and hatcheck relocate bringing its sessions along: two in the
    b profile, one in a, one in Codex, and one in Kimi, one in DSH and one in mcode, which stay;
    the folder is bound to b, so the binding moves too."""
    profile_b, profile_a = os.path.join(h.home, ".claude-b"), os.path.join(h.home, ".claude-a")
    web = h.folder("code", "web")
    h.session(profile_b, web, "5a1e2b3c-4d5e-4f60-8a7b-9c0d1e2f3a4b", "Move the checkout page to the new API", 300)
    h.session(profile_b, web, "6b2f3c4d-5e6f-4071-9b8c-0d1e2f3a4b5c", "Fix the date picker on Safari", 240)
    h.session(profile_a, web, "7c3a4d5e-6f70-4182-8c9d-1e2f3a4b5c6d", "Try the new image pipeline", 200)
    h.codex_session(os.path.join(h.home, ".codex"), web, "019a8c1e-aaaa-7b21-9c4e-2f1a0b9c8d7e", "Speed up the build", 180)
    h.kimi_session(os.path.join(h.home, ".kimi-code"), web, "8d4b5e6f-7081-4293-9dae-2f3a4b5c6d7e", "Write the store's README", 150)
    h.dsh_session(os.path.join(h.home, ".dsh"), web, "d54e0010-0000-4000-8000-000000000010", "Check the webhook log", 140)
    h.mcode_session(os.path.join(h.home, ".minimax"), web, "c54e0010000040008000000000000010", "Check the retry log", 140)
    subprocess.run([BINARY, "bind", "b"], cwd=web, env=env, capture_output=True, check=True)
    site = os.path.join(h.home, "code", "site")
    os.rename(web, site)
    text = on_terminal([BINARY, "relocate", web, site], h.home, env)
    for home in sorted({h.home, os.path.realpath(h.home)}, key=len, reverse=True):
        text = text.replace(re.sub(r"[^A-Za-z0-9]", "-", home), "-Users-me").replace(home, "/Users/me")
    return "$ mv ~/code/web ~/code/site\n$ hatcheck relocate ~/code/web ~/code/site\n" + ansi_to_html(text)


def refresh(page, kind, fill):
    text = open(page, encoding="utf-8").read()
    pattern = r"<!-- %s:([a-z]+) -->.*?<!-- /%s:\1 -->" % (kind, kind)
    new, count = re.subn(pattern, lambda m: "<!-- %s:%s -->%s<!-- /%s:%s -->" % (kind, m.group(1), fill(m.group(1)), kind, m.group(1)), text, flags=re.S)
    if count == 0:
        sys.exit("%s has no %s blocks" % (os.path.basename(page), kind))
    if new == text:
        print("%d %s blocks in %s already match" % (count, kind, os.path.basename(page)))
    else:
        open(page, "w", encoding="utf-8").write(new)
        print("refreshed %d %s blocks in %s" % (count, kind, os.path.basename(page)))


if __name__ == "__main__":
    refresh(os.path.join(HERE, "commands.html"), "help", lambda name: html.escape(help_text(name), quote=False))
    home = demo_home()
    try:
        refresh(os.path.join(HERE, "index.html"), "sample", lambda name: sample(name, home))
    finally:
        home[0].cleanup()
