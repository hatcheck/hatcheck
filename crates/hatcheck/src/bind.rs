//! hatcheck bind and unbind: tie a folder to a profile, so work there happens under the right
//! account; and the warning hook, a SessionStart hook in every Claude Code profile's
//! settings.json and every Codex home's hooks.json, which runs hatcheck bind --check. The
//! docs/commands.md's `hatcheck bind` section describes them.

use crate::agent::Agent;
use crate::args::{Count, Parser};
use crate::ospath;
use crate::out;
use crate::paths;
use crate::py::{self, get, s, Dict};
use serde_json::{json, Value};

const HOOK: &str = "bind --check"; // in the hook's command, to find it again
/// agy's hooks.json holds named hooks, each with a flat SessionStart list of handlers; the one
/// hatcheck adds is named after it, and one of ours is found by its command wherever it sits.
const AGY_HOOK: &str = "hatcheck";

/// Write JSON whole, under a temporary name until it's complete, as Python's json.dump with
/// indent=2 writes it, and a newline.
fn write_json(path: &str, data: &Value) -> std::io::Result<()> {
    std::fs::create_dir_all(ospath::dirname(path))?;
    let tmp = format!("{path}.hatcheck-tmp");
    std::fs::write(&tmp, py::dumps(data) + "\n")?;
    std::fs::rename(&tmp, path)
}

pub(crate) fn save_bindings(bindings: &Dict) -> std::io::Result<()> {
    let mut sorted: Vec<(&String, &Value)> = bindings.iter().collect();
    sorted.sort_by(|a, b| a.0.cmp(b.0));
    let folders: Dict = sorted
        .into_iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    write_json(
        &paths::bindings_path(),
        &json!({"version": 1, "folders": folders}),
    )
}

/// (name to keep, folder) for a PROFILE or --codex argument: default, a NAME or a path.
fn name_of(agent: Agent, spec: &str) -> (String, String) {
    let spec = if spec == agent.key() { "default" } else { spec }; // claude, codex, agy: the agent's own
    if spec == "default"
        || (!spec.contains('/') && !spec.contains(ospath::SEP) && !spec.starts_with('~'))
    {
        return (spec.to_string(), paths::profile_folder(agent, spec));
    }
    let path = ospath::abspath(&ospath::expanduser(spec));
    (path.clone(), path)
}

fn describe(binding: &Value) -> String {
    let parts: Vec<String> = Agent::ALL
        .iter()
        .filter_map(|agent| {
            let name = s(get(binding, agent.key())).filter(|n| !n.is_empty())?;
            Some(format!(
                "{} {}",
                agent.name(),
                paths::shell_path(&paths::profile_folder(*agent, name))
            ))
        })
        .collect();
    if parts.is_empty() {
        "nothing".into()
    } else {
        parts.join(" · ")
    }
}

/// The JSON object in a settings file ({} when there's no file), or why it can't be used.
fn read_settings(path: &str) -> Result<Dict, String> {
    if !ospath::exists(path) {
        return Ok(Dict::new());
    }
    let name = ospath::basename(path);
    let text = std::fs::read(path).map_err(|e| {
        format!(
            "can't read its {name} ({})",
            e.to_string().split(" (os error").next().unwrap_or("")
        )
    })?;
    match serde_json::from_slice::<Value>(&text) {
        Ok(Value::Object(o)) => Ok(o),
        Ok(_) => Err(format!("its {name} isn't a JSON object")),
        Err(_) => Err(format!("can't read its {name} (not valid JSON)")),
    }
}

fn ours(hook: &Value) -> bool {
    s(get(hook, "command")).is_some_and(|c| c.contains(HOOK) && c.contains("hatcheck"))
}

/// The program a hook's command line runs: its first word, unquoted as it was quoted for the
/// shell ('…' by sh_quote, "…" on Windows), or the word as it is.
pub fn hook_program(command: &str) -> String {
    let command = command.trim_start();
    for quote in ['\'', '"'] {
        if let Some(rest) = command.strip_prefix(quote) {
            if let Some(end) = rest.find(quote) {
                return rest[..end].to_string();
            }
        }
    }
    command.split_whitespace().next().unwrap_or("").to_string()
}

/// (agent, profile, home or folder, the program its hook of ours runs) for every hook hatcheck
/// bind --hook added.
pub fn hook_runs() -> Vec<(Agent, String, String)> {
    let mut runs = Vec::new();
    for (agent, folder, path) in hook_files() {
        let Ok(settings) = read_settings(&path) else {
            continue;
        };
        let handlers: Vec<Value> = match agent {
            Agent::Agy => agy_handlers(&settings),
            _ => {
                let hooks = settings
                    .get("hooks")
                    .filter(|h| py::truthy(h))
                    .map_or(py::null(), |h| get(h, "SessionStart"));
                match hooks {
                    Value::Array(entries) => entries
                        .iter()
                        .filter(|e| e.is_object())
                        .filter_map(|e| get(e, "hooks").as_array())
                        .flatten()
                        .filter(|h| ours(h))
                        .cloned()
                        .collect(),
                    _ => Vec::new(),
                }
            }
        };
        for hook in handlers {
            let program = hook_program(s(get(&hook, "command")).unwrap_or(""));
            runs.push((agent, folder.clone(), program));
        }
    }
    runs
}

