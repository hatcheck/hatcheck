#!/bin/sh
# cl <profile> [claude args...]
# Run Claude Code with the ~/.claude-<profile> config dir, which has its own
# login, settings and sessions:
#   cl work --resume <id>  ==  CLAUDE_CONFIG_DIR=~/.claude-work claude --resume <id>
# Keep the path spelled exactly like this: Claude Code files each profile's
# Keychain login under a hash of the CLAUDE_CONFIG_DIR string.
[ -n "$1" ] || { echo "usage: cl <profile> [claude args...]  (uses ~/.claude-<profile>)" >&2; exit 2; }
CLAUDE_CONFIG_DIR="$HOME/.claude-$1"
export CLAUDE_CONFIG_DIR
shift
exec claude "$@"
