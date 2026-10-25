#!/usr/bin/env python3
"""What has changed upstream since the docs were last checked: the first step of
docs/maintenance.md, run every week or two and before a release.

    python3 scripts/upstream.py [--providers]

1. The agents: each one's version installed here, its latest release, and the versions the docs
   say hatcheck was checked against.
2. The desktop apps: each Homebrew cask's version, and any the cask marks deprecated.
3. Every link in the README, docs/ and site/, and each install script the site gives: its status,
   and where it redirects now.
4. With --providers: Kimi's and Z.ai's model lists, asked with the keys in claude-k's and
   claude-z's config files (sent only to those providers' own model-list endpoints, as claude-k
   and claude-z send them), beside the menus in crates/hatcheck/src/provider.rs.

It changes nothing and starts no agent: the installed versions come from each command's
--version (ZCode's from its app bundle, read as a file), and nothing else runs an agent."""
import glob
import json
import os
import plistlib
import re
import shutil
import subprocess
import sys
import urllib.error
import urllib.request

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
AGENT_UA = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 Chrome/141 Safari/537.36"
DOCS = ["README.md", "docs/agents.md", "docs/commands.md", "docs/install.md"]
VERSION = r"\d+\.\d+\.\d+(?:-[a-z]+\.\d+)?"


def fetch(url, headers=None, method="GET", timeout=20):
    """(status, final url, body bytes) of a request; status 0 and the error when it fails."""
    req = urllib.request.Request(url, method=method, headers=dict({"User-Agent": AGENT_UA}, **(headers or {})))
    try:
        with urllib.request.urlopen(req, timeout=timeout) as r:
            return r.status, r.geturl(), r.read() if method == "GET" else b""
    except urllib.error.HTTPError as e:
        return e.code, url, b""
    except Exception as e:  # DNS, TLS, timeouts
        return 0, url, str(e).encode()


def fetch_json(url, headers=None):
    status, _, body = fetch(url, headers)
    try:
        return json.loads(body) if status == 200 else None
    except ValueError:
        return None


def npm(package, tag="latest"):
    d = fetch_json("https://registry.npmjs.org/" + package.replace("/", "%2f"))
    return (d or {}).get("dist-tags", {}).get(tag)


def cask(name):
    """(version, deprecated) of a Homebrew cask, or (None, None)."""
    d = fetch_json("https://formulae.brew.sh/api/cask/%s.json" % name)
    if not d:
        return None, None
    return str(d.get("version", "")).split(",")[0], d.get("deprecation_reason") if d.get("deprecated") else None


def github_release(repo):
    for r in fetch_json("https://api.github.com/repos/%s/releases?per_page=20" % repo) or []:
        if not r.get("prerelease") and not r.get("draft"):
            m = re.search(VERSION, r.get("tag_name", ""))
            return m.group(0) if m else r.get("tag_name")
    return None


def run_version(argv):
    if not shutil.which(argv[0]):
        return "not on PATH"
    try:
        p = subprocess.run(argv, capture_output=True, text=True, timeout=30, stdin=subprocess.DEVNULL)
    except subprocess.TimeoutExpired:
        return "no answer"
    m = re.search(VERSION, p.stdout + p.stderr)
    return m.group(0) if m else (p.stdout + p.stderr).strip()[:40]


def zcode_installed():
    """The ZCode app's version and the CLI version its bundle names, read as files."""
    app = "/Applications/ZCode.app/Contents"
    if not os.path.isdir(app):
        return "no ZCode app here"
    with open(os.path.join(app, "Info.plist"), "rb") as fh:
        version = plistlib.load(fh).get("CFBundleShortVersionString")
    cli = "?"
    try:
        data = open(os.path.join(app, "Resources", "glm", "zcode.cjs"), "rb").read().decode("utf-8", "replace")
        m = re.search(r'join\)\(\w+\(\),"(%s)",t\.target' % VERSION, data)
        cli = m.group(1) if m else "?"
    except OSError:
        pass
    return "app %s, CLI %s" % (version, cli)


def cited(names, own=None):
    """The versions the docs give for an agent: those beside one of its names, and every match of
    `own`, a pattern of the agent's own version numbers, for docs that name the agent elsewhere."""
    parts = [r"(?:%s)[`'\s(]{1,4}(%s)" % ("|".join(re.escape(n) for n in names), VERSION)]
    if own:
        parts.append("(%s)" % own)
    pattern = re.compile("|".join(parts))
    found = set()
    for doc in DOCS:
        path = os.path.join(REPO, doc)
        if os.path.exists(path):
            for m in pattern.finditer(open(path, encoding="utf-8").read()):
                found.update(g for g in m.groups() if g)
    return ", ".join(sorted(found, key=lambda v: [int(x) if x.isdigit() else x for x in re.split(r"[.-]", v)])) or "none"