fn has_hook(settings: &Dict) -> bool {
    let hooks = settings
        .get("hooks")
        .filter(|h| py::truthy(h))
        .map_or(py::null(), |h| get(h, "SessionStart"));
    let Value::Array(entries) = hooks else {
        return false;
    };
    entries
        .iter()
        .filter(|e| e.is_object())
        .any(|e| match get(e, "hooks") {
            Value::Array(list) => list.iter().any(ours),
            _ => false,
        })
}

/// Add the hook to Claude Code's or Codex's settings: false when their hooks aren't in the usual form.
fn add_hook(settings: &mut Dict, line: &str) -> bool {
    let hooks = settings.entry("hooks").or_insert_with(|| json!({}));
    let Some(hooks) = hooks.as_object_mut() else {
        return false;
    };
    let Some(list) = hooks
        .entry("SessionStart")
        .or_insert_with(|| json!([]))
        .as_array_mut()
    else {
        return false;
    };
    list.push(json!({"hooks": [{"type": "command", "command": line}]}));
    true
}

/// Take the hooks of ours out of Claude Code's or Codex's settings.
fn remove_hook(settings: &mut Dict) {
    let Some(Value::Object(hooks)) = settings.get_mut("hooks") else {
        return;
    };
    let entries = hooks
        .get("SessionStart")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut kept = Vec::new();
    for mut entry in entries {
        if let Some(Value::Array(list)) = entry.as_object_mut().and_then(|e| e.get_mut("hooks")) {
            list.retain(|h| !ours(h));
            if list.is_empty() {
                continue;
            }
        }
        kept.push(entry);
    }
    if kept.is_empty() {
        hooks.remove("SessionStart");
    } else {
        hooks.insert("SessionStart".into(), Value::Array(kept));
    }
    if hooks.is_empty() {
        settings.remove("hooks");
    }
}

/// The handlers of ours in agy's hooks.json, in any named hook.
fn agy_handlers(settings: &Dict) -> Vec<Value> {
    settings
        .values()
        .filter(|v| v.is_object())
        .flat_map(|named| match get(named, "SessionStart") {
            Value::Array(list) => list.clone(),
            _ => Vec::new(),
        })
        .filter(ours)
        .collect()
}

fn agy_has_hook(settings: &Dict) -> bool {
    !agy_handlers(settings).is_empty()
}

/// agy's repoint_hooks: the command the first handler of ours that ran something else ran.
fn agy_repoint(settings: &mut Dict, line: &str) -> Option<String> {
    let mut old = None;
    for named in settings.values_mut() {
        let Some(Value::Array(list)) = named
            .as_object_mut()
            .and_then(|n| n.get_mut("SessionStart"))
        else {
            continue;
        };
        for hook in list.iter_mut().filter(|h| ours(h)) {
            let was = s(get(hook, "command")).unwrap_or("").to_string();
            if was != line {
                old.get_or_insert(was);
                hook["command"] = json!(line);
            }
        }
    }
    old
}

/// Add the hook to agy's hooks.json, under the hatcheck name: false when what's there under
/// that name isn't a hook.
fn agy_add(settings: &mut Dict, line: &str) -> bool {
    let named = settings.entry(AGY_HOOK).or_insert_with(|| json!({}));
    let Some(named) = named.as_object_mut() else {
        return false;
    };
    let Some(list) = named
        .entry("SessionStart")
        .or_insert_with(|| json!([]))
        .as_array_mut()
    else {
        return false;
    };
    list.push(json!({"type": "command", "command": line}));
    true
}

/// Take the handlers of ours out of agy's hooks.json, and a named hook left with nothing.
fn agy_remove(settings: &mut Dict) {
    let names: Vec<String> = settings.keys().cloned().collect();
    for name in names {
        let Some(named) = settings.get_mut(&name).and_then(Value::as_object_mut) else {
            continue;
        };
        if let Some(Value::Array(list)) = named.get_mut("SessionStart") {
            list.retain(|h| !ours(h));
            if list.is_empty() {
                named.remove("SessionStart");
            }
        }
        if named.keys().all(|k| k == "enabled") {
            settings.shift_remove(&name);
        }
    }
}

