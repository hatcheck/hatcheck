# Proposal: Antigravity CLI (agy) as the third agent

**Status:** phases 0 and 1 built, October 2026. [docs/agents.md](../agents.md#antigravity-cli-agy)
documents what hatcheck reads of agy as it is. Phase 2 is open: see the end.

Google's [Antigravity CLI](https://antigravity.google/docs/cli/), `agy`, replaced Gemini CLI in
June 2026. It keeps everything under `~/.gemini/antigravity-cli`: an SQLite index of its
conversations (`conversation_summaries.db`), one SQLite file of protobuf per conversation
(`conversations/<id>.db`, no public schema), a readable transcript beside each
(`brain/<id>/.system_generated/logs/transcript_full.jsonl`), a lock it holds while a conversation
is open (`presence/<id>.lock`), and its login (`antigravity-oauth-token`, or the keyring). Nothing
but `HOME` moves that folder; `agy -p /usage --output-format json` answers its usage headless
without starting a conversation. All of this was checked against agy 1.4.2 on 2026-10-19.

## Phase 0: an `Agent` enum

Every command told Claude Code and Codex apart by comparing the agent's name as text, with the
Claude Code branch as the fallback, so a third agent would have fallen into Claude Code's branch
everywhere. `agent.rs` names the agents as an enum the commands match on, which the compiler
checks is complete, with output byte for byte as before (the command tests held it to that).

## Phase 1: agy read-only, in its one folder

- `agy.rs` reads the index (through `rusqlite`, bundled, read-only), the transcript, the lock,
  and agy's `/usage`, and writes a conversation converted for agy as a file.
- `hatcheck` lists agy's conversations with `agy --conversation <id>`; the app's are hidden like
  Desktop's; `where`, `show`, `open`, `best` and `bind --agy` know them; `relocate` lists them as
  left alone, since the folder lives inside agy's database.
- `move` and `fork` convert an agy conversation into a Claude Code or Codex session through
  `conv::Conversation`, as Codex's are converted. To agy, nothing hatcheck could write is a
  conversation agy would resume, so the session is written as Markdown in hatcheck's cache
  folder and the command printed starts agy on it, with `--add-dir` so agy may read the file and
  `-i` with a first prompt. Checked: agy read such a file and summarised it correctly.
- Usage: agy's own `/usage`, run from a folder of hatcheck's own, with hatcheck keeping the last
  reading since agy saves none. On a business sign-in agy reported no quota groups, so the rows
  follow agy's field names and await a check on a Pro or Ultra account.
- Left out, and why: profiles. (`bind --hook` covers agy since: its `hooks.json` has named hooks,
  `SessionStart` handlers get `workspacePaths` and answer with `injectSteps`.)

## Phase 2, open: profiles

A second Google account needs a second `HOME`, as agy-pool and multigravity-cli do it, which
every command the agent runs would see too: git, gh, npm and the rest would read that home. A
launcher for it (`cg`, since `ag` is the Silver Searcher) would set `HOME=~/.agy-NAME`, and copies
between agy folders would copy `conversations/<id>.db`, `brain/<id>` and `annotations/<id>.pbtxt`
and rely on agy's reconcile of its index, which is untested. Whether hatcheck should do this is a
product decision for the maintainer; a data-folder variable from Google would make it moot.
