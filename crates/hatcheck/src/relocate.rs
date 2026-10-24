//! hatcheck relocate OLD NEW [--dry-run] [--json]: when a project folder has been moved or
//! renamed, bring its sessions along. Claude Code files a session under projects/<the folder's
//! name> and records the folder in each line as cwd, so after a move `claude --resume` from
//! the new folder doesn't list them, and the resume commands hatcheck prints cd to a folder
//! that's gone. This moves each profile's project folder to its new name, rewrites the cwd
//! the lines record, and brings Claude Code's sessions-index.json along; a Codex session,
//! filed by date, has the cwd in its session_meta and turn_context lines rewritten. Folders
//! under OLD come along too. Running it the other way round undoes it. docs/commands.md's
//! `hatcheck relocate` section says what it refuses.

use crate::agent::Agent;
use crate::args::{Count, Parser};
use crate::facts;
use crate::find;
use crate::ospath::{self, Part, SEP};
use crate::out;
use crate::paths;
use crate::py::{self, get, s};
use crate::sessions;
use crate::times;
use serde_json::{json, Map, Value};
use std::fs;

/// A file written in the last minute may still be being written: an open session, or one
/// closed a moment ago, so nothing is touched until a minute has passed.
const SETTLE: f64 = 60.0;
/// Claude Code keeps a project folder's name to this many characters.
const NAME_MOST: usize = 200;
const TMP: &str = ".hatcheck-relocate-tmp";

/// A stop with a message: exit code 1.
type Stop = String;

/// One session to bring along.
#[derive(Clone, Debug)]
struct Move {
    agent: Agent,
    profile: String,
    id: String,
    cwd_from: String,
    cwd_to: String,
    /// The transcript or session file, and where it goes (the same place for Codex).
    from: String,
    to: String,
    /// Claude Code's data folder beside the transcript (subagents, tool output), if any.
    data: Option<(String, String)>,
}

/// A session in a project folder named like OLD's that's left alone, and why.
#[derive(Clone, Debug)]
struct Skip {
    agent: Agent,
    profile: String,
    id: String,
    why: String,
}

/// What becomes of one of Claude Code's project folders.
#[derive(Clone, Debug)]
struct Project {
    profile: String,
    from: String,
    to: String,
    /// Indexes into the plan's moves.
    moves: Vec<usize>,
    /// Every session in it moves, to one folder that doesn't exist yet: the folder is renamed
    /// whole, so its memory and index come along.
    whole: bool,
    /// Entries that aren't sessions: memory/, sessions-index.json, and so on.
    others: Vec<String>,
}

#[derive(Debug)]
struct Plan {
    old: String,
    new: String,
    moves: Vec<Move>,
    skips: Vec<Skip>,
    projects: Vec<Project>,
    /// A folder binding (hatcheck bind) of OLD, to move to NEW: its key as bound.
    binding: Option<String>,
}

/// What of path lies under folder: "" when it's the folder, "/sub" when it's inside it, None
/// otherwise. Compared as the system compares names (without case on Windows).
fn rest_under(path: &str, folder: &str) -> Option<String> {
    let folder = folder.trim_end_matches(ospath::is_sep); // C:/old/ on Windows too
    let (p, f) = (ospath::normcase(path), ospath::normcase(folder));
    if p == f {
        return Some(String::new());
    }
    if p.len() > f.len() && p.starts_with(&f) && p[f.len()..].starts_with(SEP) {
        return Some(path[folder.len()..].to_string());
    }
    None
}

/// Where a folder that was under old is under new.
fn mapped(cwd: &str, old: &str, new: &str) -> Option<String> {
    rest_under(cwd, old).map(|rest| format!("{}{rest}", new.trim_end_matches(ospath::is_sep)))
}

/// The folder a Claude Code transcript ran in: as the session list works it out, or, for a
/// transcript with no conversation yet, what its first line records.
fn claude_cwd(path: &str) -> Option<String> {
    if let Ok(Some(summary)) = facts::read_session(path) {
        if let Some(cwd) = summary.cwd.filter(|c| !c.is_empty()) {
            return Some(cwd);
        }
    }
    let data = fs::read(path).ok()?;
    let first = data.split(|b| *b == b'\n').next()?;
    let record: Value = serde_json::from_slice(first).ok()?;
    s(get(&record, "cwd")).map(String::from)
}

