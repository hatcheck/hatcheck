//! hatcheck provider ...: the settings for running Claude Code on Z.ai (GLM models) or Kimi
//! instead of Anthropic, behind claude-z, claude-k, cl -z and cl -k. The README's `claude-z`,
//! `claude-k` section describes the config files and what's sent where.

use crate::args::{Count, Parser};
use crate::ospath;
use crate::out;
use crate::paths;
use crate::py;
use serde_json::{Map, Value};
use std::io::{BufRead, Write};

/// A provider: its name, the prefix of its config fields, its endpoint, where to get a key,
/// its model slots, and the model choices, each filling all the slots.
pub struct Provider {
    pub key: &'static str,
    pub name: &'static str,
    pub prefix: &'static str,
    pub url: &'static str,
    pub key_from: &'static str,
    pub models: &'static [&'static str],
    pub choices: &'static [(&'static str, &'static str, &'static [&'static str])],
}

pub const PROVIDERS: [Provider; 2] = [
    Provider {
        key: "k",
        name: "Kimi",
        prefix: "CK_",
        url: "https://api.kimi.com/coding/",
        key_from: "the Kimi Code Console, https://www.kimi.com/code (Create API Key)",
        models: &["MODEL"],
        choices: &[
            (
                "k3[1m]",
                "K3, 1M context (Allegretto plan and up)",
                &["k3[1m]"],
            ),
            ("k3", "K3, 256k context (Moderato and up)", &["k3"]),
            (
                "kimi-for-coding",
                "K2.7 Code, 256k context (all plans)",
                &["kimi-for-coding"],
            ),
            (
                "kimi-for-coding-highspeed",
                "K2.7 Code, faster output (Allegretto and up)",
                &["kimi-for-coding-highspeed"],
            ),
        ],
    },
    Provider {
        key: "z",
        name: "Z.ai",
        prefix: "CZ_",
        url: "https://api.z.ai/api/anthropic",
        key_from: "https://z.ai/manage-apikey/apikey-list",
        models: &["MODEL_OPUS", "MODEL_SONNET", "MODEL_HAIKU"],
        choices: &[
            (
                "glm-5.3[1m]",
                "flagship, 1M context, with glm-5.3-flash for quick tasks",
                &["glm-5.3[1m]", "glm-5.3[1m]", "glm-5.3-flash"],
            ),
            (
                "glm-5.3-flash",
                "fast, multimodal, cheapest",
                &["glm-5.3-flash", "glm-5.3-flash", "glm-5.3-flash"],
            ),
            (
                "glm-4.7",
                "fast, good at code",
                &["glm-4.7", "glm-4.7", "glm-4.7"],
            ),
        ],
    },
];

/// The providers in the order the setup and the status list them: z, then k.
fn in_order() -> [&'static Provider; 2] {
    [&PROVIDERS[1], &PROVIDERS[0]]
}

pub fn provider(key: &str) -> Option<&'static Provider> {
    PROVIDERS.iter().find(|p| p.key == key)
}

/// ~/.config/claude-z/config, or %APPDATA%\claude-z\config on Windows.
pub fn config_path(p: &Provider) -> String {
    let home = paths::home();
    let base = if ospath::WINDOWS {
        std::env::var("APPDATA")
            .ok()
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| ospath::join_all(&home, &["AppData", "Roaming"]))
    } else {
        ospath::join(&home, ".config")
    };
    ospath::join_all(&base, &[&format!("claude-{}", p.key), "config"])
}

pub fn shown(path: &str) -> String {
    let home = paths::home();
    match path.strip_prefix(&format!("{home}{}", ospath::SEP)) {
        Some(_) => format!("~{}", &path[home.len()..]),
        None => path.to_string(),
    }
}

