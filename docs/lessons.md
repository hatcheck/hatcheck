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

**Logins stay with the agent that made them.** A token read from Claude Code's Keychain item or
`.credentials.json` and sent anywhere is a copy of a login; refreshing it rotates it and signs
Claude Code out. Reading usage through Claude Code's own `/usage` needs none of it.
- Evidence: the Python version and the first Rust port sent the token to the usage endpoint and
  documented "never refreshed: a refresh rotates it and would sign Claude Code out of that
  profile" (README, until 2026-10-18). Replaced by `run_claude_usage` in `usage.rs`.
- Seen 1× (2026-10-18).

**A path given to another app is spelled one way.** Claude Code names each profile's Keychain
entry after the exact `CLAUDE_CONFIG_DIR` string, so `~/.claude-work` and `/Users/me/.claude-work`
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

**A probe that runs another agent stops at the first step that fails.** A check that converts a
session and then has the real agent read the file must not reach the agent when the conversion
was refused; run as one script without that check, it ran the agent on an empty path, which
cost a model turn and left a conversation in the agent's store.
- Evidence: 2026-10-19, `agy -p "Read , …"` after `hatcheck move` refused an open session:
  conversation `cd859a98` in the maintainer's agy folder, left for them to delete.
- Seen 1× (2026-10-19).

**A failed read means unknown, never empty.** A snapshot's limit that has reset since it was
saved is shown as `?`, not as 100% or 0%; usage that couldn't be fetched is shown as cached with
its age and the reason, never as fresh.
- Evidence: `percent_left` returning `None` in `usage.rs`; `why` on every cached account.

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
