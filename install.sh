#!/bin/bash
# install.sh
# Put this repo's Claude Code and Codex CLI helpers on your PATH (macOS, Linux) by
# symlinking them into a bin folder (default ~/.local/bin, where Claude Code's
# installer puts `claude`). On Windows, use install.ps1.
# The links follow the repo, so a git pull updates them. Shell startup files
# are not touched.
#
# Usage:
#   ./install.sh [--bin DIR] [--force]    link the commands
#   ./install.sh [--bin DIR] --uninstall  remove the links this script made
#
#   hatcheck                recent sessions in every profile and home, with resume commands and
#                           plan usage; hatcheck usage, where, move and provider do the rest
#   cl                      claude with a ~/.claude-<name> profile: cl work --resume <id>
#   cx                      codex with a ~/.codex-<name> home: cx work resume <id>
#   claude-z, claude-k      claude on Z.ai or Kimi instead of Anthropic (links to cl)
#
# The commands from before the rename are linked too, for now: claude-recent-sessions,
# claude-usage, claude-where, claude-move-session and claude-provider run hatcheck,
# hatcheck usage, where, move and provider.
#
#   --force  replace files or links that already exist under those names. Copies of
#            claude-z and claude-k made by the claude-z repo's installers are replaced
#            without it: these do the same job and read the same config files.
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd -P)"
BIN_DIR="$HOME/.local/bin"
UNINSTALL=false
FORCE=false
while [ $# -gt 0 ]; do
    case "$1" in
        --bin) BIN_DIR="${2:?--bin needs a folder}"; shift ;;
        --force) FORCE=true ;;
        --uninstall) UNINSTALL=true ;;
        -h|--help) sed -n '2,/^set -e/p' "$0" | sed '$d; s/^# \{0,1\}//'; exit 0 ;;
        *) echo "Unknown option: $1 (see $0 --help)" >&2; exit 2 ;;
    esac
    shift
done

# command:file-in-this-repo
COMMANDS="hatcheck:hatcheck.py cl:cl cx:cx claude-z:cl claude-k:cl
claude-recent-sessions:claude-recent-sessions.py claude-usage:claude-usage.py claude-where:claude-where
claude-move-session:claude-move-session.py claude-provider:claude-provider.py"

# A launcher that the claude-z repo's install-z.sh or install-k.sh copied here.
# shellcheck disable=SC2016  # the pattern matches a literal $HOME
claude_z_copy() {
    [ -f "$1" ] && [ ! -L "$1" ] && grep -q '^CONFIG_DIR="\$HOME/\.config/claude-[zk]"' "$1"
}

if $UNINSTALL; then
    for pair in $COMMANDS; do
        link="$BIN_DIR/${pair%%:*}"
        if [ -L "$link" ] && [ "$(readlink "$link")" = "$SCRIPT_DIR/${pair#*:}" ]; then
            rm "$link"
            echo "🗑  Removed $link"
        elif [ -e "$link" ] || [ -L "$link" ]; then
            echo "ℹ️  Left $link alone: it doesn't link to this repo"
        fi
    done
    exit 0
fi

mkdir -p "$BIN_DIR"
skipped=0
for pair in $COMMANDS; do
    name="${pair%%:*}"
    src="$SCRIPT_DIR/${pair#*:}"
    link="$BIN_DIR/$name"
    chmod +x "$src"
    if [ -L "$link" ] && [ "$(readlink "$link")" = "$src" ]; then
        echo "✅ $name (already linked)"
    elif [ -d "$link" ] && [ ! -L "$link" ]; then
        echo "⚠️  Skipped $name: $link is a folder"
        skipped=$((skipped + 1))
    elif claude_z_copy "$link"; then
        ln -sfn "$src" "$link"
        echo "✅ $name -> $src (replaced the copy from the claude-z repo; ~/.config/$name/config still applies)"
    elif { [ -e "$link" ] || [ -L "$link" ]; } && ! $FORCE; then
        echo "⚠️  Skipped $name: $link already exists (run again with --force to replace it)"
        skipped=$((skipped + 1))
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
if ! command -v python3 >/dev/null; then
    if [ "$(uname)" = Darwin ]; then hint="xcode-select --install"; else hint="your package manager's python3"; fi
    echo "ℹ️  hatcheck needs python3 ($hint)"
fi
command -v claude >/dev/null || echo "ℹ️  cl runs claude, which isn't on your PATH: install Claude Code first"
for rc in "$HOME/.zshrc" "$HOME/.bashrc"; do
    if [ -f "$rc" ] && grep -qE '^[[:space:]]*(function[[:space:]]+)?(cl|cx|claude-where)[[:space:]]*\(\)' "$rc"; then
        echo "ℹ️  $rc defines cl(), cx() or claude-where() itself. A shell function wins over the"
        echo "   installed command; they do the same thing, so keep it or delete it."
    fi
done
[ "$skipped" -eq 0 ]
