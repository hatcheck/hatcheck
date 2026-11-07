//! hatcheck profiles [A B] [--json]: what each Claude Code profile, Codex home and Kimi home is
//! set up with, read as the agent keeps it: its instructions file, skills, agents, commands,
//! prompts, plugins, hooks, MCP servers, permissions and settings; and, with two named, what
//! differs between them. Nothing is written, and no value that could be a secret is shown: an
//! env variable goes by its name, an MCP server by its name, and a file by its size.
//! docs/commands.md's `hatcheck profiles` section has the rest.

use crate::agent::Agent;
use crate::args::{Count, Parser};
use crate::find;
use crate::ospath::{self, Part};
use crate::out::{self, Paint};
use crate::paths;
use crate::py::{self, get, s};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

/// One thing a profile is set up with.
#[derive(Clone, Debug)]
pub struct Item {
    /// What kind of thing: skill, agent, command, plugin, hook, MCP server, setting, …
    pub kind: &'static str,
    /// Its name, one of its kind in the profile.
    pub name: String,
    /// What equality means: the same fingerprint, the same thing (a hash, never the content).
    pub fingerprint: String,
    /// What's safe to show of it: a setting's value, a file's size; None for what may be a
    /// secret, such as an env variable's value or an MCP server's definition.
    pub shown: Option<String>,
    /// The file or folder it is, when it's one.
    pub path: Option<String>,
}

/// A profile's setup.
pub struct Setup {
    pub agent: Agent,
    pub folder: String,
    pub items: Vec<Item>,
    /// Files that are there but couldn't be read, with why: said rather than shown as absent.
    pub unread: Vec<String>,
}

/// The kinds in the order they're shown, each with its plural.
const KINDS: &[(&str, &str)] = &[
    ("instructions", "instructions"),
    ("skill", "skills"),
    ("synced skill", "synced skills"),
    ("agent", "agents"),
    ("command", "commands"),
    ("prompt", "prompts"),
    ("rule", "rules"),
    ("plugin", "plugins"),
    ("hook", "hooks"),
    ("hooks file", "hooks files"),
    ("MCP server", "MCP servers"),
    ("profile", "profiles"),
    ("trusted folder", "trusted folders"),
    ("permission", "permissions"),
    ("env", "env variables"),
    ("setting", "settings"),
    ("keybindings", "keybindings"),
];

/// A kind as one of it is counted: "1 env variable", "1 skill".
fn one(kind: &str) -> &str {
    if kind == "env" {
        "env variable"
    } else {
        kind
    }
}

fn kind_order(kind: &str) -> usize {
    KINDS.iter().position(|(k, _)| *k == kind).unwrap_or(99)
}

fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// A size as "415 B", "2.1 KB" or "1.3 MB".
pub fn size(bytes: u64) -> String {
    if bytes < 1000 {
        format!("{bytes} B")
    } else if bytes < 1_000_000 {
        format!("{:.1} KB", bytes as f64 / 1000.0)
    } else {
        format!("{:.1} MB", bytes as f64 / 1e6)
    }
}

/// Every file under a folder, as (relative path, full path), sorted.
fn walk(folder: &str) -> Vec<(String, String)> {
    let mut found = Vec::new();
    let mut stack = vec![(String::new(), folder.to_string())];
    while let Some((rel, dir)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let full = ospath::join(&dir, &name);
            let here = if rel.is_empty() {
                name.clone()
            } else {
                format!("{rel}/{name}")
            };
            if ospath::isdir(&full) {
                stack.push((here, full));
            } else {
                found.push((here, full));
            }
        }
    }
    found.sort();
    found
}

/// A folder's fingerprint, over every file's relative path and content, and how many files.
fn folder_item(kind: &'static str, name: &str, folder: &str) -> Item {
    let files = walk(folder);
    let mut hasher = Sha256::new();
    let mut bytes = 0u64;
    for (rel, full) in &files {
        hasher.update(rel.as_bytes());
        hasher.update([0]);
        if let Ok(data) = std::fs::read(full) {
            bytes += data.len() as u64;
            hasher.update(&data);
        }
        hasher.update([0]);
    }
    Item {
        kind,
        name: name.to_string(),
        fingerprint: format!("{:x}", hasher.finalize()),
        shown: Some(format!(
            "{} file{}, {}",
            files.len(),
            if files.len() == 1 { "" } else { "s" },
            size(bytes)
        )),
        path: Some(folder.to_string()),
    }
}

/// A file's fingerprint and size.
fn file_item(kind: &'static str, name: &str, path: &str) -> Option<Item> {
    let data = std::fs::read(path).ok()?;
    Some(Item {
        kind,
        name: name.to_string(),
        fingerprint: sha(&data),
        shown: Some(size(data.len() as u64)),
        path: Some(path.to_string()),
    })
}