/// Point the hooks of ours in settings at line, what they should run now: the command the first
/// one that ran something else ran, or None when they all run line already.
fn repoint_hooks(settings: &mut Dict, line: &str) -> Option<String> {
    let entries = settings
        .get_mut("hooks")?
        .get_mut("SessionStart")?
        .as_array_mut()?;
    let mut old = None;
    for entry in entries.iter_mut() {
        let Some(Value::Array(list)) = entry.as_object_mut().and_then(|e| e.get_mut("hooks"))
        else {
            continue;
        };
        for hook in list.iter_mut().filter(|h| ours(h)) {
            let was = s(get(hook, "command")).unwrap_or("").to_string();
            if was != line {
                old.get_or_insert(was);
                hook["command"] = json!(line);
            }
        }
    }
    old
}

/// (agent, profile, home or folder, the file its hooks are in) for every profile, Codex home
/// and agy folder: agy's hooks are in ~/.gemini/config/hooks.json, beside its folder.
fn hook_files() -> Vec<(Agent, String, String)> {
    let mut files: Vec<(Agent, String, String)> = paths::find_profiles()
        .into_iter()
        .map(|p| (Agent::Claude, p.clone(), ospath::join(&p, "settings.json")))
        .collect();
    files.extend(
        paths::find_codex_homes()
            .into_iter()
            .map(|h| (Agent::Codex, h.clone(), ospath::join(&h, "hooks.json"))),
    );
    files.extend(paths::find_agy_roots().into_iter().map(|r| {
        let beside = ospath::dirname(r.trim_end_matches(ospath::is_sep));
        (
            Agent::Agy,
            r.clone(),
            ospath::join_all(&beside, &["config", "hooks.json"]),
        )
    }));
    files
}

/// How Claude Code and Codex run the hook: hatcheck from your PATH, or this program.
fn hook_command() -> String {
    let quote = |s: &str| {
        if ospath::WINDOWS {
            format!("\"{s}\"")
        } else {
            paths::sh_quote(s)
        }
    };
    let exe = ospath::which("hatcheck").or_else(|| {
        std::env::current_exe()
            .ok()
            .map(|p| p.to_string_lossy().into_owned())
    });
    format!(
        "{} {HOOK}",
        quote(&exe.unwrap_or_else(|| "hatcheck".into()))
    )
}

