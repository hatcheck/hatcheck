//! hatcheck completions SHELL: the completions for zsh, bash, fish or PowerShell, printed for
//! the shell to load, and `hatcheck complete profiles|sessions`, what the scripts ask for as
//! they run. The subcommands and their options come from the parsers themselves, recorded as
//! each subcommand's --help is run, so a script matches the hatcheck that printed it; the
//! profiles and sessions are read when the shell asks. docs/commands.md's `hatcheck
//! completions` section has how to load them.

use crate::args::{self, Count, Parser};
use crate::cli;
use crate::out;
use crate::paths;
use crate::py::{self, get, s, truthy};
use serde_json::Value;

pub const SHELLS: [&str; 4] = ["zsh", "bash", "fish", "powershell"];
/// How far back the sessions offered reach.
const SESSION_HOURS: &str = "168";

/// What a word at a place is: a profile or home, a session's ID, a folder, a shell, a choice
/// from a list, or anything.
#[derive(Clone, PartialEq, Debug)]
enum Kind {
    Profiles,
    Sessions,
    Folders,
    Shells,
    Choices(Vec<String>),
    Any,
}

/// What a metavar or positional's name stands for: PROFILE is a profile or home, SESSION an
/// ID, OLD, NEW and FOLDER folders, SHELL one of the shells.
fn kind_of(metavar: &str, choices: &[String]) -> Kind {
    if !choices.is_empty() {
        return Kind::Choices(choices.to_vec());
    }
    let m = metavar.to_uppercase();
    if m.contains("PROFILE") {
        Kind::Profiles
    } else if m.contains("SESSION") {
        Kind::Sessions
    } else if ["FOLDER", "OLD", "NEW", "DIR"]
        .iter()
        .any(|w| m.contains(w))
    {
        Kind::Folders
    } else if m.contains("SHELL") {
        Kind::Shells
    } else {
        Kind::Any
    }
}

/// An option as the scripts need it.
struct OptSpec {
    flags: Vec<String>,
    takes: String,
    metavar: String,
    help: String,
    kind: Kind,
}

/// A positional as the scripts need it.
struct PosSpec {
    metavar: String,
    count: String,
    kind: Kind,
}

/// A command (hatcheck itself, a subcommand, or a subcommand's own) and what it takes.
struct Command {
    /// The words after hatcheck: [] for the session list, ["desktop", "new"] for that one.
    words: Vec<String>,
    help: String,
    options: Vec<OptSpec>,
    positionals: Vec<PosSpec>,
    /// Its own subcommands, with their help.
    subcommands: Vec<(String, String)>,
}

fn strings(v: &Value) -> Vec<String> {
    match v {
        Value::Array(a) => a.iter().filter_map(|x| s(x).map(String::from)).collect(),
        _ => Vec::new(),
    }
}

fn command_of(words: Vec<String>, help: &str, spec: &Value) -> Command {
    let options = match get(spec, "options") {
        Value::Array(opts) => opts
            .iter()
            .filter(|o| !truthy(get(o, "hidden")))
            .map(|o| {
                let choices = strings(get(o, "choices"));
                let metavar = py::str_of(get(o, "metavar"));
                OptSpec {
                    flags: strings(get(o, "flags")),
                    takes: py::str_of(get(o, "takes")),
                    kind: kind_of(&metavar, &choices),
                    metavar,
                    help: py::str_of(get(o, "help")),
                }
            })
            .collect(),
        _ => Vec::new(),
    };
    let positionals = match get(spec, "positionals") {
        Value::Array(ps) => ps
            .iter()
            .map(|p| {
                let metavar = py::str_of(get(p, "metavar"));
                PosSpec {
                    kind: kind_of(&metavar, &[]),
                    metavar,
                    count: py::str_of(get(p, "count")),
                }
            })
            .collect(),
        _ => Vec::new(),
    };
    let subcommands = match get(spec, "subcommands") {
        Value::Array(subs) => subs
            .iter()
            .map(|x| (py::str_of(get(x, "name")), py::str_of(get(x, "help"))))
            .collect(),
        _ => Vec::new(),
    };
    Command {
        words,
        help: help.to_string(),
        options,
        positionals,
        subcommands,
    }
}

