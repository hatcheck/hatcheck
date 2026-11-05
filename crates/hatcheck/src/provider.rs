//! hatcheck provider ...: the settings for running Claude Code on Z.ai (GLM models), Kimi,
//! MiniMax or DeepSeek instead of Anthropic, behind claude-z, claude-k, claude-m, claude-d and
//! cl -z, -k, -m and -D. docs/agents.md's `claude-z`, `claude-k`, `claude-m`, `claude-d` section
//! describes the config files and what's sent where.

use crate::args::{Count, Parser};
use crate::ospath;
use crate::out;
use crate::paths;
use crate::py;
use serde_json::{Map, Value};
use std::io::{BufRead, Write};

/// A provider: the flag cl takes for it, its name, the prefix of its config fields, its endpoint,
/// where to get a key, its model slots, the model choices, each filling all the slots, the first
/// being its flagship and the default, and where Claude Code compacts a session on one of its
/// [1m] models, as the provider's Claude Code guide sets it.
pub struct Provider {
    pub key: &'static str,
    /// -z, -k and -m, but -D for DeepSeek: cl leaves -d to Claude Code, whose -d is --debug, so
    /// that a debug run typed as cl -d can't send a session to another provider.
    pub cl_flag: &'static str,
    pub name: &'static str,
    pub prefix: &'static str,
    pub url: &'static str,
    pub key_from: &'static str,
    pub models: &'static [&'static str],
    pub choices: &'static [(&'static str, &'static str, &'static [&'static str])],
    pub compact_at: &'static str,
}

pub const PROVIDERS: [Provider; 4] = [
    Provider {
        key: "k",
        cl_flag: "-k",
        name: "Kimi",
        prefix: "CK_",
        url: "https://api.kimi.ai/coding/",
        key_from: "the Kimi Code Console, https://www.kimi.com/code (Create API Key)",
        models: &["MODEL"],
        // Kimi's six model IDs, as its model list and its Claude Code guide's table by plan had
        // them on 2026-11-05, with [1m] wherever the guide gives a model 1M of context: K3.1 Pro
        // and K3 on Pro or Allegretto and up (256k below), K3.1 Pro Highspeed on every plan that
        // has it, and K2.8 Preview on every plan. k3-256k, K3 at 256k, takes half k3's quota, so
        // it replaced plain k3; K2.7 Code Highspeed has 256k and no effort setting.
        choices: &[
            (
                "k3.1-pro[1m]",
                "K3.1 Pro, the flagship, 1M context (Pro or Allegretto plan and up)",
                &["k3.1-pro[1m]"],
            ),
            (
                "k3.1-pro",
                "K3.1 Pro, 256k context (Plus or Moderato and up)",
                &["k3.1-pro"],
            ),
            (
                "k3.1-pro-highspeed[1m]",
                "K3.1 Pro, 2-3x faster at twice the quota, 1M context (Pro or Allegretto and up)",
                &["k3.1-pro-highspeed[1m]"],
            ),
            (
                "k3[1m]",
                "K3, 1M context (Pro or Allegretto plan and up)",
                &["k3[1m]"],
            ),
            (
                "k3-256k",
                "K3, 256k context, half k3's quota (Plus or Moderato and up)",
                &["k3-256k"],
            ),
            (
                "kimi-for-coding[1m]",
                "K2.8 Preview, 1M context (every coding plan)",
                &["kimi-for-coding[1m]"],
            ),
            (
                "kimi-for-coding-highspeed",
                "K2.7 Code Highspeed, about 6x faster at 3x the quota, 256k (Pro or Allegretto and up)",
                &["kimi-for-coding-highspeed"],
            ),
        ],
        compact_at: "1048576",
    },
    Provider {
        key: "z",
        cl_flag: "-z",
        name: "Z.ai",
        prefix: "CZ_",
        url: "https://api.z.ai/api/anthropic",
        key_from: "https://z.ai/manage-apikey/apikey-list",
        models: &["MODEL_OPUS", "MODEL_SONNET", "MODEL_HAIKU"],
        // As Z.ai's Claude Code guide pairs them (checked 2026-11-05): GLM-5.3 and GLM-5.3-Flash
        // are the two models every GLM Coding Plan has; its model list's glm-5.3-flashx isn't
        // on the plan, and older names are served by these two.
        choices: &[
            (
                "glm-5.3[1m]",
                "flagship, 1M context, with glm-5.3-flash[1m] for quick tasks",
                &["glm-5.3[1m]", "glm-5.3[1m]", "glm-5.3-flash[1m]"],
            ),
            (
                "glm-5.3-flash[1m]",
                "fast, multimodal, cheapest, 1M context",
                &[
                    "glm-5.3-flash[1m]",
                    "glm-5.3-flash[1m]",
                    "glm-5.3-flash[1m]",
                ],
            ),
        ],
        compact_at: "1000000",
    },
    Provider {
        key: "m",
        cl_flag: "-m",
        name: "MiniMax",
        prefix: "CM_",
        url: "https://api.minimax.io/anthropic",
        key_from:
            "the MiniMax console, https://platform.minimax.io/console/plan (the M Plan API key)",
        models: &["MODEL"],
        // As MiniMax's Claude Code guide sets it (checked 2026-11-05): M3.1 Flash, the M Plan's
        // one text model on every tier, with 1M of context, compacting at 512K, which MiniMax
        // recommends for everyday coding. Its models page also lists MiniMax-M3, the generation
        // before, which isn't on the M Plan.
        choices: &[(
            "MiniMax-M3.1-Flash[1m]",
            "M3.1 Flash, 1M context, compacting at 512K (every M Plan tier)",
            &["MiniMax-M3.1-Flash[1m]"],
        )],
        compact_at: "524288",
    },
    Provider {
        key: "d",
        cl_flag: "-D",
        name: "DeepSeek",
        prefix: "CD_",
        url: "https://api.deepseek.com/anthropic",
        key_from: "the DeepSeek Platform, https://platform.deepseek.com/api_keys",
        models: &["MODEL", "MODEL_HAIKU"],
        // DeepSeek's two models, as its models page and its Claude Code guide had them on
        // 2026-11-05: the model, with deepseek-flash for the haiku tier and subagents, as the
        // guide sets them, compacting at 768K of the 1M both have. The guide names Flash alone;
        // V4 Pro, the flagship, is what it maps Claude's opus models to.
        choices: &[
            (
                "deepseek-v4-pro[1m]",
                "DeepSeek V4 Pro, the flagship, 1M context, with Flash for quick tasks",
                &["deepseek-v4-pro[1m]", "deepseek-flash"],
            ),
            (
                "deepseek-flash[1m]",
                "DeepSeek V4.1 Flash, 1M context, a quarter to a third of Pro's price",
                &["deepseek-flash[1m]", "deepseek-flash"],
            ),
        ],
        compact_at: "786432",
    },
];

