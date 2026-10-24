# The agents and their launchers

How Claude Code, Codex, Antigravity CLI, Kimi Code CLI, ZCode and DeepSeek Harness keep their
profiles, homes and folders, and the launchers that run them with one: `cl`, `cx`, `ck`, and
`claude-z` and `claude-k` for Claude Code on Z.ai and Kimi. The commands are in
[commands.md](commands.md), and installing in [install.md](install.md).

## `cl`: run Claude Code with a profile

```bash
cl a                              # Claude Code with ~/.claude-a (sign in on first run)
cl a --resume <session-id>        # resume one of that profile's sessions
cl b --continue                   # any claude option works
cl -k a --resume <session-id>     # that session on Kimi; -z for Z.ai (see below)
```

Plain `claude` keeps using `~/.claude`. Claude Code names each profile's Keychain entry after its
`CLAUDE_CONFIG_DIR` value, so keep one spelling of each path; `cl` always uses `~/.claude-<name>`.
In a folder tied to a profile with [`hatcheck bind`](commands.md#hatcheck-bind-tie-a-folder-to-a-profile), the
name can be left out: `cl --resume <session-id>` there uses that profile.

## `cx`: run Codex with a home per account

[Codex](https://github.com/openai/codex) keeps its login, config and sessions in `~/.codex`, and
`CODEX_HOME` points it at another folder, so you can keep one home per ChatGPT account the same way:
`~/.codex-a`, `~/.codex-b`, and so on.

```bash
cx a login                        # sign in once, with that account
cx a                              # Codex with ~/.codex-a
cx a resume <session-id>          # resume one of that home's sessions
cx b exec "run the tests"         # any codex command works
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
  run gives nothing. The two steps are two requests to Google and take five seconds or more
  together, the longest of any account's reading, so a reading under five minutes old is shown
  again, as cached with its age and `asked at most once in 5m`, and agy is asked after that.
  One hatcheck asks agy at a time, with a lock in that folder: two `/usage` runs at once made
  Google's quota service answer the second with a 503 after a minute, so a second hatcheck
  waits for the first and takes its reading the same way. agy 1.4.3 answers with each group's
  limits as `buckets`, each with a `window` (`5h` or `weekly`), a `remaining_fraction` and a
  `reset_time`, which hatcheck shows as one row per bucket with the window's length; the
  `windows` and `models` of 1.4.2's answer are
  still read. Checked against the answer agy 1.4.3 gave on 2026-10-20.
- A session converted for agy can't be written into its store, so `hatcheck move <id> agy` writes
  the conversation as a file in `imports` in hatcheck's cache folder and prints the command that
  starts agy on it: see [With Antigravity CLI](commands.md#with-antigravity-cli).
- `hatcheck bind default` binds its folder along with `~/.claude` and `~/.codex`, and
  `hatcheck bind --agy default` binds just that, so the session list marks its sessions in a folder
  bound elsewhere. `hatcheck bind --hook` adds a hook for agy too, in `~/.gemini/config/hooks.json`
  in agy's own form: see [`hatcheck bind`](commands.md#hatcheck-bind-tie-a-folder-to-a-profile).

## `ck`: run Kimi Code CLI with a home per account

[Kimi Code CLI](https://moonshotai.github.io/kimi-code/) keeps its sessions, settings and login in
one home folder, `~/.kimi-code`, and `KIMI_CODE_HOME` points it at another, so you can keep one
home per Kimi account the way `cx` keeps Codex homes: `~/.kimi-code-a`, `~/.kimi-code-b`,
and so on.

```bash
ck a login                        # sign in once, with that account (a device code)
ck a                              # Kimi Code CLI with ~/.kimi-code-a
ck a --session <session-id>       # resume one of that home's sessions
ck b -p "run the tests"           # any kimi option or command works
```

- `ck` makes the folder on first use, so `ck a login` starts a new home.
- A new home starts without your `~/.kimi-code/config.toml` (the default model, providers, hooks);
  kimi writes one when it first runs there, and you can copy yours over.
- Plain `kimi` keeps using `~/.kimi-code`.
- In a folder tied to a home with [`hatcheck bind`](commands.md#hatcheck-bind-tie-a-folder-to-a-profile),
  the name can be left out: `ck --session <session-id>` there uses that home. A Kimi command such
  as `login` counts as a home's name only when `~/.kimi-code-login` exists; anywhere else,
  `ck login` says it needs a home rather than making one.

## Kimi Code CLI (`kimi`)

hatcheck reads every `~/.kimi-code*` folder that has a `sessions/` folder. What it reads and
writes, checked against kimi 2.2.0:

- `sessions/wd_<name>_<hash>/session_<uuid>/`: one folder per session, under a key kimi makes from
  the folder the session ran in (its name, and the first 12 hex digits of the SHA-256 of its path).
  `state.json` holds the title, the folder, the times and an absolute path into the home;
  `agents/main/wire.jsonl` is the event stream kimi resumes from, with one per subagent beside it.
  hatcheck reads the sessions from these folders, not from the index.
- `session_index.jsonl`, at the home's root: one line per session with its folder and working
  directory as absolute paths, which `kimi --session` resolves a session through. A session
  hatcheck writes or moves gets its line there, and a moved session's line goes with it. kimi has
  had two lines land on one ([MoonshotAI/kimi-code#1925](https://github.com/MoonshotAI/kimi-code/issues/1925)),
  so hatcheck gives the file a newline first when it ends without one.
- The wire's `agent.message.appended` records carry the conversation: what you typed, the replies
  with their tool calls, and each tool's output. `hatcheck show` prints it, and `move` and `fork`
  convert it. Kimi's thinking is plain text in it, left out as the other agents' is. Nothing in a
  session is signed or encrypted, so a session folder copied to another home runs there as it is.
- hatcheck names a Kimi session by the UUID after `session_`, and puts the prefix back in the
  commands it prints and the files it writes; a pasted `session_…` is understood.
- Nothing marks a session open: kimi writes no lock and no pid
  ([#2848](https://github.com/MoonshotAI/kimi-code/issues/2848)), so `move` can't refuse an open
  one. Quit kimi before moving a session; the check of every copied byte before anything is
  deleted still holds.
- Usage comes from the endpoint kimi's own `/usage` asks, `…/coding/v1/usages` under the API host
  the home's `region` file names (kimi.com, or kimi.ai for the global region), with the access
  token from `credentials/kimi-code.json` sent as it is and never refreshed: the 5-hour, 7-day and
  monthly limits, each the share used and when it resets. The token lasts 15 minutes (kimi 2.2.0
  writes `expires_in: 900`) and kimi renews it as it runs, so a live reading needs kimi to have
  run in the last quarter of an hour; otherwise, as when the request fails, the last reading
  hatcheck kept is shown with its age, since kimi saves none. hatcheck never refreshes the token
  itself: a refresh would rotate it under kimi. kimi's print mode can't be asked instead: `kimi -p /usage` sends the
  words to the model, and every print-mode run leaves a session folder behind. Nothing else of the
  login is read.
- `hatcheck move <id> kimi` converts a session into a Kimi session, written as kimi's own importer
  writes one: see [With Kimi Code CLI](commands.md#with-kimi-code-cli). `hatcheck bind default`
  binds `~/.kimi-code` along with the rest when it has sessions, `bind NAME` binds `~/.kimi-code-NAME`
  when it exists, and `--kimi NAME` picks one. No hook is added for kimi: its hooks live in
  `config.toml`.

## ZCode (`zcode`)

[ZCode](https://zcode.z.ai/en) keeps its sessions in a SQLite database, `~/.zcode/cli/db/db.sqlite`,
shared with the ZCode desktop app, and nothing but `HOME` moves it (`ZCODE_STORAGE_DIR` and
`ZCODE_DATA_BASE_DIR` move other parts of `~/.zcode`), so hatcheck reads that one folder, and there
are no ZCode profiles or a launcher for them (see [Roadmap](../README.md#roadmap)). What it
reads, checked against ZCode 0.16.9:

- The database's `session` rows: the ID (`sess_<uuid>`), the folder the session ran in, its title
  and times, and whether ZCode ran it itself (a subagent), which aren't listed. Its `message` and
  `part` rows hold the conversation: text, each tool call with its input and output, and ZCode's
  reasoning, which is left out. hatcheck opens the database read-only and never writes it:
  [the proposal](proposals/kimi-zcode.md#writing-into-zcodes-database-the-risk) says why.
- `cli/artifacts/sess_<id>/` beside it holds tool results spilled to files, which `where` lists
  when there is one. `v2/` is the desktop app's, with the login, which hatcheck never reads.
- hatcheck names a session by the UUID after `sess_`, and puts the prefix back in
  `zcode --resume sess_…`; a pasted `sess_…` is understood.
- Nothing marks a session open.
- `hatcheck move <id> zcode` writes the conversation as a file and prints the command that starts
  zcode on it: see [With ZCode](commands.md#with-zcode).
- The `zcode` command: the ZCode desktop app ships the CLI as a Node bundle, `Resources/glm/zcode.cjs`
  inside the app, and puts no `zcode` on the `PATH`; ZCode's source repo builds the same bundle, or
  a single-file binary that has everything in it. Run from the app's bundle, the CLI needs
  `ZCODE_BUILTIN_PROVIDER_CONFIG_FILE` pointed at the app's `Resources/config/provider/zcode-builtin.json`,
  which the app passes it. A wrapper on the `PATH` does the same, and is how the checks here were
  run:

  ```sh
  #!/bin/sh
  ZCODE_BUILTIN_PROVIDER_CONFIG_FILE=/Applications/ZCode.app/Contents/Resources/config/provider/zcode-builtin.json \
    exec node /Applications/ZCode.app/Contents/Resources/glm/zcode.cjs "$@"
  ```
- `hatcheck usage` lists `~/.zcode` with no data and why: zcode has no usage command, and the CLI's
  bundle asks no usage endpoint; Z.ai shows the plan's usage at z.ai/manage-apikey/subscription.
- `hatcheck bind default` binds `~/.zcode` along with the rest when it has sessions, and
  `--zcode default` binds just it. No hook is added for zcode.

## DeepSeek Harness (`dsh`)

[DeepSeek Harness](https://github.com/deepseek-ai/deepseek-harness) keeps its home at `~/.dsh`, and
`DSH_HOME` moves that home whole. hatcheck discovers `~/.dsh`, every `~/.dsh-NAME` with sessions
and the home `DSH_HOME` names; `dsh` and `dsh:NAME` name these in profile arguments. There is no
hatcheck DSH launcher or installer alias. DSH's own profiles are plugin stacks under
`DSH_HOME/profiles`, not hatcheck account homes: the shipped app profiles are `web`, `headless`,
`acp`, `sdk` and `sdk-minimal`, while `tui` is only an example custom profile name in DSH's docs.
What hatcheck reads, checked against dsh 0.2.1-rc.2:

- `sessions/<project>/<id>/session.v4.jsonl[.zstd]`: the current v4 log, raw or as concatenated
  Zstandard frames. hatcheck selects the highest canonical generation; an unsupported or corrupt
  newest one is never passed over for an older one. The header supplies the session's cwd, start
  time and origin.
- `session/title`, `request/context` and `model/selection` supply the title and model. `show` and
  conversions understand direct `source.kind=user` prompts, assistant text and tool calls, tool
  results, compact checkpoints and surface replacements. Runtime or injected context, reasoning,
  images and files, replaced history and subagent sessions are left out; subagents are hidden from
  the session list as usual.
- A conventional `session-<uuid>` is shown without `session-` in JSON and short IDs; a pasted full
  ID is understood.
- On POSIX, a held `session.lock` marks the session open in `dsh`: it stays listed even when older
  than the time window, and `open` and `move` refuse it. A stale lock file isn't open. Windows uses
  a named semaphore, which hatcheck doesn't detect, so it neither marks nor refuses an open DSH
  session there.
- The session's command is `dsh web`, or `DSH_HOME=… dsh web` for a named or custom home (in
  PowerShell, `$env:DSH_HOME=…; dsh web; $env:DSH_HOME=$null`). From the session's recorded cwd it
  opens the official web UI and its session history; choose the session there. The chosen ID isn't
  deep-linked or resumed. This is the only DSH open or sign-in command hatcheck uses:
  `hatcheck open --login dsh[:NAME]` starts it explicitly so sign-in stays with DSH, rather than
  inventing a `dsh login` command.
- `hatcheck usage` lists one no-data account per DSH home, with agent `dsh` and plan `DeepSeek`:
  `dsh has no plan-usage command or endpoint (0.2.1-rc.2)`. It reads no DSH credentials and makes
  no DSH usage request.
- `move` and `fork` convert DSH sessions out to existing writable targets, always keeping the
  original. A DSH target is refused, and so are copies between DSH homes: the logs and workspace
  state aren't safe for hatcheck to copy. hatcheck never writes DSH's store or converts into it.
- `relocate` lists affected DSH sessions as skipped: the header's cwd and project key are
  immutable, and the workspace state stays as it was. `--dry-run` changes nothing.
- `hatcheck bind default` binds `~/.dsh` along with the rest, `bind NAME` binds `~/.dsh-NAME` when
  it has sessions, and `--dsh NAME` or `--dsh default` picks one explicitly. `bind --which dsh`
  prints `default`, `NAME` or the bound path. An agent-only binding command with no PROFILE keeps
  the existing DSH binding unless `--dsh` replaces it. No hook is added for DSH.

## `claude-z`, `claude-k`: Claude Code on Z.ai or Kimi

`claude-z` and `claude-k` start Claude Code on [Z.ai](https://z.ai)'s GLM models or
[Kimi](https://www.kimi.com/code)'s, with your plan there, instead of on Anthropic's. `cl -z <name>` and
`cl -k <name>` do the same in a profile, so a session can switch between Claude and those models and
stay in its profile:

```bash
claude-k reconfig                   # once: your Kimi API key and model (claude-z reconfig for Z.ai)
claude-k                            # Claude Code on Kimi, with ~/.claude
cl -k a --resume <session-id>       # carry on a session in ~/.claude-a on Kimi…
cl a --resume <session-id>          # …and later back on its account's Claude plan
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
- Resuming a session on another provider sends it the whole conversation so far, so check that
  what's in it may go there first.
- `hatcheck usage` and the comments in `hatcheck` show the Claude and ChatGPT plans, not Z.ai's or
  Kimi's.

Coming from the claude-z repo: `./install.sh` replaces the copies its installers put in
`~/.local/bin`, and your config stays. On Windows, delete `%LOCALAPPDATA%\claude-z` (and `\claude-k`)
and take it off your `PATH` by hand, but keep `%APPDATA%\claude-z`, which holds your key; that repo's
uninstallers delete both. The permission mode its installers asked for isn't used (the old launchers
only ever applied `dangerously-skip-permissions`): set `permissions.defaultMode` in the profile's
`settings.json`, or pass `--permission-mode`. OpenRouter (`claude-o`) isn't included.
