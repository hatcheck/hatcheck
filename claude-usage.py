#!/usr/bin/env python3
"""Show how much of each Claude plan limit is left, for every ~/.claude* profile.

Usage:
    python3 claude-usage.py [--cached] [--email]

  --cached : don't call the API; show the snapshot Claude Code last saved.
  --email  : show each account's email address (hidden by default).

For each profile this reads the OAuth token Claude Code keeps in the macOS
Keychain, or in the profile's .credentials.json file on Linux and Windows, and
asks the endpoint /usage uses (api.anthropic.com/api/oauth/usage).
The token is used as is and never refreshed: a refresh rotates it and would
sign out the copy Claude Code holds. When the token has expired or the request
fails (the endpoint rate-limits busy accounts), the snapshot Claude Code cached
in the profile's .claude.json is shown instead, with its age.

Rows come from the server's limits[] list: the 5-hour session, the weekly limit
for all models, per-model weekly limits such as Fable, and any new meter the
server adds. Percentages are what's left (100 - used). Quota fields the server
sends that Claude Code doesn't display are listed under their raw names.
Profiles signed in to the same account are shown together.

Each row has two bars: what's left of the limit, then how much of its window,
5 hours or a week, is left before it resets. The second is coloured by urgency,
from blue with most of the window to go, through gold and orange, to red as the
reset nears. When the first bar is shorter than the second, the limit is
being used up faster than its window runs out.

claude-recent-sessions.py loads this file to show the same numbers beside each
session, so keep the two scripts in the same folder.
"""
import argparse
import glob
import hashlib
import json
import os
import subprocess
import sys
import time
import urllib.error
import urllib.request
from concurrent.futures import ThreadPoolExecutor
from datetime import datetime

HOME = os.path.expanduser("~")
DEFAULT_PROFILE = os.path.join(HOME, ".claude")
USAGE_URL = "https://api.anthropic.com/api/oauth/usage"
SEVERITY_COLOR = {"normal": "32", "warning": "33", "critical": "31"}
# Time-until-reset bars, in equal steps from red at the reset, through orange, gold
# and muted tones, to blue at the start of the window (xterm 256-colour palette).
# Green is left out so the bars don't look like the green plan bars beside them.
URGENCY_COLORS = (160, 166, 172, 178, 143, 109, 110, 74, 68, 33, 27)


def find_profiles():
    """~/.claude plus every ~/.claude-* (and $CLAUDE_CONFIG_DIR) Claude Code config dir."""
    candidates = [DEFAULT_PROFILE] + sorted(glob.glob(os.path.join(HOME, ".claude-*")))
    if os.environ.get("CLAUDE_CONFIG_DIR"):
        candidates.append(os.path.abspath(os.path.expanduser(os.environ["CLAUDE_CONFIG_DIR"])))
    profiles = []
    for d in candidates:
        if d not in profiles and os.path.isdir(d) and (
                d == DEFAULT_PROFILE or os.path.isdir(os.path.join(d, "projects"))
                or os.path.isfile(os.path.join(d, ".claude.json"))):
            profiles.append(d)
    return profiles


def config_path(profile):
    # Without CLAUDE_CONFIG_DIR, Claude Code keeps its config beside ~/.claude, not in it.
    return os.path.join(HOME, ".claude.json") if profile == DEFAULT_PROFILE \
        else os.path.join(profile, ".claude.json")


def keychain_service(profile):
    # Claude Code suffixes the item with a hash of the CLAUDE_CONFIG_DIR it ran with.
    if profile == DEFAULT_PROFILE:
        return "Claude Code-credentials"
    return "Claude Code-credentials-" + hashlib.sha256(profile.encode()).hexdigest()[:8]


def read_oauth(profile):
    """The claudeAiOauth entry (token, expiry, plan), or None. It's in the macOS Keychain,
    or where there's no keychain (Linux, Windows) in the profile's .credentials.json."""
    if sys.platform == "darwin":
        try:
            out = subprocess.run(["security", "find-generic-password", "-s", keychain_service(profile), "-w"],
                                 capture_output=True, text=True, timeout=15).stdout
            oauth = json.loads(out).get("claudeAiOauth")
            if oauth:
                return oauth
        except (OSError, subprocess.SubprocessError, ValueError, AttributeError):
            pass
    try:
        with open(os.path.join(profile, ".credentials.json"), encoding="utf-8") as fh:
            return json.load(fh).get("claudeAiOauth")
    except (OSError, ValueError, AttributeError):
        return None