# Each agent hatcheck reads: how this machine says which version it has, where its newest
# release is announced, and how the docs name it beside a version.
AGENTS = [
    ("Claude Code", lambda: run_version(["claude", "--version"]),
     lambda: "%s (stable %s)" % (npm("@anthropic-ai/claude-code"), npm("@anthropic-ai/claude-code", "stable")),
     ["claude", "Claude Code"], r"\b2\.1\.\d{3}\b"),
    ("Codex", lambda: run_version(["codex", "--version"]), lambda: npm("@openai/codex"), ["codex"],
     r"\b0\.1\d\d\.\d+\b"),
    ("Antigravity CLI", lambda: run_version(["agy", "--version"]), lambda: cask("antigravity-cli")[0], ["agy"]),
    ("Kimi Code CLI", lambda: run_version(["kimi", "--version"]), lambda: github_release("MoonshotAI/kimi-code"),
     ["kimi"]),
    ("ZCode", zcode_installed, lambda: "app %s; its CLI's version is inside the app" % cask("zcode")[0], ["ZCode"]),
    ("DeepSeek Harness", lambda: run_version(["dsh", "--version"]),
     lambda: "%s (next %s)" % (npm("@deepseek-ai/dsh"), npm("@deepseek-ai/dsh", "next")), ["dsh"]),
    # Its installer puts mcode in ~/.minimax-code/bin, which needn't be on the PATH.
    ("MiniMax Code", lambda: run_version([shutil.which("mcode") or os.path.expanduser("~/.minimax-code/bin/mcode"),
                                          "--version"]),
     lambda: "? (no release feed: see its installer)", ["mcode", "MiniMax Code"], r"\b0\.6\.\d+\b"),
]

APPS = [("Claude Desktop", "claude"), ("ChatGPT, with Codex", "chatgpt"), ("Codex app (old cask)", "codex-app"),
        ("Antigravity", "antigravity"), ("ZCode", "zcode")]


def links():
    """Every outside link in the README, docs and site, and the install scripts the site gives."""
    urls = set()
    files = glob.glob(os.path.join(REPO, "docs", "**", "*.md"), recursive=True)
    files += [os.path.join(REPO, "README.md")] + glob.glob(os.path.join(REPO, "site", "*.html"))
    for path in files:
        for url in re.findall(r"https?://[^\s)\"'<>`]+", open(path, encoding="utf-8").read()):
            url = url.rstrip(".,;:")
            if not re.search(r"github\.com/hatcheck/hatcheck|hatcheck\.dev|localhost|127\.0\.0\.1|example\.|api\.|backend-api|youtube",
                             url):
                urls.add(url.replace("&amp;", "&"))
    return sorted(urls)


def menus():
    """The model choices claude-k and claude-z offer, from provider.rs: {"k": [...], "z": [...]}."""
    text = open(os.path.join(REPO, "crates", "hatcheck", "src", "provider.rs"), encoding="utf-8").read()
    return {m.group(1): re.findall(r'\(\s*"([\w.\-\[\]]+)",', m.group(2))
            for m in re.finditer(r'key: "(\w)".*?choices: &\[(.*?)\n        \],', text, re.S)}


def config_key(provider):
    base = os.environ.get("APPDATA") if os.name == "nt" else os.path.join(os.path.expanduser("~"), ".config")
    path = os.path.join(base or "", "claude-" + provider, "config")
    if not os.path.exists(path):
        return None
    m = re.search(r'^C%s_API_KEY="?([^"\n]*)"?$' % provider.upper(), open(path, encoding="utf-8-sig").read(), re.M)
    return m.group(1) if m and m.group(1) else None


def providers():
    for key, name, url in (("k", "Kimi", "https://api.kimi.ai/coding/v1/models"),
                           ("z", "Z.ai", "https://api.z.ai/api/anthropic/v1/models")):
        secret = config_key(key)
        if not secret:
            print("  %s: claude-%s isn't set up here" % (name, key))
            continue
        d = fetch_json(url, {"x-api-key": secret, "Authorization": "Bearer " + secret, "anthropic-version": "2023-06-01"})
        if not d:
            print("  %s: the model list didn't answer" % name)
            continue
        offered = {m["id"]: m.get("display_name", "") for m in d.get("data", [])}
        menu = [m.replace("[1m]", "") for m in menus().get(key, [])]
        print("  %s models: %s" % (name, ", ".join("%s (%s)" % kv for kv in offered.items())))
        missing = [m for m in menu if m not in offered]
        unlisted = [m for m in offered if m not in menu]
        print("    in claude-%s's menu but not offered: %s" % (key, ", ".join(missing) or "none"))
        print("    offered but not in the menu: %s" % (", ".join(unlisted) or "none"))


def main():
    print("The agents (installed here · latest · the docs cite):")
    for name, installed, latest, names, *own in AGENTS:
        print("  %-17s %s · %s · %s" % (name, installed(), latest(), cited(names, *own)))
    print("\nDesktop apps (Homebrew cask version):")
    for name, cask_name in APPS:
        version, deprecated = cask(cask_name)
        print("  %-22s %s%s" % (name, version or "no cask", " · deprecated: " + deprecated if deprecated else ""))
    print("\nLinks and install scripts:")
    broken = 0
    for url in links():
        status, final, _ = fetch(url)
        moved = final.rstrip("/") != url.rstrip("/")
        if status != 200 or moved:
            broken += status != 200
            print("  %s %s%s" % (status or "error", url, " -> " + final if moved else ""))
    print("  every other link answered 200" if broken == 0 else "  %d didn't answer 200" % broken)
    if "--providers" in sys.argv[1:]:
        print("\nProvider models (claude-k, claude-z):")
        providers()


if __name__ == "__main__":
    main()