/// The folders directly under a folder, by name.
fn subfolders(folder: &str) -> Vec<(String, String)> {
    let mut found: Vec<(String, String)> = ospath::glob(folder, &[Part::Wild("", "")])
        .into_iter()
        .filter(|p| ospath::isdir(p))
        .map(|p| (ospath::basename(&p), p))
        .collect();
    found.sort();
    found
}

/// The .md files under a folder, nested ones named with ':' as Claude Code names a command in
/// a subfolder (commands/git/commit.md is /git:commit).
fn md_files(folder: &str) -> Vec<(String, String)> {
    walk(folder)
        .into_iter()
        .filter(|(rel, _)| rel.ends_with(".md"))
        .map(|(rel, full)| (rel[..rel.len() - 3].replace('/', ":"), full))
        .collect()
}

/// The JSON object in a file: None when there's no file, Err when it can't be read.
fn read_object(path: &str) -> Result<Option<Map<String, Value>>, String> {
    if !ospath::exists(path) {
        return Ok(None);
    }
    let shown = paths::shell_path(path);
    let text = std::fs::read(path).map_err(|e| {
        format!(
            "{shown}: can't read it ({})",
            e.to_string().split(" (os error").next().unwrap_or("")
        )
    })?;
    match serde_json::from_slice::<Value>(&text) {
        Ok(Value::Object(o)) => Ok(Some(o)),
        Ok(_) => Err(format!("{shown}: not a JSON object")),
        Err(_) => Err(format!("{shown}: not valid JSON")),
    }
}

/// A value as a setting shows it: a string as it is, a number or bool as JSON, an object or
/// array as compact JSON cut short.
fn scalar_text(v: &Value) -> String {
    match v {
        Value::String(t) => t.clone(),
        Value::Object(o)
            if !o.is_empty() && o.values().all(|x| !x.is_object() && !x.is_array()) =>
        {
            o.iter()
                .map(|(k, x)| format!("{k} {}", scalar_text(x)))
                .collect::<Vec<_>>()
                .join(", ")
        }
        Value::Object(_) | Value::Array(_) => {
            let text = py::dumps_compact(v);
            let cut = py::head(&text, 60);
            if cut.len() < text.len() {
                format!("{cut}…")
            } else {
                text
            }
        }
        other => py::dumps_compact(other),
    }
}

fn setting(name: String, v: &Value) -> Item {
    Item {
        kind: "setting",
        name,
        fingerprint: sha(py::dumps_compact(v).as_bytes()),
        shown: Some(scalar_text(v)),
        path: None,
    }
}

/// Each hook command under an agent's `hooks` object ({Event: [{matcher?, hooks: [{command}]}]},
/// as Claude Code's settings.json and Codex's hooks.json have it), as "Event: command".
fn hook_items(hooks: &Value) -> Vec<Item> {
    let mut items = Vec::new();
    let Value::Object(events) = hooks else {
        return items;
    };
    for (event, entries) in events {
        let Value::Array(entries) = entries else {
            continue;
        };
        for entry in entries {
            let matcher = s(get(entry, "matcher")).unwrap_or("");
            let Value::Array(hooks) = get(entry, "hooks") else {
                continue;
            };
            for hook in hooks {
                let command = s(get(hook, "command")).unwrap_or("");
                let what = if matcher.is_empty() {
                    command.to_string()
                } else {
                    format!("[{matcher}] {command}")
                };
                let short = py::collapse(&what);
                let cut = py::head(&short, 70);
                items.push(Item {
                    kind: "hook",
                    name: format!(
                        "{event}: {cut}{}",
                        if cut.len() < short.len() { "…" } else { "" }
                    ),
                    fingerprint: sha(py::dumps_compact(hook).as_bytes()),
                    shown: None,
                    path: None,
                });
            }
        }
    }
    items
}

/// Key = value pairs, as their text.
type Pairs = Vec<(String, String)>;
/// Tables, each named by its parts, with their pairs.
type Tables = Vec<(Vec<String>, Pairs)>;

