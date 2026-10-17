<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://raw.githubusercontent.com/yilab/hatcheck/main/site/logo-wordmark-dark.svg">
    <img src="https://raw.githubusercontent.com/yilab/hatcheck/main/site/logo-wordmark.svg" alt="Hatcheck" width="360">
  </picture>
</p>

# Hatcheck

Several Claude accounts on one computer, each kept apart: isolated Claude Desktop instances on
macOS, and Claude Code CLI profiles, with recent sessions, plan usage and resume commands across all
of them. OpenAI's Codex CLI gets the same treatment, and sessions can be carried between the two.

*Unofficial: not made by, affiliated with or endorsed by Anthropic or OpenAI. Claude, Claude Code and
Claude Desktop are trademarks of Anthropic; Codex and ChatGPT are trademarks of OpenAI.*

Run **two (or more) fully isolated Claude Desktop instances** side by side on macOS — e.g. one
signed into your **personal** account and one into your **work** account — each with its own
**color-coded Dock launcher** that **focuses the existing window instead of spawning duplicates**.

This builds on Philipp Stracker's excellent
[`--user-data-dir` approach](https://gist.github.com/stracker-phil/9f84927a556632c7f9cc06663b534f14)
([write-up](https://philippstracker.com/multiple-claude-instances/)) and adds two things:

1. **Recolored icons** so you can tell the launchers apart at a glance (blue work / red personal / etc.).
2. A **smart, self-contained launcher** that fixes the "every click opens another window" problem.

It also has **command-line tools for Claude Code and [Codex](https://github.com/openai/codex)**, for
keeping one profile per account: `cl` and `cx` run Claude Code and Codex with a profile, and
`hatcheck` lists the recent sessions in all of them, with the command that resumes each one and
what's left of each plan. `hatcheck usage` shows the plans, `hatcheck best` says which account to use
now, `hatcheck where` finds a session, `hatcheck open` resumes one in a new terminal, and
`hatcheck move` carries one over to another profile, or converts it between Claude Code and Codex.
`hatcheck bind` ties a folder to a profile, so `cl` and `cx` pick it there, and `--json` gives the
sessions and plans to scripts and status bars. `claude-z` and `claude-k` run Claude Code on Z.ai's
GLM or Kimi's models instead, in any profile. See
[Claude Code and Codex CLI profiles](#claude-code-and-codex-cli-profiles).

|                                 | macOS | Linux | Windows |
|---------------------------------|:-----:|:-----:|:-------:|
| Claude Desktop instances        |  yes  | no official Desktop app | [planned](#roadmap) |
| Claude Code and Codex CLI tools |  yes  |  yes  |   yes   |

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

## Claude Code and Codex CLI profiles

Claude Code (the `claude` CLI) keeps its settings, history and session transcripts in `~/.claude`
(plus `~/.claude.json`), and its login in the macOS Keychain, or on Linux and Windows in
`~/.claude/.credentials.json`. Point `CLAUDE_CONFIG_DIR` at another folder and you get a separate
profile with its own login, so you can keep one per account:
`~/.claude-work`, `~/.claude-personal`, and so on. These are separate from the Desktop instances
above, though sessions from Desktop's Code tab can be filed in them too (the session list below
hides those unless you pass `--all`). Codex works the same way with `CODEX_HOME`: see
[`cx`](#cx-run-codex-with-a-home-per-account).

### Install

As a package, on macOS, Linux and Windows, without a clone. It installs `hatcheck`, `hat`, `cl`,
`cx`, `claude-z` and `claude-k`:

```bash
uv tool install hatcheck          # or: pipx install hatcheck
brew install yilab/tap/hatcheck   # Homebrew, on macOS and Linux
```

Those two wait for the first release, made by `.github/workflows/release.yml` from a tag such as
`v0.1.0`. Until then, from GitHub:

```bash
uv tool install git+https://github.com/yilab/hatcheck
```

The package's `cl`, `cx`, `claude-z` and `claude-k` are Python (`hatcheck/launch.py`), doing what
the `sh` and PowerShell ones below do, so they work on every system. Python 3.9 or later.

From a clone, on macOS and Linux:

```bash
./install.sh               # link the commands below into ~/.local/bin
./install.sh --bin ~/bin   # or into another folder on your PATH
./install.sh --uninstall   # remove the links again
```

The commands are symlinks to this repo, so a `git pull` updates them. The installer doesn't edit
your shell startup files: if the folder isn't on your `PATH`, it prints the line to add. A file that
already has one of these names is left alone unless you pass `--force`, except the copies of
`claude-z` and `claude-k` that the [claude-z](https://github.com/yiwang/claude-z) installers made,
which these replace. `hatcheck` needs Python 3.9 or later (on macOS, `xcode-select --install`
provides it). `hat` is installed as its short name too, unless another command on your `PATH` already
has that name.

Before the rename to Hatcheck these were separate commands: `claude-recent-sessions` (now
`hatcheck`), `claude-usage`, `claude-where`, `claude-move-session` and `claude-provider` (now
`hatcheck usage`, `where`, `move` and `provider`). The installers remove the links to them.

On Windows, in PowerShell:

```powershell
.\install.ps1               # add .cmd launchers for the commands below to ~\.local\bin
.\install.ps1 -Bin D:\bin   # or to another folder on your PATH
.\install.ps1 -Uninstall    # remove them again
```

The launchers run the scripts in this repo, so a `git pull` updates them, and they work from
PowerShell and `cmd` alike, whatever the script execution policy. You need Python 3.9 or later
(python.org, or `winget install Python.Python.3.13`). If PowerShell won't run `install.ps1`, start
it with `powershell -ExecutionPolicy Bypass -File install.ps1`. On Windows, `hatcheck` prints `cd`
and resume commands for PowerShell. Paste them into PowerShell, not `cmd`, because
`cmd` doesn't treat the trailing `# usage` as a comment. Inside a Visual Studio developer prompt,
the MSVC compiler is also called `cl`, and whichever comes first on `PATH` wins.

#### Or as shell functions

`cl` and `cx` are small enough to keep as shell functions instead. Add this to `~/.zshrc` (or
`~/.bashrc`):

```bash
# cl work --resume <id>  ==  CLAUDE_CONFIG_DIR=~/.claude-work claude --resume <id>
# cl -k work ... runs the installed claude-k instead (-z: claude-z)
cl() {
  local run=claude
  case $1 in -z|-k) run=claude$1; shift ;; esac
  [ -n "$1" ] || { echo "usage: cl [-z|-k] <profile> [claude args...]  (uses ~/.claude-<profile>)" >&2; return 2; }
  CLAUDE_CONFIG_DIR="$HOME/.claude-$1" "$run" "${@:2}"
}

# cx work resume <id>  ==  CODEX_HOME=~/.codex-work codex resume <id>
cx() {
  [ -n "$1" ] || { echo "usage: cx <profile> [codex args...]  (uses ~/.codex-<profile>)" >&2; return 2; }
  mkdir -p "$HOME/.codex-$1" && CODEX_HOME="$HOME/.codex-$1" codex "${@:2}"
}

```

A shell function takes precedence over an installed command with the same name, and both do the
same thing. `hatcheck` needs the install; without it, `find ~/.claude* ~/.codex* -name "*3f1c2a9e*"`
finds a session's files.

### `cl`: run Claude Code with a profile

```bash
cl work                           # Claude Code with ~/.claude-work (sign in on first run)
cl work --resume <session-id>     # resume one of that profile's sessions
cl personal --continue            # any claude option works
cl -k work --resume <session-id>  # that session on Kimi; -z for Z.ai (see below)
```

Plain `claude` keeps using `~/.claude`. Claude Code names each profile's Keychain entry after its
`CLAUDE_CONFIG_DIR` value, so keep one spelling of each path; `cl` always uses `~/.claude-<name>`.
In a folder tied to a profile with [`hatcheck bind`](#hatcheck-bind-tie-a-folder-to-a-profile), the
name can be left out: `cl --resume <session-id>` there uses that profile.

### `cx`: run Codex with a home per account

[Codex](https://github.com/openai/codex) keeps its login, config and sessions in `~/.codex`, and
`CODEX_HOME` points it at another folder, so you can keep one home per ChatGPT account the same way:
`~/.codex-work`, `~/.codex-personal`, and so on.

```bash
cx work login                     # sign in once, with the work account
cx work                           # Codex with ~/.codex-work
cx work resume <session-id>       # resume one of that home's sessions
cx personal exec "run the tests"  # any codex command works
```

- `cx` makes the folder on first use, since Codex won't start with a `CODEX_HOME` that doesn't exist.
- A new home starts without your `~/.codex/config.toml` (model, trusted folders, MCP servers); copy
  it over if you want the same settings.
- Plain `codex` keeps using `~/.codex`.
- In a folder tied to a home with [`hatcheck bind`](#hatcheck-bind-tie-a-folder-to-a-profile), the name
  can be left out: `cx resume <session-id>` there uses that home. A Codex command such as `resume`
  counts as a home's name only when `~/.codex-resume` exists; anywhere else, `cx resume …` says it
  needs a home rather than making `~/.codex-resume`.

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
  made with those works as is. `hatcheck provider` shows what's set up.
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
- `hatcheck usage` and the comments in `hatcheck` show the Claude and ChatGPT plans, not Z.ai's or
  Kimi's.

Coming from the claude-z repo: `./install.sh` replaces the copies its installers put in
`~/.local/bin`, and your config stays. On Windows, delete `%LOCALAPPDATA%\claude-z` (and `\claude-k`)
and take it off your `PATH` by hand, but keep `%APPDATA%\claude-z`, which holds your key; that repo's
uninstallers delete both. The permission mode its installers asked for isn't used (the old launchers
only ever applied `dangerously-skip-permissions`): set `permissions.defaultMode` in the profile's
`settings.json`, or pass `--permission-mode`. OpenRouter (`claude-o`) isn't included.

### `hatcheck where`: find a session

```console
$ hatcheck where 3f1c2a9e
/Users/me/.claude-work/projects/-Users-me-code-api/3f1c2a9e-8b7d-4c1e-9f2a-6d5e4c3b2a10.jsonl
$ hatcheck where 019a8c1e
/Users/me/.codex/sessions/2026/10/14/rollout-2026-10-14T09-12-33-019a8c1e-7d3f-7b21-9c4e-2f1a0b9c8d7e.jsonl
```

The profile in the path tells you how to resume it (`claude`, or `cl work` here), and the folder
under `projects/` is the directory the session ran in, with every non-alphanumeric character turned
into `-`. A session with subagents or large tool results also has a data folder of the same name,
which is listed too. Codex files its sessions by the date they started, archived ones under
`archived_sessions/`; resume one with `codex resume <id>`, or `cx <name> resume <id>` for a
`~/.codex-<name>` home.

### `hatcheck`: what to resume

`hat` is the short name: `hat`, `hat usage`, `hat move …` and so on work the same.

Lists the Claude Code sessions from every `~/.claude*` profile, and the Codex sessions from every
`~/.codex*` home, that were active in the last 24 hours or are still open, grouped by folder and
newest first. Run from a folder that has sessions, it lists
just that folder's (`--all-folders` lists them all). Each one comes with the command that resumes it
and what's left of that profile's plan:

```
$ hatcheck
Claude Code and Codex CLI sessions active in the last 24h or open now (by folder, newest first; % = plan left, ↻ = hours/days until reset)
cd ~/code/api
    4m  Fix flaky login test · main · ● open in pid 48213 (busy)
        cl work --resume 3f1c2a9e-8b7d-4c1e-9f2a-6d5e4c3b2a10      # 5h 100% █████               Fable 48% ███▍░░░ ↻████▎░░ 4d6h
   40m  Add retries to the uploader · codex · main
        cx work resume 019a8c1e-7d3f-7b21-9c4e-2f1a0b9c8d7e        # 5h  80% ████░ ↻███▏░ 3h10m  week  64% ████▌░░ ↻█▊░░░░░ 1d18h
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
  (`week`) and per-model weekly limits such as `Fable`, which are named in violet, with their bar
  drawn in it, so they stand apart from `week`. Each one has the % left and a bar of it, then `↻`, a bar of the time left until
  it resets and that time. Bars have a block per hour of the 5-hour window and per day of a weekly
  one, as in `↻████▎░░ 4d6h`.
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
- Codex sessions are marked `codex` and resume with `codex resume <id>`, or `cx <name> resume <id>`
  for a `~/.codex-<name>` home. Their comment shows the ChatGPT plan's Codex limits (see
  `hatcheck usage` below). Sessions from the Codex app, its IDE extension and `codex exec` are hidden
  like Desktop ones, and Codex's own approval reviews aren't listed.
- Since the usage is a shell comment, pasting the whole line runs just the command (zsh needs
  `setopt interactivecomments`).
- Usage is fetched live, one request per account. When that fails (expired token, rate limit), Claude
  Code's last saved snapshot is used instead and named at the bottom with its age.
- A session that ran in a folder [bound](#hatcheck-bind-tie-a-folder-to-a-profile) to another
  profile is marked `⚠ folder bound to ~/.claude-work`, so it's easy to spot and `hatcheck move`.

| Option       | What it does                                                                  |
|--------------|-------------------------------------------------------------------------------|
| `HOURS`      | how far back to look (default 24); takes `36`, `1.5`, `12h` or `3d`           |
| `-n N`       | show only the newest N sessions per folder and count the rest                 |
| `--all`      | also list sessions from Claude Desktop's Code tab, the SDK, the Codex app, its IDE extension and `codex exec` |
| `--all-folders` | list every folder, even when run from one that has sessions                |
| `--full`     | print `CLAUDE_CONFIG_DIR=~/.claude-<name> claude …` instead of `cl`, and `CODEX_HOME=… codex …` instead of `cx` |
| `--cached`   | take usage from Claude Code's and Codex's saved snapshots, without calling the APIs |
| `--no-usage` | leave plan usage out                                                          |
| `--json`     | print the sessions, and the accounts behind them, as JSON ([below](#--json-for-scripts-status-bars-and-apps)) |
| `-e`, `--email` | with `--json`, include each account's email address                        |

### `hatcheck usage`: plan usage for every profile

```
$ hatcheck usage
Plan usage by profile (% left, then time left until reset)
Sorted by Fable: those with some left first, the soonest reset on top; then those out of it, the soonest back on top.
~/.claude-work  team max 5x · live
  session (5h)      100% ██████████
  week (Fable)       48% ████▊░░░░░  ██████▏░░░ resets Sun 8:20pm (in 4d6h)
~/.claude  pro · live
  session (5h)       62% ██████▎░░░  █████▍░░░░ resets 5:01pm (in 2h41m)
  week (all models)  91% █████████▏  ████▍░░░░░ resets Sat 4:20pm (in 3d2h)
~/.claude-personal  pro · cached 9h00m ago (token expired; run claude in this profile to renew it)
  session (5h)         ? ··········             reset 10:20am, after this snapshot
  week (all models)  97% █████████▊  ███████▎░░ resets Mon 4:20pm (in 5d2h)
~/.codex-work  ChatGPT plus · live
  session (5h)       80% ████████░░  ██████▍░░░ resets 5:30pm (in 3h10m)
  week (all models)  64% ██████▍░░░  ██▌░░░░░░░ resets Fri 8:20am (in 1d18h)
```

- Accounts are sorted by their Fable limit. First come those with some Fable left, the soonest
  Fable reset on top: what's left there goes unused once it resets, so that's the one to use first.
  Next come those out of Fable, or out of the weekly limit for all models, which Fable counts against
  too, the soonest back on top. Then come accounts without a Fable limit, such as Pro plans, and
  Codex homes. A snapshot saved before its Fable limit reset counts as full until the reset after.
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
- Profiles signed in to the same account are shown together. Per-model limits such as
  `week (Fable)` are named in violet, with their bar drawn in it, to tell them from `week (all
  models)`; the % keeps the severity's colour. Quota fields the server
  sends that Claude Code doesn't display are listed dimmed under their raw names.
- Codex homes show their ChatGPT plan's Codex limits: the 5-hour and weekly windows (a free plan has
  one 30-day window instead), then limits for single models such as `GPT-5.3-Codex-Spark`, named in
  violet like `Fable`. The access token in the home's `auth.json` is sent as it is to the endpoint
  Codex's `/status` uses, and never refreshed. When it has expired, or the request fails, the newest
  limits Codex recorded in that home's session files are shown, with their age. Codex doesn't send a
  severity, so a limit turns yellow from 75% used and red from 90%. Homes signed in to the same
  account are shown together.
- `--cached` skips the APIs. `-e` / `--email` adds each account's email address, which is hidden by
  default so the output is safe to share. `--json` prints the accounts as data
  ([below](#--json-for-scripts-status-bars-and-apps)).

### `hatcheck move`: carry on a session with another profile

When one account runs low, continue a session on another profile, or in the other agent. It copies
the session by default; `--no-keep` deletes the original once the copy is checked, which moves it.

```
$ hatcheck move 9a8b7c6d work
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
  `hatcheck` marks those `● open`.
- Resume it with the printed command, from the directory it ran in, or answer `y` when it asks and
  it starts Claude Code there with the new profile. It only asks in a terminal, so scripts never
  wait on it. The conversation comes across whole; the account and its plan limits, settings,
  permissions, MCP servers, plugins and project memory are the new profile's. If the new account
  rejects the signatures on earlier thinking, Claude Code drops those thinking blocks and carries
  on.
- Plan files stay in the old profile even with `--no-keep`, since sessions can share one.
- If the session's last reply came from Z.ai or Kimi, it also prints the `cl -z` or `cl -k` command
  that carries on there instead of on the new profile's Claude plan.

#### When the profile already has a copy

A session copied to another profile and carried on there can be moved back: the copy it left
behind is an older version of it. The two copies are compared line by line:

- **The copy there is older**, because the session went on from it: it's brought up to date, and
  each item shows `(updated an older copy)`. Nothing is lost, since the copy being moved starts with
  all of it.
- **The copy there is newer**, because the session went on there: nothing is copied. With
  `--no-keep`, the older copy here is deleted.
- **Both went on** after the copy was made: nothing is copied, since that would lose the prompts
  only the copy there has. It says when they split and how many prompts each has had since.
  `--fork` brings this one in as a new session with its own ID and keeps both, as Claude Code's
  `claude --resume <id> --fork-session` and `codex fork` do. Or delete the copy there first.

```
$ hatcheck move 9a8b7c6d default
Session 9a8b7c6d went on in both ~/.claude-work and ~/.claude after Oct 15 01:40: 3 new prompts in ~/.claude-work, 1 in ~/.claude. Copying this one over would lose what's only in ~/.claude, so nothing was copied.
To keep both, bring this one in as a new session with its own ID:
  hatcheck move 9a8b7c6d default --fork
Or delete the copy in ~/.claude first, to replace it:
  ~/.claude/projects/-Users-me-code-api/9a8b7c6d-5e4f-4a3b-8c2d-1e0f9a8b7c6d.jsonl
```

A fork changes the session's ID in its records and gives it its own folders for subagents, saved
tool output and `/rewind` snapshots. Its message IDs stay the same, as in Claude Code's own fork.
Resuming a fork, a round trip and a copy that went on in both profiles were tested with the real
`claude` (2.1.301).

When more than one profile has the session, you're asked which copy to take. The one used last
comes first and is the default, and each other one says how it stands to it:

```
$ hatcheck move 4215508b hp-alan
Session 4215508b is in 3 profiles:
  1. ~/.claude-hp-aria    Oct 17 14:02   312 prompts  used last
  2. ~/.claude-hp-yi      Oct 16 09:11   298 prompts  14 prompts behind 1
  3. ~/.claude-hp-aiqing  Oct 12 18:40   201 prompts  went its own way after Oct 11 20:15: 2 prompts of its own
Which one? [1]
```

A number, a profile's name or Enter picks one. When the output isn't a terminal, as in a script or
with `--json`, the copy used last is taken and a line says so; `--from` always decides.

#### With Codex

`codex` is `~/.codex` and `codex:<name>` is `~/.codex-<name>`, as a place to copy to or `--from`:

```
$ hatcheck move 019a8c1e work
Converted Codex session 019a8c1e "Add retries to the uploader" from ~/.codex-work into a Claude Code session in ~/.claude-work:
  conversation               5 of your messages, with the replies
  tool calls                 191, as text with their output shortened
  left out                   Codex's reasoning, images, goals and subagent sessions
  new session                6f6749d7-0f59-4683-997c-a3595421b93f (241.9 KB)
The original stays in ~/.codex-work.
Resume it with:
  cd ~/code/api
  cl work --resume 6f6749d7-0f59-4683-997c-a3595421b93f
Resume it now in ~/.claude-work? [y/N]
```

- Between Codex homes signed in to the same ChatGPT account, the session file is copied as it is,
  with its ID and its name, and `--no-keep` works as for Claude Code. A copy already there is
  compared the same way, and `--fork` gives the copy a new ID in its file name, its
  `session_meta` and its events.
- Between Claude Code and Codex (`hatcheck move 3f1c2a9e codex:work` goes the other way), and
  between Codex homes on different accounts, the session is converted into a new one with a new ID,
  and the original is always kept. Codex encrypts its reasoning and its summaries of compacted
  context for the account that wrote them, so a copy can't take them to another account.
- Your messages and the replies come across as text, and each tool call as a block with its input
  and the start of its output, in the format Codex's own `/import` uses for Claude Code sessions
  (`[external_agent_tool_call: Bash] … [/external_agent_tool_call]`). A note at the start tells the
  model where the session came from. Claude's thinking and Codex's reasoning are left out, and so are
  images, plans, goals, `/rewind` snapshots and subagent transcripts.
- The whole conversation before a compaction is carried over, not its summary. Past about 100,000
  tokens the oldest turns after the first are left out, so it fits the new agent's context window.
- Converted sessions were tested by resuming them with the real `claude` (2.1.301) and `codex`
  (0.155.1), which sent the whole converted history to the model and carried on in the same
  session. Codex's resume picker lists them by their Claude Code title.
- Codex can import Claude Code sessions itself (`/import` in Codex), but only from `~/.claude` and
  the last 30 days. Claude Code's `claude import codex` brings over Codex's instructions, agents and
  hooks, not sessions.

| Option           | What it does                                                                 |
|------------------|------------------------------------------------------------------------------|
| `SESSION`        | the session ID, or the start of it                                           |
| `PROFILE`        | where it goes: `NAME` for `~/.claude-NAME`, `default` for `~/.claude`, `codex` for `~/.codex`, `codex:NAME` for `~/.codex-NAME`, or a path |
| `--from PROFILE` | the profile or home to take it from, when more than one has it; without it you're asked, with the one used last as the default, and a script or `--json` gets that one |
| `--no-keep`      | delete the original once the copy is checked, which moves it (not when converting) |
| `--fork`         | copy it in as a new session with its own ID, leaving a copy already there as it is (a converted session always gets one) |
| `--dry-run`      | show what would be copied, converted and deleted, and change nothing         |

### `hatcheck open`: resume a session in a new terminal

```console
$ hatcheck open 3f1c2a9e          # a new terminal window in ~/code/api, running cl work --resume …
$ hatcheck open                   # the newest session started from this folder, in any profile or home
$ hatcheck open 3f1c2a9e --here   # in this terminal instead
$ hatcheck open 3f1c2a9e --print  # the two commands, for scripts
cd ~/code/api
cl work --resume 3f1c2a9e-8b7d-4c1e-9f2a-6d5e4c3b2a10
```

- It finds the session by its ID or the start of it, in every Claude Code profile and Codex home.
  When more than one profile has a copy, it takes the one used last; `--from <profile>` picks one.
- The command is the one `hatcheck` prints: `cl` or `claude`, `cx` or `codex`, and `cl -z`/`cl -k` for a
  session whose last reply came from Z.ai or Kimi. Without `cl` or `cx` on the `PATH` it's the full
  form (`CLAUDE_CONFIG_DIR=… claude …`), as `--full` gives.
- A session that's open in a terminal is refused, so it isn't resumed twice.
- The terminal:
  - **macOS:** Terminal, or the one `hatcheck` runs in when that's iTerm2, Ghostty or WezTerm. Terminal
    and iTerm2 are driven with AppleScript, which macOS asks you to allow once.
  - **Windows:** Windows Terminal (`wt`), otherwise a new PowerShell window.
  - **Linux:** `$TERMINAL`, otherwise `x-terminal-emulator`, GNOME Terminal or Konsole.
  - `--terminal NAME`, or `HATCHECK_TERMINAL`, picks another: `terminal`, `iterm`, `ghostty`,
    `wezterm`, `wt`, `powershell`, `gnome-terminal`, `konsole`, `kitty`, or any terminal that takes
    `-e`. A shell stays open in the folder after the session ends.
- If the terminal can't be opened, it prints the two commands to run instead.

### `hatcheck best`: which account to use now

```console
$ hatcheck best
Claude Code, best first (sorted by Fable, as hatcheck usage sorts)
   1. ~/.claude-work      Fable 31% left, resets Tue 12:00pm · 5h 80%   → cl work
   2. ~/.claude-team      Fable 100% left, resets Thu 1:00am · 5h 100%  → cl team
   3. ~/.claude-personal  Fable 0% left, back Sat 4:59pm · 5h 100%      → cl personal
   4. ~/.claude           week 64% · 5h 100%                            → claude
Codex, best first
   1. ~/.codex  month 100%  → codex
Advice only: nothing is started, moved or switched. hatcheck best --for <session> says where to carry a session.
$ hatcheck best --for 9a8b7c6d
Session 9a8b7c6d "Add rate limiting to the login endpoint" is in ~/.claude-personal: Fable 0% left, back Sat 4:59pm · 5h 100%.
Carry it to ~/.claude-work: Fable 31% left, resets Tue 12:00pm · 5h 80%.
  hatcheck move 9a8b7c6d work --from personal
It prints the command that resumes it there, and offers to run it.
```

- Claude Code accounts come in the order `hatcheck usage` sorts them: those with Fable left first,
  the soonest Fable reset on top; then those out of Fable, or out of the weekly limit for all models,
  the soonest back on top; then accounts without a Fable limit, by the limit with the least left.
  Codex homes are ranked by the limit with the least left.
- `--for <session>` picks the best other account of the session's agent that has room now, and
  prints the [`hatcheck move`](#hatcheck-move-carry-on-a-session-with-another-profile) command. If
  the session's own account is the best, it says so and prints the resume command.
- The numbers are the ones `hatcheck usage` shows, live when it can be, with a snapshot's age
  otherwise; `--cached` skips the APIs.
- **Advice only.** It runs when you ask, prints commands, and never starts, moves or switches
  anything. Using several accounts to get around one plan's limits may break Anthropic's or OpenAI's
  terms; this is for choosing between accounts you may use for the work at hand, such as your own
  and your company's.

### `hatcheck bind`: tie a folder to a profile

With a work and a personal account, it's easy to start `claude` in a work repo out of habit. The
session then runs on the personal account and is filed in the personal profile. A binding says
which profile a folder's work belongs to:

```console
$ cd ~/code/api
$ hatcheck bind work                    # this folder and everything under it: ~/.claude-work, ~/.codex-work
Bound ~/code/api and everything under it: Claude Code ~/.claude-work · Codex ~/.codex-work.
Here, cl --resume <id> means cl work --resume <id> and cx resume <id> means cx work resume <id>.
$ hatcheck bind work --codex personal   # a different Codex home
$ hatcheck bind                         # what this folder is bound to
$ hatcheck bind --list                  # every bound folder
$ hatcheck unbind                       # untie it
$ hatcheck bind --hook                  # warn in Claude Code and Codex (once; hatcheck unbind --hook undoes it)
```

- Bindings live in your own config, `~/.config/hatcheck/bindings.json`
  (`%APPDATA%\hatcheck\bindings.json` on Windows), not in the repo, so profile names don't end up in
  git. The deepest bound folder wins, and folders are compared once symlinks are resolved.
- `default` binds `~/.claude` and `~/.codex`. The Codex home is the one named like the profile when
  there is one; `--codex <name>` picks another, and `--codex` alone binds just the Codex home.
- `cl` and `cx` use the binding when no profile name is given: `cl --resume <id>`, `cl`,
  `cx resume <id>`. A name given still wins, and outside bound folders nothing changes.
- `hatcheck` marks sessions that ran in a bound folder under another profile:
  `⚠ folder bound to ~/.claude-work`.
- `--hook` adds a `SessionStart` hook, `hatcheck bind --check`, to every profile's `settings.json`
  and every Codex home's `hooks.json`. When a session starts in a bound folder under another
  profile, it shows a warning and nothing is blocked:
  `hatcheck: ~/code/api is bound to ~/.claude-work, but this session runs in ~/.claude-personal. Quit
  and run cl work here instead.` For a resumed session it gives the `hatcheck move` command instead.
  - **Claude Code:** tested with the real `claude` (2.1.301). It shows the warning in the interactive
    UI; `claude -p` doesn't display it.
  - **Codex:** Codex runs a new hook only once you trust it with `/hooks` in `codex`, once per home.
    Tested with `codex exec` (0.163): it runs the hook from the home's `hooks.json`, with the folder,
    the session and `CODEX_HOME`, but `codex exec` doesn't print hook messages. Codex's docs say its
    interactive UI shows them as a warning; that part wasn't tested here.

### `--json`: for scripts, status bars and apps

`hatcheck --json` and `hatcheck usage --json` print what the text shows, as data:

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
      "continued_in": null,
      "bound_to": null,
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
      "why": null,
      "limits": [
        {"name": "session (5h)", "short": "5h", "kind": "session", "used_percent": 20, "left_percent": 80,
         "resets_at": "2026-10-16T01:30:00Z", "window_seconds": 18000, "severity": "normal", "per_model": false}
      ],
      "extra": null,
      "fable": {"left": true, "resets_at": "2026-10-20T16:00:00Z"}
    }
  ],
  "hidden": {"Desktop": 3},
  "other_folders": 0,
  "older_not_listed": {}
}
```

- **The same choices as the text:** the time window, `--all`, `--all-folders` and `-n` pick the same
  sessions in the same order, and `resume` is the command the text prints. `account` is the
  session's place in `accounts`, which holds the accounts behind the listed sessions (none with
  `--no-usage`). `hatcheck usage --json` gives every account, in its Fable order, with `sorted_by`.
- **Stable:** absolute paths rather than `~`, times in UTC (ISO 8601), no colours. `version` goes up
  only when a field changes meaning or goes away; new fields can appear at any time.
- `left_percent` is `null` for a snapshot's limit that has reset since it was saved. `fable` is
  `{"left": true, "resets_at": …}`, `{"left": false, "back_at": …}`, or `null` without a Fable limit.
  `source` is `live`, `cached` or `none`, with `why` when it isn't live.
- Errors go to stderr with a non-zero exit, so stdout is always valid JSON. Email addresses only with
  `--email`.

## The desktop app

A menu-bar app (system tray on Windows and Linux) built on the CLI. It shows what's left of every
plan, sorted by Fable, with bars and reset times, and the last day's sessions with **Resume**,
which opens a terminal in the session's folder, and **Copy**. It also has buttons for the Claude
Desktop instances and notifies you when a limit runs low or resets.

Its main window browses every session, with search, filters and a read-only preview of the
conversation. From there you can resume a session, copy its command, or move it to another
profile after a dry run. The window also lists profiles and homes, signs in to a new one in a
terminal, and makes Desktop instances.

It ships the CLI inside itself, so it needs no Python. It never handles a login, and live usage is
off until you turn it on. See [desktop/README.md](desktop/README.md) to build it; releases come
from `.github/workflows/desktop.yml` for macOS, Windows and Linux. The plan behind it is
[the proposal](docs/proposals/desktop-app.md).

## Files in this repo

Claude Code CLI profiles and Codex homes (macOS, Linux, Windows):

- **`install.sh`** / **`install.ps1`** — put the commands below on your `PATH` (`~/.local/bin` by default).
- **`pyproject.toml`** — the Python package (`uv tool install`, `pipx`); **`packaging/`** — the Homebrew
  formula and cask, and the winget manifest.
- **`hatcheck.py`** — the `hatcheck` command (`hat` for short) from a clone; it runs **`hatcheck/cli.py`**,
  which runs the subcommands in **`hatcheck/`**:
  - **`sessions.py`** — recent sessions across profiles and homes, with resume commands and plan usage.
  - **`usage.py`** — plan usage left for every profile and home.
  - **`where.py`** — finds a Claude Code or Codex session's files by ID or ID prefix.
  - **`move.py`** — copies a session to another profile, moves it with `--no-keep`, or converts it between
    Claude Code and Codex.
  - **`open.py`** — resumes a session in a new terminal, in its folder.
  - **`best.py`** — which account to use now, and where to carry a session.
  - **`bind.py`** and **`unbind.py`** — folder bindings, and the warning hook.
  - **`show.py`** — a session's conversation, read-only.
  - **`desktop.py`** — Claude Desktop instances on macOS: make, list, open and remove them
    ([docs/desktop-instances.md](docs/desktop-instances.md)).
  - **`launch.py`** — `cl`, `cx`, `claude-z` and `claude-k` in Python, for package installs.
  - **`provider.py`** — the Z.ai and Kimi keys and models behind `claude-z`, `claude-k`, `cl -z` and `cl -k`.
- **`cl`** / **`cl.ps1`** — runs `claude` with a `~/.claude-<name>` profile, on Z.ai or Kimi with `-z` or
  `-k`; installed as `claude-z` and `claude-k` too.
- **`cx`** / **`cx.ps1`** — runs `codex` with a `~/.codex-<name>` home.

The subcommands load each other, so keep the `hatcheck/` folder whole and beside `hatcheck.py`. The
`.ps1` files are the Windows versions of the `sh` ones.

- **`desktop/`** — the desktop app: Tauri, with the CLI inside it ([its README](desktop/README.md)).
- **`site/`** — the landing page, one self-contained file, served at [hatcheck.dev](https://hatcheck.dev)
  from Cloudflare as static assets; `wrangler deploy` in that folder ships it. The logo lives there too:
  `logo.svg` and `logo-dark.svg` (the mark), `logo-wordmark.svg` and `logo-wordmark-dark.svg`, `favicon.svg`.
- **`docs/proposals/`** — the plans these came from: [the CLI features](docs/proposals/cli-features.md)
  and [the desktop app](docs/proposals/desktop-app.md).
- **`tests/`** — tests for the CLI tools against throwaway home folders:
  `python3 -m unittest discover -s tests`. They never read your real profiles, Keychain or the network.
  GitHub Actions runs them on macOS, Linux and Windows (`.github/workflows/tests.yml`).

## Roadmap

- **Claude Desktop instances on Windows.** Claude Desktop is an Electron app there too, so a
  separate `--user-data-dir` per account should work. A Windows version would create a shortcut per
  instance with its own icon, and a launcher that brings a running instance forward instead of
  opening a duplicate. It needs testing on a Windows machine first.
- **Linux.** There's no official Claude Desktop for Linux, so only the CLI tools apply there.
- **First releases:** the CLI on PyPI and in the Homebrew tap, and the app's installers, once
  the release workflows have their accounts and secrets (the workflows list them).
- **Signing:** a Developer ID for the macOS app, and a certificate for the Windows one.

## Credits

- **Claude Desktop instances:** this repo started as a fork of
  [Aaron Oxborrow](https://github.com/aoxborrow)'s
  [gist](https://gist.github.com/aoxborrow/5a3a0ac0aa37819cc19439b70a552221), whose scripts made
  colour-coded launchers for isolated instances. On 16 October 2026 his scripts and his sections of
  this README were replaced by a rewrite, `hatcheck desktop`, so none of his code or text remains.
  The idea both implement, running Claude Desktop with `--user-data-dir`, is from
  [Philipp Stracker's gist](https://gist.github.com/stracker-phil/9f84927a556632c7f9cc06663b534f14)
  ([write-up](https://philippstracker.com/multiple-claude-instances/)), itself a cleaned-up version
  of [weidwonder/claude-desktop-multi-instance](https://github.com/weidwonder/claude-desktop-multi-instance).
- **Claude Code and Codex CLI tools:** `hatcheck`, `cl`, `cx`, the installers and the tests are by
  [Yi Wang](https://github.com/yiwang).
- **Z.ai and Kimi:** the provider settings in `hatcheck/provider.py` (endpoints, model slots, context
  windows and the config files) come from [claude-z](https://github.com/sam-fakhreddine/claude-z) by
  Kerim ([lettt325](https://github.com/lettt325)), MIT-licensed, and Yi Wang's
  [fork](https://github.com/yiwang/claude-z) of it, which added Kimi.

## License

MIT, see [LICENSE](LICENSE). It covers everything in this repo: the CLI, the desktop app, the site
and the docs.
