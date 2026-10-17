# The Rust port

Status: started 17 October 2026, and everything is ported: every subcommand (`hatcheck sessions`,
the plain `hatcheck` list, `usage`, `where`, `move`, `open`, `show`, `desktop`, `best`, `bind`,
`unbind`, `provider` and `serve`) and the launchers `cl`, `cx`, `claude-z` and `claude-k`. What's
left is the switch below: packaging the binary, and taking the Python code out.
The desktop app's self-test passes with the Rust binary as its engine: its first call takes 51 ms
where the bundled Python engine takes 649 ms, from a 2.8 MB binary instead of 8.1 MB.

## Why

- **One binary per platform, and no Python.** Users get `hatcheck` and `hat` as a single executable,
  and the desktop app ships the 2.8 MB binary as its engine instead of an 8 MB PyInstaller one.
  That engine needs signing and notarizing on its own, costs about 0.3 s to unpack on every start,
  and is the kind of single-file bundle Windows Defender tends to flag.
- **Speed.** The session list reads hundreds of megabytes of transcripts. On the same profiles the
  Rust binary takes 0.12 s cold and 0.04 s warm, against Python's 0.57 s and 0.12 s.

The desktop app's first release waits until the port is done, so it never ships the Python engine.

## How it's done

Command by command, with the same output: the same text, the same JSON, the same exit codes and
the same argparse messages. Nothing about the formats changes, so the desktop app, scripts and the
Python version can be swapped for the Rust one at any point.

- **The Python tests are the specification.** `tests/helpers.py` runs the subcommands through
  `HATCHECK_ENGINE` when it's set, and `python3 tests/conformance.py` builds the Rust binary and
  runs the tests of the ported subcommands against it. Tests of the Python code itself are marked
  `python_only` and skipped there. The Rust crate has unit tests of its own for what those covered.
