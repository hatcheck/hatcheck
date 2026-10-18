# Proposal: a Hatcheck desktop app

**Status:** built, October 2026, in [`desktop/`](../../desktop/README.md), as proposed: Tauri v2, the
CLI inside as a PyInstaller sidecar, the menu bar (version 1) and the main window (version 2). The
release workflow builds macOS (Apple silicon), Windows and Linux, with a stable and a beta channel and
the updater. What's left needs accounts and certificates only you can set up, and decisions:

- **Signing:** a Developer ID certificate for macOS (and notarizing), and one for Windows. Until
  then the builds are unsigned, which is fine for your own use; the workflow signs once the secrets
  are set.
- **Publishing:** the first `desktop-v*` tag, the Homebrew cask and the winget manifest
  (`packaging/`). The landing page is up at hatcheck.dev, served from Cloudflare out of `site/`.
- **Intel Macs:** the workflow builds for them too, on GitHub's `macos-15-intel` runner; untested
  here, with no Intel Mac at hand.
- **Desktop instances** are made by the engine itself, `hatcheck desktop new`, a rewrite that
  replaced `make-claude-instance.sh` ([docs/desktop-instances.md](../desktop-instances.md)), so the
  packaged app makes them too and nothing of Aaron Oxborrow's scripts remains.
- **Updates:** the updater plugin is built without its `zip` feature, because the `zip` versions it
  asks for are yanked; Tauri 2's update files don't need it.

## Summary

A small desktop app for what `hatcheck` does in a terminal. It shows what's left of every Claude
and ChatGPT plan at a glance, finds and resumes recent sessions, and launches the right Claude
Desktop instance. It lives in the macOS menu bar (the system tray on Windows and Linux), with an
optional main window for browsing and moving sessions.

The `hatcheck` CLI stays the engine and the app is a front end to it: nothing about Claude Code's or
Codex's files is reimplemented in the app.

## What it does

### Version 1: the menu bar

- **Plan usage:** each account's limits as bars, Claude's 5-hour, weekly and per-model limits and
  ChatGPT's Codex limits side by side, with the time left until each one resets.
- **Recent sessions:** grouped by folder, newest first, as `hatcheck` lists them. Each has
  **Resume**, which opens a terminal in the session's folder and runs its resume command, and
  **Copy command**.
- **Claude Desktop instances:** a button per instance that launches it, or brings it forward if
  it's running.
- **Notifications:** when a limit runs low, and when it resets.

### Version 2: the main window

- **Session browser:** every session in every profile and home, with search by title or folder,
  filters by account and agent, and a read-only preview of the conversation.
- **Move and convert:** `hatcheck move` with a dry-run preview of what comes across and what's left
  out, then a confirmation.
- **Profiles and homes:** create one. Signing in opens `cl <name>` or `cx <name> login` in a terminal,
  so the login stays with Claude Code and Codex.
- **Claude Desktop instances (macOS):** create one with its own colour, and launch or focus it,
  through `hatcheck desktop`, which replaced `make-claude-instance.sh`.

## How it's built

- **[Tauri](https://tauri.app) v2:** a Rust shell with a web UI in TypeScript. It has a tray and
  menu-bar icon, an auto-updater and installers for macOS, Windows and Linux built in, and its apps
  are small.
- **The engine is the Python code in this repo.** The app runs `hatcheck … --json` and reads the
  result (see [the CLI proposal](cli-features.md#1---json-output)).
- **No Python needed on the user's machine.** The engine is bundled as a standalone binary (built
  with PyInstaller) and shipped inside the app as a Tauri sidecar. If bundling Python turns out to
  be a pain (signing, size, start-up time), port the engine to Rust then; the CLI's tests describe
  what it has to do.
- **Sessions run where they always do.** Resume opens the user's terminal (Terminal, iTerm, Windows
  Terminal, …) through `hatcheck open` ([the CLI proposal](cli-features.md#2-hatcheck-open)), rather
  than running Claude Code or Codex inside the app.

## Rules it keeps

- **No credentials.** It never asks for, stores, refreshes or passes on a login. Signing in happens
  in Claude Code and Codex.
- **Live usage is opt-in.** By default it shows what Claude Code and Codex last saved on disk. With
  live checks turned on, it asks the usage endpoints sparingly, since they rate-limit, and never
  refreshes a token.
- **No automatic switching** between accounts to get around a plan's limits. It can show which
  account has the most left when asked ([`hatcheck best`](cli-features.md#4-hatcheck-best)).
- **Unofficial.** No Anthropic or OpenAI logos or names in the app's name or icon, and an
  "Unofficial" line in About.
- **Keychain:** on macOS it reads Claude Code's Keychain entries through the `security` tool, as the
  CLI does, which may avoid new Keychain prompts. That's untested from an app.

## Plan

| Phase | Scope | Rough effort |
|---|---|---|
| 0 | `--json` output and `hatcheck open` in the CLI | done (October 2026) |
| 1 | macOS menu-bar app, unsigned, for our own use | done |
| 2 | The main window, then a signed and notarized release | window done; signing waits for the certificate |
| 3 | Windows and Linux builds, auto-update channels | done in the workflow; the first release is yours to tag |

Phase 0 is useful without the app, for scripts and status bars, so it came first; it's built.

## Distribution and costs

- **macOS:** a signed and notarized DMG on GitHub Releases, and a Homebrew cask in our own tap. It
  needs the Apple Developer Program ($99 a year). Not the Mac App Store: its sandbox can't read
  `~/.claude*` or run `claude`.
- **Windows:** a signed installer, and winget. Signing through Azure costs about $10 a month, for
  individuals in the US and Canada only; otherwise an OV certificate. Unsigned builds get SmartScreen
  warnings.
- **Linux:** an AppImage and a `.deb`.
- **Updates:** stable releases from tags, and betas as GitHub pre-releases.
- **Website:** a landing page with the downloads, at hatcheck.dev. The page is `site/index.html`,
  served from Cloudflare; `wrangler deploy` in `site/` ships it.

## Risks

- **Formats change.** Claude Code's and Codex's session files and usage endpoints aren't documented
  and can change with any release. The app has to cope: show what it can and say what it can't. The
  auto-updater gets fixes to users quickly.
- **Licensing.** The repo has been MIT-licensed since 17 October 2026 (`LICENSE`). The Desktop-instance feature was
  rewritten in October 2026 (`hatcheck desktop`), so it no longer depends on Aaron Oxborrow's
  scripts ([issue #1](https://github.com/hatcheck/hatcheck/issues/1)); the `--user-data-dir` technique
  itself is free to use.
- **Three platforms to maintain.** Start with macOS only.

## Decisions needed

- **Stack:** Tauri (recommended). SwiftUI would make the nicest Mac app but is macOS-only; Flutter
  works too, but its tray support is weaker.
- **Version 1 scope:** the menu bar only?
- **Cost:** pay for the Apple developer account now, or once there's something to release.
- **License:** MIT, chosen on 17 October 2026.
