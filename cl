#!/bin/sh
# cl [-z|-k] <profile> [claude args...]
# Run Claude Code with the ~/.claude-<profile> config dir, which has its own
# login, settings and sessions:
#   cl work --resume <id>  ==  CLAUDE_CONFIG_DIR=~/.claude-work claude --resume <id>
# -z or -k runs it on Z.ai (GLM) or Kimi instead of Anthropic, with the key and
# model `hatcheck provider` keeps (set them up with claude-z reconfig or claude-k reconfig).
# Linked as claude-z or claude-k, it does that without a profile, so with ~/.claude
# or whatever CLAUDE_CONFIG_DIR already says: claude-k --resume <id>
# Keep the path spelled exactly like this: Claude Code files each profile's
# Keychain login under a hash of the CLAUDE_CONFIG_DIR string.
provider=
# `hatcheck provider`, or claude-provider in an install from before the rename.
provider_settings() {
    if command -v hatcheck >/dev/null 2>&1; then
        hatcheck provider "$@"
    elif command -v claude-provider >/dev/null 2>&1; then
        claude-provider "$@"
    else
        echo "${0##*/}: hatcheck isn't on your PATH; run hatcheck's install.sh" >&2
        return 1
    fi
}
case ${0##*/} in
    claude-z) provider=z ;;
    claude-k) provider=k ;;
esac
if [ -n "$provider" ]; then
    if [ "$1" = reconfig ]; then provider_settings setup "$provider"; exit; fi
else
    case $1 in -z|-k) provider=${1#-}; shift ;; esac
    case $1 in
        ""|-*) echo "usage: cl [-z|-k] <profile> [claude args...]  (uses ~/.claude-<profile>)" >&2; exit 2 ;;
    esac
    CLAUDE_CONFIG_DIR="$HOME/.claude-$1"
    export CLAUDE_CONFIG_DIR
    shift
fi
if [ -n "$provider" ]; then
    settings=$(provider_settings env "$provider") || exit
    eval "$settings"
fi
exec claude "$@"
