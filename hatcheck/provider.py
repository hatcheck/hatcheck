#!/usr/bin/env python3
"""Settings for running Claude Code on Z.ai (GLM models) or Kimi instead of Anthropic.

Usage:
    hatcheck provider                    show what's set up
    hatcheck provider setup z|k          ask for the API key and model, and save them
    hatcheck provider env z|k [--json]   the environment that points Claude Code there

You rarely run this yourself. claude-z and claude-k start Claude Code on Z.ai or
Kimi, cl -z <profile> and cl -k <profile> do it with a profile, and all of them get
their environment from `hatcheck provider env`. `claude-z reconfig` and
`claude-k reconfig` run the setup.

The settings live where the claude-z repo's launchers keep them, so a setup made
with those works as is: ~/.config/claude-z/config and ~/.config/claude-k/config
(%APPDATA%\\claude-z\\config and %APPDATA%\\claude-k\\config on Windows), with one
KEY="value" per line:

  Z.ai  CZ_API_KEY, CZ_MODEL_OPUS, CZ_MODEL_SONNET, CZ_MODEL_HAIKU, CZ_MAX_TOKENS
  Kimi  CK_API_KEY, CK_MODEL, CK_MAX_TOKENS

plus CZ_BASE_URL or CK_BASE_URL to use another endpoint. Other lines are kept
and ignored, including the launchers' *_PERMISSION_MODE.

The key reaches Claude Code as ANTHROPIC_AUTH_TOKEN, which it always uses ahead
of the profile's claude.ai login and never asks about, and ANTHROPIC_API_KEY is
cleared. Claude Code asks once per profile whether to use an ANTHROPIC_API_KEY,
with No preselected, and after a No it sends the profile's claude.ai login token
to the provider instead. The main model is pinned with ANTHROPIC_MODEL, so a
"model" in the profile's settings.json, such as "fable", can't send a Claude
model name to the provider.
"""
import argparse
import getpass
import json
import os
import re
import sys

HOME = os.path.expanduser("~")

# models: the config fields for the model slots, each choice filling all of them.
PROVIDERS = {
    "z": {
        "name": "Z.ai", "prefix": "CZ_", "url": "https://api.z.ai/api/anthropic",
        "key_from": "https://z.ai/manage-apikey/apikey-list",
        "models": ("MODEL_OPUS", "MODEL_SONNET", "MODEL_HAIKU"),
        "choices": [
            ("glm-5.3[1m]", "flagship, 1M context, with glm-5.3-flash for quick tasks",
             ("glm-5.3[1m]", "glm-5.3[1m]", "glm-5.3-flash")),
            ("glm-5.3-flash", "fast, multimodal, cheapest", ("glm-5.3-flash",) * 3),
            ("glm-4.7", "fast, good at code", ("glm-4.7",) * 3),
        ],
    },
    "k": {
        "name": "Kimi", "prefix": "CK_", "url": "https://api.kimi.com/coding/",
        "key_from": "the Kimi Code Console, https://www.kimi.com/code (Create API Key)",
        "models": ("MODEL",),
        "choices": [
            ("k3[1m]", "K3, 1M context (Allegretto plan and up)", ("k3[1m]",)),
            ("k3", "K3, 256k context (Moderato and up)", ("k3",)),
            ("kimi-for-coding", "K2.7 Code, 256k context (all plans)", ("kimi-for-coding",)),
            ("kimi-for-coding-highspeed", "K2.7 Code, faster output (Allegretto and up)",
             ("kimi-for-coding-highspeed",)),
        ],
    },
}
LINE = re.compile(r"\s*([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(.*?)\s*$")


def config_path(p):
    if os.name == "nt":
        base = os.environ.get("APPDATA") or os.path.join(HOME, "AppData", "Roaming")
    else:
        base = os.path.join(HOME, ".config")
    return os.path.join(base, "claude-" + p, "config")


def shown(path):
    return "~" + path[len(HOME):] if path.startswith(HOME + os.sep) else path


