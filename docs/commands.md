# The commands

What each `hatcheck` command does, as it is, with its options; the agents and their launchers
(`cl`, `cx`, Antigravity CLI, Kimi Code CLI, ZCode, `claude-z` and `claude-k`) are in
[agents.md](agents.md), and
installing, switching and `hatcheck doctor` in [install.md](install.md). The
[README](../README.md) has the short tour.

## `hatcheck where`: find a session

```console
$ hatcheck where 3f1c2a9e
/Users/me/.claude-work/projects/-Users-me-code-api/3f1c2a9e-8b7d-4c1e-9f2a-6d5e4c3b2a10.jsonl
$ hatcheck where 019a8c1e
/Users/me/.codex/sessions/2026/10/14/rollout-2026-10-14T09-12-33-019a8c1e-7d3f-7b21-9c4e-2f1a0b9c8d7e.jsonl
```

The profile in the path tells you how to resume it (`claude`, or `cl work` here), and the folder
under `projects/` is the directory the session ran in, with every non-alphanumeric character turned
into `-`. A session with subagents or large tool results also has a data folder of the same name,
which is listed too. Codex files its sessions by the date they started, archived ones under
`archived_sessions/`; resume one with `codex resume <id>`, or `cx <name> resume <id>` for a
`~/.codex-<name>` home. An Antigravity CLI conversation is
`~/.gemini/antigravity-cli/conversations/<id>.db`, listed with the `brain/<id>` folder agy writes
beside it; resume it with `agy --conversation <id>`. A Kimi Code CLI session is its folder,
`~/.kimi-code/sessions/wd_<folder>_<hash>/session_<id>`, resumed with `kimi --session session_<id>`,
or `ck <name> --session session_<id>` for a `~/.kimi-code-<name>` home; a ZCode session is a row in
`~/.zcode/cli/db/db.sqlite`, listed as that file and, when there is one, its folder of tool results
under `cli/artifacts/`, resumed with `zcode --resume sess_<id>`. For both, the ID is the UUID after
the agent's own prefix, which can be pasted along with it.

## `hatcheck`: what to resume

`hat` is the short name: `hat`, `hat usage`, `hat move …` and so on work the same. `hat check` is the name
in two words, so it's `hat` again: `hat check` lists sessions and `hat check usage` is `hat usage`.

A subcommand can be cut to its first letters, after `hat` or `hatcheck`: `hat m …` is `hat move …`
and `hat f …` is `hat fork …`. Where two share a letter, it goes to the one used more: `b` is
`best`, `s` is `show`, `u` is `usage` and `d` is `desktop`, so `bind`, `unbind` and `doctor` take
two letters, `bi`, `un` and `do`. What it prints, and the commands it suggests, use the whole name.

Lists the Claude Code sessions from every `~/.claude*` profile, and the Codex sessions from every
`~/.codex*` home, that were active in the last 24 hours or are still open, grouped by folder and
newest first. Run from a folder that has sessions, it lists
just that folder's (`--all-folders` lists them all). Each one comes with the command that resumes it
and what's left of that profile's plan:

```
$ hatcheck
Claude Code and Codex CLI sessions active in the last 24h or open now (by folder, newest first; % = plan left, ↻ = hours/days until reset)
cd ~/code/api
    4m  Fix flaky login test · main · ● open in pid 48213 (busy)
        cl work --resume 3f1c2a9e-8b7d-4c1e-9f2a-6d5e4c3b2a10      # 5h 100% █████               Fable 48% ███▍░░░ ↻████▎░░ 4d6h
   40m  Add retries to the uploader · main
        cx work resume 019a8c1e-7d3f-7b21-9c4e-2f1a0b9c8d7e        # 5h  80% ████░ ↻███▏░ 3h10m  week  64% ████▌░░ ↻█▊░░░░░ 1d18h
    1h  Refactor the auth middleware · main · on k3
        cl -k work --resume 2d4f6a8c-1e3b-4d5f-9a7c-0b2d4e6f8a1c
    3h  Add rate limiting to the login endpoint · feat/rate-limit
        claude --resume 9a8b7c6d-5e4f-4a3b-8c2d-1e0f9a8b7c6d       # 5h  62% ███▏░ ↻██▋░░ 2h41m  week  91% ██████▍ ↻███▏░░░ 3d2h
cd ~/code/blog
   20h  Draft post on Rust lifetimes · main
        cl personal --resume c47e2b1d-0a9f-4e8d-b7c6-5d4e3f2a1b0c  # 5h    ? ·····               week  97% ██████▊ ↻█████▏░ 5d2h
cd ~/notes
    6h  Plan next sprint · main
        cl work --resume 5b6c7d8e-9f0a-4b1c-8d2e-3f4a5b6c7d8e      # 5h 100% █████               Fable 48% ███▍░░░ ↻████▎░░ 4d6h
    9h  Write release notes · main
        cl work --resume 7e8f9a0b-1c2d-4e3f-8a4b-5c6d7e8f9a0b
Profiles: ~/.claude (1), ~/.claude-personal (1), ~/.claude-work (4)
Usage from Claude Code's last snapshot (Claude Code's /usage couldn't fetch new numbers; offline, or rate limited?): ~/.claude-personal 9h ago
```

- Run the folder's `cd` line, then the session's command. The command's program, `cl`, `claude`,
  `cx`, `codex`, `agy`, `kimi` or `zcode`, is in cyan: that word says which agent the session
  belongs to. The branch
  after the title is the one the session ran on, as the agent wrote it down. Sessions still open in
  a terminal are marked `● open`, so you don't resume one twice: Claude Code's say the process, from
  the registry Claude Code keeps; Codex's and agy's, which hold a lock on a session they have open,
  say the agent. An open session is listed even when its last message is older than the window.
- The comment after each command shows each plan limit: the 5-hour session (`5h`), the weekly limit
  (`week`) and per-model weekly limits such as `Fable`, which are named in violet, with their bar
  drawn in it, so they stand apart from `week`. Each one has the % left and a bar of it, then `↻`, a bar of the time left until
  it resets and that time. Bars have a block per hour of the 5-hour window and per day of a weekly
  one, as in `↻████▎░░ 4d6h`.
- Time bars are coloured by urgency: blue with most of the window to go, then gold and orange, and
  red as the reset nears.
- A plan bar shorter than its time bar means that limit is being used up faster than its window runs
  out, like `Fable` above: 48% left with more than half the week to go. `?` means the limit has
  reset since the saved snapshot it came from.
