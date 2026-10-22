# Changelog

<!-- One line per paragraph and per bullet: a release's section here becomes its notes on GitHub, where a line break shows. -->

What changed in each release of the hatcheck CLI: `hatcheck`, `hat`, `cl`, `cx`, `claude-z` and `claude-k`. A release's notes on GitHub are its section here. The desktop app is released on its own, from `desktop-v*` tags.

## Unreleased

Kimi Code CLI (`kimi`) and ZCode (`zcode`) join Claude Code, Codex and Antigravity CLI: their sessions are listed, shown and carried to the other agents, and theirs to them; a Kimi session moves between Kimi homes as a Codex session does between Codex homes.

### Added

- **Kimi Code CLI (`kimi`)** beside the others, with **`ck`**, a launcher for its homes as `cx` is for Codex's: `ck work login` signs a new `~/.kimi-code-work` in, `ck work` runs Kimi Code CLI with it, and in a bound folder the name can be left out. `hatcheck` lists the sessions of every `~/.kimi-code*` home with `kimi --session session_<id>` or `ck <name> --session session_<id>` to resume each, and `where`, `show`, `open`, `best`, `bind --kimi` and `relocate` know them. `hatcheck move` and `fork` carry a session between Kimi homes as its folder, with its `state.json` repointed and its line in kimi's index; convert a Kimi session into a Claude Code, Codex or agy session; and convert a Claude Code, Codex, agy or ZCode session into a Kimi session, written as kimi's own importer writes one, which kimi's terminal UI resumes. `--json` has `"agent": "kimi"`, with the session's `id` the UUID after `session_`. Checked against kimi 2.2.0. Nothing marks a Kimi session open, so quit kimi before moving one.
- **ZCode (`zcode`)** beside the others: `hatcheck` lists the sessions in `~/.zcode`'s database with `zcode --resume sess_<id>` to resume each, read-only, and `where`, `show`, `open`, `best`, `bind --zcode` and `relocate` know them. `hatcheck move` and `fork` convert a ZCode session into a session of another agent, and a session of another agent into a file zcode is started on with `--attach`, since hatcheck never writes ZCode's database. `--json` has `"agent": "zcode"`, with the session's `id` the UUID after `sess_`. Checked against ZCode 0.16.9. `hatcheck usage` lists its folder with why there's no reading: zcode has no usage command, and Z.ai shows the plan's usage on its site.
- **`hatcheck usage` and the list show each Kimi home's limits:** the 5-hour, 7-day and monthly windows from the endpoint kimi's own `/usage` asks, with the home's token sent as it is and never refreshed; expired or offline, hatcheck's last reading is shown with its age. `hatcheck best` ranks Kimi homes by them.
- **A bare profile name follows a Kimi session too:** `work` is `~/.kimi-code-work` when that home exists and `~/.claude-work` otherwise, with `kimi:work` to spell it out; `kimi` and `zcode` name their default folders in `move`, `fork`, `open --login`, `best --wait` and `bind`, and `bind default` binds them along with the rest.
- **`hatcheck resume`, `hat r` for short:** carries on the newest session started from this folder, or the one named, in this terminal, showing the session and the command and asking first; `--yes` skips the question, `--print` gives the commands, and without a terminal to ask at it stops rather than start an agent. `hatcheck open` keeps opening a new terminal window. `r` now names `resume`; `relocate` is `rel`.
- **`hatcheck doctor`** lists `kimi` and `zcode` with the other agents, and `ck` with the commands; every way of installing carries `ck`; the desktop app names both agents and signs in to a new Kimi home.

### Changed

- **`hatcheck move work` and `hatcheck fork work`, with a profile alone:** the newest session started from this folder goes to that profile, after asking, as with nothing it goes to the folder's Claude Code profile. The word has to name a profile or home that exists and start no session's ID; one that does both is asked for more.
- **The session list's heading** names each agent that has a profile, home or folder here: `Claude Code, Codex and Kimi Code CLI sessions`, and so on.

### Fixed