#[allow(clippy::too_many_arguments)]
fn bind(
    profile: Option<&str>,
    folder: Option<&str>,
    codex: Option<&str>,
    agy: Option<&str>,
    kimi: Option<&str>,
    zcode: Option<&str>,
    dsh: Option<&str>,
    mcode: Option<&str>,
    qwen: Option<&str>,
    copilot: Option<&str>,
    opencode: Option<&str>,
) -> i32 {
    let show = paths::shell_path;
    let folder = ospath::realpath(&ospath::expanduser(
        &folder.map(String::from).unwrap_or_else(ospath::cwd),
    ));
    if !ospath::isdir(&folder) {
        return out::fail(&format!("{} isn't a folder", show(&folder)));
    }
    let mut bindings = paths::load_bindings();
    let old = bindings.get(&folder).cloned().unwrap_or(json!({}));
    let mut binding = Dict::new();
    if profile.is_none() {
        if let Some(claude) = old.get(Agent::Claude.key()) {
            binding.insert(Agent::Claude.key().into(), claude.clone());
        }
    }
    let mut notes = Vec::new();
    if let Some(spec) = profile {
        let (name, path) = name_of(Agent::Claude, spec);
        if !ospath::isdir(&path) {
            let how = if name == "default" {
                "claude".to_string()
            } else {
                format!("cl {name}")
            };
            return out::fail(&format!(
                "{} isn't a profile folder. Sign in there first, e.g. with: {how}",
                show(&path)
            ));
        }
        binding.insert(Agent::Claude.key().into(), json!(name));
    }
    if let Some(spec) = codex {
        let (name, path) = name_of(Agent::Codex, spec);
        if !ospath::isdir(&path) {
            let how = if name == "default" {
                "codex login".to_string()
            } else {
                format!("cx {name} login")
            };
            return out::fail(&format!(
                "{} isn't a Codex home. Sign in there first, e.g. with: {how}",
                show(&path)
            ));
        }
        binding.insert(Agent::Codex.key().into(), json!(name));
    } else if profile.is_none() {
        if let Some(name) = old.get(Agent::Codex.key()) {
            binding.insert(Agent::Codex.key().into(), name.clone());
        }
    } else if let Some(spec) = profile.filter(|p| {
        s(binding.get(Agent::Claude.key()).unwrap_or(py::null())) == Some(*p)
            && !p.contains('/')
            && !p.contains(ospath::SEP)
    }) {
        let path = paths::profile_folder(Agent::Codex, spec); // the home named like the profile
        if ospath::isdir(&path) {
            binding.insert(Agent::Codex.key().into(), json!(spec));
        } else {
            notes.push(format!(
                "Codex isn't bound: there's no {} (add --codex NAME for another home).",
                show(&path)
            ));
        }
    }
    if let Some(spec) = agy {
        let (name, path) = name_of(Agent::Agy, spec);
        if !crate::agy::is_root(&path) {
            return out::fail(&format!(
                "{} isn't an Antigravity CLI folder. Run agy once to sign in",
                show(&path)
            ));
        }
        binding.insert(Agent::Agy.key().into(), json!(name));
    } else if profile.is_none() {
        if let Some(name) = old.get(Agent::Agy.key()) {
            binding.insert(Agent::Agy.key().into(), name.clone());
        }
    } else if profile == Some("default") && crate::agy::is_root(&paths::default_folder(Agent::Agy))
    {
        // default binds every agent's own folder; agy has no other.
        binding.insert(Agent::Agy.key().into(), json!("default"));
    }
    // Kimi homes are named like Codex's; ZCode has one folder, like agy.
    if let Some(spec) = kimi {
        let (name, path) = name_of(Agent::Kimi, spec);
        if !crate::kimi::is_home(&path) || !ospath::isdir(&path) {
            let how = if name == "default" {
                "kimi login".to_string()
            } else {
                format!("KIMI_CODE_HOME={} kimi login", show(&path))
            };
            return out::fail(&format!(
                "{} isn't a Kimi home. Sign in there first, e.g. with: {how}",
                show(&path)
            ));
        }
        binding.insert(Agent::Kimi.key().into(), json!(name));
    } else if profile.is_none() {
        if let Some(name) = old.get(Agent::Kimi.key()) {
            binding.insert(Agent::Kimi.key().into(), name.clone());
        }
    } else if let Some(spec) = profile.filter(|p| !p.contains('/') && !p.contains(ospath::SEP)) {
        let path = paths::profile_folder(Agent::Kimi, spec); // the home named like the profile
        if ospath::isdir(&ospath::join(&path, "sessions")) {
            binding.insert(Agent::Kimi.key().into(), json!(spec));
        }
    }
    if let Some(spec) = zcode {
        let (name, path) = name_of(Agent::Zcode, spec);
        if !crate::zcode::is_root(&path) {
            return out::fail(&format!(
                "{} isn't a ZCode data folder. Run zcode once to sign in",
                show(&path)
            ));
        }
        binding.insert(Agent::Zcode.key().into(), json!(name));
    } else if profile.is_none() {
        if let Some(name) = old.get(Agent::Zcode.key()) {
            binding.insert(Agent::Zcode.key().into(), name.clone());
        }
    } else if profile == Some("default")
        && crate::zcode::is_root(&paths::default_folder(Agent::Zcode))
    {
        binding.insert(Agent::Zcode.key().into(), json!("default"));
    }
    // DSH_HOME moves a whole home; its own plugin-stack profiles are unrelated to these bindings.
    if let Some(spec) = dsh {
        let (name, path) = name_of(Agent::Dsh, spec);
        if !crate::dsh::is_home(&path) || !ospath::isdir(&path) {
            return out::fail(&format!(
                "{} isn't a DeepSeek Harness home. Sign in there first, e.g. with: {}",
                show(&path),
                paths::start_command(Agent::Dsh, &path, false)
            ));
        }
        binding.insert(Agent::Dsh.key().into(), json!(name));
    } else if profile.is_none() {
        if let Some(name) = old.get(Agent::Dsh.key()) {
            binding.insert(Agent::Dsh.key().into(), name.clone());
        }
    } else if profile == Some("default")
        && ospath::isdir(&ospath::join(
            &paths::default_folder(Agent::Dsh),
            "sessions",
        ))
    {
        binding.insert(Agent::Dsh.key().into(), json!("default"));
    } else if let Some(spec) = profile.filter(|p| !p.contains('/') && !p.contains(ospath::SEP)) {
        let path = paths::profile_folder(Agent::Dsh, spec);
        if ospath::isdir(&ospath::join(&path, "sessions")) {
            binding.insert(Agent::Dsh.key().into(), json!(spec));
        }
    }
    // MCode's installer is ~/.minimax-code, not a data profile named code; only a real v2 store binds.
    if let Some(spec) = mcode {
        let (name, path) = name_of(Agent::Mcode, spec);
        if !crate::mcode::is_home(&path) || !ospath::isdir(&path) {
            return out::fail(&format!(
                "{} isn't a MiniMax Code data home. Sign in there first, e.g. with: {}",
                show(&path),
                paths::start_command(Agent::Mcode, &path, false) + " login"
            ));
        }
        binding.insert(Agent::Mcode.key().into(), json!(name));
    } else if profile.is_none() {
        if let Some(name) = old.get(Agent::Mcode.key()) {
            binding.insert(Agent::Mcode.key().into(), name.clone());
        }
    } else if profile == Some("default")
        && crate::mcode::is_home(&paths::default_folder(Agent::Mcode))
    {
        binding.insert(Agent::Mcode.key().into(), json!("default"));
    } else if let Some(spec) = profile.filter(|p| !p.contains('/') && !p.contains(ospath::SEP)) {
        let path = paths::profile_folder(Agent::Mcode, spec);
        if crate::mcode::is_home(&path) {
            binding.insert(Agent::Mcode.key().into(), json!(spec));
        }
    }
    // QWEN_HOME moves a whole home; a folder qwen has run in has its installation_id or chats.
    if let Some(spec) = qwen {
        let (name, path) = name_of(Agent::Qwen, spec);
        if !crate::qwen::is_home(&path) || !ospath::isdir(&path) {
            return out::fail(&format!(
                "{} isn't a Qwen Code home. Sign in there first, e.g. with: {}",
                show(&path),
                paths::start_command(Agent::Qwen, &path, false)
            ));
        }
        binding.insert(Agent::Qwen.key().into(), json!(name));
    } else if profile.is_none() {
        if let Some(name) = old.get(Agent::Qwen.key()) {
            binding.insert(Agent::Qwen.key().into(), name.clone());
        }
    } else if profile == Some("default")
        && crate::qwen::is_home(&paths::default_folder(Agent::Qwen))
    {
        binding.insert(Agent::Qwen.key().into(), json!("default"));
    } else if let Some(spec) = profile.filter(|p| !p.contains('/') && !p.contains(ospath::SEP)) {
        let path = paths::profile_folder(Agent::Qwen, spec);
        if crate::qwen::is_home(&path) {
            binding.insert(Agent::Qwen.key().into(), json!(spec));
        }
    }
    // COPILOT_HOME moves a whole home; a folder copilot has run in has its session-state folder
    // or config.json.
    if let Some(spec) = copilot {
        let (name, path) = name_of(Agent::Copilot, spec);
        if !crate::copilot::is_home(&path) || !ospath::isdir(&path) {
            return out::fail(&format!(
                "{} isn't a GitHub Copilot CLI home. Sign in there first, e.g. with: {}",
                show(&path),
                paths::copilot_command(&path, "login", false)
            ));
        }
        binding.insert(Agent::Copilot.key().into(), json!(name));
    } else if profile.is_none() {
        if let Some(name) = old.get(Agent::Copilot.key()) {
            binding.insert(Agent::Copilot.key().into(), name.clone());
        }
    } else if profile == Some("default")
        && crate::copilot::is_home(&paths::default_folder(Agent::Copilot))
    {
        binding.insert(Agent::Copilot.key().into(), json!("default"));
    } else if let Some(spec) = profile.filter(|p| !p.contains('/') && !p.contains(ospath::SEP)) {
        let path = paths::profile_folder(Agent::Copilot, spec);
        if crate::copilot::is_home(&path) {
            binding.insert(Agent::Copilot.key().into(), json!(spec));
        }
    }
    // OpenCode has one data folder, like agy and zcode.
    if let Some(spec) = opencode {
        let (name, path) = name_of(Agent::Opencode, spec);
        if !crate::opencode::is_home(&path) {
            return out::fail(&format!(
                "{} isn't an OpenCode data folder. Run opencode once to make it",
                show(&path)
            ));
        }
        binding.insert(Agent::Opencode.key().into(), json!(name));
    } else if profile.is_none() {
        if let Some(name) = old.get(Agent::Opencode.key()) {
            binding.insert(Agent::Opencode.key().into(), name.clone());
        }
    } else if profile == Some("default")
        && crate::opencode::is_home(&paths::default_folder(Agent::Opencode))
    {
        binding.insert(Agent::Opencode.key().into(), json!("default"));
    }
    let binding = Value::Object(binding);
    bindings.insert(folder.clone(), binding.clone());
    if let Err(e) = save_bindings(&bindings) {
        return out::fail(&e.to_string());
    }
    out::success(&format!(
        "Bound {} and everything under it: {}.",
        show(&folder),
        describe(&binding)
    ));
    for note in notes {
        out::warn(&note);
    }
    let bound = |agent: Agent| {
        s(get(&binding, agent.key()))
            .filter(|n| !n.is_empty())
            .map(String::from)
    };
    let mut here = Vec::new();
    if let Some(name) = bound(Agent::Claude) {
        here.push(format!(
            "cl --resume <id> means {} --resume <id>",
            paths::start_command(
                Agent::Claude,
                &paths::profile_folder(Agent::Claude, &name),
                false
            )
        ));
    }
    if let Some(name) = bound(Agent::Codex) {
        here.push(format!(
            "cx resume <id> means {} resume <id>",
            paths::start_command(
                Agent::Codex,
                &paths::profile_folder(Agent::Codex, &name),
                false
            )
        ));
    }
    if let Some(name) = bound(Agent::Kimi) {
        here.push(format!(
            "ck --session <id> means {} --session <id>",
            paths::start_command(
                Agent::Kimi,
                &paths::profile_folder(Agent::Kimi, &name),
                false
            )
        ));
    }
    if !here.is_empty() {
        out::println(&format!("Here, {}.", here.join(" and ")));
    }
    if !hook_files()
        .iter()
        .any(|(_, _, path)| read_settings(path).is_ok_and(|s| has_hook(&s)))
    {
        out::println(&format!(
            "{} adds a warning to Claude Code, Codex and agy when a session starts here under another profile.",
            out::Paint::new().command("hatcheck bind --hook")
        ));
    }
    0
}

