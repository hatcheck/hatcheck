# Keeping up with the agents

hatcheck reads files other apps write, prints commands they take, and names their models, apps
and installers; each of them changes every few weeks. The tests can't see it: they read fixtures
shaped like the versions they were written against. So every week or two, and before a release,
check the newest version of each agent against hatcheck, and update the code, docs and site with
what changed. [lessons.md](lessons.md#the-agents-files) says why, with what the first run found.

## 1. What's new

```sh
python3 scripts/upstream.py --providers
```

It changes nothing and runs no agent but `--version`. It prints, for each agent, the version
installed here, the latest release and the versions the docs cite; each desktop app's Homebrew
cask and whether it's deprecated; every link in the README, `docs/` and `site/` that doesn't
answer 200 or now redirects, install scripts included; and, with `--providers`, Kimi's and Z.ai's
model lists beside the menus of `claude-k` and `claude-z` (asked with the keys in their config
files, sent only to those providers' model-list endpoints). An agent added to hatcheck gets a row
in its `AGENTS`.

Then read each maker's install page and each provider's Claude Code guide for what a list can't
show: a renamed app, a new install command, a plan renamed, a model's context or a setting it
needs. The addresses are in [agents.md](agents.md#getting-the-agents) and the `claude-z`,
`claude-k` section.

## 2. Probe each new version

Bring each agent here to its latest (its own updater, or `npx -y <package>@<version>` for one
run of an npm one), then probe from a folder of its own, so everything a probe leaves is easy to
find: `mkdir` one in a temporary folder, `git init` it, `touch` a marker beside it. Use prompts
that ask for one word, so nothing of yours is sent anywhere, and an account of yours with room.

- **A session from each:** `cl <profile> -p "Reply with exactly the word PINEAPPLE"`,
  `codex exec "… MANGO"`, `kimi -p "… KIWI"`. In the folder, `hatcheck sessions --all --json
  --no-usage` lists each with its folder, title, model and resume command, and `hatcheck show`
  and `where` find each.
- **Into each agent:** `hatcheck fork <claude-id> codex`, then `codex exec resume <new-id> "Which
  single word did you reply with earlier? Answer with just that word."`, which should answer
  PINEAPPLE; `fork <codex-id> <profile>`, then `cl <profile> -p --resume <new-id> "…"`; `fork
  <id> kimi`, opened once in kimi's terminal UI (`kimi --session …`, which validates every event,
  then quit) and asked in print mode; `fork <id> agy`, then the printed `agy --add-dir … ` with `-p`
  and the question in place of `-i`. The agy conversation that makes is also the check that
  hatcheck reads agy's newest version: it's listed in the probe folder, and `show` reads it.
- **Between profiles:** `hatcheck move` and `fork` a Claude Code session to another profile with
  room, and resume it there.
- **Usage:** `hatcheck usage --json`: every signed-in account reads `live`, with its limits.
- **ZCode:** its CLI comes in the app and shares the app's database, so a newer CLI is compared,
  not run: mount the new app's disk image read-only, and compare its `Resources/glm/zcode.cjs`
  with the installed one: the version, the `create table` and `alter table` statements for
  `session`, `message` and `part`, the migrations (`schema_migration` in `~/.zcode/cli/db/db.sqlite`
  names those applied), and the flags hatcheck prints (`--resume`, `--attach`, `--mode`, `-p`,
  `-c`).
- **claude-k and claude-z:** `python3 scripts/provider_probe.py` runs each model of their menus
  through Claude Code once.

## 3. Clean up

Every probe leaves something in a real home. Look for what changed since the marker (`find <home>
-newer <marker>`), and take out only what the probe made, after copying any shared file you edit:

- Claude Code: the probe folder's `projects/<key>` folder in each profile used.
- Codex: the probe's `sessions/YYYY/MM/DD/rollout-…-<id>.jsonl` files and their lines in
  `session_index.jsonl`.
- Kimi: `sessions/wd_<probe>_<hash>/`, its sessions' lines in `session_index.jsonl`, its entry in
  `workspaces.json` (kimi writes it compact: keep it so), `workspace-trust/wd_<probe>_<hash>`, and
  the `user-history/` file of what you typed in its UI.
- agy: the conversation stays: it lives in agy's own database, which hatcheck never writes; delete
  it from the Antigravity app if you want it gone.
- hatcheck: the probe's `imports/<id>.md` in its cache folder.

## 4. Update

- **Code:** what changed in a format, a fix with a test whose fixture has the new shape (the
  helpers in `tests/helpers.py` take what each version writes).
- **Docs:** each "checked against" names the version and date it was checked again, and only what
  was; dated evidence of a probe that wasn't run again stays as it was. Installers, apps, models
  and plans in [agents.md](agents.md#getting-the-agents) and the `claude-z`, `claude-k` section.
- **Site:** `site/agents.html`'s table and its install commands and app cards; at a release,
  `python3 site/refresh.py` for the help blocks and samples.
- **CHANGELOG.md** for what users notice, and [lessons.md](lessons.md) when something had broken.
- Add a line below.

## Runs

| Date | Checked against | Found |
|---|---|---|
| 2026-10-24 | Claude Code 2.1.310, Codex 0.164.0, agy 1.4.7, kimi 2.3.1, ZCode 0.16.9 (and 0.17.0 compared), dsh 0.2.1-rc.2 | agy 1.4.7 lists `--add-dir` folders first, filing an imported conversation under hatcheck's cache; ZCode 0.17.0 keeps deleted sessions' rows; Kimi's K3.1 Pro and English address, and `kimi-for-coding` renamed K2.8 Preview; Z.ai pairs `glm-5.3-flash[1m]`; the Codex app is part of ChatGPT; Claude Desktop has a Linux beta |