/// A KEY = value line, as the config's regex reads it: (key, value with any quotes taken off).
fn parse_line(line: &str) -> Option<(String, String)> {
    let t = line.trim_start_matches(py::is_space);
    let name_len = t
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .count();
    if name_len == 0 || t.starts_with(|c: char| c.is_ascii_digit()) {
        return None;
    }
    let (name, rest) = t.split_at(name_len);
    let rest = rest.trim_start_matches(py::is_space).strip_prefix('=')?;
    let mut value = rest.trim_matches(py::is_space).to_string();
    let b = value.as_bytes();
    if b.len() >= 2 && b[0] == b[b.len() - 1] && (b[0] == b'"' || b[0] == b'\'') {
        value = value[1..value.len() - 1].to_string();
    }
    Some((name.to_string(), value))
}

/// (values, lines) of a config file, or (None, []) when there's none.
pub fn read_config(path: &str) -> (Option<Map<String, Value>>, Vec<String>) {
    let Ok(bytes) = std::fs::read(path) else {
        return (None, Vec::new());
    };
    let text = String::from_utf8_lossy(&bytes);
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text); // PowerShell 5 writes a BOM
    let lines: Vec<String> = py::splitlines(text).into_iter().map(String::from).collect();
    let mut values = Map::new();
    for line in &lines {
        if line.trim_start_matches(py::is_space).starts_with('#') {
            continue;
        }
        if let Some((k, v)) = parse_line(line) {
            values.insert(k, Value::String(v));
        }
    }
    (Some(values), lines)
}

fn field(p: &Provider, values: &Map<String, Value>, name: &str) -> String {
    values
        .get(&format!("{}{name}", p.prefix))
        .and_then(Value::as_str)
        .map(|v| py::strip(v).to_string())
        .unwrap_or_default()
}

pub fn models(p: &Provider, values: &Map<String, Value>) -> Vec<String> {
    p.models
        .iter()
        .zip(p.choices[0].2)
        .map(|(slot, default)| {
            Some(field(p, values, slot))
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| default.to_string())
        })
        .collect()
}

/// [(name, value)] for Claude Code; a value of None means unset it.
pub fn environment(
    p: &Provider,
    values: &Map<String, Value>,
) -> Vec<(&'static str, Option<String>)> {
    let get = |name: &str| field(p, values, name);
    let base = Some(get("BASE_URL"))
        .filter(|u| !u.is_empty())
        .unwrap_or_else(|| p.url.to_string());
    let mut env: Vec<(&'static str, Option<String>)> = vec![
        ("ANTHROPIC_BASE_URL", Some(base)),
        ("ANTHROPIC_AUTH_TOKEN", Some(get("API_KEY"))),
        // Unset, so that no Anthropic credential from your environment reaches the provider.
        ("ANTHROPIC_API_KEY", None),
        ("CLAUDE_CODE_OAUTH_TOKEN", None),
        ("API_TIMEOUT_MS", Some("3000000".into())),
    ];
    let chosen = models(p, values);
    if p.key == "z" {
        let (opus, sonnet, haiku) = (chosen[0].clone(), chosen[1].clone(), chosen[2].clone());
        env.extend([
            ("ANTHROPIC_MODEL", Some(opus.clone())),
            ("ANTHROPIC_DEFAULT_FABLE_MODEL", Some(opus.clone())),
            ("ANTHROPIC_DEFAULT_OPUS_MODEL", Some(opus.clone())),
            ("ANTHROPIC_DEFAULT_SONNET_MODEL", Some(sonnet)),
            ("ANTHROPIC_DEFAULT_HAIKU_MODEL", Some(haiku)),
            ("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC", Some("1".into())),
        ]);
        if opus.contains("[1m]") {
            env.push(("CLAUDE_CODE_AUTO_COMPACT_WINDOW", Some("1000000".into())));
        }
    } else {
        let model = chosen[0].clone();
        for name in [
            "ANTHROPIC_MODEL",
            "ANTHROPIC_DEFAULT_FABLE_MODEL",
            "ANTHROPIC_DEFAULT_OPUS_MODEL",
            "ANTHROPIC_DEFAULT_SONNET_MODEL",
            "ANTHROPIC_DEFAULT_HAIKU_MODEL",
            "CLAUDE_CODE_SUBAGENT_MODEL",
        ] {
            env.push((name, Some(model.clone())));
        }
        let window = if model.ends_with("[1m]") {
            "1048576"
        } else {
            "262144"
        };
        env.push(("CLAUDE_CODE_AUTO_COMPACT_WINDOW", Some(window.into())));
        env.push(("CLAUDE_CODE_MAX_CONTEXT_TOKENS", Some(window.into())));
    }
    if !get("MAX_TOKENS").is_empty() {
        env.push(("CLAUDE_CODE_MAX_OUTPUT_TOKENS", Some(get("MAX_TOKENS"))));
    }
    env
}