/// The providers in the order the setup and the status list them: z, k, m, d.
fn in_order() -> [&'static Provider; 4] {
    [&PROVIDERS[1], &PROVIDERS[0], &PROVIDERS[2], &PROVIDERS[3]]
}

pub fn provider(key: &str) -> Option<&'static Provider> {
    PROVIDERS.iter().find(|p| p.key == key)
}

/// The provider cl's first argument names: -z, -k, -m or -D.
pub fn by_cl_flag(arg: &str) -> Option<&'static Provider> {
    PROVIDERS.iter().find(|p| p.cl_flag == arg)
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

/// Claude Code's effort levels, as --effort takes them.
const EFFORTS: [&str; 5] = ["low", "medium", "high", "xhigh", "max"];

/// The effort the launchers start Claude Code at: the config's, max when it names none, and None
/// for auto, which leaves it to Claude Code. Every provider here takes max: Kimi and Z.ai map
/// xhigh to it and medium to high, and MiniMax and DeepSeek set it in their guides.
pub fn effort(p: &Provider, values: &Map<String, Value>) -> Option<String> {
    match field(p, values, "EFFORT").to_lowercase().as_str() {
        "" => Some("max".into()),
        "auto" => None,
        level => Some(level.into()),
    }
}

/// Whether the models saved are none of the choices offered now, such as a model the provider
/// has since replaced. A slot not saved takes the default's model, so a config with no models is
/// the default.
pub fn outdated(p: &Provider, values: &Map<String, Value>) -> bool {
    let chosen = models(p, values);
    !p.choices.iter().any(|(_, _, slots)| chosen == *slots)
}

