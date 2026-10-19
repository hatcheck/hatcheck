# The agents and their launchers

How Claude Code, Codex and Antigravity CLI keep their profiles, homes and folders, and the
launchers that run them with one: `cl`, `cx`, and `claude-z` and `claude-k` for Z.ai and Kimi.
The commands are in [commands.md](commands.md), and installing in [install.md](install.md).

## `cl`: run Claude Code with a profile

```bash
cl work                           # Claude Code with ~/.claude-work (sign in on first run)
cl work --resume <session-id>     # resume one of that profile's sessions
cl personal --continue            # any claude option works
cl -k work --resume <session-id>  # that session on Kimi; -z for Z.ai (see below)
```

Plain `claude` keeps using `~/.claude`. Claude Code names each profile's Keychain entry after its
`CLAUDE_CONFIG_DIR` value, so keep one spelling of each path; `cl` always uses `~/.claude-<name>`.
In a folder tied to a profile with [`hatcheck bind`](commands.md#hatcheck-bind-tie-a-folder-to-a-profile), the
name can be left out: `cl --resume <session-id>` there uses that profile.

## `cx`: run Codex with a home per account

[Codex](https://github.com/openai/codex) keeps its login, config and sessions in `~/.codex`, and
`CODEX_HOME` points it at another folder, so you can keep one home per ChatGPT account the same way:
`~/.codex-work`, `~/.codex-personal`, and so on.

```bash
cx work login                     # sign in once, with the work account
cx work                           # Codex with ~/.codex-work
cx work resume <session-id>       # resume one of that home's sessions
cx personal exec "run the tests"  # any codex command works
```

- `cx` makes the folder on first use, since Codex won't start with a `CODEX_HOME` that doesn't exist.
- A new home starts without your `~/.codex/config.toml` (model, trusted folders, MCP servers); copy
  it over if you want the same settings.
- Plain `codex` keeps using `~/.codex`.
- In a folder tied to a home with [`hatcheck bind`](commands.md#hatcheck-bind-tie-a-folder-to-a-profile), the name
  can be left out: `cx resume <session-id>` there uses that home. A Codex command such as `resume`
  counts as a home's name only when `~/.codex-resume` exists; anywhere else, `cx resume …` says it
  needs a home rather than making `~/.codex-resume`.

## Antigravity CLI (`agy`)

[Antigravity CLI](https://antigravity.google/docs/cli/) keeps its conversations, settings and login
under `~/.gemini/antigravity-cli`, and nothing but `HOME` points it anywhere else, so hatcheck reads
that one folder, and there are no `agy` profiles or a launcher for them yet (see
[Roadmap](../README.md#roadmap)). What it reads, checked against agy 1.4.2:

- `conversation_summaries.db`, agy's index of its conversations: each one's title, folder and
  times, and whether it's the Antigravity app's or one agy ran itself. `hatcheck` lists the CLI's
  own from it, marked `agy`, with `agy --conversation <id>` to resume each; those agy ran itself
  aren't listed.
- The Antigravity app keeps its own conversations the same way, beside agy's in
  `~/.gemini/antigravity`, and each index mentions some of the other's, so each conversation is
  read from the folder it belongs to. The app's are hidden unless you pass `--all`, like
  Desktop's; listed, their line is plain `agy`, since agy doesn't resume them but imports one from
  the Antigravity tab of its `/resume` picker. `show`, `move` and `fork` read them from the app's
  own transcripts, so an app conversation can be carried to Claude Code or Codex; nothing is
  converted into the app's folder, which only the app writes.
- `brain/<id>/.system_generated/logs/transcript_full.jsonl`, the transcript agy writes beside each
  conversation: what you typed, what the model said, each tool call and its output. `hatcheck show`
  prints it, and `hatcheck move` and `fork` convert it. The conversation itself,
  `conversations/<id>.db`, holds protobuf with no public schema, which hatcheck never reads or
  writes.
- `presence/<id>.lock`, which agy holds while a conversation is open: `move` and `open` refuse one
  that's held, as for Codex. agy leaves the files behind, so only a held lock counts.
- Usage comes from agy itself: `hatcheck usage` runs `agy -p /usage --output-format json` from a
  folder of hatcheck's own (`agy-usage` in its cache folder) and shows the quota groups it answers
  with, one row per group, so hatcheck never reads agy's login. First it runs `agy models`, which
  agy answers only when signed in and otherwise refuses at once with "Please sign in", opening
  nothing: asked for anything without a sign-in, agy's print mode opens the browser on Google's
  sign-in page and waits a minute for it, whatever stdin or `NO_BROWSER` say, which hatcheck must
  never set off. Signed out, the row says so and nothing else is run. agy's print mode answers
  `/usage` without starting a conversation (checked by counting them before and after). agy saves
  no snapshot, so hatcheck keeps the last reading in that folder and shows it with its age when a
  run gives nothing. On a business sign-in agy reported no quota groups, so the rows follow the
  field names agy 1.4.2 uses (`group_name`, `remaining_fraction`, `reset_time`) and haven't been
  checked against a Pro or Ultra account yet.
- A session converted for agy can't be written into its store, so `hatcheck move <id> agy` writes
  the conversation as a file in `imports` in hatcheck's cache folder and prints the command that
  starts agy on it: see [With Antigravity CLI](commands.md#with-antigravity-cli).
- `hatcheck bind default` binds its folder along with `~/.claude` and `~/.codex`, and
  `hatcheck bind --agy default` binds just that, so the session list marks its sessions in a folder
  bound elsewhere. `hatcheck bind --hook` adds a hook for agy too, in `~/.gemini/config/hooks.json`
  in agy's own form: see [`hatcheck bind`](commands.md#hatcheck-bind-tie-a-folder-to-a-profile).

## `claude-z`, `claude-k`: Claude Code on Z.ai or Kimi

`claude-z` and `claude-k` start Claude Code on [Z.ai](https://z.ai)'s GLM models or
[Kimi](https://www.kimi.com/code)'s, with your plan there, instead of on Anthropic's. `cl -z <name>` and
`cl -k <name>` do the same in a profile, so a session can switch between Claude and those models and
stay in its profile:

```bash
claude-k reconfig                   # once: your Kimi API key and model (claude-z reconfig for Z.ai)
claude-k                            # Claude Code on Kimi, with ~/.claude
cl -k work --resume <session-id>    # carry on a work-profile session on Kimi…
cl work --resume <session-id>       # …and later back on the work account's Claude plan
```

- Setup asks for the API key (from [z.ai](https://z.ai/manage-apikey/apikey-list) or the
  [Kimi Code Console](https://www.kimi.com/code)), the model, and an optional cap on output tokens.
  It saves them in `~/.config/claude-z/config` or `~/.config/claude-k/config` (`%APPDATA%\claude-z\config`
  on Windows), the files the [claude-z](https://github.com/yiwang/claude-z) launchers use, so a setup
  made with those works as is. `hatcheck provider` shows what's set up.
- Models: on Z.ai `glm-5.3[1m]` (the default, with `glm-5.3-flash` for quick tasks), `glm-5.3-flash`
  or `glm-4.7`; on Kimi `k3[1m]` (the default), `k3`, `kimi-for-coding` or
  `kimi-for-coding-highspeed`. `[1m]` turns on the 1M-token context window.
- The key reaches Claude Code as `ANTHROPIC_AUTH_TOKEN`, which it always uses ahead of the profile's
  claude.ai login and never asks about, and `ANTHROPIC_API_KEY` is cleared. Claude Code asks once per
  profile whether to use an `ANTHROPIC_API_KEY`, with No preselected, and after a No it sends the
  profile's claude.ai login token to the provider instead.
- The model is pinned with `ANTHROPIC_MODEL`, so a `"model"` in the profile's `settings.json`, such
  as `"fable"`, can't send a Claude model name to Z.ai or Kimi.
- Resuming a session on another provider sends it the whole conversation so far, so check that's
  allowed before doing it with a work account's sessions.
- `hatcheck usage` and the comments in `hatcheck` show the Claude and ChatGPT plans, not Z.ai's or
  Kimi's.

Coming from the claude-z repo: `./install.sh` replaces the copies its installers put in
`~/.local/bin`, and your config stays. On Windows, delete `%LOCALAPPDATA%\claude-z` (and `\claude-k`)
and take it off your `PATH` by hand, but keep `%APPDATA%\claude-z`, which holds your key; that repo's
uninstallers delete both. The permission mode its installers asked for isn't used (the old launchers
only ever applied `dangerously-skip-permissions`): set `permissions.defaultMode` in the profile's
`settings.json`, or pass `--permission-mode`. OpenRouter (`claude-o`) isn't included.
