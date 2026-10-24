# Notes for coding agents

Hatcheck keeps Claude Code, Codex, Antigravity CLI, Kimi Code CLI and ZCode accounts on one
computer apart: `crates/hatcheck` is the whole CLI (and the engine the desktop app in `desktop/`
runs), `tests/` holds the command tests, and `docs/commands.md`, `docs/agents.md` and
`docs/install.md` document every command, agent and way of installing as it is. Read
[docs/lessons.md](docs/lessons.md) before changing code: what earlier work got wrong, and the rule
that would have caught it.

## Rules the code keeps

The README's [Rules it keeps](README.md#rules-it-keeps) section lists them with the code that
enforces each. In short: hatcheck is never in the path between an agent and its model; it never
asks for, stores, refreshes or moves a login; it never switches accounts on its own; nothing runs
in the background; nothing leaves the computer but the usage requests Claude Code, the Codex
and Kimi usage endpoints and agy's own `/usage` are asked for, and agy's model list, asked first
so a run never starts agy's sign-in. A change that bends one of these is a product decision, not
a fix: say so, and leave it to the maintainer.

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

The desktop app has its own checks, which CI runs: `npm run build` in `desktop/` (the types and
the pages) and `cargo check` in `desktop/src-tauri`. To try the app, `npm run demo` builds it and
runs it against a demo home made with `tests/helpers.py`, never against your profiles; the
commands are in [desktop/README.md](desktop/README.md#build-and-test-it).

## Conventions

- Output keeps the Python version's wording and layout, with the shared terminal style below;
  `py.rs`, `times.rs`, `args.rs` and `widths.rs` exist for that compatibility
  ([docs/rust-port.md](docs/rust-port.md)). New commands follow the same voice: plain sentences
  about what happened, paths shown with `~`, commands the user can paste.
- Human-facing output uses the semantic styles in `out.rs`: `✓` for a completed change, `◇` for a
  dry run and `⚠` for a warning, with bold headings, cyan commands, dim explanations and red
  errors; `🎩` only in the main help and the session list's heading. Colour needs a terminal and
  respects `NO_COLOR`; pipes, `hatcheck serve`, `--json` and raw output (`--print`, `where`,
  `bind --which`, `provider env`, `--version`) get no ANSI or added symbols.
- Every command documents itself in `docs/commands.md` (launchers and agents in `docs/agents.md`,
  installing in `docs/install.md`), in the section named after it, and the module's doc comment
  points there. A change in behaviour changes that section in the same change; the README keeps
  the short tour and the rules.
- Another app's files are read as that app writes them, checked against the real binary, and the
  version is written down. A success is read from its effect, never from an exit code alone.
- `--json` output is stable: absolute paths, UTC times, no colours; fields can be added at any
  time, and `version` goes up only when one changes meaning or goes away.
- `--dry-run` runs every check the real run does and writes nothing.
- Paths work on Windows too, where they have `\`, which JSON doubles, and either separator. A
  test compares a path with parsed values or `json.dumps(path)[1:-1]`, and expects this system's
  separator (`os.sep`, `ospath::SEP`); code that trims or splits a path someone typed takes `/` as
  well as `\` (`ospath::is_sep`). The tests run on windows-latest only in CI, and a failing unit
  test there hides the command tests behind it.
- A change users would notice gets a line in `CHANGELOG.md`, under `## Unreleased`, which the
  release renames to its version: that section becomes the release's notes on GitHub.
- Commits: conventional (`feat:`, `fix:`, `docs:`, `chore:`), no co-author lines, and the body
  says what was run.