def read_config(path):
    """(values, lines) of a config file, or (None, []) when there's none."""
    try:
        with open(path, encoding="utf-8-sig") as fh:  # PowerShell 5 writes a BOM
            lines = fh.read().splitlines()
    except FileNotFoundError:
        return None, []
    values = {}
    for line in lines:
        m = LINE.match(line)
        if m and not line.lstrip().startswith("#"):
            value = m.group(2)
            if len(value) >= 2 and value[0] == value[-1] and value[0] in "\"'":
                value = value[1:-1]
            values[m.group(1)] = value
    return values, lines


def models(p, values):
    spec = PROVIDERS[p]
    return tuple(values.get(spec["prefix"] + field, "").strip() or default
                 for field, default in zip(spec["models"], spec["choices"][0][2]))


def environment(p, values):
    """[(name, value)] for Claude Code; a value of None means unset it."""
    spec = PROVIDERS[p]

    def get(field):
        return values.get(spec["prefix"] + field, "").strip()

    env = [("ANTHROPIC_BASE_URL", get("BASE_URL") or spec["url"]),
           ("ANTHROPIC_AUTH_TOKEN", get("API_KEY")),
           # Unset, so that no Anthropic credential from your environment reaches the provider.
           ("ANTHROPIC_API_KEY", None), ("CLAUDE_CODE_OAUTH_TOKEN", None),
           ("API_TIMEOUT_MS", "3000000")]
    if p == "z":
        opus, sonnet, haiku = models(p, values)
        env += [("ANTHROPIC_MODEL", opus), ("ANTHROPIC_DEFAULT_FABLE_MODEL", opus),
                ("ANTHROPIC_DEFAULT_OPUS_MODEL", opus), ("ANTHROPIC_DEFAULT_SONNET_MODEL", sonnet),
                ("ANTHROPIC_DEFAULT_HAIKU_MODEL", haiku), ("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC", "1")]
        if "[1m]" in opus:
            env.append(("CLAUDE_CODE_AUTO_COMPACT_WINDOW", "1000000"))
    else:
        model, = models(p, values)
        env += [(name, model) for name in (
            "ANTHROPIC_MODEL", "ANTHROPIC_DEFAULT_FABLE_MODEL", "ANTHROPIC_DEFAULT_OPUS_MODEL",
            "ANTHROPIC_DEFAULT_SONNET_MODEL", "ANTHROPIC_DEFAULT_HAIKU_MODEL", "CLAUDE_CODE_SUBAGENT_MODEL")]
        window = "1048576" if model.endswith("[1m]") else "262144"
        env += [("CLAUDE_CODE_AUTO_COMPACT_WINDOW", window), ("CLAUDE_CODE_MAX_CONTEXT_TOKENS", window)]
    if get("MAX_TOKENS"):
        env.append(("CLAUDE_CODE_MAX_OUTPUT_TOKENS", get("MAX_TOKENS")))
    return env


def sh_quote(value):
    return "'" + value.replace("'", "'\"'\"'") + "'"


def print_env(p, as_json):
    path = config_path(p)
    values, _ = read_config(path)
    name = PROVIDERS[p]["name"]
    if values is None:
        sys.exit("%s isn't set up yet. Run: claude-%s reconfig" % (name, p))
    if not values.get(PROVIDERS[p]["prefix"] + "API_KEY", "").strip():
        sys.exit("No %s API key in %s. Run: claude-%s reconfig" % (name, shown(path), p))
    env = environment(p, values)
    if as_json:
        print(json.dumps(dict(env)))
    else:
        for name, value in env:
            print("unset " + name if value is None else "export %s=%s" % (name, sh_quote(value)))


def describe(chosen):
    """The main model, then any other slot that differs from it."""
    others = ["%s %s" % (slot, model) for slot, model in zip(("sonnet", "haiku"), chosen[1:])
              if model != chosen[0]]
    return chosen[0] + (" (%s)" % ", ".join(others) if others else "")


def status():
    for p, spec in PROVIDERS.items():
        path = config_path(p)
        values, _ = read_config(path)
        print("%s (claude-%s, or cl -%s <profile>): %s" % (
            spec["name"], p, p, shown(path) if values is not None else "not set up; run claude-%s reconfig" % p))
        if values is not None:
            key = values.get(spec["prefix"] + "API_KEY", "").strip()
            tokens = values.get(spec["prefix"] + "MAX_TOKENS", "").strip()
            print("  key %s · %s%s%s" % (
                "…" + key[-4:] if key else "missing", describe(models(p, values)),
                " · max output %s" % tokens if tokens else "",
                " · " + values[spec["prefix"] + "BASE_URL"] if values.get(spec["prefix"] + "BASE_URL") else ""))