fn show_here() -> i32 {
    let bindings = paths::load_bindings();
    let cwd = ospath::cwd();
    let Some(folder) = paths::bound_folder(Some(&cwd), &bindings) else {
        let command = out::Paint::new().command("hatcheck bind <profile>");
        out::println(&format!("This folder isn't bound. Bind it with: {command}"));
        return 0;
    };
    let where_ = if paths::same_folder(&folder, &cwd) {
        "This folder".to_string()
    } else {
        format!("This folder is under {}, which", paths::shell_path(&folder))
    };
    out::println(&format!(
        "{where_} is bound to {}.",
        describe(&bindings[&folder])
    ));
    0
}

fn list_bindings() -> i32 {
    let bindings = paths::load_bindings();
    if bindings.is_empty() {
        let command = out::Paint::new().command("hatcheck bind <profile>");
        out::println(&format!(
            "No folders are bound. Bind this one with: {command}"
        ));
        return 0;
    }
    out::heading(&format!(
        "Bound folders, from {}:",
        paths::shell_path(&paths::bindings_path())
    ));
    let width = bindings
        .keys()
        .map(|f| paths::shell_path(f).chars().count())
        .max()
        .unwrap_or(0);
    let mut folders: Vec<&String> = bindings.keys().collect();
    folders.sort_by_key(|f| f.to_lowercase());
    for folder in folders {
        let line = format!(
            "  {:<width$}  {}",
            paths::shell_path(folder),
            describe(&bindings[folder])
        );
        if ospath::isdir(folder) {
            out::println(&line);
        } else {
            out::warn(&format!("{line}  (folder missing)"));
        }
    }
    0
}

