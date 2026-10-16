#!/usr/bin/env python3
"""Which account to use now, and where to carry a session.

Usage:
    hatcheck best [--for SESSION] [--cached]

  --for    : where to carry a session, by its ID or the start of it: the hatcheck
             move command for the best other account of its agent.
  --cached : don't call the APIs; use the snapshots Claude Code and Codex last saved.

Claude Code accounts are ranked as hatcheck usage sorts them. First come those with
Fable left, the soonest Fable reset on top, since what's left goes unused once it
resets. Next come those out of Fable, or out of the weekly limit for all models
that it counts against too, the soonest back on top. Then come accounts without a
Fable limit, by the tightest limit that applies (the one with the least left of the
5-hour and weekly limits), the sooner reset first on a tie. Codex homes are ranked
by their tightest limit too. Each line ends with the command that starts the agent
on that account. --for skips accounts that are out, and the session's own.

Advice only: it runs when you ask, prints commands, and never starts, moves or
switches anything. Using several accounts to get around one plan's limits may break
Anthropic's or OpenAI's terms; this is for choosing between accounts you may use for
the work at hand, such as your own and your company's.

This script loads sessions.py, usage.py and move.py, so keep them in one folder.
"""
import argparse
import importlib.util
import os
import sys
import time


def load_sibling(name, file):
    sys.dont_write_bytecode = True  # leave no __pycache__ folder beside the scripts
    path = os.path.join(os.path.dirname(os.path.realpath(__file__)), file)
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def meters(usage, saved_at, now, u):
    """(short name, % left, next reset, kind) for an account's 5-hour, weekly and per-model limits."""
    out = []
    for r in (usage or {}).get("limits") or []:
        if isinstance(r, dict):
            left, reset = u.meter_state(r, saved_at, now)
            out.append((u.row_label(r, short=True), left, reset, r.get("kind")))
    return out


def rank(usage, saved_at, now, u):
    """Sort key and state for an account: (group, time or -% left, reset), and whether it
    has room now. Groups: 0 Fable left, 1 out of Fable, 2 no Fable limit, 3 no data."""
    if usage is None:
        return (3, 0, 0), False
    group, at = u.fable_rank(usage, saved_at, now)
    if group < 2:
        return (group, at, 0), group == 0
    rows = [m for m in meters(usage, saved_at, now, u) if not str(m[3] or "").endswith("_scoped")]
    if not rows:
        return (2, 0, 0), True
    tightest = min(rows, key=lambda m: (m[1], m[2] or float("inf")))
    return (2, -tightest[1], tightest[2] or float("inf")), tightest[1] > 0


def summary(usage, saved_at, why, now, u, room):
    """What the line says about an account: its Fable limit first, then the tightest other."""
    if usage is None:
        return "no data (%s)" % why
    rows = meters(usage, saved_at, now, u)
    fable = next(((n, left, reset) for n, left, reset, kind in rows if kind == "weekly_scoped" and "fable" in n.lower()),
                 None)
    others = sorted((m for m in rows if not str(m[3] or "").endswith("_scoped")), key=lambda m: m[1])
    parts = []
    if fable:
        name, left, reset = fable
        parts.append("%s %d%% left%s" % (name, left, ", resets %s" % u.clock(reset, now) if reset and left else
                                         ", back %s" % u.clock(reset, now) if reset else ""))
    for name, left, reset, _ in others[:2 if not fable else 1]:
        if left <= 0 and reset:
            parts.append("%s used up, back %s" % (name, u.clock(reset, now)))
        else:
            parts.append("%s %d%%" % (name, left))
    if not room and fable and fable[1] > 0:
        parts.append("(the weekly limit blocks Fable)")
    text = " · ".join(parts) or "no limits"
    return text + (" · cached %s ago" % u.duration(now - saved_at) if saved_at is not None else "")


def ranked(accounts, now, u):
    """[(key, room, entry, result)] best first, for one agent's accounts."""
    out = []
    for entry, result in accounts:
        usage, saved_at, _ = result
        key, room = rank(usage, saved_at, now, u)
        out.append((key, room, entry, result))
    return sorted(out, key=lambda a: a[0])


def place_name(agent, profile, sessions):
    """How hatcheck move names a profile or home: default, NAME, codex, codex:NAME, or a path."""
    home = os.path.join(sessions.HOME, ".codex" if agent == "codex" else ".claude")
    if sessions.same_folder(profile, home):
        return "codex" if agent == "codex" else "default"
    parent, base = os.path.split(profile.rstrip("/" + os.sep))
    prefix = os.path.basename(home) + "-"
    if sessions.same_folder(parent, sessions.HOME) and base.startswith(prefix):
        return ("codex:" if agent == "codex" else "") + base[len(prefix):]
    return profile


