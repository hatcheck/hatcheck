"""npm run demo: the app against a demo home, to try it, Move and Fork included, without
touching your own profiles.

  npm run demo                 build the engine and the app, then start it
  npm run demo -- --no-build   start the last build as it is
  npm run demo -- --keep       keep the demo home when the app quits, and say where it is

The demo home is the command tests' throwaway home (tests/helpers.py, FakeHome): three Claude
Code profiles with saved usage, two Codex homes, sessions in two folders, and a folder bound to a
profile. The app runs with HOME, and the folders Windows and Linux keep data in, pointing into
it, so the engine reads and writes only there. Live usage finds no signed-in profile there, so it
shows what's saved. Resume, Sign in and Add one open your terminal, which runs in your real home,
and the Desktop tab's instances are apps in /Applications. The demo home is deleted when the app
quits, unless --keep."""
import argparse
import os
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
DESKTOP = os.path.dirname(HERE)
REPO = os.path.dirname(DESKTOP)
MAC, WINDOWS = sys.platform == "darwin", os.name == "nt"
EXE = ".exe" if WINDOWS else ""
RELEASE = os.path.join(DESKTOP, "src-tauri", "target", "release")
# What `tauri build` leaves: on macOS the app inside Hatcheck.app, so it runs as the bundle does,
# with the engine beside it, as in a release.
APP_DIR = os.path.join(RELEASE, "bundle", "macos", "Hatcheck.app", "Contents", "MacOS") if MAC else RELEASE
APP = os.path.join(APP_DIR, "hatcheck-desktop" + EXE)
ENGINE = os.path.join(APP_DIR, "hatcheck" + EXE)


def npm(*args):
    subprocess.run(["npm.cmd" if WINDOWS else "npm", *args], cwd=DESKTOP, check=True)


def build():
    npm("run", "engine")
    # Only the app: a .dmg or an installer takes longer and isn't needed to run it.
    npm("run", "tauri", "build", "--", *(["--bundles", "app"] if MAC else ["--no-bundle"]))


def make_home():
    """The demo home, made the way the command tests make theirs."""
    os.environ["HATCHECK_ENGINE"] = ENGINE  # the helpers build a debug engine without it
    sys.path.insert(0, os.path.join(REPO, "tests"))
    from helpers import NOW, FakeHome, iso

    def fable(used, days):
        return {"limits": [
            {"kind": "session", "group": "session", "percent": 10, "severity": "normal", "resets_at": iso(NOW + 3600)},
            {"kind": "weekly_scoped", "group": "weekly", "percent": used, "severity": "normal",
             "resets_at": iso(NOW + days * 86400), "scope": {"model": {"display_name": "Fable"}}}]}

    def codex_limits(used_5h, used_week):
        return {"limit_id": "codex", "limit_name": None, "plan_type": "plus",
                "primary": {"used_percent": used_5h, "window_minutes": 300, "resets_at": int(NOW + 2 * 3600)},
                "secondary": {"used_percent": used_week, "window_minutes": 10080, "resets_at": int(NOW + 3 * 86400)},
                "credits": {"has_credits": False, "unlimited": False, "balance": None}}

    h = FakeHome()
    work = h.profile("work", "acct-work", usage=fable(40, 2),
                     plan={"organizationType": "claude_team", "userRateLimitTier": "default_claude_max_5x"})
    personal = h.profile("personal", "acct-personal", usage=fable(95, 1), plan={"organizationType": "claude_pro"})
    # Saved two days ago, before its weekly limit reset: shown as not known.
    default = h.profile("default", "acct-default", saved_at=NOW - 2 * 86400, plan={"organizationType": "claude_team"},
                        usage={"limits": [{"kind": "weekly_all", "group": "weekly", "percent": 70,
                                           "resets_at": iso(NOW - 86400)}]})
    api, web = h.folder("code", "api"), h.folder("code", "web")
    h.session(work, api, "aaaaaaaa-0000-4000-8000-000000000001", "Add rate limiting to the login endpoint", 5)
    h.session(work, api, "aaaaaaaa-0000-4000-8000-000000000002", "Write the API docs", 180)
    h.session(personal, web, "bbbbbbbb-0000-4000-8000-000000000001", "Fix the flaky checkout test", 40)
    h.session(personal, api, "bbbbbbbb-0000-4000-8000-000000000002", "Look into the slow search query", 90,
              model="k3")  # its last reply came from Kimi
    h.session(default, web, "cccccccc-0000-4000-8000-000000000001", "Upgrade the build to Vite 8", 2 * 24 * 60)
    codex = h.codex_home("default", "acct-codex")
    h.codex_home("work", "acct-codex")  # the same ChatGPT account: a move there is a plain move
    h.codex_session(codex, api, "019f0000-aaaa-7000-8000-000000000001", "Speed up the build", 20,
                    rate_limits=codex_limits(40, 10), name="Faster build")
    # ~/code/api belongs to the work profile, so the personal session there is marked.
    subprocess.run([ENGINE, "bind", "work"], cwd=api, env=h.env(), check=True, capture_output=True)
    return h


def main():
    parser = argparse.ArgumentParser(prog="npm run demo --", description="Start the app against a demo home.")
    parser.add_argument("--no-build", action="store_true", help="start the last build as it is")
    parser.add_argument("--keep", action="store_true", help="keep the demo home when the app quits")
    args = parser.parse_args()
    if not args.no_build:
        build()
    if not os.path.isfile(APP) or not os.path.isfile(ENGINE):
        sys.exit("There's no build of the app to start: run npm run demo without --no-build.")
    h = make_home()
    env = h.env()  # HOME, and APPDATA and the like on Windows, in the demo home
    env["PATH"] = os.environ.get("PATH", "")  # the tests' is /bin alone on macOS
    if os.environ.get("SHELL"):
        env["SHELL"] = os.environ["SHELL"]  # the app asks your login shell for the PATH
    print("Demo home:", h.home)
    print("  ~/.claude-work      team max 5x; ~/code/api is bound to it, and to ~/.codex-work")
    print("  ~/.claude-personal  pro, nearly out of Fable, a session in ~/code/api")
    print("  ~/.claude           team, a session from two days ago")
    print("  ~/.codex            a Codex session; ~/.codex-work is on the same account")
    print("Hatcheck is in the menu bar (the tray on Windows and Linux). Quit it there, or press Ctrl+C here.")
    app = subprocess.Popen([APP], cwd=h.home, env=env)
    try:
        app.wait()
    except KeyboardInterrupt:
        app.terminate()
        app.wait()
    finally:
        if args.keep:
            print("The demo home stays at", h.home)
        else:
            h.cleanup()


if __name__ == "__main__":
    main()