/// For cl and cx: the name the folder (this one by default) is bound to, or exit 1.
fn which(agent: Agent, folder: Option<&str>) -> i32 {
    let bindings = paths::load_bindings();
    let path = ospath::expanduser(&folder.map(String::from).unwrap_or_else(ospath::cwd));
    let name = paths::bound_folder(Some(&path), &bindings)
        .and_then(|f| s(get(&bindings[&f], agent.key())).map(String::from));
    match name.filter(|n| !n.is_empty()) {
        Some(n) => {
            out::println(&n);
            0
        }
        None => 1,
    }
}
fn add_hooks() -> i32 {
    let command = hook_command();
    let paint = out::Paint::new();
    let mut codex = false;
    for (agent, folder, path) in hook_files() {
        let show = paths::shell_path(&folder);
        let mut settings = match read_settings(&path) {
            Ok(s) => s,
            Err(why) => {
                out::warn(&format!("  {show}  skipped: {why}"));
                continue;
            }
        };
        // Claude Code's hook runs the plain check; another agent's names the agent.
        let line = match agent {
            Agent::Claude => command.clone(),
            other => format!("{command} {}", other.key()),
        };
        let has = match agent {
            Agent::Agy => agy_has_hook(&settings),
            _ => has_hook(&settings),
        };
        if has {
            // It may run a hatcheck that has gone, after a switch of install: point it at this one.
            let old = match agent {
                Agent::Agy => agy_repoint(&mut settings, &line),
                _ => repoint_hooks(&mut settings, &line),
            };
            let Some(old) = old else {
                out::note(&format!("  {show}  already has it"));
                continue;
            };
            if let Err(e) = write_json(&path, &Value::Object(settings)) {
                out::warn(&format!("  {show}  skipped: {e}"));
                continue;
            }
            out::success(&format!(
                "  {show}  updated, in {}: it ran {old}",
                ospath::basename(&path)
            ));
            codex |= agent == Agent::Codex;
            continue;
        }
        let added = match agent {
            Agent::Agy => agy_add(&mut settings, &line),
            _ => add_hook(&mut settings, &line),
        };
        if !added {
            out::warn(&format!(
                "  {show}  skipped: its hooks aren't in the usual form"
            ));
            continue;
        }
        if let Err(e) = write_json(&path, &Value::Object(settings)) {
            out::warn(&format!("  {show}  skipped: {e}"));
            continue;
        }
        out::success(&format!("  {show}  added, in {}", ospath::basename(&path)));
        codex |= agent == Agent::Codex;
    }
    out::println(&format!(
        "They run: {} (with codex or agy after it for Codex and agy)",
        paint.command(&command)
    ));
    out::println(&format!(
        "Claude Code, Codex and agy warn when a session starts in a bound folder under another profile. {} removes them.",
        paint.command("hatcheck unbind --hook")
    ));
    if codex {
        out::println("Codex runs a new hook only once you trust it: in each Codex home, open /hooks in codex (or cx <name>) and trust it.");
    }
    0
}
fn remove_hooks() -> i32 {
    for (agent, folder, path) in hook_files() {
        let Ok(mut settings) = read_settings(&path) else {
            continue;
        };
        let has = match agent {
            Agent::Agy => agy_has_hook(&settings),
            _ => has_hook(&settings),
        };
        if !has {
            continue;
        }
        match agent {
            Agent::Agy => agy_remove(&mut settings),
            _ => remove_hook(&mut settings),
        }
        let written = if agent != Agent::Claude && settings.is_empty() {
            std::fs::remove_file(&path) // hooks.json held nothing else
        } else {
            write_json(&path, &Value::Object(settings))
        };
        if written.is_ok() {
            out::success(&format!("  {}  removed", paths::shell_path(&folder)));
        }
    }
    out::println("No profile, home or folder runs the hook now.");
    0
}