- **agy 1.4.3's `/usage`** puts each group's limits in `buckets`, each with a window; hatcheck showed no data for it, and now shows one row per bucket, with the window's length behind the time bar.
- **Two hatchecks asking agy at once**, as the desktop app's engine and the CLI can, made the second wait on Google's quota service for a minute and then show no data: now one hatcheck asks agy at a time, and the second takes the reading the first saved.
- **`hatcheck usage`, `best` and the list took five to ten seconds**, nearly all of it agy's two requests to Google: a reading of agy's `/usage` under five minutes old is now shown again, as `cached <1m ago (asked at most once in 5m)`, so most runs take a second or two; `--cached` shows it at any age. While a run asks, a line on stderr says who is being asked (`Asking Claude Code (5 accounts), Codex, agy and Kimi for usage…`) and goes when the answers are in; only when stderr is a terminal, and never on stdout, so a pipe and `--json` see nothing of it. A run that gives up on an agent says the seconds it waited (`agy didn't finish within 45s`), not `<1m`.
- **A run within a minute of another said Claude Code's `/usage` `couldn't fetch new numbers; offline, or rate limited?`** for every profile: Claude Code's `/usage` shows a snapshot under a minute old again without asking Anthropic (2.1.307), so hatcheck now shows that snapshot as `cached <1m ago (Claude Code's /usage reuses a reading under a minute old)` and doesn't run `claude` for it, which takes another second off such a run.

## 0.4.0 - 2026-10-19

Antigravity CLI (`agy`) joins Claude Code and Codex: its conversations are listed, shown and carried to the other agents, and theirs to it. `hatcheck doctor` says how hatcheck and the agents are installed, and the README is now the short tour, with each command documented as it is in `docs/`.

### Added

- **Antigravity CLI (`agy`)** beside Claude Code and Codex: `hatcheck` lists its conversations from `~/.gemini/antigravity-cli` with `agy --conversation <id>` to resume each, `hatcheck usage` and `hatcheck best` show what its `/usage` reports, and `where`, `show`, `open` and `bind` know its sessions. `hatcheck move` and `hatcheck fork` convert an agy conversation into a Claude Code or Codex session, and a Claude Code or Codex session into a file agy reads to carry on, since agy's own store can't be written. The Antigravity app's conversations, kept the same way in `~/.gemini/antigravity`, are listed with `--all`, shown and converted too. `--json` has `"agent": "agy"`. Checked against agy 1.4.2; agy keeps one login per home folder, so there are no agy profiles yet. `bind --hook` adds agy's hook too, in its own form in `~/.gemini/config/hooks.json`. Before reading usage, hatcheck asks agy for its model list, which agy answers only when signed in, so a run never opens agy's sign-in in the browser; signed out, the row says so.

- **`hatcheck move` and `hatcheck fork` with nothing** take the newest session started from this folder to the folder's bound Claude Code profile, or `~/.claude`, after asking in the terminal; `--dry-run` just says, and a script has to say both. A session already where it would go is refused before anything is asked.
- **Codex and agy sessions show `● open` too**, from the lock each holds on a session it has open, as `● open in codex` or `in agy`, and an open one is listed even when older than the window; `--json`'s `open` says what it's open `in`.
- **A bare profile name follows the session:** for a Codex session, `work` is `~/.codex-work` when that home exists and `~/.claude-work` otherwise; when both exist you're asked which, or say `codex:work` or `claude:work`. `claude` names `~/.claude` as a profile, as `codex` and `agy` name theirs, in `move`, `fork`, `open --login`, `best --wait` and `bind`.
- **The installers point at `hatcheck doctor`:** the release installers end by naming it, and `install.sh` and `install.ps1` say when another hatcheck comes first on your `PATH`, so a second install doesn't go unnoticed.
- **`hatcheck doctor`** lists every install of hatcheck on your `PATH` (uv, pipx, Homebrew, cargo, winget, the release installer or a build from a clone), which one runs and the line that removes each of the others; every `claude`, `codex` and `agy` with its version and where it came from; and the hooks `bind --hook` added that run a hatcheck that has gone. It changes nothing; `--json` prints what it found, and the exit status says whether there's something to fix. `d` is `desktop` as a short name, so `doctor` is `do`.

### Changed

- **The list names each session's agent by its command, not a tag:** the `codex` and `agy` after a title are gone, and the program that resumes the session, `cl`, `claude`, `cx`, `codex` or `agy`, is in cyan instead. The branch after the title stays: it's the one the session ran on, as the agent wrote it down, so an agy row has none. `--json` has `agent` as before.
- **The docs:** the README keeps what someone deciding to install needs, a tour with one line per command, and the rules; each command, agent and way of installing is documented as it is in `docs/commands.md`, `docs/agents.md` and `docs/install.md`, the sections moved with their headings, so links into them still land.

### Fixed

- **`install.sh` and `install.ps1`** add `hat` when the `hat` already on your `PATH` is hatcheck's own, from another install; they held it back as if it were another program's.
- **`hatcheck bind --hook`** points a hook whose `hatcheck` has gone, as after installing hatcheck another way, at the one it finds now, where it said the profile already had the hook. A Codex hook it repoints may need trusting again with `/hooks`.

## 0.3.0

`hatcheck move` now moves: it deletes the original once every copied file is checked, where 0.2.0 copied unless given `--no-keep`. `--keep` copies, as 0.2.0 did by default.

### Changed

- **`hatcheck move` moves by default.** The original is deleted only after every copied file is checked against it, and plan files stay, since sessions can share one. `--keep` keeps the original, which copies the session. `--no-keep` is still accepted.
- **A move never puts an older copy over a newer one.** When the profile already has a copy that went on further, the move stops and changes nothing, in a dry run too, and prints the command that resumes that copy and the files of the older one, to delete by hand. With `--json` this is now an error (exit 1, nothing on stdout) where 0.2.0 reported `"action": "nothing"`.
- **Claude Code usage comes from Claude Code.** hatcheck runs `claude -p /usage` in each profile and reads the snapshot Claude Code saves, so it never reads a Claude login: no Keychain read, and no usage request of its own. Without `claude` on the `PATH`, signed out or offline, the last snapshot is shown, with its age and why. Codex usage is read as before.
- **`hatcheck best`** shows each account's pace, what's left per hour until the reset, and sorts accounts past 90% of their Fable limit behind fresher ones, and those past 98% behind those.

### Added

- **`hatcheck fork SESSION PROFILE`** brings a session into another profile as a new session with its own ID, beside any copy already there, and keeps the original, as `move --fork` did in 0.2.0. `move --fork` is still accepted and does the same. A move that stops because the session went on in both profiles prints the fork command.
- **`hatcheck relocate OLD NEW`** brings the sessions of a project folder that was moved or renamed along to its new place, in every Claude Code profile and Codex home, and moves a folder binding with them. `--dry-run` writes nothing, and `hatcheck relocate NEW OLD` puts them back byte for byte.
- **`hatcheck best --wait [PROFILE]`** sleeps until an account has room, then answers.
- **Short subcommands:** a subcommand can be cut to its first letters, `hat m` for `hat move` and `hat f` for `hat fork`. `b`, `s` and `u` are `best`, `show` and `usage`, so `bind` and `unbind` take two letters, `bi` and `un`.
- **`--json`:** `limits[].pace` and `fable.band` for each account, and `hatcheck fork --json`.
- **crates.io:** `cargo install hatcheck` builds it, and `cargo binstall hatcheck` fetches the release's binaries.

## 0.2.0 - 2026-10-18

The first release of the Rust binaries, which replace the Python version: the same commands and the same output, with no Python needed.

- Binaries for macOS (Apple Silicon and Intel), Linux (x64, ARM64 and x64 musl) and Windows (x64) on the GitHub release, with shell and PowerShell installers, and `brew install hatcheck/tap/hatcheck`.
- PyPI has them as one wheel per platform, so `uv tool install hatcheck` and `pipx install hatcheck` install the same binaries.

## 0.1.0 - 2026-10-17

The first release: the Python version, on PyPI.