def read_config(profile):
    try:
        with open(config_path(profile), encoding="utf-8") as fh:
            config = json.load(fh)
    except (OSError, ValueError):
        return {}, None
    return config.get("oauthAccount") or {}, config.get("cachedUsageUtilization")


def fetch_usage(token):
    """(usage body, None) or (None, why it failed)."""
    request = urllib.request.Request(USAGE_URL, headers={
        "Authorization": "Bearer " + token,
        "anthropic-beta": "oauth-2025-04-20",
        "Content-Type": "application/json",
        "User-Agent": "claude-usage-script",
    })
    try:
        with urllib.request.urlopen(request, timeout=10) as response:
            return json.load(response), None
    except urllib.error.HTTPError as e:
        if e.code == 429:
            retry = e.headers.get("retry-after")
            return None, "rate limited" + (", retry in " + duration(int(retry)) if retry and retry.isdigit() else "")
        return None, "token rejected" if e.code in (401, 403) else "HTTP %d" % e.code
    except (OSError, ValueError) as e:
        return None, "offline" if isinstance(e, urllib.error.URLError) else type(e).__name__


def parse_time(stamp):
    try:
        return datetime.fromisoformat(stamp.replace("Z", "+00:00")).timestamp()
    except (AttributeError, ValueError):
        return None


