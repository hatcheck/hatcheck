//! Local, session-owned backups for same-agent moves. Never a whole profile, shared index,
//! database, credential store, or symlink target. Called by move --backup after its conflict checks.

use crate::agent::Agent;
use crate::ospath;
use crate::paths;
use crate::py;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn within_profile(profile: &str, path: &str) -> bool {
    let profile = PathBuf::from(ospath::realpath(profile));
    Path::new(&ospath::realpath(path)).starts_with(profile)
}

fn sources(
    agent: Agent,
    sid: &str,
    profile: &str,
    files: &[String],
) -> Result<Vec<String>, String> {
    let mut found = BTreeSet::new();
    for path in files {
        let owned = match agent {
            Agent::Claude if path.ends_with(".jsonl") => {
                let data = &path[..path.len() - ".jsonl".len()];
                ospath::basename(data) == sid && within_profile(profile, path)
            }
            Agent::Codex => {
                crate::paths::codex_file_id(path) == Some(sid) && within_profile(profile, path)
            }
            Agent::Kimi => crate::kimi::dir_id(path) == Some(sid) && within_profile(profile, path),
            _ => false,
        };
        if owned {
            found.insert(ospath::abspath(path));
            if agent == Agent::Claude {
                let data = &path[..path.len() - ".jsonl".len()];
                if ospath::isdir(data) && within_profile(profile, data) {
                    found.insert(ospath::abspath(data));
                }
            }
        }
    }
    if found.is_empty() {
        return Err(format!(
            "No session-owned files of {} can be backed up from {}",
            agent.name(),
            paths::shell_path(profile)
        ));
    }
    Ok(found.into_iter().collect())
}

#[derive(Default)]
struct Count {
    files: usize,
    bytes: u64,
    skipped: usize,
}

fn copy_path(source: &Path, destination: Option<&Path>, count: &mut Count) -> std::io::Result<()> {
    let metadata = std::fs::symlink_metadata(source)?;
    if metadata.file_type().is_symlink() {
        count.skipped += 1;
        return Ok(()); // a backup never follows a link out of the session's files
    }
    if metadata.is_dir() {
        if let Some(destination) = destination {
            std::fs::create_dir_all(destination)?;
        }
        let mut entries: Vec<PathBuf> = std::fs::read_dir(source)?
            .map(|entry| entry.map(|e| e.path()))
            .collect::<Result<_, _>>()?;
        entries.sort();
        for entry in entries {
            let name = entry.file_name().and_then(|n| n.to_str()).ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "non-UTF-8 session filename",
                )
            })?;
            copy_path(&entry, destination.map(|d| d.join(name)).as_deref(), count)?;
        }
        return Ok(());
    }
    if !metadata.is_file() {
        count.skipped += 1;
        return Ok(());
    }
    if let Some(destination) = destination {
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let copied = std::fs::copy(source, destination)?;
        if copied != metadata.len() || std::fs::metadata(destination)?.len() != metadata.len() {
            return Err(std::io::Error::other(format!(
                "backup of {} has the wrong size",
                source.display()
            )));
        }
    } else {
        std::fs::File::open(source)?; // a dry run checks that the source can be opened too
    }
    count.files += 1;
    count.bytes += metadata.len();
    Ok(())
}

/// Back up the session-owned source files, or with dry_run only check and describe that backup.
/// The original is untouched; a failed real copy removes only the incomplete backup it just made.
pub fn create(
    agent: Agent,
    sid: &str,
    profile: &str,
    files: &[String],
    dry_run: bool,
) -> Result<Value, String> {
    if !matches!(agent, Agent::Claude | Agent::Codex | Agent::Kimi) {
        return Err(
            "Backups are available for same-agent Claude Code, Codex and Kimi moves".into(),
        );
    }
    let sources = sources(agent, sid, profile, files)?;
    let path = ospath::join_all(
        &paths::config_dir(),
        &[
            "backups",
            &format!(
                "{}-{}-{}-{}",
                crate::times::now() as u64,
                agent.key(),
                py::head(sid, 8),
                std::process::id()
            ),
        ],
    );
    let root = PathBuf::from(&path);
    if !dry_run {
        std::fs::create_dir_all(root.parent().unwrap_or_else(|| Path::new("")))
            .and_then(|()| std::fs::create_dir(&root))
            .map_err(|e| format!("can't make backup {}: {e}", paths::shell_path(&path)))?;
    }
    let result = (|| {
        let mut total = Count::default();
        let mut items = Vec::new();
        for (index, source) in sources.iter().enumerate() {
            let destination = root.join("items").join(format!("{index:03}"));
            let mut count = Count::default();
            copy_path(
                Path::new(source),
                (!dry_run).then_some(destination.as_path()),
                &mut count,
            )
            .map_err(|e| format!("can't back up {}: {e}", paths::shell_path(source)))?;
            total.files += count.files;
            total.bytes += count.bytes;
            total.skipped += count.skipped;
            items.push(
                json!({"source": source, "backup": destination.to_string_lossy(),
                "files": count.files, "bytes": count.bytes, "skipped": count.skipped}),
            );
        }
        let manifest = json!({"version": 1, "created_at": crate::times::iso_utc(crate::times::now()),
            "agent": agent.key(), "id": sid, "profile": ospath::abspath(profile), "items": items});
        if !dry_run {
            crate::bind::write_json(&ospath::join(&path, "manifest.json"), &manifest)
                .map_err(|e| format!("can't write the backup manifest: {e}"))?;
        }
        Ok(
            json!({"path": path, "dry_run": dry_run, "files": total.files,
            "bytes": total.bytes, "skipped": total.skipped, "items": manifest["items"].clone()}),
        )
    })();
    if result.is_err() && !dry_run {
        let _ = std::fs::remove_dir_all(&root); // only the folder this run just created
    }
    result
}
