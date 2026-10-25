# Claude Desktop instances

What `hatcheck desktop` makes and how it behaves. This is the description the code was written
from; the [README](../README.md#claude-desktop-instances-macos) has the short version.

## The problem

Claude Desktop keeps one login. To be signed in to two accounts at once, it has to run twice, each
run with its own data folder. Claude Desktop is an Electron app, and Electron's `--user-data-dir`
option points it at another folder for everything it keeps: the login, settings, sessions, MCP
servers and Cowork data.

```
open -n -a /Applications/Claude.app --args --user-data-dir="$HOME/.claude-instances/a"
```

That idea comes from [Philipp Stracker's gist](https://gist.github.com/stracker-phil/9f84927a556632c7f9cc06663b534f14)
([write-up](https://philippstracker.com/multiple-claude-instances/)), itself a cleaned-up version
of [weidwonder/claude-desktop-multi-instance](https://github.com/weidwonder/claude-desktop-multi-instance).

Two things make a plain `open -n` unpleasant on its own:

- **Every click starts another process.** `open -n` means a new instance even when one is running,
  and Claude Desktop keeps no single-instance lock per folder. Only the first process on a folder
  gets its login; a second one on the same folder comes up signed out, and two processes on one
  folder can damage it.
- **Nothing tells the instances apart** in the Dock or Launchpad.

## One instance per account

The most reliable setup is to create a **separate, dedicated instance for each account, the one
you use most included, and stop using Claude's built-in default profile for day-to-day use.** Don't try to make one launcher target the shared default profile.

Why: Claude has **no per-profile single-instance lock**, so two processes can run on one profile
at once — and **only the *first* instance on a profile can use its login database. Every
additional instance on that same profile shows up logged out.** If a launcher points
at the default profile (which the stock Claude icon also uses), you end up with duplicate,
logged-out windows. Giving each account its own dir guarantees **one profile → one instance →
always logged in**, and makes the focus-don't-duplicate logic rock-solid.

Already logged into the default profile and don't want to re-auth? See
[Copying an existing login into an instance](#copying-an-existing-login-into-an-instance) — you can copy your current
profile into the new dedicated dir.

**Tip:** to keep the focus-don't-relaunch behavior working, **close Claude with ⌘W** (close
window, app keeps running) when you want it to stay live, and **⌘Q** when you truly want it gone.

## What an instance is

An instance is a data folder, `~/.claude-instances/<name>`, and a small launcher app,
`/Applications/Claude <Display>.app`, made by `hatcheck desktop new <name> "<Display>"`:

```
Claude A.app/
  Contents/
    Info.plist          the app's name and bundle ID (dev.hatcheck.hatcheck.claude-<name>), and the
                        instance's name and data folder, as HatcheckInstance and HatcheckDataDir
    MacOS/launch        the launcher, a shell script
    Resources/icon.icns Claude's icon, recoloured
```

The instance named `default` is Claude's built-in data folder, with no `--user-data-dir`, for those
who want a launcher for it. It's only safe while nothing else runs that folder: don't also use the
stock Claude icon then.

## What the launcher does

On each click it looks for Claude Desktop's main process on this instance's folder: a process of
`/Applications/Claude.app/Contents/MacOS/` whose command line has `--user-data-dir=<the folder>`
(for `default`: none at all) and no `--type=`, which marks Electron's helper processes.

- **Not running:** it makes the data folder if needed and starts Claude Desktop with `open -n` and
  that folder. One launch runs at a time, through a lock in `$TMPDIR`, held until the new process
  is listed, so two clicks at once start one process and the second brings it forward.
- **Running:** it brings that process forward by its process ID, through System Events, since to
  macOS every instance is the same app and `open -a` or "activate" can't pick one. The first time,
  macOS asks to let the launcher control System Events; allow it once per launcher.
- **Running without a window:** closing the window with ⌘W leaves the process alive. Coming forward
  usually brings the window back. When no window (a `--type=renderer` process on the folder) has
  appeared after a moment and it's the only Claude Desktop running, the launcher asks it for one
  with `open -a`, which never starts a process; beside other instances, which `open -a` could ask
  instead, a notification says to click its own icon in the Dock. It never quits the windowless
  process: it may be in the middle of a reply.

The launcher never starts a second process on a folder that has one.

## Colours

`--hue` turns every hue in Claude's icon by that many degrees (0 to 360, or negative), and
`--saturation` scales the saturation (1.0 keeps it). Some starting points, from Claude's orange:

| Look       | `--hue` |
|------------|--------:|
| Red-orange |   `-10` |
| Green      |   `110` |
| Blue       |   `190` |
| Purple     |   `240` |

Recolouring needs macOS's `iconutil`; without it the launcher gets the stock icon and still works.

The colour is on the launcher only. While running, every instance shows Claude's own icon and the
name "Claude" in the Dock, because they all run the one `/Applications/Claude.app`; the signed-in
account tells them apart.

## Commands

```
hatcheck desktop                                   the instances, and which are running
hatcheck desktop new NAME DISPLAY [--hue DEG] [--saturation X] [--replace]
hatcheck desktop open NAME                         start it, or bring it forward
hatcheck desktop remove NAME [--data]              delete the launcher; --data deletes the
                                                   data folder too, login and all
hatcheck desktop --json                            the list as data, for the app
```

`NAME` is letters, digits, `.`, `_` and `-`, other than `.` and `..`. `remove --data` deletes the
data folder only when it's a folder of its own in `~/.claude-instances`, and otherwise removes
nothing: a launcher made by 0.5.0 for `.` or `..` keeps its folder, `~/.claude-instances` itself
or the home folder, and goes without `--data`. `DISPLAY` is what the launcher is called, as in
`Claude A.app`. `--replace` rebuilds a launcher that exists, including one made by the
`make-claude-instance.sh` script that `hatcheck desktop` replaced; such launchers are listed and
opened as they are, since the launcher reads their `config.sh` for the instance's name.

## Copying an existing login into an instance

A new instance starts signed out; signing in once is the simple way. To carry over the login,
settings, MCP servers and sessions of Claude's built-in folder instead, copy that folder while
Claude Desktop is fully quit (⌘Q, every instance), leaving out its caches and lock files:

```
rsync -a --exclude 'Singleton*' --exclude 'Cache' --exclude 'Code Cache' --exclude 'GPUCache' \
  --exclude 'DawnGraphiteCache' --exclude 'DawnWebGPUCache' --exclude 'ShaderCache' \
  --exclude 'Crashpad' --exclude 'blob_storage' \
  "$HOME/Library/Application Support/Claude/" "$HOME/.claude-instances/a/"
```

The built-in folder is left as it was. The copy can be large: the Cowork runtime's `vm_bundles/`
folder can run to many GB, and can be deleted from the copy, since it downloads again when needed.

## Limits

- macOS only. Claude Desktop on Windows is an Electron app too, so the same idea should work with a
  shortcut per instance; it hasn't been done here. Claude Desktop for Linux is in beta, for Ubuntu
  and Debian, and hasn't been tried with a data folder of its own.
- The instances share the one Claude Desktop install, so they update together, and a running
  instance can't carry its launcher's colour.
- Each instance keeps its own settings and MCP servers; nothing is shared between them unless you
  copy it.
