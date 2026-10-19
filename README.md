<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://raw.githubusercontent.com/hatcheck/hatcheck/main/site/logo-wordmark-dark.svg">
    <img src="https://raw.githubusercontent.com/hatcheck/hatcheck/main/site/logo-wordmark.svg" alt="Hatcheck" width="360">
  </picture>
</p>

# Hatcheck

Several Claude accounts on one computer, each kept apart: isolated Claude Desktop instances on
macOS, and Claude Code CLI profiles, with recent sessions, plan usage and resume commands across all
of them. OpenAI's Codex CLI gets the same treatment, Google's Antigravity CLI (`agy`) is read
beside them, and sessions can be carried between the three.

*Unofficial: not made by, affiliated with or endorsed by Anthropic, OpenAI or Google. Claude, Claude
Code and Claude Desktop are trademarks of Anthropic; Codex and ChatGPT are trademarks of OpenAI;
Antigravity and Gemini are trademarks of Google LLC.*

Run **two (or more) fully isolated Claude Desktop instances** side by side on macOS — e.g. one
signed into your **personal** account and one into your **work** account — each with its own
**color-coded Dock launcher** that **focuses the existing window instead of spawning duplicates**.

This builds on Philipp Stracker's excellent
[`--user-data-dir` approach](https://gist.github.com/stracker-phil/9f84927a556632c7f9cc06663b534f14)
([write-up](https://philippstracker.com/multiple-claude-instances/)) and adds two things:

1. **Recolored icons** so you can tell the launchers apart at a glance (blue work / red personal / etc.).
2. A **smart, self-contained launcher** that fixes the "every click opens another window" problem.

It also has **command-line tools for Claude Code, [Codex](https://github.com/openai/codex) and
[Antigravity CLI](https://antigravity.google/docs/cli/)**, for keeping one profile per account:
`cl` and `cx` run Claude Code and Codex with a profile, and `hatcheck` lists the recent sessions
in all of them, with the command that resumes each one and what's left of each plan. The rest
of the commands find, show, carry and convert sessions between the accounts and the agents, tie
folders to accounts, and check how everything is installed: see
[the tour](#claude-code-codex-and-antigravity-cli) below, and
[docs/commands.md](docs/commands.md) for each command as it is.

**Not a gateway.** Hatcheck is never between an agent and its model. It rewrites no agent's
config, holds no login, and runs nothing in the background. Tools such as
[magpie](https://github.com/yetone/magpie) put a local gateway in that path so any agent can use
any model and accounts can take over from each other when a quota runs out. If that is what you
want, use one of those. Hatcheck is for the opposite case: keeping a work account and a personal
account apart, and knowing which one each session ran on. See [Rules it keeps](#rules-it-keeps).

|                                 | macOS | Linux | Windows |
|---------------------------------|:-----:|:-----:|:-------:|
| Claude Desktop instances        |  yes  | no official Desktop app | [planned](#roadmap) |
| Claude Code, Codex and Antigravity CLI tools |  yes  |  yes  |   yes   |

---

## Claude Desktop instances (macOS)

`hatcheck desktop` makes them: each instance gets its own data folder, `~/.claude-instances/<name>`,
and a launcher in `/Applications` with Claude's icon in a colour you pick. The launcher brings a
running instance's window forward instead of starting a second one on the same folder.

```bash
hatcheck desktop new work Work --hue 190          # Claude Work.app, blue
hatcheck desktop new personal Personal --hue -10  # Claude Personal.app, red-orange
hatcheck desktop open work                        # or open it from Launchpad or the Dock
hatcheck desktop                                  # the instances, and which are running
hatcheck desktop remove work                      # its launcher; --data deletes its data folder too
```

Sign each one in to its account the first time it opens. The first time a launcher brings a window
forward, macOS asks whether it may control System Events; allow it, once per launcher. `--hue`
turns the icon's colours by that many degrees (about 110 is green, 190 blue, 240 purple) and
`--saturation` scales them. hatcheck recolours the icon itself, with macOS's `iconutil`, and a
launcher it can't recolour keeps Claude's own icon. The name `default` makes a launcher for
Claude's built-in data folder, which is only safe while nothing else runs on that folder, the stock
Claude icon included. [docs/desktop-instances.md](docs/desktop-instances.md) has the rest: what
the launcher does, and how to copy a login you already have into an instance.

---

## Claude Code, Codex and Antigravity CLI

Claude Code (the `claude` CLI) keeps its settings, history and session transcripts in `~/.claude`
(plus `~/.claude.json`), and its login in the macOS Keychain, or on Linux and Windows in
`~/.claude/.credentials.json`. Point `CLAUDE_CONFIG_DIR` at another folder and you get a separate
profile with its own login, so you can keep one per account:
`~/.claude-work`, `~/.claude-personal`, and so on. These are separate from the Desktop instances
above, though sessions from Desktop's Code tab can be filed in them too (the session list
hides those unless you pass `--all`). Codex works the same way with `CODEX_HOME`: see
[`cx`](docs/agents.md#cx-run-codex-with-a-home-per-account). Antigravity CLI keeps everything under
`~/.gemini/antigravity-cli`, which only `HOME` moves, so hatcheck reads that one folder: see
[Antigravity CLI](docs/agents.md#antigravity-cli-agy).

### Install

One binary per platform, with no Python needed. Each of these installs `hatcheck`, `hat`, `cl`,
`cx`, `claude-z` and `claude-k`:

```bash
curl -LsSf https://github.com/hatcheck/hatcheck/releases/latest/download/hatcheck-installer.sh | sh
brew install hatcheck/tap/hatcheck  # Homebrew, on macOS and Linux
uv tool install hatcheck            # or: pipx install hatcheck
```

On Windows, in PowerShell:

```powershell
powershell -ExecutionPolicy Bypass -c "irm https://github.com/hatcheck/hatcheck/releases/latest/download/hatcheck-installer.ps1 | iex"
```

The installer scripts put the commands in `~/.local/bin` and add that folder to your `PATH`; set
`HATCHECK_NO_MODIFY_PATH=1` first to keep your shell's startup files as they are. Each release, made
from a tag such as `v0.2.0`, has them and the binaries for each platform, and PyPI gets the same
binaries as wheels. The release builds start after 0.1.0, which is the Python version.

From a clone, on Windows, switching between installs or removing one, and `hatcheck doctor`,
which lists every copy on your `PATH`: [docs/install.md](docs/install.md).

### The commands

Each is documented as it is, with its options and what it prints, in
[docs/commands.md](docs/commands.md); the launchers and the agents' folders in
[docs/agents.md](docs/agents.md).

| Command | What it does |
|---|---|
| [`cl work --resume <id>`](docs/agents.md#cl-run-claude-code-with-a-profile) | Claude Code with `~/.claude-work`; `cl -z` and `cl -k` run it on Z.ai or Kimi there |
| [`cx work resume <id>`](docs/agents.md#cx-run-codex-with-a-home-per-account) | Codex with `~/.codex-work` |
| [`hatcheck`](docs/commands.md#hatcheck-what-to-resume) | the sessions active in the last day, in every profile, home and folder, each with its resume command and what's left of its plan |
| [`hatcheck usage`](docs/commands.md#hatcheck-usage-plan-usage-for-every-profile) | what's left of every plan limit, sorted by Fable |
| [`hatcheck best`](docs/commands.md#hatcheck-best-which-account-to-use-now) | which account to use now, or to carry a session to; `--wait` sleeps until one has room |
| [`hatcheck where SESSION`](docs/commands.md#hatcheck-where-find-a-session) | where a session's files are |
| [`hatcheck open SESSION`](docs/commands.md#hatcheck-open-resume-a-session-in-a-new-terminal) | resume it in a new terminal, in its folder |
| `hatcheck show SESSION` | its conversation, read-only |
| [`hatcheck move SESSION PROFILE`](docs/commands.md#hatcheck-move-carry-on-a-session-with-another-profile) | carry it to another profile, or convert it for another agent |
| [`hatcheck fork SESSION PROFILE`](docs/commands.md#hatcheck-fork-a-session-as-a-new-one-in-another-profile) | bring it in there as a new session, keeping the original |
| [`hatcheck relocate OLD NEW`](docs/commands.md#hatcheck-relocate-a-project-folder-that-moved) | a moved project folder's sessions, brought along |
| [`hatcheck bind PROFILE`](docs/commands.md#hatcheck-bind-tie-a-folder-to-a-profile) | tie a folder to a profile, so `cl` and `cx` pick it there and the list flags strays |
| [`hatcheck doctor`](docs/install.md#hatcheck-doctor-every-copy-of-hatcheck-and-the-agents) | every copy of hatcheck and the agents on your `PATH`, which runs, how to remove the rest |
| [`hatcheck desktop`](#claude-desktop-instances-macos) | Claude Desktop instances on macOS |
| [`--json`](docs/commands.md#--json-for-scripts-status-bars-and-apps) | the sessions, plans and results as data, for scripts, status bars and the desktop app |

`hat` is the short name, and a subcommand can be cut to its first letters: `hat m` is `hat move`.
[Antigravity CLI](docs/agents.md#antigravity-cli-agy) sessions are listed, shown and converted
from its one folder, with what its `/usage` reports; [`claude-z` and `claude-k`](docs/agents.md#claude-z-claude-k-claude-code-on-zai-or-kimi)
run Claude Code on Z.ai's or Kimi's models in any profile.


## Rules it keeps

Each rule names the code that keeps it, so it can be checked rather than taken on trust.

- **Not in the path.** Nothing of Hatcheck sits between an agent and its model. `cl` and `cx`
  set `CLAUDE_CONFIG_DIR` or `CODEX_HOME` (and, for `cl -z` and `cl -k`, the provider variables
  you set up) and run the agent in their place; nothing stays running
  ([`launch.rs`](crates/hatcheck/src/launch.rs), `run`).
- **No agent's config is rewritten**, except when you ask: `hatcheck bind --hook` adds one
  session-start hook to each profile's `settings.json`, each Codex home's `hooks.json` and
  Antigravity CLI's `~/.gemini/config/hooks.json`, or points the one it added at the `hatcheck` it
  finds now, and `unbind --hook` removes only what it added ([`bind.rs`](crates/hatcheck/src/bind.rs),
  `add_hooks` and `remove_hooks`).
- **No credentials.** It never asks for, stores, refreshes or moves a login. For Claude it
  never reads one: Claude Code's own `/usage` is run in the profile and the snapshot it saves is
  read ([`usage.rs`](crates/hatcheck/src/usage.rs), `run_claude_usage` and `claude_live`). For
  Codex, the home's `auth.json` token is sent only to ChatGPT's usage endpoint, as it is, and
  never refreshed (`read_codex_login` and `fetch_codex_usage` there). For agy it never reads one
  either: agy's own `/usage` is run, after `agy models` has said agy is signed in, since asked
  without a sign-in agy would start one ([`agy.rs`](crates/hatcheck/src/agy.rs), `run_usage` and
  `signed_in`).
- **Nothing leaves your computer** except those usage requests: Claude Code's to Anthropic, one
  to ChatGPT per Codex account, and agy's own `/usage` to Google, with the model list agy fetches
  first to say it's signed in. `CODEX_USAGE_URL` in `usage.rs` is the one address the
  binary itself calls; `--cached` and `--no-usage` make none. The Z.ai and Kimi endpoints in
  [`provider.rs`](crates/hatcheck/src/provider.rs) are handed to Claude Code, which calls them
  only when you run `cl -z` or `cl -k`.
- **No automatic switching** between accounts. `hatcheck best` ranks and prints commands, and
  nothing in the binary starts, moves or switches a session on its own
  ([`best.rs`](crates/hatcheck/src/best.rs): advice only, `--wait` sleeps and then prints).
- **Nothing runs in the background.** There's no daemon. `hatcheck serve` is the desktop app's
  engine and lives only while the app does, one command per line
  ([`serve.rs`](crates/hatcheck/src/serve.rs)).
- **Sessions are only ever moved, copied or rewritten in ways you can undo.** `move` deletes the
  original only once every copied file is checked against it, never puts an older copy over a
  newer one, and is undone by moving the session back; `fork` keeps the original; `relocate`
  prints the command that undoes it; and all three have `--dry-run`, which runs every check and
  writes nothing ([`move_cmd.rs`](crates/hatcheck/src/move_cmd.rs),
  [`relocate.rs`](crates/hatcheck/src/relocate.rs)).

Function names rather than line numbers, since lines move; `git grep` finds them.

## The desktop app

A menu-bar app (system tray on Windows and Linux) built on the CLI. It shows what's left of every
plan, sorted by Fable, with bars and reset times, and the last day's sessions with **Resume**,
which opens a terminal in the session's folder, and **Copy**. It also has buttons for the Claude
Desktop instances and notifies you when a limit runs low or resets.

Its main window browses every session, with search, filters and a read-only preview of the
conversation. From there you can resume a session, copy its command, or move or fork it to
another profile after a dry run. The window also lists profiles and homes, signs in to a new one
in a terminal, and makes Desktop instances.

It ships the CLI inside itself, so it needs no Python. It never handles a login, and live usage is
off until you turn it on. See [desktop/README.md](desktop/README.md) to build and test it, with
`npm run demo` running it against a demo home rather than your profiles; releases come from
`.github/workflows/desktop.yml` for macOS, Windows and Linux. The plan behind it is
[the proposal](docs/proposals/desktop-app.md).

## Files in this repo

- **`crates/hatcheck/`**: hatcheck in Rust, the commands and the launchers. Its `src/` has a file per
  subcommand (`sessions.rs`, `usage.rs`, `move_cmd.rs`, `open_cmd.rs`, `desktop.rs` and the rest),
  `launch.rs` for `cl`, `cx`, `claude-z` and `claude-k`, `agy.rs` for what it reads of Antigravity
  CLI, `doctor.rs` for `hatcheck doctor`, and `src/bin/` for those binaries and `hat`.
  Its own README is the crate's page on crates.io. It was ported from a Python version with the
  same output; see [docs/rust-port.md](docs/rust-port.md).
- **`CHANGELOG.md`**: what changed in each release of the CLI; a release's notes on GitHub are its
  section there.
- **`install.sh`** / **`install.ps1`**: build the commands from a clone and put them on your `PATH`
  (`~/.local/bin` by default).
- **`pyproject.toml`**: the PyPI package, the binaries as wheels built with maturin.
  **`dist-workspace.toml`**: the release builds, installers and Homebrew formula, made by cargo-dist.
  **`packaging/`**: the app's Homebrew cask, and the winget manifests for the app and the CLI.
- **`desktop/`**: the desktop app: Tauri, with the CLI inside it ([its README](desktop/README.md)).
- **`site/`**: the landing page, one self-contained file, served at [hatcheck.dev](https://hatcheck.dev)
  from Cloudflare as static assets; `site/deploy.sh` ships it. The logo lives there too:
  `logo.svg` and `logo-dark.svg` (the mark), `logo-wordmark.svg` and `logo-wordmark-dark.svg`, `favicon.svg`.
- **`docs/proposals/`**: the plans these came from: [the CLI features](docs/proposals/cli-features.md)
  and [the desktop app](docs/proposals/desktop-app.md).
- **`docs/`**: [commands.md](docs/commands.md), [agents.md](docs/agents.md) and
  [install.md](docs/install.md), each command, agent and way of installing as it is;
  [desktop-instances.md](docs/desktop-instances.md) for the Desktop instances;
  [rust-port.md](docs/rust-port.md) on the port from Python.
- **`AGENTS.md`** and **`docs/lessons.md`**: notes for coding agents (`CLAUDE.md` imports them): how
  to build and check, the rules the code keeps, and what earlier work got wrong, with the rule that
  would have caught it.
- **`tests/`**: the command tests, which run the binary against throwaway home folders:
  `python3 -m unittest discover -s tests`, which builds it with cargo first, and the crate's own
  tests with `cargo test -p hatcheck`. They never read your real profiles, Keychain or the network.
  GitHub Actions runs both on macOS, Linux and Windows (`.github/workflows/tests.yml`).

## Roadmap

- **Claude Desktop instances on Windows.** Claude Desktop is an Electron app there too, so a
  separate `--user-data-dir` per account should work. A Windows version would create a shortcut per
  instance with its own icon, and a launcher that brings a running instance forward instead of
  opening a duplicate. It needs testing on a Windows machine first.
- **Linux.** There's no official Claude Desktop for Linux, so only the CLI tools apply there.
- **Antigravity CLI profiles.** agy keeps one login per home folder, so a second account needs a
  home folder of its own (`HOME=~/.agy-work agy`), which every command the agent runs would see
  too. Whether hatcheck should make such folders, and a launcher for them, is still open; until
  then it reads `~/.gemini/antigravity-cli` only. The Antigravity app has the same one login,
  in the system keyring, shared with agy, so an instance of it with its own data folder would
  still be the same account; nothing for hatcheck to make until Google separates logins.
- **Releases:** the CLI is on GitHub, the Homebrew tap, PyPI and crates.io from 0.2.0
  ([CHANGELOG.md](CHANGELOG.md)). Next are winget, then the app's installers, once its release
  workflow has its signing and updater secrets (`desktop.yml` lists them).
- **Signing:** a Developer ID for the macOS app, and a certificate for the Windows one.

## Credits

- **Claude Desktop instances:** this repo started as a fork of
  [Aaron Oxborrow](https://github.com/aoxborrow)'s
  [gist](https://gist.github.com/aoxborrow/5a3a0ac0aa37819cc19439b70a552221), whose scripts made
  colour-coded launchers for isolated instances. On 16 October 2026 his scripts and his sections of
  this README were replaced by a rewrite, `hatcheck desktop`, so none of his code or text remains.
  The idea both implement, running Claude Desktop with `--user-data-dir`, is from
  [Philipp Stracker's gist](https://gist.github.com/stracker-phil/9f84927a556632c7f9cc06663b534f14)
  ([write-up](https://philippstracker.com/multiple-claude-instances/)), itself a cleaned-up version
  of [weidwonder/claude-desktop-multi-instance](https://github.com/weidwonder/claude-desktop-multi-instance).
- **Claude Code and Codex CLI tools:** `hatcheck`, `cl`, `cx`, the installers and the tests are by
  [Yi Wang](https://github.com/yiwang).
- **Z.ai and Kimi:** the provider settings in `crates/hatcheck/src/provider.rs` (endpoints, model slots, context
  windows and the config files) come from [claude-z](https://github.com/sam-fakhreddine/claude-z) by
  Kerim ([lettt325](https://github.com/lettt325)), MIT-licensed, and Yi Wang's
  [fork](https://github.com/yiwang/claude-z) of it, which added Kimi.

## License

MIT, see [LICENSE](LICENSE). It covers everything in this repo: the CLI, the desktop app, the site
and the docs.