/// The spec of a command, by running its --help with the parsers recorded: the first parser
/// to parse is the command's own.
fn spec_of(words: &[String]) -> Option<Value> {
    let mut argv: Vec<String> = words.to_vec();
    argv.push("--help".into());
    let (_, specs) = args::recorded(|| out::captured(|| cli::served(argv)));
    specs.into_iter().next()
}

/// The one-line help of each subcommand, from the main help's usage lines.
fn subcommand_help(name: &str) -> String {
    let (_, text, _) = out::captured(|| cli::print_help("hatcheck"));
    let mut lines = text.lines().peekable();
    while let Some(line) = lines.next() {
        let Some(rest) = line.trim_start().strip_prefix("hatcheck ") else {
            continue;
        };
        let Some(first) = rest.split_whitespace().next() else {
            continue;
        };
        if first.trim_matches(|c| c == '[' || c == ']') != name || !line.starts_with("    ") {
            continue;
        }
        // The help is at column 39, over this line and the indented ones after it.
        let mut help: Vec<String> = Vec::new();
        let at = |l: &str| l.chars().skip(39).collect::<String>().trim().to_string();
        help.push(at(line));
        while let Some(next) = lines.peek() {
            if next.starts_with("                                       ") {
                help.push(at(next));
                lines.next();
            } else {
                break;
            }
        }
        return help.join(" ");
    }
    String::new()
}

/// Every command: the session list (no words), each subcommand, and each subcommand's own.
fn commands() -> Vec<Command> {
    let mut found = Vec::new();
    if let Some(spec) = spec_of(&[]) {
        found.push(command_of(Vec::new(), &subcommand_help("sessions"), &spec));
    }
    for name in cli::SUBCOMMANDS {
        let words = vec![name.to_string()];
        let Some(spec) = spec_of(&words) else {
            continue;
        };
        let help = subcommand_help(name);
        let command = command_of(words.clone(), &help, &spec);
        let subs = command.subcommands.clone();
        found.push(command);
        for (sub, help) in subs {
            let mut words = words.clone();
            words.push(sub.clone());
            if let Some(spec) = spec_of(&words) {
                found.push(command_of(words, &help, &spec));
            }
        }
    }
    found
}

// What the scripts ask for as they run.

/// Every profile and home, as a PROFILE argument names it, with the folder: one per line,
/// tab-separated.
fn profiles_text() -> String {
    let mut lines = Vec::new();
    for (agent, folder) in paths::find_all() {
        let name = crate::best::place_name(agent, &folder);
        lines.push(format!("{name}\t{}", paths::shell_path(&folder)));
    }
    lines.join("\n")
}

/// The sessions active in the last week, from every folder: the ID and the title, one per
/// line, tab-separated, newest first.
fn sessions_text() -> String {
    let argv: Vec<String> = [
        "sessions",
        "--all-folders",
        SESSION_HOURS,
        "--no-usage",
        "--json",
    ]
    .iter()
    .map(|w| w.to_string())
    .collect();
    let (_, text, _) = out::captured(|| cli::served(argv));
    let Ok(data) = serde_json::from_str::<Value>(&text) else {
        return String::new();
    };
    let Value::Array(sessions) = get(&data, "sessions") else {
        return String::new();
    };
    let mut rows: Vec<(String, String, String)> = sessions
        .iter()
        .map(|x| {
            (
                py::str_of(get(x, "last_active")),
                py::str_of(get(x, "id")),
                py::collapse(&py::str_of(get(x, "title"))),
            )
        })
        .collect();
    rows.sort_by(|a, b| b.0.cmp(&a.0));
    rows.into_iter()
        .map(|(_, id, title)| format!("{id}\t{}", py::head(&title, 60)))
        .collect::<Vec<_>>()
        .join("\n")
}

