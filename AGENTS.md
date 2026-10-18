# Notes for coding agents

Hatcheck keeps Claude Code and Codex accounts on one computer apart: `crates/hatcheck` is the
whole CLI (and the engine the desktop app in `desktop/` runs), `tests/` holds the command tests,
and the README documents every command as it is. Read [docs/lessons.md](docs/lessons.md) before
changing code: what earlier work got wrong, and the rule that would have caught it.

## Rules the code keeps

The README's [Rules it keeps](README.md#rules-it-keeps) section lists them with the code that
enforces each. In short: hatcheck is never in the path between an agent and its model; it never
asks for, stores, refreshes or moves a login; it never switches accounts on its own; nothing runs
in the background; nothing leaves the computer but the usage requests Claude Code and the Codex
usage endpoint are asked for. A change that bends one of these is a product decision, not a fix:
say so, and leave it to the maintainer.

## Building and checking

```sh
cargo fmt -p hatcheck && cargo clippy -p hatcheck --all-targets -- -D warnings
cargo test -p hatcheck                      # the crate's unit tests
python3 -m unittest discover -s tests       # the command tests, against the built binary
```

The command tests build `target/debug/hatcheck` and run it, and the launchers beside it, against
throwaway homes; `HATCHECK_ENGINE=path` names another binary. They never read your real profiles,
the Keychain or the network, and they must stay that way: a fake `claude` and expired Codex
tokens stand in (`tests/helpers.py`). Run the whole suite, not the file you touched: a change to
what a command prints breaks the test that pinned it elsewhere.

## Conventions

- Output is what the Python version printed, byte for byte, where a user or a test sees it:
  `py.rs`, `times.rs`, `args.rs` and `widths.rs` exist for that ([docs/rust-port.md](docs/rust-port.md)).
  New commands follow the same voice: plain sentences about what happened, paths shown with `~`,
  commands the user can paste.
- Every command documents itself in the README, in the section named after it, and the module's
  doc comment points there. A change in behaviour changes the README in the same change.
- Another app's files are read as that app writes them, checked against the real binary, and the
  version is written down. A success is read from its effect, never from an exit code alone.
- `--json` output is stable: absolute paths, UTC times, no colours; fields can be added at any
  time, and `version` goes up only when one changes meaning or goes away.
- `--dry-run` runs every check the real run does and writes nothing.
- Commits: conventional (`feat:`, `fix:`, `docs:`, `chore:`), no co-author lines, and the body
  says what was run.
