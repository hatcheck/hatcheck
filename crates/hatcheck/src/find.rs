//! Finding sessions by ID or by folder, for hatcheck open, show and best: every copy of a
//! session across the profiles and homes, the newest one started from a folder, and whether
//! one is open, and the profile or home a PROFILE argument names.

use crate::agent::Agent;
use crate::facts::{self, Summary};
use crate::ospath::{self, Part};
use crate::paths;
use crate::py::{get, s};
use serde_json::Value;

/// (agent, folder) for a PROFILE argument: NAME for ~/.claude-NAME, default or claude for
/// ~/.claude, codex for ~/.codex, codex:NAME for ~/.codex-NAME (claude:NAME spells out a Claude
/// Code one), agy for ~/.gemini/antigravity-cli, kimi for ~/.kimi-code, kimi:NAME for
/// ~/.kimi-code-NAME, zcode for ~/.zcode, dsh for ~/.dsh, dsh:NAME for ~/.dsh-NAME, mcode for
/// ~/.minimax and mcode:NAME for ~/.minimax-NAME, qwen for ~/.qwen and qwen:NAME for
/// ~/.qwen-NAME, or a path.
pub fn place(spec: &str) -> (Agent, String) {
    if spec == "default" {
        return (Agent::Claude, paths::default_folder(Agent::Claude));
    }
    for agent in Agent::ALL {
        if spec == agent.key() || spec == format!("{}:default", agent.key()) {
            return (agent, paths::default_folder(agent));
        }
        if let Some(name) = spec
            .strip_prefix(agent.key())
            .and_then(|r| r.strip_prefix(':'))
            .filter(|n| !n.is_empty())
        {
            return (agent, paths::profile_folder(agent, name));
        }
    }
    let b = spec.as_bytes();
    let drive = b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':';
    if spec.contains('/') || spec.contains(ospath::SEP) || spec.starts_with('~') || drive {
        let path = ospath::abspath(&ospath::expanduser(spec));
        let agy = ospath::basename(&path) == "antigravity-cli" || crate::agy::is_root(&path);
        let zcode = ospath::basename(&path) == Agent::Zcode.base() || crate::zcode::is_root(&path);
        let kimi = crate::kimi::is_home(&path);
        let dsh = crate::dsh::is_home(&path);
        let mcode = crate::mcode::is_home(&path);
        let qwen = ospath::basename(&path).starts_with(".qwen") || crate::qwen::is_home(&path);
        let codex = ospath::basename(&path).starts_with(".codex")
            || (ospath::isdir(&ospath::join(&path, "sessions"))
                && !ospath::isdir(&ospath::join(&path, "projects")));
        let agent = if agy {
            Agent::Agy
        } else if zcode {
            Agent::Zcode
        } else if kimi {
            Agent::Kimi
        } else if dsh {
            Agent::Dsh
        } else if mcode {
            Agent::Mcode
        } else if qwen {
            Agent::Qwen
        } else if codex {
            Agent::Codex
        } else {
            Agent::Claude
        };
        return (agent, path);
    }
    (Agent::Claude, paths::profile_folder(Agent::Claude, spec))
}

/// A session's copies: its (agent, ID), and each (profile or home, file) that has one.
pub type Copies = ((Agent, String), Vec<(String, String)>);

