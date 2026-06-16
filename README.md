# Multiple Claude Desktop instances on macOS — with color-coded, no-duplicate launchers

Run **two (or more) fully isolated Claude Desktop instances** side by side on macOS — e.g. one
signed into your **personal** account and one into your **work** account — each with its own
**color-coded Dock launcher** that **focuses the existing window instead of spawning duplicates**.

This builds on Philipp Stracker's excellent
[`--user-data-dir` approach](https://gist.github.com/stracker-phil/9f84927a556632c7f9cc06663b534f14)
([write-up](https://philippstracker.com/multiple-claude-instances/)) and adds two things:

1. **Recolored icons** so you can tell the launchers apart at a glance (blue work / red personal / etc.).
2. A **smart, self-contained launcher** that fixes the "every click opens another window" problem.

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

## Files in this gist

- **`make-claude-instance.sh`** — builds the `.app` wrapper (Info.plist, config, smart launcher) and optionally recolors the icon.
- **`claude-launcher`** — the self-contained smart launcher template (embedded into each wrapper).
- **`recolor-icon.py`** — hue/saturation recolor of the `.icns` (used by the helper; needs Pillow).

## Credits

Core `--user-data-dir` isolation technique by
[Philipp Stracker](https://philippstracker.com/multiple-claude-instances/). Smart focus-or-launch
behavior and icon recoloring added here.