def duration(seconds):
    seconds = max(int(seconds), 0)
    if seconds < 60:
        return "<1m"
    if seconds < 3600:
        return "%dm" % (seconds // 60)
    if seconds < 86400:
        return "%dh%02dm" % (seconds // 3600, seconds % 3600 // 60)
    if seconds < 10 * 86400:
        return "%dd%dh" % (seconds // 86400, seconds % 86400 // 3600)
    return "%dd" % (seconds // 86400)


def clock(t, now):
    """9:40pm within a day, Sat 9:40pm within the week, else Nov 5. (Built by hand:
    strftime's %-I and %-d, which drop the leading zero, don't exist on Windows.)"""
    d = datetime.fromtimestamp(t)
    time_of_day = "%d:%02d%s" % (d.hour % 12 or 12, d.minute, "am" if d.hour < 12 else "pm")
    if abs(t - now) < 86400:
        return time_of_day
    if abs(t - now) < 6 * 86400:
        return d.strftime("%a ") + time_of_day
    return d.strftime("%b ") + str(d.day)


def setup_console():
    """On Windows, make output UTF-8 (the bars and arrows aren't in the ANSI code page)
    and turn on the console's ANSI colour handling. Returns whether colours will work."""
    if os.name != "nt":
        return True
    for stream in (sys.stdout, sys.stderr):
        try:
            stream.reconfigure(encoding="utf-8", errors="replace")
        except (AttributeError, ValueError):
            pass
    try:
        import ctypes
        kernel32 = ctypes.windll.kernel32
        handle = kernel32.GetStdHandle(-11)  # standard output
        mode = ctypes.c_uint32()
        if not kernel32.GetConsoleMode(handle, ctypes.byref(mode)):
            return False  # not a console: output is going to a file or pipe
        return bool(kernel32.SetConsoleMode(handle, mode.value | 0x0004))  # virtual terminal processing
    except (AttributeError, OSError):
        return False


def bar(fraction, cells=10):
    """A bar `cells` wide, filled to `fraction` (0 to 1) in eighths of a cell."""
    fraction = max(0.0, min(1.0, fraction))
    eighths = int(round(fraction * cells * 8))
    if 0 < fraction < 1:
        eighths = min(max(eighths, 1), cells * 8 - 1)  # only exactly 0 or 1 looks empty or full
    return ("█" * (eighths // 8) + " ▏▎▍▌▋▊▉"[eighths % 8]).rstrip().ljust(cells, "░")


def urgency_color(fraction):
    """Colour for a time-until-reset bar `fraction` full: red as the reset nears,
    through orange and gold, to blue with most of the window to go."""
    step = min(int(max(0.0, fraction) * len(URGENCY_COLORS)), len(URGENCY_COLORS) - 1)
    if "256" in os.environ.get("TERM", "") or os.environ.get("COLORTERM"):
        return "38;5;%d" % URGENCY_COLORS[step]
    # A 16-colour terminal: the nearest basic colour.
    return "31" if step < 2 else "33" if step < 4 else "36" if step < 7 else "34"


def window_seconds(name):
    """How long a meter's window runs, from a limits[] row's group or kind or a quota
    field's name: 5 hours, a week, or None when it isn't known."""
    name = name or ""
    if name in ("session", "five_hour"):
        return 5 * 3600
    if name.startswith(("weekly", "seven_day")):
        return 7 * 86400
    return None


def plan_text(oauth):
    if not oauth:
        return "not signed in"
    plan = oauth.get("subscriptionType") or "?"
    tier = oauth.get("rateLimitTier") or ""
    for multiple in ("20x", "5x"):
        if tier.endswith("max_" + multiple):
            plan += " max " + multiple
    return plan


def row_label(row, short=False):
    scope = row.get("scope") or {}
    name = ((scope.get("model") or {}).get("display_name")
            or (scope.get("surface") or {}).get("display_name"))
    kind = row.get("kind") or "?"
    if kind == "session":
        return "5h" if short else "session (5h)"
    if kind == "weekly_all":
        return "week" if short else "week (all models)"
    if kind == "weekly_scoped" and name:
        return name if short else "week (%s)" % name
    return kind.replace("_", " ") + (" (%s)" % name if name else "")


def percent_left(used, resets_at, saved_at, now):
    """What's left of a meter, or None when it comes from a snapshot saved before the meter reset."""
    reset = parse_time(resets_at)
    if saved_at is not None and reset is not None and reset <= now:
        return None
    return max(0, min(100, 100 - (used or 0)))


def usage_rows(usage):
    """(label, percent used, resets_at, window, severity, note, labeled) for every meter in a usage body."""
    rows = [(row_label(r), r.get("percent"), r.get("resets_at"), window_seconds(r.get("group") or r.get("kind")),
             r.get("severity"), "", True)
            for r in usage.get("limits") or []]
    seen = {(r.get("percent"), r.get("resets_at")) for r in usage.get("limits") or []}
    # Other quota fields (five_hour, seven_day, codenamed meters); skip ones that
    # repeat a limits[] row.
    for key, value in usage.items():
        if not isinstance(value, dict) or value.get("utilization") is None:
            continue
        if (value["utilization"], value.get("resets_at")) in seen:
            continue
        note = []
        if value.get("limit_dollars") is not None:
            note.append("$%.2f of $%g used" % (value.get("used_dollars") or 0, value["limit_dollars"]))
        if value.get("locked_reason"):
            note.append("locked: %s" % value["locked_reason"])
        rows.append((key, value["utilization"], value.get("resets_at"), window_seconds(key), None,
                     " · ".join(note), False))
    return rows


def extra_usage_text(usage):
    extra = usage.get("extra_usage") or {}
    if not extra.get("is_enabled") and not extra.get("used_credits"):
        return ""
    scale = 10 ** (extra.get("decimal_places") or 2)
    currency = extra.get("currency") or "USD"
    used = (extra.get("used_credits") or 0) / scale
    text = "extra usage: %.2f %s used" % (used, currency)
    if extra.get("monthly_limit"):
        text += " of %.2f this month" % (extra["monthly_limit"] / scale)
    return text


def load_accounts(profiles):
    """Profiles grouped by the account they're signed in to, since they share its limits."""
    accounts = {}
    for profile in profiles:
        account, cache = read_config(profile)
        oauth = read_oauth(profile)
        key = account.get("accountUuid") or profile
        entry = accounts.setdefault(key, {"profiles": [], "email": account.get("emailAddress"),
                                          "oauth": None, "cache": None})
        entry["profiles"].append(profile)
        if cache and cache.get("accountUuid") not in (None, account.get("accountUuid")):
            cache = None  # saved while the profile was signed in to another account
        if oauth and (entry["oauth"] is None or oauth.get("expiresAt", 0) > entry["oauth"].get("expiresAt", 0)):
            entry["oauth"] = oauth
        if cache and (entry["cache"] is None or cache.get("fetchedAtMs", 0) > entry["cache"].get("fetchedAtMs", 0)):
            entry["cache"] = cache
    return list(accounts.values())


def account_usage(entry, cached=False):
    """(usage, saved_at, why): the live usage body with saved_at None, or else Claude Code's
    snapshot and when it was saved (both None without one) and why it isn't live."""
    oauth = entry["oauth"]
    if cached:
        why = "--cached"
    elif not oauth or not oauth.get("accessToken"):
        why = "not signed in"
    elif oauth.get("expiresAt", 0) / 1000 < time.time() + 60:
        why = "token expired; run claude in this profile to renew it"
    else:
        usage, why = fetch_usage(oauth["accessToken"])
        if usage is not None:
            return usage, None, None
    if entry["cache"] is None:
        return None, None, why
    return entry["cache"].get("utilization") or {}, entry["cache"].get("fetchedAtMs", 0) / 1000, why


def load_usage(profiles, cached=False):
    """[(account entry, (usage, saved_at, why))] for the accounts behind `profiles`, fetched in parallel."""
    entries = load_accounts(profiles)
    with ThreadPoolExecutor(max_workers=max(len(entries), 1)) as pool:
        return list(zip(entries, pool.map(lambda entry: account_usage(entry, cached), entries)))


def main():
    parser = argparse.ArgumentParser(
        description="Show how much of each Claude plan limit is left, for every ~/.claude* profile.")
    parser.add_argument("--cached", action="store_true",
                        help="don't call the API; show the snapshot Claude Code last saved")
    parser.add_argument("-e", "--email", action="store_true",
                        help="show each account's email address (hidden by default)")
    args = parser.parse_args()

    ansi = setup_console()
    color = sys.stdout.isatty() and not os.environ.get("NO_COLOR") and ansi
    paint = (lambda code, s: "\033[%sm%s\033[0m" % (code, s) if code else s) if color \
        else (lambda code, s: s)
    now = time.time()

    blocks = []
    for entry, (usage, saved_at, why) in load_usage(find_profiles(), args.cached):
        names = ", ".join("~" + p[len(HOME):] for p in entry["profiles"])
        if usage is None:
            source = paint("31", "no data") + paint("2", " (%s)" % why)
        elif saved_at is None:
            source = paint("32", "live")
        else:
            source = paint("33", "cached %s ago" % duration(now - saved_at)) + paint("2", " (%s)" % why)
        details = ([entry["email"] or "?"] if args.email else []) + [plan_text(entry["oauth"])]
        header = paint("1;36", names) + "  " + paint("2", " · ".join(details) + " · ") + source
        blocks.append((header, usage_rows(usage) if usage else [], extra_usage_text(usage or {}), saved_at))

    width = max([len(row[0]) for _, rows, _, _ in blocks for row in rows] or [0])
    unlabeled = False
    print(paint("1", "Claude plan usage by profile") + paint("2", " (% left, then time left until reset)"))
    for header, rows, extra, saved_at in blocks:
        print(header)
        for label, used, resets_at, window, severity, note, labeled in rows:
            t = parse_time(resets_at)
            left = percent_left(used, resets_at, saved_at, now)
            time_bar = " " * 10  # stays blank unless the reset time and the window's length are known
            if left is None:
                # The snapshot predates this window's reset, so its number is gone.
                pct, plan_bar, code = "   ?", "·" * 10, "2"
                when = "reset %s, after this snapshot" % clock(t, now)
            else:
                pct, plan_bar = "%3d%%" % left, bar(left / 100.0)
                code = SEVERITY_COLOR.get(severity, "" if labeled else "2")
                when = "resets %s (in %s)" % (clock(t, now), duration(t - now)) if t else ""
                if t and window:
                    to_go = (t - now) / window  # fraction of the window left
                    time_bar = paint(urgency_color(to_go) if labeled else "2", bar(to_go))
            tail = " · ".join(text for text in (when, note) if text)
            print(("  %s %s %s  %s %s" % (paint("" if labeled else "2", label.ljust(width)),
                                          paint(code, pct), paint(code, plan_bar), time_bar,
                                          paint("2", tail) if tail else "")).rstrip())
            unlabeled |= not labeled
        if extra:
            print("  " + paint("2", extra))
    if unlabeled:
        print(paint("2", "Dim rows are quota fields Claude Code doesn't display, shown by their raw names."))


if __name__ == "__main__":
    main()
