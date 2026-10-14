# Multiple Claude Desktop instances on macOS — with color-coded, no-duplicate launchers

Run **two (or more) fully isolated Claude Desktop instances** side by side on macOS — e.g. one
signed into your **personal** account and one into your **work** account — each with its own
**color-coded Dock launcher** that **focuses the existing window instead of spawning duplicates**.

This builds on Philipp Stracker's excellent
[`--user-data-dir` approach](https://gist.github.com/stracker-phil/9f84927a556632c7f9cc06663b534f14)
([write-up](https://philippstracker.com/multiple-claude-instances/)) and adds two things:

1. **Recolored icons** so you can tell the launchers apart at a glance (blue work / red personal / etc.).
2. A **smart, self-contained launcher** that fixes the "every click opens another window" problem.

It also has **command-line helpers for Claude Code** (the `claude` CLI) for keeping one profile per
account there: `cl` runs Claude Code with a profile, `claude-where` finds any session, and two scripts
list recent sessions and plan usage across all profiles. See
[Claude Code CLI profiles](#claude-code-cli-profiles).

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

The **smart launcher** in this gist fixes it. On click it inspects *that specific instance*
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

# 1. Download the scripts from this gist, then make the helper executable
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
(plus `~/.claude.json`) and its login in the macOS Keychain. Point `CLAUDE_CONFIG_DIR` at another
folder and you get a separate profile with its own login, so you can keep one per account:
`~/.claude-work`, `~/.claude-personal`, and so on. These are separate from the Desktop instances
above, though sessions from Desktop's Code tab can be filed in them too (the session list below
hides those unless you pass `--all`).

### Install

```bash
./install.sh               # link cl, claude-where, claude-recent-sessions, claude-usage into ~/.local/bin
./install.sh --bin ~/bin   # or into another folder on your PATH
./install.sh --uninstall   # remove the links again
```

The commands are symlinks to this repo, so a `git pull` updates them. The installer doesn't edit
your shell startup files: if the folder isn't on your `PATH`, it prints the line to add. A file that
already has one of these names is left alone unless you pass `--force`. `claude-recent-sessions`
and `claude-usage` need `python3` (`xcode-select --install` provides it).

#### Or as shell functions

`cl` and `claude-where` are small enough to keep as shell functions instead. Add this to
`~/.zshrc` (or `~/.bashrc`):

```bash
# cl work --resume <id>  ==  CLAUDE_CONFIG_DIR=~/.claude-work claude --resume <id>
cl() {
  [ -n "$1" ] || { echo "usage: cl <profile> [claude args...]  (uses ~/.claude-<profile>)" >&2; return 2; }
  CLAUDE_CONFIG_DIR="$HOME/.claude-$1" claude "${@:2}"
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
cl work                         # Claude Code with ~/.claude-work (sign in on first run)
cl work --resume <session-id>   # resume one of that profile's sessions
cl personal --continue          # any claude option works
```

Plain `claude` keeps using `~/.claude`. Claude Code names each profile's Keychain entry after its
`CLAUDE_CONFIG_DIR` value, so keep one spelling of each path; `cl` always uses `~/.claude-<name>`.

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
or are still open, grouped by folder and newest first. Each one comes with the command that resumes
it and what's left of that profile's plan:

```
$ claude-recent-sessions
Claude Code CLI sessions active in the last 24h or open now (by folder, newest first; % = plan usage left, ↻ = resets in)
cd ~/code/api
    4m  Fix flaky login test · main · ● open in pid 48213 (busy)
        cl work --resume 3f1c2a9e-8b7d-4c1e-9f2a-6d5e4c3b2a10      # 5h 100%         Fable 48% ↻4d6h
    3h  Add rate limiting to the login endpoint · feat/rate-limit
        claude --resume 9a8b7c6d-5e4f-4a3b-8c2d-1e0f9a8b7c6d       # 5h  62% ↻2h41m  week  91% ↻3d2h
cd ~/code/blog
   20h  Draft post on Rust lifetimes · main
        cl personal --resume c47e2b1d-0a9f-4e8d-b7c6-5d4e3f2a1b0c  # 5h    ?         week  97% ↻5d2h
cd ~/notes
    6h  Plan next sprint · main
        cl work --resume 5b6c7d8e-9f0a-4b1c-8d2e-3f4a5b6c7d8e      # 5h 100%         Fable 48% ↻4d6h
Profiles: ~/.claude (1), ~/.claude-personal (1), ~/.claude-work (2)
Usage from Claude Code's last snapshot (token expired; run claude in this profile to renew it): ~/.claude-personal 9h ago
```

- Run the folder's `cd` line, then the session's command. Sessions still open in a terminal are
  marked `● open`, so you don't resume one twice.
- The comment after each command shows what's left of each plan limit and how long until it resets
  (`↻`): the 5-hour session (`5h`), the weekly limit (`week`) and per-model weekly limits such as
  `Fable`. `?` means the limit has reset since the saved snapshot it came from. Since it's a shell
  comment, pasting the whole line runs just the command (zsh needs `setopt interactivecomments`).
- Usage is fetched live, one request per account. When that fails (expired token, rate limit), Claude
  Code's last saved snapshot is used instead and named at the bottom with its age.

| Option       | What it does                                                                  |
|--------------|-------------------------------------------------------------------------------|
| `HOURS`      | how far back to look (default 24); takes `36`, `1.5`, `12h` or `3d`           |
| `-n N`       | show only the newest N sessions per folder and count the rest                 |
| `--all`      | also list sessions last used from Claude Desktop's Code tab or the SDK        |
| `--full`     | print `CLAUDE_CONFIG_DIR=~/.claude-<name> claude …` instead of `cl`          |
| `--cached`   | take usage from Claude Code's saved snapshots, without calling the API        |
| `--no-usage` | leave plan usage out                                                          |

### `claude-usage`: plan usage for every profile

```
$ claude-usage
Claude plan usage by profile (% left)
~/.claude  pro · live
  session (5h)       62% ██████░░░░ resets 5:01pm (in 2h41m)
  week (all models)  91% █████████░ resets Sat 4:20pm (in 3d2h)
~/.claude-personal  pro · cached 9h00m ago (token expired; run claude in this profile to renew it)
  session (5h)         ? ·········· reset 10:20am, after this snapshot
  week (all models)  97% ██████████ resets Mon 4:20pm (in 5d2h)
~/.claude-work  team max 5x · live
  session (5h)      100% ██████████
  week (Fable)       48% █████░░░░░ resets Sun 8:20pm (in 4d6h)
```

- It reads each profile's OAuth token from the Keychain and asks the same endpoint as Claude Code's
  `/usage`. The token is only read, never refreshed: a refresh rotates it and would sign Claude Code
  out of that profile.
- When a token has expired or the endpoint rate-limits the account, Claude Code's last saved snapshot
  is shown with its age. Run `claude` (or `cl <name>`) in that profile to renew the token.
- Profiles signed in to the same account are shown together. Quota fields the server sends that
  Claude Code doesn't display are listed dimmed under their raw names.
- `--cached` skips the API. `-e` / `--email` adds each account's email address, which is hidden by
  default so the output is safe to share.

## Files in this gist

Claude Desktop instances:

- **`make-claude-instance.sh`** — builds the `.app` wrapper (Info.plist, config, smart launcher) and optionally recolors the icon.
- **`claude-launcher`** — the self-contained smart launcher template (embedded into each wrapper).
- **`recolor-icon.py`** — hue/saturation recolor of the `.icns` (used by the helper; needs Pillow).

Claude Code CLI profiles:

- **`install.sh`** — links the four commands below into `~/.local/bin` (or `--bin DIR`).
- **`cl`** — runs `claude` with a `~/.claude-<name>` profile.
- **`claude-where`** — finds a session's files by ID or ID prefix.
- **`claude-recent-sessions.py`** — recent sessions across profiles, with resume commands and plan usage.
- **`claude-usage.py`** — plan usage left for every profile. `claude-recent-sessions.py` loads it, so keep the two together.

## Credits

Core `--user-data-dir` isolation technique by
[Philipp Stracker](https://philippstracker.com/multiple-claude-instances/). Smart focus-or-launch
behavior and icon recoloring added here.