/// The entries of one of Claude Code's project folders, sorted: (name, path).
fn entries_of(dir: &str) -> Vec<(String, String)> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
        .into_iter()
        .map(|n| {
            let p = ospath::join(dir, &n);
            (n, p)
        })
        .collect()
}

/// The sessions that ran in old, in every profile and home, and what to do with them.
fn plan(old: &str, new: &str) -> Plan {
    let mut spellings = vec![old.to_string()];
    if ospath::isdir(old) {
        let real = ospath::realpath(old);
        if real != old {
            spellings.push(real);
        }
    }
    let encoded: Vec<String> = spellings.iter().map(|p| paths::encode_project(p)).collect();
    let mut plan = Plan {
        old: old.to_string(),
        new: new.to_string(),
        moves: Vec::new(),
        skips: Vec::new(),
        projects: Vec::new(),
        binding: None,
    };
    for profile in paths::find_profiles() {
        let projects_dir = ospath::join(&profile, "projects");
        let mut dirs = ospath::glob(&projects_dir, &[Part::Wild("", "")]);
        dirs.sort();
        for dir in dirs.into_iter().filter(|d| ospath::isdir(d)) {
            let name = ospath::basename(&dir);
            let alike = encoded
                .iter()
                .any(|e| name == *e || name.starts_with(&format!("{e}-")));
            if !alike {
                continue;
            }
            let mut project = Project {
                profile: profile.clone(),
                from: dir.clone(),
                to: String::new(),
                moves: Vec::new(),
                whole: false,
                others: Vec::new(),
            };
            let entries = entries_of(&dir);
            let mut data_dirs: Vec<String> = Vec::new();
            for (entry, path) in &entries {
                let Some(id) = entry
                    .strip_suffix(".jsonl")
                    .filter(|_| ospath::isfile(path))
                else {
                    continue;
                };
                let cwd = claude_cwd(path);
                let Some(cwd_to) = cwd.as_deref().and_then(|c| mapped(c, old, new)) else {
                    plan.skips.push(Skip {
                        agent: Agent::Claude,
                        profile: profile.clone(),
                        id: id.to_string(),
                        why: match &cwd {
                            Some(c) => {
                                format!("ran in {}, a folder named alike", paths::shell_path(c))
                            }
                            None => "records no folder".into(),
                        },
                    });
                    continue;
                };
                let to_dir = ospath::join(&projects_dir, &paths::encode_project(&cwd_to));
                let data_from = ospath::join(&dir, id);
                let data = ospath::isdir(&data_from).then(|| {
                    data_dirs.push(id.to_string());
                    (data_from, ospath::join(&to_dir, id))
                });
                plan.moves.push(Move {
                    agent: Agent::Claude,
                    profile: profile.clone(),
                    id: id.to_string(),
                    cwd_from: cwd.unwrap_or_default(),
                    cwd_to,
                    from: path.clone(),
                    to: ospath::join(&to_dir, entry),
                    data,
                });
                project.moves.push(plan.moves.len() - 1);
            }
            if project.moves.is_empty() {
                continue;
            }
            let moved_ids: Vec<&str> = project
                .moves
                .iter()
                .map(|&k| plan.moves[k].id.as_str())
                .collect();
            project.others = entries
                .iter()
                .filter(|(n, _)| {
                    !n.strip_suffix(".jsonl").is_some_and(|id| {
                        moved_ids.contains(&id)
                            || plan
                                .skips
                                .iter()
                                .any(|s| s.id == id && s.profile == profile)
                    }) && !moved_ids.contains(&n.as_str())
                        && !data_dirs.iter().any(|d| d == n)
                })
                .map(|(n, _)| n.clone())
                .collect();
            let targets: Vec<String> = project
                .moves
                .iter()
                .map(|&k| ospath::dirname(&plan.moves[k].to))
                .collect();
            let one_target = targets.windows(2).all(|w| w[0] == w[1]);
            let all_moved = entries
                .iter()
                .filter(|(n, p)| n.ends_with(".jsonl") && ospath::isfile(p))
                .count()
                == project.moves.len();
            project.to = targets[0].clone();
            project.whole = one_target && all_moved && !ospath::exists(&project.to);
            plan.projects.push(project);
        }
    }
    for home in paths::find_codex_homes() {
        let mut files = paths::codex_session_files(&home);
        files.extend(ospath::glob(
            &home,
            &[
                Part::Name("archived_sessions"),
                Part::Wild("rollout-", ".jsonl"),
            ],
        ));
        files.sort();
        for path in files {
            let Some(id) = paths::codex_file_id(&path) else {
                continue;
            };
            let Some(cwd) = find::codex_cwd(&path) else {
                continue;
            };
            let Some(cwd_to) = mapped(&cwd, old, new) else {
                continue;
            };
            plan.moves.push(Move {
                agent: Agent::Codex,
                profile: home.clone(),
                id: id.to_string(),
                cwd_from: cwd,
                cwd_to,
                from: path.clone(),
                to: path.clone(),
                data: None,
            });
        }
    }
    // Antigravity CLI keeps the folder inside its database, so its sessions stay as they are.
    for root in paths::find_agy_stores() {
        for row in crate::agy::rows(&root) {
            let ran_there = row
                .workspaces
                .first()
                .is_some_and(|w| mapped(w, old, new).is_some());
            if ran_there {
                plan.skips.push(Skip {
                    agent: Agent::Agy,
                    profile: root.clone(),
                    id: row.id.clone(),
                    why: "keeps the folder in agy's database, which stays as it was".into(),
                });
            }
        }
    }
    // Kimi files a session under a hash of the folder and keeps the folder in its state and
    // index; ZCode keeps it in its database. Both stay as they are.
    for home in paths::find_kimi_homes() {
        for x in crate::kimi::sessions(&home) {
            let ran_there = s(get(&x.state, "cwd")).is_some_and(|c| mapped(c, old, new).is_some());
            if ran_there {
                plan.skips.push(Skip {
                    agent: Agent::Kimi,
                    profile: home.clone(),
                    id: x.id.clone(),
                    why: "is filed under a hash of the folder, which stays as it was".into(),
                });
            }
        }
    }
    for root in paths::find_zcode_roots() {
        for row in crate::zcode::rows(&root) {
            if mapped(&row.directory, old, new).is_some() {
                plan.skips.push(Skip {
                    agent: Agent::Zcode,
                    profile: root.clone(),
                    id: row.id.clone(),
                    why: "keeps the folder in zcode's database, which stays as it was".into(),
                });
            }
        }
    }
    let bindings = paths::load_bindings();
    plan.binding = bindings
        .keys()
        .find(|k| rest_under(k, old).is_some_and(|r| r.is_empty()))
        .cloned();
    plan
}