/// A TOML file read line by line, as a plain subset: the top-level `key = value` pairs, and
/// each `[table]` with its pairs, the table named by its parts (quoted ones kept whole, so
/// `[projects."/a/b.c"]` is ["projects", "/a/b.c"]). Values are kept as their text, a quoted
/// string unquoted. Nothing else is read, and a line that isn't one of those is passed over.
fn toml_tables(text: &str) -> (Pairs, Tables) {
    let mut top = Vec::new();
    let mut tables: Tables = Vec::new();
    let mut skipping = false; // inside an [[array of tables]], which isn't read
    let mut lines = text.lines();
    while let Some(raw) = lines.next() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(rest) = line.strip_prefix('[') {
            skipping = rest.starts_with('[');
            if skipping {
                continue;
            }
            let Some(inner) = rest.split(']').next() else {
                continue;
            };
            let mut parts = Vec::new();
            let mut part = String::new();
            let mut quoted = false;
            for c in inner.chars() {
                match c {
                    '"' => quoted = !quoted,
                    '.' if !quoted => parts.push(std::mem::take(&mut part).trim().to_string()),
                    _ => part.push(c),
                }
            }
            parts.push(part.trim().to_string());
            tables.push((parts, Vec::new()));
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if skipping {
            continue;
        }
        let key = key.trim().trim_matches('"').to_string();
        let mut value = value.trim().to_string();
        // An array or inline table over several lines: on to where its brackets balance.
        let mut depth = value.chars().filter(|c| matches!(c, '[' | '{')).count() as i64
            - value.chars().filter(|c| matches!(c, ']' | '}')).count() as i64;
        while depth > 0 {
            let Some(more) = lines.next() else { break };
            let more = more.trim();
            depth += more.chars().filter(|c| matches!(c, '[' | '{')).count() as i64
                - more.chars().filter(|c| matches!(c, ']' | '}')).count() as i64;
            value.push(' ');
            value.push_str(more);
        }
        // A quoted string up to its closing quote, a comment after it dropped; anything else
        // as its text, up to a comment.
        if let Some(rest) = value.strip_prefix('"') {
            let mut end = rest.len();
            let mut escaped = false;
            for (i, c) in rest.char_indices() {
                match c {
                    '\\' if !escaped => escaped = true,
                    '"' if !escaped => {
                        end = i;
                        break;
                    }
                    _ => escaped = false,
                }
            }
            value = rest[..end].replace("\\\"", "\"");
        } else if let Some(rest) = value.strip_prefix('\'') {
            value = rest.split('\'').next().unwrap_or("").to_string();
        } else if let Some((before, _)) = value.split_once(" #") {
            value = before.trim().to_string();
        }
        match tables.last_mut() {
            Some((_, pairs)) => pairs.push((key, value)),
            None => top.push((key, value)),
        }
    }
    (top, tables)
}

/// A table's pairs as one fingerprint, so a server or profile defined differently shows as such.
fn pairs_fingerprint(pairs: &[(String, String)]) -> String {
    let text: Vec<String> = pairs.iter().map(|(k, v)| format!("{k}={v}")).collect();
    sha(text.join("\n").as_bytes())
}