/// hatcheck complete profiles|sessions: what the scripts ask for, as data.
pub fn complete(argv: &[String]) -> i32 {
    match argv.first().map(String::as_str) {
        Some("profiles") => {
            out::println(&profiles_text());
            0
        }
        Some("sessions") => {
            out::println(&sessions_text());
            0
        }
        _ => {
            out::error("hatcheck complete: profiles or sessions\n");
            2
        }
    }
}

// The scripts.

fn sh_single(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

/// A help text as a completion shows it: one line, its first sentence or clause, cut to about
/// 70 characters on a word, with an opening bracket left unclosed cut too.
fn brief(help: &str) -> String {
    let flat = py::collapse(help);
    let mut first = flat
        .split(['.', ';', ':'])
        .next()
        .unwrap_or(&flat)
        .trim()
        .to_string();
    if first.chars().count() > 70 {
        let cut = py::head(&first, 70);
        first = match cut.rfind(' ') {
            Some(i) => format!("{}…", &cut[..i]),
            None => format!("{cut}…"),
        };
    }
    while first.matches('(').count() > first.matches(')').count() {
        match first.rfind('(') {
            Some(i) => first.truncate(i),
            None => break,
        }
    }
    first.trim().trim_end_matches(',').to_string()
}

/// The words a command's positional at a place offers, for the shells that can't ask: the
/// kind as a shell function's name.
fn kind_fn(kind: &Kind) -> Option<&'static str> {
    match kind {
        Kind::Profiles => Some("profiles"),
        Kind::Sessions => Some("sessions"),
        Kind::Folders => Some("folders"),
        Kind::Shells => Some("shells"),
        _ => None,
    }
}

