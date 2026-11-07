<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://raw.githubusercontent.com/hatcheck/hatcheck/main/site/logo-wordmark-dark.svg">
    <img src="https://raw.githubusercontent.com/hatcheck/hatcheck/main/site/logo-wordmark.svg" alt="Hatcheck" width="360">
  </picture>
</p>

# Hatcheck

Several Claude accounts on one computer, each kept apart: work and personal, a team seat and your
own plan. On macOS and Windows each gets its own Claude Desktop, with its own login and an icon in
its own colour; anywhere, each gets its own Claude Code profile, with recent sessions, plan usage
and resume commands across all of them in one list. OpenAI's Codex CLI and Moonshot's Kimi Code CLI
get the same treatment, Google's Antigravity CLI (`agy`) and Z.ai's ZCode are read beside them, and
DeepSeek Harness (`dsh`), MiniMax Code (`mcode`), Qwen Code (`qwen`), GitHub Copilot CLI (`copilot`)
and OpenCode (`opencode`) join as read-only agents. Sessions can be carried between all ten: as a
file the agent is started on for those without an importer, and through their own importers for
copilot and opencode.

Hatcheck never asks for, stores, copies or moves a login, is never between an agent and its model,
runs nothing in the background, and never switches accounts on its own: see
[Rules it keeps](#rules-it-keeps).

**Start here:**

- [Two Claude accounts on one Mac](https://hatcheck.dev/two-claude-accounts-on-one-mac): a Claude
  Desktop per account, in three commands.
- [Claude Code with two accounts](https://hatcheck.dev/claude-code-two-accounts): a profile per
  account, and one list of what's in all of them; the same for
  [Codex](https://hatcheck.dev/codex-two-accounts) and [Kimi Code CLI](https://hatcheck.dev/kimi-code-two-accounts),
  and [what's left of each plan](https://hatcheck.dev/claude-usage-limits).
- [Install the CLI](#install): one line on macOS, Linux or Windows. The menu-bar app is
  [on its way](#the-desktop-app).

*Unofficial: not made by, affiliated with or endorsed by Anthropic, OpenAI, Google, Moonshot AI,
Z.ai, DeepSeek, MiniMax, Alibaba Cloud, GitHub or the OpenCode project. Claude, Claude Code and
Claude Desktop are trademarks of Anthropic; Codex and ChatGPT are trademarks of OpenAI; Antigravity
and Gemini are trademarks of Google LLC; Kimi is Moonshot AI's, ZCode and GLM are Z.ai's, DeepSeek
Harness is DeepSeek's, MiniMax Code is MiniMax's, Qwen Code is Alibaba Cloud's, GitHub Copilot is
GitHub's and OpenCode is the OpenCode project's.*

On macOS and Windows, `hatcheck desktop` runs **one Claude Desktop per account, side by side**:
each with its own data folder and login, and a launcher in a colour you pick, which brings the
running one forward rather than opening it twice.

Running Claude Desktop with its own `--user-data-dir` is
[Philipp Stracker's approach](https://gist.github.com/stracker-phil/9f84927a556632c7f9cc06663b534f14)
([write-up](https://philippstracker.com/multiple-claude-instances/)); hatcheck adds the colours and
the launchers.

It also has **command-line tools for all ten agents**, for keeping one profile per account:
`cl`, `cx` and `ck` run Claude Code, Codex and Kimi Code CLI with a profile, and `hatcheck` lists
the recent sessions in all of them, with the command that resumes each one or opens DSH's session
history, and what's left of each plan. The rest
of the commands find, show, carry and convert sessions between the accounts and the agents, tie
folders to accounts, and check how everything is installed: see
[the tour](#claude-code-codex-antigravity-cli-kimi-code-cli-zcode-deepseek-harness-minimax-code-qwen-code-github-copilot-cli-and-opencode) below, and
[docs/commands.md](docs/commands.md) for each command as it is.

**Not a gateway.** Hatcheck is never between an agent and its model. It rewrites no agent's
config, holds no login, and runs nothing in the background. Tools such as
[magpie](https://github.com/yetone/magpie) put a local gateway in that path so any agent can use
any model and accounts can take over from each other when a quota runs out. If that is what you
want, use one of those. Hatcheck is for the opposite case: keeping several accounts apart, and
knowing which one each session ran on. See [Rules it keeps](#rules-it-keeps).

|                                 | macOS | Linux | Windows |
|---------------------------------|:-----:|:-----:|:-------:|
| Claude Desktop instances        |  yes  | [untried](#roadmap): Claude Desktop for Linux is in beta |   yes   |
| Claude Code, Codex, Antigravity CLI, Kimi Code CLI, ZCode, DeepSeek Harness, MiniMax Code, Qwen Code, GitHub Copilot CLI and OpenCode tools |  yes  |  yes  |   yes   |

---

## Claude Desktop instances (macOS and Windows)

`hatcheck desktop` makes them: each instance gets its own data folder, `~/.claude-instances/<name>`,
and a launcher with Claude's icon in a colour you pick: an app in `/Applications` on macOS, a
shortcut in the Start menu on Windows. The launcher brings a running instance's window forward
instead of starting a second one on the same folder.

```bash
hatcheck desktop new a A --hue 190                # Claude A.app, or Claude A in the Start menu: blue
hatcheck desktop new b B --hue -10                # Claude B, red-orange
hatcheck desktop open a                           # or open it from Launchpad, the Dock or the Start menu
hatcheck desktop                                  # the instances, and which are running
hatcheck desktop remove a                         # its launcher; --data deletes its data folder too
```

Sign each one in to its account the first time it opens. On macOS, the first time a launcher brings
a window forward, macOS asks whether it may control System Events; allow it, once per launcher. On
Windows, Claude Desktop (the Microsoft Store version) keeps one process per data folder itself, so
the launcher is a plain shortcut. `--hue`
turns the icon's colours by that many degrees (about 110 is green, 190 blue, 240 purple) and
`--saturation` scales them. hatcheck recolours the icon itself, with macOS's `iconutil` or, on
Windows, from the artwork in Claude's own package, and a launcher it can't recolour keeps Claude's
own icon. The name `default` makes a launcher for Claude's built-in data folder, which on macOS is
only safe while nothing else runs on that folder, the stock Claude icon included. [docs/desktop-instances.md](docs/desktop-instances.md) has the rest: what
the launcher does, and how to copy a login you already have into an instance.

---

## Claude Code, Codex, Antigravity CLI, Kimi Code CLI, ZCode, DeepSeek Harness, MiniMax Code, Qwen Code, GitHub Copilot CLI and OpenCode

Claude Code (the `claude` CLI) keeps its settings, history and session transcripts in `~/.claude`
(plus `~/.claude.json`), and its login in the macOS Keychain, or on Linux and Windows in
`~/.claude/.credentials.json`. Point `CLAUDE_CONFIG_DIR` at another folder and you get a separate
profile with its own login, so you can keep one per account: `~/.claude-a`, `~/.claude-b`, and so
on. These are separate from the Desktop instances above, though sessions from Desktop's Code tab
can be filed in them too (the session list hides those unless you pass `--all`). The other agents
keep theirs as the table says; [docs/agents.md](docs/agents.md) has each one's folders, what
hatcheck reads of them, and the version that was checked against.

| Agent | Its folder | A home per account | A session carried to it | Plan usage |
|---|---|---|---|---|
| Claude Code | `~/.claude` | `CLAUDE_CONFIG_DIR`, with [`cl`](docs/agents.md#cl-run-claude-code-with-a-profile) | written into the profile | Claude Code's own `/usage` |
| Codex | `~/.codex` | `CODEX_HOME`, with [`cx`](docs/agents.md#cx-run-codex-with-a-home-per-account) | written into the home | ChatGPT's usage endpoint |
| [Antigravity CLI](docs/agents.md#antigravity-cli-agy) | `~/.gemini/antigravity-cli` | none: only `HOME` moves it | a file `agy` is started on | agy's own `/usage` |
| [Kimi Code CLI](docs/agents.md#kimi-code-cli-kimi) | `~/.kimi-code` | `KIMI_CODE_HOME`, with [`ck`](docs/agents.md#ck-run-kimi-code-cli-with-a-home-per-account) | written into the home | Kimi's usage endpoint |
| [ZCode](docs/agents.md#zcode-zcode) | `~/.zcode`, a database | none: only `HOME` moves it | a file `zcode` is started on | none to read |
| [DeepSeek Harness](docs/agents.md#deepseek-harness-dsh) | `~/.dsh` | `DSH_HOME` | a prompt to paste into `dsh web` | none to read |
| [MiniMax Code](docs/agents.md#minimax-code-mcode) | `~/.minimax` | `MINIMAX_DATA_DIR`, or `MAVIS_DATA_DIR` | a file `mcode` is started on | not wired yet |
| [Qwen Code](docs/agents.md#qwen-code-qwen) | `~/.qwen` | `QWEN_HOME` | a file `qwen` is started on | none to read |
| [GitHub Copilot CLI](docs/agents.md#github-copilot-cli-copilot) | `~/.copilot` | `COPILOT_HOME` | `copilot sessions import` | none to read |
| [OpenCode](docs/agents.md#opencode-opencode) | `~/.local/share/opencode`, one database | not yet | `opencode session import` | its providers' own |

Every agent's sessions are listed and shown, and can be carried to any of the others, each in the
way the table's fourth column says. Claude Code's, Codex's and Kimi Code CLI's also move between
two homes of the same agent; ZCode's database and DeepSeek Harness's, MiniMax Code's, Qwen
Code's, Copilot's and OpenCode's stores are only ever read.

### Install

One binary per platform, with no Python needed. Each of these installs `hatcheck`, `hat`, `cl`,
`cx`, `ck`, `claude-z`, `claude-k`, `claude-m` and `claude-d`:

```bash
curl -LsSf https://github.com/hatcheck/hatcheck/releases/latest/download/hatcheck-installer.sh | sh
brew install hatcheck/tap/hatcheck  # Homebrew, on macOS and Linux
uv tool install hatcheck            # or: pipx install hatcheck
cargo binstall hatcheck             # the release binaries, with cargo-binstall
cargo install hatcheck              # or build them from crates.io
```

On Windows, in PowerShell:

```powershell
powershell -ExecutionPolicy Bypass -c "irm https://github.com/hatcheck/hatcheck/releases/latest/download/hatcheck-installer.ps1 | iex"
```

The installer scripts put the commands in `~/.local/bin` and add that folder to your `PATH`; set
`HATCHECK_NO_MODIFY_PATH=1` first to keep your shell's startup files as they are. Completions
for zsh, bash, fish and PowerShell come from
[`hatcheck completions`](docs/commands.md#hatcheck-completions-for-zsh-bash-fish-and-powershell):
`eval "$(hatcheck completions zsh)"` after `compinit`, and the others as that section says. Each release, made
from a tag such as `v0.2.0`, has them and the binaries for each platform, and PyPI gets the same
binaries as wheels. The release builds start after 0.1.0, which is the Python version.

From a clone, on Windows, switching between installs or removing one, and `hatcheck doctor`,
which lists every copy on your `PATH`: [docs/install.md](docs/install.md). hatcheck installs no
agent: [docs/agents.md](docs/agents.md#getting-the-agents) has each one's installer and app.

### The commands

Each is documented as it is, with its options and what it prints, in
[docs/commands.md](docs/commands.md); the launchers and the agents' folders in
[docs/agents.md](docs/agents.md).

| Command | What it does |
|---|---|
| [`cl a --resume <id>`](docs/agents.md#cl-run-claude-code-with-a-profile) | Claude Code with `~/.claude-a`; `cl -z`, `-k`, `-m` and `-D` run it on Z.ai, Kimi, MiniMax or DeepSeek there |
| [`cx a resume <id>`](docs/agents.md#cx-run-codex-with-a-home-per-account) | Codex with `~/.codex-a` |
| [`ck a --session <id>`](docs/agents.md#ck-run-kimi-code-cli-with-a-home-per-account) | Kimi Code CLI with `~/.kimi-code-a` |
| [`hatcheck`](docs/commands.md#hatcheck-what-to-resume) | the sessions active in the last day in this folder, or every folder with `-A`, each with its resume or open command, what it cost and what's left of its plan; `-d` adds each one's turns and spend by model, `-c` puts the costliest first, and [`config.toml`](docs/commands.md#configtoml-the-lists-defaults-and-what-to-leave-out) sets the defaults and the agents and folders every command leaves out |
| [`hatcheck usage`](docs/commands.md#hatcheck-usage-plan-usage-for-every-profile) | what's left of every plan limit, sorted by Fable, and, from hatcheck's own readings over time, how fast each is being used up and whether it runs out before it resets; DSH, Qwen Code, Copilot and OpenCode homes say why there's no reading, and mcode's quota isn't wired |
| [`hatcheck best`](docs/commands.md#hatcheck-best-which-account-to-use-now) | the best account now, with its limits, and the `move` command that carries the newest session started here to it; `--wait` sleeps until one has room |
| [`hatcheck where SESSION`](docs/commands.md#hatcheck-where-find-a-session) | where a session's files are |
| [`hatcheck open SESSION`](docs/commands.md#hatcheck-open-resume-a-session-in-a-new-terminal) | resume it in a new terminal, or open DSH's session history in its folder |
| [`hatcheck resume`](docs/commands.md#hatcheck-resume-carry-on-the-newest-session-here) | carry on the newest session started from this folder, or open DSH's session history, after asking; `hat r` for short |
| [`hatcheck show SESSION`](docs/commands.md#hatcheck-show-a-sessions-conversation) | its conversation, read-only; `--markdown` writes it out as a document to keep or share |
| [`hatcheck search WORDS`](docs/commands.md#hatcheck-search-the-sessions-that-mention-something) | the sessions whose title or conversation has every word, in every profile and home, newest first, with the turns the words are in |
| [`hatcheck move SESSION PROFILE`](docs/commands.md#hatcheck-move-carry-on-a-session-with-another-profile) | carry it to another profile, or convert it for another agent; `--brief` hands over a brief in place of the whole conversation |
| [`hatcheck fork SESSION PROFILE`](docs/commands.md#hatcheck-fork-a-session-as-a-new-one-in-another-profile) | bring it in there as a new session, keeping the original |
| [`hatcheck relocate OLD NEW`](docs/commands.md#hatcheck-relocate-a-project-folder-that-moved) | a moved project folder's sessions, brought along; DSH's, mcode's, qwen's, copilot's and opencode's stay as they are |
| [`hatcheck bind PROFILE`](docs/commands.md#hatcheck-bind-tie-a-folder-to-a-profile) | tie a folder to a profile, so `cl`, `cx` and `ck` pick it there and the list flags strays |
| [`hatcheck profiles [A B]`](docs/commands.md#hatcheck-profiles-what-each-profile-is-set-up-with) | what each profile and home is set up with: skills, agents, commands, plugins, hooks, MCP servers, permissions and settings; with two named, what differs between them |
| [`hatcheck statusline --install PROFILE`](docs/commands.md#hatcheck-statusline-which-account-a-session-is-on-in-claude-codes-status-line) | a status line for Claude Code with the profile the session runs in and what's left of its plan: `work · Opus 5.5 · 5h 62% ↻2h41m · wk 30% ↻3d2h · ctx 23% · $1.23` |
| [`hatcheck completions SHELL`](docs/commands.md#hatcheck-completions-for-zsh-bash-fish-and-powershell) | the completions for zsh, bash, fish or PowerShell: subcommands, options, profiles and recent sessions |
| [`hatcheck doctor`](docs/install.md#hatcheck-doctor-every-copy-of-hatcheck-and-the-agents) | every copy of hatcheck and the agents on your `PATH`, which runs, how to remove the rest |
| [`hatcheck desktop`](#claude-desktop-instances-macos-and-windows) | Claude Desktop instances on macOS and Windows |
| [`--json`](docs/commands.md#--json-for-scripts-status-bars-and-apps) | the sessions, plans and results as data, for scripts, status bars and the desktop app |

`hat` is the short name, and a subcommand can be cut to its first letters: `hat m` is `hat move`, `hat r` is `hat resume`; the list's flags have single letters that bundle, so `hat -dc` is `hat --details --by-cost`. [`claude-z`, `claude-k`, `claude-m` and `claude-d`](docs/agents.md#claude-z-claude-k-claude-m-claude-d-claude-code-on-zai-kimi-minimax-or-deepseek) run Claude Code on Z.ai's, Kimi's, MiniMax's or DeepSeek's models in any profile.


## Rules it keeps

Each rule names the code that keeps it, so it can be checked rather than taken on trust.

- **Not in the path.** Nothing of Hatcheck sits between an agent and its model. `cl`, `cx` and
  `ck` set `CLAUDE_CONFIG_DIR`, `CODEX_HOME` or `KIMI_CODE_HOME` (and, for `cl -z`, `-k`, `-m` and
  `-D`, the provider variables you set up) and run the agent in their place; nothing stays running
  ([`launch.rs`](crates/hatcheck/src/launch.rs), `run`).
- **No agent's config is rewritten**, except when you ask: `hatcheck bind --hook` adds one
  session-start hook to each profile's `settings.json`, each Codex home's `hooks.json` and
  Antigravity CLI's `~/.gemini/config/hooks.json`, or points the one it added at the `hatcheck` it
  finds now, and `unbind --hook` removes only what it added ([`bind.rs`](crates/hatcheck/src/bind.rs),
  `add_hooks` and `remove_hooks`); `hatcheck statusline --install` sets the `statusLine` of the
  profiles you name, leaves one somebody else set alone, and `--uninstall` takes out only the one
  it added ([`statusline.rs`](crates/hatcheck/src/statusline.rs), `install` and `uninstall`).
  `hatcheck profiles` and `search` only read.
- **No credentials.** It never asks for, stores, refreshes or moves a login. For Claude it never
  reads one: Claude Code's own `/usage` is run in the profile and the snapshot it saves is read
  ([`usage.rs`](crates/hatcheck/src/usage.rs), `run_claude_usage` and `claude_live`). For Codex, the
  home's `auth.json` token is sent only to ChatGPT's usage endpoint, as it is, and never refreshed
  (`read_codex_login` and `fetch_codex_usage` there). For agy it never reads one either: agy's own
  `/usage` is run, after `agy models` has said agy is signed in, since asked without a sign-in agy
  would start one ([`agy.rs`](crates/hatcheck/src/agy.rs), `run_usage` and `signed_in`). For Kimi,
  the home's `credentials/kimi-code.json` token is sent only to Kimi's usage endpoint, the one
  kimi's own `/usage` asks, as it is, and never refreshed ([`kimi.rs`](crates/hatcheck/src/kimi.rs),
  `read_login`; `usage.rs`, `fetch_kimi_usage`). For ZCode, DeepSeek Harness, Qwen Code, GitHub
  Copilot CLI and OpenCode it reads none, and reads no usage: none of them has a plan-usage command
  or endpoint hatcheck can use. MiniMax Code reads no auth or provider config, and its plan quota
  isn't wired into hatcheck yet.
- **Nothing leaves your computer** except those usage requests: Claude Code's to Anthropic, one to
  ChatGPT per Codex account, one to Kimi per Kimi home, and agy's own `/usage` to Google, with the
  model list agy fetches first to say it's signed in. `CODEX_USAGE_URL` in `usage.rs` and
  `DEFAULT_USAGE_URL` and `GLOBAL_USAGE_URL` in `kimi.rs` are the only addresses the binary itself
  calls; DSH, mcode, qwen, copilot and opencode add none. `--cached` and `--no-usage` make none. The
  Z.ai, Kimi, MiniMax and DeepSeek endpoints in [`provider.rs`](crates/hatcheck/src/provider.rs) are
  handed to Claude Code, which calls them only when you run `claude-z`, `claude-k`, `claude-m` or
  `claude-d`, or `cl` with `-z`, `-k`, `-m` or `-D`.
- **What the agents send is theirs to govern.** Hatcheck adds no data path of its own. What each
  agent sends to its maker, and whether that is kept or used to train models, depends on the
  maker's terms for your product and plan, which differ between them and change; check them for
  each account you keep apart: [Claude Code](https://code.claude.com/docs/en/data-usage),
  [Codex](https://help.openai.com/en/articles/11369540-openai-codex) (and
  [the API](https://developers.openai.com/api/docs/guides/your-data)),
  [Antigravity](https://antigravity.google/terms),
  [Kimi](https://www.kimi.com/user/agreement/userPrivacy),
  [Z.ai](https://docs.z.ai/legal-agreement/privacy-policy),
  [DeepSeek](https://cdn.deepseek.com/policies/en-US/deepseek-privacy-policy.html),
  [MiniMax](https://chat.minimax.io/doc/privacy-policy.html),
  [Qwen Code](https://qwenlm.github.io/qwen-code-docs/en/users/support/tos-privacy/) and
  [GitHub Copilot](https://docs.github.com/en/site-policy/privacy-policies/github-copilot-privacy-statement),
  and OpenCode's to whichever provider you signed it in to, under that provider's terms. Hatcheck
  summarises none of them; [the weekly check](docs/maintenance.md) only keeps the links alive.
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
  [`relocate.rs`](crates/hatcheck/src/relocate.rs)). A Kimi session's line in kimi's index goes
  with its folder ([`kimi.rs`](crates/hatcheck/src/kimi.rs), `index_add` and `index_remove`),
  ZCode's database is only ever read ([`zcode.rs`](crates/hatcheck/src/zcode.rs), `open`), DSH's
  session logs and workspace state are only ever read ([`dsh.rs`](crates/hatcheck/src/dsh.rs)),
  mcode's catalog, canonical history and assets are only ever read
  ([`mcode.rs`](crates/hatcheck/src/mcode.rs)), qwen's transcripts, registry and sidecars are only
  ever read ([`qwen.rs`](crates/hatcheck/src/qwen.rs)), copilot's session folders are only ever
  read ([`copilot.rs`](crates/hatcheck/src/copilot.rs)), and opencode's database is only ever read
  ([`opencode.rs`](crates/hatcheck/src/opencode.rs), `open`).

Function names rather than line numbers, since lines move; `git grep` finds them.

## The desktop app

A menu-bar app (system tray on Windows and Linux) built on the CLI. It shows what's left of every
plan, sorted by Fable, with bars and reset times, and the last day's sessions with **Resume**,
or **Open DSH** for DeepSeek Harness's web session history, and **Copy**. It also has buttons for
the Claude Desktop instances and notifies you when a limit runs low or resets.

Its main window browses every session, with search, filters and a read-only preview of the
conversation. From there you can resume or open a session, copy its command, or move or fork it to
another profile after a dry run, handing over a brief in place of the whole conversation when it
goes to another agent. The window also lists profiles and homes, signs in to a new one in a
terminal, and makes Desktop instances.

It ships the CLI inside itself, so it needs no Python. It never handles a login, and live usage is
off until you turn it on. See [desktop/README.md](desktop/README.md) to build and test it, with
`npm run demo` running it against a demo home rather than your profiles; releases come from
`.github/workflows/desktop.yml` for macOS, Windows and Linux. The plan behind it is
[the proposal](docs/proposals/desktop-app.md).

## Files in this repo

- **`crates/hatcheck/`**: the CLI in Rust: a file per subcommand in `src/` and one per agent
  (`agy.rs`, `kimi.rs`, `zcode.rs` and the rest), with the launchers in `launch.rs` and `src/bin/`.
  It was ported from a Python version with the same output ([docs/rust-port.md](docs/rust-port.md)).
- **`desktop/`**: the desktop app, Tauri with the CLI inside it ([its README](desktop/README.md)).
- **`docs/`**: [commands.md](docs/commands.md), [agents.md](docs/agents.md) and
  [install.md](docs/install.md), each command, agent and way of installing as it is;
  [formats.md](docs/formats.md), each agent's session format side by side;
  [maintenance.md](docs/maintenance.md), the check of each agent's newest version every week or
  two, and the release steps; [desktop-instances.md](docs/desktop-instances.md); and
  [proposals/](docs/proposals/), the plans these came from.
- **`tests/`**: the command tests, which run the binary against throwaway home folders and never
  read your real profiles, the Keychain or the network: `python3 -m unittest discover -s tests`,
  beside the crate's own `cargo test -p hatcheck`. GitHub Actions runs both on macOS, Linux and
  Windows.
- **`AGENTS.md`** and **`docs/lessons.md`**: notes for coding agents (`CLAUDE.md` imports them),
  and what earlier work got wrong, with the rule that would have caught it.
- **`CHANGELOG.md`**, each release's notes; **`install.sh`** and **`install.ps1`**, to build from a
  clone; **`pyproject.toml`** and **`dist-workspace.toml`**, the PyPI wheels and the release
  builds; **`packaging/`**, the app's cask and the winget manifests; **`scripts/`**, `upstream.py`
  (the Monday check) and `provider_probe.py`; and **`site/`**, [hatcheck.dev](https://hatcheck.dev)
  and the logo.

## Roadmap

- **Claude Desktop instances on Linux.** Claude Desktop for Linux is in beta, for Ubuntu and
  Debian. Whether its own `--user-data-dir` keeps an instance apart there, as on macOS, hasn't been
  tried; until it has, only the CLI tools apply on Linux.
- **Releases:** the CLI is on GitHub, the Homebrew tap, PyPI and crates.io from 0.2.0
  ([CHANGELOG.md](CHANGELOG.md)). Next are winget, then the app's installers, once its release
  workflow has its signing and updater secrets (`desktop.yml` lists them).
- **Signing:** a Developer ID for the macOS app, and a certificate for the Windows one.
- **Each agent's gaps**, from Antigravity CLI profiles to a second OpenCode store, with what
  would close each: [docs/agents.md](docs/agents.md#what-isnt-done-yet).

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
- **The CLI tools:** `hatcheck`, `cl`, `cx`, `ck`, the installers and the tests are by
  [Yi Wang](https://github.com/yiwang).
- **Z.ai and Kimi:** the provider settings in `crates/hatcheck/src/provider.rs` (endpoints, model slots, context
  windows and the config files) come from [claude-z](https://github.com/sam-fakhreddine/claude-z) by
  Kerim ([lettt325](https://github.com/lettt325)), MIT-licensed, and Yi Wang's
  [fork](https://github.com/yiwang/claude-z) of it, which added Kimi. MiniMax's and DeepSeek's are
  their own Claude Code guides'.

## License

MIT, see [LICENSE](LICENSE). It covers everything in this repo: the CLI, the desktop app, the site
and the docs.
