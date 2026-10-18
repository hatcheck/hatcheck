# Changelog

<!-- One line per paragraph and per bullet: a release's section here becomes its notes on GitHub, where a line break shows. -->

What changed in each release of the hatcheck CLI: `hatcheck`, `hat`, `cl`, `cx`, `claude-z` and `claude-k`. A release's notes on GitHub are its section here. The desktop app is released on its own, from `desktop-v*` tags.

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