/// Every copy of the sessions whose ID starts with prefix, in every Claude Code profile and
/// Codex home, archived Codex ones too, by ID.
pub fn session_copies(prefix: &str) -> Vec<Copies> {
    let mut found: Vec<Copies> = Vec::new();
    let mut add = |key: (Agent, String), place: (String, String)| match found
        .iter_mut()
        .find(|(k, _)| *k == key)
    {
        Some((_, places)) => places.push(place),
        None => found.push((key, vec![place])),
    };
    for profile in paths::find_profiles() {
        let mut files = ospath::glob(
            &profile,
            &[
                Part::Name("projects"),
                Part::Wild("", ""),
                Part::Wild(prefix, ".jsonl"),
            ],
        );
        files.sort();
        for path in files {
            let name = ospath::basename(&path);
            add(
                (
                    Agent::Claude,
                    name[..name.len() - ".jsonl".len()].to_string(),
                ),
                (profile.clone(), path),
            );
        }
    }
    let lower = prefix.to_lowercase();
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
            if let Some(id) = paths::codex_file_id(&path).filter(|id| id.starts_with(&lower)) {
                add((Agent::Codex, id.to_string()), (home.clone(), path.clone()));
            }
        }
    }
    for root in paths::find_agy_stores() {
        for row in crate::agy::rows_starting_with(&root, prefix) {
            let path = crate::agy::conversation_path(&root, &row.id);
            add((Agent::Agy, row.id), (root.clone(), path));
        }
    }
    for home in paths::find_kimi_homes() {
        for x in crate::kimi::sessions_starting_with(&home, prefix) {
            add((Agent::Kimi, x.id), (home.clone(), x.dir));
        }
    }
    for root in paths::find_zcode_roots() {
        for row in crate::zcode::rows_starting_with(&root, prefix) {
            let path = crate::zcode::artifacts_path(&root, &row.id);
            add((Agent::Zcode, row.id), (root.clone(), path));
        }
    }
    for home in paths::find_dsh_homes() {
        for x in crate::dsh::sessions_starting_with(&home, prefix) {
            add((Agent::Dsh, x.id), (home.clone(), x.dir));
        }
    }
    for home in paths::find_mcode_homes() {
        for x in crate::mcode::sessions_starting_with(&home, prefix) {
            add((Agent::Mcode, x.row.id), (home.clone(), x.dir));
        }
    }
    for home in paths::find_qwen_homes() {
        for x in crate::qwen::sessions_starting_with(&home, prefix) {
            add((Agent::Qwen, x.id), (home.clone(), x.file));
        }
    }
    found
}

/// The folder a Codex session started in, from its first record.
pub fn codex_cwd(path: &str) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    let first = bytes.split(|b| *b == b'\n').next()?;
    let record: Value = serde_json::from_str(&String::from_utf8_lossy(first)).ok()?;
    s(get(get(&record, "payload"), "cwd")).map(String::from)
}

/// The newest session started from folder, in any profile or home, or None. Only terminal
/// sessions, as the list shows by default, unless everything.
pub fn newest_here(folder: &str, everything: bool) -> Option<Summary> {
    let here = ospath::realpath(folder);
    let mut best: Option<Summary> = None;
    let better = |s: &Summary, best: &Option<Summary>| {
        s.has_messages
            && s.cwd
                .as_deref()
                .is_some_and(|c| !c.is_empty() && paths::same_folder(c, &here))
            && s.entrypoint_text() != "subagent"
            && (everything || s.entrypoint_text() == "cli")
            && best.as_ref().is_none_or(|b| s.when > b.when)
    };
    let mut names = vec![paths::encode_project(folder)];
    let real_name = paths::encode_project(&here);
    if !names.contains(&real_name) {
        names.push(real_name);
    }
    for profile in paths::find_profiles() {
        for name in &names {
            let long = name.chars().count() > 200;
            let short: String = name.chars().take(200).collect();
            let dir = if long {
                Part::Wild(&short, "")
            } else {
                Part::Name(name)
            };
            for path in ospath::glob(
                &profile,
                &[Part::Name("projects"), dir, Part::Wild("", ".jsonl")],
            ) {
                if let Some(s) = facts::summarize(Agent::Claude, &profile, &path) {
                    if better(&s, &best) {
                        best = Some(s);
                    }
                }
            }
        }
    }
    for home in paths::find_codex_homes() {
        let mut files: Vec<(f64, String)> = paths::codex_session_files(&home)
            .into_iter()
            .map(|p| (ospath::getmtime(&p).unwrap_or(0.0), p))
            .collect();
        files.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        for (mtime, path) in files {
            if best.as_ref().is_some_and(|b| mtime < b.when) {
                break; // a file's mtime is never older than its last message
            }
            if codex_cwd(&path).is_some_and(|c| paths::same_folder(&c, &here)) {
                if let Some(s) = facts::summarize(Agent::Codex, &home, &path) {
                    if better(&s, &best) {
                        best = Some(s);
                        break;
                    }
                }
            }
        }
    }
    for root in paths::find_agy_stores() {
        for row in crate::agy::rows(&root) {
            let there = row
                .workspaces
                .first()
                .is_some_and(|w| paths::same_folder(w, &here));
            if there {
                let s = crate::agy::summary(&root, &row);
                if better(&s, &best) {
                    best = Some(s);
                }
            }
        }
    }
    for home in paths::find_kimi_homes() {
        for x in crate::kimi::sessions(&home) {
            let there = s(get(&x.state, "cwd")).is_some_and(|c| paths::same_folder(c, &here));
            if there {
                let s = crate::kimi::summary(&home, &x);
                if better(&s, &best) {
                    best = Some(s);
                }
            }
        }
    }
    for root in paths::find_zcode_roots() {
        for row in crate::zcode::rows(&root) {
            if paths::same_folder(&row.directory, &here) {
                let s = crate::zcode::summary(&root, &row);
                if better(&s, &best) {
                    best = Some(s);
                }
            }
        }
    }
    for home in paths::find_dsh_homes() {
        for x in crate::dsh::sessions(&home) {
            if s(get(&x.header, "cwd")).is_some_and(|cwd| paths::same_folder(cwd, &here)) {
                let summary = crate::dsh::summary(&home, &x);
                if better(&summary, &best) {
                    best = Some(summary);
                }
            }
        }
    }
    for home in paths::find_mcode_homes() {
        for x in crate::mcode::sessions(&home) {
            if paths::same_folder(&x.row.directory, &here) {
                let summary = crate::mcode::summary(&home, &x);
                if better(&summary, &best) {
                    best = Some(summary);
                }
            }
        }
    }
    for home in paths::find_qwen_homes() {
        for x in crate::qwen::sessions(&home) {
            if x.cwd
                .as_deref()
                .is_some_and(|cwd| paths::same_folder(cwd, &here))
            {
                let summary = crate::qwen::summary(&home, &x);
                if better(&summary, &best) {
                    best = Some(summary);
                }
            }
        }
    }
    facts::save_cache();
    best
}