/// Why the plan can't be carried out as it is, or nothing: an open or just-written session, a
/// link in place of a file, a session already filed under NEW, a folder name too long for
/// Claude Code, or a line that names the folder but isn't JSON.
fn refusals(plan: &Plan, now: f64) -> Vec<String> {
    let mut why = Vec::new();
    let profiles: Vec<String> = plan
        .moves
        .iter()
        .filter(|m| m.agent == Agent::Claude)
        .map(|m| m.profile.clone())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    let live = sessions::open_sessions(&profiles);
    let is_link = |p: &str| fs::symlink_metadata(p).is_ok_and(|m| m.file_type().is_symlink());
    for m in &plan.moves {
        let name = format!("{} session {}", m.agent.name(), py::head(&m.id, 8));
        let open = match m.agent {
            Agent::Claude => live
                .get(&(ospath::realpath(&m.profile), m.id.clone()))
                .map(|entry| format!("pid {}", py::str_of(get(entry, "pid")))),
            Agent::Codex => {
                find::codex_session_open(&m.profile, &m.id).then(|| "codex".to_string())
            }
            Agent::Agy => crate::agy::session_open(&m.profile, &m.id)
                .then(|| crate::agy::holder(&m.profile).to_string()),
            Agent::Kimi | Agent::Zcode => None, // never moved: skipped in plan()
        };
        if let Some(whom) = open {
            why.push(format!(
                "{name} is open in {whom}; quit it first, so it stops changing"
            ));
            continue;
        }
        let written = ospath::getmtime(&m.from).unwrap_or(0.0);
        if written > now - SETTLE {
            why.push(format!(
                "{name} was written {} ago; wait a minute, in case it's still being written",
                times::duration(now - written)
            ));
            continue;
        }
        if is_link(&m.from) || m.data.as_ref().is_some_and(|(d, _)| is_link(d)) {
            why.push(format!("{name} is a link, not a file; move the real one"));
            continue;
        }
        if m.agent == Agent::Claude {
            if ospath::exists(&m.to) {
                why.push(format!(
                    "{name} is already filed under {} too; remove one copy first",
                    paths::shell_path(&m.cwd_to)
                ));
                continue;
            }
            if ospath::basename(&ospath::dirname(&m.to)).chars().count() > NAME_MOST {
                why.push(format!(
                    "{}'s name is too long for Claude Code's project folders",
                    paths::shell_path(&m.cwd_to)
                ));
                continue;
            }
        }
        if let Err(e) = rewrite(m, true) {
            why.push(e);
        }
    }
    why
}