def ask(prompt, secret=False):
    """A line from the user; Enter or end of input gives ''."""
    try:
        if secret and sys.stdin.isatty():
            return getpass.getpass(prompt).strip()
        return input(prompt).strip()
    except EOFError:
        print()
        return ""


def write_config(path, lines, updates):
    """Set the fields in `updates`, keeping every other line, readable only by you."""
    out, done = [], set()
    for line in lines:
        m = LINE.match(line)
        if m and m.group(1) in updates and not line.lstrip().startswith("#"):
            if m.group(1) not in done:
                out.append('%s="%s"' % (m.group(1), updates[m.group(1)]))
                done.add(m.group(1))
            continue
        out.append(line)
    out += ['%s="%s"' % (name, value) for name, value in updates.items() if name not in done]
    os.makedirs(os.path.dirname(path), exist_ok=True)
    tmp = path + ".tmp"
    with os.fdopen(os.open(tmp, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600), "w",
                   encoding="utf-8", newline="\n") as fh:
        fh.write("\n".join(out) + "\n")
    os.replace(tmp, path)
    if os.name != "nt":
        os.chmod(path, 0o600)


def setup(p):
    spec, path = PROVIDERS[p], config_path(p)
    values, lines = read_config(path)
    values = values or {}
    pre = spec["prefix"]
    print("Claude Code on %s. Settings go in %s" % (spec["name"], shown(path)))

    current = values.get(pre + "API_KEY", "").strip()
    print("Get an API key from %s" % spec["key_from"])
    while True:
        key = ask("API key%s: " % (" (Enter keeps …%s)" % current[-4:] if current else ""), secret=True) or current
        if not key:
            sys.exit("Nothing saved: an API key is needed")
        if not re.search(r"[\s\"'\\]", key):
            break
        print("That has spaces, quotes or backslashes in it; paste just the key.")

    chosen = models(p, values)
    print("Model:")
    for n, (label, about, _) in enumerate(spec["choices"], 1):
        print("  %d) %-26s %s" % (n, label, about))
    while True:
        answer = ask("Choose 1-%d, or Enter to keep %s: " % (len(spec["choices"]), describe(chosen)))
        if not answer:
            break
        if answer.isdigit() and 1 <= int(answer) <= len(spec["choices"]):
            chosen = spec["choices"][int(answer) - 1][2]
            break
        print("Type a number from the list, or press Enter.")

    tokens = values.get(pre + "MAX_TOKENS", "").strip()
    while True:
        answer = ask("Max output tokens per reply, e.g. 64000 (Enter keeps %s; - for Claude Code's default): "
                     % (tokens or "Claude Code's default"))
        if answer == "-":
            tokens = ""
        elif answer.isdigit():
            tokens = answer
        elif answer:
            print("Type a number, - or press Enter.")
            continue
        break

    updates = {pre + "API_KEY": key}
    updates.update((pre + field, model) for field, model in zip(spec["models"], chosen))
    updates[pre + "MAX_TOKENS"] = tokens
    write_config(path, lines, updates)
    print("Saved. Start Claude Code on %s with claude-%s, or with a profile: cl -%s <profile>"
          % (spec["name"], p, p))


def main(argv=None, prog=None):
    parser = argparse.ArgumentParser(
        prog=prog, description="Settings for running Claude Code on Z.ai (z) or Kimi (k) instead of Anthropic.")
    sub = parser.add_subparsers(dest="command")
    s = sub.add_parser("setup", help="ask for the API key and model, and save them")
    s.add_argument("provider", choices=sorted(PROVIDERS))
    e = sub.add_parser("env", help="the environment that points Claude Code at the provider")
    e.add_argument("provider", choices=sorted(PROVIDERS))
    e.add_argument("--json", action="store_true", help="as JSON (for cl.ps1), with null for unset")
    args = parser.parse_args(argv)
    if args.command == "setup":
        setup(args.provider)
    elif args.command == "env":
        print_env(args.provider, args.json)
    else:
        status()


if __name__ == "__main__":
    main()
