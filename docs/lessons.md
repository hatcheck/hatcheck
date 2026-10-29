# Lessons

What this repo's work got wrong, or nearly did, and the rule that would have caught it. Each
lesson is a rule, then why, then the evidence. Read it before changing code. A lesson seen a
second time gets its date added; after a third, it moves into [AGENTS.md](../AGENTS.md) as a
standard. The format comes from magpie's LESSONS file, which keeps the same kind of record.

## The agents' files

**Another app's success is read from its effect, never from its exit code or its words.**
Claude Code's headless `/usage` (`claude -p /usage --output-format json`) exits 0 whatever
happens: offline it prints its last reading, and in a profile that isn't signed in it prints a
cost summary. The only sign it fetched new numbers is the `fetchedAtMs` of the snapshot it saves
in the profile's `.claude.json` moving past the time the run started.
- Evidence: probed on 2026-10-18 with Claude Code 2.1.302, with the network blocked through a
  dead proxy (same text, snapshot unchanged) and with an empty profile (cost summary, exit 0).
  `claude_live` in `crates/hatcheck/src/usage.rs` goes by the timestamp.
- Seen 1× (2026-10-18).

**Claude Code makes a project folder for whatever folder it runs in, with `memory/` inside, even
with `--no-session-persistence`.** So anything that runs it from a temporary folder leaves one
`projects/<that folder's name>` behind in the profile per run. Run it from one folder of
hatcheck's own instead, so the folder it makes is the same every time.
- Evidence: the two probe runs above left `projects/-private-tmp-…-usage-probe-…/memory` and
  `…-run-here/memory` in a real profile (removed by hand). `usage_run_dir` in `usage.rs` is the
  one folder now.
- Seen 1× (2026-10-18).