/// What a Claude Code profile is set up with, read as Claude Code 2.1.319 keeps it.
fn claude_setup(folder: &str) -> Setup {
    let mut items = Vec::new();
    let mut unread = Vec::new();
    let file = |name: &str| ospath::join(folder, name);
    if let Some(item) = file_item("instructions", "CLAUDE.md", &file("CLAUDE.md")) {
        items.push(item);
    }
    for (name, path) in subfolders(&file("skills")) {
        if name == "synced" {
            // Skills synced from claude.ai: skills/synced/<organisation>_<user>/<skill>/, the
            // account's own, so two profiles differ in them by their logins alone.
            for (_, bucket) in subfolders(&path) {
                for (skill, path) in subfolders(&bucket) {
                    items.push(folder_item("synced skill", &skill, &path));
                }
            }
        } else {
            items.push(folder_item("skill", &name, &path));
        }
    }
    for (name, path) in md_files(&file("agents")) {
        items.extend(file_item("agent", &name, &path));
    }
    for (name, path) in md_files(&file("commands")) {
        items.extend(file_item("command", &name, &path));
    }
    for (rel, path) in walk(&file("hooks")) {
        items.extend(file_item("hooks file", &rel, &path));
    }
    items.extend(file_item(
        "keybindings",
        "keybindings.json",
        &file("keybindings.json"),
    ));
    // Plugins: installed, from installed_plugins.json, and enabled, from settings.json.
    let mut plugins: Vec<(String, String)> = Vec::new(); // (name, version)
    match read_object(&ospath::join(&file("plugins"), "installed_plugins.json")) {
        Ok(Some(installed)) => {
            if let Some(Value::Object(list)) = installed.get("plugins") {
                for (name, entries) in list {
                    let versions: Vec<String> = match entries {
                        Value::Array(a) => a
                            .iter()
                            .filter_map(|e| s(get(e, "version")).map(String::from))
                            .collect(),
                        other => s(get(other, "version"))
                            .map(String::from)
                            .into_iter()
                            .collect(),
                    };
                    plugins.push((name.clone(), versions.join(", ")));
                }
            }
        }
        Ok(None) => {}
        Err(why) => unread.push(why),
    }
    let settings = match read_object(&file("settings.json")) {
        Ok(found) => found.unwrap_or_default(),
        Err(why) => {
            unread.push(why);
            Map::new()
        }
    };
    let enabled: Vec<(String, bool)> = match settings.get("enabledPlugins") {
        Some(Value::Object(o)) => o.iter().map(|(k, v)| (k.clone(), py::truthy(v))).collect(),
        _ => Vec::new(),
    };
    for (name, on) in &enabled {
        if !plugins.iter().any(|(n, _)| n == name) {
            plugins.push((name.clone(), String::new()));
        }
        let _ = on;
    }
    plugins.sort();
    for (name, version) in plugins {
        let state = match enabled.iter().find(|(n, _)| *n == name) {
            Some((_, true)) => "enabled",
            Some((_, false)) => "disabled",
            None => "installed, not enabled",
        };
        let shown = if version.is_empty() {
            format!("{state}, not installed")
        } else {
            format!("{version}, {state}")
        };
        items.push(Item {
            kind: "plugin",
            name,
            fingerprint: sha(shown.as_bytes()),
            shown: Some(shown),
            path: None,
        });
    }
    for (key, value) in &settings {
        match key.as_str() {
            "enabledPlugins" => {}
            "hooks" => items.extend(hook_items(value)),
            "env" => {
                if let Value::Object(vars) = value {
                    for (name, v) in vars {
                        items.push(Item {
                            kind: "env",
                            name: name.clone(),
                            fingerprint: sha(py::dumps_compact(v).as_bytes()),
                            shown: None, // may be a key
                            path: None,
                        });
                    }
                }
            }
            "permissions" => {
                if let Value::Object(perms) = value {
                    for (field, v) in perms {
                        match (field.as_str(), v) {
                            ("allow" | "deny" | "ask", Value::Array(rules)) => {
                                for rule in rules {
                                    let text = py::str_of(rule);
                                    items.push(Item {
                                        kind: "permission",
                                        name: format!("{field} {text}"),
                                        fingerprint: sha(text.as_bytes()),
                                        shown: None,
                                        path: None,
                                    });
                                }
                            }
                            _ => items.push(setting(format!("permissions.{field}"), v)),
                        }
                    }
                }
            }
            _ => match value {
                // A table of settings, such as modelSettings, a setting per entry.
                Value::Object(o) if !o.is_empty() && o.values().all(Value::is_object) => {
                    for (sub, v) in o {
                        items.push(setting(format!("{key}.{sub}"), v));
                    }
                }
                _ => items.push(setting(key.clone(), value)),
            },
        }
    }
    // MCP servers at user scope live in .claude.json, beside ~/.claude for the default profile.
    let config = if paths::same_folder(folder, &paths::default_profile()) {
        ospath::join(&paths::home(), ".claude.json")
    } else {
        file(".claude.json")
    };
    match read_object(&config) {
        Ok(Some(c)) => {
            if let Some(Value::Object(servers)) = c.get("mcpServers") {
                for (name, def) in servers {
                    items.push(Item {
                        kind: "MCP server",
                        name: name.clone(),
                        fingerprint: sha(py::dumps_compact(def).as_bytes()),
                        shown: None, // its env may hold a token
                        path: None,
                    });
                }
            }
        }
        Ok(None) => {}
        Err(why) => unread.push(why),
    }
    Setup {
        agent: Agent::Claude,
        folder: folder.to_string(),
        items,
        unread,
    }
}

