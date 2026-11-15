#!/usr/bin/env python3
"""Alfred Script Filter: recent Hatcheck sessions, with a resume action per result."""
import json
import os
import shlex
import subprocess
import sys


def main():
    query = " ".join(sys.argv[1:]).lower()
    binary = os.environ.get("HATCHECK_BIN", "hatcheck")
    report = subprocess.run(
        [binary, "--json", "--cached", "--no-usage", "--all-folders", "168"],
        check=True, capture_output=True, text=True, encoding="utf-8",
    )
    sessions = json.loads(report.stdout).get("sessions", [])
    items = []
    for session in sessions:
        searchable = " ".join(str(session.get(key) or "") for key in
                              ("title", "agent", "profile", "cwd", "model")).lower()
        if query and not all(word in searchable for word in query.split()):
            continue
        cwd = session.get("cwd")
        resume = session["resume"]
        action = f"cd {shlex.quote(cwd)} && {resume}" if cwd else resume
        title = session.get("title") or "Untitled session"
        subtitle = f"{session.get('agent', '?')} · {session.get('profile', '?')} · {cwd or '?'}"
        items.append({
            "uid": f"{session['agent']}:{session['profile']}:{session['id']}",
            "title": title,
            "subtitle": subtitle,
            "arg": action,
            "variables": {"resume": resume, "cwd": cwd or ""},
            "mods": {
                "cmd": {"subtitle": "Copy the resume command", "arg": resume},
                "alt": {"subtitle": "Show the raw session JSON", "arg": json.dumps(session, ensure_ascii=False)},
            },
        })
    if not items:
        items.append({"uid": "empty", "title": "No matching sessions",
                      "subtitle": "Start a session with cl, cx or ck, or change the query.", "arg": ""})
    print(json.dumps({"items": items}, ensure_ascii=False))


if __name__ == "__main__":
    main()
