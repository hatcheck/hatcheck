#!/usr/bin/env python3
"""Run each model claude-k, claude-z, claude-m and claude-d offer through Claude Code once, as
those launchers run it, and say whether it answered: a step of docs/maintenance.md, for after a
menu changes or a provider's guide does.

    python3 scripts/provider_probe.py [path/to/hatcheck] [k|z|m|d ...]

Each run asks for a one-word reply (one small request on your Kimi, Z.ai or MiniMax plan, or your
DeepSeek balance, per model), with the environment `hatcheck provider env` gives for that
launcher's config, the model in every slot and max effort, in a throwaway CLAUDE_CONFIG_DIR,
saving no session. Prints no key."""
import json
import os
import shutil
import subprocess
import sys
import tempfile

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from upstream import REPO, menus  # noqa: E402

HATCHECK = next((a for a in sys.argv[1:] if os.sep in a), os.path.join(REPO, "target", "debug", "hatcheck"))
CHOSEN = [a for a in sys.argv[1:] if a in ("k", "z", "m", "d")] or ["k", "z", "m", "d"]
SLOTS = ["ANTHROPIC_MODEL", "ANTHROPIC_DEFAULT_FABLE_MODEL", "ANTHROPIC_DEFAULT_OPUS_MODEL",
         "ANTHROPIC_DEFAULT_SONNET_MODEL", "ANTHROPIC_DEFAULT_HAIKU_MODEL"]


def probe(provider, model):
    given = json.loads(subprocess.run([HATCHECK, "provider", "env", provider, "--json"], capture_output=True,
                                      text=True, check=True).stdout)
    # Nothing of the shell this runs in, such as a Claude Code session's own variables.
    env = {k: v for k, v in os.environ.items() if not k.startswith(("CLAUDE", "ANTHROPIC"))}
    env.update({k: v for k, v in given.items() if v is not None})
    for name in SLOTS + (["CLAUDE_CODE_SUBAGENT_MODEL"] if provider != "z" else []):
        env[name] = model
    if provider == "k":
        window = "1048576" if model.endswith("[1m]") else "262144"
        env["CLAUDE_CODE_AUTO_COMPACT_WINDOW"] = env["CLAUDE_CODE_MAX_CONTEXT_TOKENS"] = window
    root = tempfile.mkdtemp(prefix="provider-probe-")
    env["CLAUDE_CONFIG_DIR"] = os.path.join(root, "config")
    try:
        # At max effort, as the launchers start Claude Code unless their config names another.
        p = subprocess.run(["claude", "--effort", "max", "-p", "Reply with exactly the word OK and nothing else.",
                            "--output-format", "stream-json", "--verbose", "--no-session-persistence"],
                           cwd=root, env=env, capture_output=True, text=True, timeout=300)
    except subprocess.TimeoutExpired:
        return "no answer in 5 minutes"
    finally:
        shutil.rmtree(root, ignore_errors=True)
    blocks, text, error = [], "", None
    for line in p.stdout.splitlines():
        try:
            event = json.loads(line)
        except ValueError:
            continue
        for block in event.get("message", {}).get("content", []) if event.get("type") == "assistant" else []:
            blocks.append(block.get("type"))
            text += block.get("text", "") if block.get("type") == "text" else ""
        if event.get("type") == "result" and event.get("is_error"):
            error = str(event.get("result"))[:200]
    if error or p.returncode:
        return "failed: %s" % (error or p.stderr.strip()[-200:])
    return "answered %r%s" % (text.strip()[:20], " with thinking" if "thinking" in blocks else "")


def main():
    claude = subprocess.run(["claude", "--version"], capture_output=True, text=True).stdout.strip()
    print("Through Claude Code %s, with %s:" % (claude, HATCHECK))
    for provider in CHOSEN:
        for model in menus().get(provider, []):
            print("  claude-%s %-26s %s" % (provider, model, probe(provider, model)))


if __name__ == "__main__":
    main()
