# Session formats, and how a conversion works

Each agent keeps its sessions in a form of its own, and hatcheck reads every one of them as the
agent writes it, checked against the real binary, with the version written down. This page puts
the ten formats side by side, then says what a conversion keeps, drops, cuts and writes. The
detail of each agent, what is read and how it was checked, is in [agents.md](agents.md); what
each command does with it is in [commands.md](commands.md).

## The formats, side by side

| Agent | Where a session lives | What it is | ID | Open when | Checked against |
|---|---|---|---|---|---|
| Claude Code | `~/.claude[-NAME]/projects/<folder>/<uuid>.jsonl`, with a `<uuid>/` folder beside it for subagents and tool output; `<folder>` is the project folder's path with every character that isn't a letter or digit made a dash | JSONL, a record per line: `type` `user` or `assistant`, `uuid` and `parentUuid` linking them, `message.content` blocks (`text`, `tool_use`, `tool_result`, `thinking`); `custom-title`, `ai-title` and `agent-name` records name it; `isSidechain`, `isMeta` and `isCompactSummary` mark what isn't the conversation | UUID | `sessions/<pid>.json` names it and that process is running, with its start time | 2.1.310 |
| Codex | `~/.codex[-NAME]/sessions/YYYY/MM/DD/rollout-<local time>-<uuid>.jsonl`, a line per session in `session_index.jsonl`, `archived_sessions/` for the archived | JSONL: `session_meta` (cwd, git, source), `turn_context` (model), `response_item` (a `message` with a role, function calls and their outputs, encrypted reasoning), `event_msg` (what you typed, titles) | UUID | `thread-writer-locks/<id>.lock` held | 0.164.0 |
| Antigravity CLI | `~/.gemini/antigravity-cli/conversation_summaries.db`, and each conversation's `brain/<id>/.system_generated/logs/transcript_full.jsonl` | a SQLite index (title, folders, times, steps), and a JSONL of steps: `USER_INPUT`, `PLANNER_RESPONSE`, `GENERIC` from the model, each with its tool calls | UUID | agy's lock on the conversation | 1.4.7 |
| Kimi Code CLI | `~/.kimi-code[-NAME]/sessions/wd_<name>_<hash>/session_<uuid>/`, with `wire.jsonl` and `context.*` records, a wire per subagent beside it; `session_index.jsonl` at the root | a wire of events (protocol 1.5 written, 1.0 read): `turn.prompt`, `agent.message.appended` with user, assistant and tool messages, `turn.ended`; kimi folds the model's view from the `context.*` records | `session_<uuid>` | nothing marks one | 2.3.1 |
| ZCode | `~/.zcode/cli/db/db.sqlite`, shared with the ZCode app; `cli/artifacts/sess_<uuid>` beside it | tables `session`, `message` and `part`: a session's messages, each with its parts (`text`, `tool` with input and output) and reasoning; `time_deleted` marks a deleted one from 0.17.0 | `sess_<uuid>` | nothing marks one | 0.16.9, 0.17.0 compared |
| DeepSeek Harness | `~/.dsh[-NAME]/sessions/<project>/session-<uuid>/session.v4.jsonl`, or `.zstd` as concatenated Zstandard frames | a `session` header (cwd, origin), then events: `user/message` with its source (`user`, runtime context, compact checkpoints), `assistant/message` (reasoning, text, tool calls), `tool/result`, `session/title`, `model/selection`, `request/context`; a replace operation stands a compaction in for earlier events | `session-<uuid>` | `session.lock` held (POSIX) | 0.2.1-rc.2 |
| MiniMax Code | `~/.minimax[-NAME]/v2/sqlite/runtime-state.sqlite`, the catalog, and `v2/sessions/<date>/<…>/messages.jsonl`, the canonical history, with a manifest and a display timeline beside it | catalog rows (`local_runtime_sessions`, columnar version 3), and envelopes with `message_id`, `turn_id` and a `message`: `user`, `assistant` (text, `toolCall`, thinking with a signature), `toolResult`, `compactionSummary`, and records of its own | `mvs_<32 hex>` | an unexpired lease in `local_runtime_session_locks` | 0.6.13 (source 0.6.5) |
| Qwen Code | `~/.qwen[-NAME]/projects/<folder>/chats/<uuid>.jsonl`, archived ones in `chats/archive/`; `<folder>` as Claude Code names it, lowercased first on Windows | JSONL: `uuid` and `parentUuid`, `type` `user`, `assistant`, `tool_result` or `system`, `provenance`, `cwd`, `gitBranch`, a Gemini-style `message.parts` list (`text`, `thought`, `functionCall`, `functionResponse`); system records for titles, sources, parents and qwen's own state | UUID | `sessions/<pid>.json` or `chats/<uuid>.runtime.json` names a running process | 0.26.0 |
| GitHub Copilot CLI | `~/.copilot[-NAME]/session-state/<uuid>/events.jsonl`, with `workspace.yaml` (folder, branch, name, times) and copilot's working files beside it; `session-store.db` at the root indexes them | JSONL of events, each with `type`, `data`, `id`, `timestamp` and `parentId`: `session.start` (folder, branch, version), `user.message`, `assistant.message` (text, `model`, `toolRequests`), `tool.user_requested`, `tool.execution_complete`, and copilot's own (`session.*`, `model.*`, `hook.*`, `system.message`) | UUID | `inuse.<pid>.lock` in the session's folder names a running process | 1.0.103 |
| OpenCode | `~/.local/share/opencode/opencode.db` (`XDG_DATA_HOME`'s), one database for every project | tables `session_v2` (folder, title, model, times, parent) and `session_message`: a JSON record per message, `type` `user` (`text`, `files`), `assistant` (`content` parts: `text`, `reasoning`, `tool` with its input and result), `shell`, `compaction` or opencode's own; `credential` and `account` hold its logins, never read | `ses_<26 characters>` | nothing marks one | 2.1.4 |

One line of a few of them, shortened, so the shapes are concrete:

```
Claude Code  {"type":"assistant","uuid":"…","parentUuid":"…","sessionId":"…","cwd":"/code/api","message":{"role":"assistant","content":[{"type":"text","text":"…"},{"type":"tool_use","name":"Bash","input":{"command":"ls"}}]}}
Codex        {"type":"response_item","timestamp":"…","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"…"}]}}
Kimi         {"type":"agent.message.appended","agentId":"…","message":{"role":"assistant","content":[{"type":"text","text":"…"}]},"time":…}
DSH          {"type":"assistant/message","seq":2,"time":…,"surfaceOp":"append","data":{"turn":1,"message":{"role":"assistant","source":{"kind":"model"},"content":[{"type":"text","text":"…"}]}}}
MiniMax Code {"message_id":"…","turn_id":"…","message":{"role":"assistant","content":[{"type":"text","text":"…"},{"type":"toolCall","name":"read_file","arguments":"{\"path\":\"src/main.rs\"}"}]}}
Qwen Code    {"uuid":"…","parentUuid":"…","type":"assistant","provenance":"assistant_output","cwd":"/code/api","model":"GLM-5.3","message":{"role":"model","parts":[{"text":"…","thought":true},{"text":"…"},{"functionCall":{"name":"read_file","args":{"path":"src/main.rs"}}}]}}
Copilot      {"type":"assistant.message","data":{"content":"…","model":"mai-code-1.1-flash","toolRequests":[{"toolCallId":"…","name":"view","arguments":{"path":"src/main.rs"}}],"turnId":"0"},"id":"…","timestamp":"…","parentId":"…"}
OpenCode     session_message.data: {"agent":"build","model":{"providerID":"anthropic","id":"…"},"content":[{"type":"text","text":"…"},{"type":"tool","name":"read","state":{"status":"completed","input":{"filePath":"src/main.rs"},"content":[{"type":"text","text":"…"}]}}],"time":{"created":…,"completed":…}}
```

## How a conversion works

Every conversion goes through one plain form, `Conversation` in `crates/hatcheck/src/conv.rs`,
which `hatcheck show` prints and `hatcheck move` and `fork` write into the other agent's format: a
hub, with a reader for each agent on one side and a writer for some on the other. Nothing is
converted directly from one agent's format into another's.

**What a reader takes.** Your own messages, as text; the replies' text; each tool call as a block
of text, `[external_agent_tool_call: NAME]`, its input as `key: value` lines cut to 30 lines and
3,000 characters, `[/external_agent_tool_call]`; and each tool result as
`[external_agent_tool_result]` with the output cut to 20 lines and 2,000 characters. That is the
format Codex's own `/import` uses for Claude Code's tool calls, so every agent sees them the same
way. The conversation keeps its title, folder, branch, model and start time.

**What every reader drops.** The agent's reasoning: Claude's thinking and MiniMax Code's are
signed for the account that wrote them, Codex's is encrypted for it, and the others' is the
agent's own; none is ever rewritten, carried or quoted. Images and files are marked as `[image]`
or `[file]` where they were. Messages the agent added itself, such as Claude Code's
`<system-reminder>` blocks, Codex's `<environment_context>`, DSH's runtime context, qwen's
injected context, the date copilot prepends and opencode's synthetic messages, are left out, as are subagents' transcripts and sidechains, and the agent's own
UI or state records. Each `show` and conversion says what it left out.

**Compaction.** Claude Code's whole history before a compaction is carried, not its summary;
MiniMax Code's compaction summaries are carried as text; DSH's compact checkpoints stand in for
the events they replaced, as DSH shows them.

**Fitting.** The turns are then fitted to the new agent: a note first, saying where the session
came from, when and in which folder, with what model, what isn't included, and to check whether
its last action completed before repeating it; past about
400,000 characters (about 100,000 tokens) the oldest turns after the first are left out, with a
line saying how many; the newest turn is always kept, cut to its latest parts when it alone is
too long; turns are merged so you and the assistant alternate; and the conversation ends on a
reply, with `(The X session ended here.)` added when it ended on yours.

**The brief.** `--brief` hands over a brief in place of the whole conversation: after the note, the
first prompt, the files and commands the tool calls named, and the last six turns as they were,
within 12,000 characters. hatcheck runs no model to make it, so it's the same every time, as
Orca's continuation after a restart is: Orca resumes the provider's own session and sends one
fixed message, where a brief carries what an agent that can't resume the session needs.
`hatcheck show --brief` shows it.

**What a writer makes.**

| Into | Written as | Resumed with |
|---|---|---|
| Claude Code | a transcript in the profile's `projects/<folder>/`, a new UUID, the `uuid` chain, and the version of a transcript already in the profile | `cl NAME --resume <id>` |
| Codex | a rollout file under `sessions/`, with its line in `session_index.jsonl` | `cx NAME resume <id>` |
| Kimi Code CLI | a session folder with a 1.0 wire and `context.*` records, written as kimi's own `kimi migrate` importer writes them, with its line in `session_index.jsonl` | `ck NAME --session session_<id>`, after the terminal UI has opened it once |
| Antigravity CLI | `imports/<id>.md` in hatcheck's cache folder: the conversation as Markdown, each turn under who said it and when | `agy --add-dir <imports> -i '…'`, which reads the file and carries on |
| ZCode | the same Markdown file | `zcode --attach <file> --mode plan -p '…'`, then `zcode -c` |
| Qwen Code | the same Markdown file | `qwen --add-dir <imports> -i '…'` |
| MiniMax Code | the same Markdown file | `mcode '…'`: its terminal UI, with the prompt as its task |
| DeepSeek Harness | the same Markdown file | `dsh web`, then the prompt pasted into its web UI |
| GitHub Copilot CLI | `imports/<id>.jsonl`: a version-1 session interchange file, a `session` header (title, folder, branch) and a `message` record per turn, the form its own `copilot sessions import` validates | `copilot sessions import <file>`, which prints the new session's ID, then `copilot --resume <id>` |
| OpenCode | `imports/<id>.json`: the `{info, messages}` session data its own `session export` prints and `session import` takes, with an ID minted as opencode mints them | `opencode session import <file>` from the session's folder, then `opencode --session ses_<id>` |

Claude Code, Codex and Kimi get a session in their own format, which they resume as one of
theirs. Antigravity CLI, ZCode, Qwen Code and MiniMax Code have no importer, so they get a file
and a command that starts them on it, and DeepSeek Harness gets the file and a prompt to paste
into its web UI, which takes no first prompt: the session exists once the agent has read the
file, with an ID of the agent's own, which is why `--json` reports `"new_session": null` and the
`file` for those five, and the prompt as `paste` for DSH. GitHub Copilot CLI and OpenCode have
importers of their own, so they get the file their importer takes and the command that runs it;
hatcheck writes neither store, and the session exists once the import has run, so `--json`
reports `"new_session": null` and the `file` for those two as well.

**What isn't a conversion.** A move or copy between an agent's own profiles carries the session
whole: Claude Code's transcript, subagent folder, `/rewind` snapshots and plan, with the paths in
it repointed except inside thinking; Codex's rollout between homes on the same account; Kimi's
session folder with its index line. ZCode's database, DSH's logs, MiniMax Code's catalog, Qwen
Code's transcripts and copilot's session folders are never copied between homes, and OpenCode has
one database.

**Checked how.** Each conversion was resumed with the real agent, and the version is written down
beside it: see [`hatcheck move`](commands.md#hatcheck-move-carry-on-a-session-with-another-profile)
and its sections for each agent, and [maintenance.md](maintenance.md) for the probes that check a
new version.
