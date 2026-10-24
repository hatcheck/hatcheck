# Installing hatcheck

Every way of installing the seven commands, from a clone, switching between installs and
removing one, and `hatcheck doctor`, which lists every copy on your `PATH`. The commands
themselves are in [commands.md](commands.md) and the agents in [agents.md](agents.md).

## Install

One binary per platform, with no Python needed. Each of these installs `hatcheck`, `hat`, `cl`,
`cx`, `ck`, `claude-z` and `claude-k`.

DeepSeek Harness support uses your own `dsh`; no DSH launcher or installer alias is installed.

```bash
curl -LsSf https://github.com/hatcheck/hatcheck/releases/latest/download/hatcheck-installer.sh | sh
brew install hatcheck/tap/hatcheck  # Homebrew, on macOS and Linux
uv tool install hatcheck            # or: pipx install hatcheck
```

On Windows, in PowerShell:

```powershell
powershell -ExecutionPolicy Bypass -c "irm https://github.com/hatcheck/hatcheck/releases/latest/download/hatcheck-installer.ps1 | iex"
```

The installer scripts put the commands in `~/.local/bin` and add that folder to your `PATH`; set
`HATCHECK_NO_MODIFY_PATH=1` first to keep your shell's startup files as they are. Each release, made
from a tag such as `v0.2.0`, has them and the binaries for each platform, and PyPI gets the same
binaries as wheels. The release builds start after 0.1.0, which is the Python version.

