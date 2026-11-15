//! Opt-in deep doctor checks: bindings, session readability, desktop launchers and processes,
//! missing binaries and storage. Diagnosis is read-only except --repair stale-bindings, which
//! removes only Hatcheck-owned bindings whose folders are gone. No credential or transcript is changed.

use crate::agent::Agent;
use crate::facts;
use crate::ospath;
use crate::out;
use crate::paths;
use crate::py;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::io::BufRead;

pub struct HealthReport {
    pub issues: Vec<Value>,
    pub storage: Vec<Value>,
    pub repaired: Vec<String>,
    pub processes_checked: bool,
    pub processes_note: Option<String>,
}

fn issue(kind: &str, message: String, path: Option<&str>, repair: Option<String>) -> Value {
    json!({"kind": kind, "severity": "warning", "message": message,
        "path": path.map(ospath::abspath), "repair": repair})
}

fn missing_agents(missing: &[Agent]) -> Vec<Value> {
    let used: Vec<Agent> = paths::find_all()
        .into_iter()
        .map(|(agent, _)| agent)
        .collect();
    missing
        .iter()
        .map(|agent| {
            let mut found = issue(
                "missing-binary",
                format!("{} ({}) isn't on your PATH", agent.name(), agent.program()),
                None,
                Some(format!(
                    "Install {}; docs/agents.md has its installer",
                    agent.program()
                )),
            );
            if !used.contains(agent) {
                found["severity"] = json!("info");
            }
            found
        })
        .collect()
}

fn bindings(repair: bool) -> Result<(Vec<Value>, Vec<String>), String> {
    let bindings = paths::load_bindings();
    let mut issues = Vec::new();
    let mut repaired = Vec::new();
    let stale: Vec<String> = bindings
        .keys()
        .filter(|folder| !ospath::isdir(folder))
        .cloned()
        .collect();
    if repair && !stale.is_empty() {
        let mut kept = bindings.clone();
        kept.retain(|folder, _| ospath::isdir(folder));
        crate::bind::save_bindings(&kept)
            .map_err(|e| format!("can't repair bindings.json: {e}"))?;
        for folder in &stale {
            repaired.push(format!(
                "Removed the binding for missing folder {}",
                paths::shell_path(folder)
            ));
        }
        return Ok((issues, repaired));
    }
    for folder in stale {
        issues.push(issue(
            "stale-binding",
            format!(
                "Bound project folder {} isn't there",
                paths::shell_path(&folder)
            ),
            Some(&folder),
            Some(format!(
                "hatcheck unbind {}; or hatcheck relocate {} NEW",
                paths::sh_quote(&folder),
                paths::sh_quote(&folder)
            )),
        ));
    }
    Ok((issues, repaired))
}

fn session_error(agent: Agent, path: &str) -> Option<String> {
    if !matches!(
        agent,
        Agent::Claude | Agent::Codex | Agent::Qwen | Agent::Copilot
    ) {
        return None;
    }
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(e) => return Some(format!("can't read the session file: {e}")),
    };
    for (n, line) in std::io::BufReader::new(file).lines().enumerate() {
        let line = match line {
            Ok(line) => line,
            Err(e) => return Some(format!("can't read line {}: {e}", n + 1)),
        };
        if !line.trim().is_empty() && serde_json::from_str::<Value>(&line).is_err() {
            return Some(format!("line {} isn't valid JSON", n + 1));
        }
    }
    None
}

fn sessions() -> Vec<Value> {
    let mut issues = Vec::new();
    let mut seen = BTreeSet::new();
    for ((agent, id), places) in crate::find::session_copies("") {
        for (profile, path) in places {
            if !seen.insert(ospath::abspath(&path)) {
                continue;
            }
            if let Some(why) = session_error(agent, &path) {
                issues.push(issue(
                    "session-unreadable",
                    format!(
                        "{} session {} in {}: {why}",
                        agent.name(),
                        py::head(&id, 8),
                        paths::shell_path(&profile)
                    ),
                    Some(&path),
                    None,
                ));
            }
        }
    }
    facts::save_cache();
    issues
}