fn zsh(commands: &[Command]) -> String {
    let mut out = String::new();
    out += "#compdef hatcheck hat\n";
    out += "# hatcheck's completions for zsh, as `hatcheck completions zsh` printed them: put the output in a\n";
    out += "# file named _hatcheck on your fpath, or add `eval \"$(hatcheck completions zsh)\"` after compinit.\n\n";
    out += "_hatcheck_profiles() {\n  local -a items\n  items=(${(f)\"$(hatcheck complete profiles 2>/dev/null)\"})\n  items=(${items//$'\\t'/:})\n  _describe -t profiles 'profile or home' items\n}\n";
    out += "_hatcheck_sessions() {\n  local -a items\n  items=(${(f)\"$(hatcheck complete sessions 2>/dev/null)\"})\n  items=(${items//:/\\\\:})\n  items=(${items//$'\\t'/:})\n  _describe -t sessions 'session' items\n}\n";
    out += "_hatcheck_shells() {\n  _values 'shell' zsh bash fish powershell\n}\n\n";
    let value_spec = |kind: &Kind, metavar: &str| -> String {
        match kind {
            Kind::Profiles => format!(":{metavar}:_hatcheck_profiles"),
            Kind::Sessions => format!(":{metavar}:_hatcheck_sessions"),
            Kind::Folders => format!(":{metavar}:_files -/"),
            Kind::Shells => format!(":{metavar}:_hatcheck_shells"),
            Kind::Choices(c) => format!(":{metavar}:({})", c.join(" ")),
            Kind::Any => format!(":{metavar}:"),
        }
    };
    let option_lines = |c: &Command| -> Vec<String> {
        let mut lines = Vec::new();
        for o in &c.options {
            let help = brief(&o.help).replace('[', "(").replace(']', ")");
            let value = match o.takes.as_str() {
                "nothing" => String::new(),
                _ => value_spec(&o.kind, &o.metavar),
            };
            let exclusive = format!("({})", o.flags.join(" "));
            for flag in &o.flags {
                lines.push(sh_single(&format!("{exclusive}{flag}[{help}]{value}")));
            }
        }
        lines
    };
    let positional_lines = |c: &Command| -> Vec<String> {
        let mut lines = Vec::new();
        let mut n = 1;
        for p in &c.positionals {
            let spec = value_spec(&p.kind, &p.metavar);
            match p.count.as_str() {
                "any" => lines.push(sh_single(&format!("*{spec}"))),
                _ => {
                    lines.push(sh_single(&format!("{n}{spec}")));
                    n += 1;
                }
            }
        }
        lines
    };
    out += "_hatcheck() {\n  local curcontext=\"$curcontext\" state line\n  typeset -A opt_args\n  local -a subcommands\n  subcommands=(\n";
    for c in commands.iter().filter(|c| c.words.len() == 1) {
        out += &format!(
            "    {}\n",
            sh_single(&format!(
                "{}:{}",
                c.words[0],
                brief(&c.help).replace(':', "\\:")
            ))
        );
    }
    out += "  )\n  _arguments -C \\\n";
    if let Some(list) = commands.iter().find(|c| c.words.is_empty()) {
        for line in option_lines(list) {
            out += &format!("    {line} \\\n");
        }
    }
    out += "    '(- *)'{-V,--version}'[which version this is]' \\\n    '1: :->command' \\\n    '*:: :->args' && return\n";
    out += "  case $state in\n    command) _describe -t commands 'hatcheck command' subcommands ;;\n    args)\n      case $line[1] in\n";
    for c in commands.iter().filter(|c| c.words.len() == 1) {
        out += &format!("        {})\n", c.words[0]);
        if c.subcommands.is_empty() {
            out += "          _arguments \\\n";
            for line in option_lines(c).into_iter().chain(positional_lines(c)) {
                out += &format!("            {line} \\\n");
            }
            out += "            && return ;;\n";
        } else {
            // A subcommand with its own: the first word picks one, the rest are its.
            out += "          local -a own\n          own=(\n";
            for (name, help) in &c.subcommands {
                out += &format!(
                    "            {}\n",
                    sh_single(&format!("{name}:{}", brief(help).replace(':', "\\:")))
                );
            }
            out += "          )\n          _arguments -C \\\n";
            for line in option_lines(c) {
                out += &format!("            {line} \\\n");
            }
            out += "            '1: :->own' \\\n            '*:: :->ownargs' && return\n";
            out += "          case $state in\n            own) _describe -t commands 'subcommand' own ;;\n            ownargs)\n              case $line[1] in\n";
            for sub in commands
                .iter()
                .filter(|x| x.words.len() == 2 && x.words[0] == c.words[0])
            {
                out += &format!(
                    "                {})\n                  _arguments \\\n",
                    sub.words[1]
                );
                for line in option_lines(sub).into_iter().chain(positional_lines(sub)) {
                    out += &format!("                    {line} \\\n");
                }
                out += "                    && return ;;\n";
            }
            out += "              esac ;;\n          esac ;;\n";
        }
    }
    out += "      esac ;;\n  esac\n}\n\ncompdef _hatcheck hatcheck hat\n";
    out
}

