#!/bin/bash
# Raycast Script Command: recent Hatcheck sessions, with resume commands to copy.
# @raycast.schemaVersion 1
# @raycast.title Hatcheck: Recent Sessions
# @raycast.mode fullOutput
# @raycast.packageName Hatcheck
# @raycast.description Browse recent agent sessions and copy a command that resumes one.
# @raycast.author Yi Wang
# @raycast.icon assets/hatcheck.png
# @raycast.argument1 { "type": "text", "placeholder": "Agent (optional: claude, codex, kimi…)", "optional": true }
set -euo pipefail

bin="${HATCHECK_BIN:-hatcheck}"
args=(--json --cached --no-usage --all-folders 168)
if [[ "${1:-}" != "" ]]; then
  args+=(--only "$1")
fi
"$bin" "${args[@]}" | python3 -c '
import json, sys
report = json.load(sys.stdin)
sessions = report.get("sessions", [])
if not sessions:
    print("No recent sessions. Start one with cl, cx or ck, then come back.")
for s in sessions:
    agent = s.get("agent", "?")
    profile = s.get("profile", "?")
    cwd = s.get("cwd") or "?"
    print(s.get("title") or "Untitled session")
    print(f"{agent} · {profile} · {cwd}")
    print(s.get("resume") or "")
    print("")
'