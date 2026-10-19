#!/bin/bash
# install.sh
# Put hatcheck's commands on your PATH (macOS, Linux) from this repo: it builds them with
# cargo and symlinks them into a bin folder (default ~/.local/bin, where Claude Code's
# installer puts `claude`). The links point at the build, so after a git pull, run it again
# to build and update them. On Windows, use install.ps1. Shell startup files are not touched.
# Without a clone or Rust, install a release instead; see the README.
#
# Usage:
#   ./install.sh [--bin DIR] [--force]    build and link the commands
#   ./install.sh [--bin DIR] --from DIR   link the binaries already built in DIR instead
#   ./install.sh [--bin DIR] --uninstall  remove the links this script made
#
#   hatcheck                recent sessions in every profile and home, with resume commands and
#                           plan usage; hatcheck usage, where, move and provider do the rest
#   hat                     the short name for hatcheck, unless another program called hat is
#                           on your PATH
#   cl                      claude with a ~/.claude-<name> profile: cl work --resume <id>
#   cx                      codex with a ~/.codex-<name> home: cx work resume <id>
#   claude-z, claude-k      claude on Z.ai or Kimi instead of Anthropic
#
# Links this script made before, to the Python hatcheck and the sh launchers, are replaced.
# Links to the commands hatcheck replaced (claude-recent-sessions, claude-usage,
# claude-where, claude-move-session, claude-provider) are removed.
#
#   --force  replace files or links that already exist under those names. Copies of
#            claude-z and claude-k made by the claude-z repo's installers are replaced
#            without it: these do the same job and read the same config files.
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd -P)"
BIN_DIR="$HOME/.local/bin"
FROM=""
UNINSTALL=false
FORCE=false
while [ $# -gt 0 ]; do
    case "$1" in
        --bin) BIN_DIR="${2:?--bin needs a folder}"; shift ;;
        --from) FROM="${2:?--from needs a folder}"; shift ;;
        --force) FORCE=true ;;
        --uninstall) UNINSTALL=true ;;
        -h|--help) sed -n '2,/^set -e/p' "$0" | sed '$d; s/^# \{0,1\}//'; exit 0 ;;
        *) echo "Unknown option: $1 (see $0 --help)" >&2; exit 2 ;;
    esac
    shift
done
if [ -n "$FROM" ]; then
    FROM="$(cd "$FROM" && pwd -P)"
else
    FROM="$SCRIPT_DIR/target/release"
fi

COMMANDS="hatcheck hat cl cx claude-z claude-k"
# command:file it ran:hatcheck subcommand that replaced it
REPLACED="claude-recent-sessions:claude-recent-sessions.py: claude-usage:claude-usage.py:usage
claude-where:claude-where:where claude-move-session:claude-move-session.py:move
claude-provider:claude-provider.py:provider"