fn bash(commands: &[Command]) -> String {
    let mut out = String::new();
    out += "# hatcheck's completions for bash, as `hatcheck completions bash` printed them: add\n";
    out += "# `eval \"$(hatcheck completions bash)\"` to ~/.bashrc, or put the output in a file bash-completion loads.\n\n";
    out += "_hatcheck_words() {\n  case \"$1\" in\n    profiles) hatcheck complete profiles 2>/dev/null | cut -f1 ;;\n    sessions) hatcheck complete sessions 2>/dev/null | cut -f1 ;;\n    shells) echo zsh bash fish powershell ;;\n  esac\n}\n\n";
    out += "_hatcheck() {\n  local cur prev words cword\n  if declare -F _init_completion >/dev/null 2>&1; then\n    _init_completion || return\n  else\n    COMPREPLY=()\n    cur=\"${COMP_WORDS[COMP_CWORD]}\"\n    prev=\"${COMP_WORDS[COMP_CWORD-1]}\"\n    words=(\"${COMP_WORDS[@]}\")\n    cword=$COMP_CWORD\n  fi\n";
    out += "  local sub=\"\" own=\"\" i n=0\n  for ((i = 1; i < cword; i++)); do\n    case \"${words[i]}\" in\n      -*) ;;\n      *) if [ -z \"$sub\" ]; then sub=\"${words[i]}\"; else n=$((n + 1)); if [ -z \"$own\" ]; then own=\"${words[i]}\"; fi; fi ;;\n    esac\n  done\n";
    // Options that take a value of a kind: by the word before.
    out += "  case \"$sub $prev\" in\n";
    for c in commands {
        let key = if c.words.is_empty() {
            "".to_string()
        } else {
            c.words[0].clone()
        };
        for o in c.options.iter().filter(|o| o.takes != "nothing") {
            if let Some(f) = kind_fn(&o.kind) {
                for flag in &o.flags {
                    out += &format!("    \"{key} {flag}\") COMPREPLY=($(compgen -W \"$(_hatcheck_words {f})\" -- \"$cur\")); return ;;\n");
                }
            }
            if let Kind::Choices(choices) = &o.kind {
                for flag in &o.flags {
                    out += &format!("    \"{key} {flag}\") COMPREPLY=($(compgen -W \"{}\" -- \"$cur\")); return ;;\n", choices.join(" "));
                }
            }
        }
    }
    out += "  esac\n";
    out += "  case \"$cur\" in\n    -*)\n      case \"$sub\" in\n";
    for c in commands.iter().filter(|c| c.words.len() <= 1) {
        let key = if c.words.is_empty() {
            "\"\"".to_string()
        } else {
            c.words[0].clone()
        };
        let flags: Vec<String> = c.options.iter().flat_map(|o| o.flags.clone()).collect();
        out += &format!(
            "        {key}) COMPREPLY=($(compgen -W \"{}\" -- \"$cur\")) ;;\n",
            flags.join(" ")
        );
    }
    out += "      esac\n      return ;;\n  esac\n";
    out += "  if [ -z \"$sub\" ]; then\n    COMPREPLY=($(compgen -W \"";
    out += &cli::SUBCOMMANDS.join(" ");
    out += "\" -- \"$cur\"))\n    return\n  fi\n";
    // Positionals: by the subcommand and how many words came before.
    out += "  case \"$sub\" in\n";
    for c in commands.iter().filter(|c| c.words.len() == 1) {
        if !c.subcommands.is_empty() {
            let names: Vec<&str> = c.subcommands.iter().map(|(n, _)| n.as_str()).collect();
            out += &format!("    {})\n      if [ -z \"$own\" ]; then COMPREPLY=($(compgen -W \"{}\" -- \"$cur\")); return; fi\n      case \"$own\" in\n", c.words[0], names.join(" "));
            for sub in commands
                .iter()
                .filter(|x| x.words.len() == 2 && x.words[0] == c.words[0])
            {
                let flags: Vec<String> = sub.options.iter().flat_map(|o| o.flags.clone()).collect();
                out += &format!("        {}) case \"$cur\" in -*) COMPREPLY=($(compgen -W \"{}\" -- \"$cur\")); return ;; esac", sub.words[1], flags.join(" "));
                if let Some(p) = sub.positionals.first() {
                    if let Some(f) = kind_fn(&p.kind) {
                        out += &format!(
                            "; COMPREPLY=($(compgen -W \"$(_hatcheck_words {f})\" -- \"$cur\"))"
                        );
                    } else if let Kind::Choices(choices) = &p.kind {
                        out += &format!(
                            "; COMPREPLY=($(compgen -W \"{}\" -- \"$cur\"))",
                            choices.join(" ")
                        );
                    }
                }
                out += " ;;\n";
            }
            out += "      esac ;;\n";
            continue;
        }
        let mut arms: Vec<String> = Vec::new();
        for (i, p) in c.positionals.iter().enumerate() {
            let place = if p.count == "any" {
                "*".to_string()
            } else {
                i.to_string()
            };
            let words = match (&p.kind, kind_fn(&p.kind)) {
                (Kind::Folders, _) => "COMPREPLY=($(compgen -d -- \"$cur\"))".to_string(),
                (_, Some(f)) => {
                    format!("COMPREPLY=($(compgen -W \"$(_hatcheck_words {f})\" -- \"$cur\"))")
                }
                (Kind::Choices(choices), _) => format!(
                    "COMPREPLY=($(compgen -W \"{}\" -- \"$cur\"))",
                    choices.join(" ")
                ),
                _ => continue,
            };
            arms.push(format!("{place}) {words} ;;"));
        }
        if !arms.is_empty() {
            out += &format!(
                "    {}) case \"$n\" in {} esac ;;\n",
                c.words[0],
                arms.join(" ")
            );
        }
    }
    out += "  esac\n}\n\ncomplete -F _hatcheck hatcheck hat\n";
    out
}