/// What a Codex home is set up with, read as Codex 0.155 keeps it.
fn codex_setup(folder: &str) -> Setup {
    let mut items = Vec::new();
    let mut unread = Vec::new();
    let file = |name: &str| ospath::join(folder, name);
    items.extend(file_item("instructions", "AGENTS.md", &file("AGENTS.md")));
    for (name, path) in subfolders(&file("skills")) {
        items.push(folder_item("skill", &name, &path));
    }
    for (name, path) in md_files(&file("prompts")) {
        items.extend(file_item("prompt", &name, &path));
    }
    for (rel, path) in walk(&file("rules")) {
        items.extend(file_item("rule", &rel, &path));
    }
    match read_object(&file("hooks.json")) {
        Ok(Some(h)) => items.extend(hook_items(h.get("hooks").unwrap_or(&Value::Null))),
        Ok(None) => {}
        Err(why) => unread.push(why),
    }
    let config = file("config.toml");
    if ospath::exists(&config) {
        match std::fs::read_to_string(&config) {
            Ok(text) => {
                let (top, tables) = toml_tables(&text);
                for (key, value) in top {
                    items.push(setting(key, &Value::String(value)));
                }
                for (parts, pairs) in tables {
                    let name = parts[1..].join(".");
                    match parts[0].as_str() {
                        "mcp_servers" if parts.len() > 1 => items.push(Item {
                            kind: "MCP server",
                            name,
                            fingerprint: pairs_fingerprint(&pairs),
                            shown: None,
                            path: None,
                        }),
                        "profiles" if parts.len() > 1 => items.push(Item {
                            kind: "profile",
                            name,
                            fingerprint: pairs_fingerprint(&pairs),
                            shown: pairs
                                .iter()
                                .find(|(k, _)| k == "model")
                                .map(|(_, v)| format!("model {v}")),
                            path: None,
                        }),
                        "projects" if parts.len() > 1 => items.push(Item {
                            kind: "trusted folder",
                            name: paths::tilde_path(&name),
                            fingerprint: pairs_fingerprint(&pairs),
                            shown: pairs
                                .iter()
                                .find(|(k, _)| k == "trust_level")
                                .map(|(_, v)| v.clone()),
                            path: None,
                        }),
                        _ => {
                            for (key, value) in pairs {
                                items.push(setting(
                                    format!("{}.{key}", parts.join(".")),
                                    &Value::String(value),
                                ));
                            }
                        }
                    }
                }
            }
            Err(e) => unread.push(format!(
                "{}: can't read it ({})",
                paths::shell_path(&config),
                e.to_string().split(" (os error").next().unwrap_or("")
            )),
        }
    }
    Setup {
        agent: Agent::Codex,
        folder: folder.to_string(),
        items,
        unread,
    }
}

/// What a Kimi home is set up with, read as Kimi Code CLI 2.2 keeps it: its config.toml's
/// top-level settings and [thinking] table, the tables kimi writes for its providers and models
/// left alone, since they hold keys; its hooks folder; and its skills, when it has any.
fn kimi_setup(folder: &str) -> Setup {
    let mut items = Vec::new();
    let mut unread = Vec::new();
    let file = |name: &str| ospath::join(folder, name);
    for (name, path) in subfolders(&file("skills")) {
        items.push(folder_item("skill", &name, &path));
    }
    for (rel, path) in walk(&file("hooks")) {
        items.extend(file_item("hooks file", &rel, &path));
    }
    let config = file("config.toml");
    if ospath::exists(&config) {
        match std::fs::read_to_string(&config) {
            Ok(text) => {
                let (top, tables) = toml_tables(&text);
                for (key, value) in top {
                    items.push(setting(key, &Value::String(value)));
                }
                for (parts, pairs) in tables {
                    if parts == ["thinking"] {
                        for (key, value) in pairs {
                            items.push(setting(format!("thinking.{key}"), &Value::String(value)));
                        }
                    }
                }
            }
            Err(e) => unread.push(format!(
                "{}: can't read it ({})",
                paths::shell_path(&config),
                e.to_string().split(" (os error").next().unwrap_or("")
            )),
        }
    }
    Setup {
        agent: Agent::Kimi,
        folder: folder.to_string(),
        items,
        unread,
    }
}

/// A profile's, home's or folder's setup, by its agent.
pub fn setup_of(agent: Agent, folder: &str) -> Option<Setup> {
    let mut setup = match agent {
        Agent::Claude => claude_setup(folder),
        Agent::Codex => codex_setup(folder),
        Agent::Kimi => kimi_setup(folder),
        _ => return None,
    };
    setup.items.sort_by(|a, b| {
        kind_order(a.kind)
            .cmp(&kind_order(b.kind))
            .then(a.name.cmp(&b.name))
    });
    Some(setup)
}

/// Every profile and home whose setup hatcheck reads: Claude Code's, with or without
/// sessions, Codex's and Kimi's.
fn all_setups() -> Vec<Setup> {
    let mut found = Vec::new();
    for p in crate::usage::find_profiles() {
        found.push(claude_setup(&p));
    }
    for h in crate::usage::find_codex_homes() {
        found.push(codex_setup(&h));
    }
    for h in paths::find_kimi_homes() {
        found.push(kimi_setup(&h));
    }
    for setup in found.iter_mut() {
        setup.items.sort_by(|a, b| {
            kind_order(a.kind)
                .cmp(&kind_order(b.kind))
                .then(a.name.cmp(&b.name))
        });
    }
    found
}

