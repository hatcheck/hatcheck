# The Rust port

Status: done, 17 October 2026. hatcheck is Rust: every subcommand (`hatcheck sessions`, the plain
`hatcheck` list, `usage`, `where`, `move`, `open`, `show`, `desktop`, `best`, `bind`, `unbind`,
`provider` and `serve`) and the launchers `cl`, `cx`, `claude-z` and `claude-k`. The Python version,
`hatcheck/` and `hatcheck.py` with the `sh` and PowerShell launchers, is gone; 0.1.0 on PyPI was it.
The desktop app's self-test passes with the Rust binary as its engine: its first call takes 51 ms
where the bundled Python engine took 649 ms, from a 2.8 MB binary instead of 8.1 MB.

## Why

- **One binary per platform, and no Python.** Users get the commands as executables, and the
  desktop app ships the 2.8 MB binary as its engine instead of an 8 MB PyInstaller one. That engine
  needed signing and notarizing on its own, cost about 0.3 s to unpack on every start, and was the
  kind of single-file bundle Windows Defender tends to flag.
- **Speed.** The session list reads hundreds of megabytes of transcripts. On the same profiles the
  Rust binary takes 0.12 s cold and 0.04 s warm, against Python's 0.57 s and 0.12 s.

## How it was done

Command by command, with the same output: the same text, the same JSON, the same exit codes and
the same argparse messages. Nothing about the formats changed, so the desktop app and scripts
didn't notice the switch. The shared terminal style in `out.rs` later added status symbols and
colour to human-facing output; the JSON and raw-output compatibility described here still holds.

- **The Python tests were the specification.** `tests/` ran each subcommand through the Python
  code or, with `HATCHECK_ENGINE` set, the Rust binary, and both had to pass. Now they run the
  binary only: `tests/helpers.py` builds it with cargo first. The tests that called Python
  functions directly became the crate's unit tests, such as the Windows and PowerShell quoting in
  `paths.rs`, which a `Shell` value lets any OS check.