- **Real data, side by side.** Before a subcommand counts as ported, its output is compared with
  Python's on real profiles, byte for byte, read-only, with `PATH=/bin` so the Keychain is never
  asked and `--cached` so nothing goes to the network. Everything ported so far matches: a year of
  session lists (550 sessions, 295 of them Codex's), usage, and `where`, `open --print` and `show`
  for 24 sessions of both agents; `best`, `bind` and `provider` on real accounts, bindings and
  provider settings; the help and errors of every ported command at several terminal widths;
  and Claude Desktop's real icon recoloured at four settings, every pixel the same. `move` was
  compared with `--dry-run` only, in text and as JSON, on 91 real sessions: 26 copies between
  profiles, 30 Codex sessions converted for Claude Code and 23 Claude Code sessions for Codex
  (six of them long enough to leave turns out), and the refusals of open and empty sessions.
- **Moves for real, in fake homes.** Every command the `move` tests run, with `--dry-run` and
  `--dry-run --json` runs of it beside it, was run by both versions in fake homes built at the same
  paths: the same output, and the same files left, with the same bytes, permissions and times.
- **The launchers** give what `hatcheck/launch.py` gives, byte for byte, in 31 cases: with a
  profile, a binding or a provider, without one, without `claude` or `codex` on the PATH, and
  `reconfig`. The `cl` and `cx` scripts' tests pass with the Rust binary answering their
  `hatcheck bind --which` and `hatcheck provider` calls.
- **Python's behaviour, kept on purpose.** `crates/hatcheck/src/py.rs` reproduces what the output
  depends on: ints and floats kept apart (63 and 63.0 print differently), truthiness, `round`
  rounding half to even, `%d`, `%g`, a float's repr and `json.dumps(indent=2)`. `times.rs` reads
  stamps the way `datetime.fromisoformat` does; `ospath.rs` is `os.path` and the narrow `glob` the
  code uses; `args.rs` is argparse's parsing, usage line, help layout and errors (Python 3.14's
  rules for negative numbers, combined short flags, exclusive groups and subcommands included,
  and textwrap's wrapping); `widths.rs` is generated from Python's `unicodedata`, so titles are
  cut where Python cut them. `desktop.rs` recolours icons with Pillow's own RGB and HSV
  arithmetic, which gives Pillow's result for all 16.7 million inputs each way, so it needs no
  Pillow.

## Layout

| File | What it is |
|---|---|
| `crates/hatcheck/src/cli.rs` | the dispatcher, as `hatcheck/cli.py` |
| `sessions.rs` | the session list, as `sessions.py`'s main |
| `facts.rs` | reading transcripts and Codex session files, and their cache |
| `usage.rs` | `usage.py`: accounts, live and cached usage, bars, the Fable order |
| `where_cmd.rs` | `where.py` |
| `open_cmd.rs`, `show.rs` | `open.py` (the session picker and the terminals) and `show.py` |
| `find.rs` | finding a session's copies, the newest one in a folder, and `place` (`--from`, `--login`) |
| `move_cmd.rs` | `move.py`: copies, updates, forks and conversions, and the copy chooser |
| `launch.rs`, `src/bin/` | `launch.py`: `cl`, `cx`, `claude-z` and `claude-k`, each also a binary of its own |
| `conv.rs` | a session as a conversation: `read_claude` and `read_codex` from `move.py` |
| `desktop.rs` | `desktop.py`: Claude Desktop instances, their launchers and recoloured icons |
| `serve.rs` | `hatcheck serve`, the desktop app's line-by-line JSON protocol, as in `cli.py` |
| `best.rs`, `bind.rs`, `provider.rs` | `best.py`, `bind.py` (with `unbind` and the hook), `provider.py` |
| `paths.rs` | profiles, Codex homes, quoting, the resume commands, folder bindings |
| `args.rs`, `py.rs`, `times.rs`, `ospath.rs`, `widths.rs`, `out.rs` | the Python behaviour above |

The workspace is the repo's `Cargo.toml`; the desktop app (`desktop/src-tauri`) stays out of it
until it links the crate.

## By design, not the same

- **The cache file.** The Rust port keeps its session cache in `session-facts.json`, beside
  Python's `sessions.json` in the same folder, so neither reads the other's entries.
- **`hatcheck --help`** leaves out the paragraph about the Python files.
- **The warning hook** runs `hatcheck` from your PATH, as Python's does, or else the Rust binary
  itself, where Python's runs its interpreter and `hatcheck.py`.
- **A recolour that fails** says the icon couldn't be recoloured; Python's says so too now, and
  still asks for Pillow when that's what's missing, which the Rust port never needs.
- **Ctrl+C at a `move` prompt on Windows** ends hatcheck there and then. Elsewhere it's taken as
  Python takes it: no copy at "Which one?", and no to "Resume it now?".
- **A Codex lock file that can't be opened** stops the move with the error, where Python's
  stopped with a traceback.
- **`hatcheck --as cl`** (or `cx`, `claude-z`, `claude-k`), and the binary run through a link of
  one of those names, is that launcher; Python's `--as` changes only the name in messages. The
  launchers' own binaries are the same code without the rest, about 400 KB each.

## The switch

- **PyPI.** The package is built with maturin (`bindings = "bin"`), the way ruff and uv ship, so
  `uv tool install hatcheck` and `pipx install hatcheck` install the binary.
- **Homebrew and winget** install the binaries, the launchers' included, and the `cl` and `cx`
  scripts go; `install.sh` and `install.ps1` link the binaries.
- **The desktop app** runs the Rust binary as its engine, still a sidecar running `hatcheck serve`:
  `npm run engine` builds it with cargo. Done. It stays a separate process rather than linking the
  crate, because `serve` sets environment variables for each command, which a process with other
  threads can't do safely, and a command can end its process, which only the engine should do.
- **The Python code goes**, with `tests/` kept as the conformance suite until the Rust crate's own
  tests cover the same ground.

## Running the checks

```sh
cargo test -p hatcheck                  # the crate's unit tests
python3 tests/conformance.py            # the ported subcommands' Python tests, against the binary
python3 tests/conformance.py test_usage # or some of them
```

CI runs both on Linux, macOS and Windows (the `rust` job in `.github/workflows/tests.yml`), with
`cargo fmt --check` and `clippy -D warnings`.
