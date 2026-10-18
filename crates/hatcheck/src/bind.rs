//! hatcheck bind and unbind: tie a folder to a profile, so work there happens under the right
//! account; and the warning hook, a SessionStart hook in every Claude Code profile's
//! settings.json and every Codex home's hooks.json, which runs hatcheck bind --check. The
//! README's `hatcheck bind` section describes them.

use crate::args::{Count, Parser};
use crate::ospath;
use crate::out;
use crate::paths;
use crate::py::{self, get, s, Dict};
use serde_json::{json, Value};

const HOOK: &str = "bind --check"; // in the hook's command, to find it again

/// Write JSON whole, under a temporary name until it's complete, as Python's json.dump with
/// indent=2 writes it, and a newline.
fn write_json(path: &str, data: &Value) -> std::io::Result<()> {
    std::fs::create_dir_all(ospath::dirname(path))?;
    let tmp = format!("{path}.hatcheck-tmp");
    std::fs::write(&tmp, py::dumps(data) + "\n")?;
    std::fs::rename(&tmp, path)
}

fn save_bindings(bindings: &Dict) -> std::io::Result<()> {
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
fn name_of(agent: &str, spec: &str) -> (String, String) {
    if spec == "default"
        || (!spec.contains('/') && !spec.contains(ospath::SEP) && !spec.starts_with('~'))
    {
        return (spec.to_string(), paths::profile_folder(agent, spec));
    }
    let path = ospath::abspath(&ospath::expanduser(spec));
    (path.clone(), path)
}

fn describe(binding: &Value) -> String {
    let parts: Vec<String> = [("claude", "Claude Code"), ("codex", "Codex")]
        .iter()
        .filter_map(|(agent, label)| {
            let name = s(get(binding, agent)).filter(|n| !n.is_empty())?;
            Some(format!(
                "{label} {}",
                paths::shell_path(&paths::profile_folder(agent, name))
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

/// (agent, profile or home, the file its hooks are in) for every profile and Codex home.
fn hook_files() -> Vec<(&'static str, String, String)> {
    let mut files: Vec<(&'static str, String, String)> = paths::find_profiles()
        .into_iter()
        .map(|p| ("claude", p.clone(), ospath::join(&p, "settings.json")))
        .collect();
    files.extend(
        paths::find_codex_homes()
            .into_iter()
            .map(|h| ("codex", h.clone(), ospath::join(&h, "hooks.json"))),
    );
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

fn bind(profile: Option<&str>, folder: Option<&str>, codex: Option<&str>) -> i32 {
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
        if let Some(claude) = old.get("claude") {
            binding.insert("claude".into(), claude.clone());
        }
    }
    let mut notes = Vec::new();
    if let Some(spec) = profile {
        let (name, path) = name_of("claude", spec);
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
        binding.insert("claude".into(), json!(name));
    }
    if let Some(spec) = codex {
        let (name, path) = name_of("codex", spec);
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
        binding.insert("codex".into(), json!(name));
    } else if let Some(spec) = profile.filter(|p| {
        s(binding.get("claude").unwrap_or(py::null())) == Some(*p)
            && !p.contains('/')
            && !p.contains(ospath::SEP)
    }) {
        let path = paths::profile_folder("codex", spec); // the home named like the profile
        if ospath::isdir(&path) {
            binding.insert("codex".into(), json!(spec));
        } else {
            notes.push(format!(
                "Codex isn't bound: there's no {} (add --codex NAME for another home).",
                show(&path)
            ));
        }
    }
    let binding = Value::Object(binding);
    bindings.insert(folder.clone(), binding.clone());
    if let Err(e) = save_bindings(&bindings) {
        return out::fail(&e.to_string());
    }
    out::println(&format!(
        "Bound {} and everything under it: {}.",
        show(&folder),
        describe(&binding)
    ));
    for note in notes {
        out::println(&note);
    }
    let bound = |agent: &str| {
        s(get(&binding, agent))
            .filter(|n| !n.is_empty())
            .map(String::from)
    };
    let mut here = Vec::new();
    if let Some(name) = bound("claude") {
        here.push(format!(
            "cl --resume <id> means {} --resume <id>",
            paths::start_command("claude", &paths::profile_folder("claude", &name), false)
        ));
    }
    if let Some(name) = bound("codex") {
        here.push(format!(
            "cx resume <id> means {} resume <id>",
            paths::start_command("codex", &paths::profile_folder("codex", &name), false)
        ));
    }
    out::println(&format!("Here, {}.", here.join(" and ")));
    if !hook_files()
        .iter()
        .any(|(_, _, path)| read_settings(path).is_ok_and(|s| has_hook(&s)))
    {
        out::println("hatcheck bind --hook adds a warning to Claude Code when a session starts here under another profile.");
    }
    0
}

fn show_here() -> i32 {
    let bindings = paths::load_bindings();
    let cwd = ospath::cwd();
    let Some(folder) = paths::bound_folder(Some(&cwd), &bindings) else {
        out::println("This folder isn't bound. Bind it with: hatcheck bind <profile>");
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
        out::println("No folders are bound. Bind this one with: hatcheck bind <profile>");
        return 0;
    }
    out::println(&format!(
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
        let missing = if ospath::isdir(folder) {
            ""
        } else {
            "  (folder missing)"
        };
        out::println(&format!(
            "  {:<width$}  {}{missing}",
            paths::shell_path(folder),
            describe(&bindings[folder])
        ));
    }
    0
}

/// For cl and cx: the name the folder (this one by default) is bound to, or exit 1.
fn which(agent: &str, folder: Option<&str>) -> i32 {
    let bindings = paths::load_bindings();
    let path = ospath::expanduser(&folder.map(String::from).unwrap_or_else(ospath::cwd));
    let name = paths::bound_folder(Some(&path), &bindings)
        .and_then(|f| s(get(&bindings[&f], agent)).map(String::from));
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
    let mut codex = false;
    for (agent, folder, path) in hook_files() {
        let show = paths::shell_path(&folder);
        let mut settings = match read_settings(&path) {
            Ok(s) => s,
            Err(why) => {
                out::println(&format!("  {show}  skipped: {why}"));
                continue;
            }
        };
        if has_hook(&settings) {
            out::println(&format!("  {show}  already has it"));
            continue;
        }
        let hooks = settings.entry("hooks").or_insert_with(|| json!({}));
        let Some(hooks) = hooks.as_object_mut() else {
            out::println(&format!(
                "  {show}  skipped: its hooks aren't in the usual form"
            ));
            continue;
        };
        let Some(list) = hooks
            .entry("SessionStart")
            .or_insert_with(|| json!([]))
            .as_array_mut()
        else {
            out::println(&format!(
                "  {show}  skipped: its hooks aren't in the usual form"
            ));
            continue;
        };
        let line = format!("{command}{}", if agent == "codex" { " codex" } else { "" });
        list.push(json!({"hooks": [{"type": "command", "command": line}]}));
        if let Err(e) = write_json(&path, &Value::Object(settings)) {
            out::println(&format!("  {show}  skipped: {e}"));
            continue;
        }
        out::println(&format!("  {show}  added, in {}", ospath::basename(&path)));
        codex |= agent == "codex";
    }
    out::println(&format!(
        "They run: {command} (with codex after it for Codex)"
    ));
    out::println("Claude Code and Codex warn when a session starts in a bound folder under another profile. hatcheck unbind --hook removes them.");
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
        if !has_hook(&settings) {
            continue;
        }
        let Some(Value::Object(hooks)) = settings.get_mut("hooks") else {
            continue;
        };
        let entries = hooks
            .get("SessionStart")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let mut kept = Vec::new();
        for mut entry in entries {
            if let Some(Value::Array(list)) = entry.as_object_mut().and_then(|e| e.get_mut("hooks"))
            {
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
        let written = if agent == "codex" && settings.is_empty() {
            std::fs::remove_file(&path) // hooks.json held nothing else
        } else {
            write_json(&path, &Value::Object(settings))
        };
        if written.is_ok() {
            out::println(&format!("  {}  removed", paths::shell_path(&folder)));
        }
    }
    out::println("No profile or home runs the hook now.");
    0
}

/// Run by the SessionStart hook in Claude Code, or in Codex (agent codex): when the session's
/// folder is bound to another profile or home, have it show a warning. Never blocks, and stays
/// quiet on any error.
fn check(agent: &str) -> i32 {
    let mut text = String::new();
    let _ = std::io::Read::read_to_string(&mut std::io::stdin(), &mut text);
    let Ok(data) = serde_json::from_str::<Value>(if text.is_empty() { "{}" } else { &text }) else {
        return 0;
    };
    if !data.is_object() {
        return 0;
    }
    let cwd = s(get(&data, "cwd"))
        .filter(|c| !c.is_empty())
        .map(String::from)
        .unwrap_or_else(ospath::cwd);
    let (variable, default) = if agent == "codex" {
        ("CODEX_HOME", ".codex")
    } else {
        ("CLAUDE_CONFIG_DIR", ".claude")
    };
    let current = ospath::expanduser(
        &std::env::var(variable)
            .ok()
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| ospath::join(&paths::home(), default)),
    );
    let bindings = paths::load_bindings();
    let Some(folder) = paths::bound_folder(Some(&cwd), &bindings) else {
        return 0;
    };
    let Some(name) = s(get(&bindings[&folder], agent))
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
    let sid = s(get(&data, "session_id")).unwrap_or("").to_string();
    let todo = if s(get(&data, "source")) == Some("resume") && !sid.is_empty() {
        let (to, resume) = if agent == "codex" {
            let to = if name.contains('/') || name.contains(ospath::SEP) {
                name.clone()
            } else if name == "default" {
                "codex".into()
            } else {
                format!("codex:{name}")
            };
            (
                to,
                paths::codex_command(&want, &format!("resume {sid}"), false),
            )
        } else {
            (
                name.clone(),
                paths::claude_command(&want, &format!("--resume {sid}"), false, None),
            )
        };
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
    out::println(&py::dumps_ascii(&json!({"systemMessage": message})));
    0
}

pub fn main(argv: &[String], prog: &str) -> i32 {
    let parser = Parser::new(prog, "Tie a folder to a profile, so cl and cx pick it there and the session list flags work done under another one.")
        .positional("profile", Count::Optional, "NAME for ~/.claude-NAME, default for ~/.claude, or a path")
        .positional("folder", Count::Optional, "the folder to bind (default: this one)")
        .option(&["--codex"], "codex", "HOME", "the Codex home to bind: NAME for ~/.codex-NAME, default for ~/.codex, or a path (default: the one named like PROFILE)")
        .flag(&["--list"], "list", "list every bound folder")
        .flag(&["--hook"], "hook", "warn in Claude Code and Codex when a session starts in a bound folder under another profile")
        .hidden_choice(&["--which"], "which", "claude", &["claude", "codex"])
        .hidden_choice(&["--check"], "check", "claude", &["claude", "codex"]);
    let args = match parser.parse(argv) {
        Ok(a) => a,
        Err(stop) => return stop.exit_code(),
    };
    let profile = args.value("profile").filter(|p| !p.is_empty());
    if let Some(agent) = args.value("check").filter(|a| !a.is_empty()) {
        return check(agent);
    }
    if let Some(agent) = args.value("which").filter(|a| !a.is_empty()) {
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
    if profile.is_some() || codex.is_some() {
        return bind(
            profile,
            args.value("folder").filter(|f| !f.is_empty()),
            codex,
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
        out::println(&format!(
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