/// Rewrite the folder a session's lines record, in place, keeping the file's timestamps: the
/// top-level cwd of a Claude Code transcript's lines, and the payload's cwd in a Codex
/// session's session_meta and turn_context lines, where it's OLD or under it. Lines that don't
/// name a folder are left as they are, byte for byte. With check, nothing is written: it only
/// says whether every line that names the folder reads as JSON. How many lines changed.
fn rewrite(m: &Move, check: bool) -> Result<usize, Stop> {
    let path = &m.to;
    let path = if check { &m.from } else { path };
    let data = fs::read(path).map_err(|e| format!("{}: {e}", paths::shell_path(path)))?;
    let key = b"\"cwd\":\"";
    if memchr::memmem::find(&data, key).is_none() {
        return Ok(0);
    }
    let (old, new) = (m.cwd_from.as_str(), m.cwd_to.as_str());
    let mut lines: Vec<Vec<u8>> = data.split(|b| *b == b'\n').map(<[u8]>::to_vec).collect();
    let mut changed = 0;
    for line in lines.iter_mut() {
        if line.is_empty() || memchr::memmem::find(line, key).is_none() {
            continue;
        }
        let mut record: Value = serde_json::from_slice(line).map_err(|_| {
            format!(
                "a line of {} that names the folder isn't JSON; is it being written?",
                paths::shell_path(path)
            )
        })?;
        let holder = match m.agent {
            Agent::Claude => Some(&mut record),
            Agent::Codex
                if matches!(
                    s(get(&record, "type")),
                    Some("session_meta" | "turn_context")
                ) =>
            {
                record.get_mut("payload")
            }
            Agent::Codex | Agent::Agy | Agent::Kimi | Agent::Zcode => None,
        };
        let Some(Value::Object(fields)) = holder else {
            continue;
        };
        let Some(cwd) = fields.get("cwd").and_then(Value::as_str) else {
            continue;
        };
        let Some(to) = mapped(cwd, old, new) else {
            continue;
        };
        fields.insert("cwd".into(), Value::String(to));
        *line = py::dumps_tight(&record).into_bytes();
        changed += 1;
    }
    if check || changed == 0 {
        return Ok(changed);
    }
    let tmp = format!("{path}{TMP}");
    fs::write(&tmp, lines.join(&b'\n'))
        .and_then(|()| crate::move_cmd::copy_stat(path, &tmp))
        .and_then(|()| fs::rename(&tmp, path))
        .map_err(|e| format!("couldn't rewrite {}: {e}", paths::shell_path(path)))?;
    Ok(changed)
}

