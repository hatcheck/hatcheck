# Hatcheck, the app

A menu-bar app (system tray on Windows and Linux) for what `hatcheck` does in a terminal: what's
left of every plan at a glance, recent sessions from every agent to resume or open, and Claude Desktop
instances a click away. A main window browses every session, with a read-only
preview, moves sessions between profiles, adds profiles and makes Desktop instances.

It's a front end to the `hatcheck` CLI, which it ships inside itself as a standalone binary, so it
needs no Python. Nothing about the agents' files is reimplemented here; the app asks
the CLI for JSON (`hatcheck --json`, `hatcheck usage --json`, `hatcheck show --json`, `hatcheck move
--json`, `hatcheck desktop --json`), and Resume runs `hatcheck open`, which opens your terminal.

*Unofficial: not made by, affiliated with or endorsed by Anthropic, OpenAI, Google, Moonshot AI, Z.ai, DeepSeek, MiniMax or Alibaba Cloud.*

## What it does

- **Menu bar:** each account's limits as bars, sorted by Fable as `hatcheck usage` sorts them, with
  the time left until each resets; the last day's sessions, by folder, with **Resume** (a terminal in
  the session's folder, through `hatcheck open`) and **Copy** (`cd … && <resume command>`); and a
  button per Claude Desktop instance, which opens it or brings it forward.
- **Notifications** when a limit runs low (10% or less left, by default), when a low one resets,
  and when a limit is about to reset with much of it unused (half or more left in the last fifth
  of its window, a day at most, by default): what's left then goes to waste, so it's the account
  to use now.
- **Main window:**
  - *Sessions:* every session of the last day to month, searchable by title and folder, filtered
    by agent and profile, with a read-only preview of the conversation (`hatcheck show`), and
    Resume, Copy and **Move**: pick a profile, see the dry run (`hatcheck move --dry-run --json`,
    or `hatcheck fork` for a new session that keeps the original; to another agent's profile,
    **Hand over a brief** passes `--brief`), then do it, and resume it there.
  - *Plans:* every account's limits.
  - *Profiles:* every profile or home, and **Sign in** or **Add one**, which runs the agent's own
    sign-in through `hatcheck open --login`, so the login stays with the agent.
  - *Desktop* (macOS): the Claude Desktop instances, and **Make one** with its own colour
    (`hatcheck desktop new`), and remove them.
  - *Settings:* live usage, how often to refresh, notifications, which terminal Resume opens, and
    **Check for updates**.

DeepSeek Harness account homes are `~/.dsh` and `~/.dsh-NAME`, selected with `DSH_HOME`; **Add one**
makes `dsh:NAME`. DSH's own profiles are plugin stacks, not Hatcheck account homes, and there is no
Hatcheck DSH launcher. **Sign in** runs `dsh web`, keeping sign-in with DSH.

For a DSH session, **Open DeepSeek Harness** in the main window or **Open DSH** in the menu bar runs
`dsh web` (with `DSH_HOME` for a nondefault home). That opens DSH's web UI and session history in
the session's recorded working directory. Choose the session there; Hatcheck doesn't deep-link or
resume the selected ID directly.

MiniMax Code homes are `~/.minimax` and `~/.minimax-NAME`, selected with `MINIMAX_DATA_DIR`; **Add
one** makes `mcode:NAME`, and there is no Hatcheck mcode launcher. `hatcheck open --login
mcode[:NAME]` explicitly runs `mcode login`, keeping sign-in with MiniMax Code. `~/.minimax-code` is
the installer directory, not a profile named `code`.

For a MiniMax Code session, **Resume** opens a terminal in the recorded folder and runs
`mcode --session mvs_<id>` directly, with `MINIMAX_DATA_DIR` for a named or custom home. Hatcheck
never writes MiniMax Code's store: mcode isn't offered as a Move or Fork target, sessions aren't
copied between MiniMax Code homes, and nothing is converted into mcode.

Qwen Code homes are `~/.qwen` and `~/.qwen-NAME`, selected with `QWEN_HOME`; **Add one** makes
`qwen:NAME`, and there is no Hatcheck qwen launcher. `hatcheck open --login qwen[:NAME]` starts
`qwen` in that home, where its own `/auth` signs in, so sign-in stays with Qwen Code. For a Qwen
Code session, **Resume** opens a terminal in the recorded folder and runs `qwen --resume <id>`,
with `QWEN_HOME` for a named or custom home. Hatcheck never writes Qwen Code's store: a session
moved or forked to Qwen Code becomes a file qwen is started on, as with Antigravity CLI and ZCode,
and sessions aren't copied between its homes.

## Rules it keeps

- **No credentials.** It never asks for, stores, refreshes or passes on a login.
- **Live usage is opt-in.** By default it shows what Claude Code and Codex last saved, and the
  last readings hatcheck kept of agy's and Kimi's (`--cached`). With live usage on, the engine runs
  Claude Code's `/usage` in each profile, which asks Anthropic with that profile's own login and
  saves the answer, asks ChatGPT's and Kimi's usage endpoints for each Codex and Kimi home with its
  saved token, read and never refreshed, and runs agy's own `/usage` once `agy models` has said
  it's signed in. DeepSeek Harness has no plan-quota command or endpoint, so Plans shows **no data**
  for its homes. For MiniMax Code, Plans shows a normal **no data** account with the reason "mcode
  plan quota isn't wired into hatcheck (0.6.13)"; no MiniMax usage request is made. Qwen Code has no
  plan-usage command or endpoint either, so its homes show **no data** too.
