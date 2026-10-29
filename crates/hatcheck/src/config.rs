//! hatcheck's own config file: the session list's defaults, and the agents and folders to leave
//! out. It's ~/.config/hatcheck/config.toml ($XDG_CONFIG_HOME's, or %APPDATA%\hatcheck\config.toml
//! on Windows), beside the bindings, or the file HATCHECK_CONFIG names. cli.rs reads it before
//! every subcommand, and a file that can't be read stops the command, since one silently ignored
//! would seem to apply. It's a plain subset of TOML, read here: comments, [tables], and
//! key = value with strings, numbers, booleans and arrays of them.
//! docs/commands.md's `config.toml` section documents it.

use crate::agent::Agent;
use crate::ospath;
use crate::paths;
use crate::py::{self, s};
use serde_json::{json, Map, Value};
use std::sync::Mutex;

/// What the file sets; None where it says nothing.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Config {
    /// Where it was looked for, and whether it was there.
    pub path: String,
    pub found: bool,
    pub details: Option<bool>,
    pub by_cost: Option<bool>,
    /// As written: 24, 1.5, 12h or 3d.
    pub hours: Option<String>,
    pub per_folder: Option<usize>,
    pub full: Option<bool>,
    pub all_folders: Option<bool>,
    pub usage: Option<bool>,
    /// The only folders the list shows, when any are named, and folders it never shows: paths
    /// with ~ expanded, * for any part of a name and ? for one character, each covering the
    /// folder it names and every folder under it.
    pub list_folders: Vec<String>,
    pub ignore_folders: Vec<String>,
    pub ignore_agents: Vec<Agent>,
}

/// The config of the command that's running, installed by cli.rs; the defaults until then.
static CURRENT: Mutex<Option<Config>> = Mutex::new(None);

/// The file's path: $HATCHECK_CONFIG, or config.toml in hatcheck's config folder.
pub fn path() -> String {
    std::env::var("HATCHECK_CONFIG")
        .ok()
        .filter(|v| !v.is_empty())
        .map(|v| ospath::expanduser(&v))
        .unwrap_or_else(|| ospath::join(&paths::config_dir(), "config.toml"))
}

/// The file, read, or the defaults when there is none. Err says what's wrong with it, with the
/// file and, for a line that doesn't parse, the line.
pub fn load() -> Result<Config, String> {
    let path = path();
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Config {
                path,
                ..Default::default()
            })
        }
        Err(e) => return Err(format!("can't read {}: {e}", paths::shell_path(&path))),
    };
    let shown = paths::shell_path(&path);
    let table = parse(&text).map_err(|e| format!("{shown}, {e}"))?;
    let mut config = from_table(&table).map_err(|e| format!("{shown}: {e}"))?;
    config.path = path;
    config.found = true;
    Ok(config)
}

/// Make a config the one every command reads.
pub fn install(config: Config) {
    *CURRENT.lock().unwrap_or_else(|e| e.into_inner()) = Some(config);
}

fn with<T>(f: impl FnOnce(&Config) -> T) -> T {
    let guard = CURRENT.lock().unwrap_or_else(|e| e.into_inner());
    match guard.as_ref() {
        Some(c) => f(c),
        None => f(&Config::default()),
    }
}

/// The config every command reads.
pub fn current() -> Config {
    with(Clone::clone)
}

/// Whether the file leaves an agent out: hatcheck then looks for none of its profiles or homes.
pub fn ignores(agent: Agent) -> bool {
    with(|c| c.ignore_agents.contains(&agent))
}

/// Whether the file leaves a session's folder out of the list: not among the folders it lists,
/// when it lists any, or among those it ignores, which win.
pub fn folder_left_out(folder: &str) -> bool {
    with(|c| c.leaves_out(folder))
}

/// Whether a pattern names a folder, or one above it: the pattern's parts, with * for any run
/// of characters and ? for one, against as many of the folder's first parts, either separator,
/// and either case on Windows.
pub fn pattern_covers(pattern: &str, folder: &str) -> bool {
    let parts = |p: &str| -> Vec<String> {
        p.split(ospath::is_sep)
            .filter(|x| !x.is_empty())
            .map(|x| {
                if ospath::WINDOWS {
                    x.to_lowercase()
                } else {
                    x.to_string()
                }
            })
            .collect()
    };
    let (want, have) = (parts(pattern), parts(folder));
    !want.is_empty()
        && want.len() <= have.len()
        && want.iter().zip(&have).all(|(w, h)| wild_match(w, h))
}

