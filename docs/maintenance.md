# Keeping up with the agents

hatcheck reads files other apps write, prints commands they take, and names their models, apps
and installers; each of them changes every few weeks. The tests can't see it: they read fixtures
shaped like the versions they were written against. So every week or two, and before a release,
check the newest version of each agent against hatcheck, and update the code, docs and site with
what changed. [lessons.md](lessons.md#the-agents-files) says why, with what the first run found.

## 1. What's new

```sh
python3 scripts/upstream.py --providers
```

It changes nothing and runs no agent but `--version`. It prints, for each agent, the version
installed here, the latest release and the versions the docs cite; each desktop app's Homebrew
cask and whether it's deprecated; every link in the README, `docs/` and `site/` that doesn't
answer 200 or now redirects, install scripts included; and, with `--providers`, Kimi's and Z.ai's
model lists beside the menus of `claude-k` and `claude-z` (asked with the keys in their config
files, sent only to those providers' model-list endpoints). It ends with what needs a look: a
latest release that isn't a version the docs cite, a cask the docs say to install that's
deprecated or gone, a link that doesn't answer 200, and a menu's model its provider no longer
lists. An agent added to hatcheck gets a row in its `AGENTS`.

Every Monday, [`upstream.yml`](../.github/workflows/upstream.yml) runs it on GitHub without
`--providers`, so it sends no key and runs no model, and keeps one issue labelled `upstream` open
while anything needs a look: rewritten when the list changes, closed once a run finds nothing.
`gh workflow run upstream.yml` runs it at once. GitHub stops a public repository's schedules after
60 days without activity; the Actions tab turns this one back on.

Then read each maker's install page and each provider's Claude Code guide for what a list can't
show: a renamed app, a new install command, a plan renamed, a model's context or a setting it
needs. The addresses are in [agents.md](agents.md#getting-the-agents) and the `claude-z`,
`claude-k` section. The makers' data-use pages the rules page links are among the links the
script checks; what they say is for the reader, and hatcheck's pages never summarise it, since
the terms differ by product, plan and region, and change.

## 2. Probe each new version

Bring each agent here to its latest (its own updater, or `npx -y <package>@<version>` for one
run of an npm one), then probe from a folder of its own, so everything a probe leaves is easy to
find: `mkdir` one in a temporary folder, `git init` it, `touch` a marker beside it. Use prompts
that ask for one word, so nothing of yours is sent anywhere, and an account of yours with room.

- **A session from each:** `cl <profile> -p "Reply with exactly the word PINEAPPLE"`, `codex exec "…
  MANGO"`, `kimi -p "… KIWI"`, `qwen -p "… PAPAYA"`, `copilot -p "… GUAVA"`, `opencode run "…
  LYCHEE"`. In the folder, `hatcheck sessions --all --json --no-usage` lists each with its folder,
  title, model and resume command, and `hatcheck show` and `where` find each. For copilot, `copilot
  --resume <id>` in its terminal UI (on a pseudo-terminal, quit with Ctrl-C and killed afterwards)
  checks that `hatcheck` marks the session open while it runs, by the `inuse.<pid>.lock` in its
  folder, and not once it's gone. For qwen, `qwen --resume <id> -p "Which single word did you reply
  with earlier? Answer with just that word."` from the folder checks the printed resume command, and
  `qwen --resume <id>` in its terminal UI (on a pseudo-terminal, killed afterwards) checks that
  `hatcheck` marks the session open while it runs, and not once it's gone.
- **Into each agent:** `hatcheck fork <claude-id> codex`, then `codex exec resume <new-id> "Which
  single word did you reply with earlier? Answer with just that word."`, which should answer
  PINEAPPLE; `fork <codex-id> <profile>`, then `cl <profile> -p --resume <new-id> "…"`; `fork
  <id> kimi`, opened once in kimi's terminal UI (`kimi --session …`, which validates every event,
  then quit) and asked in print mode; `fork <id> agy`, then the printed `agy --add-dir … ` with `-p`
  and the question in place of `-i`. The agy conversation that makes is also the check that
  hatcheck reads agy's newest version: it's listed in the probe folder, and `show` reads it.
  `fork <id> mcode`, then the printed `mcode '…'` in its terminal UI on a pseudo-terminal, with
  the question added to the prompt, quit with its own keys and killed afterwards, and sent nothing
  else: a key sent on a word in its output becomes a message. `hatcheck show` of the session mcode
  made reads its reply. DSH's handover is a prompt pasted into its web UI, and isn't probed.
  `fork <id> copilot`, then the printed `copilot sessions import …`, which prints the new session's
  ID, then `copilot --resume <new-id> -p "Which single word…"`; `fork <id> opencode`, then the
  printed `opencode session import …` from the folder, then `opencode run --session ses_<new-id>
  "Which single word…"`. Both importers were checked without a login on 2026-10-25, in a
  throwaway home with copilot's `--dry-run` and a real import, and opencode's with its service
  disabled (`service.json` `{"disabled": true}` in its config folder); the resumes above are the
  check with a login. For copilot, sign in twice with two `COPILOT_HOME`s to see whether its system
  credential store keeps one login per home, which hatcheck has no way to read and hasn't checked.
- **Between profiles:** `hatcheck move` and `fork` a Claude Code session to another profile with
  room, and resume it there.
- **Usage:** `hatcheck usage --json`: every signed-in account reads `live`, with its limits.
- **ZCode:** its CLI comes in the app and shares the app's database, so a newer CLI is compared,
  not run: mount the new app's disk image read-only, and compare its `Resources/glm/zcode.cjs`
  with the installed one: the version, the `create table` and `alter table` statements for
  `session`, `message` and `part`, the migrations (`schema_migration` in `~/.zcode/cli/db/db.sqlite`
  names those applied), and the flags hatcheck prints (`--resume`, `--attach`, `--mode`, `-p`,
  `-c`).
- **claude-k and claude-z:** `python3 scripts/provider_probe.py` runs each model of their menus
  through Claude Code once.

## 3. Clean up

Every probe leaves something in a real home. Look for what changed since the marker (`find <home>
-newer <marker>`), and take out only what the probe made, after copying any shared file you edit:

- Claude Code: the probe folder's `projects/<key>` folder in each profile used, and the prompt's
  line at the end of its `history.jsonl`; its other files that change (`.claude.json`, remote
  settings, plugin sync) are its own housekeeping, left alone.
- Codex: the probe's `sessions/YYYY/MM/DD/rollout-…-<id>.jsonl` files and their lines in
  `session_index.jsonl`.
- Kimi: `sessions/wd_<probe>_<hash>/`, its sessions' lines in `session_index.jsonl`, its entry in
  `workspaces.json` (kimi writes it compact: keep it so), `workspace-trust/wd_<probe>_<hash>`, and
  the `user-history/` file of what you typed in its UI.
- agy: the conversation stays: it lives in agy's own database, which hatcheck never writes; delete
  it from the Antigravity app if you want it gone.
- mcode: nothing of mcode's deletes a session for you. Its session browser (`mcode --session`)
  archives one (`Archive session?`, "History stays available under Archived"), its in-session tool
  refuses (`Deleting a session is reserved for the user`), and the deletion in its code (0.6.13) is
  reached only through the desktop app's service. So take the probe's session out by hand, the way
  that code does, with no mcode running and a copy of `v2/sqlite/runtime-state.sqlite` (with its
  `-wal` and `-shm`) taken first. In one transaction, its rows by `session_id` in these
  `local_runtime_` tables: `queue_items`, `queue_row_migrations`, `queues`, `session_assets`,
  `session_asset_index_state`, `token_usage`, `message_rows`, `message_row_migrations`, `messages`,
  `pi_history_file_migrations`, `pi_history_rows`, `pi_history_row_migrations`,
  `ledger_watermarks`, `session_projection_watermarks`, `turn_diffs`, `turn_diff_journal` (its
  trigger clears `turn_diff_retention_queue`), `turn_diff_rewind_operations`,
  `communication_messages` (`from_session` and `to_session`), `query_view_states`,
  `turn_ingress_client_requests`, `turn_ingress_sequences` (by the session's `turn_id`s in
  `turn_ingress`), `turn_ingress`, `session_agent_state`, `sessions_fts` (by the `fts_rowid` in
  `session_fts_keys`), `session_fts_keys` and `sessions`; then what the probe alone made, its
  `background_tasks` row (`owner_session_id`) and its `projects` row for the probe folder, once no
  session names it. Its files: the session's folder under `v2/sessions/<date>/`,
  `sessions/mvs_<id>/permission.json`, the `background-tasks/bg_…` folder,
  `v2/observability/logs/sessions/mvs_<id>/`, the run's `v2/observability/mcode/…-<pid>.jsonl`,
  its lease in `v2/runtime-owner-leases/` and its socket in `run/` if the run was killed, and its
  empty folder in `v2/mcode/drafts/`. `mcode acp`, sent `initialize` and `session/list` over
  stdin and nothing else, then shows what mcode lists. The shared `runtime-<hour>.log`, the events
  file and the migration receipts are mcode's housekeeping, left alone.
- qwen: the probe folder's `projects/<folder>` (its chats, `meta.json`, `extract-cursor.json` and
  `memory/`), the session's lines in `usage/token-usage-<month>.jsonl` and `usage_record.jsonl`,
  `tmp/<sha256 of the folder>` and `file-history/<session>` when the UI ran, and the registry
  record `sessions/<pid>.json` a killed UI couldn't remove.
- copilot: the probe's session folders under `session-state/` and their rows in `session-store.db`,
  which its session browser lists; the `sidebar-sessions-state/<hash>.json` of the probe folder,
  and a `logs/process-<ms>-<pid>.log` per run. The package cache under `~/Library/Caches/copilot`
  (`XDG_CACHE_HOME`'s on Linux) is copilot's own.
- opencode: the probe's session, which `opencode session delete <id>` removes, and the `project`
  row for the probe folder, which stays; a `log/opencode.log` per day.
- hatcheck: the probe's `imports/<id>.md`, `.jsonl` or `.json` in its cache folder.

## 4. Update

- **Code:** what changed in a format, a fix with a test whose fixture has the new shape (the
  helpers in `tests/helpers.py` take what each version writes).
- **Docs:** each "checked against" names the version and date it was checked again, and only what
  was; dated evidence of a probe that wasn't run again stays as it was. Installers, apps, models
  and plans in [agents.md](agents.md#getting-the-agents) and the `claude-z`, `claude-k` section;
  a changed format in [formats.md](formats.md) too.
- **Site:** `site/agents.html`'s table and its install commands and app cards; at a release,
  `python3 site/refresh.py` for the help blocks and samples (it runs the release binary; for the
  current build, `python3 site/refresh.py "$PWD/target/debug/hatcheck"`, by absolute path).
- **CHANGELOG.md** for what users notice, and [lessons.md](lessons.md) when something had broken.
- Add a line below.

## Releasing the CLI

Steps 1 to 4 run first. A release is then a commit and a tag on it, in that order: the three
workflows a tag starts check the tag against the version in `Cargo.toml` and refuse one that
doesn't match, so a tag pushed before the release commit releases nothing. (`v0.6.0` went up on a
0.5.0 commit on 2026-10-25: all three refused, nothing was published, and the tag was deleted and
pushed again on the release commit.) The desktop app is released on its own, from `desktop-v*`
tags ([desktop/README.md](../desktop/README.md#releases)).

1. **The checks**, on `main` with CI green for its last push on all three platforms (`gh run list
   --branch main --limit 3`): `cargo fmt -p hatcheck -- --check`, `cargo clippy -p hatcheck
   --all-targets -- -D warnings`, `cargo test -p hatcheck`, `python3 -m unittest discover -s
   tests`, `cargo publish -p hatcheck --dry-run --locked --allow-dirty` (what crates.io will get
   packages and builds) and `cargo check` in `desktop/src-tauri`.
2. **The release commit**, `chore: release X.Y.Z, with its notes in CHANGELOG.md`, whose body says
   what was run: the version in `Cargo.toml` (`[workspace.package]`, which the crate and the PyPI
   package take) and `Cargo.lock`; in `packaging/winget/cli/`, `PackageVersion` in all three
   manifests and the `InstallerUrl` and `ReleaseNotesUrl` for `vX.Y.Z`, with `InstallerSha256`
   left as the last release's until the new zip exists; in `CHANGELOG.md`, `## Unreleased` renamed
   `## X.Y.Z - YYYY-MM-DD`, with a paragraph under it saying what the release is, one line per
   paragraph and bullet, since the section becomes the release's notes on GitHub, where a line
   break shows. `tests/test_packaging.py` pins the versions and the URL together, and
   `target/debug/hatcheck --version` prints the new one.
3. **The tag**, annotated, on that commit and pushed with it:

   ```sh
   git tag -a vX.Y.Z -m "hatcheck X.Y.Z: what it brings, in a line"
   git push origin main vX.Y.Z
   ```

   Three workflows run on it, each on its own. **Release** (`v-release.yml`, written by cargo-dist
   from `dist-workspace.toml`) builds the seven commands for the six targets, the shell and
   PowerShell installers and the Homebrew formula, pushes the formula to `hatcheck/homebrew-tap`
   and makes the GitHub release, with the changelog section as its notes. **PyPI** (`release.yml`)
   builds a wheel for each of five platforms and the sdist, and publishes them through PyPI's
   trusted publisher. **crates.io** (`crates.yml`) runs `cargo publish -p hatcheck --locked`
   through crates.io's. `gh run list --limit 5` watches them and `gh run view <id> --log-failed`
   reads a failure; a failed one runs again without a new tag, `gh run rerun <id>` for Release and
   `gh workflow run release.yml -f tag=vX.Y.Z` for PyPI, the same with `crates.yml` for crates.io.
   The release is out when the GitHub release has an archive and its `.sha256` for each target,
   the two installers, `hatcheck.rb`, `sha256.sum` and `dist-manifest.json`, the version is on
   [pypi.org/project/hatcheck](https://pypi.org/project/hatcheck/) and
   [crates.io/crates/hatcheck](https://crates.io/crates/hatcheck), and the tap's formula names it.
4. **After it**, each commit's body saying what was run:
   - `chore: give the CLI's winget manifest X.Y.Z's zip SHA-256`: download
     `hatcheck-x86_64-pc-windows-msvc.zip` and its `.sha256` from the release, check that
     `shasum -a 256` of the zip gives the published hash and `unzip -l` lists the seven `.exe`
     files at its top, put the hash in `InstallerSha256` (upper case, as the earlier ones), and
     run `python3 -m unittest discover -s tests -p test_packaging.py`. The manifests are ready for
     a winget submission, which hasn't been made yet.
   - `docs: bring the site up to X.Y.Z`: `cargo build --release && python3 site/refresh.py`
     rewrites the site's help blocks and samples from the release binary; read the diff, then
     `site/deploy.sh` ships it.
   - A correction to the notes on GitHub goes in `CHANGELOG.md` first, then
     `gh release edit vX.Y.Z --notes-file <file>` with the section's text.
   - `cargo binstall hatcheck --no-track --install-path <empty folder>` fetches the release's
     archive for this machine and unpacks the seven commands: the crate's `binstall` metadata still
     matches what cargo-dist packed.

## Runs

| Date | Checked against | Found |
|---|---|---|
| 2026-10-24 | Claude Code 2.1.310, Codex 0.164.0, agy 1.4.7, kimi 2.3.1, ZCode 0.16.9 (and 0.17.0 compared), dsh 0.2.1-rc.2, qwen 0.26.0 (added, both ways) | agy 1.4.7 lists `--add-dir` folders first, filing an imported conversation under hatcheck's cache; ZCode 0.17.0 keeps deleted sessions' rows; Kimi's K3.1 Pro and English address, and `kimi-for-coding` renamed K2.8 Preview; Z.ai pairs `glm-5.3-flash[1m]`; the Codex app is part of ChatGPT; Claude Desktop has a Linux beta; qwen 0.26.1 is out on npm while Homebrew has 0.26.0 |
| 2026-10-25 | mcode 0.6.13 (added as a target, as a file) | mcode's terminal UI starts on the prompt and reads the file outside the workspace without a question in its Auto mode; a pseudo-terminal probe that answered words in its status bar sent keys mcode took as messages, eight turns of `y`; mcode's session browser archives and nothing of mcode's deletes, so the probe's session was taken out by hand, following mcode's own deletion |
| 2026-10-25 | copilot 1.0.103, opencode 2.1.4 (added, both ways; importers probed without a login) | copilot's `sessions import` takes a version-1 interchange JSONL (a `session` header, then `message`, `tool-call` and `tool-result` records); it validated and imported hatcheck's file into a throwaway home, naming the session by its title. opencode's `session import` took the `{info, messages}` JSON its export prints, with the background service disabled, and listed and exported the session back. Resuming either with a signed-in agent wasn't run. `opencode2` is a shim opencode's installer writes that runs `opencode` |
