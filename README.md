# Multiple Claude accounts — isolated Desktop instances and Claude Code CLI profiles

Run **two (or more) fully isolated Claude Desktop instances** side by side on macOS — e.g. one
signed into your **personal** account and one into your **work** account — each with its own
**color-coded Dock launcher** that **focuses the existing window instead of spawning duplicates**.

This builds on Philipp Stracker's excellent
[`--user-data-dir` approach](https://gist.github.com/stracker-phil/9f84927a556632c7f9cc06663b534f14)
([write-up](https://philippstracker.com/multiple-claude-instances/)) and adds two things:

1. **Recolored icons** so you can tell the launchers apart at a glance (blue work / red personal / etc.).
2. A **smart, self-contained launcher** that fixes the "every click opens another window" problem.

It also has **command-line helpers for Claude Code** (the `claude` CLI) for keeping one profile per
account there: `cl` runs Claude Code with a profile, `claude-where` finds any session, two scripts
list recent sessions and plan usage across all profiles, and `claude-move-session` carries a session
over to another profile. `claude-z` and `claude-k` run it on Z.ai's GLM or Kimi's models instead, in
any profile. See [Claude Code CLI profiles](#claude-code-cli-profiles).

|                          | macOS | Linux | Windows |
|--------------------------|:-----:|:-----:|:-------:|
| Claude Desktop instances |  yes  | no official Desktop app | [planned](#roadmap) |
| Claude Code CLI tools    |  yes  |  yes  |   yes   |

> ## ⭐ Recommended: give *every* account its own isolated instance
>
> The most reliable setup is to create a **separate, dedicated instance for each account —
> including your "personal" one — and stop using Claude's built-in default profile for day-to-day
> use.** Don't try to make one launcher target the shared default profile.
>
> Why: Claude has **no per-profile single-instance lock**, so two processes can run on one profile
> at once — and **only the *first* instance on a profile can use its login database. Every
> additional instance on that same profile shows up logged out.** If a "personal" launcher points
> at the default profile (which the stock Claude icon also uses), you end up with duplicate,
> logged-out windows. Giving each account its own dir guarantees **one profile → one instance →
> always logged in**, and makes the focus-don't-duplicate logic rock-solid.
>
> Already logged into the default profile and don't want to re-auth? See
> [Migrating an existing login](#migrating-an-existing-login-optional) — you can copy your current
> profile into the new dedicated dir.

---

## How it works

Claude Desktop is an Electron app. Electron's `--user-data-dir` flag points the app at a
completely separate profile folder (config, sessions, **auth tokens**, MCP servers, Cowork data):

```bash
open -n -a "/Applications/Claude.app" --args --user-data-dir="$HOME/.claude-instances/work"
```

A tiny macOS `.app` "wrapper" just runs that command, so it shows up in Launchpad/Dock like a
normal app. Each wrapper stores its instance name in `Contents/MacOS/config.sh` and runs
`Contents/MacOS/claude-launcher`.

### The duplicate problem (and the fix)

The naive launcher uses `open -n` — the `-n` flag means *"open a NEW instance, even if one is
already running."* Claude has **no per-profile single-instance lock**, so **every click spawns
another process** against the same profile dir. That stacks up duplicate windows and risks
corrupting the shared profile.

The **smart launcher** in this repo fixes it. On click it inspects *that specific instance*
(matched by `--user-data-dir`; for the default instance, the main process that has *no*
`--user-data-dir`) and does one of three things:

- **Running with a window** → focuses that process's window by PID via System Events. No duplicate.
- **Running but windowless** → opens a fresh window. Claude keeps its main process alive after you
  close the window, and a windowless instance *cannot* be re-shown per-instance (all instances share
  one bundle id, so `open -a` / "activate" can't target it). We deliberately **do not kill** the
  windowless process: it is often windowless precisely because it's still doing background work
  (e.g. a chat that's "thinking"), and killing it would lose that work. The leftover windowless
  process is harmless and goes away when you fully **Quit (⌘Q)** Claude.
- **Not running** → launches fresh with `open -n`.

How "has a window?" is detected: a live window shows up as one or more `--type=renderer` processes
for that profile; closing the window drops that count to 0.

So "always click the launcher you want" reliably **focuses-or-launches** the right instance, even
after you've closed its window.

> **Tip:** to keep the focus-don't-relaunch behavior working, **close Claude with ⌘W** (close
> window, app keeps running) when you want it to stay live, and **⌘Q** when you truly want it gone.

### What the colors do (and don't do)

The recolored icon lives on the **launcher you click**. Because all instances share the one
`/Applications/Claude.app` binary, **while running they all show the same stock Dock icon and the
name "Claude"** — the color only marks the launcher. The reliable in-app tell is the signed-in
account. (Truly distinct *running* Dock icons would require a full standalone copy of Claude.app.)

---

## Quick start

```bash
# 0. Have Claude Desktop installed at /Applications/Claude.app  (https://claude.ai/download)

# 1. Clone this repo (or download the scripts), then make the helper executable
chmod +x make-claude-instance.sh

# 2. Create a WORK instance (isolated profile) with a BLUE launcher
./make-claude-instance.sh work Work 135

# 3. Create a PERSONAL instance (its own isolated profile) with a RED-ORANGE icon
./make-claude-instance.sh personal Personal -7

# 4. Launch from Launchpad / drag to Dock:  "Claude Work"  and  "Claude Personal"
#    First launch of each is a fresh profile -> sign into the right account once.
```

- The **first** time a launcher focuses a running window, macOS asks
  *"Claude … wants to control System Events"* → click **OK** (one time per launcher).
- **Give every instance its own isolated dir** — use a real name like `personal` / `work`, **not**
  `default`. One profile → one instance (see the ⭐ callout above for why).
- The instance name **`default`** is still supported (it uses Claude's built-in profile with no
  `--user-data-dir`) for folks who want one launcher onto the stock profile — but it's only safe if
  that profile is *never* run by more than one instance at a time (i.e. don't also use the stock
  Claude icon). A dedicated dir is the robust, recommended choice.

### Color cheatsheet (hue shift, in 0–255 = 0–360° units)

| Look          | hueShift | satGain (optional) |
|---------------|---------:|-------------------:|
| Stock orange  | `0`      | `1.0`              |
| Red-orange    | `-7`     | `1.08`             |
| Green         | `+80`    | `1.0`              |
| Blue          | `+135`   | `1.0`              |
| Purple        | `+170`   | `1.0`              |

Icon recoloring is **optional** — it only runs if Python 3 + [Pillow](https://pypi.org/project/Pillow/)
are available (`pip install Pillow`). Without them you still get a fully working launcher with the
stock icon.

---

## Migrating an existing login (optional)

If you're already signed into Claude's **default** profile and want your new dedicated instance to
start out logged in (with your settings, MCP servers, and sessions intact), copy the profile across
**while Claude is fully quit** so the copy is consistent:

```bash
# 1. Quit Claude completely first (⌘Q every window; make sure nothing is running):
ps -axo command | grep "[C]laude.app/Contents/MacOS/Claude"   # should print nothing

# 2. Copy the default profile into your new dedicated dir, skipping lock files + caches:
rsync -a \
  --exclude 'Singleton*' \
  --exclude 'Cache' --exclude 'Code Cache' --exclude 'GPUCache' \
  --exclude 'DawnGraphiteCache' --exclude 'DawnWebGPUCache' --exclude 'ShaderCache' \
  --exclude 'Crashpad' --exclude 'blob_storage' \
  "$HOME/Library/Application Support/Claude/"  "$HOME/.claude-instances/personal/"
```

Your original default profile is left untouched (the stock Claude icon still opens it). The auth
token lives in the copied profile (and/or the shared macOS Keychain), so the new instance should
launch already signed in. If it doesn't, just sign in once — it sticks from then on.

> Heads-up on size: the local-agent / Cowork runtime lives in a `vm_bundles/` folder that can be
> **many GB**. The copy above includes it. To save disk, you can `rm -rf` the `vm_bundles` folder in
> the new dir afterward — it re-downloads on demand the next time you use that feature.

## Managing instances

```bash
# List running Claude instances and their profiles
ps -axo pid,command | grep "[C]laude.app/Contents/MacOS/Claude"

# Remove a wrapper + its profile
rm -rf "/Applications/Claude Work.app"
rm -rf "$HOME/.claude-instances/work"
```

---

## Claude Code CLI profiles

Claude Code (the `claude` CLI) keeps its settings, history and session transcripts in `~/.claude`
(plus `~/.claude.json`), and its login in the macOS Keychain, or on Linux and Windows in
`~/.claude/.credentials.json`. Point `CLAUDE_CONFIG_DIR` at another folder and you get a separate
profile with its own login, so you can keep one per account:
`~/.claude-work`, `~/.claude-personal`, and so on. These are separate from the Desktop instances
above, though sessions from Desktop's Code tab can be filed in them too (the session list below
hides those unless you pass `--all`).

### Install

On macOS and Linux:

```bash
./install.sh               # link the commands below into ~/.local/bin
./install.sh --bin ~/bin   # or into another folder on your PATH
./install.sh --uninstall   # remove the links again
```

The commands are symlinks to this repo, so a `git pull` updates them. The installer doesn't edit
your shell startup files: if the folder isn't on your `PATH`, it prints the line to add. A file that
already has one of these names is left alone unless you pass `--force`, except the copies of
`claude-z` and `claude-k` that the [claude-z](https://github.com/yiwang/claude-z) installers made,
which these replace. `claude-recent-sessions`, `claude-usage`, `claude-move-session` and
`claude-provider` need Python 3.9 or later (on macOS, `xcode-select --install` provides it).

On Windows, in PowerShell:

```powershell
.\install.ps1               # add .cmd launchers for the commands below to ~\.local\bin
.\install.ps1 -Bin D:\bin   # or to another folder on your PATH
.\install.ps1 -Uninstall    # remove them again
```

The launchers run the scripts in this repo, so a `git pull` updates them, and they work from
PowerShell and `cmd` alike, whatever the script execution policy. You need Python 3.9 or later
(python.org, or `winget install Python.Python.3.13`). If PowerShell won't run `install.ps1`, start
it with `powershell -ExecutionPolicy Bypass -File install.ps1`. On Windows, `claude-recent-sessions`
prints `cd` and resume commands for PowerShell. Paste them into PowerShell, not `cmd`, because
`cmd` doesn't treat the trailing `# usage` as a comment. Inside a Visual Studio developer prompt,
the MSVC compiler is also called `cl`, and whichever comes first on `PATH` wins.

#### Or as shell functions

`cl` and `claude-where` are small enough to keep as shell functions instead. Add this to
`~/.zshrc` (or `~/.bashrc`):

```bash
# cl work --resume <id>  ==  CLAUDE_CONFIG_DIR=~/.claude-work claude --resume <id>
# cl -k work ... runs the installed claude-k instead (-z: claude-z)
cl() {
  local run=claude
  case $1 in -z|-k) run=claude$1; shift ;; esac
  [ -n "$1" ] || { echo "usage: cl [-z|-k] <profile> [claude args...]  (uses ~/.claude-<profile>)" >&2; return 2; }
  CLAUDE_CONFIG_DIR="$HOME/.claude-$1" "$run" "${@:2}"
}

# claude-where 3f1c2a9e   ← a session ID, or a prefix of one
claude-where() {
  find ~/.claude ~/.claude-* -maxdepth 3 -path '*/projects/*' -name "$1*" 2>/dev/null
}
```

A shell function takes precedence over an installed command with the same name, and both do the
same thing.

### `cl`: run Claude Code with a profile

```bash
cl work                           # Claude Code with ~/.claude-work (sign in on first run)
cl work --resume <session-id>     # resume one of that profile's sessions
cl personal --continue            # any claude option works
cl -k work --resume <session-id>  # that session on Kimi; -z for Z.ai (see below)
```

Plain `claude` keeps using `~/.claude`. Claude Code names each profile's Keychain entry after its
`CLAUDE_CONFIG_DIR` value, so keep one spelling of each path; `cl` always uses `~/.claude-<name>`.

### `claude-z`, `claude-k`: Claude Code on Z.ai or Kimi

`claude-z` and `claude-k` start Claude Code on [Z.ai](https://z.ai)'s GLM models or
[Kimi](https://www.kimi.com/code)'s, with your plan there, instead of on Anthropic's. `cl -z <name>` and
`cl -k <name>` do the same in a profile, so a session can switch between Claude and those models and
stay in its profile:

```bash
claude-k reconfig                   # once: your Kimi API key and model (claude-z reconfig for Z.ai)
claude-k                            # Claude Code on Kimi, with ~/.claude
cl -k work --resume <session-id>    # carry on a work-profile session on Kimi…
cl work --resume <session-id>       # …and later back on the work account's Claude plan
```

- Setup asks for the API key (from [z.ai](https://z.ai/manage-apikey/apikey-list) or the
  [Kimi Code Console](https://www.kimi.com/code)), the model, and an optional cap on output tokens.
  It saves them in `~/.config/claude-z/config` or `~/.config/claude-k/config` (`%APPDATA%\claude-z\config`
  on Windows), the files the [claude-z](https://github.com/yiwang/claude-z) launchers use, so a setup
  made with those works as is. `claude-provider` shows what's set up.
- Models: on Z.ai `glm-5.3[1m]` (the default, with `glm-5.3-flash` for quick tasks), `glm-5.3-flash`
  or `glm-4.7`; on Kimi `k3[1m]` (the default), `k3`, `kimi-for-coding` or
  `kimi-for-coding-highspeed`. `[1m]` turns on the 1M-token context window.
- The key reaches Claude Code as `ANTHROPIC_AUTH_TOKEN`, which it always uses ahead of the profile's
  claude.ai login and never asks about, and `ANTHROPIC_API_KEY` is cleared. Claude Code asks once per
  profile whether to use an `ANTHROPIC_API_KEY`, with No preselected, and after a No it sends the
  profile's claude.ai login token to the provider instead.
- The model is pinned with `ANTHROPIC_MODEL`, so a `"model"` in the profile's `settings.json`, such
  as `"fable"`, can't send a Claude model name to Z.ai or Kimi.
- Resuming a session on another provider sends it the whole conversation so far, so check that's
  allowed before doing it with a work account's sessions.
- `claude-usage` and the comments in `claude-recent-sessions` show Claude plans only.

Coming from the claude-z repo: `./install.sh` replaces the copies its installers put in
`~/.local/bin`, and your config stays. On Windows, delete `%LOCALAPPDATA%\claude-z` (and `\claude-k`)
and take it off your `PATH` by hand, but keep `%APPDATA%\claude-z`, which holds your key; that repo's
uninstallers delete both. The permission mode its installers asked for isn't used (the old launchers
only ever applied `dangerously-skip-permissions`): set `permissions.defaultMode` in the profile's
`settings.json`, or pass `--permission-mode`. OpenRouter (`claude-o`) isn't included.

### `claude-where`: find a session

```console
$ claude-where 3f1c2a9e
/Users/me/.claude-work/projects/-Users-me-code-api/3f1c2a9e-8b7d-4c1e-9f2a-6d5e4c3b2a10.jsonl
```

The profile in the path tells you how to resume it (`claude`, or `cl work` here), and the folder
under `projects/` is the directory the session ran in, with every non-alphanumeric character turned
into `-`. A session with subagents or large tool results also has a data folder of the same name,
which is listed too.

### `claude-recent-sessions`: what to resume

Lists the Claude Code sessions from every `~/.claude*` profile that were active in the last 24 hours
or are still open, grouped by folder and newest first. Run from a folder that has sessions, it lists
just that folder's (`--all-folders` lists them all). Each one comes with the command that resumes it
and what's left of that profile's plan:

```
$ claude-recent-sessions
Claude Code CLI sessions active in the last 24h or open now (by folder, newest first; % = plan left, ↻ = hours/days until reset)
cd ~/code/api
    4m  Fix flaky login test · main · ● open in pid 48213 (busy)
        cl work --resume 3f1c2a9e-8b7d-4c1e-9f2a-6d5e4c3b2a10      # 5h 100% █████               Fable 48% ███▍░░░ ↻████▎░░ 4d6h
    1h  Refactor the auth middleware · main · on k3
        cl -k work --resume 2d4f6a8c-1e3b-4d5f-9a7c-0b2d4e6f8a1c
    3h  Add rate limiting to the login endpoint · feat/rate-limit
        claude --resume 9a8b7c6d-5e4f-4a3b-8c2d-1e0f9a8b7c6d       # 5h  62% ███▏░ ↻██▋░░ 2h41m  week  91% ██████▍ ↻███▏░░░ 3d2h
cd ~/code/blog
   20h  Draft post on Rust lifetimes · main
        cl personal --resume c47e2b1d-0a9f-4e8d-b7c6-5d4e3f2a1b0c  # 5h    ? ·····               week  97% ██████▊ ↻█████▏░ 5d2h
cd ~/notes
    6h  Plan next sprint · main
        cl work --resume 5b6c7d8e-9f0a-4b1c-8d2e-3f4a5b6c7d8e      # 5h 100% █████               Fable 48% ███▍░░░ ↻████▎░░ 4d6h
    9h  Write release notes · main
        cl work --resume 7e8f9a0b-1c2d-4e3f-8a4b-5c6d7e8f9a0b
Profiles: ~/.claude (1), ~/.claude-personal (1), ~/.claude-work (4)
Usage from Claude Code's last snapshot (token expired; run claude in this profile to renew it): ~/.claude-personal 9h ago
```

- Run the folder's `cd` line, then the session's command. Sessions still open in a terminal are
  marked `● open`, so you don't resume one twice.
- The comment after each command shows each plan limit: the 5-hour session (`5h`), the weekly limit
  (`week`) and per-model weekly limits such as `Fable`. Each one has the % left and a bar of it, then
  `↻`, a bar of the time left until it resets and that time. Bars have a block per hour of the
  5-hour window and per day of a weekly one, as in `↻████▎░░ 4d6h`.
- Time bars are coloured by urgency: blue with most of the window to go, then gold and orange, and
  red as the reset nears.
- A plan bar shorter than its time bar means that limit is being used up faster than its window runs
  out, like `Fable` above: 48% left with more than half the week to go. `?` means the limit has
  reset since the saved snapshot it came from.
- A command without the comment, like the second one in `~/notes`, uses the same account as the
  command above it, even through another profile, so its usage is the same.
- A session whose newest reply came from Z.ai or Kimi shows that model, like `on k3` above, and its
  command resumes it there (`claude-k` for `~/.claude`). Use `cl work` instead to go back to Claude;
  its comment is still the profile's Claude plan.
- Since the usage is a shell comment, pasting the whole line runs just the command (zsh needs
  `setopt interactivecomments`).
- Usage is fetched live, one request per account. When that fails (expired token, rate limit), Claude
  Code's last saved snapshot is used instead and named at the bottom with its age.

| Option       | What it does                                                                  |
|--------------|-------------------------------------------------------------------------------|
| `HOURS`      | how far back to look (default 24); takes `36`, `1.5`, `12h` or `3d`           |
| `-n N`       | show only the newest N sessions per folder and count the rest                 |
| `--all`      | also list sessions last used from Claude Desktop's Code tab or the SDK        |
| `--all-folders` | list every folder, even when run from one that has sessions                |
| `--full`     | print `CLAUDE_CONFIG_DIR=~/.claude-<name> claude …` instead of `cl`          |
| `--cached`   | take usage from Claude Code's saved snapshots, without calling the API        |
| `--no-usage` | leave plan usage out                                                          |

### `claude-usage`: plan usage for every profile

```
$ claude-usage
Claude plan usage by profile (% left, then time left until reset)
~/.claude  pro · live
  session (5h)       62% ██████▎░░░  █████▍░░░░ resets 5:01pm (in 2h41m)
  week (all models)  91% █████████▏  ████▍░░░░░ resets Sat 4:20pm (in 3d2h)
~/.claude-personal  pro · cached 9h00m ago (token expired; run claude in this profile to renew it)
  session (5h)         ? ··········             reset 10:20am, after this snapshot
  week (all models)  97% █████████▊  ███████▎░░ resets Mon 4:20pm (in 5d2h)
~/.claude-work  team max 5x · live
  session (5h)      100% ██████████
  week (Fable)       48% ████▊░░░░░  ██████▏░░░ resets Sun 8:20pm (in 4d6h)
```

- Each limit has two bars: what's left of it, then how much of its window, 5 hours or a week, is
  left before it resets. In a terminal the second goes from blue with most of the window to go,
  through gold and orange, to red as the reset nears. When the first bar is shorter than the
  second, as for `Fable` here, the limit is being used up faster than its window runs out. A limit
  without a reset time, like an unused 5-hour session, has no time bar.
- It reads each profile's OAuth token from the macOS Keychain, or from the profile's
  `.credentials.json` on Linux and Windows, and asks the same endpoint as Claude Code's `/usage`. The token is only read, never refreshed: a refresh rotates it and would sign Claude Code
  out of that profile.
- When a token has expired or the endpoint rate-limits the account, Claude Code's last saved snapshot
  is shown with its age. Run `claude` (or `cl <name>`) in that profile to renew the token.
- Profiles signed in to the same account are shown together. Quota fields the server sends that
  Claude Code doesn't display are listed dimmed under their raw names.
- `--cached` skips the API. `-e` / `--email` adds each account's email address, which is hidden by
  default so the output is safe to share.

### `claude-move-session`: carry on a session with another profile

When one account runs low, continue a session on another profile. It copies the session by default;
`--no-keep` deletes the original once the copy is checked, which moves it.

```
$ claude-move-session 9a8b7c6d work
Copied session 9a8b7c6d "Add rate limiting to the login endpoint" from ~/.claude to ~/.claude-work:
  transcript                 53.0 KB
  subagents and tool output  3 files
  /rewind file snapshots     4 files
  plan                       quiet-river-plan.md
Paths in the conversation to these files now point to the copies.
Kept the original in ~/.claude; --no-keep deletes it once the copy is checked.
Resume it with:
  cd ~/code/api
  cl work --resume 9a8b7c6d-5e4f-4a3b-8c2d-1e0f9a8b7c6d
Resume it now in ~/.claude-work? [y/N]
```

- It copies the transcript, the folder of subagent transcripts and saved tool output, the `/rewind`
  file snapshots and the plan file, keeping their timestamps, and checks every copied file. Paths in
  the conversation that point at those files are changed to point at the copies, except inside
  Claude's thinking, whose signatures would stop matching.
- A session that's open in a terminal is refused: quit it first, so it stops changing.
  `claude-recent-sessions` marks those `● open`.
- Resume it with the printed command, from the directory it ran in, or answer `y` when it asks and
  it starts Claude Code there with the new profile. It only asks in a terminal, so scripts never
  wait on it. The conversation comes across whole; the account and its plan limits, settings,
  permissions, MCP servers, plugins and project memory are the new profile's. If the new account
  rejects the signatures on earlier thinking, Claude Code drops those thinking blocks and carries
  on.
- Plan files stay in the old profile even with `--no-keep`, since sessions can share one.
- If the session's last reply came from Z.ai or Kimi, it also prints the `cl -z` or `cl -k` command
  that carries on there instead of on the new profile's Claude plan.

| Option           | What it does                                                                 |
|------------------|------------------------------------------------------------------------------|
| `SESSION`        | the session ID, or the start of it                                           |
| `PROFILE`        | where it goes: `NAME` for `~/.claude-NAME`, `default` for `~/.claude`, or a path |
| `--from PROFILE` | the profile to take it from, when more than one has it                       |
| `--no-keep`      | delete the original once the copy is checked, which moves it                 |
| `--dry-run`      | show what would be copied and deleted, and change nothing                    |

## Files in this repo

Claude Desktop instances (macOS):

- **`make-claude-instance.sh`** — builds the `.app` wrapper (Info.plist, config, smart launcher) and optionally recolors the icon.
- **`claude-launcher`** — the self-contained smart launcher template (embedded into each wrapper).
- **`recolor-icon.py`** — hue/saturation recolor of the `.icns` (used by the helper; needs Pillow).

Claude Code CLI profiles (macOS, Linux, Windows):

- **`install.sh`** / **`install.ps1`** — put the commands below on your `PATH` (`~/.local/bin` by default).
- **`cl`** / **`cl.ps1`** — runs `claude` with a `~/.claude-<name>` profile, on Z.ai or Kimi with `-z` or
  `-k`; installed as `claude-z` and `claude-k` too.
- **`claude-provider.py`** — the Z.ai and Kimi keys and models behind `claude-z`, `claude-k`, `cl -z` and
  `cl -k`.
- **`claude-where`** / **`claude-where.ps1`** — finds a session's files by ID or ID prefix.
- **`claude-recent-sessions.py`** — recent sessions across profiles, with resume commands and plan usage.
- **`claude-usage.py`** — plan usage left for every profile.
- **`claude-move-session.py`** — copies a session to another profile, or moves it with `--no-keep`.

`claude-recent-sessions.py` loads `claude-usage.py`, and `claude-move-session.py` loads both, so keep
the three together. The `.ps1` files are the Windows versions of the `sh` ones.

- **`tests/`** — tests for the CLI tools against throwaway home folders:
  `python3 -m unittest discover -s tests`. They never read your real profiles, Keychain or the network.
  GitHub Actions runs them on macOS, Linux and Windows (`.github/workflows/tests.yml`).

## Roadmap

- **Claude Desktop instances on Windows.** Claude Desktop is an Electron app there too, so a
  separate `--user-data-dir` per account should work. A Windows version would create a shortcut per
  instance with its own icon, and a launcher that brings a running instance forward instead of
  opening a duplicate. It needs testing on a Windows machine first.
- **Linux.** There's no official Claude Desktop for Linux, so only the CLI tools apply there.

## Credits

- **Claude Desktop instances:** the `make-claude-instance.sh`, `claude-launcher` and
  `recolor-icon.py` scripts, and the Desktop sections of this README, are by
  [Aaron Oxborrow](https://github.com/aoxborrow), from his
  [gist](https://gist.github.com/aoxborrow/5a3a0ac0aa37819cc19439b70a552221) that this repo started
  as a fork of. They build on the core `--user-data-dir` isolation technique from
  [Philipp Stracker's gist](https://gist.github.com/stracker-phil/9f84927a556632c7f9cc06663b534f14)
  ([write-up](https://philippstracker.com/multiple-claude-instances/)), itself a cleaned-up version
  of [weidwonder/claude-desktop-multi-instance](https://github.com/weidwonder/claude-desktop-multi-instance).
- **Claude Code CLI tools:** `cl`, `claude-where`, `claude-recent-sessions`, `claude-usage`,
  `claude-move-session`, `claude-provider`, the installers and the tests are by
  [Yi Wang](https://github.com/yiwang).
- **Z.ai and Kimi:** the provider settings in `claude-provider.py` (endpoints, model slots, context
  windows and the config files) come from [claude-z](https://github.com/sam-fakhreddine/claude-z) by
  Kerim ([lettt325](https://github.com/lettt325)), MIT-licensed, and Yi Wang's
  [fork](https://github.com/yiwang/claude-z) of it, which added Kimi.
