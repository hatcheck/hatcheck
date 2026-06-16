# Multiple Claude Desktop instances on macOS — with color-coded, no-duplicate launchers

Run **two (or more) fully isolated Claude Desktop instances** side by side on macOS — e.g. one
signed into your **personal** account and one into your **work** account — each with its own
**color-coded Dock launcher** that **focuses the existing window instead of spawning duplicates**.

This builds on Philipp Stracker's excellent
[`--user-data-dir` approach](https://gist.github.com/stracker-phil/9f84927a556632c7f9cc06663b534f14)
([write-up](https://philippstracker.com/multiple-claude-instances/)) and adds two things:

1. **Recolored icons** so you can tell the launchers apart at a glance (blue work / red personal / etc.).
2. A **smart, self-contained launcher** that fixes the "every click opens another window" problem.

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

# 3. Create a PERSONAL launcher (uses Claude's built-in/default profile) with a RED-ORANGE icon
./make-claude-instance.sh default Personal -7

# 4. Launch from Launchpad / drag to Dock:  "Claude Work"  and  "Claude Personal"
```

- The **first** time a launcher focuses a running window, macOS asks
  *"Claude … wants to control System Events"* → click **OK** (one time per launcher).
- Use the instance name **`default`** for the personal/built-in profile (no separate data dir).
  Any other name creates an isolated profile under `~/.claude-instances/<name>`.

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

## Managing instances

```bash
# List running Claude instances and their profiles
ps -axo pid,command | grep "[C]laude.app/Contents/MacOS/Claude"

# Remove a wrapper + its profile
rm -rf "/Applications/Claude Work.app"
rm -rf "$HOME/.claude-instances/work"
```

## Files in this gist

- **`make-claude-instance.sh`** — builds the `.app` wrapper (Info.plist, config, smart launcher) and optionally recolors the icon.
- **`claude-launcher`** — the self-contained smart launcher template (embedded into each wrapper).
- **`recolor-icon.py`** — hue/saturation recolor of the `.icns` (used by the helper; needs Pillow).

## Credits

Core `--user-data-dir` isolation technique by
[Philipp Stracker](https://philippstracker.com/multiple-claude-instances/). Smart focus-or-launch
behavior and icon recoloring added here.