**An agent that answers from its own cache is read by its cache's rule.** Claude Code's `/usage`
shows a reading under a minute old again without a request (`nt=60000` beside
`cachedUsageUtilization` in 2.1.307's binary), so a snapshot that stays put after a run within
the minute isn't a failed fetch, and a run within the minute asks for nothing. hatcheck called
every such run "offline, or rate limited?", a guess plausible enough that twenty runs in a row
passed unquestioned.
- Evidence: 2026-10-21: every `hat usage` of a timing session said so for all five team
  profiles; one run alone, 139 seconds after the last, moved the snapshot, and five a minute
  later didn't. `claude_reuses` in `usage.rs` reads the snapshot's age first.
- Seen 1× (2026-10-21).

**Logins stay with the agent that made them.** A token read from Claude Code's Keychain item or
`.credentials.json` and sent anywhere is a copy of a login; refreshing it rotates it and signs
Claude Code out. Reading usage through Claude Code's own `/usage` needs none of it.
- Evidence: the Python version and the first Rust port sent the token to the usage endpoint and
  documented "never refreshed: a refresh rotates it and would sign Claude Code out of that
  profile" (README, until 2026-10-18). Replaced by `run_claude_usage` in `usage.rs`.
- Seen 1× (2026-10-18).

**A path given to another app is spelled one way.** Claude Code names each profile's Keychain
entry after the exact `CLAUDE_CONFIG_DIR` string, so `~/.claude-a` and `/Users/me/.claude-a`
are two logins. `cl` always spells it `~/.claude-<name>` expanded the same way.
- Evidence: docs/agents.md, "`cl`: run Claude Code with a profile"; `cl` in `launch.rs`.

**Signed content is never rewritten.** Claude's thinking blocks carry a signature over their
text; a path repointed inside one makes the next account reject the session.
- Evidence: `repoint` in `move_cmd.rs` leaves `thinking` and `redacted_thinking` blocks alone;
  `hatcheck relocate` rewrites a line's top-level `cwd` only, nothing in messages.

**Which variable moves an agent's folder is found by probing, with a command that needs its
login and starts nothing.** Antigravity CLI's folder, `~/.gemini/antigravity-cli`, moves with
`HOME` and with nothing else; `agy models` lists models only when signed in and starts no
conversation, so run with each candidate variable pointed at an empty folder it says at once
whether the agent looked there.
- Evidence: probed on 2026-10-19 with agy 1.4.2: `ANTIGRAVITY_EXECUTABLE_DATA_DIR`,
  `XDG_CONFIG_HOME` and `GEMINI_CLI_HOME` were ignored, and `HOME=<empty folder>` made
  `<folder>/.gemini/antigravity-cli` and asked to sign in. `agy.rs` reads the one folder.
- Seen 1× (2026-10-19).

**An agent asked for a reading without a sign-in starts its sign-in.** agy's print mode, asked
anything while signed out, opens the browser on Google's sign-in page and waits a minute for it,
with stdin closed and `NO_BROWSER` set alike; the same `hat` that read usage in the morning opened
the browser in the afternoon, once agy's sign-in had lapsed. A reading is asked for only after a
command that refuses without a sign-in and starts nothing has said there is one: `agy models`
exits 1 at once with "Please sign in" and opens nothing.
- Evidence: 2026-10-19, agy 1.4.2, probed with empty home folders and a logging stand-in for
  `open` first on the PATH, so nothing opened: print mode called it with the sign-in URL, with and
  without `NO_BROWSER`; `agy models` didn't. `signed_in` in `agy.rs` runs before `run_usage`.
- Seen 1× (2026-10-19).

**A reading is asked of an agent once at a time across hatchecks.** The desktop app's engine
and the CLI can both ask agy for `/usage` at once, and two such runs at the same time made
Google's quota service answer the second with a 503 after a minute, which hatcheck showed as agy
not finishing. The first guess, that agy waited on hatcheck's stdin, was wrong: hatcheck has
given every run `Stdio::null()` since the port, which a test now guards; what the probe had done
was run two hatchecks at once.
- Evidence: 2026-10-20, agy 1.4.3: two `agy -p /usage` at once, stdin closed, gave one
  `SUCCESS` in 3 seconds and one `UNAVAILABLE (code 503)` after 68; `take_run_lock` in `agy.rs`,
  and a reading under a minute old handed to the one that waited.
- Seen 1× (2026-10-20).

**An agent's answer is read with the version installed now.** agy 1.4.3 answers `/usage` with
`buckets` where 1.4.2 had `windows`; hatcheck read the groups and found no limits, and showed no
data without saying the shape had changed. A reading that finds the groups but none of the
fields should say so, and the doc names the version the fields were taken from.
- Evidence: 2026-10-20: `agy's /usage reported limits hatcheck couldn't read` from `hat usage`,
  the day after agy 1.4.3 arrived; `usage_body` in `agy.rs` reads both shapes now. And
  2026-10-29, agy 1.4.10: a bucket per model and effort tier with no window, and the epoch as
  the reset of a limit nothing was spent from, which hatcheck showed as `?` for all 22, read as
  "reset since the snapshot"; noticed only when the list's comment ran off the screen.
- Seen 2× (2026-10-20, 2026-10-29).

**A probe that runs another agent stops at the first step that fails.** A check that converts a
session and then has the real agent read the file must not reach the agent when the conversion
was refused; run as one script without that check, it ran the agent on an empty path, which
cost a model turn and left a conversation in the agent's store.
- Evidence: 2026-10-19, `agy -p "Read , …"` after `hatcheck move` refused an open session:
  conversation `cd859a98` in the maintainer's agy folder, left for them to delete.
- Seen 1× (2026-10-19).

**A pseudo-terminal probe sends an agent nothing it hasn't asked for.** A probe that typed `y`
whenever a word like `allow` or `trust` appeared in the output answered mcode's status bar, and each
`y` became a message, a model turn each; the terminal UI had asked nothing, and had read the file
in its Auto mode without a question. The probe quits with the agent's own keys and sends nothing on
what it merely sees.
- Evidence: 2026-10-25, mcode 0.6.13: eight `y` turns in the session the probe made, read back by
  `hatcheck show`.
- Seen 1× (2026-10-25).

**A cleanup step is written down once it has been done, naming what it took out.** The step that
said to delete a probe's session from mcode's own session browser was written before anyone had:
the browser archives a session (`Archive session?`, "History stays available under Archived") and
nothing of mcode's deletes one, its in-session tool answering that deleting is reserved for the
user, in the app. The session was taken out by hand, following the deletion in mcode's own code,
which [maintenance.md](maintenance.md#3-clean-up) now lists table by table.
- Evidence: 2026-10-25, mcode 0.6.13: `renderArchiveConfirmation` in its launcher, the tool's
  `SESSION_DELETE_NOT_ALLOWED`, and `deleteSession` in its runtime, reached by the desktop
  service; the probe's rows in fifteen tables and its files, removed with a copy taken first, and
  `mcode acp`'s `session/list` showing the two sessions left.
- Seen 1× (2026-10-25).

**A session written for an agent follows what the agent's own importer writes, not what the agent
writes live.** kimi validates each event of a session it resumes against a schema and folds the
model's view from its `context.*` records; a wire built from the events kimi writes as it runs
failed twice over (a `toolCalls` left off a message, a `finishReason` outside its enum), while the
records kimi's own `kimi migrate` importer writes resume. An agent's importer is the smallest wire
it accepts, written by the people who know the schema.
- Evidence: 2026-10-20, kimi 2.2.0: `StoreError: event 'message.appended' at seq 6 failed schema
  validation`, then `Cannot read properties of undefined (reading 'map')`, both from a wire in the
  live shape; `write_session` in `kimi.rs` follows the importer (`buildTurnRecords` and
  `writeMainAgentWire` in kimi's binary).
- Seen 1× (2026-10-20).

**A resume is checked the way the printed command runs it.** kimi binds its model and profile when
its terminal UI opens a session; print mode (`-p`) on a session the UI hasn't opened yet says no
model is set. A probe through print mode alone would have called a working import broken, and the
command hatcheck prints is the UI's.
- Evidence: 2026-10-20, kimi 2.2.0: `model.not_configured: Model not set` from `kimi --session
  <id> -p …` on a session hatcheck wrote; the same session opened in the UI through a
  pseudo-terminal gained `profile.bind` and answered from the imported history.
- Seen 1× (2026-10-20).

**An agent's CLI taken from its desktop app may need what the app hands it.** ZCode's desktop app
ships the CLI as `Resources/glm/zcode.cjs` and starts it with `ZCODE_BUILTIN_PROVIDER_CONFIG_FILE`
pointed at the app's `Resources/config/provider/zcode-builtin.json`; run by hand without it, the
CLI refuses with its provider config missing, while `--help` and `doctor` run fine. A probe of a
bundled CLI starts from how the app starts it.
- Evidence: 2026-10-20, ZCode 0.16.9: `无法定位 CLI ZCode Built-in Provider Config` from
  `node zcode.cjs --attach … -p …`; with the variable set it read the file and resumed. docs/agents.md
  has the wrapper.
- Seen 1× (2026-10-20).

**An agent's print mode is asked for a reading only once a probe has shown it answers one.**
`kimi -p /usage` doesn't run the slash command: the words go to the model as a message, which
answered with the docs for `/usage`, fetched a web page on the way, and left a session folder in
the home, as every print-mode run does, signed in or not. Kimi's usage is read from the endpoint
its `/usage` asks instead, found in its binary.
- Evidence: 2026-10-20, kimi 2.2.0: two sessions in `~/.kimi-code/sessions/wd_kimi-usage-run_…`
  and two model turns from one probe, removed afterwards; `usage_url` and `usage_body` in
  `kimi.rs`.
- Seen 1× (2026-10-20).

**A probe that needs the agent's login runs in its real home: name what it leaves, and take it
out.** zcode's login moves with `ZCODE_DATA_BASE_DIR`, so its probe ran in a throwaway `HOME` with
only the login shared; kimi's login lives in its home, so its probe wrote a session into the real
`~/.kimi-code`, and kimi then added the folder to `workspaces.json`, `workspace-trust/` and
`user-history/`. Each was removed afterwards, and the index compared with a copy taken before.
- Evidence: 2026-10-20, the session folders, index lines and the three side files of the probe
  folder in `~/.kimi-code`, all removed; `session_index.jsonl` identical to its copy from before.
- Seen 1× (2026-10-20).

**An example custom profile name isn't a shipped interface; verify the official templates before
probing.** DSH's docs show `dsh --profile tui --resume` only as an example assuming a custom
terminal app, so `dsh tui` can't be assumed to start a shipped terminal UI. The shipped app
profiles are `web`, `headless`, `acp`, `sdk` and `sdk-minimal`.
- Evidence: dsh 0.2.1-rc.2, 2026-10-24: this checkout's `~/.dsh/profiles/tui` was base-only;
  `dsh tui --help` waited until the probe timed out, with no model turn or session created, while
  `dsh --help` and `dsh --profile headless --help` exited.
- Seen 1× (2026-10-24).

**Catalog and display metadata aren't canonical history.** A catalog can list a session and supply
its title and times, but `show` and conversions read only the active canonical history. An empty
canonical file stays an empty conversation; display rows are never merged to invent one.
- Evidence: 2026-10-24, a real MiniMax Code session had an empty active `messages.jsonl` while its
  display data held a user message. `summary` and `conversation` in `mcode.rs` keep the catalog and
  canonical history separate.
- Seen 1× (2026-10-24).

**A failed read means unknown, never empty.** A snapshot's limit that has reset since it was
saved is shown as `?`, not as 100% or 0%; usage that couldn't be fetched is shown as cached with
its age and the reason, never as fresh.
- Evidence: `percent_left` returning `None` in `usage.rs`; `why` on every cached account. And
  2026-10-24: `rank` in `best.rs` took a body with no limits it could read for an account with
  room, so `hatcheck best` recommended it; it's `NO_DATA` now.
- Seen 2× (2026-10-24).

**An agent's new version is checked when it comes out, not when something breaks.** The tests
read fixtures shaped like the versions they were written against, so they pass whatever an agent
changes; only a session the new version wrote, read by hatcheck, shows the change. Run
[maintenance.md](maintenance.md) every week or two.
- Evidence: 2026-10-24, the first such run: agy 1.4.7 lists a folder added with `--add-dir` before
  the one a conversation started in, so the conversation hatcheck's own import command starts was
  filed under hatcheck's cache folder; ZCode 3.16.3's CLI (0.17.0) keeps a deleted session's row,
  which hatcheck would have listed again; Kimi's `kimi-for-coding` had become K2.8 Preview while
  `claude-k` still called it K2.7 Code. `started_in_first` in `agy.rs`, `rows` in `zcode.rs`.
- Seen 1× (2026-10-24).

## What hatcheck changes

**A folder is deleted only once it's checked, where it's deleted, to be one of hatcheck's own.**
An instance's data folder is `~/.claude-instances/NAME`, so the name `..` made it the home
folder, and `hatcheck desktop remove --data` emptied that. The name was checked when the launcher
was made, but a launcher records any folder, and the old script's take theirs from a config file:
only a check at the deletion covers them all.
- Evidence: 2026-10-24, reproduced in a throwaway home: `hatcheck desktop new .. X`, then
  `remove .. --data`, deleted everything in the home folder and said it had removed the data
  folder. `own_folder` in `desktop.rs`.
- Seen 1× (2026-10-24).

**A change that writes over a copy checks that copy too.** `hatcheck move` refused a session open
where it was, and wrote over an older copy of it open where it was going.
- Evidence: 2026-10-24; `open_refusal` and `lock_or_stop` in `move_cmd.rs` take the profile or
  home the session goes to as well.
- Seen 1× (2026-10-24).

**A change made in steps puts back its own steps when one fails, so the run again that its error
suggests finds everything it left.** `hatcheck relocate` moved a transcript, failed to move its
data folder onto one already there, and said to run it again; the transcript, moved but still
naming the old folder, was then where neither that run nor the undo command looked.
- Evidence: 2026-10-24, reproduced in a throwaway home; `move_session` in `relocate.rs`, with a
  check before anything moves for the data folder too.
- Seen 1× (2026-10-24).

**A rule about which agents exist is applied wherever their homes are found, and the test that
pins it runs every command.** An agent's homes are found in three places: `paths::find_*` for
the list, `best`, `open`, `move` and `relocate`; `usage::find_profiles` and `find_codex_homes`
for `usage`, which also take a folder with a login and no sessions; and `where`'s own search of
`~/.codex*`. `config.toml`'s `ignore.agents` guarded the first, and `hat usage` and `hat where`
still found Codex.
- Evidence: 2026-10-29, `test_an_agent_left_out_is_looked_for_nowhere` in `tests/test_config.py`,
  which runs the list, `usage` and `where`; `unless_ignored` in `paths.rs`, and the guards in
  `usage.rs` and `where_cmd.rs`.
- Seen 1× (2026-10-29).

## What hatcheck prints

**A printed command is quoted for the shell that will run it, and a flag is never quoted.**
- Evidence: 4afe947 "fix: keep --fork bare in the rerun command printed for PowerShell"; the
  `fork_command` test in `move_cmd.rs`.

**The exit status says whether it found what it was asked for, and `--json` keeps stdout valid.**
Errors go to stderr with a non-zero exit, so a script can trust stdout.
- Evidence: 1b3a09f "fix: make claude-where's exit status say whether it found the session";
  `test_errors_leave_stdout_empty` in `tests/test_json.py`; `hatcheck relocate` exits 1 when no
  session ran in the folder.

**A preview writes nothing.** `--dry-run` runs every check the real thing runs, and changes
nothing, so what it says is what would happen.
- Evidence: `hatcheck move --dry-run`, and `hatcheck relocate --dry-run`, whose
  `test_dry_run_changes_nothing` compares the transcript's bytes before and after.

**The engine never runs a launcher.** A command the desktop app's engine runs as `cl` would put
Claude Code in the engine's place.
- Evidence: 64f475f "fix: never run a launcher from hatcheck serve"; `served` in `cli.rs`.

## Tests

Windows paths, seen three times (fc1c21f, `test_sessions_come_along` and agy's
`workspace_uris_are_paths`), are a standard in [AGENTS.md](../AGENTS.md) now.

**Tests never touch the real machine or the network.** They run in a throwaway home with the
`PATH` cut to `/bin` on macOS, so the `security` tool and the real `claude` are never found; a
fake `claude` stands in for Claude Code, and Codex tokens are expired so nothing is sent.
- Evidence: `FakeHome.env` and `fake_claude` in `tests/helpers.py`.

**A fake can only use what the test's `PATH` has.** With `PATH=/bin`, a wrapper that calls
`dirname` (in `/usr/bin`) fails silently, and the test fails somewhere else.
- Evidence: the first fake `claude` wrapper, 2026-10-18; `${0%/*}` now.
- Seen 1× (2026-10-18).

**Two processes appending to one file can write over each other on Windows.** Its C runtime
appends by moving to the end and then writing, so runs at once can land on the same spot. A fake
that several runs use at once writes a file per run.
- Evidence: `test_default_profile_runs_without_a_config_dir` failed on windows-latest
  (2026-10-19) on a line of the fake `claude`'s shared log that wasn't JSON, since `usage` runs
  `/usage` in each profile at once; the fake writes `claude-runs/<time>-<pid>.json` now.
- Seen 1× (2026-10-19).

**Another app's format is checked against the real app, and the version is written down.**
- Evidence: docs/commands.md, `hatcheck move`: "tested by resuming them with the real `claude` (2.1.301)
  and `codex` (0.155.1)"; `hatcheck bind --hook`: "tested with the real `claude` (2.1.301)";
  `docs/rust-port.md`: every ported command compared with the Python one byte for byte on real
  profiles before it counted as ported.

## Releases

**What wasn't built wasn't released.** A release workflow is run for every target before a tag
is trusted, and a tag is not a release until its files exist.
- Evidence: 6403cbc "fix: build the ARM Linux wheel on ARM, and let the PyPI workflow run for
  a tag".

**A tag goes on the release commit, after it exists.** The three workflows a `vX.Y.Z` tag starts
check the tag against the version in `Cargo.toml` and refuse one that doesn't match, so a tag
pushed first releases nothing and has to be deleted and pushed again. The steps are in
[maintenance.md](maintenance.md#releasing-the-cli); until they were, they lived in a conversation.
- Evidence: 2026-10-25, `v0.6.0` on a commit whose `Cargo.toml` said 0.5.0: "The tag is v0.6.0 but
  the version is 0.5.0" from the PyPI and crates.io workflows, and Release found no package at that
  version; nothing was published.
- Seen 1× (2026-10-25).
