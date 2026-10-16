# Hatcheck, the app

A menu-bar app (system tray on Windows and Linux) for what `hatcheck` does in a terminal: what's
left of every Claude and ChatGPT plan at a glance, recent Claude Code and Codex sessions to resume,
and Claude Desktop instances a click away. A main window browses every session, with a read-only
preview, moves sessions between profiles, adds profiles and makes Desktop instances.

It's a front end to the `hatcheck` CLI, which it ships inside itself as a standalone binary, so it
needs no Python. Nothing about Claude Code's or Codex's files is reimplemented here; the app asks
the CLI for JSON (`hatcheck --json`, `hatcheck usage --json`, `hatcheck show --json`, `hatcheck move
--json`, `hatcheck desktop --json`), and Resume runs `hatcheck open`, which opens your terminal.

*Unofficial: not made by, affiliated with or endorsed by Anthropic or OpenAI.*

## What it does

- **Menu bar:** each account's limits as bars, sorted by Fable as `hatcheck usage` sorts them, with
  the time left until each resets; the last day's sessions, by folder, with **Resume** (a terminal in
  the session's folder, through `hatcheck open`) and **Copy** (`cd … && <resume command>`); and a
  button per Claude Desktop instance, which opens it or brings it forward.
- **Notifications** when a limit runs low (10% or less left, by default) and when a low one resets.
- **Main window:**
  - *Sessions:* every session of the last day to month, searchable by title and folder, filtered
    by agent and profile, with a read-only preview of the conversation (`hatcheck show`), and
    Resume, Copy and **Move**: pick a profile, see the dry run (`hatcheck move --dry-run --json`),
    then do it, and resume it there.
  - *Plans:* every account's limits.
  - *Profiles:* every profile and Codex home, and **Sign in** or **Add one**, which opens
    `cl <name>` or `cx <name> login` in a terminal (`hatcheck open --login`), so the login stays
    with Claude Code and Codex.
  - *Desktop* (macOS): the Claude Desktop instances, and **Make one** with its own colour
    (`hatcheck desktop new`), and remove them.
  - *Settings:* live usage, how often to refresh, notifications, which terminal Resume opens, and
    **Check for updates**.

## Rules it keeps

- **No credentials.** It never asks for, stores, refreshes or passes on a login.
- **Live usage is opt-in.** By default it shows what Claude Code and Codex last saved
  (`--cached`). With live usage on, the engine asks the usage endpoints with each profile's saved
  token, read and never refreshed.
- **No automatic switching** between accounts.

## Build it

Needs Node 22, Rust (1.90 or newer) and [uv](https://docs.astral.sh/uv/), which fetches PyInstaller
for the engine.

```bash
cd desktop
npm install
npm run engine          # the hatcheck CLI as src-tauri/binaries/hatcheck-<target triple>
npm run tauri build     # src-tauri/target/release/bundle/: Hatcheck.app and a .dmg on macOS
npm run tauri dev       # or run it from source, with the pages reloading as they change
```

`npm run dev` alone serves the pages in a browser with sample data, for working on them.
`Hatcheck.app/Contents/MacOS/hatcheck-desktop --selftest` runs the bundled engine as the app does
and prints what it got, without opening a window. `npm run icons` redraws the icons
(`scripts/make-icons.py`, with Pillow) and makes every size.

## Layout

- `src-tauri/src/lib.rs`: the menu-bar icon and its popover window, the main window, and `engine`,
  the command the pages call to run `hatcheck`. It gives the engine the PATH your login shell
  sets, so it finds `cl` and `cx`.
- `tray.html`, `src/tray.ts`: the popover, which also refreshes in the background and sends the
  notifications. `index.html`, `src/main.ts`: the main window.
- `src/engine.ts`: each engine call and the shape of its JSON. `src/format.ts`: bars, times and
  paths. `src/sample.ts`: the sample data for the browser.
- `scripts/build-engine.py`: PyInstaller, through uvx, for the engine.

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
