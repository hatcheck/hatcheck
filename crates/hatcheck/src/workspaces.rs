//! hatcheck workspaces: project folders pinned by the user, with their bindings and recent
//! sessions shown by the desktop app. Only Hatcheck's own workspaces.json is written; no agent's
//! configuration or login is touched. docs/commands.md's `hatcheck workspaces` section describes it.

use crate::agent::Agent;
use crate::args::{Count, Parser};
use crate::ospath;
use crate::out;
use crate::paths;
use crate::py;
use serde_json::{json, Value};
use std::collections::BTreeMap;

const VERSION: u64 = 1;

#[derive(Clone, Debug, PartialEq)]
struct Workspace {
    name: String,
    folder: String,
}

pub fn path() -> String {
    ospath::join(&paths::config_dir(), "workspaces.json")
}

fn load() -> Result<Vec<Workspace>, String> {
    let path = path();
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("can't read {}: {e}", paths::shell_path(&path))),
    };
    let data: Value = serde_json::from_str(&text)
        .map_err(|e| format!("can't read {}: {e}", paths::shell_path(&path)))?;
    if data.get("version").and_then(Value::as_u64) != Some(VERSION) {
        return Err(format!(
            "{} isn't a version-{} workspace list",
            paths::shell_path(&path),
            VERSION
        ));
    }
    let rows = data
        .get("workspaces")
        .and_then(Value::as_array)
        .ok_or_else(|| format!("{} has no workspace list", paths::shell_path(&path)))?;
    let mut found = BTreeMap::new();
    for row in rows {
        let name = row.get("name").and_then(Value::as_str).unwrap_or("").trim();
        let folder = row.get("folder").and_then(Value::as_str).unwrap_or("");
        if name.is_empty() || folder.is_empty() || !ospath::isabs(folder) {
            return Err(format!(
                "{} has a workspace without a name and absolute folder",
                paths::shell_path(&path)
            ));
        }
        found.insert(
            ospath::normcase(&ospath::realpath(folder)),
            Workspace {
                name: name.to_string(),
                folder: ospath::abspath(folder),
            },
        );
    }
    Ok(found.into_values().collect())
}

fn save(workspaces: &[Workspace]) -> Result<(), String> {
    let mut rows = workspaces.to_vec();
    rows.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then_with(|| a.folder.cmp(&b.folder))
    });
    let data = json!({"version": VERSION, "workspaces": rows.iter().map(|w|
        json!({"name": w.name, "folder": w.folder})).collect::<Vec<_>>()});
    crate::bind::write_json(&path(), &data)
        .map_err(|e| format!("can't write {}: {e}", paths::shell_path(&path())))
}

fn folder(text: &str) -> String {
    ospath::realpath(&ospath::abspath(&ospath::expanduser(text)))
}

fn add(text: &str, name: Option<&str>) -> Result<Workspace, String> {
    let folder = folder(text);
    if !ospath::isdir(&folder) {
        return Err(format!(
            "Workspace folder {} isn't there",
            paths::shell_path(&folder)
        ));
    }
    let name = name
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .map(String::from)
        .unwrap_or_else(|| ospath::basename(&folder));
    if name.is_empty() {
        return Err("Give the workspace a name with --name".into());
    }
    let mut workspaces = load()?;
    let workspace = Workspace {
        name,
        folder: folder.clone(),
    };
    workspaces.retain(|w| !paths::same_folder(&w.folder, &folder));
    workspaces.push(workspace.clone());
    save(&workspaces)?;
    Ok(workspace)
}

fn remove(text: &str) -> Result<Workspace, String> {
    let folder = folder(text);
    let mut workspaces = load()?;
    let Some(index) = workspaces
        .iter()
        .position(|w| paths::same_folder(&w.folder, &folder))
    else {
        return Err(format!(
            "{} isn't a pinned workspace",
            paths::shell_path(&folder)
        ));
    };
    let removed = workspaces.remove(index);
    save(&workspaces)?;
    Ok(removed)
}

