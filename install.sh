#!/bin/bash
# install.sh
# Put this repo's Claude Code CLI helpers on your PATH (macOS, Linux) by
# symlinking them into a bin folder (default ~/.local/bin, where Claude Code's
# installer puts `claude`). On Windows, use install.ps1.
# The links follow the repo, so a git pull updates them. Shell startup files
# are not touched.
#
# Usage:
#   ./install.sh [--bin DIR] [--force]    link the commands
#   ./install.sh [--bin DIR] --uninstall  remove the links this script made
#
#   cl                      claude with a ~/.claude-<name> profile: cl work --resume <id>
#   claude-where            where a session is stored, by ID or ID prefix
#   claude-recent-sessions  recent sessions in every profile, with resume commands and plan usage
#   claude-usage            plan usage left for every profile
#   claude-move-session     copy a session to another profile (move it with --no-keep)
#
#   --force  replace files or links that already exist under those names
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
COMMANDS="cl:cl claude-where:claude-where claude-recent-sessions:claude-recent-sessions.py claude-usage:claude-usage.py claude-move-session:claude-move-session.py"

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
    echo "ℹ️  claude-recent-sessions, claude-usage and claude-move-session need python3 ($hint)"
fi
command -v claude >/dev/null || echo "ℹ️  cl runs claude, which isn't on your PATH: install Claude Code first"
for rc in "$HOME/.zshrc" "$HOME/.bashrc"; do
    if [ -f "$rc" ] && grep -qE '^[[:space:]]*(function[[:space:]]+)?(cl|claude-where)[[:space:]]*\(\)' "$rc"; then
        echo "ℹ️  $rc defines cl() or claude-where() itself. A shell function wins over the"
        echo "   installed command; they do the same thing, so keep it or delete it."
    fi
done
[ "$skipped" -eq 0 ]