- A command without the comment, like the second one in `~/notes`, uses the same account as the
  command above it, even through another profile, so its usage is the same.
- A session whose newest reply came from Z.ai or Kimi shows that model, like `on k3` above, and its
  command resumes it there (`claude-k` for `~/.claude`). Use `cl work` instead to go back to Claude;
  its comment is still the profile's Claude plan.
- Codex sessions resume with `codex resume <id>`, or `cx <name> resume <id>` for a `~/.codex-<name>`
  home. Their comment shows the ChatGPT plan's Codex limits (see `hatcheck usage` below). Sessions
  from the Codex app, its IDE extension and `codex exec` are hidden like Desktop ones, and Codex's
  own approval reviews aren't listed.
- Antigravity CLI sessions resume with `agy --conversation <id>`, and have no branch after the
  title, since agy writes none down. Their comment shows what agy's `/usage` reports. The
  Antigravity app's conversations, read from the app's own folder, are hidden like Desktop ones,
  and conversations agy ran itself aren't listed.
- Kimi Code CLI sessions, from every `~/.kimi-code*` home, resume with `kimi --session session_<id>`,
  or `ck <name> --session session_<id>` for a `~/.kimi-code-<name>` home; ZCode sessions, from
  `~/.zcode`'s database, with `zcode --resume sess_<id>`, and sessions zcode ran itself aren't
  listed. Neither has a branch, and nothing marks their sessions open. A Kimi session's comment
  shows the home's Kimi Code limits (see `hatcheck usage` below); a ZCode session has none, since
  zcode has no usage command.
- Since the usage is a shell comment, pasting the whole line runs just the command (zsh needs
  `setopt interactivecomments`).