fn sh_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn print_env(p: &Provider, as_json: bool) -> i32 {
    let path = config_path(p);
    let (values, _) = read_config(&path);
    let Some(values) = values else {
        return out::fail(&format!(
            "{} isn't set up yet. Run: claude-{} reconfig",
            p.name, p.key
        ));
    };
    if field(p, &values, "API_KEY").is_empty() {
        return out::fail(&format!(
            "No {} API key in {}. Run: claude-{} reconfig",
            p.name,
            shown(&path),
            p.key
        ));
    }
    let env = environment(p, &values);
    if as_json {
        let map: Map<String, Value> = env
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.map_or(Value::Null, Value::String)))
            .collect();
        out::println(&py::dumps_ascii(&Value::Object(map)));
    } else {
        for (name, value) in env {
            out::println(&match value {
                None => format!("unset {name}"),
                Some(v) => format!("export {name}={}", sh_quote(&v)),
            });
        }
    }
    0
}

/// The main model, then any other slot that differs from it.
fn describe(chosen: &[String]) -> String {
    let others: Vec<String> = ["sonnet", "haiku"]
        .iter()
        .zip(chosen.iter().skip(1))
        .filter(|(_, m)| **m != chosen[0])
        .map(|(slot, m)| format!("{slot} {m}"))
        .collect();
    if others.is_empty() {
        chosen[0].clone()
    } else {
        format!("{} ({})", chosen[0], others.join(", "))
    }
}

fn status() -> i32 {
    for p in in_order() {
        let path = config_path(p);
        let (values, _) = read_config(&path);
        let place = if values.is_some() {
            shown(&path)
        } else {
            format!("not set up; run claude-{} reconfig", p.key)
        };
        out::println(&format!(
            "{} (claude-{}, or cl -{} <profile>): {place}",
            p.name, p.key, p.key
        ));
        if let Some(values) = values {
            let key = field(p, &values, "API_KEY");
            let tokens = field(p, &values, "MAX_TOKENS");
            let tail: String = key
                .chars()
                .rev()
                .take(4)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            let base = values
                .get(&format!("{}BASE_URL", p.prefix))
                .and_then(Value::as_str)
                .filter(|b| !b.is_empty());
            out::println(&format!(
                "  key {} · {}{}{}",
                if key.is_empty() {
                    "missing".to_string()
                } else {
                    format!("…{tail}")
                },
                describe(&models(p, &values)),
                if tokens.is_empty() {
                    String::new()
                } else {
                    format!(" · max output {tokens}")
                },
                base.map_or(String::new(), |b| format!(" · {b}"))
            ));
        }
    }
    0
}

