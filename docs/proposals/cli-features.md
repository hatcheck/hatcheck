# Proposal: the next four CLI features

**Status:** proposals, none of them built yet (October 2026).

Four additions to the `hatcheck` command. The first two are what a [desktop app](desktop-app.md) would
need, and both are useful without one.

1. [`--json` output](#1---json-output): the session list and plan usage as data, for scripts, status
   bars and the app.
2. [`hatcheck open`](#2-hatcheck-open): resume a session in a new terminal, already in its folder.
3. [Folder bindings](#3-folder-bindings-hatcheck-bind): tie a folder to a profile, so work happens
   under the right account.
4. [`hatcheck best`](#4-hatcheck-best): which account has the most left, when you ask.

## 1. `--json` output

**Problem:** the session list and the usage bars are made for people. A script, a status bar such as
SketchyBar or tmux's, or a desktop app would have to scrape the text, and the text changes.

**Proposal:** a `--json` option for `hatcheck` (the session list) and `hatcheck usage`, and later for
`hatcheck where` and `hatcheck move --dry-run`.

```console
$ hatcheck --json
{
  "version": 1,
  "generated_at": "2026-10-15T22:41:07Z",
  "hours": 24,
  "this_folder_only": false,
  "sessions": [
    {
      "agent": "claude",
      "id": "3f1c2a9e-8b7d-4c1e-9f2a-6d5e4c3b2a10",
      "profile": "/Users/me/.claude-work",
      "account": 0,
      "title": "Fix flaky login test",
      "cwd": "/Users/me/code/api",
      "last_active": "2026-10-15T22:37:02Z",
      "entrypoint": "cli",
      "branch": "main",
      "model": "claude-fable-5-1",
      "open": {"pid": 48213, "status": "busy"},
      "resume": "cl work --resume 3f1c2a9e-8b7d-4c1e-9f2a-6d5e4c3b2a10"
    }
  ],
  "accounts": [
    {
      "agent": "claude",
      "profiles": ["/Users/me/.claude-work"],
      "plan": "team max 5x",
      "source": "live",
      "saved_at": null,
      "limits": [
        {"name": "session (5h)", "short": "5h", "used_percent": 20, "left_percent": 80,
         "resets_at": "2026-10-16T01:30:00Z", "window_seconds": 18000, "severity": "normal",
         "per_model": false}
      ]
    }
  ],
  "hidden": {"Desktop": 3}
}
```

- **One call for an app:** the session list includes the accounts behind the listed sessions, as the
  comments do now; `--no-usage` leaves them out. `hatcheck usage --json` gives every account.
- **Same choices as the text:** the time window, `--all`, `--all-folders` and `-n` pick the same
  sessions, and `resume` is the same command the text prints, including `cl -k` for a session last
  on Kimi.
- **Stable:** absolute paths rather than `~`, times in UTC (ISO 8601), no colours, and `version`
  goes up only when a field changes meaning or goes away. New fields can appear at any time.
- **Errors** stay on stderr with a non-zero exit, so stdout is always valid JSON.
- **Email addresses** only with `--email`, as in the text.

## 2. `hatcheck open`

**Problem:** resuming takes two steps, `cd` to the session's folder and then the resume command, and
from a desktop app there's no terminal to type them in.

**Proposal:**

```console
$ hatcheck open 3f1c2a9e        # a new terminal window in ~/code/api, running cl work --resume …
$ hatcheck open                 # the newest session in this folder, in any profile or home
$ hatcheck open 3f1c2a9e --here # in this terminal instead
$ hatcheck open 3f1c2a9e --print
cd ~/code/api
cl work --resume 3f1c2a9e-8b7d-4c1e-9f2a-6d5e4c3b2a10
```

- **Finding it:** by ID or the start of it, in Claude Code profiles and Codex homes, as
  `hatcheck where` does. With no ID, the newest session started from the current folder.
- **The command:** the one the session list prints: `cl`/`claude` or `cx`/`codex`, with `-z` or
  `-k` for a session whose last reply came from Z.ai or Kimi.
- **Already open:** refused, as `hatcheck move` refuses, so a session isn't resumed twice: Claude
  Code's open sessions from its process registry, Codex's from the lock Codex holds on a session it's
  writing.
- **Which terminal:**
  - macOS: Terminal, or the app `hatcheck` runs in when it's iTerm2, Ghostty or WezTerm (from
    `TERM_PROGRAM`), driven by AppleScript or the terminal's own command line. Driving Terminal
    or iTerm2 needs macOS's one-time permission for automation.
  - Windows: Windows Terminal (`wt -d <folder>`), otherwise a new PowerShell window.
  - Linux: `$TERMINAL`, otherwise `x-terminal-emulator`, GNOME Terminal or Konsole.
  - `--terminal NAME`, or `HATCHECK_TERMINAL`, picks one.
- **`--here`** runs it in the current terminal, as `hatcheck move` does when you answer `y`.
- **`--print`** prints the two lines, quoted for the current shell, for scripts and the desktop app's
  Copy command.

## 3. Folder bindings (`hatcheck bind`)

**Problem:** with a work and a personal account, it's easy to start `claude` in a work repo out of
habit. The session then runs on the personal account and is filed in the personal profile.

**Proposal:**

```console
$ cd ~/code/api
$ hatcheck bind work            # this folder and everything under it: ~/.claude-work, ~/.codex-work
$ hatcheck bind work --codex personal   # a different Codex home
$ hatcheck bind --list
$ hatcheck unbind
```

- **Where it's kept:** in your own config, `~/.config/hatcheck/bindings.json`
  (`%APPDATA%\hatcheck\bindings.json` on Windows), not in the repo, so profile names don't end up in
  git. The deepest bound folder wins, and folders are compared after resolving symlinks.
- **`cl` and `cx` use it:** without a profile name in a bound folder, `cl --resume <id>` means
  `cl work --resume <id>`. Today `cl` needs a profile name, so nothing that works now changes.
- **A warning when it's ignored:** an opt-in `hatcheck bind --hook` adds a SessionStart hook to each
  profile's `settings.json`. When a session starts in a bound folder under another profile, it shows
  a warning that names the folder's profile and the `cl` command to use. It warns and doesn't block.
  Whether Codex's `hooks.json` can do the same is still to be checked.
- **The session list** marks sessions that ran in a bound folder under another profile, so they're
  easy to spot and `hatcheck move` them.

## 4. `hatcheck best`

**Problem:** choosing an account for the next task, or one to carry a session over to, means reading
every account's bars and doing the sums.

**Proposal:**

```console
$ hatcheck best
Most left now (Claude Code):
  1. work      5h 80% left · week 64% · Fable 52%    → cl work
  2. personal  5h 31% left · week 90%                → cl personal
  3. default   week 4% left, resets Fri 8:20am
Most left now (Codex):
  1. ~/.codex  month 100% left                       → codex
$ hatcheck best --model fable    # count the Fable limit too
$ hatcheck best --for 3f1c2a9e   # where to carry this session: prints the hatcheck move command
```

- **Ranking:** by the tightest limit that applies, the smallest % left among the 5-hour, weekly and
  (with `--model`) per-model limits, with the sooner reset first on a tie. The numbers come from
  `hatcheck usage`, live when it can, and a snapshot's age is shown when it isn't live.
- **Advice only.** It runs when you ask, prints commands, and never starts, moves or switches
  anything by itself. Nothing in Hatcheck rotates accounts in the background.
- **The terms still apply.** Using several accounts to get around one plan's limits may break
  Anthropic's or OpenAI's terms. This is for choosing between accounts you may use for the work at
  hand, such as your own and your company's.

## Not a CLI feature, but next too

Publish the CLI on PyPI (`uv tool install hatcheck`) and Homebrew, so installing doesn't need a clone.
