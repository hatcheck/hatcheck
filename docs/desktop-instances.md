# Claude Desktop instances

What `hatcheck desktop` makes and how it behaves. This is the description the code was written
from; the [README](../README.md#claude-desktop-instances-macos) has the short version.

## The problem

Claude Desktop keeps one login. To be signed in to two accounts at once, it has to run twice, each
run with its own data folder. Claude Desktop is an Electron app, and Electron's `--user-data-dir`
option points it at another folder for everything it keeps: the login, settings, sessions, MCP
servers and Cowork data.

```
open -n -a /Applications/Claude.app --args --user-data-dir="$HOME/.claude-instances/work"
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

## What an instance is

An instance is a data folder, `~/.claude-instances/<name>`, and a small launcher app,
`/Applications/Claude <Display>.app`, made by `hatcheck desktop new <name> "<Display>"`:

```
Claude Work.app/
  Contents/
    Info.plist          the app's name and bundle ID (com.yilab.hatcheck.claude-<name>), and the
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
  that folder.
- **Running:** it brings that process forward by its process ID, through System Events, since to
  macOS every instance is the same app and `open -a` or "activate" can't pick one. The first time,
  macOS asks to let the launcher control System Events; allow it once per launcher.
- **Running without a window:** closing the window with ⌘W leaves the process alive. Coming forward
  usually brings the window back; when no window (a `--type=renderer` process on the folder) has
  appeared after a moment, the launcher starts a fresh one. It never quits the windowless process:
  it may be in the middle of a reply.

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

Recolouring needs [Pillow](https://pypi.org/project/Pillow/) (`pip install Pillow`) and macOS's
`iconutil`; without them the launcher gets the stock icon and still works.

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

`NAME` is letters, digits, `.`, `_` and `-`. `DISPLAY` is what the launcher is called, as in
`Claude Work.app`. `--replace` rebuilds a launcher that exists, including one made by the
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
  "$HOME/Library/Application Support/Claude/" "$HOME/.claude-instances/personal/"
```

The built-in folder is left as it was. The copy can be large: the Cowork runtime's `vm_bundles/`
folder can run to many GB, and can be deleted from the copy, since it downloads again when needed.

## Limits

- macOS only. Claude Desktop on Windows is an Electron app too, so the same idea should work with a
  shortcut per instance; it hasn't been done here.
- The instances share the one Claude Desktop install, so they update together, and a running
  instance can't carry its launcher's colour.
- Each instance keeps its own settings and MCP servers; nothing is shared between them unless you
  copy it.
