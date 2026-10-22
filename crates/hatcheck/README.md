# hatcheck

Claude Code, Codex, Antigravity CLI, Kimi Code CLI and ZCode accounts on one computer, each kept apart. `hatcheck`
lists the recent sessions in every Claude Code profile (`~/.claude-<name>`), Codex home
(`~/.codex-<name>`) and Kimi home (`~/.kimi-code-<name>`), and in Antigravity CLI's and ZCode's
folders, each with the command that resumes it and what's left of its account's plan. It carries a session to another account, or converts it between the
agents, and makes Claude Desktop instances on macOS. `cl` and `cx` run Claude Code and Codex with
a profile, and `claude-z` and `claude-k` run Claude Code on Z.ai's or Kimi's models.

*Unofficial: not made by, affiliated with or endorsed by Anthropic, OpenAI or Google.*

## Install

```sh
cargo binstall hatcheck   # the binaries from the GitHub release
cargo install hatcheck    # or build them
```

Either way you get `hatcheck` (`hat` for short), `cl`, `cx`, `claude-z` and `claude-k`. Homebrew,
PyPI and the install scripts are in [docs/install.md](https://github.com/hatcheck/hatcheck/blob/main/docs/install.md).

## Commands

```text
hatcheck [HOURS]                   recent sessions in every profile and home, with resume
                                   commands and plan usage
hatcheck usage                     plan usage left for every profile and home
hatcheck best [--for SESSION]      the best account now, and the move command that carries
                                   a session to it
hatcheck move SESSION PROFILE      move a session to another profile or home, or convert it
                                   between the agents
hatcheck fork SESSION PROFILE      fork a session into another profile or home: a new session
                                   with its own ID, and the original stays
hatcheck relocate OLD NEW          a moved project folder's sessions, brought along
hatcheck open [SESSION]            resume a session in a new terminal, in its folder
hatcheck show SESSION              a session's conversation, read-only
hatcheck where SESSION             where a session is stored
hatcheck bind PROFILE              tie a folder to a profile, for cl, cx and the list
hatcheck doctor                    every copy of hatcheck and the agents on your PATH
hatcheck desktop                   Claude Desktop instances on macOS
hatcheck provider                  the Z.ai and Kimi keys and models behind claude-z and claude-k
cl work --resume <id>              Claude Code with ~/.claude-work
cx work resume <id>                Codex with ~/.codex-work
```

`hatcheck SUBCOMMAND --help` lists a subcommand's options, and most take `--json`. A subcommand
can be cut to its first letters: `hat m` is `hatcheck move`, and `b`, `s`, `u` and `d` are `best`,
`show`, `usage` and `desktop`. The
[docs](https://github.com/hatcheck/hatcheck/blob/main/docs/commands.md) have the details, and
[hatcheck.dev](https://hatcheck.dev) the overview.

## The library

The crate's library is how its binaries share code, not an interface for other crates: its
modules change with the commands, and keep no compatibility from one version to the next.

## License

MIT, see [LICENSE](https://github.com/hatcheck/hatcheck/blob/main/LICENSE).