/// fnmatch's * and ? over one name.
fn wild_match(pattern: &str, text: &str) -> bool {
    let (p, t): (Vec<char>, Vec<char>) = (pattern.chars().collect(), text.chars().collect());
    let (mut i, mut j) = (0, 0);
    let (mut star, mut mark) = (None, 0);
    while j < t.len() {
        if i < p.len() && (p[i] == '?' || p[i] == t[j]) {
            i += 1;
            j += 1;
        } else if i < p.len() && p[i] == '*' {
            star = Some(i);
            mark = j;
            i += 1;
        } else if let Some(k) = star {
            i = k + 1;
            mark += 1;
            j = mark;
        } else {
            return false;
        }
    }
    while i < p.len() && p[i] == '*' {
        i += 1;
    }
    i == p.len()
}

impl Config {
    /// Whether a folder is left out of the list: see folder_left_out.
    pub fn leaves_out(&self, folder: &str) -> bool {
        let covered = |patterns: &[String]| patterns.iter().any(|p| pattern_covers(p, folder));
        (!self.list_folders.is_empty() && !covered(&self.list_folders))
            || covered(&self.ignore_folders)
    }

    /// What the file sets, a line each, for hatcheck doctor.
    pub fn describe(&self) -> Vec<String> {
        let mut lines = Vec::new();
        let mut list = Vec::new();
        let either = |v: Option<bool>, yes: &str, no: &str| match v {
            Some(true) => Some(yes.to_string()),
            Some(false) => Some(no.to_string()),
            None => None,
        };
        list.extend(either(self.details, "details", "no details"));
        list.extend(either(self.by_cost, "by cost", "by time"));
        list.extend(self.hours.clone());
        list.extend(self.per_folder.map(|n| format!("{n} per folder")));
        list.extend(either(self.full, "full commands", "cl, cx and ck"));
        list.extend(either(self.all_folders, "every folder", "this folder's"));
        list.extend(either(self.usage, "usage", "no usage"));
        if !self.list_folders.is_empty() {
            list.push(format!("folders {}", shown(&self.list_folders)));
        }
        if !list.is_empty() {
            lines.push(format!("list: {}", list.join(", ")));
        }
        let mut ignore = Vec::new();
        if !self.ignore_agents.is_empty() {
            ignore.push(format!(
                "agents {}",
                self.ignore_agents
                    .iter()
                    .map(|a| a.key())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        if !self.ignore_folders.is_empty() {
            ignore.push(format!("folders {}", shown(&self.ignore_folders)));
        }
        if !ignore.is_empty() {
            lines.push(format!("ignore: {}", ignore.join("; ")));
        }
        if lines.is_empty() {
            lines.push("nothing set".into());
        }
        lines
    }

    /// As hatcheck doctor --json shows it.
    pub fn to_json(&self) -> Value {
        json!({
            "path": ospath::abspath(&self.path), "found": self.found,
            "list": {"details": self.details, "by_cost": self.by_cost, "hours": self.hours, "per_folder": self.per_folder,
                     "full": self.full, "all_folders": self.all_folders, "usage": self.usage,
                     "folders": self.list_folders.iter().map(|f| ospath::abspath(f)).collect::<Vec<_>>()},
            "ignore": {"agents": self.ignore_agents.iter().map(|a| a.key()).collect::<Vec<_>>(),
                       "folders": self.ignore_folders.iter().map(|f| ospath::abspath(f)).collect::<Vec<_>>()},
        })
    }
}

const LIST_KEYS: [&str; 8] = [
    "details",
    "by_cost",
    "hours",
    "per_folder",
    "full",
    "all_folders",
    "usage",
    "folders",
];

/// Folder patterns as doctor shows them, with ~ back.
fn shown(patterns: &[String]) -> String {
    patterns
        .iter()
        .map(|f| paths::tilde_path(f))
        .collect::<Vec<_>>()
        .join(", ")
}

/// A setting's list of folder patterns, each expanded to a path, or what's wrong with it.
fn folder_patterns(value: &Value, setting: &str) -> Result<Vec<String>, String> {
    let Value::Array(items) = value else {
        return Err(format!("{setting} takes a list of folders"));
    };
    let mut patterns = Vec::new();
    for item in items {
        let Some(pattern) = s(item).map(str::trim) else {
            return Err(format!("{setting} takes a list of folders"));
        };
        if pattern.is_empty() {
            return Err(format!("{setting}: a folder is empty"));
        }
        patterns.push(ospath::abspath(&ospath::expanduser(pattern)));
    }
    Ok(patterns)
}

/// The config a parsed file makes, or what's wrong with a setting.
fn from_table(table: &Map<String, Value>) -> Result<Config, String> {
    let mut config = Config::default();
    for (name, section) in table {
        let Value::Object(keys) = section else {
            return Err(format!(
                "'{name}' isn't a section; the sections are [list] and [ignore]"
            ));
        };
        match name.as_str() {
            "list" => {
                for (key, value) in keys {
                    let flag = || match value {
                        Value::Bool(b) => Ok(Some(*b)),
                        _ => Err(format!("list.{key} takes true or false")),
                    };
                    match key.as_str() {
                        "details" => config.details = flag()?,
                        "by_cost" => config.by_cost = flag()?,
                        "full" => config.full = flag()?,
                        "all_folders" => config.all_folders = flag()?,
                        "usage" => config.usage = flag()?,
                        "hours" => {
                            let text = match value {
                                Value::String(t) => t.clone(),
                                Value::Number(_) => py::str_of(value),
                                _ => String::new(),
                            };
                            if crate::sessions::parse_hours(&text).is_none() {
                                return Err(
                                    "list.hours takes hours like 24, 1.5, 12h or 3d".to_string()
                                );
                            }
                            config.hours = Some(text);
                        }
                        "per_folder" => match value.as_u64() {
                            Some(n) if n >= 1 => config.per_folder = Some(n as usize),
                            _ => {
                                return Err(
                                    "list.per_folder takes a whole number of 1 or more".to_string()
                                )
                            }
                        },
                        "folders" => config.list_folders = folder_patterns(value, "list.folders")?,
                        other => {
                            return Err(format!(
                                "list.{other} isn't a setting; [list] takes {}",
                                and_list(&LIST_KEYS)
                            ))
                        }
                    }
                }
            }
            "ignore" => {
                for (key, value) in keys {
                    let strings = |what: &str| -> Result<Vec<String>, String> {
                        match value {
                            Value::Array(items) if items.iter().all(Value::is_string) => Ok(items
                                .iter()
                                .filter_map(|i| s(i).map(String::from))
                                .collect()),
                            _ => Err(format!("ignore.{key} takes a list of {what}")),
                        }
                    };
                    match key.as_str() {
                        "agents" => {
                            for name in strings("agents")? {
                                let Some(agent) = Agent::from_key(name.trim()) else {
                                    return Err(format!(
                                        "ignore.agents: '{name}' isn't an agent; they're {}",
                                        and_list(
                                            &Agent::ALL.iter().map(|a| a.key()).collect::<Vec<_>>()
                                        )
                                    ));
                                };
                                if !config.ignore_agents.contains(&agent) {
                                    config.ignore_agents.push(agent);
                                }
                            }
                        }
                        "folders" => {
                            config.ignore_folders = folder_patterns(value, "ignore.folders")?
                        }
                        other => {
                            return Err(format!(
                                "ignore.{other} isn't a setting; [ignore] takes agents and folders"
                            ))
                        }
                    }
                }
            }
            other => {
                return Err(format!(
                    "'{other}' isn't a section; the sections are [list] and [ignore]"
                ))
            }
        }
    }
    Ok(config)
}

/// Items as a sentence lists them: a, b and c.
fn and_list(items: &[&str]) -> String {
    match items.len() {
        0 => String::new(),
        1 => items[0].to_string(),
        n => format!("{} and {}", items[..n - 1].join(", "), items[n - 1]),
    }
}

/// A line without its comment: a # outside quotes ends it.
fn strip_comment(line: &str) -> &str {
    let (mut basic, mut literal, mut escaped) = (false, false, false);
    for (i, c) in line.char_indices() {
        match c {
            '\\' if basic && !escaped => {
                escaped = true;
                continue;
            }
            '"' if !literal && !escaped => basic = !basic,
            '\'' if !basic => literal = !literal,
            '#' if !basic && !literal => return &line[..i],
            _ => {}
        }
        escaped = false;
    }
    line
}

/// How many [ are open at the end of text, outside quotes.
fn open_brackets(text: &str) -> i64 {
    let (mut basic, mut literal, mut escaped, mut depth) = (false, false, false, 0i64);
    for c in text.chars() {
        match c {
            '\\' if basic && !escaped => {
                escaped = true;
                continue;
            }
            '"' if !literal && !escaped => basic = !basic,
            '\'' if !basic => literal = !literal,
            '[' if !basic && !literal => depth += 1,
            ']' if !basic && !literal => depth -= 1,
            _ => {}
        }
        escaped = false;
    }
    depth
}

/// The file as tables of keys, read as a plain subset of TOML: comments, [table] headers (a
/// dotted one nested), and key = value with a "string" (with \", \\, \n, \t and \uXXXX), a
/// 'string' as it is, an integer, a float, true or false, or an array of those, over several
/// lines when it has to be. Err names the line.
pub fn parse(text: &str) -> Result<Map<String, Value>, String> {
    let mut root = Map::new();
    let mut table: Vec<String> = Vec::new();
    let mut lines = text.lines().enumerate();
    while let Some((n, raw)) = lines.next() {
        let at = |message: String| format!("line {}: {message}", n + 1);
        let line = strip_comment(raw).trim();
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix('[') {
            let name = rest
                .strip_suffix(']')
                .filter(|_| !rest.starts_with('['))
                .ok_or_else(|| at("expected [table]".into()))?
                .trim();
            let parts: Vec<String> = name
                .split('.')
                .map(|p| p.trim().trim_matches('"').to_string())
                .collect();
            if parts.iter().any(String::is_empty) {
                return Err(at("expected [table]".into()));
            }
            place(&mut root, &parts).map_err(at)?;
            table = parts;
            continue;
        }
        let (key, value) = line
            .split_once('=')
            .ok_or_else(|| at("expected key = value".into()))?;
        let key = key.trim().trim_matches('"');
        if key.is_empty() || key.contains('.') || key.contains(' ') {
            return Err(at(format!(
                "'{key}' isn't a key; a dotted one goes under a [table]"
            )));
        }
        let mut value = value.trim().to_string();
        while value.starts_with('[') && open_brackets(&value) > 0 {
            // An array over several lines: on to where its brackets balance.
            let (_, more) = lines
                .next()
                .ok_or_else(|| at("an array that doesn't end".into()))?;
            value.push('\n');
            value.push_str(strip_comment(more).trim());
        }
        let parsed = parse_value(&value).map_err(at)?;
        let target = place(&mut root, &table).map_err(at)?;
        if target.contains_key(key) {
            return Err(at(format!("'{key}' is set twice")));
        }
        target.insert(key.to_string(), parsed);
    }
    Ok(root)
}

/// The table a path of names leads to, made on the way.
fn place<'a>(
    root: &'a mut Map<String, Value>,
    path: &[String],
) -> Result<&'a mut Map<String, Value>, String> {
    let mut here = root;
    for name in path {
        let next = here
            .entry(name.clone())
            .or_insert_with(|| Value::Object(Map::new()));
        here = match next {
            Value::Object(m) => m,
            _ => return Err(format!("'{name}' is a value, not a table")),
        };
    }
    Ok(here)
}

/// A "string", with its escapes read, and what follows its closing quote.
fn basic_string(text: &str) -> Result<(String, &str), String> {
    let mut out = String::new();
    let mut chars = text.char_indices().skip(1);
    while let Some((i, c)) = chars.next() {
        match c {
            '"' => return Ok((out, &text[i + 1..])),
            '\\' => {
                let (_, e) = chars.next().ok_or("a string that doesn't end")?;
                match e {
                    '"' | '\\' => out.push(e),
                    'n' => out.push('\n'),
                    't' => out.push('\t'),
                    'r' => out.push('\r'),
                    'u' => {
                        let hex: String = (0..4)
                            .filter_map(|_| chars.next().map(|(_, h)| h))
                            .collect();
                        let code = u32::from_str_radix(&hex, 16)
                            .ok()
                            .and_then(char::from_u32)
                            .ok_or_else(|| format!("\\u{hex} isn't a character"))?;
                        out.push(code);
                    }
                    other => return Err(format!("\\{other} isn't an escape")),
                }
            }
            _ => out.push(c),
        }
    }
    Err("a string that doesn't end".into())
}

/// One value, which has to take up all of text.
fn parse_value(text: &str) -> Result<Value, String> {
    let text = text.trim();
    if text.starts_with('"') {
        let (string, rest) = basic_string(text)?;
        if !rest.trim().is_empty() {
            return Err(format!("unexpected '{}' after a string", rest.trim()));
        }
        return Ok(Value::String(string));
    }
    if let Some(rest) = text.strip_prefix('\'') {
        let (string, rest) = rest.split_once('\'').ok_or("a string that doesn't end")?;
        if !rest.trim().is_empty() {
            return Err(format!("unexpected '{}' after a string", rest.trim()));
        }
        return Ok(Value::String(string.to_string()));
    }
    if let Some(inner) = text.strip_prefix('[') {
        let inner = inner.strip_suffix(']').ok_or("an array that doesn't end")?;
        let mut items = Vec::new();
        for item in split_items(inner)? {
            let item = item.trim();
            if item.is_empty() {
                continue; // a trailing comma
            }
            if item.starts_with('[') {
                return Err("an array in an array isn't read".into());
            }
            items.push(parse_value(item)?);
        }
        return Ok(Value::Array(items));
    }
    match text {
        "true" => return Ok(Value::Bool(true)),
        "false" => return Ok(Value::Bool(false)),
        _ => {}
    }
    let number = text.replace('_', "");
    if let Ok(n) = number.parse::<i64>() {
        return Ok(json!(n));
    }
    if let Ok(x) = number.parse::<f64>() {
        if x.is_finite()
            && number.chars().any(|c| c.is_ascii_digit())
            && !number.contains(['i', 'n'])
        {
            return Ok(json!(x));
        }
    }
    Err(format!(
        "'{text}' isn't a value: a \"string\", a number, true or false, or [a list]"
    ))
}

/// An array's items, split on the commas outside quotes.
fn split_items(inner: &str) -> Result<Vec<String>, String> {
    let mut items = Vec::new();
    let mut item = String::new();
    let (mut basic, mut literal, mut escaped) = (false, false, false);
    for c in inner.chars() {
        match c {
            '\\' if basic && !escaped => {
                escaped = true;
                item.push(c);
                continue;
            }
            '"' if !literal && !escaped => basic = !basic,
            '\'' if !basic => literal = !literal,
            ',' if !basic && !literal => {
                items.push(std::mem::take(&mut item));
                escaped = false;
                continue;
            }
            _ => {}
        }
        escaped = false;
        item.push(c);
    }
    if basic || literal {
        return Err("a string that doesn't end".into());
    }
    items.push(item);
    Ok(items)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_plain_subset_of_toml() {
        let text = "# hatcheck's config\n[list]\ndetails = true  # the flag\nhours = \"3d\"\nper_folder = 3\nratio = 1.5\n\n[ignore]\nagents = [\"copilot\", 'opencode',]\nfolders = [\n  \"~/orca/workspaces/*\", # the orca folders\n  \"/tmp\",\n]\nnote = \"a \\\"quoted\\\" # not a comment\"\n[a.b]\nc = -2_000\n";
        let table = parse(text).unwrap();
        assert_eq!(table["list"]["details"], json!(true));
        assert_eq!(table["list"]["hours"], json!("3d"));
        assert_eq!(table["list"]["per_folder"], json!(3));
        assert_eq!(table["list"]["ratio"], json!(1.5));
        assert_eq!(table["ignore"]["agents"], json!(["copilot", "opencode"]));
        assert_eq!(
            table["ignore"]["folders"],
            json!(["~/orca/workspaces/*", "/tmp"])
        );
        assert_eq!(
            table["ignore"]["note"],
            json!("a \"quoted\" # not a comment")
        );
        assert_eq!(table["a"]["b"]["c"], json!(-2000));
        for (text, error) in [
            ("[list\n", "line 1: expected [table]"),
            ("details\n", "line 1: expected key = value"),
            ("[list]\ndetails = yes\n", "line 2: 'yes' isn't a value"),
            (
                "[list]\ndetails = true\ndetails = false\n",
                "line 3: 'details' is set twice",
            ),
            ("x = \"open\n", "line 1: a string that doesn't end"),
            ("x = [1,\n", "line 1: an array that doesn't end"),
            ("x = [[1]]\n", "line 1: an array in an array isn't read"),
            ("a.b = 1\n", "line 1: 'a.b' isn't a key"),
            ("[[t]]\n", "line 1: expected [table]"),
        ] {
            let e = parse(text).unwrap_err();
            assert!(e.starts_with(error), "{text:?}: {e}");
        }
    }

    #[test]
    fn the_settings_are_checked() {
        let config = |text: &str| parse(text).and_then(|t| from_table(&t));
        let c = config("[list]\ndetails = true\nby_cost = false\nhours = 36\nper_folder = 2\nusage = false\n[ignore]\nagents = [\"codex\", \"agy\"]\nfolders = [\"~/tmp/*\"]\n").unwrap();
        assert_eq!(
            (
                c.details,
                c.by_cost,
                c.hours.as_deref(),
                c.per_folder,
                c.full,
                c.usage
            ),
            (
                Some(true),
                Some(false),
                Some("36"),
                Some(2),
                None,
                Some(false)
            )
        );
        assert_eq!(c.ignore_agents, [Agent::Codex, Agent::Agy]);
        let listed = config("[list]\nfolders = [\"~/repos/*\"]\n").unwrap();
        assert_eq!(
            listed.list_folders,
            [ospath::abspath(&ospath::expanduser("~/repos/*"))]
        );
        assert_eq!(listed.describe(), ["list: folders ~/repos/*"]);
        // Only the folders listed are in, when any are, and those ignored are out either way.
        let both = Config {
            list_folders: vec!["/home/me/repos".into()],
            ignore_folders: vec!["/home/me/repos/junk".into()],
            ..Default::default()
        };
        assert!(!both.leaves_out("/home/me/repos/api"));
        assert!(both.leaves_out("/home/me/repos/junk/x"));
        assert!(both.leaves_out("/home/me/other"));
        assert!(!Config::default().leaves_out("/home/me/other"));
        assert_eq!(
            c.ignore_folders,
            [ospath::abspath(&ospath::expanduser("~/tmp/*"))]
        );
        assert_eq!(
            c.describe(),
            [
                "list: details, by time, 36, 2 per folder, no usage",
                "ignore: agents codex, agy; folders ~/tmp/*"
            ]
        );
        assert_eq!(Config::default().describe(), ["nothing set"]);
        for (text, error) in [
            ("[list]\ndetails = 1\n", "list.details takes true or false"),
            ("[list]\ndetail = true\n", "list.detail isn't a setting; [list] takes details, by_cost, hours, per_folder, full, all_folders, usage and folders"),
            ("[list]\nfolders = \"~/repos\"\n", "list.folders takes a list of folders"),
            ("[list]\nhours = \"soon\"\n", "list.hours takes hours like 24, 1.5, 12h or 3d"),
            ("[list]\nper_folder = 0\n", "list.per_folder takes a whole number of 1 or more"),
            ("[ignore]\nagents = \"codex\"\n", "ignore.agents takes a list of agents"),
            ("[ignore]\nagents = [\"gemini\"]\n", "ignore.agents: 'gemini' isn't an agent; they're claude, codex, agy, kimi, zcode, dsh, mcode, qwen, copilot and opencode"),
            ("[ignore]\nfolders = [\"\"]\n", "ignore.folders: a folder is empty"),
            ("[ignore]\nbranches = []\n", "ignore.branches isn't a setting; [ignore] takes agents and folders"),
            ("[lists]\n", "'lists' isn't a section; the sections are [list] and [ignore]"),
            ("details = true\n", "'details' isn't a section; the sections are [list] and [ignore]"),
        ] {
            assert_eq!(config(text).unwrap_err(), error, "{text:?}");
        }
    }

    #[test]
    fn a_pattern_covers_a_folder_and_those_under_it() {
        assert!(pattern_covers(
            "/home/me/orca/workspaces/*",
            "/home/me/orca/workspaces/infra"
        ));
        assert!(pattern_covers(
            "/home/me/orca/workspaces/*",
            "/home/me/orca/workspaces/infra/deep"
        ));
        assert!(pattern_covers(
            "/home/me/orca/workspaces",
            "/home/me/orca/workspaces/infra"
        ));
        assert!(pattern_covers(
            "/home/me/orca/workspaces",
            "/home/me/orca/workspaces"
        ));
        assert!(!pattern_covers(
            "/home/me/orca/workspaces/*",
            "/home/me/orca/workspaces"
        ));
        assert!(!pattern_covers(
            "/home/me/orca/workspaces",
            "/home/me/orca/workspaces2"
        ));
        assert!(pattern_covers(
            "/home/me/orca/w?rk*",
            "/home/me/orca/workspaces/x"
        ));
        assert!(!pattern_covers("/home/me/code", "/home/me/orca/code"));
        assert!(!pattern_covers("", "/home/me"));
        assert!(
            wild_match("*", "anything")
                && wild_match("a*c*", "abcabc")
                && !wild_match("a*d", "abc")
        );
    }
}