/// Whether Codex has a session open: it holds a lock on thread-writer-locks/<id>.lock while
/// it's writing the session. Not checked on Windows.
pub fn codex_session_open(home: &str, sid: &str) -> bool {
    lock_held(&ospath::join_all(
        home,
        &["thread-writer-locks", &format!("{sid}.lock")],
    ))
}

/// Whether another process holds the lock on a lock file, as Codex and agy hold one on a
/// session they have open. A file that isn't there, or that nobody holds, isn't held. Not
/// checked on Windows.
pub fn lock_held(path: &str) -> bool {
    if ospath::WINDOWS || !ospath::isfile(path) {
        return false;
    }
    let Ok(file) = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
    else {
        return false;
    };
    match file.try_lock() {
        Ok(()) => {
            let _ = file.unlock();
            false
        }
        Err(std::fs::TryLockError::WouldBlock) => true,
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(not(windows))]
    fn places() {
        let home = paths::home();
        assert_eq!(
            place("work"),
            (Agent::Claude, format!("{home}/.claude-work"))
        );
        assert_eq!(place("default"), (Agent::Claude, format!("{home}/.claude")));
        assert_eq!(place("codex"), (Agent::Codex, format!("{home}/.codex")));
        assert_eq!(
            place("codex:work"),
            (Agent::Codex, format!("{home}/.codex-work"))
        );
        assert_eq!(
            place("claude:x"),
            (Agent::Claude, format!("{home}/.claude-x"))
        );
        assert_eq!(place("/srv/.codex-a").0, Agent::Codex);
        assert_eq!(
            place("agy"),
            (Agent::Agy, format!("{home}/.gemini/antigravity-cli"))
        );
        assert_eq!(place("agy:default"), place("agy"));
        assert_eq!(place("/x/.gemini/antigravity-cli").0, Agent::Agy);
        assert_eq!(place("claude"), place("default")); // as codex and agy name theirs
        assert_eq!(place("kimi"), (Agent::Kimi, format!("{home}/.kimi-code")));
        assert_eq!(
            place("kimi:work"),
            (Agent::Kimi, format!("{home}/.kimi-code-work"))
        );
        assert_eq!(place("/srv/.kimi-code-a").0, Agent::Kimi);
        assert_eq!(place("zcode"), (Agent::Zcode, format!("{home}/.zcode")));
        assert_eq!(place("/x/.zcode").0, Agent::Zcode);
        assert_eq!(place("dsh"), (Agent::Dsh, format!("{home}/.dsh")));
        assert_eq!(place("dsh:work"), (Agent::Dsh, format!("{home}/.dsh-work")));
        assert_eq!(place("/srv/.dsh-a").0, Agent::Dsh);
        assert_eq!(place("mcode"), (Agent::Mcode, format!("{home}/.minimax")));
        assert_eq!(
            place("mcode:work"),
            (Agent::Mcode, format!("{home}/.minimax-work"))
        );
        assert_eq!(place("qwen"), (Agent::Qwen, format!("{home}/.qwen")));
        assert_eq!(
            place("qwen:work"),
            (Agent::Qwen, format!("{home}/.qwen-work"))
        );
        assert_eq!(place("/srv/.qwen-a").0, Agent::Qwen);
    }
}
