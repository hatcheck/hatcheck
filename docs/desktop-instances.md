# Claude Desktop instances

What `hatcheck desktop` makes and how it behaves. This is the description the code was written
from; the [README](../README.md#claude-desktop-instances-macos-and-windows) has the short version.
Everything up to [On Windows](#on-windows) is how it works on macOS; that section says what's
different there.

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

## On Windows

The same idea, checked on Windows 11 with Claude Desktop 4.4212 from the Microsoft Store (an MSIX
package) on 2026-10-31. These were checked against the real app:

- **A second instance runs beside the first.** `claude-desktop.exe --user-data-dir=<folder>` starts
  Claude Desktop on its own folder while the stock one keeps running. `claude-desktop.exe` is the
  package's app execution alias in `%LOCALAPPDATA%\Microsoft\WindowsApps`, which stays put when an
  update moves the package's own folder.
- **Signing in lands in the instance that asked.** A second account signed in inside the instance,
  and the stock instance's login was unchanged before and after. The sign-in came back without any
  `claude://` link being opened, so Windows' handler for those links, which would have started the
  stock instance, was never involved. The login is kept, encrypted, in the instance's own
  `config.json`, with the key in its own `Local State`, and was still there after the instance was
  closed and started again.
- **Claude Desktop keeps one process per data folder itself.** A second launch on a folder that has
  a process starts nothing, and the same holds for the built-in folder. So on Windows the launcher
  is a plain shortcut: none of the macOS launcher's lock, process search or System Events.

An instance there is the data folder, `%USERPROFILE%\.claude-instances\<name>`, and:

```
%APPDATA%\Microsoft\Windows\Start Menu\Programs\Claude <Display>.lnk
    target       %LOCALAPPDATA%\Microsoft\WindowsApps\claude-desktop.exe
    arguments    --user-data-dir="<the data folder>"   (none for default)
    description  Claude Desktop instance <name>, made by hatcheck
    icon         %APPDATA%\hatcheck\icons\Claude <Display>.ico
```

The description names the instance; `hatcheck desktop` lists the shortcuts in that folder whose
description it wrote, and reads the data folder from the arguments. The shortcut is written by
Windows' own `WScript.Shell`, through Windows PowerShell, and read back before it counts as made.
Without the Store package, Claude's older installer (`%LOCALAPPDATA%\AnthropicClaude\claude.exe`) is
the target; that one hasn't been tried.

The icon is Claude's own artwork from the package's `assets` folder (the unplated
`Square44x44Logo` images at 16, 24, 32, 48 and 256 pixels), recoloured with the same arithmetic as
on macOS and kept as an `.ico` in hatcheck's config folder rather than in the package, so it
survives Claude's updates. When the package's artwork can't be found, the shortcut has no icon of
its own and shows Claude's.

`hatcheck desktop open` opens the shortcut through the shell, as a click does. It doesn't start
Claude Desktop as a child process: that child would inherit hatcheck's output pipe, and whatever
reads hatcheck's output would wait until Claude Desktop quit. An instance is running when a main
process (no `--type=`) of the package's `claude.exe` has its folder in its `--user-data-dir`, read
from the process's command line.

`default` is safe on Windows, since the stock icon and the launcher bring the same process forward.
To copy a login you already have into an instance, the same steps as on macOS apply, with Claude
Desktop fully quit (from its icon in the notification area, not just by closing the window) and
`%APPDATA%\Claude` as the folder to copy.

## Limits

- macOS and Windows. Claude Desktop for Linux is in beta, for Ubuntu and Debian, and hasn't been
  tried with a data folder of its own.
- The instances share the one Claude Desktop install, so they update together, and a running
  instance can't carry its launcher's colour. On Windows every instance is the one Store app, so
  the taskbar most likely groups them under Claude's own button and icon; that hasn't been checked.
- Windows' guarantee of one process per folder is Claude Desktop's own, checked with 4.4212; a
  version that dropped it would bring back the macOS problem of a second, signed-out process.
- Each instance keeps its own settings and MCP servers; nothing is shared between them unless you
  copy it.