/// Read a line without showing it, from a terminal.
fn read_hidden(prompt: &str) -> Option<String> {
    out::print(prompt);
    #[cfg(unix)]
    {
        // SAFETY: termios on stdin, which is a terminal here; the old settings come back after.
        unsafe {
            let mut old: libc::termios = std::mem::zeroed();
            if libc::tcgetattr(0, &mut old) == 0 {
                let mut quiet = old;
                quiet.c_lflag &= !libc::ECHO;
                libc::tcsetattr(0, libc::TCSAFLUSH, &quiet);
                let line = read_line();
                libc::tcsetattr(0, libc::TCSAFLUSH, &old);
                out::println("");
                return line;
            }
        }
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::Console::{
            GetConsoleMode, GetStdHandle, SetConsoleMode, ENABLE_ECHO_INPUT, STD_INPUT_HANDLE,
        };
        // SAFETY: the console's input mode, restored after.
        unsafe {
            let handle = GetStdHandle(STD_INPUT_HANDLE);
            let mut mode = 0;
            if GetConsoleMode(handle, &mut mode) != 0 {
                SetConsoleMode(handle, mode & !ENABLE_ECHO_INPUT);
                let line = read_line();
                SetConsoleMode(handle, mode);
                out::println("");
                return line;
            }
        }
    }
    read_line()
}

/// A line from stdin, without its end; None at the end of input.
fn read_line() -> Option<String> {
    let mut line = String::new();
    match std::io::stdin().lock().read_line(&mut line) {
        Ok(0) | Err(_) => None,
        Ok(_) => Some(line.trim_end_matches(['\n', '\r']).to_string()),
    }
}

/// A line from the user; Enter or the end of input gives "".
fn ask(prompt: &str, secret: bool) -> String {
    use std::io::IsTerminal;
    let line = if secret && std::io::stdin().is_terminal() {
        read_hidden(prompt)
    } else {
        out::print(prompt);
        let _ = std::io::stdout().flush();
        read_line()
    };
    match line {
        Some(l) => py::strip(&l).to_string(),
        None => {
            out::println("");
            String::new()
        }
    }
}