/// The settings a summary line leads with, in this order.
const LEAD_SETTINGS: &[&str] = &[
    "model",
    "effortLevel",
    "model_reasoning_effort",
    "default_model",
    "thinking.effort",
    "fastMode",
    "theme",
];

fn item_json(i: &Item) -> Value {
    json!({"kind": i.kind, "name": i.name, "shown": i.shown, "path": i.path.as_deref().map(ospath::abspath),
        "fingerprint": i.fingerprint})
}

fn setup_json(x: &Setup) -> Value {
    json!({"agent": x.agent.key(), "path": ospath::abspath(&x.folder),
        "items": x.items.iter().map(item_json).collect::<Vec<_>>(), "unread": x.unread})
}

/// The summary's lines for one setup: its leading settings, then how many of each kind.
fn summary_lines(x: &Setup, paint: &Paint) -> Vec<String> {
    let p = |code: &str, text: &str| paint.p(code, text);
    let settings: Vec<&Item> = x.items.iter().filter(|i| i.kind == "setting").collect();
    let mut lead: Vec<String> = Vec::new();
    for name in LEAD_SETTINGS {
        if let Some(i) = settings.iter().find(|i| i.name == *name) {
            let value = i.shown.clone().unwrap_or_default();
            lead.push(match name.rsplit('.').next().unwrap_or(name) {
                "fastMode" if value == "true" => "fast mode".to_string(),
                "fastMode" => continue,
                _ => format!("{} {}", name.rsplit('.').next().unwrap_or(name), value),
            });
        }
    }
    let other = settings
        .iter()
        .filter(|i| !LEAD_SETTINGS.contains(&i.name.as_str()))
        .count();
    if other > 0 {
        lead.push(format!(
            "{other} more setting{}",
            if other == 1 { "" } else { "s" }
        ));
    }
    let mut counts: Vec<String> = Vec::new();
    for (kind, plural) in KINDS {
        if *kind == "setting" {
            continue;
        }
        let of_kind: Vec<&Item> = x.items.iter().filter(|i| i.kind == *kind).collect();
        if of_kind.is_empty() {
            continue;
        }
        counts.push(match *kind {
            "instructions" | "keybindings" => of_kind
                .iter()
                .map(|i| format!("{} {}", i.name, i.shown.clone().unwrap_or_default()))
                .collect::<Vec<_>>()
                .join(" · "),
            _ if of_kind.len() == 1 => format!("1 {}", one(kind)),
            _ => format!("{} {plural}", of_kind.len()),
        });
    }
    let mut lines = Vec::new();
    let label = paint.accent(&paths::shell_path(&x.folder));
    let default = if paths::same_folder(&x.folder, &paths::default_folder(x.agent)) {
        p("2", "  (default)")
    } else {
        String::new()
    };
    lines.push(format!("  {label}{default}"));
    if !lead.is_empty() {
        lines.push(format!("    {}", lead.join(p("2", " · ").as_str())));
    }
    lines.push(format!(
        "    {}",
        if counts.is_empty() {
            p("2", "nothing set up yet")
        } else {
            counts.join(&p("2", " · "))
        }
    ));
    for why in &x.unread {
        lines.push(format!("    {}", paint.warning(why)));
    }
    lines
}

/// What differs between two setups: (only in a, only in b, in both but different, the same).
type Diff<'a> = (
    Vec<&'a Item>,
    Vec<&'a Item>,
    Vec<(&'a Item, &'a Item)>,
    Vec<&'a Item>,
);

fn diff<'a>(a: &'a Setup, b: &'a Setup) -> Diff<'a> {
    let (mut only_a, mut only_b, mut differ, mut same) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for i in &a.items {
        match b
            .items
            .iter()
            .find(|j| j.kind == i.kind && j.name == i.name)
        {
            None => only_a.push(i),
            Some(j) if j.fingerprint == i.fingerprint => same.push(i),
            Some(j) => differ.push((i, j)),
        }
    }
    for j in &b.items {
        if !a.items.iter().any(|i| i.kind == j.kind && i.name == j.name) {
            only_b.push(j);
        }
    }
    (only_a, only_b, differ, same)
}

/// "skill k2", "setting model", "MCP server github".
fn named(i: &Item) -> String {
    format!("{} {}", i.kind, i.name)
}

/// How many of each kind, as "10 skills, 1 plugin and 3 settings".
fn counted(items: &[&Item]) -> String {
    let mut parts: Vec<String> = Vec::new();
    for (kind, plural) in KINDS {
        let n = items.iter().filter(|i| i.kind == *kind).count();
        if n == 1 {
            parts.push(format!("1 {}", one(kind)));
        } else if n > 1 {
            parts.push(format!("{n} {plural}"));
        }
    }
    let parts: Vec<&str> = parts.iter().map(String::as_str).collect();
    crate::config::and_list(&parts)
}

