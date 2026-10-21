# Proposal: Kimi Code CLI and ZCode as the fourth and fifth agents

**Status:** phases 0, 1 and 2 built, October 2026 (unreleased, see `CHANGELOG.md`).
[docs/agents.md](../agents.md) documents what hatcheck reads and writes of each agent as it is;
the end of this page says what was built and checked, and what is open.

Moonshot's [Kimi Code CLI](https://moonshotai.github.io/kimi-code/), `kimi`, and Z.ai's
[ZCode](https://zcode.z.ai/en), `zcode`, are the two agents the maintainer runs beside Claude Code,
Codex and Antigravity CLI. Both were probed on 2026-10-20 on the maintainer's Mac, with `kimi`
2.2.0 and ZCode 0.16.9, using only commands that need no sign-in and start nothing: `--help`,
`kimi session list --json`, `zcode doctor --json`, and reading their folders. They land on opposite
ends of the shape hatcheck already has. Kimi is the Codex case: one variable moves a whole home,
sessions are plain files, so `hatcheck move` between Kimi homes can be a real move and everything
else a conversion. ZCode is the agy case: its sessions live in a SQLite store that only `HOME`
moves, the login is the desktop app's too, so it starts read-only in one folder, with conversions
in and a file for zcode to read on the way out.

## What each agent keeps

### Kimi Code CLI

- Home: `~/.kimi-code`, moved whole by `KIMI_CODE_HOME`. Probed: pointed at an empty folder,
  `kimi session list` said "No sessions found" and made `device_id`, `cache/`, `workspaces.json`,
  `sessions/` and `logs/` there. So a second account is a second home, `~/.kimi-code-NAME`, signed
  in once with `kimi login` there, exactly as `~/.codex-NAME` is for Codex. Note that running
  `kimi` at all writes to its home, so hatcheck never runs it against a folder it shouldn't fill.
- Sessions: `sessions/wd_<name>_<hash>/session_<uuid>/`, where `<name>` is the folder's base name
  and `<hash>` the first 12 hex digits of the SHA-256 of its absolute path (checked:
  `/Volumes/ep4t/repos/murky-swamp` gives `eccd5d64c70f`). Inside: `state.json` (id, `cwd`,
  title, `lastPrompt`, `createdAt`, `updatedAt` in ms, `archived`, `forkedFrom`, and
  `agents.main.homedir`, an absolute path into the home), `agents/main/wire.jsonl` (the main
  agent's event stream, `protocol_version` 1.5, used to resume), `agents/<sub>/wire.jsonl` for
  each subagent, `agents/main/file-history/<hash>@vN` (file snapshots, keyed relatively),
  `logs/`, `notify/`, `tasks/`, `cron/`, `plans/`.
- The wire: `agent.message.appended` records carry the conversation, one message each, with
  `role` user, assistant or tool and `content` parts of type `text`, `think` (plain text, a
  `reasoningKey`, no signature) and `toolCalls` (name and JSON arguments), the tool role's text
  being the result. Around them: `turn.prompt`, `agent.turn.started`/`ended`, `llm.request` (the
  `modelAlias`, such as `kimi-code/k3`), `usage.record`, `file_history.tracked`. No absolute
  home path appears in the wire; only `state.json` and the index have them.
- Index: `session_index.jsonl` at the home's root, one `{sessionId, sessionDir, workDir}` per
  line with absolute paths, which `kimi -S` resolves a session through. Kimi has had a bug where
  two entries landed on one line ([#1925](https://github.com/MoonshotAI/kimi-code/issues/1925)),
  so anything appending to it makes sure the file ends in a newline first. Beside it, per
  folder: `file-history/<wdkey>` (`{"sessions":[{id, touchedAt}]}`) and
  `user-history/<md5 of the cwd>.jsonl` (prompt history).
- Resume: `kimi -S session_<uuid>` (`--session`), `kimi -c` for the folder's latest. Also
  `kimi fork [id]`, `kimi export [id]` (a zip), `kimi session list --all --json`, `kimi login`,
  `kimi -p '<prompt>' --output-format stream-json` for print mode, `--add-dir` to widen the
  workspace. Kimi's docs say not to hand-edit `sessions/`.
- Open sessions: nothing marks one. No pid or lock is written
  ([#2848](https://github.com/MoonshotAI/kimi-code/issues/2848)); `search-index/db.lock` is
  global.
- Login: `credentials/kimi-code.json`, mode 0600, one per home; `oauth/` and `config.toml`'s
  `[providers]` point at it. hatcheck never reads it.
- Usage: `/usage` exists in the TUI. Whether `kimi -p /usage` answers headless, and what print
  mode does when signed out (Kimi signs in with a device code), is unprobed.
- Hooks: `config.toml` has a `hooks` array with `event = "SessionStart"` entries and a command,
  so `bind --hook` could add one later.

### ZCode

- Home: `~/.zcode`. The CLI builds its session database's path from `os.homedir()` and nothing
  else (`.zcode/cli/db/db.sqlite`, read in the 0.16.9 bundle), so only `HOME` (`USERPROFILE` on
  Windows) moves it. `ZCODE_STORAGE_DIR` moves some `cli/` paths (`cli/exec` for one) and
  `ZCODE_DATA_BASE_DIR` the `v2/` folder with the login and provider config; the
  `ZCODE_SESSION_DB_PATH` the config docs mention isn't in 0.16.9. A second Z.ai account is
  therefore a second `HOME`, agy's open problem again, and the login is shared with the ZCode
  desktop app (`zcode logout` says "Remove the shared Z.AI login credentials").
- The command: ZCode's desktop app ships the CLI as a Node bundle,
  `/Applications/ZCode.app/Contents/Resources/glm/zcode.cjs`, and nothing put a `zcode` on this
  Mac's `PATH`; the source repo builds the same bundle or a single-file binary. `zcode --help` and
  `zcode doctor --json` in an empty `HOME` created nothing there, so both are safe probes.
- Sessions: SQLite, `cli/db/db.sqlite`, WAL, OpenCode's schema with ZCode's additions: `session`
  (`id` like `sess_<uuid>`, `project_id` like `proj_users-yi-repos-murky-swamp`, `parent_id` for
  subagents, `slug`, `directory`, `title`, `version` of the zcode that wrote it, `task_type`
  `interactive` or `subagent_child`, `time_created`/`updated`/`archived` in ms), `message` (JSON
  `data`: role, time, `parentID`, `modelID`, `providerID`, `path.cwd`, tokens, `semantics`,
  `contextSnapshot`), `part` (JSON `data` of type `text`, `reasoning`, `tool` with `callID`,
  `tool` and `state.input`/`output`, `step-start`, `step-finish`, `timeline`), `session_entry`
  (model selection, workspace checkpoints), `todo`, `permission`, `input_history`,
  `local_setting`, workflow tables, and `schema_migration`, 22 so far, the last at 0.16.5. Rows
  point at files beside the database: `cli/artifacts/sess_<id>/` (tool results spilled to files,
  as `zcode-artifact://` references), `cli/exec/sess_<id>/` (command output), `cli/agents/sess_<id>/`
  (subagent transcripts), `cli/rollout/model-io-*.jsonl` (diagnostic model I/O, three files kept).
- `v2/` is the desktop app's: `credentials.json`, `provider_config.json`, `coding-plan-cache.json`
  (which Z.ai plans the login is entitled to), `tasks-index.sqlite` (the app's own index of
  sessions), `setting.json`, and `sessions/<hash>/claude-import-*.json`, an older import format
  from June 2026. hatcheck reads none of it.
- Resume: `zcode --resume sess_<uuid>`, `zcode -c` for the folder's latest. `-p '<prompt>'`
  runs one prompt without the TUI, `--attach <path>` attaches a local file to it, `--cwd`,
  `--mode plan|yolo|…`, `--json`, `--no-browser` prints the sign-in URL instead of opening one.
  No session list or export command.
- Open sessions: nothing marks one. The bundle's locks (`<path>.lock` folders with `owner-*.json`)
  guard migrations and telemetry; `session_target.active_run_last_seen_at` is for goal runs only.
- Usage: no command in `--help`; `coding-plan-cache.json` says which plans are entitled, not what
  is left.

## Writing into ZCode's database: the risk

The question that decides ZCode's shape is whether `hatcheck move <session> zcode` may write rows
into `cli/db/db.sqlite`, the way it writes a Codex session file. The case against, from the probe:

- **The schema is private and moving.** 22 migrations in a few months, CHECK constraints on
  `task_type` and `title_source`, NOT NULL `version`, `slug`, `project_id` and `directory`, and
  `sequence` columns added later and null in older rows. The JSON in `message.data` and `part.data`
  carries `semantics`, `contextSnapshot`, `anchor`, `metadata` fields that zcode's reader maps with
  fallbacks today and may not tomorrow. hatcheck would fill `version` with a zcode version it isn't,
  and derive `project_id` and `slug` by copying an algorithm read from minified code.
- **The store is shared with a running app.** The desktop app has the same database open, keeps
  its own index in `v2/tasks-index.sqlite`, and already loses CLI-made sessions from its sidebar
  ([zai-org/feedback #617](https://github.com/zai-org/feedback/issues/617)). A row hatcheck adds
  is at best invisible there until the app reconciles, and at worst a row the app's reader trips
  over, taking the sidebar with it.
- **zcode's own writes are guarded.** Migrations run under a file lock hatcheck would have to
  honour, and the writer is the zcode version that owns the schema. SQLite itself takes a second
  writer fine; the application above it is what hasn't been designed for one.
- **hatcheck's rule.** It rewrites no agent's config and never writes where the agent didn't
  mean it to. A Codex session is a file in a folder Codex scans; a ZCode row is inside the
  database zcode owns. For agy the answer was a file agy reads, and nothing broke.

What zcode offers instead: `--attach` hands a file to a print-mode prompt, and that session
persists, so `zcode --cwd <folder> --attach <file> -p '<read it and carry on>'` creates a real
session from the conversation and `zcode -c` in the folder continues it. Also, the runtime holds
an importer that takes a conversation as `{source: "claudeCode", messages: [{role, content,
timestamp}]}` and writes the rows itself (`persistImportedSessionHistory` in the bundle), but it is
reached only through the ZCode Protocol app server the desktop app speaks, which isn't documented.
If Z.ai documents it or adds a `zcode import` command, hatcheck should use that and let zcode write
its own rows. Until then: no writes into the database. This is the product decision the
maintainer asked about, recorded here.

## Phase 0: the enum

`Agent` in `agent.rs` gains `Kimi` and `Zcode`, keys `kimi` and `zcode`, and the compiler walks
every match (14 files). Two places aren't enum-driven and generalise: `Target::resolve` in
`move_cmd.rs` hardcodes "the session's own home or Claude Code's" for Codex, and `find::place`
sniffs a path's agent from its folder names. Session IDs carry `session_` and `sess_` prefixes:
matching a typed prefix and the eight characters hatcheck prints go by the UUID after the prefix;
the resume command keeps the whole ID. `--json` gains `"agent": "kimi"` and `"zcode"`; fields
added, no version bump.

## Phase 1: Kimi

- `kimi.rs`: the homes (`~/.kimi-code` and `~/.kimi-code-*`), each session from its `state.json`
  (not the index, which the newline bug can break), the conversation from the wire into
  `conv::Conversation`: user text, replies, each tool call as a block with its arguments and the
  tool role's result; `think` parts left out as Codex's reasoning is.
- Listed with `kimi -S session_<uuid>` when the home is `~/.kimi-code` and with
  `KIMI_CODE_HOME=~/.kimi-code-NAME kimi -S …` otherwise, until a launcher exists; `where`, `show`,
  `open`, `best`, `bind --kimi`, `relocate` and `doctor` know the sessions.
- `move` and `fork` between Kimi homes copy the session folder with its timestamps, rewrite
  `agents.main.homedir` in the copy's `state.json`, add the index line (newline first) and the
  `file-history/<wdkey>` entry at the target, check every copied byte, then delete the original and
  its index line. Nothing in a Kimi session is signed or encrypted, so the copy should run on any
  account; that is what the resume probe has to show. A copy already there is compared as a Codex
  one is.
- To Kimi from the other agents, a conversion writes `state.json` and a wire with the messages,
  plus the index and file-history lines, and counts only once the real `kimi -S` has resumed one
  and gone on; if kimi refuses a wire it didn't write, the fallback is agy's: a Markdown file and
  `kimi --add-dir <folder> -p …`.
- No open-session check is possible; `move` says so in its docs and relies on the byte check.
- A launcher, `ck NAME` or a name the maintainer prefers, setting `KIMI_CODE_HOME`, with
  `ck NAME login` once, is a later step: it touches the installers, the winget manifests,
  `pyproject.toml` and the site, about 29 files for `claude-k` today.
- Usage waits for a probe: `kimi -p /usage` with an empty `KIMI_CODE_HOME` and a logging
  stand-in for `open`, to see whether a signed-out print mode starts the device-code sign-in.

## Phase 2: ZCode, read-only

- `zcode.rs`: `~/.zcode` only; `cli/db/db.sqlite` opened read-only through the `rusqlite` already
  in the tree; the `session` rows with no `parent_id` and `task_type` `interactive`, and each
  conversation from `message` and `part` in time order: text parts, tool parts as blocks with their
  input and output, reasoning left out; `directory` is the folder.
- Listed with `zcode --resume sess_<uuid>`; `where`, `show`, `open`, `best`, `bind --zcode` and
  `relocate` (left alone, the folder is in the database) know them. `doctor` finds `zcode` on the
  `PATH` or in the desktop app's bundle.
- To ZCode, a session is written as `imports/<id>.md` in hatcheck's cache folder and the printed
  command is `zcode --cwd <folder> --attach <file> -p '<prompt>'`, then `zcode -c` there. The
  session exists once zcode runs, so `--json` reports `"new_session": null` and the `file`, as for
  agy.
- Profiles and a launcher wait, as agy's do, for a data-folder variable from Z.ai; usage waits
  for a usage command.

## Probes, in the order that stops early

1. Kimi: copy a real session folder into an empty second `KIMI_CODE_HOME`, add the index line, run
   `kimi -S <id> -p 'say in one line what this session was about' --output-format stream-json`,
   and read success from the wire growing, never from the exit code.
2. Kimi: the same with a session hatcheck wrote from a Claude Code one.
3. ZCode: `--attach` with a converted conversation in print mode, with `HOME` at an empty folder
   and `ZCODE_DATA_BASE_DIR` at the real one so the login is found and the database isn't the real
   one; then `zcode -c` or `--resume` on the session it made.
4. Kimi and ZCode signed out, with a logging stand-in for `open` first on the `PATH`, before
   hatcheck ever runs either for usage.

## Built

Built on 2026-10-20, checked against kimi 2.2.0 and ZCode 0.16.9 on the maintainer's Mac:

- **Phase 0.** `Agent::Kimi` and `Agent::Zcode`, with `has_profiles`, `heading_name`, and
  `id_prefix`/`bare_id`/`full_id` for the `session_` and `sess_` prefixes; `Target` and
  `find::place` generalised. Every command tells the five agents apart by the enum.
- **Phase 1, Kimi.** `kimi.rs` reads the homes, the session folders, the wire and the index;
  `move` and `fork` copy a session folder between homes with its `state.json` repointed and its
  index line, compared by the `agents/` folder; conversions go both ways. The written session
  follows kimi's own importer, not its live wire: the live shape failed kimi's schema validation
  twice, and kimi binds its model only when its terminal UI opens the session, so print mode on a
  fresh import says no model is set (both in [docs/lessons.md](../lessons.md)). Checked: a Claude
  Code session converted this way was opened in kimi's UI through a pseudo-terminal, showed the
  conversation, and answered a question from it. Probe 1 (a copied session resumed in a second
  home) wasn't run: nothing in a session is signed, the copy is byte-identical but for
  `state.json`'s paths, and a second home would need its own sign-in.
- **Phase 2, ZCode.** `zcode.rs` reads the database read-only and converts a session's rows; a
  session converted for zcode is a file started with `zcode --attach <file> --mode plan -p …`,
  then `zcode -c`. Checked in a throwaway `HOME` with the login shared through
  `ZCODE_DATA_BASE_DIR`: zcode read the file, summarised the conversation correctly, resumed the
  session it made and answered from it, and hatcheck listed and showed that session from the
  database zcode had written. The desktop app's bundled CLI needs
  `ZCODE_BUILTIN_PROVIDER_CONFIG_FILE`; [docs/agents.md](../agents.md#zcode-zcode) has the wrapper.
- **Probe 1, run later the same day.** A session kimi had opened once was moved to a second home,
  `~/.kimi-code-probe`, where kimi's UI opened it with its history (signed out there, so no model
  turn), moved back, and opened again in the real home. Both homes were cleaned afterwards.
- **The `ck` launcher**, named by the pattern `cl` and `cx` set: `ck work login`, `ck work`,
  `ck work --session <id>`, with the folder made on first use and the binding used without a
  name; carried by the installers, the winget manifests, the PyPI package and the desktop app.
- **Kimi usage**, from the endpoint kimi's own `/usage` asks (`…/coding/v1/usages`, by region),
  with the home's token sent as it is and never refreshed, as Codex's is; the last reading kept
  for when the token has expired. A probe first showed that kimi's print mode doesn't answer
  `/usage` and leaves a session per run, so it isn't asked, and that signed out it exits at once
  and opens nothing (both in [docs/lessons.md](../lessons.md)). Checked against the live
  endpoint's answer on 2026-10-20.
- **Tests.** `tests/test_kimi.py`, `tests/test_zcode.py` and the `ck` cases in
  `tests/test_commands.py`, against a fake Kimi home and a fake ZCode database in
  `tests/helpers.py`; the usage tests use expired tokens, so nothing is sent.

Open, as decisions for the maintainer: ZCode profiles, which only `HOME` gives, and the database
write, which stays off unless Z.ai documents its import.