/// Claude Code's sessions-index.json in a project folder: its entries, with the index's other
/// fields, or None when there's none (or it doesn't read).
fn read_index(dir: &str) -> Option<(Map<String, Value>, Vec<Value>)> {
    let text = fs::read_to_string(ospath::join(dir, "sessions-index.json")).ok()?;
    let Value::Object(mut root) = serde_json::from_str::<Value>(&text).ok()? else {
        return None;
    };
    let entries = match root.remove("entries") {
        Some(Value::Array(e)) => e,
        _ => Vec::new(),
    };
    Some((root, entries))
}

fn write_index(dir: &str, root: Map<String, Value>, entries: Vec<Value>) -> Result<(), Stop> {
    let mut root = root;
    root.insert("entries".into(), Value::Array(entries));
    let path = ospath::join(dir, "sessions-index.json");
    let tmp = format!("{path}{TMP}");
    fs::write(&tmp, py::dumps(&Value::Object(root)) + "\n")
        .and_then(|()| fs::rename(&tmp, &path))
        .map_err(|e| format!("couldn't rewrite {}: {e}", paths::shell_path(&path)))
}

/// An index entry with its paths as they are after the move.
fn moved_entry(entry: &Value, m: &Move) -> Value {
    let mut e = entry.clone();
    if let Value::Object(fields) = &mut e {
        if fields.get("fullPath").and_then(Value::as_str).is_some() {
            fields.insert("fullPath".into(), Value::String(m.to.clone()));
        }
        if let Some(p) = fields.get("projectPath").and_then(Value::as_str) {
            if let Some(to) = mapped(p, &m.cwd_from, &m.cwd_to) {
                fields.insert("projectPath".into(), Value::String(to));
            }
        }
    }
    e
}

/// Bring the sessions along: the project folders renamed or their sessions moved, the lines
/// rewritten, the indexes updated, the binding moved. Stops at the first error, leaving what
/// was done; running it again picks up the rest.
fn carry_out(plan: &Plan) -> Result<(), Stop> {
    let moved = |e: std::io::Error, what: &str| format!("couldn't move {what}: {e}");
    for project in &plan.projects {
        let moves: Vec<&Move> = project.moves.iter().map(|&k| &plan.moves[k]).collect();
        if project.whole {
            fs::rename(&project.from, &project.to)
                .map_err(|e| moved(e, &paths::shell_path(&project.from)))?;
            if let Some((root, entries)) = read_index(&project.to) {
                let entries = entries
                    .iter()
                    .map(|entry| {
                        let id = s(get(entry, "sessionId")).unwrap_or("");
                        moves
                            .iter()
                            .find(|m| m.id == id)
                            .map_or_else(|| entry.clone(), |m| moved_entry(entry, m))
                    })
                    .collect();
                write_index(&project.to, root, entries)?;
            }
        } else {
            fs::create_dir_all(&project.to)
                .map_err(|e| moved(e, &paths::shell_path(&project.to)))?;
            for m in &moves {
                fs::rename(&m.from, &m.to).map_err(|e| moved(e, &paths::shell_path(&m.from)))?;
                if let Some((from, to)) = &m.data {
                    fs::rename(from, to).map_err(|e| moved(e, &paths::shell_path(from)))?;
                }
            }
            if let Some((root, entries)) = read_index(&project.from) {
                let (gone, kept): (Vec<Value>, Vec<Value>) =
                    entries.into_iter().partition(|entry| {
                        let id = s(get(entry, "sessionId")).unwrap_or("");
                        moves.iter().any(|m| m.id == id)
                    });
                write_index(&project.from, root, kept)?;
                if let Some((root, mut there)) = read_index(&project.to) {
                    for entry in &gone {
                        let id = s(get(entry, "sessionId")).unwrap_or("");
                        if let Some(m) = moves.iter().find(|m| m.id == id) {
                            there.push(moved_entry(entry, m));
                        }
                    }
                    write_index(&project.to, root, there)?;
                }
            }
        }
        for m in moves {
            rewrite(m, false)?;
        }
    }
    for m in plan.moves.iter().filter(|m| m.agent == Agent::Codex) {
        rewrite(m, false)?;
    }
    if let Some(key) = &plan.binding {
        let mut bindings = paths::load_bindings();
        if let Some(value) = bindings.remove(key) {
            bindings.insert(plan.new.clone(), value);
            crate::bind::save_bindings(&bindings)
                .map_err(|e| format!("couldn't move the folder binding: {e}"))?;
        }
    }
    Ok(())
}