/// Run by the SessionStart hook in Claude Code, or in Codex (agent codex): when the session's
/// folder is bound to another profile or home, have it show a warning. Never blocks, and stays
/// quiet on any error.
fn check(agent: Agent) -> i32 {
    let mut text = String::new();
    let _ = std::io::Read::read_to_string(&mut std::io::stdin(), &mut text);
    let Ok(data) = serde_json::from_str::<Value>(if text.is_empty() { "{}" } else { &text }) else {
        return 0;
    };
    if !data.is_object() {
        return 0;
    }
    // Claude Code and Codex say cwd and session_id; agy workspacePaths and conversationId.
    let workspace = get(&data, "workspacePaths")
        .as_array()
        .and_then(|w| w.first())
        .map(py::str_of);
    let cwd = s(get(&data, "cwd"))
        .map(String::from)
        .or(workspace)
        .filter(|c| !c.is_empty())
        .unwrap_or_else(ospath::cwd);
    let current = ospath::expanduser(
        &agent
            .variable()
            .and_then(|v| std::env::var(v).ok())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| paths::default_folder(agent)),
    );
    let bindings = paths::load_bindings();
    let Some(folder) = paths::bound_folder(Some(&cwd), &bindings) else {
        return 0;
    };
    let Some(name) = s(get(&bindings[&folder], agent.key()))
        .filter(|n| !n.is_empty())
        .map(String::from)
    else {
        return 0;
    };
    let want = paths::profile_folder(agent, &name);
    if paths::same_folder(&want, &current) {
        return 0;
    }
    let start = paths::start_command(agent, &want, false);
    let sid = s(get(&data, "session_id"))
        .or_else(|| s(get(&data, "conversationId")))
        .unwrap_or("")
        .to_string();
    let todo = if s(get(&data, "source")) == Some("resume") && !sid.is_empty() {
        // How hatcheck move names the bound profile: a path or NAME as given, default for
        // Claude Code's, codex or agy for theirs, and codex:NAME for another home.
        let to = if name.contains('/') || name.contains(ospath::SEP) || agent == Agent::Claude {
            name.clone()
        } else if name == "default" {
            agent.key().into()
        } else {
            format!("{}:{name}", agent.key())
        };
        let resume = paths::session_command(agent, &want, &sid, false, None);
        format!("Quit, then carry this session there with: hatcheck move {} {to}, and resume it with {resume}", py::head(&sid, 8))
    } else {
        format!("Quit and run {start} here instead")
    };
    let message = format!(
        "hatcheck: {} is bound to {}, but this session runs in {}. {todo}.",
        paths::shell_path(&folder),
        paths::shell_path(&want),
        paths::shell_path(&current)
    );
    // Each agent reads its hook's answer in its own form.
    let answer = match agent {
        Agent::Agy => json!({"injectSteps": [{"systemMessage": {"systemMessage": message}}]}),
        _ => json!({"systemMessage": message}),
    };
    out::println(&py::dumps_ascii(&answer));
    0
}

