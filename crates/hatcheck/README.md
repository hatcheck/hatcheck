# hatcheck

Claude Code, Codex, Antigravity CLI, Kimi Code CLI, ZCode, DeepSeek Harness, MiniMax Code, Qwen Code, GitHub Copilot CLI and OpenCode accounts on one computer, each kept apart. `hatcheck`
lists the recent sessions in every Claude Code profile (`~/.claude-<name>`), Codex home
(`~/.codex-<name>`), Kimi home (`~/.kimi-code-<name>`), DeepSeek Harness home (`~/.dsh-<name>`), MiniMax Code home (`~/.minimax-<name>`), Qwen Code home (`~/.qwen-<name>`) and GitHub Copilot CLI home (`~/.copilot-<name>`), and in Antigravity CLI's, ZCode's and OpenCode's
folders, each with the command that resumes it or opens DSH's session history, and what's left of its account's plan. It carries a session to another account, converts it between the other agents, and makes Claude Desktop instances on macOS and Windows. `cl`, `cx` and `ck` run Claude Code, Codex and
Kimi Code CLI with a profile, and `claude-z`, `claude-k`, `claude-m` and `claude-d` run Claude Code on Z.ai's, Kimi's, MiniMax's or DeepSeek's models.

*Unofficial: not made by, affiliated with or endorsed by Anthropic, OpenAI, Google, Moonshot AI, Z.ai, DeepSeek, MiniMax, Alibaba Cloud, GitHub or the OpenCode project.*

## Install

```sh
cargo binstall hatcheck   # the binaries from the GitHub release
cargo install hatcheck    # or build them
```

Either way you get `hatcheck` (`hat` for short), `cl`, `cx`, `ck`, `claude-z`, `claude-k`, `claude-m`
and `claude-d`. Homebrew,
PyPI and the install scripts are in [docs/install.md](https://github.com/hatcheck/hatcheck/blob/main/docs/install.md).

## Commands

```text
hatcheck [HOURS]                   recent sessions in every profile and home, with resume
                                   commands and plan usage
hatcheck usage                     plan usage left for every profile and home
hatcheck best [--for SESSION]      the best account now, and the move command that carries
                                   a session to it
hatcheck move SESSION PROFILE      move a session to another profile or home, or convert it
                                   between the other agents
hatcheck fork SESSION PROFILE      fork a session into another profile or home: a new session
                                   with its own ID, and the original stays
hatcheck relocate OLD NEW          a moved project folder's sessions, brought along
hatcheck open [SESSION]            resume a session in a new terminal, or open DSH's web UI
hatcheck resume [SESSION]          carry on the newest session started here, or open DSH,
                                   after asking (hat r)
hatcheck show SESSION              a session's conversation, read-only
hatcheck search WORDS...           the sessions whose title or conversation has every word,
                                   in every profile and home, with the turns they're in
hatcheck where SESSION             where a session is stored
hatcheck bind PROFILE              tie a folder to a profile, for cl, cx, ck and the list
hatcheck profiles [A B]            what each profile and home is set up with, and what
                                   differs between two
hatcheck statusline --install A    a status line for Claude Code: the profile a session runs
                                   in, and what's left of its plan
hatcheck doctor                    every copy of hatcheck and the agents on your PATH
hatcheck completions SHELL         the completions for zsh, bash, fish or PowerShell
hatcheck desktop                   Claude Desktop instances on macOS and Windows
hatcheck provider                  the Z.ai, Kimi, MiniMax and DeepSeek keys and models behind
                                   claude-z, -k, -m and -d
cl a --resume <id>                 Claude Code with ~/.claude-a
cx a resume <id>                   Codex with ~/.codex-a
ck a --session <id>                Kimi Code CLI with ~/.kimi-code-a
```

`hatcheck SUBCOMMAND --help` lists a subcommand's options, and most take `--json`. A subcommand
can be cut to its first letters: `hat m` is `hatcheck move`, and `b`, `s`, `u`, `d` and `r` are `best`,
`show`, `usage`, `desktop` and `resume`. The
[docs](https://github.com/hatcheck/hatcheck/blob/main/docs/commands.md) have the details, and
[hatcheck.dev](https://hatcheck.dev) the overview.

## The library

The crate's library is how its binaries share code, not an interface for other crates: its
modules change with the commands, and keep no compatibility from one version to the next.

## License

MIT, see [LICENSE](https://github.com/hatcheck/hatcheck/blob/main/LICENSE).