fn fish(commands: &[Command]) -> String {
    let mut out = String::new();
    out += "# hatcheck's completions for fish, as `hatcheck completions fish` printed them: save the output as\n";
    out += "# ~/.config/fish/completions/hatcheck.fish, and the same under the name hat.fish for hat.\n\n";
    let subs: Vec<&str> = cli::SUBCOMMANDS.to_vec();
    for cmd in ["hatcheck", "hat"] {
        out += &format!("complete -c {cmd} -f\n");
        let top = format!("__fish_seen_subcommand_from {}", subs.join(" "));
        for c in commands.iter().filter(|c| c.words.len() == 1) {
            out += &format!(
                "complete -c {cmd} -n 'not {top}' -a {} -d {}\n",
                c.words[0],
                sh_single(&brief(&c.help))
            );
        }
        for c in commands {
            let condition = match c.words.len() {
                0 => format!("not {top}"),
                1 => format!("__fish_seen_subcommand_from {}", c.words[0]),
                _ => format!(
                    "__fish_seen_subcommand_from {}; and __fish_seen_subcommand_from {}",
                    c.words[0], c.words[1]
                ),
            };
            for (name, help) in &c.subcommands {
                let own: Vec<&str> = c.subcommands.iter().map(|(n, _)| n.as_str()).collect();
                out += &format!(
                    "complete -c {cmd} -n '{condition}; and not __fish_seen_subcommand_from {}' -a {name} -d {}\n",
                    own.join(" "),
                    sh_single(&brief(help))
                );
            }
            for o in &c.options {
                let mut line = format!("complete -c {cmd} -n '{condition}'");
                for flag in &o.flags {
                    if let Some(short) = flag
                        .strip_prefix('-')
                        .filter(|f| f.len() == 1 && !f.starts_with('-'))
                    {
                        line += &format!(" -s {short}");
                    } else if let Some(long) = flag.strip_prefix("--") {
                        line += &format!(" -l {long}");
                    }
                }
                if o.takes != "nothing" {
                    line += " -r";
                    match &o.kind {
                        Kind::Profiles => line += " -a '(hatcheck complete profiles)'",
                        Kind::Sessions => line += " -a '(hatcheck complete sessions)'",
                        Kind::Folders => line += " -a '(__fish_complete_directories)'",
                        Kind::Shells => line += " -a 'zsh bash fish powershell'",
                        Kind::Choices(choices) => line += &format!(" -a '{}'", choices.join(" ")),
                        Kind::Any => {}
                    }
                }
                line += &format!(" -d {}", sh_single(&brief(&o.help)));
                out += &line;
                out += "\n";
            }
            for p in &c.positionals {
                let words = match &p.kind {
                    Kind::Profiles => "(hatcheck complete profiles)".to_string(),
                    Kind::Sessions => "(hatcheck complete sessions)".to_string(),
                    Kind::Folders => "(__fish_complete_directories)".to_string(),
                    Kind::Shells => "zsh bash fish powershell".to_string(),
                    Kind::Choices(choices) => choices.join(" "),
                    Kind::Any => continue,
                };
                out += &format!("complete -c {cmd} -n '{condition}' -a '{words}'\n");
            }
        }
        out += "\n";
    }
    out
}