pub fn main(argv: &[String], prog: &str) -> i32 {
    let parser = Parser::new(prog, "Tie a folder to a profile, so cl, cx and ck pick it there and the session list flags work done under another one.")
        .positional("profile", Count::Optional, "NAME for ~/.claude-NAME, default for ~/.claude, or a path")
        .positional("folder", Count::Optional, "the folder to bind (default: this one)")
        .option(&["--codex"], "codex", "HOME", "the Codex home to bind: NAME for ~/.codex-NAME, default for ~/.codex, or a path (default: the one named like PROFILE)")
        .option(&["--agy"], "agy", "FOLDER", "the Antigravity CLI folder to bind: default for ~/.gemini/antigravity-cli, or a path (default: that one, when PROFILE is default)")
        .option(&["--kimi"], "kimi", "HOME", "the Kimi home to bind: NAME for ~/.kimi-code-NAME, default for ~/.kimi-code, or a path (default: the one named like PROFILE)")
        .option(&["--zcode"], "zcode", "FOLDER", "the ZCode folder to bind: default for ~/.zcode, or a path (default: that one, when PROFILE is default)")
        .option(&["--dsh"], "dsh", "HOME", "the DeepSeek Harness home to bind: NAME for ~/.dsh-NAME, default for ~/.dsh, or a path (default: the one named like PROFILE)")
        .option(&["--mcode"], "mcode", "HOME", "the MiniMax Code data home to bind: NAME for ~/.minimax-NAME, default for ~/.minimax, or a path (default: the one named like PROFILE)")
        .option(&["--qwen"], "qwen", "HOME", "the Qwen Code home to bind: NAME for ~/.qwen-NAME, default for ~/.qwen, or a path (default: the one named like PROFILE)")
        .option(&["--copilot"], "copilot", "HOME", "the GitHub Copilot CLI home to bind: NAME for ~/.copilot-NAME, default for ~/.copilot, or a path (default: the one named like PROFILE)")
        .option(&["--opencode"], "opencode", "FOLDER", "the OpenCode data folder to bind: default for ~/.local/share/opencode, or a path (default: that one, when PROFILE is default)")
        .flag(&["--list"], "list", "list every bound folder")
        .flag(&["--hook"], "hook", "warn in Claude Code, Codex and agy when a session starts in a bound folder under another profile")
        .hidden_choice(&["--which"], "which", "claude", &Agent::KEYS)
        .hidden_choice(&["--check"], "check", "claude", &Agent::KEYS);
    let args = match parser.parse(argv) {
        Ok(a) => a,
        Err(stop) => return stop.exit_code(),
    };
    let profile = args.value("profile").filter(|p| !p.is_empty());
    if let Some(agent) = args.value("check").and_then(Agent::from_key) {
        return check(agent);
    }
    if let Some(agent) = args.value("which").and_then(Agent::from_key) {
        return which(agent, profile);
    }
    out::setup_console();
    if args.flag("list") {
        return list_bindings();
    }
    if args.flag("hook") {
        return add_hooks();
    }
    let codex = args.value("codex").filter(|c| !c.is_empty());
    let agy = args.value("agy").filter(|a| !a.is_empty());
    let kimi = args.value("kimi").filter(|k| !k.is_empty());
    let zcode = args.value("zcode").filter(|z| !z.is_empty());
    let dsh = args.value("dsh").filter(|d| !d.is_empty());
    let mcode = args.value("mcode").filter(|m| !m.is_empty());
    let qwen = args.value("qwen").filter(|q| !q.is_empty());
    let copilot = args.value("copilot").filter(|c| !c.is_empty());
    let opencode = args.value("opencode").filter(|o| !o.is_empty());
    if profile.is_some()
        || codex.is_some()
        || agy.is_some()
        || kimi.is_some()
        || zcode.is_some()
        || dsh.is_some()
        || mcode.is_some()
        || qwen.is_some()
        || copilot.is_some()
        || opencode.is_some()
    {
        return bind(
            profile,
            args.value("folder").filter(|f| !f.is_empty()),
            codex,
            agy,
            kimi,
            zcode,
            dsh,
            mcode,
            qwen,
            copilot,
            opencode,
        );
    }
    show_here()
}

pub fn unbind_main(argv: &[String], prog: &str) -> i32 {
    let parser = Parser::new(prog, "Remove a folder's binding, or the warning hook.")
        .positional(
            "folder",
            Count::Optional,
            "the bound folder (default: this one)",
        )
        .flag(
            &["--hook"],
            "hook",
            "remove the warning hook from every profile",
        );
    let args = match parser.parse(argv) {
        Ok(a) => a,
        Err(stop) => return stop.exit_code(),
    };
    if args.flag("hook") {
        return remove_hooks();
    }
    let show = paths::shell_path;
    let folder = ospath::realpath(&ospath::expanduser(
        &args
            .value("folder")
            .filter(|f| !f.is_empty())
            .map(String::from)
            .unwrap_or_else(ospath::cwd),
    ));
    let mut bindings = paths::load_bindings();
    if let Some(key) = bindings
        .keys()
        .find(|f| paths::same_folder(f, &folder))
        .cloned()
    {
        let binding = bindings.shift_remove(&key).unwrap_or(Value::Null);
        if let Err(e) = save_bindings(&bindings) {
            return out::fail(&e.to_string());
        }
        out::success(&format!(
            "Unbound {} (it was bound to {}).",
            show(&folder),
            describe(&binding)
        ));
        return 0;
    }
    if let Some(parent) = paths::bound_folder(Some(&folder), &bindings) {
        return out::fail(&format!(
            "{} isn't bound itself; it's under {}, which is: hatcheck unbind {}",
            show(&folder),
            show(&parent),
            show(&parent)
        ));
    }
    out::fail(&format!("{} isn't bound", show(&folder)))
}