- Usage is fetched live: Claude Code's `/usage` is run once per account, in its profile, and the
  snapshot it saves is read (see [`hatcheck usage`](#hatcheck-usage-plan-usage-for-every-profile)
  below), unless that snapshot is under a minute old, which Claude Code's `/usage` shows again
  without asking; ChatGPT's usage endpoint is asked once per Codex account. When that gives
  nothing new (offline, not signed in, rate limited), the last saved snapshot is used instead
  and named at the bottom with its age.
- A session that ran in a folder [bound](#hatcheck-bind-tie-a-folder-to-a-profile) to another
  profile is marked `⚠ folder bound to ~/.claude-work`, so it's easy to spot and `hatcheck move`.
- What it reads from each session file is cached, in `sessions.json` in `~/Library/Caches/hatcheck`
  on macOS (`%LOCALAPPDATA%\hatcheck\Cache` on Windows, `~/.cache/hatcheck` elsewhere, or
  `$HATCHECK_CACHE_DIR`). A repeat run reads only what was added to a session since, so a session
  that's still being written costs little however long it is. Deleting the folder is always safe.

| Option       | What it does                                                                  |
|--------------|-------------------------------------------------------------------------------|
| `HOURS`      | how far back to look (default 24); takes `36`, `1.5`, `12h` or `3d`           |
| `-n N`       | show only the newest N sessions per folder and count the rest                 |
| `--all`      | also list sessions from Claude Desktop's Code tab, the SDK, the Codex app, its IDE extension, `codex exec` and the Antigravity app |
| `--all-folders` | list every folder, even when run from one that has sessions                |
| `--full`     | print `CLAUDE_CONFIG_DIR=~/.claude-<name> claude …` instead of `cl`, `CODEX_HOME=… codex …` instead of `cx`, and `KIMI_CODE_HOME=… kimi …` instead of `ck` |
| `--cached`   | take usage from Claude Code's and Codex's saved snapshots and hatcheck's last readings of agy's `/usage` and Kimi's usage, without calling the APIs |
| `--no-usage` | leave plan usage out                                                          |
| `--json`     | print the sessions, and the accounts behind them, as JSON ([below](#--json-for-scripts-status-bars-and-apps)) |
| `-e`, `--email` | with `--json`, include each account's email address                        |

## `hatcheck usage`: plan usage for every profile

```
$ hatcheck usage
Plan usage by profile (% left, then time left until reset)
Sorted by Fable: those with some left first, the soonest reset on top; then those out of it, the soonest back on top.
~/.claude-work  team max 5x · live
  session (5h)      100% ██████████
  week (Fable)       48% ████▊░░░░░  ██████▏░░░ resets Sun 8:20pm (in 4d6h)
~/.claude  pro · live
  session (5h)       62% ██████▎░░░  █████▍░░░░ resets 5:01pm (in 2h41m)
  week (all models)  91% █████████▏  ████▍░░░░░ resets Sat 4:20pm (in 3d2h)
~/.claude-personal  pro · cached 9h00m ago (Claude Code's /usage couldn't fetch new numbers; offline, or rate limited?)
  session (5h)         ? ··········             reset 10:20am, after this snapshot
  week (all models)  97% █████████▊  ███████▎░░ resets Mon 4:20pm (in 5d2h)
~/.codex-work  ChatGPT plus · live
  session (5h)       80% ████████░░  ██████▍░░░ resets 5:30pm (in 3h10m)
  week (all models)  64% ██████▍░░░  ██▌░░░░░░░ resets Fri 8:20am (in 1d18h)
```

- Accounts are sorted by their Fable limit. First come those with some Fable left, the soonest
  Fable reset on top: what's left there goes unused once it resets, so that's the one to use first.
  An account past 90% of its Fable limit goes behind the fresher ones, and past 98% behind those
  too: nearly out, it's the one to use last. Next come those out of Fable, or out of the weekly
  limit for all models, which Fable counts against too, the soonest back on top. Then come accounts
  without a Fable limit, such as Pro plans, and Codex homes. A snapshot saved before its Fable limit
  reset counts as full until the reset after.
- Each limit has two bars: what's left of it, then how much of its window, 5 hours or a week, is
  left before it resets. In a terminal the second goes from blue with most of the window to go,
  through gold and orange, to red as the reset nears. When the first bar is shorter than the
  second, as for `Fable` here, the limit is being used up faster than its window runs out. A limit
  without a reset time, like an unused 5-hour session, has no time bar.
- A Claude Code account's numbers come from Claude Code itself: `claude -p /usage` is run in the
  profile, with no tools, MCP servers, settings or transcript, which asks Anthropic with the
  profile's own login and saves the answer in the profile's `.claude.json`; that snapshot is what's
  shown. hatcheck never reads a login: the plan's name comes from the account details Claude Code
  keeps beside the snapshot. The runs take about a second each, in parallel, from a folder of
  hatcheck's own (`claude-usage` in its cache folder), so the one empty project folder Claude Code
  makes for the folder it runs in is the same every time.
- A snapshot under a minute old is shown as it is, as `cached <1m ago (Claude Code's /usage
  reuses a reading under a minute old)`, and `claude` isn't run for it: Claude Code's own
  `/usage` shows such a reading again without asking Anthropic (`nt=60000` beside
  `cachedUsageUtilization` in 2.1.307; five runs a minute after the last left their snapshots
  where they were, on 2026-10-21), so a run would give the same numbers a second later.
- When that gives nothing new (`claude` not on the `PATH`, the profile not signed in, offline, or
  Anthropic rate-limiting the account), Claude Code's last saved snapshot is shown with its age and
  the reason.
- Profiles signed in to the same account are shown together. Per-model limits such as
  `week (Fable)` are named in violet, with their bar drawn in it, to tell them from `week (all
  models)`; the % keeps the severity's colour. Quota fields the server
  sends that Claude Code doesn't display are listed dimmed under their raw names.
- Kimi homes show their Kimi Code plan's limits: the 5-hour and 7-day windows and the month's,
  with `month (code)` named in violet as a model group's. They come from the endpoint kimi's own
  `/usage` asks, with the access token in the home's `credentials/kimi-code.json` sent as it is
  and never refreshed. That token lasts 15 minutes and kimi renews it as it runs, so a live reading
  needs kimi to have run in the last quarter of an hour; otherwise, as when the request fails,
  hatcheck's last reading is shown with its age, since kimi saves none. Kimi sends no severity, so
  a limit turns yellow from 75% used and red from 90%. Checked against kimi 2.2.0 and the answer
  its endpoint gave on 2026-10-20.
- ZCode's folder is listed with no data and why: zcode has no usage command and asks no usage
  endpoint (ZCode 0.16.9); Z.ai shows the plan's usage at z.ai/manage-apikey/subscription.
- Codex homes show their ChatGPT plan's Codex limits: the 5-hour and weekly windows (a free plan has
  one 30-day window instead), then limits for single models such as `GPT-5.3-Codex-Spark`, named in
  violet like `Fable`. The access token in the home's `auth.json` is sent as it is to the endpoint
  Codex's `/status` uses, and never refreshed. When it has expired, or the request fails, the newest
  limits Codex recorded in that home's session files are shown, with their age. Codex doesn't send a
  severity, so a limit turns yellow from 75% used and red from 90%. Homes signed in to the same
  account are shown together.
- Antigravity CLI's folder shows the limits agy's own `/usage` reports, one row per model group
  and window (agy 1.4.3 names a 5-hour and a weekly one per group), named in violet like
  per-model limits, under `Antigravity` and the plan tier agy names. When agy isn't signed in,
  the row says so, with the last reading and its age, and nothing is run that would start agy's
  sign-in. agy is the slowest to ask, two requests to Google taking five seconds or more, so a
  reading from the last five minutes is shown again instead, as `cached <1m ago (asked at most
  once in 5m)`; `--cached` shows it at any age, and once it's five minutes old agy is asked
  again. One hatcheck asks agy at a time: a second run while another is asking, as when the
  desktop app's engine and the CLI coincide, waits for it and takes its reading the same way. See
  [Antigravity CLI](agents.md#antigravity-cli-agy) for how it's run and what's kept.
- `--cached` skips the APIs. `-e` / `--email` adds each account's email address, which is hidden by
  default so the output is safe to share. `--json` prints the accounts as data
  ([below](#--json-for-scripts-status-bars-and-apps)).

## `hatcheck move`: carry on a session with another profile

When one account runs low, continue a session on another profile, or in another agent. It moves
the session: the original is deleted once every copied file is checked against it, and an older
copy never replaces a newer one. `--keep` keeps the original, which copies it instead.

```
$ hatcheck move 9a8b7c6d work
Moved session 9a8b7c6d "Add rate limiting to the login endpoint" from ~/.claude to ~/.claude-work:
  transcript                 53.0 KB
  subagents and tool output  3 files
  /rewind file snapshots     4 files
  plan                       quiet-river-plan.md
Paths in the conversation to these files now point to the copies.
Deleted the original from ~/.claude; the plan file stays, since sessions can share one.
Resume it with:
  cd ~/code/api
  cl work --resume 9a8b7c6d-5e4f-4a3b-8c2d-1e0f9a8b7c6d
Resume it now in ~/.claude-work? [y/N]
```

- It copies the transcript, the folder of subagent transcripts and saved tool output, the `/rewind`
  file snapshots and the plan file, keeping their timestamps, and checks every copied file before it
  deletes anything. Paths in the conversation that point at those files are changed to point at the
  copies, except inside Claude's thinking, whose signatures would stop matching.
- A session that's open in a terminal is refused: quit it first, so it stops changing.
  `hatcheck` marks those `● open`.
- Resume it with the printed command, from the directory it ran in, or answer `y` when it asks and
  it starts Claude Code there with the new profile. It only asks in a terminal, so scripts never
  wait on it. The conversation comes across whole; the account and its plan limits, settings,
  permissions, MCP servers, plugins and project memory are the new profile's. If the new account
  rejects the signatures on earlier thinking, Claude Code drops those thinking blocks and carries
  on.
- The plan file stays in the old profile, since sessions can share one.
- If the session's last reply came from Z.ai or Kimi, it also prints the `cl -z` or `cl -k` command
  that carries on there instead of on the new profile's Claude plan.

### When the profile already has a copy

A session copied to another profile with `--keep` and carried on there can be moved back: the copy
it left behind is an older version of it. The two copies are compared line by line:

- **The copy there is older**, because the session went on from it: it's brought up to date, and
  each item shows `(updated an older copy)`. Nothing is lost, since the copy being moved starts with
  all of it.
- **The copy there is newer**, because the session went on there: it stops and changes nothing,
  since putting this copy there would lose what came after. It prints the command that resumes the
  session there, and the files of the older copy here, to delete if you don't want them.
- **Both went on** after the copy was made: nothing is moved, since that would lose the prompts
  only the copy there has. It says when they split and how many prompts each has had since.
  [`hatcheck fork`](#hatcheck-fork-a-session-as-a-new-one-in-another-profile) brings this one in
  as a new session with its own ID and keeps both. Or delete the copy there first.

```
$ hatcheck move 9a8b7c6d work
~/.claude-work already has session 9a8b7c6d "Add rate limiting to the login endpoint", with everything in this copy and more: the session went on there. This copy is older, so nothing was moved.
Resume it there with:
  cd ~/code/api
  cl work --resume 9a8b7c6d-5e4f-4a3b-8c2d-1e0f9a8b7c6d
To drop the older copy in ~/.claude, delete:
  ~/.claude/projects/-Users-me-code-api/9a8b7c6d-5e4f-4a3b-8c2d-1e0f9a8b7c6d.jsonl
  ~/.claude/projects/-Users-me-code-api/9a8b7c6d-5e4f-4a3b-8c2d-1e0f9a8b7c6d
  ~/.claude/file-history/9a8b7c6d-5e4f-4a3b-8c2d-1e0f9a8b7c6d
```

```
$ hatcheck move 9a8b7c6d default
Session 9a8b7c6d went on in both ~/.claude-work and ~/.claude after Oct 15 01:40: 3 new prompts in ~/.claude-work, 1 in ~/.claude. Moving this one over would lose what's only in ~/.claude, so nothing was moved.
To keep both, bring this one in as a new session with its own ID:
  hatcheck fork 9a8b7c6d default
Or delete the copy in ~/.claude first, to replace it:
  ~/.claude/projects/-Users-me-code-api/9a8b7c6d-5e4f-4a3b-8c2d-1e0f9a8b7c6d.jsonl
  ~/.claude/projects/-Users-me-code-api/9a8b7c6d-5e4f-4a3b-8c2d-1e0f9a8b7c6d
  ~/.claude/file-history/9a8b7c6d-5e4f-4a3b-8c2d-1e0f9a8b7c6d
```

A round trip and a copy that went on in both profiles were tested with the real `claude`
(2.1.301).

When more than one profile has the session, you're asked which copy to take. The one used last
comes first and is the default, and each other one says how it stands to it:

```
$ hatcheck move 4215508b hp-alan
Session 4215508b is in 3 profiles:
  1. ~/.claude-hp-aria    Oct 17 14:02   312 prompts  used last
  2. ~/.claude-hp-yi      Oct 16 09:11   298 prompts  14 prompts behind 1
  3. ~/.claude-hp-aiqing  Oct 12 18:40   201 prompts  went its own way after Oct 11 20:15: 2 prompts of its own
Which one? [1]
```

A number, a profile's name or Enter picks one. When the output isn't a terminal, as in a script or
with `--json`, the copy used last is taken and a line says so; `--from` always decides.

### With Codex

`codex` is `~/.codex` and `codex:<name>` is `~/.codex-<name>`, as a place to copy to or `--from`. A
bare name follows the session: for a Codex session, `work` is `~/.codex-work` when that home exists
and `~/.claude-work` otherwise, and when both exist you're asked which, or say `codex:work` or
`claude:work` (a Kimi session follows the same rule with `~/.kimi-code-work` and `kimi:work`):

```
$ hatcheck move 019a8c1e claude:work
Converted Codex session 019a8c1e "Add retries to the uploader" from ~/.codex-work into a Claude Code session in ~/.claude-work:
  conversation               5 of your messages, with the replies
  tool calls                 191, as text with their output shortened
  left out                   Codex's reasoning, images, goals and subagent sessions
  new session                6f6749d7-0f59-4683-997c-a3595421b93f (241.9 KB)
The original stays in ~/.codex-work, since a conversion leaves things out.
Resume it with:
  cd ~/code/api
  cl work --resume 6f6749d7-0f59-4683-997c-a3595421b93f
Resume it now in ~/.claude-work? [y/N]
```

- Between Codex homes signed in to the same ChatGPT account, the session file is copied as it is,
  with its ID and its name, and the original is deleted once the copy is checked, as for Claude
  Code. A copy already there is compared the same way.
- Between Claude Code and Codex (`hatcheck move 3f1c2a9e codex:work` goes the other way), and
  between Codex homes on different accounts, the session is converted into a new one with a new ID,
  and the original is always kept. Codex encrypts its reasoning and its summaries of compacted
  context for the account that wrote them, so a copy can't take them to another account.
- Your messages and the replies come across as text, and each tool call as a block with its input
  and the start of its output, in the format Codex's own `/import` uses for Claude Code sessions
  (`[external_agent_tool_call: Bash] … [/external_agent_tool_call]`). A note at the start tells the
  model where the session came from. Claude's thinking and Codex's reasoning are left out, and so are
  images, plans, goals, `/rewind` snapshots and subagent transcripts.
- The whole conversation before a compaction is carried over, not its summary. Past about 100,000
  tokens the oldest turns after the first are left out, so it fits the new agent's context window.
- Converted sessions were tested by resuming them with the real `claude` (2.1.301) and `codex`
  (0.155.1), which sent the whole converted history to the model and carried on in the same
  session. Codex's resume picker lists them by their Claude Code title.
- Codex can import Claude Code sessions itself (`/import` in Codex), but only from `~/.claude` and
  the last 30 days. Claude Code's `claude import codex` brings over Codex's instructions, agents and
  hooks, not sessions.

### With Antigravity CLI

`agy` is `~/.gemini/antigravity-cli`, as a place to convert to or `--from`:

```
$ hatcheck move c134893c work
Converted Antigravity CLI session c134893c "Fix the uploader" from ~/.gemini/antigravity-cli into a Claude Code session in ~/.claude-work:
  conversation               5 of your messages, with the replies
  tool calls                 88, as text with their output shortened
  left out                   Antigravity's thinking, images, artifacts and subagent conversations
  new session                6f6749d7-0f59-4683-997c-a3595421b93f (241.9 KB)
The original stays in ~/.gemini/antigravity-cli, since a conversion leaves things out.
Resume it with:
  cd ~/code/api
  cl work --resume 6f6749d7-0f59-4683-997c-a3595421b93f
```

- From agy, the transcript agy writes beside each conversation is converted as a Codex session is:
  your messages and the replies as text, each tool call as a block with its input and its output.
  agy's thinking, images, artifacts and its subagents' conversations are left out.
- To agy, nothing hatcheck could write is a conversation agy would resume, so the session is written
  as a Markdown file, `imports/<id>.md` in hatcheck's cache folder, and the command printed starts
  agy on it from the session's folder: `agy --add-dir <the imports folder> -i '…'`, with the folder
  added to the workspace so agy may read the file, and a first prompt that has it read the
  conversation and carry on. The session exists once agy starts, so `--json` reports
  `"new_session": null` and the `file`. The original stays, as after every conversion.
- Checked against the real `agy` (1.4.2): `hatcheck show` reads its transcripts, and a Claude Code
  session converted this way was read from the imports folder and summarised correctly by agy in
  print mode.

### With Kimi Code CLI

`kimi` is `~/.kimi-code` and `kimi:<name>` is `~/.kimi-code-<name>`, as a place to move to or
`--from`, and a bare name follows a Kimi session the way it follows a Codex one. Between Kimi homes
the session is moved: its folder is copied with its timestamps, the copy's `state.json` is made to
point into the new home (and given the new ID for a fork, which also names the session it came
from), the new home's index gets its line, every copied byte is checked, and then the original
folder and its index line go. A copy already there is compared by the wires and file snapshots in
its `agents/` folder, as a Codex copy is by its file; `state.json` changes with every use, so it
isn't compared. Nothing in a Kimi session is signed or encrypted, so the copy runs on any account.

```
$ hatcheck move 0752ad88 kimi:work
Moved Kimi session 0752ad88 "Fix the uploader" from ~/.kimi-code to ~/.kimi-code-work:
  session folder             19 files
Its state.json points into ~/.kimi-code-work, whose index lists it.
Deleted the original from ~/.kimi-code, and its line in the index there.
Resume it with:
  cd ~/code/api
  ck work --session session_0752ad88-cc62-4e00-9912-48e4146b7628
```

- From Kimi, the wire is converted as a Codex session is: your messages and the replies as text,
  each tool call as a block with its arguments and the tool's output. Kimi's thinking, images and
  subagent conversations are left out.
- To Kimi (`hatcheck move 3f1c2a9e kimi`), the session is written as kimi's own importer (`kimi
  migrate`, for kimi-cli sessions) writes one: a folder under the working directory's key, with
  `state.json` and a wire holding, per turn, the prompt, the messages kimi rebuilds the model's view
  from and the turn's end, and its line in the index. kimi binds its model and profile when its
  terminal UI opens the session, so resume it with the printed `kimi --session …`; asked in print
  mode (`-p`) before the UI has opened the session once, kimi 2.2.0 says no model is set.
- Checked against the real `kimi` (2.2.0): a Claude Code session converted this way was opened in
  kimi's terminal UI, which showed the conversation and answered a question about it from the
  history. kimi validates each event of a session it resumes against a schema, which these pass.
- Nothing marks a Kimi session open, so `move` can't refuse one that is: quit kimi first.

### With ZCode

`zcode` is `~/.zcode`, as a place to convert to or `--from`:

```
$ hatcheck move 1fb068a4 work
Converted ZCode session 1fb068a4 "Fix the uploader" from ~/.zcode into a Claude Code session in ~/.claude-work:
  conversation               5 of your messages, with the replies
  tool calls                 42, as text with their output shortened
  left out                   ZCode's reasoning, images, checkpoints and subagent sessions
  new session                6f6749d7-0f59-4683-997c-a3595421b93f (98.1 KB)
The original stays in ~/.zcode, since a conversion leaves things out.
Resume it with:
  cd ~/code/api
  cl work --resume 6f6749d7-0f59-4683-997c-a3595421b93f
```

- From ZCode, the session's rows are converted as a Codex session is: your messages and the replies
  as text, each tool call as a block with its input and output. ZCode's reasoning, images,
  checkpoints and subagent sessions are left out.
- To ZCode, hatcheck never writes ZCode's database ([the proposal](proposals/kimi-zcode.md#writing-into-zcodes-database-the-risk)
  says why), so the session is written as a Markdown file, `imports/<id>.md` in hatcheck's cache
  folder, and the command printed runs zcode once in print mode from the session's folder, with the
  file attached and in plan mode so nothing is changed: `zcode --attach <file> --mode plan -p '…'`,
  a first prompt that has it read the conversation and say where it left off. That persists the
  session in zcode's database, and `zcode -c` in the folder then carries it on in the terminal UI.
  `--json` reports `"new_session": null` and the `file`. The original stays, as after every
  conversion.
- Checked against the real ZCode (0.16.9, the CLI from the desktop app's bundle, in a throwaway
  home): zcode read the attached file, summarised the conversation correctly, and `zcode --resume`
  on the session it had made answered a question from it; hatcheck then listed and showed that
  session from the database zcode wrote.

| Option           | What it does                                                                 |
|------------------|------------------------------------------------------------------------------|
| `SESSION`        | the session ID, or the start of it; left out, the newest session started from this folder |
| `PROFILE`        | where it goes: `NAME` for `~/.claude-NAME`, or for a Codex or Kimi session `~/.codex-NAME` or `~/.kimi-code-NAME` when that home exists (`claude:NAME`, `codex:NAME` and `kimi:NAME` spell it out; with both, you're asked), `default` or `claude` for `~/.claude`, `codex` for `~/.codex`, `agy` for `~/.gemini/antigravity-cli`, `kimi` for `~/.kimi-code`, `zcode` for `~/.zcode`, or a path; left out, this folder's bound Claude Code profile, or `~/.claude` |
| `--from PROFILE` | the profile or home to take it from, when more than one has it; without it you're asked, with the one used last as the default, and a script or `--json` gets that one |
| `--keep`         | keep the original, which copies the session rather than moving it (a converted session's original is always kept) |
| `--dry-run`      | show what would be moved, copied, converted and deleted, and change nothing  |

With nothing, `hatcheck move` (or `hat m`) moves the newest session started from this folder,
whichever agent's, to the folder's bound Claude Code profile, or `~/.claude`, after asking in the
terminal:

```
$ hat m
Move session 3f1c2a9e "Fix flaky login test" from ~/.gemini/antigravity-cli to ~/.claude-work? [y/N] y
```

`--dry-run` says what it would do without asking, and a script, a pipe or `--json` has to say
both the session and the profile. `--no-keep`, which a move needed before it was the default, is
still accepted, and so is `--fork`, which does what
[`hatcheck fork`](#hatcheck-fork-a-session-as-a-new-one-in-another-profile) does.

## `hatcheck fork`: a session as a new one in another profile

Bring a session into another profile as a new one with its own ID, and keep the original, as
Claude Code's `claude --resume <id> --fork-session` and `codex fork` do within one profile. It's
how to keep both when a move stops because the session went on in both profiles.

```
$ hatcheck fork 9a8b7c6d default
Forked session 9a8b7c6d "Add rate limiting to the login endpoint" from ~/.claude-work to ~/.claude as a new session:
  transcript                 61.2 KB
  subagents and tool output  3 files
  /rewind file snapshots     5 files
  plan                       quiet-river-plan.md  (already there)
  new session                5b1e0c4a-7d2f-4e6b-9a3c-8f1d2e3c4b5a
Paths in the conversation to these files now point to the copies.
The copy of session 9a8b7c6d that was already in ~/.claude stays as it was.
The original stays in ~/.claude-work.
Resume it with:
  cd ~/code/api
  claude --resume 5b1e0c4a-7d2f-4e6b-9a3c-8f1d2e3c4b5a
Resume it now in ~/.claude? [y/N]
```

- It copies what `move` copies and checks it the same way, and the copy gets the new ID in its
  records, and its own folders for subagents, saved tool output and `/rewind` snapshots. Its
  message IDs stay the same, as in Claude Code's own fork. A Codex session gets the new ID in its
  file name, its `session_meta` and its events; a Kimi session in its folder's name and its
  `state.json`, which also names the session it was forked from.
- A copy already in the profile stays as it is, so a fork never stops for one, and the original
  always stays where it was. A plan file already there stays too, since the copy there uses it.
- To another agent, or to a Codex home on another account, it converts the session, as `move`
  does: a converted session is always a new one, and for agy and zcode a file the agent starts one
  on.
- Resuming a fork was tested with the real `claude` (2.1.301).

| Option           | What it does                                                                 |
|------------------|------------------------------------------------------------------------------|
| `SESSION`        | the session ID, or the start of it; left out, the newest session started from this folder, as for `move` |
| `PROFILE`        | where it goes, as for [`move`](#hatcheck-move-carry-on-a-session-with-another-profile); left out, this folder's Claude Code profile, after asking |
| `--from PROFILE` | the profile or home to take it from, when more than one has it, as for `move` |
| `--dry-run`      | show what would be copied or converted, and change nothing                   |

## `hatcheck relocate`: a project folder that moved

Claude Code files a session under `projects/<the folder's name>` and records the folder in every
line, so once a project folder is renamed or moved, `claude --resume` from the new folder doesn't
list its sessions, and the commands `hatcheck` prints `cd` to a folder that's gone. `hatcheck
relocate` brings the sessions along, in every profile and home:

```console
$ mv ~/code/api ~/code/api2
$ hatcheck relocate ~/code/api ~/code/api2
Relocated 4 sessions from ~/code/api to ~/code/api2:
  ~/.claude-work  2 sessions: projects/-Users-me-code-api renamed projects/-Users-me-code-api2, with its memory and index
  ~/.claude  1 session: projects/-Users-me-code-api renamed projects/-Users-me-code-api2
  ~/.codex  1 session: the folder in the file rewritten
The folder binding of ~/code/api moved to ~/code/api2.
Resume them from ~/code/api2. To undo: hatcheck relocate ~/code/api2 ~/code/api
```

- Sessions that ran in the folder, or in a folder under it, come along. Each profile's project
  folder is renamed to the new name when every session in it moves, so its `memory/` and
  `sessions-index.json` come with it, the index's paths updated; when some stay, the sessions are
  moved into the new project folder one by one, with their subagent and tool-output folders. In
  each transcript the lines' top-level `cwd` is rewritten to the new folder, and nothing else:
  what was said about the old path stays as it was. A Codex session, filed by date, stays where
  it is, with the folder in its `session_meta` and `turn_context` lines rewritten, so `codex
  resume` lists it from the new folder. Timestamps are kept, and a folder binding
  ([`hatcheck bind`](#hatcheck-bind-tie-a-folder-to-a-profile)) of the old folder moves to the new.
- A project folder whose name matches but whose sessions ran elsewhere (Claude Code names
  `~/code/api-old` and `~/code/api/old` alike) is left alone and listed. So is an Antigravity CLI
  conversation that ran in the folder: agy keeps the folder inside its database, which stays as it
  was.
- It refuses, and changes nothing, when a session is open, was written in the last minute, is a
  link rather than a file, is already filed under the new folder, or has a line naming the folder
  that isn't JSON; and when NEW doesn't exist, is OLD, or is inside OLD. `--dry-run` shows what
  would move. Running it the other way round, `hatcheck relocate NEW OLD`, undoes it, byte for
  byte.
- `--json` prints the sessions moved and skipped, the binding and the undo command.

## `hatcheck open`: resume a session in a new terminal

```console
$ hatcheck open 3f1c2a9e          # a new terminal window in ~/code/api, running cl work --resume …
$ hatcheck open                   # the newest session started from this folder, in any profile or home
$ hatcheck open 3f1c2a9e --here   # in this terminal instead (hatcheck resume asks first)
$ hatcheck open 3f1c2a9e --print  # the two commands, for scripts
cd ~/code/api
cl work --resume 3f1c2a9e-8b7d-4c1e-9f2a-6d5e4c3b2a10
```

- It finds the session by its ID or the start of it, in every Claude Code profile, Codex home and
  Kimi home, and in Antigravity CLI's and ZCode's folders. When more than one profile has a copy,
  it takes the one used last; `--from <profile>` picks one.
- The command is the one `hatcheck` prints: `cl` or `claude`, `cx` or `codex`, `agy --conversation`,
  `ck` or `kimi --session`, `zcode --resume`, and `cl -z`/`cl -k` for a session whose last reply
  came from Z.ai or Kimi. Without `cl`, `cx` or `ck` on the `PATH` it's the full form
  (`CLAUDE_CONFIG_DIR=… claude …`), as `--full` gives.
- A session that's open in a terminal is refused, so it isn't resumed twice.
- The terminal:
  - **macOS:** Terminal, or the one `hatcheck` runs in when that's iTerm2, Ghostty or WezTerm. Terminal
    and iTerm2 are driven with AppleScript, which macOS asks you to allow once.
  - **Windows:** Windows Terminal (`wt`), otherwise a new PowerShell window.
  - **Linux:** `$TERMINAL`, otherwise `x-terminal-emulator`, GNOME Terminal or Konsole.
  - `--terminal NAME`, or `HATCHECK_TERMINAL`, picks another: `terminal`, `iterm`, `ghostty`,
    `wezterm`, `wt`, `powershell`, `gnome-terminal`, `konsole`, `kitty`, or any terminal that takes
    `-e`. A shell stays open in the folder after the session ends.
- If the terminal can't be opened, it prints the two commands to run instead.

## `hatcheck resume`: carry on the newest session here

```console
$ hat r                           # in the folder the session ran in
Session 3f1c2a9e "Fix the login test" · Claude Code in ~/.claude-work · 5m ago
  cl work --resume 3f1c2a9e-8b7d-4c1e-9f2a-6d5e4c3b2a10
Resume it here, in ~/code/api? [Y/n]
$ hatcheck resume 3f1c2a9e --yes  # a session by its ID, without asking
$ hatcheck resume --print         # the two commands, for scripts
cd ~/code/api
cl work --resume 3f1c2a9e-8b7d-4c1e-9f2a-6d5e4c3b2a10
```

- `hat r` is the short form. Without a session, it's the newest session started from this folder,
  in any profile or home, picked as `hatcheck open` picks it (`--all` counts Claude Desktop's, the
  SDK's and the Codex app's too); with one, that session, found as `hatcheck open` finds it,
  `--from` included.
- It shows the session, its agent and profile, how long since it was last active, and the command
  it would run, flags the folder's binding when the session is in another profile (`⚠ folder
  bound to ~/.claude-work`, as the list does), and asks. Enter or `y` runs the command, anything
  else leaves it (`Nothing started.`, exit 1). `--yes` skips the question. Without a terminal to
  ask at, as in a script, it stops and says to give `--yes`, or `--print` for the commands, so a
  script never starts an agent by accident.
- The command runs in this terminal, in the session's folder, in hatcheck's place, as
  `hatcheck open --here` runs it; `hatcheck open` opens a new terminal window instead.
- A session that's open in a terminal is refused, so it isn't resumed twice.

## `hatcheck show`: a session's conversation

```console
$ hatcheck show 3f1c2a9e --last 2
Session 3f1c2a9e "Fix flaky login test" in ~/.claude-work, 14 turns, the last 2:

You  (2026-10-15 22:30)
  Run the login test again

Claude  (2026-10-15 22:31)
  [external_agent_tool_call: Bash]
  command: pytest tests/test_login.py
  [/external_agent_tool_call]
  [external_agent_tool_result]
  3 passed
  [/external_agent_tool_result]
  It passes now.

Left out: Claude's thinking, images, plans, /rewind snapshots and subagent transcripts.
```

- Read-only: your messages and the replies, with each tool call as a block of text with its input
  and the start of its output, as a conversion carries them (see
  [`hatcheck move`](#hatcheck-move-carry-on-a-session-with-another-profile)). A Codex session, an
  Antigravity conversation, a Kimi session or a ZCode session is shown the same way, from its own
  file or rows.
- `--last N` shows only the last N turns, `--from PROFILE` picks the copy to read when several
  profiles have one, and `--json` prints the turns as data.

## `hatcheck best`: which account to use now

```console
$ hatcheck best
Claude Code, best first (sorted by Fable, as hatcheck usage sorts)
   1. ~/.claude-work      Fable 31% left, resets Tue 12:00pm, 0.5%/h · 5h 80%   → cl work
   2. ~/.claude-team      Fable 100% left, resets Thu 1:00am, 0.9%/h · 5h 100%  → cl team
   3. ~/.claude-personal  Fable 0% left, back Sat 4:59pm · 5h 100%              → cl personal
   4. ~/.claude           week 64%, 1.3%/h · 5h 100%                            → claude
Codex, best first
   1. ~/.codex  month 100%, 0.2%/h  → codex
Advice only: nothing is started, moved or switched. hatcheck best --for <session> says where to carry a session.
$ hatcheck best --for 9a8b7c6d
Session 9a8b7c6d "Add rate limiting to the login endpoint" is in ~/.claude-personal: Fable 0% left, back Sat 4:59pm · 5h 100%.
Carry it to ~/.claude-work: Fable 31% left, resets Tue 12:00pm, 0.5%/h · 5h 80%.
  hatcheck move 9a8b7c6d work --from personal
It prints the command that resumes it there, and offers to run it.
$ hatcheck best --wait work && cl work
No room yet; waiting for ~/.claude-work until Tue 12:00pm (in 2d14h)…
```

- Claude Code accounts come in the order `hatcheck usage` sorts them: those with Fable left first,
  the soonest Fable reset on top, with those past 90% used behind the fresher ones and those past
  98% behind those; then those out of Fable, or out of the weekly limit for all models, the soonest
  back on top; then accounts without a Fable limit, by the limit with the least left. Codex homes
  and Antigravity CLI's folder are ranked by the limit with the least left.
- The Fable part ends with the account's pace: what's left, divided by the hours until it resets,
  so what the account can spend per hour without running out before the reset. `0.5%/h` with 31%
  left means about 62 hours to go. An account without a Fable limit shows the pace of its tightest
  limit instead.
- `--for <session>` picks the best other account of the session's agent that has room now, and
  prints the [`hatcheck move`](#hatcheck-move-carry-on-a-session-with-another-profile) command. If
  the session's own account is the best, it says so and prints the resume command.
- `--wait` sleeps until an account has room, then answers as above: any Claude Code account;
  `--wait NAME` (or `default`, `codex`, `codex:NAME`) that one profile or home; with `--for`, an
  account of the session's agent. It sleeps until the soonest reset the numbers name, says so on
  stderr, then looks again; when nothing says when an account will have room, it stops with an
  error instead of polling. For a script: `hatcheck best --wait work && cl work`.
- The numbers are the ones `hatcheck usage` shows, live when it can be, with a snapshot's age
  otherwise; `--cached` skips the APIs.
- **Advice only.** It runs when you ask, prints commands, and never starts, moves or switches
  anything. Using several accounts to get around one plan's limits may break Anthropic's or OpenAI's
  terms; this is for choosing between accounts you may use for the work at hand, such as your own
  and your company's.

## `hatcheck bind`: tie a folder to a profile

With a work and a personal account, it's easy to start `claude` in a work repo out of habit. The
session then runs on the personal account and is filed in the personal profile. A binding says
which profile a folder's work belongs to:

```console
$ cd ~/code/api
$ hatcheck bind work                    # this folder and everything under it: ~/.claude-work, ~/.codex-work
Bound ~/code/api and everything under it: Claude Code ~/.claude-work · Codex ~/.codex-work.
Here, cl --resume <id> means cl work --resume <id> and cx resume <id> means cx work resume <id>.
$ hatcheck bind work --codex personal   # a different Codex home
$ hatcheck bind                         # what this folder is bound to
$ hatcheck bind --list                  # every bound folder
$ hatcheck unbind                       # untie it
$ hatcheck bind --hook                  # warn in Claude Code and Codex (once; hatcheck unbind --hook undoes it)
```

- Bindings live in your own config, `~/.config/hatcheck/bindings.json`
  (`%APPDATA%\hatcheck\bindings.json` on Windows), not in the repo, so profile names don't end up in
  git. The deepest bound folder wins, and folders are compared once symlinks are resolved.
- `default` binds `~/.claude` and `~/.codex`, and `~/.gemini/antigravity-cli`, `~/.kimi-code` and
  `~/.zcode` when agy, kimi and zcode have sessions there. The Codex home and the Kimi home are the
  ones named like the profile when there are any; `--codex <name>` and `--kimi <name>` pick others,
  and either alone binds just that home. `--agy default` and `--zcode default` bind just those
  folders.
- `cl`, `cx` and `ck` use the binding when no profile name is given: `cl --resume <id>`, `cl`,
  `cx resume <id>`, `ck --session <id>`. A name given still wins, and outside bound folders
  nothing changes.
- `hatcheck` marks sessions that ran in a bound folder under another profile:
  `⚠ folder bound to ~/.claude-work`.
- `--hook` adds a `SessionStart` hook, `hatcheck bind --check`, to every profile's `settings.json`
  and every Codex home's `hooks.json`. When a session starts in a bound folder under another
  profile, it shows a warning and nothing is blocked:
  `hatcheck: ~/code/api is bound to ~/.claude-work, but this session runs in ~/.claude-personal. Quit
  and run cl work here instead.` For a resumed session it gives the `hatcheck move` command instead.
  The hook runs the `hatcheck` that `--hook` found on your `PATH`, by its path. Run `--hook` again
  after installing hatcheck another way, and it points the hooks at the new one. No hook is added
  for kimi or zcode.
  - **Claude Code:** tested with the real `claude` (2.1.301). It shows the warning in the interactive
    UI; `claude -p` doesn't display it.
  - **Codex:** Codex runs a new hook only once you trust it with `/hooks` in `codex`, once per home.
    Tested with `codex exec` (0.163): it runs the hook from the home's `hooks.json`, with the folder,
    the session and `CODEX_HOME`, but `codex exec` doesn't print hook messages. Codex's docs say its
    interactive UI shows them as a warning; that part wasn't tested here.
  - **Antigravity CLI:** agy's hooks live in `~/.gemini/config/hooks.json`, as named hooks each with a
    list of handlers per event; `--hook` adds one named `hatcheck` with a `SessionStart` handler,
    `hatcheck bind --check agy`, beside any others, and `unbind --hook` takes just that out. agy
    hands the hook the conversation's `workspacePaths`, and the answer goes back in agy's form, a
    system message in `injectSteps`. With no agy profiles yet, it only speaks up for a folder bound
    to another agy folder with `--agy`. Tested with the real `agy` (1.4.2) in print mode: it ran
    the hook as a conversation started, and the model quoted the message back.

## `--json`: for scripts, status bars and apps

`hatcheck --json` and `hatcheck usage --json` print what the text shows, as data:

```console
$ hatcheck --json
{
  "version": 1,
  "generated_at": "2026-10-15T22:41:07Z",
  "hours": 24,
  "this_folder_only": false,
  "sessions": [
    {
      "agent": "claude",
      "id": "3f1c2a9e-8b7d-4c1e-9f2a-6d5e4c3b2a10",
      "profile": "/Users/me/.claude-work",
      "account": 0,
      "title": "Fix flaky login test",
      "cwd": "/Users/me/code/api",
      "last_active": "2026-10-15T22:37:02Z",
      "entrypoint": "cli",
      "branch": "main",
      "model": "claude-fable-5-1",
      "open": {"pid": 48213, "status": "busy", "in": "claude"},
      "continued_in": null,
      "bound_to": null,
      "resume": "cl work --resume 3f1c2a9e-8b7d-4c1e-9f2a-6d5e4c3b2a10"
    }
  ],
  "accounts": [
    {
      "agent": "claude",
      "profiles": ["/Users/me/.claude-work"],
      "plan": "team max 5x",
      "source": "live",
      "saved_at": null,
      "why": null,
      "limits": [
        {"name": "session (5h)", "short": "5h", "kind": "session", "used_percent": 20, "left_percent": 80,
         "resets_at": "2026-10-16T01:30:00Z", "window_seconds": 18000, "severity": "normal", "per_model": false,
         "pace": 28.42}
      ],
      "extra": null,
      "fable": {"left": true, "resets_at": "2026-10-20T16:00:00Z", "band": "fresh"}
    }
  ],
  "hidden": {"Desktop": 3},
  "other_folders": 0,
  "older_not_listed": {}
}
```

- **The same choices as the text:** the time window, `--all`, `--all-folders` and `-n` pick the same
  sessions in the same order, and `resume` is the command the text prints. `account` is the
  session's place in `accounts`, which holds the accounts behind the listed sessions (none with
  `--no-usage`). `hatcheck usage --json` gives every account, in its Fable order, with `sorted_by`.
- **Stable:** absolute paths rather than `~`, times in UTC (ISO 8601), no colours. `version` goes up
  only when a field changes meaning or goes away; new fields can appear at any time. `agent` is
  `claude`, `codex`, `agy`, `kimi` or `zcode`; a Kimi or ZCode session's `id` is the UUID after
  the agent's own `session_` or `sess_`, which `resume` puts back.
- `open` is `null`, or what a session is open in: Claude Code's with the process's `pid` and `status`,
  Codex's and agy's with `pid` and `status` `null`, since only a lock says they're open. Kimi's and
  ZCode's are never marked open, since nothing marks one.
- `left_percent` is `null` for a snapshot's limit that has reset since it was saved. `pace` is what's
  left per hour until the reset (`left_percent` over the hours to go, at least one), `null` without
  a reset ahead. `fable` is `{"left": true, "resets_at": …}`, `{"left": false, "back_at": …}`, or
  `null` without a Fable limit; its `band` is `fresh`, `low` (past 90% used), `nearly out` (past 98%)
  or `out`, the order `hatcheck usage` sorts by. `source` is `live`, `cached` or `none`, with `why`
  when it isn't live.
- Errors go to stderr with a non-zero exit, so stdout is always valid JSON. Email addresses only with
  `--email`.