def main(argv=None, prog=None):
    parser = argparse.ArgumentParser(prog=prog, description="Which account to use now, and where to carry a session. "
                                                            "Advice only: it never starts, moves or switches anything.")
    parser.add_argument("--for", dest="session", metavar="SESSION",
                        help="where to carry this session: the hatcheck move command for the best other account")
    parser.add_argument("--cached", action="store_true",
                        help="don't call the APIs; use the snapshots Claude Code and Codex last saved")
    args = parser.parse_args(argv)

    sessions = load_sibling("hatcheck_sessions", "sessions.py")
    u = sessions.load_usage_module()
    if not u:
        sys.exit("hatcheck best needs usage.py in its folder")
    ansi = u.setup_console()
    color = sys.stdout.isatty() and not os.environ.get("NO_COLOR") and ansi
    paint = (lambda code, s: "\033[%sm%s\033[0m" % (code, s) if code else s) if color else (lambda code, s: s)
    show = sessions.shell_path
    now = time.time()
    loaded = u.load_usage(sessions.find_profiles(), args.cached, sessions.find_codex_homes())
    by_agent = {"claude": ranked([a for a in loaded if not a[0].get("codex")], now, u),
                "codex": ranked([a for a in loaded if a[0].get("codex")], now, u)}

    if args.session:
        return carry(args, sessions, u, by_agent, now, show, (parser.prog or "hatcheck").split()[0])

    for agent, title in (("claude", "Claude Code, best first"), ("codex", "Codex, best first")):
        accounts = by_agent[agent]
        if not accounts:
            continue
        print(paint("1", title) + (paint("2", " (sorted by Fable, as hatcheck usage sorts)") if agent == "claude" else ""))
        rows = []
        for n, (key, room, entry, (usage, saved_at, why)) in enumerate(accounts, 1):
            names = ", ".join(show(p) for p in entry["profiles"])
            rows.append(("%d." % n if key[0] < 3 else " -", names, summary(usage, saved_at, why, now, u, room),
                         sessions.start_command(agent, entry["profiles"][0]), room))
        width = max(len(r[1]) for r in rows)
        text_width = max(len(r[2]) for r in rows)
        for number, names, text, command, room in rows:
            print("  %s %s  %s  %s" % (number.rjust(3), paint("1" if room else "2", names.ljust(width)),
                                      paint("" if room else "2", text.ljust(text_width)), paint("36", "→ " + command)))
    me = (parser.prog or "hatcheck best").split()[0]
    print(paint("2", "Advice only: nothing is started, moved or switched. %s best --for <session> says where "
                     "to carry a session." % me))


def carry(args, sessions, u, by_agent, now, show, me):
    """--for: the best other account for a session of the same agent, as a hatcheck move command."""
    copies = sessions.session_copies(args.session)
    if not copies:
        sys.exit("No session starting with %s" % args.session)
    if len(copies) > 1:
        sys.exit("%s matches %d sessions; give more of the ID" % (args.session, len(copies)))
    ((agent, sid), where), = copies.items()
    found = [s for s in (sessions.summarize(agent, p, f) for p, f in where) if s]
    if not found:
        sys.exit("Session %s holds no conversation" % sid[:8])
    s = max(found, key=lambda s: (s["when"], s.get("canonical", True), s["size"]))
    accounts = by_agent[agent]
    mine = next((a for a in accounts if any(sessions.same_folder(p, s["profile"]) for p in a[2]["profiles"])), None)
    if mine:
        _, room, entry, (usage, saved_at, why) = mine
        state = summary(usage, saved_at, why, now, u, room)
    else:
        state = "no usage data"
    print('Session %s "%s" is in %s: %s.' % (sid[:8], sessions.truncate(s["title"], 60), show(s["profile"]), state))
    best = next((a for a in accounts if a[1] and a is not mine), None)
    if mine and accounts and mine[1] and accounts.index(mine) < (accounts.index(best) if best else len(accounts)):
        print("Its own account is the best one now; resume it there with:")
        if s.get("cwd"):
            print("  cd " + show(s["cwd"]))
        print("  " + sessions.session_command(s, False))
        return
    if best is None:
        print("No other %s account has room now." % ("Codex" if agent == "codex" else "Claude Code"))
        return 1
    _, _, entry, (usage, saved_at, why) = best
    dest = entry["profiles"][0]
    print("Carry it to %s: %s." % (show(dest), summary(usage, saved_at, why, now, u, True)))
    print("  %s move %s %s --from %s" % (me, sid[:8], place_name(agent, dest, sessions),
                                               place_name(agent, s["profile"], sessions)))
    print("It prints the command that resumes it there, and offers to run it.")


if __name__ == "__main__":
    sys.exit(main())