/// The report for --json.
fn report(plan: &Plan, dry_run: bool, undo: &str) -> Value {
    json!({
        "version": 1, "dry_run": dry_run, "from": plan.old, "to": plan.new,
        "sessions": plan.moves.iter().map(|m| json!({
            "agent": m.agent.key(), "id": m.id, "profile": ospath::abspath(&m.profile),
            "cwd_from": m.cwd_from, "cwd_to": m.cwd_to, "from": m.from, "to": m.to})).collect::<Vec<_>>(),
        "skipped": plan.skips.iter().map(|k| json!({
            "agent": k.agent.key(), "id": k.id, "profile": ospath::abspath(&k.profile), "why": k.why})).collect::<Vec<_>>(),
        "binding": plan.binding.as_ref().map_or(Value::Null, |b| json!({"from": b, "to": plan.new})),
        "undo": undo,
    })
}

/// What was, or would be, done, as text.
fn describe(plan: &Plan, dry_run: bool, undo: &str) {
    let show = paths::shell_path;
    let n = plan.moves.len();
    let text = format!(
        "{} {n} session{} from {} to {}:",
        if dry_run {
            "Would relocate"
        } else {
            "Relocated"
        },
        if n == 1 { "" } else { "s" },
        show(&plan.old),
        show(&plan.new)
    );
    if dry_run {
        out::preview(&text);
    } else {
        out::success(&text);
    }
    for project in &plan.projects {
        let count = project.moves.len();
        let what = if project.whole {
            let extras = if project.others.is_empty() {
                String::new()
            } else {
                ", with its ".to_string()
                    + &project
                        .others
                        .iter()
                        .map(|o| {
                            if o == "memory" {
                                "memory".to_string()
                            } else if o == "sessions-index.json" {
                                "index".to_string()
                            } else {
                                o.clone()
                            }
                        })
                        .collect::<Vec<_>>()
                        .join(" and ")
            };
            format!(
                "projects/{} renamed projects/{}{extras}",
                ospath::basename(&project.from),
                ospath::basename(&project.to)
            )
        } else {
            format!(
                "moved into projects/{}; what else is in projects/{} stays",
                ospath::basename(&project.to),
                ospath::basename(&project.from)
            )
        };
        out::println(&format!(
            "  {}  {count} session{}: {what}",
            show(&project.profile),
            if count == 1 { "" } else { "s" }
        ));
    }
    let mut by_home: Vec<(String, usize)> = Vec::new();
    for m in plan.moves.iter().filter(|m| m.agent == Agent::Codex) {
        match by_home.iter_mut().find(|(h, _)| *h == m.profile) {
            Some(entry) => entry.1 += 1,
            None => by_home.push((m.profile.clone(), 1)),
        }
    }
    for (home, count) in by_home {
        out::println(&format!(
            "  {}  {count} session{}: the folder in the file rewritten",
            show(&home),
            if count == 1 { "" } else { "s" }
        ));
    }
    for skip in &plan.skips {
        out::warn(&format!(
            "Left alone: {} session {} in {} {}.",
            skip.agent.name(),
            py::head(&skip.id, 8),
            show(&skip.profile),
            skip.why
        ));
    }
    if let Some(key) = &plan.binding {
        out::println(&format!(
            "The folder binding of {} {} to {}.",
            show(key),
            if dry_run { "would move" } else { "moved" },
            show(&plan.new)
        ));
    }
    if dry_run {
        out::println("Nothing was changed; without --dry-run it does this.");
    } else {
        out::println(&format!(
            "Resume them from {}. To undo: {}",
            show(&plan.new),
            out::Paint::new().command(undo)
        ));
    }
}