/// Set the fields in updates, keeping every other line, readable only by you.
fn write_config(path: &str, lines: &[String], updates: &[(String, String)]) -> std::io::Result<()> {
    let mut out_lines: Vec<String> = Vec::new();
    let mut done: Vec<&str> = Vec::new();
    for line in lines {
        let is_comment = line.trim_start_matches(py::is_space).starts_with('#');
        if let Some((name, _)) = parse_line(line).filter(|_| !is_comment) {
            if let Some((k, v)) = updates.iter().find(|(k, _)| *k == name) {
                if !done.contains(&k.as_str()) {
                    out_lines.push(format!("{k}=\"{v}\""));
                    done.push(k);
                }
                continue;
            }
        }
        out_lines.push(line.clone());
    }
    for (k, v) in updates {
        if !done.contains(&k.as_str()) {
            out_lines.push(format!("{k}=\"{v}\""));
        }
    }
    std::fs::create_dir_all(ospath::dirname(path))?;
    let tmp = format!("{path}.tmp");
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&tmp)?;
    file.write_all((out_lines.join("\n") + "\n").as_bytes())?;
    drop(file);
    std::fs::rename(&tmp, path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

/// Ask for the API key, the model and the output limit, and save them.
pub fn setup(p: &Provider) -> i32 {
    let path = config_path(p);
    let (values, lines) = read_config(&path);
    let values = values.unwrap_or_default();
    let pre = p.prefix;
    out::println(&format!(
        "Claude Code on {}. Settings go in {}",
        p.name,
        shown(&path)
    ));
    let current = field(p, &values, "API_KEY");
    out::println(&format!("Get an API key from {}", p.key_from));
    let key = loop {
        let tail: String = current
            .chars()
            .rev()
            .take(4)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        let keeps = if current.is_empty() {
            String::new()
        } else {
            format!(" (Enter keeps …{tail})")
        };
        let answer = ask(&format!("API key{keeps}: "), true);
        let key = if answer.is_empty() {
            current.clone()
        } else {
            answer
        };
        if key.is_empty() {
            return out::fail("Nothing saved: an API key is needed");
        }
        if !key
            .chars()
            .any(|c| py::is_space(c) || c == '"' || c == '\'' || c == '\\')
        {
            break key;
        }
        out::println("That has spaces, quotes or backslashes in it; paste just the key.");
    };
    let mut chosen: Vec<String> = models(p, &values);
    out::println("Model:");
    for (n, (label, about, _)) in p.choices.iter().enumerate() {
        out::println(&format!("  {}) {label:<26} {about}", n + 1));
    }
    loop {
        let answer = ask(
            &format!(
                "Choose 1-{}, or Enter to keep {}: ",
                p.choices.len(),
                describe(&chosen)
            ),
            false,
        );
        if answer.is_empty() {
            break;
        }
        if let Some(n) = answer.parse::<usize>().ok().filter(|n| {
            answer.bytes().all(|b| b.is_ascii_digit()) && (1..=p.choices.len()).contains(n)
        }) {
            chosen = p.choices[n - 1].2.iter().map(|m| m.to_string()).collect();
            break;
        }
        out::println("Type a number from the list, or press Enter.");
    }
    let mut tokens = field(p, &values, "MAX_TOKENS");
    loop {
        let keep = if tokens.is_empty() {
            "Claude Code's default".to_string()
        } else {
            tokens.clone()
        };
        let answer = ask(&format!("Max output tokens per reply, e.g. 64000 (Enter keeps {keep}; - for Claude Code's default): "), false);
        if answer == "-" {
            tokens = String::new();
        } else if !answer.is_empty() && answer.bytes().all(|b| b.is_ascii_digit()) {
            tokens = answer;
        } else if !answer.is_empty() {
            out::println("Type a number, - or press Enter.");
            continue;
        }
        break;
    }
    let mut updates: Vec<(String, String)> = vec![(format!("{pre}API_KEY"), key)];
    updates.extend(
        p.models
            .iter()
            .zip(chosen)
            .map(|(slot, m)| (format!("{pre}{slot}"), m)),
    );
    updates.push((format!("{pre}MAX_TOKENS"), tokens));
    if let Err(e) = write_config(&path, &lines, &updates) {
        return out::fail(&e.to_string());
    }
    out::println(&format!(
        "Saved. Start Claude Code on {} with claude-{}, or with a profile: cl -{} <profile>",
        p.name, p.key, p.key
    ));
    0
}

pub fn main(argv: &[String], prog: &str) -> i32 {
    let parser = Parser::new(
        prog,
        "Settings for running Claude Code on Z.ai (z) or Kimi (k) instead of Anthropic.",
    )
    .subcommands(
        "command",
        &[
            ("setup", "ask for the API key and model, and save them"),
            (
                "env",
                "the environment that points Claude Code at the provider",
            ),
        ],
    );
    let args = match parser.parse(argv) {
        Ok(a) => a,
        Err(stop) => return stop.exit_code(),
    };
    let command = args.value("command").map(String::from);
    let sub = |name: &str, json: bool| {
        let p = Parser::new(&format!("{prog} {name}"), "").positional_as(
            "provider",
            "{k,z}",
            Count::One,
            "",
        );
        if json {
            p.flag(
                &["--json"],
                "json",
                "as JSON, for scripts, with null for unset",
            )
        } else {
            p
        }
    };
    let pick = |parser: &Parser, a: &crate::args::Args| -> Result<&'static Provider, i32> {
        let key = a.value("provider").unwrap_or("");
        provider(key).ok_or_else(|| {
            parser
                .error(&format!(
                    "argument provider: invalid choice: '{key}' (choose from 'k', 'z')"
                ))
                .exit_code()
        })
    };
    match command.as_deref() {
        Some(name @ ("setup" | "env")) => {
            let p = sub(name, name == "env");
            let a = match p.parse_known(&args.rest) {
                Ok((_, extras)) if !extras.is_empty() => {
                    return parser
                        .error(&format!("unrecognized arguments: {}", extras.join(" ")))
                        .exit_code()
                }
                Ok((a, _)) => a,
                Err(stop) => return stop.exit_code(),
            };
            let chosen = match pick(&p, &a) {
                Ok(c) => c,
                Err(code) => return code,
            };
            if name == "setup" {
                setup(chosen)
            } else {
                print_env(chosen, a.flag("json"))
            }
        }
        _ => status(),
    }
}