fn powershell(commands: &[Command]) -> String {
    let ps_single = |text: &str| format!("'{}'", text.replace('\'', "''"));
    let mut out = String::new();
    out += "# hatcheck's completions for PowerShell, as `hatcheck completions powershell` printed them: add\n";
    out +=
        "# `hatcheck completions powershell | Out-String | Invoke-Expression` to your profile.\n\n";
    out += "Register-ArgumentCompleter -Native -CommandName hatcheck, hat -ScriptBlock {\n";
    out += "  param($wordToComplete, $commandAst, $cursorPosition)\n";
    out += "  $words = @($commandAst.CommandElements | Select-Object -Skip 1 | ForEach-Object { $_.ToString() })\n";
    out += "  if ($wordToComplete -ne '' -and $words.Count -gt 0) { $words = $words[0..($words.Count - 2)] }\n";
    out += "  $positional = @($words | Where-Object { $_ -notlike '-*' })\n";
    out += "  $sub = if ($positional.Count -gt 0) { $positional[0] } else { '' }\n";
    out += "  $prev = if ($words.Count -gt 0) { $words[-1] } else { '' }\n";
    out += "  $options = @{\n";
    for c in commands.iter().filter(|c| c.words.len() <= 1) {
        let key = if c.words.is_empty() {
            "".to_string()
        } else {
            c.words[0].clone()
        };
        let flags: Vec<String> = c
            .options
            .iter()
            .flat_map(|o| o.flags.iter().map(|f| ps_single(f)))
            .collect();
        out += &format!("    {} = @({})\n", ps_single(&key), flags.join(", "));
    }
    out += "  }\n";
    out += "  $subcommands = @(";
    out += &cli::SUBCOMMANDS
        .iter()
        .map(|s| ps_single(s))
        .collect::<Vec<_>>()
        .join(", ");
    out += ")\n";
    // What a word of a kind completes from.
    out += "  function words($kind) {\n    switch ($kind) {\n      'profiles' { hatcheck complete profiles 2>$null | ForEach-Object { ($_ -split \"`t\")[0] } }\n      'sessions' { hatcheck complete sessions 2>$null | ForEach-Object { ($_ -split \"`t\")[0] } }\n      'shells' { @('zsh', 'bash', 'fish', 'powershell') }\n      'folders' { Get-ChildItem -Directory -Name -Path . -ErrorAction SilentlyContinue }\n    }\n  }\n";
    out += "  $candidates = @()\n  $byPrev = @{\n";
    for c in commands.iter().filter(|c| c.words.len() <= 1) {
        let key = if c.words.is_empty() {
            "".to_string()
        } else {
            c.words[0].clone()
        };
        for o in c.options.iter().filter(|o| o.takes != "nothing") {
            if let Some(f) = kind_fn(&o.kind) {
                for flag in &o.flags {
                    out += &format!("    {} = 'kind:{f}'\n", ps_single(&format!("{key} {flag}")));
                }
            } else if let Kind::Choices(choices) = &o.kind {
                for flag in &o.flags {
                    out += &format!(
                        "    {} = {}\n",
                        ps_single(&format!("{key} {flag}")),
                        ps_single(&choices.join(" "))
                    );
                }
            }
        }
    }
    out += "  }\n";
    out += "  $positionals = @{\n";
    for c in commands.iter().filter(|c| c.words.len() == 1) {
        let kinds: Vec<String> = c
            .positionals
            .iter()
            .map(|p| match (&p.kind, kind_fn(&p.kind)) {
                (_, Some(f)) => format!("kind:{f}"),
                (Kind::Choices(choices), _) => choices.join(" "),
                _ => String::new(),
            })
            .collect();
        if !c.subcommands.is_empty() {
            let names: Vec<&str> = c.subcommands.iter().map(|(n, _)| n.as_str()).collect();
            out += &format!(
                "    {} = @({})\n",
                ps_single(&c.words[0]),
                ps_single(&names.join(" "))
            );
        } else if !kinds.is_empty() {
            out += &format!(
                "    {} = @({})\n",
                ps_single(&c.words[0]),
                kinds
                    .iter()
                    .map(|k| ps_single(k))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
    }
    out += "  }\n";
    out += "  $key = \"$sub $prev\"\n  if ($byPrev.ContainsKey($key)) {\n    $spec = $byPrev[$key]\n    $candidates = if ($spec -like 'kind:*') { words $spec.Substring(5) } else { $spec -split ' ' }\n  }\n";
    out += "  elseif ($wordToComplete -like '-*') {\n    $candidates = if ($options.ContainsKey($sub)) { $options[$sub] } else { $options[''] }\n  }\n";
    out += "  elseif ($sub -eq '') { $candidates = $subcommands }\n";
    out += "  elseif ($positionals.ContainsKey($sub)) {\n    $n = $positional.Count - 1\n    $specs = $positionals[$sub]\n    if ($n -lt $specs.Count) {\n      $spec = $specs[$n]\n      $candidates = if ($spec -like 'kind:*') { words $spec.Substring(5) } elseif ($spec -ne '') { $spec -split ' ' } else { @() }\n    }\n  }\n";
    out += "  $candidates | Where-Object { $_ -like \"$wordToComplete*\" } | ForEach-Object {\n    [System.Management.Automation.CompletionResult]::new($_, $_, 'ParameterValue', $_)\n  }\n}\n";
    out
}

pub fn main(argv: &[String], prog: &str) -> i32 {
    let parser = Parser::new(
        prog,
        "Print the completions for a shell: the subcommands, each one's options, and, where a word is a profile, a session or a folder, those, asked of hatcheck as you type. Load them as the script's first lines say: eval \"$(hatcheck completions zsh)\" after compinit, eval \"$(hatcheck completions bash)\", hatcheck completions fish > ~/.config/fish/completions/hatcheck.fish, or hatcheck completions powershell | Out-String | Invoke-Expression.",
    )
    .positional_as("shell", "SHELL", Count::One, "zsh, bash, fish or powershell");
    let args = match parser.parse(argv) {
        Ok(a) => a,
        Err(stop) => return stop.exit_code(),
    };
    let shell = args.value("shell").unwrap_or("");
    if !SHELLS.contains(&shell) {
        return parser
            .error(&format!(
                "argument SHELL: '{shell}' isn't one of zsh, bash, fish or powershell"
            ))
            .exit_code();
    }
    let commands = commands();
    let text = match shell {
        "zsh" => zsh(&commands),
        "bash" => bash(&commands),
        "fish" => fish(&commands),
        _ => powershell(&commands),
    };
    out::print(&text);
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_command_has_a_spec_and_the_kinds_follow_the_names() {
        let commands = commands();
        let names: Vec<&str> = commands
            .iter()
            .filter(|c| c.words.len() == 1)
            .map(|c| c.words[0].as_str())
            .collect();
        for name in cli::SUBCOMMANDS {
            assert!(names.contains(name), "{name}");
        }
        let search = commands.iter().find(|c| c.words == ["search"]).unwrap();
        let from = search
            .options
            .iter()
            .find(|o| o.flags == ["--from"])
            .unwrap();
        assert_eq!(from.kind, Kind::Profiles);
        assert!(search.positionals.iter().any(|p| p.count == "any"));
        let show = commands.iter().find(|c| c.words == ["show"]).unwrap();
        assert_eq!(show.positionals[0].kind, Kind::Sessions);
        let relocate = commands.iter().find(|c| c.words == ["relocate"]).unwrap();
        assert_eq!(relocate.positionals[0].kind, Kind::Folders);
        let desktop = commands.iter().find(|c| c.words == ["desktop"]).unwrap();
        assert!(!desktop.subcommands.is_empty());
        assert!(commands
            .iter()
            .any(|c| c.words.len() == 2 && c.words[0] == "desktop"));
        assert!(!subcommand_help("search").is_empty());
        for shell in SHELLS {
            let text = match shell {
                "zsh" => zsh(&commands),
                "bash" => bash(&commands),
                "fish" => fish(&commands),
                _ => powershell(&commands),
            };
            assert!(text.contains("search") && text.contains("since"), "{shell}");
        }
    }
}
