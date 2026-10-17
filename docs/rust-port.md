# The Rust port

Status: started 17 October 2026. `hatcheck sessions` (the plain `hatcheck` list), `usage`, `where`,
`open`, `show` and `desktop` are ported; the rest of the CLI is still Python, in `hatcheck/`.

## Why

- **One binary per platform, and no Python.** Users get `hatcheck` and `hat` as a single executable,
  and the desktop app links the code instead of shipping a 8 MB PyInstaller engine beside itself.
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
  for 24 sessions of both agents; the help and errors of every ported command at five terminal
  widths; and Claude Desktop's real icon recoloured at four settings, every pixel the same.
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
| `conv.rs` | a session as a conversation: `read_claude` and `read_codex` from `move.py` |
| `desktop.rs` | `desktop.py`: Claude Desktop instances, their launchers and recoloured icons |
| `paths.rs` | profiles, Codex homes, quoting, the resume commands, folder bindings |
| `args.rs`, `py.rs`, `times.rs`, `ospath.rs`, `widths.rs`, `out.rs` | the Python behaviour above |

The workspace is the repo's `Cargo.toml`; the desktop app (`desktop/src-tauri`) stays out of it
until it links the crate.

## By design, not the same

- **The cache file.** The Rust port keeps its session cache in `session-facts.json`, beside
  Python's `sessions.json` in the same folder, so neither reads the other's entries.
- **`hatcheck --help`** leaves out the paragraph about the Python files.
- **A recolour that fails** says the icon couldn't be recoloured; Python's says so too now, and
  still asks for Pillow when that's what's missing, which the Rust port never needs.

## Still to port

In the order that suits the desktop app, which calls them:

1. `serve`, the protocol the app speaks to its engine today, so the app can run the Rust binary
   before it links the crate.
2. `best`, `bind` and `unbind` (with the session-start hook), `provider`.
3. `move`, the largest: copies, fast-forwards and forks between profiles, and conversion between
   Claude Code and Codex.
4. The launchers `cl`, `cx`, `claude-z` and `claude-k`, now shell and PowerShell scripts, and
   Python for the PyPI package.

Then the switch:

- **PyPI.** The package is built with maturin (`bindings = "bin"`), the way ruff and uv ship, so
  `uv tool install hatcheck` and `pipx install hatcheck` install the binary.
- **Homebrew and winget** install the binary; `install.sh` and `install.ps1` link it.
- **The desktop app** depends on the crate and calls it directly; `scripts/build-engine.py` and the
  sidecar go.
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