pub fn main(argv: &[String], prog: &str) -> i32 {
    let parser = Parser::new(
        prog,
        "What each Claude Code profile, Codex home and Kimi home is set up with, read as the agent keeps it: its instructions file, skills, agents, commands, prompts, plugins, hooks, MCP servers, permissions and settings. With two named, what differs between them: what one has that the other doesn't, and what both have that isn't the same. Nothing is changed, and nothing that could be a secret is shown.",
    )
    .positional("profiles", Count::Any, "two profiles or homes to compare: NAME for ~/.claude-NAME, default for ~/.claude, codex or codex:NAME, kimi or kimi:NAME, or a path")
    .flag(&["-j", "--json"], "json", "print the setups, or the comparison, as JSON");
    let args = match parser.parse(argv) {
        Ok(a) => a,
        Err(stop) => return stop.exit_code(),
    };
    let names = args.values("profiles");
    if names.len() == 1 || names.len() > 2 {
        return parser
            .error("give two profiles to compare, or none to see every one")
            .exit_code();
    }
    out::setup_console();
    let paint = Paint::new();
    let p = |code: &str, text: &str| paint.p(code, text);
    if names.is_empty() {
        let setups = all_setups();
        if args.flag("json") {
            out::println(&py::dumps(&json!({"version": 1,
                "profiles": setups.iter().map(setup_json).collect::<Vec<_>>()})));
            return 0;
        }
        if setups.is_empty() {
            out::println("No Claude Code profile, Codex home or Kimi home to read: none in the home folder, or config.toml leaves their agents out.");
            return 1;
        }
        out::println(&paint.heading("What each profile and home is set up with"));
        let mut last: Option<Agent> = None;
        for x in &setups {
            if last != Some(x.agent) {
                out::println(&p("1", x.agent.name()));
                last = Some(x.agent);
            }
            for line in summary_lines(x, &paint) {
                out::println(&line);
            }
        }
        out::println(&p(
            "2",
            &format!("{prog} A B shows what differs between two; nothing is changed."),
        ));
        return 0;
    }
    let mut setups = Vec::new();
    for name in names {
        let (agent, folder) = match find::place_seen(name) {
            Ok(place) => place,
            Err(message) => return out::fail(&message),
        };
        if !ospath::isdir(&folder) {
            return out::fail(&format!("There's no {}", paths::shell_path(&folder)));
        }
        match setup_of(agent, &folder) {
            Some(setup) => setups.push(setup),
            None => {
                return out::fail(&format!(
                    "{} is {}'s, whose setup hatcheck doesn't read: only Claude Code profiles, Codex homes and Kimi homes",
                    paths::shell_path(&folder),
                    agent.name()
                ))
            }
        }
    }
    let (a, b) = (&setups[0], &setups[1]);
    if a.agent != b.agent {
        return out::fail(&format!(
            "{} is {}'s and {} is {}'s: hatcheck compares two of one agent's",
            paths::shell_path(&a.folder),
            a.agent.name(),
            paths::shell_path(&b.folder),
            b.agent.name()
        ));
    }
    if paths::same_folder(&a.folder, &b.folder) {
        return out::fail(&format!(
            "{} and {} are the same folder",
            paths::shell_path(&a.folder),
            paths::shell_path(&b.folder)
        ));
    }
    let (only_a, only_b, differ, same) = diff(a, b);
    let identical = only_a.is_empty() && only_b.is_empty() && differ.is_empty();
    if args.flag("json") {
        out::println(&py::dumps(
            &json!({"version": 1, "a": setup_json(a), "b": setup_json(b),
            "only_in_a": only_a.iter().map(|i| item_json(i)).collect::<Vec<_>>(),
            "only_in_b": only_b.iter().map(|i| item_json(i)).collect::<Vec<_>>(),
            "different": differ.iter().map(|(i, j)| json!({"kind": i.kind, "name": i.name, "a": i.shown, "b": j.shown,
                "a_path": i.path.as_deref().map(ospath::abspath), "b_path": j.path.as_deref().map(ospath::abspath)})).collect::<Vec<_>>(),
            "same": same.len(), "identical": identical}),
        ));
        return i32::from(!identical);
    }
    let (sa, sb) = (paths::shell_path(&a.folder), paths::shell_path(&b.folder));
    let what = match a.agent {
        Agent::Claude => "Claude Code profiles",
        Agent::Codex => "Codex homes",
        _ => "Kimi Code CLI homes",
    };
    out::println(&paint.heading(&format!("{sa} and {sb}, {what}")));
    let width = out::columns(100);
    let show = |items: &[&Item]| {
        let name_width = items
            .iter()
            .map(|i| named(i).chars().count())
            .max()
            .unwrap_or(0)
            .min(50);
        for i in items {
            let name = named(i);
            let detail = match (&i.shown, &i.path) {
                (_, Some(path)) => p("2", &paths::shell_path(path)),
                (Some(shown), None) => p("2", shown),
                (None, None) => String::new(),
            };
            let line = if detail.is_empty() || name.chars().count() > name_width {
                format!("  {name}  {detail}")
            } else {
                format!("  {name:<name_width$}  {detail}")
            };
            out::println(line.trim_end());
        }
    };
    if !only_a.is_empty() {
        out::println(&p("1", &format!("Only in {sa}:")));
        show(&only_a);
    }
    if !only_b.is_empty() {
        out::println(&p("1", &format!("Only in {sb}:")));
        show(&only_b);
    }
    if !differ.is_empty() {
        out::println(&p("1", "In both, not the same:"));
        let name_width = differ
            .iter()
            .map(|(i, _)| named(i).chars().count())
            .max()
            .unwrap_or(0)
            .min(50);
        for (i, j) in &differ {
            let name = named(i);
            let detail = match (&i.shown, &j.shown) {
                (Some(x), Some(y)) => format!("{x} in {sa}, {y} in {sb}"),
                _ if i.kind == "env" => "set differently".to_string(),
                _ => "defined differently".to_string(),
            };
            let line = format!("  {name:<name_width$}  {}", p("2", &detail));
            out::println(crate::widths::truncate(line.trim_end(), width).as_str());
        }
    }
    if identical {
        out::println(&format!(
            "The same setup in both: {}.",
            if same.is_empty() {
                "nothing in either".to_string()
            } else {
                counted(&same)
            }
        ));
    } else if !same.is_empty() {
        out::println(&p("2", &format!("The same in both: {}.", counted(&same))));
    }
    for x in [a, b] {
        for why in &x.unread {
            out::println(&paint.warning(why));
        }
    }
    out::println(&p(
        "2",
        "Nothing was changed: hatcheck reads a setup and never writes one. A skill, agent, command, prompt or hooks file is a folder or file to copy into the other; a setting, permission, hook, plugin or MCP server is set in the agent itself or its config file.",
    ));
    i32::from(!identical)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toml_is_read_as_tables_with_quoted_names_kept_whole() {
        let text = "model = \"gpt-5.5\"\nnotify = [\"a\",\n  \"b\"]\n# comment\n[projects.\"/Users/me/app.v2\"]\ntrust_level = \"trusted\"\n[mcp_servers.github]\ncommand = \"npx\" # the cli\nargs = [\"-y\", \"x\"]\n[[array_table]]\nk = 1\n[tui]\nnotifications = true\n";
        let (top, tables) = toml_tables(text);
        assert_eq!(
            top,
            [
                ("model".to_string(), "gpt-5.5".to_string()),
                ("notify".to_string(), "[\"a\", \"b\"]".to_string())
            ]
        );
        assert_eq!(
            tables[0],
            (
                vec!["projects".to_string(), "/Users/me/app.v2".to_string()],
                vec![("trust_level".to_string(), "trusted".to_string())]
            )
        );
        assert_eq!(tables[1].0, ["mcp_servers", "github"]);
        assert_eq!(
            tables[1].1,
            [
                ("command".to_string(), "npx".to_string()),
                ("args".to_string(), "[\"-y\", \"x\"]".to_string())
            ]
        );
        // The array of tables is passed over, and k = 1 lands nowhere.
        assert_eq!(tables[2].0, ["tui"]);
        assert_eq!(tables.len(), 3);
    }

    #[test]
    fn hooks_are_one_item_per_command_and_sizes_read() {
        let hooks = json!({"PreToolUse": [{"matcher": "Bash", "hooks": [{"type": "command", "command": "echo one"}, {"type": "command", "command": "echo two"}]}],
            "Stop": [{"hooks": [{"type": "command", "command": "   echo\n three  "}]}]});
        let items = hook_items(&hooks);
        let names: Vec<&str> = items.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "PreToolUse: [Bash] echo one",
                "PreToolUse: [Bash] echo two",
                "Stop: echo three"
            ]
        );
        assert_ne!(items[0].fingerprint, items[1].fingerprint);
        assert_eq!(
            (size(415), size(2100), size(1_300_000)),
            (
                "415 B".to_string(),
                "2.1 KB".to_string(),
                "1.3 MB".to_string()
            )
        );
    }
}