- **No automatic switching** between accounts.

## Build and test it

Needs Node 22 and Rust (1.90 or newer), and on macOS, Xcode or its command line tools.

```bash
cd desktop
npm install             # once, and again when package.json changes
npm run engine          # the hatcheck binary, from crates/hatcheck, as src-tauri/binaries/hatcheck-<target triple>
npm run dev             # the pages in a browser, at http://localhost:1420, with sample data
npm run tauri dev       # the app from source, against your profiles, the pages reloading as they change
npm run demo            # the app against a demo home, to try Move and the rest on sessions that don't matter
npm run tauri build     # src-tauri/target/release/bundle/: Hatcheck.app and a .dmg on macOS
npm run build           # what CI checks of the pages: the types (tsc) and the build (vite)
```

- **The engine** is built only by `npm run engine`, not by `tauri dev` or `tauri build`: run it
  again after changing the CLI. The app keeps its engines running, so quit it and start it again
  to get the new one. `Hatcheck.app/Contents/MacOS/hatcheck-desktop --selftest` runs the bundled
  engine as the app does, each call twice with how long it took, and prints what it got, without
  opening a window.
- **`npm run dev`** serves the main window (`index.html`) and the menu-bar popover (`tray.html`)
  with the sample data in `src/sample.ts`, and no engine: for working on the pages.
- **`npm run tauri dev`** runs the app in the menu bar against your own profiles. Its log is that
  terminal, and right-click → Inspect Element opens the web inspector. Looking is safe: the lists,
  previews and plans only read, and live usage is off until you turn it on. Doing isn't: **Move it**
  deletes the original once the copy is checked (**Preview** first, or tick **Keep the original**),
  making or removing a Desktop instance changes `/Applications`, and Resume and Sign in open your
  terminal.
- **`npm run demo`** builds the engine and the app, makes a demo home with the command tests'
  throwaway profiles (three Claude Code profiles with saved usage, two Codex homes, sessions in two
  folders, a bound folder), and starts the app with `HOME` there, so Move and Fork change only the
  demo's sessions. It deletes the demo home when the app quits. `npm run demo -- --no-build` starts
  the last build, and `-- --keep` keeps the demo home. Resume, Sign in and Add one still open a
  terminal in your own home, and Desktop instances are real apps.
- **`npm run tauri build`** makes the app as a release has it, unsigned, which runs on the Mac
  that built it. The updater and macOS signing need a release ([Releases](#releases)).
- CI (`.github/workflows/desktop-check.yml`) runs `npm ci && npm run build`, and `cargo check` in
  `src-tauri`.

`npm run icons` redraws the icons (`scripts/make-icons.py`, with Pillow, from the logo's shapes in
`site/favicon.svg`) and makes every size: the app icon, a black menu-bar icon that macOS tints as
a template, and a coloured tray icon for Windows and Linux, which show tray icons as they are.

## Layout

- `src-tauri/src/lib.rs`: the menu-bar icon and its popover window, the main window, and `engine`,
  the command the pages call to run `hatcheck`. It keeps up to two engines running
  (`hatcheck serve`, which takes a command per line of JSON) so a call doesn't wait for one to
  start, and runs the engine once per call if that fails. It gives the engine the PATH your login
  shell sets, so it finds `cl`, `cx` and `ck`.
- `tray.html`, `src/tray.ts`: the popover, which also refreshes in the background and sends the
  notifications. `index.html`, `src/main.ts`: the main window.
- `src/engine.ts`: each engine call and the shape of its JSON. `src/format.ts`: bars, times and
  paths. `src/sample.ts`: the sample data for the browser.
- `scripts/build-engine.mjs`: the engine, the hatcheck binary built with cargo from the repo's
  `crates/hatcheck`, named for the target as Tauri wants its sidecars. `scripts/demo.py`:
  `npm run demo`, which makes its demo home with `tests/helpers.py`.

## Releases

`.github/workflows/desktop.yml` builds macOS (Apple silicon and Intel), Windows and Linux on a tag such as
`desktop-v0.1.0`, into a draft GitHub release: a `.dmg` and `.app`, a Windows installer (`.exe`)
and `.msi`, and a Linux AppImage and `.deb`. A tag such as `desktop-v0.2.0-beta.1` makes a
pre-release, the beta channel. The updater follows the newest stable release's `latest.json`.

Signing, each part once its secrets are in the repo's Actions secrets (the workflow lists them):

- **Updates:** the key pair for update signing. Its public key is in `src-tauri/tauri.conf.json`;
  the private key was made with `npm run tauri signer generate -- -w ~/.tauri/hatcheck-updater.key`
  and goes in the `TAURI_SIGNING_PRIVATE_KEY` secret, with an empty
  `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. Keep it: updates signed with another key won't install.
- **macOS:** a Developer ID Application certificate, from the Apple Developer Program, for signing,
  and an app-specific password for notarizing.
- **Windows:** unsigned for now, so SmartScreen warns. Signing through Azure Trusted Signing or an
  OV certificate is a step in the workflow to add.

Homebrew and winget: `../packaging/homebrew/hatcheck-app.rb` is the cask and `../packaging/winget/`
the winget manifest, each to fill in with a release's version and checksum.