From a clone, on macOS and Linux, with Rust 1.90 or newer ([rustup.rs](https://rustup.rs)):

```bash
./install.sh               # build the commands and link them into ~/.local/bin
./install.sh --bin ~/bin   # or into another folder on your PATH
./install.sh --uninstall   # remove the links again
```

The links point at the build in `target/release`, so after a `git pull`, run `./install.sh` again
to rebuild them. The installer doesn't edit your shell startup files: if the folder isn't on your
`PATH`, it prints the line to add. A file that already has one of these names is left alone unless
you pass `--force`, except the links it made before, to the Python `hatcheck` and the `sh` launchers,
and the copies of `claude-z` and `claude-k` that the [claude-z](https://github.com/yiwang/claude-z)
installers made, which these replace. `hat` is installed as its short name too, unless another program, not hatcheck's, already has
that name on your `PATH`.

Before the rename to Hatcheck these were separate commands: `claude-recent-sessions` (now
`hatcheck`), `claude-usage`, `claude-where`, `claude-move-session` and `claude-provider` (now
`hatcheck usage`, `where`, `move` and `provider`). The installers remove the links to them.

On Windows, in PowerShell:

```powershell
.\install.ps1               # build the commands and add .cmd launchers for them to ~\.local\bin
.\install.ps1 -Bin D:\bin   # or to another folder on your PATH
.\install.ps1 -Uninstall    # remove them again
```

The launchers run the build, so after a `git pull`, run it again. They work from PowerShell and
`cmd` alike, whatever the script execution policy. If PowerShell won't run `install.ps1`, start it
with `powershell -ExecutionPolicy Bypass -File install.ps1`. On Windows, `hatcheck` prints `cd` and
resume commands for PowerShell. Paste them into PowerShell, not `cmd`, because `cmd` doesn't treat
the trailing `# usage` as a comment. Inside a Visual Studio developer prompt, the MSVC compiler is
also called `cl`, and whichever comes first on `PATH` wins.

### Or as shell functions

`cl`, `cx` and `ck` are small enough to keep as shell functions instead. Add this to `~/.zshrc` (or
`~/.bashrc`):

```bash
# cl a --resume <id>  ==  CLAUDE_CONFIG_DIR=~/.claude-a claude --resume <id>
# cl -k a ... runs the installed claude-k instead (-z: claude-z)
cl() {
  local run=claude
  case $1 in -z|-k) run=claude$1; shift ;; esac
  [ -n "$1" ] || { echo "usage: cl [-z|-k] <profile> [claude args...]  (uses ~/.claude-<profile>)" >&2; return 2; }
  CLAUDE_CONFIG_DIR="$HOME/.claude-$1" "$run" "${@:2}"
}

# cx a resume <id>  ==  CODEX_HOME=~/.codex-a codex resume <id>
cx() {
  [ -n "$1" ] || { echo "usage: cx <profile> [codex args...]  (uses ~/.codex-<profile>)" >&2; return 2; }
  mkdir -p "$HOME/.codex-$1" && CODEX_HOME="$HOME/.codex-$1" codex "${@:2}"
}

# ck a --session <id>  ==  KIMI_CODE_HOME=~/.kimi-code-a kimi --session <id>
ck() {
  [ -n "$1" ] || { echo "usage: ck <profile> [kimi args...]  (uses ~/.kimi-code-<profile>)" >&2; return 2; }
  mkdir -p "$HOME/.kimi-code-$1" && KIMI_CODE_HOME="$HOME/.kimi-code-$1" kimi "${@:2}"
}
```

A shell function takes precedence over an installed command with the same name, and both do the
same thing. `hatcheck` needs the install; without it,
`find ~/.claude* ~/.codex* ~/.kimi-code* ~/.dsh* -name "*3f1c2a9e*"` finds a session's files.

### Removing it, or switching how it's installed

Each way of installing puts the seven commands in a folder of its own, and the copy that comes first
on your `PATH` is the one that runs: `which -a hatcheck` (`where.exe hatcheck` on Windows) lists
them all. A copy left in `~/.local/bin`, which is often first, hides one from Homebrew, and
Homebrew warns about it. So remove the old install when you switch:

| Installed with | Removed with |
|---|---|
| `install.sh` or `install.ps1`, from a clone | `./install.sh --uninstall` or `.\install.ps1 -Uninstall`, which remove only the links and launchers they made |
| the installer scripts from a release | deleting the seven commands from `~/.local/bin`: `rm ~/.local/bin/{hatcheck,hat,cl,cx,ck,claude-z,claude-k}` (on Windows, the `.exe` files in `~\.local\bin`) |
| Homebrew | `brew uninstall hatcheck` |
| uv or pipx | `uv tool uninstall hatcheck` or `pipx uninstall hatcheck` |
| cargo | `cargo uninstall hatcheck` |

- `hatcheck doctor` lists every install of hatcheck on your `PATH`, which one runs, and the line
  that removes each of the others; see
  [`hatcheck doctor`](#hatcheck-doctor-every-copy-of-hatcheck-and-the-agents).
- The release installers also added a line to your shell's startup files that puts `~/.local/bin`
  on your `PATH`. Other tools use that folder too, so it can stay.
- `hatcheck bind --hook` writes the path of the `hatcheck` it finds into each profile's hook. After
  a switch, run `hatcheck bind --hook` again with the new install, which points each hook at it.
  Before removing hatcheck for good, run `hatcheck unbind --hook`, or Claude Code, Codex and
  Antigravity CLI would try to run a command that's gone each time a session starts.
- Removing it leaves your profiles, sessions and logins as they are. What's left of hatcheck's own
  is its folder bindings, in `~/.config/hatcheck` (`%APPDATA%\hatcheck` on Windows), its cache
  folder (`~/Library/Caches/hatcheck` on macOS, `~/.cache/hatcheck` on Linux,
  `%LOCALAPPDATA%\hatcheck\Cache` on Windows), and in each profile, the `projects/` folder Claude
  Code makes for that cache's `claude-usage` folder when hatcheck reads usage. All of them can go.
  Desktop instances stay until `hatcheck desktop remove`.

## `hatcheck doctor`: every copy of hatcheck and the agents

```
$ hatcheck doctor
This is hatcheck 0.3.0, installed with uv: ~/.local/share/uv/tools/hatcheck/bin/hatcheck
Installs of hatcheck on your PATH, the first of each name running:
  1. uv: ~/.local/bin -> ~/.local/share/uv/tools/hatcheck/bin               0.3.0  all seven commands; runs
  2. Homebrew: /opt/homebrew/bin -> /opt/homebrew/Cellar/hatcheck/0.3.0/bin  0.3.0  all seven commands; shadowed
  To remove Homebrew's: brew uninstall hatcheck
The agents:
  Claude Code      2.1.303 (Claude Code)    ~/.local/bin/claude -> ~/.local/share/claude/versions/2.1.303  (the native installer, runs, and cl runs it)
                   2.0.69 (Claude Code)     /opt/homebrew/bin/claude -> /opt/homebrew/Caskroom/claude-code/2.0.69/claude  (Homebrew, shadowed)
  Codex            not on your PATH; cx needs it
  Antigravity CLI  1.4.2                    ~/.local/bin/agy  (the agy installer, runs)
  Kimi Code CLI    2.2.0                    ~/.kimi-code/bin/kimi  (the Kimi Code installer, runs, and ck runs it)
  ZCode            not on your PATH
  DeepSeek Harness not on your PATH
Hooks from hatcheck bind --hook:
  ~/.claude-a              Claude Code runs ~/.local/bin/hatcheck
⚠ To fix: 2 installs of hatcheck are on your PATH; keep one.
```

Each way of installing puts hatcheck's seven commands in a folder of its own, and the first copy of
each name on your `PATH` is the one that runs, so a second install sits there unused, or runs for
some of the names and not others. `hatcheck doctor` finds every copy, says how it got there (uv,
pipx, Homebrew, cargo, winget, the release installer or a build from a clone, told by what each
links to and by the version each prints) and which runs, and prints the line that removes each of
the others, as in the table above. It changes nothing. It also lists every `claude`, `codex`,
`agy`, `kimi`, `zcode` and `dsh` on your `PATH` with its version and where it came from, since
`cl`, `cx`, `ck` and the resume or DSH web commands run the first one, and the hooks
`hatcheck bind --hook` added, flagging one that runs a hatcheck that has gone. A link to a file that's gone, which a shell may find first, a shell
function `cl()` or `cx()` in `~/.zshrc` or `~/.bashrc`, which wins over the command, and one of
the seven commands that isn't on your `PATH` at all are named too. The exit status is 1 when there's something to fix and 0 otherwise, and `--json` prints what
it found as data. What it says about the agents is for you to act on: Claude Code's `claude doctor`
and Codex's `codex doctor` know their own installs best. The release installers end by naming it,
and `install.sh` and `install.ps1` say when another hatcheck comes first on your `PATH`.
