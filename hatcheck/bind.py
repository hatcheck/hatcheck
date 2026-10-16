#!/usr/bin/env python3
"""Tie a folder to a profile, so work there happens under the right account.

Usage:
    hatcheck bind PROFILE [FOLDER] [--codex HOME]  bind FOLDER (this one by default) and
                                                   everything under it
    hatcheck bind                                  what this folder is bound to
    hatcheck bind --list                           every bound folder
    hatcheck bind --hook                           warn when Claude Code or Codex starts in a
                                                   bound folder under another profile
    hatcheck unbind [FOLDER]                       remove a folder's binding
    hatcheck unbind --hook                         remove that warning again

  PROFILE : NAME for ~/.claude-NAME, default for ~/.claude, or a path. The folder's
            Codex home is the one by the same name, ~/.codex-NAME (~/.codex for
            default), when there is one.
  --codex : the Codex home to bind instead: NAME for ~/.codex-NAME, default for
            ~/.codex, or a path. Without PROFILE it binds just the Codex home.

Bindings live in your own config, ~/.config/hatcheck/bindings.json
(%APPDATA%\\hatcheck\\bindings.json on Windows), not in the repo, so profile names
don't end up in git. The deepest bound folder wins, and folders are compared once
symlinks are resolved.

cl and cx use them: without a profile name in a bound folder, cl --resume <id>
means cl work --resume <id>, and cx resume <id> means cx work resume <id>. The
session list marks a session that ran in a bound folder under another profile,
"⚠ folder bound to ~/.claude-work", so it's easy to spot and hatcheck move.

--hook adds a SessionStart hook to every Claude Code profile's settings.json and
every Codex home's hooks.json, which runs hatcheck bind --check. When a session
starts in a bound folder under another profile, Claude Code or Codex shows a
warning that names the folder's profile and the command to use. It only warns;
nothing is blocked. Codex runs a new hook only once you trust it, with /hooks in
codex. Claude Code shows the warning in its interactive mode, not with claude -p.

This script loads sessions.py, so keep the two in one folder.
"""
import argparse
import importlib.util
import json
import os
import shlex
import shutil
import sys

HOOK = "bind --check"  # in the hook's command, to find it again