fn as_json(workspace: &Workspace) -> Value {
    let bindings = paths::load_bindings();
    let bound = paths::bound_folder(Some(&workspace.folder), &bindings)
        .and_then(|folder| bindings.get(&folder).cloned())
        .unwrap_or_else(|| json!({}));
    let profiles: BTreeMap<String, String> = Agent::ALL
        .iter()
        .filter(|agent| !crate::config::ignores(**agent))
        .filter_map(|agent| {
            let name = py::s(py::get(&bound, agent.key()))?;
            (!name.is_empty()).then(|| {
                (
                    agent.key().to_string(),
                    ospath::abspath(&paths::profile_folder(*agent, name)),
                )
            })
        })
        .collect();
    json!({"name": workspace.name, "folder": ospath::abspath(&workspace.folder),
        "exists": ospath::isdir(&workspace.folder), "bindings": profiles})
}

fn list(json_output: bool) -> Result<(), String> {
    let workspaces = load()?;
    if json_output {
        out::println(&py::dumps(&json!({"version": VERSION,
            "workspaces": workspaces.iter().map(as_json).collect::<Vec<_>>()})));
        return Ok(());
    }
    if workspaces.is_empty() {
        out::human("No pinned workspaces yet. Add this folder with: hatcheck workspaces add");
        return Ok(());
    }
    for workspace in &workspaces {
        let missing = if ospath::isdir(&workspace.folder) {
            ""
        } else {
            " (folder isn't there)"
        };
        out::human(&format!(
            "{}  {}{missing}",
            workspace.name,
            paths::shell_path(&workspace.folder)
        ));
    }
    Ok(())
}

pub fn main(argv: &[String], prog: &str) -> i32 {
    let parser = Parser::new(
        prog,
        "Pin project folders as workspaces. The desktop app shows their bindings and recent sessions, and starts a new session only in the agent and profile you choose. Pinning changes no binding or agent configuration.",
    )
    .flag(&["-j", "--json"], "json", "list the workspaces as JSON")
    .subcommands(
        "action",
        &[
            ("list", "list pinned workspaces (the default)"),
            ("add", "pin a folder, or change its display name"),
            ("remove", "unpin a folder without deleting it"),
        ],
    );
    let args = match parser.parse(argv) {
        Ok(args) => args,
        Err(stop) => return stop.exit_code(),
    };
    let action = args.value("action");
    let sub = |name: &str| format!("{prog} {name}");
    let add_parser = Parser::new(&sub("add"), "")
        .positional_as(
            "folder",
            "FOLDER",
            Count::Optional,
            "the project folder to pin (default: this folder)",
        )
        .option(&["--name"], "name", "NAME", "the workspace's display name");
    let remove_parser = Parser::new(&sub("remove"), "").positional_as(
        "folder",
        "FOLDER",
        Count::Optional,
        "the project folder to unpin (default: this folder)",
    );
    let sub_args = match action {
        Some("add") => add_parser.parse_known(&args.rest),
        Some("remove") => remove_parser.parse_known(&args.rest),
        _ => Ok((Default::default(), Vec::new())),
    };
    let sub_args = match sub_args {
        Ok((_, extras)) if !extras.is_empty() => {
            return parser
                .error(&format!("unrecognized arguments: {}", extras.join(" ")))
                .exit_code();
        }
        Ok((args, _)) => args,
        Err(stop) => return stop.exit_code(),
    };
    let result = match action {
        Some("add") => add(
            sub_args.value("folder").unwrap_or("."),
            sub_args.value("name"),
        )
        .map(|workspace| {
            if args.flag("json") {
                out::println(&py::dumps(&as_json(&workspace)));
            } else {
                out::success(&format!(
                    "Pinned workspace \"{}\" at {}.",
                    workspace.name,
                    paths::shell_path(&workspace.folder)
                ));
            }
        }),
        Some("remove") => remove(sub_args.value("folder").unwrap_or(".")).map(|workspace| {
            if args.flag("json") {
                out::println(&py::dumps(&as_json(&workspace)));
            } else {
                out::success(&format!(
                    "Unpinned workspace \"{}\". Its folder is still there.",
                    workspace.name
                ));
            }
        }),
        _ => list(args.flag("json")),
    };
    match result {
        Ok(()) => 0,
        Err(message) => out::fail(&message),
    }
}
