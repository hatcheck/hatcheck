# The agents and their launchers

How Claude Code, Codex, Antigravity CLI, Kimi Code CLI, ZCode, DeepSeek Harness, MiniMax Code, Qwen
Code, GitHub Copilot CLI and OpenCode keep their profiles, homes and folders, and the launchers that
run them with one: `cl`, `cx`, `ck`, and `claude-z` and `claude-k` for Claude Code on Z.ai and Kimi.
The commands are in [commands.md](commands.md), and installing hatcheck in [install.md](install.md).

## Getting the agents

hatcheck installs none of them: each comes from its maker. Their own installers, as of 2026-10-24:

```bash
curl -fsSL https://claude.ai/install.sh | bash                # Claude Code: claude
curl -fsSL https://chatgpt.com/codex/install.sh | sh          # Codex: codex
curl -fsSL https://antigravity.google/cli/install.sh | bash   # Antigravity CLI: agy
curl -fsSL https://code.kimi.com/kimi-code/install.sh | bash  # Kimi Code CLI: kimi
npm install -g @deepseek-ai/dsh                               # DeepSeek Harness: dsh (needs Node.js)
curl -fsSL https://filecdn.minimax.chat/public/install.sh | bash # MiniMax Code: mcode
curl -fsSL https://qwen-code-assets.oss-cn-hangzhou.aliyuncs.com/installation/install-qwen-standalone.sh | bash # Qwen Code: qwen
curl -fsSL https://gh.io/copilot-install | bash               # GitHub Copilot CLI: copilot
curl -fsSL https://opencode.ai/v2/install | bash              # OpenCode: opencode (with an opencode2 shim beside it)
```

On Windows, in PowerShell:

```powershell
irm https://claude.ai/install.ps1 | iex
powershell -ExecutionPolicy ByPass -c "irm https://chatgpt.com/codex/install.ps1 | iex"
irm https://antigravity.google/cli/install.ps1 | iex
irm https://code.kimi.com/kimi-code/install.ps1 | iex
npm install -g @deepseek-ai/dsh
irm https://filecdn.minimax.chat/public/install.ps1 | iex
irm https://qwen-code-assets.oss-cn-hangzhou.aliyuncs.com/installation/install-qwen-standalone.ps1 | iex
winget install GitHub.Copilot
npm i -g @opencode/cli@latest
```

- Homebrew on macOS has the first three and Copilot:
  `brew install --cask claude-code codex antigravity-cli copilot-cli`; OpenCode is a formula,
  `brew install anomalyco/tap/opencode-v2` (its own tap, always current) or `brew install opencode`.
