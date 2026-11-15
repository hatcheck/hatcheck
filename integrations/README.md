# Small integrations

Each integration calls `hatcheck` and reads its stable JSON output. Nothing reimplements an agent's
session store, reads a login, refreshes a token or starts a background service. Session and plan
queries use cached data (`--cached --no-usage` for sessions, `usage --cached` for plans); insights
are local. A session starts only when you choose its Resume action. `HATCHECK_BIN` overrides the
executable for all three; otherwise `hatcheck` must be on your `PATH`.

## Raycast: recent sessions

`raycast/hatcheck-sessions.sh` is a Raycast Script Command. Make it executable and add it to your
Raycast script-command folders, keeping the beside-it `assets/` folder with `hatcheck.png` for its
icon (or copy both into the folder you add):

```sh
chmod +x integrations/raycast/hatcheck-sessions.sh
```

Run **Hatcheck: Recent Sessions**. It lists each title, agent, profile, folder and resume command
to copy. Its optional argument selects one agent (`claude`, `codex`, `kimi`, and so on). The last
week is included, with no live usage request. Raycast's **Copy** action puts the command in your
terminal, where you choose whether to run it.

## Alfred: searchable sessions

Create a workflow with a **Script Filter** running:

```text
/usr/bin/env python3 /absolute/path/to/integrations/alfred/hatcheck_sessions.py {query}
```

A keyword such as `hat` works well. Results filter by title, agent, profile, folder and model, and
`{query}` can contain several words. The main action is `cd <folder> && <resume command>` for an
Open Terminal / Run Script action. Command shows the resume command alone, and Alt shows the raw
session JSON for inspection or export. An empty result explains that no session matched.

## VS Code: sessions and usage

`vscode-hatcheck/` is a plain JavaScript extension with no runtime dependencies. Start an Extension
Development Host from the repository root:

```sh
code --extensionDevelopmentPath="$PWD/integrations/vscode-hatcheck"
```

Then run these commands from the editor's command palette:

- **Hatcheck: Resume a Session**: search sessions, then choose Resume in terminal, Copy resume
  command or Copy raw session JSON. A terminal is created in the session's own folder.
- **Hatcheck: Cached Plan Usage**: the plain `hatcheck usage --cached` report in a document.
- **Hatcheck: Local Usage Insights**: the structured local report, formatted as JSON.

Set `hatcheck.path` if the binary isn't on your `PATH`. `HATCHECK_BIN` takes precedence. No Node
package is required to run it; `npm test` runs the Node built-in tests.