def load_sessions():
    sys.dont_write_bytecode = True  # leave no __pycache__ folder beside the scripts
    path = os.path.join(os.path.dirname(os.path.realpath(__file__)), "sessions.py")
    spec = importlib.util.spec_from_file_location("hatcheck_sessions", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def write_json(path, data):
    """Write JSON whole, under a temporary name until it's complete."""
    os.makedirs(os.path.dirname(path), exist_ok=True)
    tmp = path + ".hatcheck-tmp"
    with open(tmp, "w", encoding="utf-8", newline="\n") as fh:
        json.dump(data, fh, ensure_ascii=False, indent=2)
        fh.write("\n")
    os.replace(tmp, path)


def save_bindings(sessions, bindings):
    write_json(sessions.bindings_path(), {"version": 1, "folders": dict(sorted(bindings.items()))})


def name_of(sessions, agent, spec):
    """(name to keep, folder) for a PROFILE or --codex argument: default, a NAME or a path."""
    if spec == "default" or ("/" not in spec and os.sep not in spec and not spec.startswith("~")):
        return spec, sessions.profile_folder(agent, spec)
    path = os.path.abspath(os.path.expanduser(spec))
    return path, path


def describe(sessions, binding):
    show = sessions.shell_path
    parts = ["%s %s" % (label, show(sessions.profile_folder(agent, binding[agent])))
             for agent, label in (("claude", "Claude Code"), ("codex", "Codex")) if binding.get(agent)]
    return " · ".join(parts) or "nothing"


def bind(args, sessions):
    show = sessions.shell_path
    folder = os.path.realpath(os.path.expanduser(args.folder or os.getcwd()))
    if not os.path.isdir(folder):
        sys.exit("%s isn't a folder" % show(folder))
    bindings = sessions.load_bindings()
    old = bindings.get(folder) or {}
    binding = {} if args.profile else {k: v for k, v in old.items() if k == "claude"}
    notes = []
    if args.profile:
        name, path = name_of(sessions, "claude", args.profile)
        if not os.path.isdir(path):
            sys.exit("%s isn't a profile folder. Sign in there first, e.g. with: %s"
                     % (show(path), "claude" if name == "default" else "cl " + name))
        binding["claude"] = name
    if args.codex:
        name, path = name_of(sessions, "codex", args.codex)
        if not os.path.isdir(path):
            sys.exit("%s isn't a Codex home. Sign in there first, e.g. with: %s"
                     % (show(path), "codex login" if name == "default" else "cx %s login" % name))
        binding["codex"] = name
    elif args.profile and binding["claude"] == args.profile and not any(c in args.profile for c in "/" + os.sep):
        path = sessions.profile_folder("codex", args.profile)  # the home named like the profile
        if os.path.isdir(path):
            binding["codex"] = args.profile
        else:
            notes.append("Codex isn't bound: there's no %s (add --codex NAME for another home)." % show(path))
    bindings[folder] = binding
    save_bindings(sessions, bindings)
    print("Bound %s and everything under it: %s." % (show(folder), describe(sessions, binding)))
    for note in notes:
        print(note)
    print("Here, %s." % " and ".join(text for agent, text in (
        ("claude", "cl --resume <id> means %s --resume <id>" % sessions.start_command("claude", sessions.profile_folder(
            "claude", binding.get("claude") or "default"))),
        ("codex", "cx resume <id> means %s resume <id>" % sessions.start_command("codex", sessions.profile_folder(
            "codex", binding.get("codex") or "default")))) if binding.get(agent)))
    if not any(has_hook(read_settings(path)[0]) for _, _, path in hook_files(sessions)):
        print("hatcheck bind --hook adds a warning to Claude Code when a session starts here under another profile.")


def show_here(sessions):
    show = sessions.shell_path
    bindings = sessions.load_bindings()
    folder = sessions.bound_folder(os.getcwd(), bindings)
    if not folder:
        print("This folder isn't bound. Bind it with: hatcheck bind <profile>")
        return
    where = "This folder" if sessions.same_folder(folder, os.getcwd()) else "This folder is under %s, which" % show(folder)
    print("%s is bound to %s." % (where, describe(sessions, bindings[folder])))


def list_bindings(sessions):
    show = sessions.shell_path
    bindings = sessions.load_bindings()
    if not bindings:
        print("No folders are bound. Bind this one with: hatcheck bind <profile>")
        return
    print("Bound folders, from %s:" % show(sessions.bindings_path()))
    width = max(len(show(f)) for f in bindings)
    for folder in sorted(bindings, key=str.lower):
        missing = "" if os.path.isdir(folder) else "  (folder missing)"
        print("  %s  %s%s" % (show(folder).ljust(width), describe(sessions, bindings[folder]), missing))


def which(args, sessions):
    """For cl and cx: the name the folder (this one by default) is bound to, or exit 1."""
    bindings = sessions.load_bindings()
    folder = sessions.bound_folder(os.path.expanduser(args.profile or os.getcwd()), bindings)
    name = bindings[folder].get(args.which) if folder else None
    if not isinstance(name, str) or not name:
        return 1
    print(name)
    return 0


# The warning hook: in each Claude Code profile's settings.json, and each Codex home's hooks.json.

def hook_command():
    """How Claude Code and Codex run the hook: hatcheck from your PATH, or this Python and hatcheck.py."""
    quote = (lambda s: '"%s"' % s) if os.name == "nt" else shlex.quote
    exe = shutil.which("hatcheck")
    if exe:
        return "%s %s" % (quote(exe), HOOK)
    script = os.path.join(os.path.dirname(os.path.dirname(os.path.realpath(__file__))), "hatcheck.py")
    return "%s %s %s" % (quote(sys.executable), quote(script), HOOK)


def hook_files(sessions):
    """(agent, profile or home, the file its hooks are in) for every Claude Code profile and Codex home."""
    return [("claude", p, os.path.join(p, "settings.json")) for p in sessions.find_profiles()] + \
        [("codex", h, os.path.join(h, "hooks.json")) for h in sessions.find_codex_homes()]


def read_settings(path):
    """(the JSON object in path, None), {} when there's no file, or (None, why not)."""
    if not os.path.exists(path):
        return {}, None
    name = os.path.basename(path)
    try:
        with open(path, encoding="utf-8") as fh:
            settings = json.load(fh)
    except (OSError, ValueError) as e:
        return None, "can't read its %s (%s)" % (name, e.strerror if isinstance(e, OSError) else "not valid JSON")
    return (settings, None) if isinstance(settings, dict) else (None, "its %s isn't a JSON object" % name)


def ours(hook):
    command = hook.get("command") if isinstance(hook, dict) else None
    return isinstance(command, str) and HOOK in command and "hatcheck" in command


def has_hook(settings):
    entries = ((settings or {}).get("hooks") or {}).get("SessionStart") if isinstance(settings, dict) else None
    return any(ours(h) for e in entries or [] if isinstance(e, dict) for h in e.get("hooks") or [])


def add_hooks(sessions):
    show, command, codex = sessions.shell_path, hook_command(), False
    for agent, folder, path in hook_files(sessions):
        settings, why = read_settings(path)
        if settings is None:
            print("  %s  skipped: %s" % (show(folder), why))
            continue
        if has_hook(settings):
            print("  %s  already has it" % show(folder))
            continue
        hooks = settings.setdefault("hooks", {})
        if not isinstance(hooks, dict) or not isinstance(hooks.setdefault("SessionStart", []), list):
            print("  %s  skipped: its hooks aren't in the usual form" % show(folder))
            continue
        hooks["SessionStart"].append({"hooks": [{"type": "command", "command": command + (" codex" * (agent == "codex"))}]})
        write_json(path, settings)
        print("  %s  added, in %s" % (show(folder), os.path.basename(path)))
        codex |= agent == "codex"
    print("They run: %s (with codex after it for Codex)" % command)
    print("Claude Code and Codex warn when a session starts in a bound folder under another profile. "
          "hatcheck unbind --hook removes them.")
    if codex:
        print("Codex runs a new hook only once you trust it: in each Codex home, open /hooks in codex (or cx <name>) "
              "and trust it.")


def remove_hooks(sessions):
    show = sessions.shell_path
    for agent, folder, path in hook_files(sessions):
        settings, why = read_settings(path)
        if not has_hook(settings):
            continue
        hooks = settings["hooks"]
        kept = []
        for entry in hooks["SessionStart"]:
            if isinstance(entry, dict) and isinstance(entry.get("hooks"), list):
                entry["hooks"] = [h for h in entry["hooks"] if not ours(h)]
                if not entry["hooks"]:
                    continue
            kept.append(entry)
        hooks["SessionStart"] = kept
        if not kept:
            del hooks["SessionStart"]
        if not hooks:
            del settings["hooks"]
        if agent == "codex" and not settings:
            os.remove(path)  # hooks.json held nothing else
        else:
            write_json(path, settings)
        print("  %s  removed" % show(folder))
    print("No profile or home runs the hook now.")


def check(sessions, agent):
    """Run by the SessionStart hook in Claude Code, or in Codex (agent codex): when the
    session's folder is bound to another profile or home, have it show a warning. Never
    blocks, and stays quiet on any error."""
    try:
        data = json.loads(sys.stdin.read() or "{}")
    except (ValueError, UnicodeError):
        return 0
    if not isinstance(data, dict):
        return 0
    show = sessions.shell_path
    cwd = data.get("cwd") or os.getcwd()
    variable, default = ("CODEX_HOME", ".codex") if agent == "codex" else ("CLAUDE_CONFIG_DIR", ".claude")
    current = os.path.expanduser(os.environ.get(variable) or os.path.join(sessions.HOME, default))
    bindings = sessions.load_bindings()
    folder = sessions.bound_folder(cwd, bindings)
    name = bindings[folder].get(agent) if folder else None
    if not isinstance(name, str) or not name:
        return 0
    want = sessions.profile_folder(agent, name)
    if sessions.same_folder(want, current):
        return 0
    start, sid = sessions.start_command(agent, want), data.get("session_id") or ""
    if data.get("source") == "resume" and sid:
        if agent == "codex":
            to = name if any(c in name for c in "/" + os.sep) else "codex" if name == "default" else "codex:" + name
            resume = sessions.codex_command(want, "resume " + sid, False)
        else:
            to, resume = name, sessions.claude_command(want, "--resume " + sid, False)
        todo = "Quit, then carry this session there with: hatcheck move %s %s, and resume it with %s" % (
            sid[:8], to, resume)
    else:
        todo = "Quit and run %s here instead" % start
    message = "hatcheck: %s is bound to %s, but this session runs in %s. %s." % (
        show(folder), show(want), show(current), todo)
    print(json.dumps({"systemMessage": message}))
    return 0


def main(argv=None, prog=None):
    parser = argparse.ArgumentParser(prog=prog, description="Tie a folder to a profile, so cl and cx pick it there and "
                                                            "the session list flags work done under another one.")
    parser.add_argument("profile", nargs="?", help="NAME for ~/.claude-NAME, default for ~/.claude, or a path")
    parser.add_argument("folder", nargs="?", help="the folder to bind (default: this one)")
    parser.add_argument("--codex", metavar="HOME", help="the Codex home to bind: NAME for ~/.codex-NAME, default for "
                                                        "~/.codex, or a path (default: the one named like PROFILE)")
    parser.add_argument("--list", action="store_true", help="list every bound folder")
    parser.add_argument("--hook", action="store_true", help="warn in Claude Code and Codex when a session starts in a "
                                                            "bound folder under another profile")
    parser.add_argument("--which", nargs="?", const="claude", choices=("claude", "codex"),
                        help=argparse.SUPPRESS)  # for cl and cx: the name bound to FOLDER, given as PROFILE
    parser.add_argument("--check", nargs="?", const="claude", choices=("claude", "codex"),
                        help=argparse.SUPPRESS)  # run by the hook, in Claude Code or in Codex
    args = parser.parse_args(argv)
    sessions = load_sessions()
    if args.check:
        return check(sessions, args.check)
    if args.which:
        return which(args, sessions)
    usage = sessions.load_usage_module()
    if usage:
        usage.setup_console()  # UTF-8 output on Windows
    if args.list:
        return list_bindings(sessions)
    if args.hook:
        return add_hooks(sessions)
    if args.profile or args.codex:
        return bind(args, sessions)
    return show_here(sessions)


def unbind_main(argv=None, prog=None):
    parser = argparse.ArgumentParser(prog=prog, description="Remove a folder's binding, or the warning hook.")
    parser.add_argument("folder", nargs="?", help="the bound folder (default: this one)")
    parser.add_argument("--hook", action="store_true", help="remove the warning hook from every profile")
    args = parser.parse_args(argv)
    sessions = load_sessions()
    show = sessions.shell_path
    if args.hook:
        return remove_hooks(sessions)
    folder = os.path.realpath(os.path.expanduser(args.folder or os.getcwd()))
    bindings = sessions.load_bindings()
    key = next((f for f in bindings if sessions.same_folder(f, folder)), None)
    if key:
        binding = bindings.pop(key)
        save_bindings(sessions, bindings)
        print("Unbound %s (it was bound to %s)." % (show(folder), describe(sessions, binding)))
        return 0
    parent = sessions.bound_folder(folder, bindings)
    if parent:
        sys.exit("%s isn't bound itself; it's under %s, which is: hatcheck unbind %s"
                 % (show(folder), show(parent), show(parent)))
    sys.exit("%s isn't bound" % show(folder))


if __name__ == "__main__":
    sys.exit(main())