/// Claude Code's arguments with --effort first, before any command of Claude Code's, unless they
/// name one already. As an argument it's the session's starting point, which /effort changes;
/// CLAUDE_CODE_EFFORT_LEVEL would hold every session to it, /effort saying "Not applied".
pub fn with_effort(p: &Provider, values: &Map<String, Value>, args: Vec<String>) -> Vec<String> {
    let named = args
        .iter()
        .any(|a| a == "--effort" || a.starts_with("--effort="));
    match effort(p, values) {
        Some(level) if !named => ["--effort".to_string(), level]
            .into_iter()
            .chain(args)
            .collect(),
        _ => args,
    }
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
            env.push(("CLAUDE_CODE_AUTO_COMPACT_WINDOW", Some(p.compact_at.into())));
        }
    } else {
        // The model in every slot, but for DeepSeek's lighter one in the haiku tier and for
        // subagents, as its guide sets them.
        let model = chosen[0].clone();
        let quick = chosen.get(1).cloned().unwrap_or_else(|| model.clone());
        for name in [
            "ANTHROPIC_MODEL",
            "ANTHROPIC_DEFAULT_FABLE_MODEL",
            "ANTHROPIC_DEFAULT_OPUS_MODEL",
            "ANTHROPIC_DEFAULT_SONNET_MODEL",
        ] {
            env.push((name, Some(model.clone())));
        }
        env.push(("ANTHROPIC_DEFAULT_HAIKU_MODEL", Some(quick.clone())));
        env.push(("CLAUDE_CODE_SUBAGENT_MODEL", Some(quick)));
        let long = model.ends_with("[1m]");
        if p.key == "k" {
            // Kimi's guide sets both to the context the plan gives: 1M, or 256k.
            let window = if long { p.compact_at } else { "262144" };
            env.push(("CLAUDE_CODE_AUTO_COMPACT_WINDOW", Some(window.into())));
            env.push(("CLAUDE_CODE_MAX_CONTEXT_TOKENS", Some(window.into())));
        } else if long {
            // MiniMax's and DeepSeek's set where to compact, short of the 1M that [1m] gives.
            env.push(("CLAUDE_CODE_AUTO_COMPACT_WINDOW", Some(p.compact_at.into())));
        }
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

/// The main model, then any other slot that differs from it, by its tier: sonnet or haiku.
fn describe(p: &Provider, chosen: &[String]) -> String {
    let others: Vec<String> = p
        .models
        .iter()
        .zip(chosen)
        .skip(1)
        .filter(|(_, m)| **m != chosen[0])
        .map(|(slot, m)| {
            let tier = slot.trim_start_matches("MODEL_").to_lowercase();
            format!("{tier} {m}")
        })
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
            "{} (claude-{}, or cl {} <profile>): {place}",
            p.name, p.key, p.cl_flag
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
                "  key {} · {}{} · effort {}{}{}",
                if key.is_empty() {
                    "missing".to_string()
                } else {
                    format!("…{tail}")
                },
                describe(p, &models(p, &values)),
                if outdated(p, &values) {
                    " (not offered now)"
                } else {
                    ""
                },
                effort(p, &values).unwrap_or_else(|| "auto".into()),
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

/// Read a line without showing it, from a terminal. The prompt shows once echo is off, since
/// turning it off drops what was typed before: what's typed after the prompt is kept.
fn read_hidden(prompt: &str) -> Option<String> {
    #[cfg(unix)]
    {
        // SAFETY: termios on stdin, which is a terminal here; the old settings come back after.
        unsafe {
            let mut old: libc::termios = std::mem::zeroed();
            if libc::tcgetattr(0, &mut old) == 0 {
                let mut quiet = old;
                quiet.c_lflag &= !libc::ECHO;
                libc::tcsetattr(0, libc::TCSAFLUSH, &quiet);
                out::print(prompt);
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
                out::print(prompt);
                let line = read_line();
                SetConsoleMode(handle, mode);
                out::println("");
                return line;
            }
        }
    }
    out::print(prompt);
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
    let prompt = out::Paint::new().prompt(prompt);
    let line = if secret && std::io::stdin().is_terminal() {
        read_hidden(&prompt)
    } else {
        out::print(&prompt);
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

/// Why the setup runs: asked for, with reconfig or hatcheck provider setup, or from a launcher,
/// which runs it first on a terminal when there's no key yet, or when the models saved aren't
/// among those offered now.
#[derive(Clone, Copy, PartialEq)]
pub enum Why {
    Asked,
    FirstRun,
    Outdated,
}

/// Ask for the API key, the model and the output limit, and save them.
pub fn setup(p: &Provider) -> i32 {
    setup_for(p, Why::Asked)
}

/// The config a launcher runs Claude Code with. On a terminal, one with no key gets the setup
/// first, and one whose models aren't offered now gets it again, Enter choosing the flagship;
/// elsewhere the first is refused with how to set it up, and the second runs as saved, saying so.
pub fn ready(p: &Provider, me: &str) -> Result<Map<String, Value>, i32> {
    let path = config_path(p);
    let values = read_config(&path).0;
    let why = match &values {
        Some(v) if field(p, v, "API_KEY").is_empty() => Why::FirstRun,
        Some(v) if outdated(p, v) => Why::Outdated,
        Some(v) => return Ok(v.clone()),
        None => Why::FirstRun,
    };
    if crate::move_cmd::interactive() {
        return match setup_for(p, why) {
            0 => Ok(read_config(&path).0.unwrap_or_default()),
            code => Err(code),
        };
    }
    match values {
        None => Err(out::fail(&format!(
            "{} isn't set up yet. Run: claude-{} reconfig",
            p.name, p.key
        ))),
        Some(_) if why == Why::FirstRun => Err(out::fail(&format!(
            "No {} API key in {}. Run: claude-{} reconfig",
            p.name,
            shown(&path),
            p.key
        ))),
        Some(v) => {
            out::eprint(&format!(
                "{}\n",
                out::Paint::stderr().warning(&format!(
                    "{me}: {} isn't among the models offered now; claude-{} reconfig picks one",
                    describe(p, &models(p, &v)),
                    p.key
                ))
            ));
            Ok(v)
        }
    }
}

fn setup_for(p: &Provider, why: Why) -> i32 {
    let path = config_path(p);
    let (values, lines) = read_config(&path);
    let values = values.unwrap_or_default();
    let pre = p.prefix;
    out::heading(&match why {
        Why::FirstRun => format!(
            "Claude Code on {} isn't set up yet: the key and model first. Settings go in {}",
            p.name,
            shown(&path)
        ),
        Why::Outdated => format!(
            "Claude Code on {}: {}, saved in {}, isn't among the models offered now. Choose again",
            p.name,
            describe(p, &models(p, &values)),
            shown(&path)
        ),
        Why::Asked => format!("Claude Code on {}. Settings go in {}", p.name, shown(&path)),
    });
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
    // The models saved, kept on Enter while they're among the choices; otherwise, or with none
    // saved, Enter takes the first, the flagship.
    let saved = p
        .models
        .iter()
        .any(|slot| !field(p, &values, slot).is_empty());
    let flagship: Vec<String> = p.choices[0].2.iter().map(|m| m.to_string()).collect();
    let mut chosen: Vec<String> = if outdated(p, &values) {
        flagship.clone()
    } else {
        models(p, &values)
    };
    let keeps = saved && !outdated(p, &values);
    if p.choices.len() == 1 {
        // One model to offer: nothing to ask.
        let (label, about, _) = p.choices[0];
        out::println(&format!("Model: {label}  {about}"));
    } else {
        out::heading("Model:");
        for (n, (label, about, _)) in p.choices.iter().enumerate() {
            out::println(&format!("  {}) {label:<26} {about}", n + 1));
        }
    }
    while p.choices.len() > 1 {
        let enter = if keeps {
            format!("Enter to keep {}", describe(p, &chosen))
        } else {
            format!("Enter for 1, {}", p.choices[0].0)
        };
        let answer = ask(
            &format!("Choose 1-{}, or {enter}: ", p.choices.len()),
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
    let mut level = effort(p, &values).unwrap_or_else(|| "auto".into());
    loop {
        let answer = ask(
            &format!("Reasoning effort: low, medium, high, xhigh or max, or auto for Claude Code's own (Enter keeps {level}): "),
            false,
        )
        .to_lowercase();
        if EFFORTS.contains(&answer.as_str()) || answer == "auto" {
            level = answer;
        } else if !answer.is_empty() {
            out::println("Type one of low, medium, high, xhigh, max or auto, or press Enter.");
            continue;
        }
        break;
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
    updates.push((format!("{pre}EFFORT"), level));
    updates.push((format!("{pre}MAX_TOKENS"), tokens));
    if let Err(e) = write_config(&path, &lines, &updates) {
        return out::fail(&e.to_string());
    }
    out::success(&if why == Why::Asked {
        format!(
            "Saved. Start Claude Code on {} with claude-{}, or with a profile: cl {} <profile>",
            p.name, p.key, p.cl_flag
        )
    } else {
        format!(
            "Saved. Starting Claude Code on {}; claude-{} reconfig changes these.",
            p.name, p.key
        )
    });
    0
}

pub fn main(argv: &[String], prog: &str) -> i32 {
    let parser = Parser::new(
        prog,
        "Settings for running Claude Code on Z.ai (z), Kimi (k), MiniMax (m) or DeepSeek (d) instead of Anthropic.",
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
            "{z,k,m,d}",
            Count::One,
            "",
        );
        if json {
            p.flag(
                &["-j", "--json"],
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
                    "argument provider: invalid choice: '{key}' (choose from 'z', 'k', 'm', 'd')"
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