# A link this script made: to a binary in FROM, or to anything in this repo, as the links to
# the Python hatcheck and the sh launchers were.
ours() {
    [ -L "$1" ] || return 1
    target="$(readlink "$1")"
    case "$target" in
        "$FROM/$2" | "$SCRIPT_DIR"/*) return 0 ;;
        *) return 1 ;;
    esac
}

# Links to the commands hatcheck replaced, whose scripts are gone from this repo.
remove_replaced() {
    for triple in $REPLACED; do
        name="${triple%%:*}"
        rest="${triple#*:}"
        link="$BIN_DIR/$name"
        if [ -L "$link" ] && [ "$(readlink "$link")" = "$SCRIPT_DIR/${rest%%:*}" ]; then
            rm "$link"
            sub="${rest#*:}"
            echo "🗑  Removed $link: it's hatcheck${sub:+ $sub} now"
        fi
    done
}

# Another program called hat on the PATH, which the short name gives way to: prints its path.
# A hat from another install of hatcheck, which says so when asked, doesn't count.
foreign_hat() {
    other=$(command -v hat) || return 1
    "$other" --version 2>/dev/null | grep -qE '^(hat|hatcheck) [0-9]' && return 1
    echo "$other"
}

# A launcher that the claude-z repo's install-z.sh or install-k.sh copied here.
# shellcheck disable=SC2016  # the pattern matches a literal $HOME
claude_z_copy() {
    [ -f "$1" ] && [ ! -L "$1" ] && grep -q '^CONFIG_DIR="\$HOME/\.config/claude-[zk]"' "$1"
}

if $UNINSTALL; then
    remove_replaced
    for name in $COMMANDS; do
        link="$BIN_DIR/$name"
        if ours "$link" "$name"; then
            rm "$link"
            echo "🗑  Removed $link"
        elif [ -e "$link" ] || [ -L "$link" ]; then
            echo "ℹ️  Left $link alone: it doesn't link to this repo"
        fi
    done
    exit 0
fi

if [ "$FROM" = "$SCRIPT_DIR/target/release" ]; then
    if ! command -v cargo >/dev/null; then
        echo "install.sh builds hatcheck with cargo, which isn't on your PATH. Install Rust" >&2
        echo "(https://rustup.rs) and run it again, or install a release instead; see the README." >&2
        exit 1
    fi
    cargo build --release --locked -p hatcheck --manifest-path "$SCRIPT_DIR/Cargo.toml"
fi
for name in $COMMANDS; do
    if [ ! -x "$FROM/$name" ]; then
        echo "There's no $name in $FROM to link" >&2
        exit 1
    fi
done

mkdir -p "$BIN_DIR"
remove_replaced
skipped=0
for name in $COMMANDS; do
    src="$FROM/$name"
    link="$BIN_DIR/$name"
    if [ -L "$link" ] && [ "$(readlink "$link")" = "$src" ]; then
        echo "✅ $name (already linked)"
    elif [ -d "$link" ] && [ ! -L "$link" ]; then
        echo "⚠️  Skipped $name: $link is a folder"
        skipped=$((skipped + 1))
    elif claude_z_copy "$link"; then
        ln -sfn "$src" "$link"
        echo "✅ $name -> $src (replaced the copy from the claude-z repo; ~/.config/$name/config still applies)"
    elif ours "$link" "$name"; then
        was="$target"
        ln -sfn "$src" "$link"
        echo "✅ $name -> $src (it linked to $was)"
    elif { [ -e "$link" ] || [ -L "$link" ]; } && ! $FORCE; then
        echo "⚠️  Skipped $name: $link already exists (run again with --force to replace it)"
        skipped=$((skipped + 1))
    elif [ "$name" = hat ] && other=$(foreign_hat) && ! $FORCE; then
        # Only a short name, so it gives way to another program called hat.
        echo "ℹ️  Didn't add hat: $other has that name (--force adds it anyway); hatcheck does the same"
    else
        ln -sfn "$src" "$link"
        echo "✅ $name -> $src"
    fi
done

case ":$PATH:" in
    *":$BIN_DIR:"*) ;;
    *) echo "ℹ️  $BIN_DIR isn't on your PATH. Add this to ~/.zshrc (or ~/.bashrc):"
       echo "     export PATH=\"$BIN_DIR:\$PATH\"" ;;
esac
command -v claude >/dev/null || echo "ℹ️  cl runs claude, which isn't on your PATH: install Claude Code first"
# Another install of hatcheck ahead of this one on the PATH: the shell runs that one.
first=$(command -v hatcheck 2>/dev/null) || first=""
if [ -n "$first" ] && [ "$first" != "$BIN_DIR/hatcheck" ]; then
    echo "ℹ️  $first comes first on your PATH, so it runs, not $BIN_DIR/hatcheck; hatcheck doctor lists every copy"
fi
for rc in "$HOME/.zshrc" "$HOME/.bashrc"; do
    if [ -f "$rc" ] && grep -qE '^[[:space:]]*(function[[:space:]]+)?(cl|cx)[[:space:]]*\(\)' "$rc"; then
        echo "ℹ️  $rc defines cl() or cx() itself. A shell function wins over the"
        echo "   installed command; they do the same thing, so keep it or delete it."
    fi
done
[ "$skipped" -eq 0 ]