- ZCode has no installer for its CLI, which comes inside the ZCode app: see [ZCode](#zcode-zcode).
- DeepSeek runs DSH with `npx @deepseek-ai/dsh web`; installed with npm, it's the `dsh` command
  hatcheck's open commands run.
- MiniMax Code's own installer puts `mcode` at `~/.minimax-code/bin/mcode`. That install folder is
  distinct from its data home: see [MiniMax Code](#minimax-code-mcode).
- Qwen Code's installer puts the program in `~/.local/lib/qwen-code` with a `qwen` wrapper in
  `~/.local/bin` (`%LOCALAPPDATA%\qwen-code\bin` on Windows); `npm install -g
  @qwen-code/qwen-code@latest` and `brew install qwen-code` (a formula, not a cask) install it too.
  Its home, `~/.qwen`, is separate: see [Qwen Code](#qwen-code-qwen).
- GitHub Copilot CLI's installer puts `copilot` in `~/.local/bin` (`$PREFIX/bin`); `npm install -g
  @github/copilot` installs it too. Its home, `~/.copilot`, is separate: see
  [GitHub Copilot CLI](#github-copilot-cli-copilot).
- OpenCode's installer puts `opencode` in `~/.opencode/bin`, with an `opencode2` shim beside it
  that runs `opencode`, a leftover from when version 2 shipped beside version 1. Its data folder,
  `~/.local/share/opencode`, is separate: see [OpenCode](#opencode-opencode).
- `hatcheck doctor` then lists each agent on your `PATH`, its version and which copy runs.

The desktop apps:

| App | Platforms | Download |
|---|---|---|
| Claude Desktop, with Claude Code in it | macOS, Windows, Linux (beta) | [claude.com/download](https://claude.com/download) |
| ChatGPT, which now includes the Codex app | macOS, Windows, Linux (preview) | [learn.chatgpt.com/docs/app](https://learn.chatgpt.com/docs/app) |
| Antigravity | macOS, Windows, Linux | [antigravity.google/download](https://antigravity.google/download) |
| Kimi Code Desktop | macOS, Windows, Linux | [Kimi Code's quick start](https://www.kimi.com/code/docs/en/kimi-code-desktop/getting-started.html) |
| ZCode | macOS, Windows, Linux (beta) | [zcode.z.ai](https://zcode.z.ai/en) |
| DeepSeek Harness | macOS (Apple silicon), Windows | [deepseek.com/harness](https://deepseek.com/harness) |
| Qwen Code Desktop | macOS, Windows, Linux | [GitHub releases, `desktop-latest`](https://github.com/QwenLM/qwen-code/releases/tag/desktop-latest) |
| MiniMax Code | macOS, Windows | [MiniMax Code's download guide](https://agent.minimax.io/docs/code/get-started/download) |
| OpenCode | macOS, Windows, Linux | [opencode.ai](https://opencode.ai/) |

Homebrew on macOS has five of them:
`brew install --cask claude chatgpt antigravity zcode opencode-desktop`.

Their source, where the maker publishes the program's:

| Agent | Source | Licence |
|---|---|---|
| Codex | [openai/codex](https://github.com/openai/codex) | Apache-2.0 |
| Kimi Code CLI | [MoonshotAI/kimi-code](https://github.com/MoonshotAI/kimi-code) | MIT |
| DeepSeek Harness | [deepseek-ai/deepseek-harness](https://github.com/deepseek-ai/deepseek-harness) | MIT |
| MiniMax Code | [MiniMax-AI/minimax-code](https://github.com/MiniMax-AI/minimax-code) | MIT for its own code, with the third-party exceptions its `LICENSE-STATUS.md` lists |
| Qwen Code | [QwenLM/qwen-code](https://github.com/QwenLM/qwen-code) | Apache-2.0 |
| OpenCode | [anomalyco/opencode](https://github.com/anomalyco/opencode) | MIT |

Claude Code, Antigravity CLI, ZCode and GitHub Copilot CLI ship without theirs:
[anthropics/claude-code](https://github.com/anthropics/claude-code) and
[github/copilot-cli](https://github.com/github/copilot-cli) hold issues, a changelog and plugins, not
the program, and the other two have no repository.

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
[Roadmap](../README.md#roadmap)). What it reads, checked against agy 1.4.7 on 2026-10-24 (first
against 1.4.2):

- `conversation_summaries.db`, agy's index of its conversations: each one's title, folder and
  times, and whether it's the Antigravity app's or one agy ran itself. `hatcheck` lists the CLI's
  own from it, marked `agy`, with `agy --conversation <id>` to resume each; those agy ran itself
  aren't listed. A conversation's folder is the first it lists, except that agy 1.4.7 lists a
  folder added with `--add-dir` first: hatcheck's imports folder, which the command printed for a
  converted session adds, is passed over, so that conversation is filed under the folder it was
  started from.
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
  waits for the first and takes its reading the same way. agy 1.4.10 answers with one group, `All
  Models`, and a bucket per model and effort tier (`Gemini 4 Argon (High)`, `(Medium)`, `(Low)`,
  `Claude Opus 5.5 (Max)`, …), each with a `remaining_fraction` and a `reset_time` that is the
  epoch while nothing has been spent from it, meaning no reset ahead; its description says the
  buckets are views of three quota pools (Gemini Pro, Gemini Flash, Claude/GPT), each showing its
  pool's tighter cap. hatcheck shows a row per model, the tiers folded while they read the same
  and a row per reading named by its tiers when they don't, and in the session list's one-line
  comment folds the models that read the same into one meter (`Gemini 4 Argon +1`, or `all
  models`). agy 1.4.3 answered with each group's limits as `buckets`, each with a `window` (`5h`
  or `weekly`), shown as one row per bucket with the window's length, and the `windows` and
  `models` of 1.4.2's answer are still read. Checked against the answers agy 1.4.3 gave on
  2026-10-20, 1.4.7 on 2026-10-24 and 1.4.10 on 2026-10-29.
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
writes, checked against kimi 2.3.1 on 2026-10-24 (first against 2.2.0). kimi 2.3.1 writes protocol
1.5 in its wires and still resumes the 1.0 wire hatcheck writes, and it prints `kimi -r` to
resume, beside `--session`, which hatcheck prints:

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
reads, checked against ZCode 0.16.9, the CLI in the ZCode 3.15.3 app. The CLI in ZCode 3.16.3,
0.17.0, was compared with it on 2026-10-24 without being run, so as not to move the shared
database on: it adds the soft delete below and a table of its own, and takes the same flags.

- The database's `session` rows: the ID (`sess_<uuid>`), the folder the session ran in, its title
  and times, and whether ZCode ran it itself (a subagent), which aren't listed. ZCode 0.17.0 keeps
  the row of a session you delete, with `time_deleted` set, and hatcheck leaves it out as ZCode's
  own lists do. Its `message` and
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
- `move` and `fork` convert DSH sessions out, always keeping the original, and convert a session
  of another agent for DSH as a file: `dsh web` is printed with the prompt to paste into its web
  UI, which has the agent read the file. Copies between DSH homes are refused: the logs and
  workspace state aren't safe for hatcheck to copy, and hatcheck never writes DSH's store.
- `relocate` lists affected DSH sessions as skipped: the header's cwd and project key are
  immutable, and the workspace state stays as it was. `--dry-run` changes nothing.
- `hatcheck bind default` binds `~/.dsh` along with the rest, `bind NAME` binds `~/.dsh-NAME` when
  it has sessions, and `--dsh NAME` or `--dsh default` picks one explicitly. `bind --which dsh`
  prints `default`, `NAME` or the bound path. An agent-only binding command with no PROFILE keeps
  the existing DSH binding unless `--dsh` replaces it. No hook is added for DSH.

## MiniMax Code (`mcode`)

MiniMax Code keeps its current data home at `~/.minimax`, with named homes at `~/.minimax-NAME`.
`MINIMAX_DATA_DIR` and `MAVIS_DATA_DIR` are both trimmed; a nonempty `MINIMAX_DATA_DIR` selects the
home first, with `MAVIS_DATA_DIR` as its fallback. `mcode` and `mcode:NAME` name the default and
named homes in profile arguments. There is no hatcheck mcode launcher or installer alias, and no
`mcode --profile` is invented. `~/.minimax-code` is the installer, not a data home named `code`;
legacy `.mavis` data isn't merged or migrated.

What hatcheck reads is checked against the 0.6.5 source preview. The installed CLI is separate:
`~/.minimax-code/bin/mcode` is 0.6.13, and its `--help` verifies direct `mcode --session mvs_<id>`
resume and `mcode login`. That 0.6.13 copy isn't on this shell's `PATH`; doctor remains PATH-based
(see [`hatcheck doctor`](install.md#hatcheck-doctor-every-copy-of-hatcheck-and-the-agents)). A
session converted for mcode was read by that 0.6.13 on 2026-10-25, started on the file in its
terminal UI with the command hatcheck prints.

- `v2/sqlite/runtime-state.sqlite` is opened read-only, with no runtime initialization or migrations
  and no writes to the store. Only `local_runtime_sessions` rows with `columnar_version=3` are read.
  They supply the `mvs_<32 hex>` session ID, `workspace_dir`, `title`, `created_at_ms` and
  `updated_at_ms`, `archived`, `visibility`, `session_kind` and `session_type`, `purpose`,
  `agent_name`, `parent_session_id`, `history_relative_dir`, and `effectiveModel` in
  `extra_data_json`. `record_json` isn't used.
- The active canonical history is `messages.jsonl` in a session directory under `v2/sessions`. A
  nonempty `history_relative_dir` must have four safe parts with POSIX `/` separators on Windows
  too. With none, hatcheck tries the directory derived in UTC from the created time and session ID,
  then the manifest fallback. A fallback manifest must have `schemaVersion=1` and match the row's
  full `sessionId` and `createdAtMs`; more than one match is ambiguous, not a choice to make
  arbitrarily. Linked session directories, manifests and history files, and paths outside the
  session root, aren't accepted. Absolute paths recorded in a manifest aren't trusted.
- The list takes the catalog's metadata, while `show` and conversions take only that canonical
  history. An empty canonical file can coexist with a catalog title and separate display metadata:
  the session is still listed from the catalog, but `show` reports zero turns. Display rows are
  never merged to invent history.
- The conversation understands direct user text, assistant text, `toolCall` and `toolResult`, and
  `compactionSummary`. `turn_config`, `history_artifact`, previous generations, reasoning and
  signatures, hidden, custom and plugin records, UI-only notices, tool-output artifacts and
  subagent sessions are left out.
- An ordinary branch isn't a subagent merely because it has a parent or branch session type.
  Explicit `session_kind=task`, purposes starting `local-task:`, `local-background-task:` or
  `team-plan:`, and agent names `explore`, `worker` and `verifier` are subagents. The `peek`, `cron`
  and `channel` kinds, and `visibility=hidden`, are internal.
- `local_runtime_session_locks.expires_at_ms` decides whether a writer is active. An unexpired lease
  marks the session busy and blocks a duplicate open or a conversion; an expired one is stale.
  Unknown lock state is treated as busy, never as proof that the writer has died. An idle session
  merely selected in the TUI has no durable marker and can't be detected. This lease check is the
  same on every platform, without DSH's platform-specific lock handling.
- Resume is direct: exactly `mcode --session mvs_<id>`, or
  `MINIMAX_DATA_DIR=… mcode --session mvs_<id>` for a named or custom home. PowerShell clears the
  variable afterwards: `$env:MINIMAX_DATA_DIR=…; mcode --session mvs_<id>;
  $env:MINIMAX_DATA_DIR=$null`. `open` and `resume` use these commands, not a DSH-style picker or a
  headless import. `hatcheck open --login mcode[:NAME]` runs `mcode login` in the selected home.
- `hatcheck usage` lists one no-data account per mcode home, with the reason exactly
  `mcode plan quota isn't wired into hatcheck (0.6.13)`. MiniMax Code has a quota client, but
  hatcheck doesn't call it: no mcode auth or provider configuration is read, no mcode usage request
  is made, and no new network request class is added. `best` doesn't count this unknown usage as
  room.
- `move` and `fork` convert mcode sessions out, always keeping the original, and convert a
  session of another agent for mcode as a file its terminal UI is started on, with the prompt as
  its task (`mcode '…'`). Same-agent copies are refused: shared SQLite state, manifest paths and
  session assets aren't safe for hatcheck to copy, and hatcheck never writes mcode's store.
- `relocate` lists affected mcode sessions as skipped: shared SQLite state and absolute manifest
  paths with related assets stay as they were. `--dry-run` changes nothing.
- `hatcheck bind default` binds `~/.minimax` along with the rest only when its actual v2 store
  exists; `bind NAME` does the same for `~/.minimax-NAME`. `--mcode NAME`, `--mcode default` or
  `--mcode PATH` picks one explicitly and requires that store. `bind --which mcode` prints
  `default`, `NAME` or the bound path. An agent-only binding command with no PROFILE keeps the
  existing mcode binding unless `--mcode` replaces it. No hook is added for mcode.

## Qwen Code (`qwen`)

[Qwen Code](https://github.com/QwenLM/qwen-code) keeps everything under `~/.qwen`, and `QWEN_HOME`
moves that home whole: the settings, the login and each project's sessions. hatcheck discovers
`~/.qwen`, every `~/.qwen-NAME` and the home `QWEN_HOME` names, when qwen has run there (its
`installation_id`, or a project's chats); `qwen` and `qwen:NAME` name these in profile arguments.
There is no hatcheck qwen launcher or installer alias. `QWEN_RUNTIME_DIR`, which moves the session
data alone and leaves the login where it was, isn't followed. What hatcheck reads, checked against
qwen 0.26.0:

- `projects/<folder>/chats/<uuid>.jsonl`, where `<folder>` is the project folder's path with every
  character that isn't a letter or digit made a dash (lowercased first on Windows), and
  `chats/archive/` for the sessions qwen has archived. Each line is a record with `uuid`,
  `parentUuid`, `type`, `timestamp`, `cwd` and `gitBranch`; a message carries a Gemini-style
  `parts` list. The first record's `cwd` is the session's folder, the last record's time its
  activity, and the title is qwen's own `custom_title` record (from `/rename`, or generated) when
  there is one, else the first prompt, as `qwen sessions list` shows it. The model is the newest
  reply's. A transcript with no records isn't a session.
- `show` and conversions follow the `parentUuid` chain back from the last message, as qwen resumes
  a rewound session, and take the user's own messages (`provenance` `real_user`), the replies'
  text and `functionCall` parts, and tool results' `functionResponse` output. Parts marked
  `thought`, messages qwen injected itself (another provenance), records marked `isSidechain`
  (subagents' rounds) and qwen's system records are left out; images and files are marked.
- A subagent's transcript has a `parent_session` record, or a `session_source` of `agent`,
  `agent-host` or `side_task`; those stay out of the list. A `session_source` naming a channel, a
  scheduled task, the daemon or another host lists under that name with `--all`.
- `sessions/<pid>.json` is qwen's registry of the sessions running now, written at startup and
  removed on exit, and `chats/<uuid>.runtime.json` the sidecar its terminal UI writes and never
  removes. A session is open when either names it and the process is running (asked of `ps`, or
  of the process on Windows); a record of a process that has gone isn't. An open session stays
  listed past the time window, and `open` and `move` refuse it.
- The resume command is `qwen --resume <id>`, run from the session's folder, where qwen looks it
  up; a named or custom home uses `QWEN_HOME=… qwen --resume <id>` (in PowerShell,
  `$env:QWEN_HOME=…; qwen --resume <id>; $env:QWEN_HOME=$null`). `hatcheck open --login
  qwen[:NAME]` starts plain `qwen` in that home, where `/auth` picks the sign-in: qwen's `auth`
  subcommand has been removed.
- `hatcheck usage` lists one no-data account per qwen home, with agent `qwen` and plan `Qwen`:
  `qwen has no plan-usage command or endpoint (0.26.0)`. qwen's `usage/` files count the tokens
  each session spent, not what a plan has left; hatcheck reads neither them nor `oauth_creds.json`,
  and makes no request.
- `move` and `fork` convert qwen sessions out to existing writable targets, always keeping the
  original. To qwen, nothing hatcheck could write is a session qwen would resume, since qwen has no
  importer, so the session is written as a Markdown file, `imports/<id>.md` in hatcheck's cache
  folder, and the command printed starts qwen on it from the session's folder:
  `qwen --add-dir <the imports folder> -i '…'`, with `QWEN_HOME=…` for another home. The folder
  is added to the workspace so qwen may read the file, and the first prompt has it read the
  conversation and carry on; the session exists once qwen starts, so `--json` reports
  `"new_session": null` and the `file`. Checked with qwen 0.26.0, which read a Claude Code session
  forked for it as a brief and carried on from it. Copies between qwen homes are refused, since a
  session's file history, registry record and side state would have to go with it; hatcheck never
  writes qwen's store.
- `relocate` lists affected qwen sessions as skipped: the transcript is filed under the folder's
  name and records the folder on every line, and both stay as they were. `--dry-run` changes
  nothing.
- `hatcheck bind default` binds `~/.qwen` along with the rest when qwen has run there, `bind NAME`
  binds `~/.qwen-NAME` when it has, and `--qwen NAME`, `--qwen default` or `--qwen PATH` picks one
  explicitly. `bind --which qwen` prints `default`, `NAME` or the bound path. An agent-only
  binding command with no PROFILE keeps the existing qwen binding unless `--qwen` replaces it. No
  hook is added for qwen.

## GitHub Copilot CLI (`copilot`)

[GitHub Copilot CLI](https://github.com/github/copilot-cli) keeps everything under `~/.copilot`, and
`COPILOT_HOME` moves that home whole: its settings, its sessions and, when the system has no
credential store, its login, which hatcheck never reads. hatcheck discovers `~/.copilot`, every
`~/.copilot-NAME` and the home `COPILOT_HOME` names, when copilot has run there (its `session-state`
folder, or its `config.json`); `copilot` and `copilot:NAME` name these in profile arguments. There
is no hatcheck copilot launcher or installer alias. Whether copilot's system credential store keeps
one login per home is for the maintainer's probe in [maintenance.md](maintenance.md): hatcheck
reads no login and can't tell. What hatcheck reads, checked against copilot 1.0.103:

- `session-state/<uuid>/events.jsonl`, the record copilot writes as a session runs: an event per
  line with `type`, `data`, `id`, `timestamp` and `parentId`. `session.start` carries the folder,
  branch and copilot's version; `user.message` your message (`content`, before the date copilot
  prepends in `transformedContent`); `assistant.message` the reply's `content`, its `model` and
  its `toolRequests` (`toolCallId`, `name`, `arguments`, as copilot's own code writes them);
  `tool.user_requested` a command you ran with `!`; `tool.execution_complete` a tool's `result`.
  copilot's own records (`session.*`, `model.*`, `hook.*`, `assistant.reasoning`, `system.message`)
  stay out of the conversation. The last event's time is the session's activity, and a folder
  without `events.jsonl`, which copilot leaves when a session is started and never spoken in, isn't
  a session.
- `workspace.yaml` beside it: `id`, `cwd` (the session's folder), `branch`, `client_name`
  (`github/cli` for the CLI; another client's sessions wait for `--all`), `name` (copilot's own name
  for the session, from `-n`, `/rename` or the one it generates; else the title is the first
  prompt) and its times. `session-store.db` at the root, copilot's index for its session browser,
  isn't read.
- `inuse.<pid>.lock` in a session's folder names the process that has it open; copilot removes it
  when it quits, so one naming a process that has gone is a crash's, and isn't open. An open
  session stays listed past the time window, and `open` and `move` refuse it.
- The resume command is `copilot --resume <id>`, run from the session's folder; a named or custom
  home uses `COPILOT_HOME=… copilot --resume <id>` (in PowerShell, `$env:COPILOT_HOME=…; copilot
  --resume <id>; $env:COPILOT_HOME=$null`). `hatcheck open --login copilot[:NAME]` runs
  `copilot login` in that home: copilot's own sign-in, to its system credential store.
- `hatcheck usage` lists one no-data account per copilot home, with agent `copilot` and plan
  `Copilot`: `copilot's /usage runs only in a session, and hatcheck makes no request (1.0.103)`.
  copilot's `/usage` is a slash command inside a session, a model turn; hatcheck reads no
  credential and asks nothing.
- `move` and `fork` convert copilot sessions out to existing writable targets, always keeping the
  original, and convert a session of another agent for copilot as a file its own importer takes:
  `imports/<id>.jsonl` in hatcheck's cache folder, a version-1 session interchange file (a
  `session` header with the title, folder and branch, then a `message` record per turn), which
  `copilot sessions import <file>` validates and imports, giving the session an ID of its own that
  it prints for `copilot --resume`. Checked with copilot 1.0.103 on 2026-10-25, whose `sessions
  import --dry-run` accepted the file and whose import made the session folder, with the title as
  its name. Copies between copilot homes are refused: hatcheck never writes copilot's store.
- `relocate` lists affected copilot sessions as skipped: its workspace file and events record the
  folder, and stay as they were. `--dry-run` changes nothing.
- `hatcheck bind default` binds `~/.copilot` along with the rest when copilot has run there, `bind
  NAME` binds `~/.copilot-NAME` when it has, and `--copilot NAME`, `--copilot default` or
  `--copilot PATH` picks one explicitly. `bind --which copilot` prints `default`, `NAME` or the
  bound path. An agent-only binding command with no PROFILE keeps the existing copilot binding
  unless `--copilot` replaces it. No hook is added for copilot.

## OpenCode (`opencode`)

[OpenCode](https://opencode.ai) 2 keeps its data in the XDG data folder, `~/.local/share/opencode`,
or `XDG_DATA_HOME`'s `opencode`: one SQLite database, `opencode.db`, with every project's sessions,
their messages and its provider logins, which hatcheck never reads, and its logs, shell history and
repository cache beside it. Its settings are in `~/.config/opencode`, and the background service
plain opencode runs through registers under `~/.local/state/opencode`. hatcheck reads the one data
folder plain opencode uses, when opencode has run (its database exists); `opencode` names it in
profile arguments. There are no named OpenCode homes: a second store needs `XDG_DATA_HOME` or
`OPENCODE_DB` and `--standalone`, since the background service keeps the store it started with,
and which of those a profile should mean is a decision not taken (see `agent.rs`). `~/.opencode`,
where its installer puts the program, isn't a data folder, and the `opencode2` shim beside it runs
`opencode`. The version-1 store, `storage/` beside the database, is what opencode 2 imports on its
first run, and isn't read. What hatcheck reads, checked against opencode 2.1.4:

- `session_v2`: a row per session with its `id` (`ses_` and 26 characters; hatcheck shows the
  characters and takes either), `parent_id` (set on the child sessions subagents run, which stay
  out of the list), `directory` (the session's folder), `title`, `model` (shown as
  `provider/model`), `time_created`, `time_updated` (the session's activity), `time_archived`,
  `cost`, what the session cost in dollars as opencode prices its tokens (0 for a model it has no
  price for, which the list leaves out), and its tokens by kind for every model together,
  `tokens_input`, `tokens_output`, `tokens_reasoning`, `tokens_cache_read` and
  `tokens_cache_write`, which `hatcheck --details` shows. Nothing records a branch.
- `session_message`: a row per message, in `seq` order, with `type` and a JSON `data` record. A
  `user` message's `text` and `files` (marked as images or files), an `assistant` message's
  `content` parts (`text` kept, `reasoning` left out, `tool` as a call with its `state.input` and
  the text of its `state.content` or its error), a `shell` message's command and output, and a
  completed `compaction`'s summary make the conversation; `synthetic`, `system` and `skill`
  messages and the agent, model and folder switches are opencode's own. The first user message's
  text is the title when the row has none.
- Nothing marks a session open; `open` and `move` refuse none.
- The resume command is `opencode --session ses_<id>`, run from the session's folder, where
  opencode files it. A data folder other than the one plain opencode uses, reached by its path, is
  run with `OPENCODE_DB=<its opencode.db> opencode --standalone --session ses_<id>` (in
  PowerShell, `$env:OPENCODE_DB=…; opencode --standalone --session ses_<id>;
  $env:OPENCODE_DB=$null`), off the background service. `hatcheck open --login opencode` runs
  `opencode auth login`, opencode's own sign-in to a provider.
- `hatcheck usage` lists the data folder as one no-data account, with agent `opencode` and plan
  `OpenCode`: `opencode has no plan-usage command: its stats count tokens spent (2.1.4)`. Its
  plans are its providers' own; hatcheck reads no credential and asks nothing.
- `move` and `fork` convert opencode sessions out to existing writable targets, always keeping the
  original, and convert a session of another agent for opencode as a file its own importer takes:
  `imports/<id>.json` in hatcheck's cache folder, the `{info, messages}` session data `opencode
  session export` prints, with an ID minted as opencode mints them, which `opencode session import
  <file>` takes from the session's folder; the session then resumes with `opencode --session
  ses_<id>`. Checked with opencode 2.1.4 on 2026-10-25, with its service disabled, whose import
  took the file, listed the session with its title and folder and exported it back. A copy into
  its own store is refused as already there; hatcheck never writes opencode's database.
- `relocate` lists affected opencode sessions as skipped: the row records the folder in opencode's
  database, which hatcheck never writes. `--dry-run` changes nothing.
- `hatcheck bind default` binds `~/.local/share/opencode` along with the rest when opencode has
  run, and `--opencode default` or `--opencode PATH` picks one explicitly. `bind --which opencode`
  prints `default` or the bound path. An agent-only binding command with no PROFILE keeps the
  existing opencode binding unless `--opencode` replaces it. No hook is added for opencode.

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
- Models, as each provider's Claude Code guide and model list had them on 2026-10-24: on Z.ai
  `glm-5.3[1m]` (the default, with `glm-5.3-flash[1m]` for quick tasks) or `glm-5.3-flash[1m]`;
  on Kimi `k3.1-pro[1m]` (K3.1 Pro, the default), `k3.1-pro`, `k3.1-pro-highspeed`, `k3[1m]`,
  `k3`, `kimi-for-coding` (K2.8 Preview) or `kimi-for-coding-highspeed` (K2.7 Code Highspeed).
  `[1m]` turns on the 1M-token context window, which Kimi gives Pro and Allegretto plans and up;
  the others run with 256k. A model saved earlier, such as `glm-4.7`, is used as it is. Each was
  run through Claude Code 2.1.310, which sends K3.1 Pro the thinking it requires.
- Kimi is reached at `https://api.kimi.ai/coding/`, the address of Kimi's English guide; a key that
  works at `https://api.kimi.com/coding/`, the Chinese guide's, worked at both on 2026-10-24.
  `CK_BASE_URL` in the config names another address.
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