- **Real data, side by side.** Before a subcommand counted as ported, its output was compared with
  Python's on real profiles, byte for byte, read-only, with `PATH=/bin` so the Keychain was never
  asked and `--cached` so nothing went to the network: a year of session lists (550 sessions, 295
  of them Codex's), usage, and `where`, `open --print` and `show` for 24 sessions of both agents;
  `best`, `bind` and `provider` on real accounts, bindings and provider settings; the help and
  errors of every command at several terminal widths; and Claude Desktop's real icon recoloured at
  four settings, every pixel the same. `move` was compared with `--dry-run` only, in text and as
  JSON, on 91 real sessions: 26 copies between profiles, 30 Codex sessions converted for Claude Code
  and 23 Claude Code sessions for Codex (six of them long enough to leave turns out), and the
  refusals of open and empty sessions.
- **Moves for real, in fake homes.** Every command the `move` tests run, with `--dry-run` and
  `--dry-run --json` runs of it beside it, was run by both versions in fake homes built at the same
  paths: the same output, and the same files left, with the same bytes, permissions and times.
- **The launchers** gave what Python's gave, byte for byte, in 31 cases: with a profile, a binding
  or a provider, without one, without `claude` or `codex` on the PATH, and `reconfig`.
- **Python's behaviour, kept on purpose.** `crates/hatcheck/src/py.rs` reproduces what the output
  depends on: ints and floats kept apart (63 and 63.0 print differently), truthiness, `round`
  rounding half to even, `%d`, `%g`, a float's repr and `json.dumps(indent=2)`. `times.rs` reads
  stamps the way `datetime.fromisoformat` does; `ospath.rs` is `os.path` and the narrow `glob` the
  code uses; `args.rs` is argparse's parsing, usage line, help layout and errors (Python 3.14's
  rules for negative numbers, combined short flags, exclusive groups and subcommands included,
  and textwrap's wrapping); `widths.rs` is generated from Python's `unicodedata`, so titles are
  cut where Python cut them. `desktop.rs` recolours icons with Pillow's own RGB and HSV
  arithmetic, which gives Pillow's result for all 16.7 million inputs each way, without Pillow.

## Layout

| File | What it is | Ported from |
|---|---|---|
| `crates/hatcheck/src/cli.rs` | the dispatcher | `hatcheck/cli.py` |
| `sessions.rs` | the session list | `sessions.py`'s main |
| `facts.rs` | reading transcripts and Codex session files, and their cache | `sessions.py` |
| `usage.rs` | accounts, live and cached usage, bars, the Fable order | `usage.py` |
| `where_cmd.rs` | finding a session's files | `where.py` |
| `open_cmd.rs`, `show.rs` | the session picker and the terminals; a session's conversation | `open.py`, `show.py` |
| `find.rs` | a session's copies, the newest one in a folder, and `place` (`--from`, `--login`) | `sessions.py`, `move.py` |
| `move_cmd.rs` | moves and copies, updates, forks (`hatcheck fork`) and conversions, and the copy chooser | `move.py` |
| `agent.rs` | the agents, Claude Code, Codex, Antigravity CLI, Kimi Code CLI, ZCode, DeepSeek Harness, MiniMax Code and Qwen Code, as one enum the commands match on | new in the Rust version |
| `doctor.rs` | `hatcheck doctor`: every copy of hatcheck and the agents on the PATH, and the hooks | new in the Rust version |
| `agy.rs` | Antigravity CLI: its conversation index and transcripts, its lock, its /usage run headless, and a conversation written for it to read | new in the Rust version |
| `dsh.rs` | DeepSeek Harness: its homes, current v4 logs and conversation surface, and its POSIX session lock | new in the Rust version |
| `mcode.rs` | MiniMax Code: its data homes, the read-only SQLite catalog's v3 rows, active canonical history and expiring writer leases | new in the Rust version |
| `qwen.rs` | Qwen Code: its homes, each project's transcripts, and the registry and sidecar that say which sessions are open | new in the Rust version |
| `relocate.rs` | `hatcheck relocate`: a moved project folder's sessions brought along | new in the Rust version |
| `conv.rs` | a session as a conversation, for `show` and `move` | `move.py` |
| `launch.rs`, `src/bin/` | `cl`, `cx`, `ck`, `claude-z` and `claude-k`, each also a binary of its own | `launch.py`, `cl`, `cx` |
| `desktop.rs` | Claude Desktop instances, their launchers and recoloured icons | `desktop.py` |
| `serve.rs` | `hatcheck serve`, the desktop app's line-by-line JSON protocol | `cli.py` |
| `best.rs`, `bind.rs`, `provider.rs` | `best`; `bind`, `unbind` and the hook; `provider` | `best.py`, `bind.py`, `provider.py` |
| `paths.rs` | profiles and Codex, Kimi, DSH, mcode and qwen homes; quoting, resume and DSH web commands; folder bindings | `sessions.py` |
| `args.rs`, `py.rs`, `times.rs`, `ospath.rs`, `widths.rs`, `out.rs` | the Python behaviour above | |

The workspace is the repo's `Cargo.toml`. The desktop app (`desktop/src-tauri`) stays out of it: it
runs the binary as its engine, a sidecar running `hatcheck serve`, rather than linking the crate,
because `serve` sets environment variables for each command, which a process with other threads
can't do safely, and a command can end its process, which only the engine should do.

## Not the same as the Python version, on purpose

- **The cache file.** The session cache is `session-facts.json`; Python's `sessions.json`, in the
  same folder, isn't read.
- **`hatcheck --help`** leaves out the paragraph about the Python files.
- **The warning hook** runs `hatcheck` from your PATH, or else the binary itself.
- **A recolour that fails** says the icon couldn't be recoloured, and never asks for Pillow.
- **Ctrl+C at a `move` prompt on Windows** ends hatcheck there and then. Elsewhere it's taken as
  Python took it: no copy at "Which one?", and no to "Resume it now?".
- **A Codex lock file that can't be opened** stops the move with the error, where Python stopped
  with a traceback.
- **`hatcheck --as cl`** (or `cx`, `ck`, `claude-z`, `claude-k`), and the binary run through a link of
  one of those names, is that launcher; Python's `--as` changed only the name in messages.
  The launchers' own binaries are the same code without the rest, about 400 KB each.
- **Windows' change time** is in the session cache's stamp, so a session file rewritten at the
  same size with its times put back is read again; Python before 3.12 didn't see that.

## Shipping it

- **PyPI.** The package is built with maturin (`bindings = "bin"`), the way ruff and uv ship, so
  `uv tool install hatcheck` and `pipx install hatcheck` install the binaries. `release.yml`
  builds a wheel for each of five platforms and the sdist, and publishes them through PyPI's
  trusted publisher, which names that file.
- **The release binaries.** cargo-dist (`dist-workspace.toml`) writes `v-release.yml`, which builds
  the seven commands for six targets into the GitHub release, with shell and PowerShell installers
  and a Homebrew formula it publishes to `hatcheck/homebrew-tap`. The CLI's winget manifests
  (`packaging/winget/cli/`) install from that release's Windows zip. `install.sh` and
  `install.ps1` build the binaries from a clone with cargo and link them.
- **crates.io.** The crate is ready for it, with a README of its own and metadata that points
  `cargo binstall` at the release's archives. Its first version is published by hand, with an API
  token; after that, `crates.yml` publishes each release through crates.io's trusted publishing,
  set up on the crate's settings page for this repo, that file and the `crates-io` environment.
- **The desktop app** builds its engine with `npm run engine`, which runs cargo.

A release needs PyPI's trusted publisher to name this repo, `release.yml` and the `pypi`
environment, and a `HOMEBREW_TAP_TOKEN` secret that can write to `hatcheck/homebrew-tap`, for the
formula. Its version is the one in `Cargo.toml`, which the CLI's winget manifests follow. The
steps are in [maintenance.md](maintenance.md#releasing-the-cli).

## Running the checks

```sh
cargo test -p hatcheck                      # the crate's unit tests
python3 -m unittest discover -s tests       # the command tests, against target/debug/hatcheck
python3 -m unittest test_usage              # or some of them, from tests/
HATCHECK_ENGINE=path/to/hatcheck python3 -m unittest discover -s tests   # another build
```

CI runs both on Linux, macOS and Windows (`.github/workflows/tests.yml`), with `cargo fmt --check`
and `clippy -D warnings`.
