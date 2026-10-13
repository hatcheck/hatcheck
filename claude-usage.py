#!/usr/bin/env python3
"""Show how much of each Claude plan limit is left, for every ~/.claude* profile.

Usage:
    python3 claude-usage.py [--cached] [--email]

  --cached : don't call the API; show the snapshot Claude Code last saved.
  --email  : show each account's email address (hidden by default).

For each profile this reads the OAuth token Claude Code keeps in the macOS
Keychain and asks the endpoint /usage uses (api.anthropic.com/api/oauth/usage).
The token is used as is and never refreshed: a refresh rotates it and would
sign out the copy Claude Code holds. When the token has expired or the request
fails (the endpoint rate-limits busy accounts), the snapshot Claude Code cached
in the profile's .claude.json is shown instead, with its age.

Rows come from the server's limits[] list: the 5-hour session, the weekly limit
for all models, per-model weekly limits such as Fable, and any new meter the
server adds. Percentages are what's left (100 - used). Quota fields the server
sends that Claude Code doesn't display are listed under their raw names.
Profiles signed in to the same account are shown together.
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
    """The claudeAiOauth entry (token, expiry, plan) from the Keychain, or None."""
    try:
        out = subprocess.run(["security", "find-generic-password", "-s", keychain_service(profile), "-w"],
                             capture_output=True, text=True, timeout=15).stdout
        return json.loads(out).get("claudeAiOauth")
    except (OSError, subprocess.SubprocessError, ValueError, AttributeError):
        return None


def read_config(profile):
    try:
        with open(config_path(profile)) as fh:
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
    if abs(t - now) < 86400:
        fmt = "%-I:%M%p"
    elif abs(t - now) < 6 * 86400:
        fmt = "%a %-I:%M%p"
    else:
        fmt = "%b %-d"
    return datetime.fromtimestamp(t).strftime(fmt).replace("AM", "am").replace("PM", "pm")


def plan_text(oauth):
    if not oauth:
        return "not signed in"
    plan = oauth.get("subscriptionType") or "?"
    tier = oauth.get("rateLimitTier") or ""
    for multiple in ("20x", "5x"):
        if tier.endswith("max_" + multiple):
            plan += " max " + multiple
    return plan


def row_label(row):
    scope = row.get("scope") or {}
    name = ((scope.get("model") or {}).get("display_name")
            or (scope.get("surface") or {}).get("display_name"))
    kind = row.get("kind") or "?"
    if kind == "session":
        return "session (5h)"
    if kind == "weekly_all":
        return "week (all models)"
    if kind == "weekly_scoped" and name:
        return "week (%s)" % name
    return kind.replace("_", " ") + (" (%s)" % name if name else "")


def usage_rows(usage):
    """(label, percent used, resets_at, severity, note, labeled) for every meter in a usage body."""
    rows = [(row_label(r), r.get("percent"), r.get("resets_at"), r.get("severity"), "", True)
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
        rows.append((key, value["utilization"], value.get("resets_at"), None, " · ".join(note), False))
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


def main():
    parser = argparse.ArgumentParser(
        description="Show how much of each Claude plan limit is left, for every ~/.claude* profile.")
    parser.add_argument("--cached", action="store_true",
                        help="don't call the API; show the snapshot Claude Code last saved")
    parser.add_argument("-e", "--email", action="store_true",
                        help="show each account's email address (hidden by default)")
    args = parser.parse_args()

    color = sys.stdout.isatty() and not os.environ.get("NO_COLOR")
    paint = (lambda code, s: "\033[%sm%s\033[0m" % (code, s) if code else s) if color \
        else (lambda code, s: s)
    now = time.time()

    # One entry per account: profiles signed in to the same account share its limits.
    accounts = {}
    for profile in find_profiles():
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

    def live(entry):
        oauth = entry["oauth"]
        if args.cached:
            return None, "--cached"
        if not oauth or not oauth.get("accessToken"):
            return None, "not signed in"
        if oauth.get("expiresAt", 0) / 1000 < now + 60:
            return None, "token expired; run claude in this profile to renew it"
        return fetch_usage(oauth["accessToken"])

    entries = list(accounts.values())
    with ThreadPoolExecutor(max_workers=max(len(entries), 1)) as pool:
        results = list(pool.map(live, entries))

    blocks = []
    for entry, (usage, why) in zip(entries, results):
        names = ", ".join("~" + p[len(HOME):] for p in entry["profiles"])
        cached = usage is None and entry["cache"] is not None
        if usage is not None:
            source = paint("32", "live")
        elif cached:
            usage = entry["cache"].get("utilization") or {}
            age = duration(now - entry["cache"].get("fetchedAtMs", 0) / 1000)
            source = paint("33", "cached %s ago" % age) + paint("2", " (%s)" % why)
        else:
            source = paint("31", "no data") + paint("2", " (%s)" % why)
        details = ([entry["email"] or "?"] if args.email else []) + [plan_text(entry["oauth"])]
        header = paint("1;36", names) + "  " + paint("2", " · ".join(details) + " · ") + source
        blocks.append((header, usage_rows(usage) if usage else [], extra_usage_text(usage or {}), cached))

    width = max([len(row[0]) for _, rows, _, _ in blocks for row in rows] or [0])
    unlabeled = False
    print(paint("1", "Claude plan usage by profile") + paint("2", " (% left)"))
    for header, rows, extra, cached in blocks:
        print(header)
        for label, used, resets_at, severity, note, labeled in rows:
            t = parse_time(resets_at)
            if cached and t is not None and t <= now:
                # The snapshot predates this window's reset, so its number is gone.
                pct, bar, code = "   ?", "·" * 10, "2"
                when = "reset %s, after this snapshot" % clock(t, now)
            else:
                left = max(0, min(100, 100 - (used or 0)))
                filled = int(round(left / 10.0))
                pct, bar = "%3d%%" % left, "█" * filled + "░" * (10 - filled)
                code = SEVERITY_COLOR.get(severity, "" if labeled else "2")
                when = "resets %s (in %s)" % (clock(t, now), duration(t - now)) if t else ""
            tail = " · ".join(text for text in (when, note) if text)
            print("  %s %s %s%s" % (paint("" if labeled else "2", label.ljust(width)),
                                    paint(code, pct), paint(code, bar),
                                    " " + paint("2", tail) if tail else ""))
            unlabeled |= not labeled
        if extra:
            print("  " + paint("2", extra))
    if unlabeled:
        print(paint("2", "Dim rows are quota fields Claude Code doesn't display, shown by their raw names."))


if __name__ == "__main__":
    main()