fn launchers() -> (Vec<Value>, bool, Option<String>) {
    let instances = crate::desktop::instances();
    let mut issues = Vec::new();
    let mains = crate::desktop::main_processes();
    for instance in &instances {
        let program = if ospath::WINDOWS {
            instance.app.clone()
        } else {
            ospath::join_all(&instance.app, &["Contents", "MacOS", "launch"])
        };
        if !ospath::exists(&program) {
            issues.push(issue(
                "broken-launcher",
                format!("Claude {}'s launcher isn't there", instance.display),
                Some(&program),
                Some(format!(
                    "hatcheck desktop new {} {} --replace",
                    paths::sh_quote(&instance.name),
                    paths::sh_quote(&instance.display)
                )),
            ));
        }
        if let Some(folder) = &instance.data_dir {
            if !ospath::isdir(folder) {
                let mut first = issue(
                    "missing-instance-data",
                    format!(
                        "Claude {} has no data folder yet; its first launch creates it",
                        instance.display
                    ),
                    Some(folder),
                    None,
                );
                first["severity"] = json!("info");
                issues.push(first);
            }
        }
        let Some(mains) = &mains else { continue };
        let running = match &instance.data_dir {
            Some(dir) => {
                let option = format!("--user-data-dir={dir}");
                mains
                    .iter()
                    .filter(|line| line.contains(&format!("{option} ")) || line.ends_with(&option))
                    .count()
            }
            None => mains
                .iter()
                .filter(|line| !line.contains("--user-data-dir="))
                .count(),
        };
        if running > 1 {
            issues.push(issue(
                "duplicate-instance",
                format!(
                    "Claude {} has {running} main processes on one data folder; quit all but one",
                    instance.display
                ),
                instance.data_dir.as_deref(),
                Some(
                    "Quit every duplicate Claude process before opening the instance again".into(),
                ),
            ));
        }
    }
    let checked = mains.is_some();
    let note = (!checked).then(|| {
        if ospath::WINDOWS {
            "Duplicate Claude Desktop process detection isn't available on Windows".to_string()
        } else {
            "The process list couldn't be read, so duplicate Claude Desktop processes are unknown"
                .to_string()
        }
    });
    (issues, checked, note)
}

fn folder_size(path: &str) -> (u64, u64, Option<String>) {
    fn walk(path: &std::path::Path, files: &mut u64, bytes: &mut u64) -> std::io::Result<()> {
        let metadata = std::fs::symlink_metadata(path)?;
        if metadata.file_type().is_symlink() {
            return Ok(());
        }
        if metadata.is_dir() {
            for entry in std::fs::read_dir(path)? {
                walk(&entry?.path(), files, bytes)?;
            }
        } else if metadata.is_file() {
            *files += 1;
            *bytes += metadata.len();
        }
        Ok(())
    }
    let (mut files, mut bytes) = (0, 0);
    match walk(std::path::Path::new(path), &mut files, &mut bytes) {
        Ok(()) => (files, bytes, None),
        Err(e) => (0, 0, Some(e.to_string())),
    }
}

fn storage() -> Vec<Value> {
    let mut seen = BTreeMap::new();
    for (agent, folder) in paths::find_all() {
        seen.entry(ospath::realpath(&folder))
            .or_insert((agent, folder));
    }
    seen.into_values()
        .map(|(agent, folder)| {
            let (files, bytes, error) = folder_size(&folder);
            let measured = error.is_none();
            json!({"agent": agent.key(), "profile": ospath::abspath(&folder),
                "files": measured.then_some(files), "bytes": measured.then_some(bytes), "error": error})
        })
        .collect()
}

pub fn run(missing: &[Agent], repair_stale_bindings: bool) -> Result<HealthReport, String> {
    let mut issues = missing_agents(missing);
    let (binding_issues, repaired) = bindings(repair_stale_bindings)?;
    issues.extend(binding_issues);
    issues.extend(sessions());
    let (launcher_issues, processes_checked, processes_note) = launchers();
    issues.extend(launcher_issues);
    Ok(HealthReport {
        issues,
        storage: storage(),
        repaired,
        processes_checked,
        processes_note,
    })
}

pub fn as_json(report: &HealthReport) -> Value {
    json!({"issues": report.issues, "storage": report.storage, "repaired": report.repaired,
        "processes_checked": report.processes_checked, "processes_note": report.processes_note})
}

pub fn describe(report: &HealthReport) {
    out::heading("Deep checks:");
    if report.issues.is_empty() {
        out::human("  No session, binding or launcher problems found.");
    }
    for issue in &report.issues {
        let message = format!("  {}", py::str_of(py::get(issue, "message")));
        if py::s(py::get(issue, "severity")) == Some("info") {
            out::println(&message);
        } else {
            out::warn(&message);
        }
        if let Some(repair) = py::s(py::get(issue, "repair")) {
            out::command(&format!("    {repair}"));
        }
    }
    for repaired in &report.repaired {
        out::success(repaired);
    }
    if let Some(note) = &report.processes_note {
        out::human(&format!("  {note}"));
    }
    if !report.storage.is_empty() {
        out::heading("Storage by profile or home:");
        for row in &report.storage {
            let profile = py::s(py::get(row, "profile")).unwrap_or("?");
            if let Some(error) = py::s(py::get(row, "error")) {
                out::human(&format!("  {profile}: unknown ({error})"));
            } else {
                out::human(&format!(
                    "  {profile}: {} bytes in {} files",
                    row["bytes"], row["files"]
                ));
            }
        }
    }
}