pub fn main(argv: &[String], prog: &str) -> i32 {
    let parser = Parser::new(prog, "Bring the sessions of a project folder that was moved or renamed along to its new place, in every Claude Code profile and Codex home: OLD is where they ran, NEW where the folder is now.")
        .positional("old", Count::One, "the folder the sessions ran in, as it was")
        .positional("new", Count::One, "the folder as it is now; it has to exist")
        .flag(&["--dry-run"], "dry_run", "say what would move and change nothing")
        .flag(&["--json"], "json", "say what was or would be done as JSON");
    let args = match parser.parse(argv) {
        Ok(a) => a,
        Err(stop) => return stop.exit_code(),
    };
    let (dry_run, json_out) = (args.flag("dry_run"), args.flag("json"));
    let old = ospath::abspath(&ospath::expanduser(args.value("old").unwrap_or("")));
    let new = ospath::abspath(&ospath::expanduser(args.value("new").unwrap_or("")));
    let old = old.trim_end_matches(ospath::is_sep).to_string();
    let new = new.trim_end_matches(ospath::is_sep).to_string();
    let fail = |why: &str| -> i32 { out::fail(why) };
    if !ospath::isdir(&new) {
        return fail(&format!(
            "{} isn't a folder; move or rename the project there first",
            paths::shell_path(&new)
        ));
    }
    if ospath::normcase(&old) == ospath::normcase(&new)
        || (ospath::isdir(&old) && paths::same_folder(&old, &new))
    {
        return fail("OLD and NEW are the same folder");
    }
    if rest_under(&new, &old).is_some() {
        return fail(&format!(
            "{} is inside {}; a folder can't move into itself",
            paths::shell_path(&new),
            paths::shell_path(&old)
        ));
    }
    let now = times::now();
    let plan = plan(&old, &new);
    facts::save_cache();
    // The command that undoes it: with ~, as every command hatcheck prints, and with the paths
    // in full in --json, whose paths are absolute.
    let undo = format!(
        "{prog} {} {}",
        paths::shell_path(&new),
        paths::shell_path(&old)
    );
    let undo_json = format!("{prog} {} {}", paths::quote(&new), paths::quote(&old));
    if plan.moves.is_empty() {
        if !json_out {
            for skip in &plan.skips {
                out::warn(&format!(
                    "Left alone: {} session {} in {} {}.",
                    skip.agent.name(),
                    py::head(&skip.id, 8),
                    paths::shell_path(&skip.profile),
                    skip.why
                ));
            }
        }
        // Claude Code's and Codex's skips are sessions in folders named alike; the other
        // agents' are sessions that stay where they are.
        let alike = plan
            .skips
            .iter()
            .any(|k| matches!(k.agent, Agent::Claude | Agent::Codex));
        let mut stayers: Vec<&str> = Vec::new();
        for k in &plan.skips {
            if !matches!(k.agent, Agent::Claude | Agent::Codex)
                && !stayers.contains(&k.agent.name())
            {
                stayers.push(k.agent.name());
            }
        }
        let staying = match stayers.len() {
            0 => String::new(),
            1 => format!("{}'s", stayers[0]),
            n => format!("{} and {}'s", stayers[..n - 1].join("'s, "), stayers[n - 1]),
        };
        return fail(&format!(
            "No sessions {}ran in {}{}",
            if plan.skips.is_empty() {
                ""
            } else {
                "to move "
            },
            paths::shell_path(&old),
            if alike {
                " (only in folders named alike)".to_string()
            } else if !plan.skips.is_empty() {
                format!(" (only {staying}, which stay as they are)")
            } else {
                String::new()
            }
        ));
    }
    let why = refusals(&plan, now);
    if !why.is_empty() {
        return fail(&format!("Nothing was moved:\n  {}", why.join("\n  ")));
    }
    if !dry_run {
        if let Err(stop) = carry_out(&plan) {
            return fail(&format!(
                "{stop}\nSome sessions may have moved already; run the same command again to go on, or {undo} to go back."
            ));
        }
    }
    if json_out {
        out::println(&py::dumps(&report(&plan, dry_run, &undo_json)));
    } else {
        describe(&plan, dry_run, &undo);
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folders_under_a_folder() {
        assert_eq!(rest_under("/a/b", "/a/b"), Some("".into()));
        assert_eq!(rest_under("/a/b", "/a/b/"), Some("".into()));
        assert_eq!(rest_under("/a/b/c", "/a/b"), Some("/c".into()));
        assert_eq!(rest_under("/a/bc", "/a/b"), None);
        assert_eq!(rest_under("/a", "/a/b"), None);
        assert_eq!(mapped("/a/b/c", "/a/b", "/x/y"), Some("/x/y/c".into()));
        assert_eq!(mapped("/a/b", "/a/b", "/x/y/"), Some("/x/y".into()));
        assert_eq!(mapped("/a/bc", "/a/b", "/x"), None);
    }

    #[test]
    fn lines_naming_the_folder_are_rewritten_and_nothing_else() {
        let dir = std::env::temp_dir().join(format!("hatcheck-relocate-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("s.jsonl").to_string_lossy().into_owned();
        let lines = [
            r#"{"type":"user","cwd":"/a/b","message":{"content":"in /a/b"},"n":1.0}"#,
            r#"{"type":"assistant","cwd":"/a/b/sub","message":{"cwd":"/a/b"}}"#,
            "{\"type\":\"ai-title\",\"aiTitle\":\"x\"}",
            r#"{"type":"user","cwd":"/other"}"#,
        ];
        fs::write(&path, lines.join("\n") + "\n").unwrap();
        let m = Move {
            agent: Agent::Claude,
            profile: String::new(),
            id: "s".into(),
            cwd_from: "/a/b".into(),
            cwd_to: "/x/y".into(),
            from: path.clone(),
            to: path.clone(),
            data: None,
        };
        assert_eq!(rewrite(&m, true), Ok(2)); // the check writes nothing
        assert_eq!(fs::read_to_string(&path).unwrap(), lines.join("\n") + "\n");
        assert_eq!(rewrite(&m, false), Ok(2));
        let out = fs::read_to_string(&path).unwrap();
        let got: Vec<&str> = out.lines().collect();
        assert_eq!(
            got[0],
            r#"{"type":"user","cwd":"/x/y","message":{"content":"in /a/b"},"n":1.0}"#
        );
        assert_eq!(
            got[1],
            r#"{"type":"assistant","cwd":"/x/y/sub","message":{"cwd":"/a/b"}}"#
        ); // only the top-level cwd
        assert_eq!(got[2], lines[2]);
        assert_eq!(got[3], lines[3]);
        assert!(out.ends_with('\n'));
        // A Codex session: the cwd in session_meta and turn_context payloads, nothing else.
        let codex = [
            r#"{"type":"session_meta","payload":{"id":"i","cwd":"/a/b"}}"#,
            r#"{"type":"turn_context","payload":{"cwd":"/a/b","model":"m"}}"#,
            r#"{"type":"response_item","payload":{"cwd":"/a/b"}}"#,
        ];
        fs::write(&path, codex.join("\n") + "\n").unwrap();
        let m = Move {
            agent: Agent::Codex,
            ..m
        };
        assert_eq!(rewrite(&m, false), Ok(2));
        let out = fs::read_to_string(&path).unwrap();
        assert!(out.contains(r#"{"type":"session_meta","payload":{"id":"i","cwd":"/x/y"}}"#));
        assert!(out.contains(codex[2]));
        // A line that names the folder but isn't JSON stops it, before anything is written.
        fs::write(&path, "{\"cwd\":\"/a/b\"\n").unwrap();
        assert!(rewrite(&m, true).unwrap_err().contains("isn't JSON"));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn index_entries_follow_the_session() {
        let m = Move {
            agent: Agent::Claude,
            profile: "/h/.claude".into(),
            id: "s".into(),
            cwd_from: "/a/b".into(),
            cwd_to: "/x/y".into(),
            from: "/h/.claude/projects/-a-b/s.jsonl".into(),
            to: "/h/.claude/projects/-x-y/s.jsonl".into(),
            data: None,
        };
        let entry = json!({"sessionId": "s", "fullPath": "/h/.claude/projects/-a-b/s.jsonl", "projectPath": "/a/b", "messageCount": 3});
        assert_eq!(
            moved_entry(&entry, &m),
            json!({"sessionId": "s", "fullPath": "/h/.claude/projects/-x-y/s.jsonl", "projectPath": "/x/y", "messageCount": 3})
        );
    }
}
