//! hatcheck move SESSION PROFILE ...: move a session to another profile, to carry on with it
//! there under that profile's account and plan limits; with --keep, copy it; between Claude Code
//! and Codex, or Codex homes on different accounts, convert it, keeping the original. The
//! original is deleted only once every copied file is checked, and a copy there that went further
//! is never replaced: the move stops instead. hatcheck fork SESSION PROFILE ...: bring it in as a
//! new session with its own ID, beside any copy already there, keeping the original. The
//! docs/commands.md's `hatcheck move` and `hatcheck fork` sections describe what's copied, how a copy
//! already there is brought up to date, and what a conversion keeps.

use crate::agent::Agent;
use crate::agy;
use crate::args::{Count, Parser, Takes};
use crate::conv::{self, add_turn, Conversation, Turn};
use crate::dsh;
use crate::facts::{self, Summary};
use crate::find::place;
use crate::kimi;
use crate::ospath::{self, Part};
use crate::out;
use crate::paths;
use crate::py::{self, get, s, truthy};
use crate::sessions;
use crate::times;
use crate::usage;
use crate::widths::truncate;
use crate::zcode;
use serde_json::{json, Map, Value};
use std::fs;
use std::io::Write;

const TMP: &str = ".hatcheck-move-tmp"; // suffix of a copy in progress
/// How much conversation to carry when converting, in characters (about 100,000 tokens).
const BUDGET: usize = 400_000;
/// Fields that hold a session's own ID, changed in a --fork copy.
const ID_KEYS: [&str; 4] = ["sessionId", "session_id", "thread_id", "id"];

/// A stop with a message (sys.exit("...")): exit code 1.
type Stop = String;

fn join(base: &str, name: &str) -> String {
    if name.is_empty() {
        base.to_string()
    } else {
        ospath::join(base, name)
    }
}

/// The files in a folder, as paths relative to it, sorted, or [""] for a file.
fn files_in(path: &str) -> Vec<String> {
    if !ospath::isdir(path) {
        return vec![String::new()];
    }
    fn walk(dir: &std::path::Path, base: &std::path::Path, out: &mut Vec<String>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let p = entry.path();
            let link = entry.file_type().is_ok_and(|t| t.is_symlink());
            if p.is_dir() {
                if !link {
                    walk(&p, base, out); // os.walk doesn't follow links to folders
                }
            } else if let Ok(rel) = p.strip_prefix(base) {
                out.push(rel.to_string_lossy().into_owned());
            }
        }
    }
    let mut out = Vec::new();
    walk(
        std::path::Path::new(path),
        std::path::Path::new(path),
        &mut out,
    );
    out.sort();
    out
}

fn read(path: &str) -> std::io::Result<Vec<u8>> {
    fs::read(path)
}

/// value with each old path changed to its new one, and with ids the session's ID changed
/// where a field holds it. Thinking blocks are left alone: their signatures cover the text.
fn repoint(value: &Value, pairs: &[(String, String)], ids: Option<&(String, String)>) -> Value {
    match value {
        Value::String(text) => {
            let mut t = text.clone();
            for (old, new) in pairs {
                t = t.replace(old.as_str(), new);
            }
            Value::String(t)
        }
        Value::Array(items) => Value::Array(items.iter().map(|v| repoint(v, pairs, ids)).collect()),
        Value::Object(o) => {
            if matches!(
                s(get(value, "type")),
                Some("thinking" | "redacted_thinking")
            ) {
                return value.clone();
            }
            Value::Object(
                o.iter()
                    .map(|(k, v)| {
                        let changed = ids.filter(|(old, _)| {
                            ID_KEYS.contains(&k.as_str()) && s(v) == Some(old.as_str())
                        });
                        (
                            k.clone(),
                            changed.map_or_else(
                                || repoint(v, pairs, ids),
                                |(_, new)| Value::String(new.clone()),
                            ),
                        )
                    })
                    .collect(),
            )
        }
        other => other.clone(),
    }
}

/// A path as it appears in JSON text: json.dumps(old)[1:-1], so with Windows' backslashes doubled.
fn json_text(text: &str) -> Vec<u8> {
    let quoted = py::dumps_compact(&Value::String(text.to_string()));
    quoted.as_bytes()[1..quoted.len() - 1].to_vec()
}

/// What the copy of file src holds: its bytes, with a transcript's paths repointed and, for
/// --fork, the session's ID changed. Only lines holding an old path or the ID are rewritten.
fn copy_bytes(
    src: &str,
    pairs: &[(String, String)],
    ids: Option<&(String, String)>,
) -> std::io::Result<Vec<u8>> {
    let data = read(src)?;
    let mut olds: Vec<Vec<u8>> = pairs.iter().map(|(old, _)| json_text(old)).collect();
    if let Some((old, _)) = ids {
        olds.push(old.as_bytes().to_vec());
    }
    let has = |hay: &[u8]| olds.iter().any(|o| memchr::memmem::find(hay, o).is_some());
    if !src.ends_with(".jsonl") || !has(&data) {
        return Ok(data);
    }
    let mut lines: Vec<Vec<u8>> = data.split(|b| *b == b'\n').map(<[u8]>::to_vec).collect();
    for line in lines.iter_mut() {
        if has(line) {
            if let Ok(record) = serde_json::from_slice::<Value>(line) {
                *line = py::dumps_tight(&repoint(&record, pairs, ids)).into_bytes();
            } // a line that doesn't round-trip stays as it was
        }
    }
    Ok(lines.join(&b'\n'))
}

/// Whether dst holds exactly what copying src gives.
fn matches(
    src: &str,
    dst: &str,
    pairs: &[(String, String)],
    ids: Option<&(String, String)>,
) -> bool {
    if !ospath::exists(dst) || ospath::isdir(src) != ospath::isdir(dst) {
        return false;
    }
    let names = files_in(src);
    names == files_in(dst) && names.iter().all(|n| matches!((read(&join(dst, n)), copy_bytes(&join(src, n), pairs, ids)), (Ok(a), Ok(b)) if a == b))
}

/// A JSONL file's lines, without the empty one after the last newline.
fn lines_of(data: &[u8]) -> Vec<&[u8]> {
    let mut lines: Vec<&[u8]> = data.split(|b| *b == b'\n').collect();
    if lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }
    lines
}

/// Whether two lines hold the same record, even if one was written out differently.
fn same_line(a: &[u8], b: &[u8]) -> bool {
    if a == b {
        return true;
    }
    match (
        serde_json::from_slice::<Value>(a),
        serde_json::from_slice::<Value>(b),
    ) {
        (Ok(x), Ok(y)) => py::same(&x, &y),
        _ => false,
    }
}

fn shared_lines(a: &[&[u8]], b: &[&[u8]]) -> usize {
    let mut n = 0;
    while n < a.len().min(b.len()) && same_line(a[n], b[n]) {
        n += 1;
    }
    n
}

/// How a file already in the profile (have) stands to what copying would put there (want).
fn standing(have: &[u8], want: &[u8], jsonl: bool, mtimes: (f64, f64)) -> &'static str {
    if have == want {
        return "same";
    }
    if jsonl {
        let (a, b) = (lines_of(have), lines_of(want));
        let n = shared_lines(&a, &b);
        if n == a.len() && n == b.len() {
            return "same";
        }
        return if n == a.len() {
            "older"
        } else if n == b.len() {
            "newer"
        } else {
            "diverged"
        };
    }
    if want.starts_with(have) {
        return "older";
    }
    if have.starts_with(want) {
        return "newer";
    }
    // Written over rather than added to, such as a plan: the one changed last is newer.
    if mtimes.0 < mtimes.1 {
        "older"
    } else if mtimes.0 > mtimes.1 {
        "newer"
    } else {
        "diverged"
    }
}

/// What standings add up to: diverged once each side has something the other lacks.
fn overall<'a>(states: impl Iterator<Item = &'a str>) -> &'static str {
    let mut found: Vec<&str> = states
        .map(|s| if s == "new" { "older" } else { s })
        .filter(|s| *s != "same")
        .collect();
    found.dedup();
    let has = |x: &str| found.contains(&x);
    if has("diverged") || (has("older") && has("newer")) {
        "diverged"
    } else if has("older") {
        "older"
    } else if has("newer") {
        "newer"
    } else {
        "same"
    }
}

fn item_standing(
    src: &str,
    dst: &str,
    pairs: &[(String, String)],
) -> std::io::Result<&'static str> {
    if !ospath::lexists(dst) {
        return Ok("new");
    }
    if !ospath::exists(src) {
        return Ok("newer");
    }
    if ospath::isdir(src) != ospath::isdir(dst) {
        return Ok("diverged");
    }
    let (ours, theirs) = (files_in(src), files_in(dst));
    let mut states: Vec<&str> = Vec::new();
    states.extend(ours.iter().filter(|n| !theirs.contains(n)).map(|_| "older"));
    states.extend(theirs.iter().filter(|n| !ours.contains(n)).map(|_| "newer"));
    for name in ours.iter().filter(|n| theirs.contains(n)) {
        let (a, b) = (join(src, name), join(dst, name));
        states.push(standing(
            &read(&b)?,
            &copy_bytes(&a, pairs, None)?,
            b.ends_with(".jsonl"),
            (ospath::getmtime(&b)?, ospath::getmtime(&a)?),
        ));
    }
    Ok(overall(states.into_iter()))
}

fn records_of<'a>(lines: &'a [&'a [u8]]) -> impl Iterator<Item = Value> + 'a {
    lines
        .iter()
        .filter_map(|l| serde_json::from_slice::<Value>(l).ok())
        .filter(Value::is_object)
}

/// How many prompts these lines of a session hold.
fn count_prompts(lines: &[&[u8]], agent: Agent) -> usize {
    match agent {
        // each prompt is in a user_message event, and again as a response item
        Agent::Codex => records_of(lines)
            .filter(|r| s(get(r, "type")) == Some("event_msg") && facts::codex_prompt(r).is_some())
            .count(),
        Agent::Claude => records_of(lines)
            .filter(|r| {
                s(get(r, "type")) == Some("user")
                    && !truthy(get(r, "isMeta"))
                    && !truthy(get(r, "isSidechain"))
                    && facts::prompt_text(r).is_some()
            })
            .count(),
        Agent::Agy | Agent::Zcode => 0, // SQLite, never compared line by line
        Agent::Dsh => 0,                // compressed generations, never compared line by line
        Agent::Mcode => 0,              // SQLite, never compared line by line
        Agent::Qwen => 0,               // never copied, so never compared
        Agent::Kimi => records_of(lines)
            .filter(|r| s(get(r, "type")) == Some("turn.prompt"))
            .count(),
    }
}

/// For two copies that each went on: lines shared, when they were last the same, and the
/// prompts each has had since (this one's, the other's).
fn since_split(ours: &[u8], theirs: &[u8], agent: Agent) -> (usize, Value, usize, usize) {
    let (a, b) = (lines_of(ours), lines_of(theirs));
    let n = shared_lines(&a, &b);
    let when = records_of(&a[..n])
        .filter(|r| truthy(get(r, "timestamp")))
        .last()
        .map_or(Value::Null, |r| get(&r, "timestamp").clone());
    (
        n,
        when,
        count_prompts(&a[n..], agent),
        count_prompts(&b[n..], agent),
    )
}

/// The time of a session file's last record that has one.
fn last_time(data: &[u8]) -> Value {
    let lines = lines_of(data);
    records_of(&lines)
        .filter(|r| truthy(get(r, "timestamp")))
        .last()
        .map_or(Value::Null, |r| get(&r, "timestamp").clone())
}

fn time_text(stamp: &Value) -> String {
    match s(stamp).and_then(times::parse_time).filter(|t| *t != 0.0) {
        Some(t) => times::short_local(t),
        None => "an earlier move".into(),
    }
}

pub(crate) fn interactive() -> bool {
    use std::io::IsTerminal;
    std::io::stdin().is_terminal() && std::io::stdout().is_terminal()
}

/// What you type at a prompt, as input() reads it: None at the end of input, or when Ctrl+C
/// stops it, which move takes the same way. The prompt goes to stderr, where input() writes it
/// in a terminal.
pub(crate) fn read_answer(prompt: &str) -> Option<String> {
    let line = read_line(prompt);
    line.map(|l| l.trim_end_matches(['\n', '\r']).to_string())
}

#[cfg(not(unix))]
fn read_line(prompt: &str) -> Option<String> {
    out::prompt(prompt);
    let mut line = String::new();
    match std::io::stdin().read_line(&mut line) {
        Ok(0) | Err(_) => None,
        Ok(_) => Some(line),
    }
}

/// A line from stdin, read a byte at a time with Ctrl+C caught while it's read, so that Ctrl+C
/// gives None (KeyboardInterrupt) rather than ending hatcheck there and then.
#[cfg(unix)]
fn read_line(prompt: &str) -> Option<String> {
    use std::sync::atomic::{AtomicBool, Ordering};
    static STOPPED: AtomicBool = AtomicBool::new(false);
    extern "C" fn on_interrupt(_: libc::c_int) {
        STOPPED.store(true, Ordering::SeqCst);
    }
    STOPPED.store(false, Ordering::SeqCst);
    // SAFETY: sigaction with a handler that only sets an atomic flag, put back below; read
    // into a one-byte buffer on the stack.
    unsafe {
        let mut catch: libc::sigaction = std::mem::zeroed();
        let mut was: libc::sigaction = std::mem::zeroed();
        catch.sa_sigaction = on_interrupt as extern "C" fn(libc::c_int) as libc::sighandler_t;
        catch.sa_flags = 0; // no SA_RESTART: read() returns EINTR on Ctrl+C
        libc::sigemptyset(&mut catch.sa_mask);
        libc::sigaction(libc::SIGINT, &catch, &mut was);
        out::prompt(prompt); // once Ctrl+C is caught, as it always is in Python
        let mut line = Vec::new();
        let got = loop {
            let mut byte = 0u8;
            match libc::read(0, (&mut byte as *mut u8).cast(), 1) {
                1 if byte == b'\n' => break true,
                1 => line.push(byte),
                0 => break !line.is_empty(), // the end of input: a line without its newline, or none
                _ if std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted
                    && !STOPPED.load(Ordering::SeqCst) => {}
                _ => break false,
            }
        };
        libc::sigaction(libc::SIGINT, &was, std::ptr::null_mut());
        got.then(|| String::from_utf8_lossy(&line).into_owned())
    }
}

/// A copy of the session in one profile: its folder, its transcript's bytes, and when it
/// was last used.
struct SessionCopy {
    folder: String,
    data: Vec<u8>,
    time: f64,
}

/// Where a session is: each profile or home that has a copy, with its files there.
type Places = Vec<(String, Vec<String>)>;

fn copies_used_last_first(places: &[(String, Vec<String>)]) -> Vec<SessionCopy> {
    let mut copies: Vec<SessionCopy> = places
        .iter()
        .map(|(folder, files)| {
            let path = newest(files);
            let data = read(&path).unwrap_or_default();
            let when = last_time(&data);
            let t = s(&when).and_then(times::parse_time).filter(|t| *t != 0.0);
            let time = t.unwrap_or_else(|| ospath::getmtime(&path).unwrap_or(0.0));
            SessionCopy {
                folder: folder.clone(),
                data,
                time,
            }
        })
        .collect();
    copies.sort_by(|a, b| {
        b.time
            .partial_cmp(&a.time)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    copies
}

/// The file changed last of several: max(files, key=os.path.getmtime).
fn newest(files: &[String]) -> String {
    files
        .iter()
        .fold(None::<(f64, &String)>, |best, f| {
            let t = ospath::getmtime(f).unwrap_or(f64::NEG_INFINITY);
            match best {
                Some((bt, _)) if t <= bt => best,
                _ => Some((t, f)),
            }
        })
        .map_or_else(String::new, |(_, f)| f.clone())
}

fn copy_lines(copies: &[SessionCopy], agent: Agent) -> Vec<String> {
    let newest = &copies[0];
    let width = copies
        .iter()
        .map(|c| paths::shell_path(&c.folder).chars().count())
        .max()
        .unwrap_or(0);
    copies
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let total = count_prompts(&lines_of(&c.data), agent);
            let standing = if i == 0 {
                "used last".to_string()
            } else {
                let (_, split, mine, other) = since_split(&newest.data, &c.data, agent);
                let s_ = |n: usize| if n != 1 { "s" } else { "" };
                if mine == 0 && other == 0 {
                    "the same as 1".into()
                } else if other == 0 {
                    format!("{mine} prompt{} behind 1", s_(mine))
                } else if mine == 0 {
                    format!("{other} prompt{} beyond 1", s_(other))
                } else {
                    format!(
                        "went its own way after {}: {other} prompt{} of its own",
                        time_text(&split),
                        s_(other)
                    )
                }
            };
            format!(
                "{:<width$}  {}  {total:>4} prompt{}  {standing}",
                paths::shell_path(&c.folder),
                times::short_local(c.time),
                if total == 1 { " " } else { "s" }
            )
        })
        .collect()
}

/// Which copy to take when several profiles have the session: the one used last, unless you
/// pick another at the prompt. A script, a pipe or --json gets the one used last.
fn choose_source(
    sid: &str,
    places: &[(String, Vec<String>)],
    agent: Agent,
    json_out: bool,
    report: &mut Map<String, Value>,
) -> Result<String, Stop> {
    let copies = copies_used_last_first(places);
    report.insert(
        "copies".into(),
        json!(copies.iter().map(|c| c.folder.clone()).collect::<Vec<_>>()),
    );
    report.insert("source_choice".into(), json!("used last"));
    let id8 = py::head(sid, 8);
    if json_out || !interactive() {
        let others = copies[1..]
            .iter()
            .map(|c| paths::shell_path(&c.folder))
            .collect::<Vec<_>>()
            .join(", ");
        out::println(&format!("Session {id8} is in {others} too. Taking the copy in {}, used last; --from picks another.", paths::shell_path(&copies[0].folder)));
        return Ok(copies[0].folder.clone());
    }
    out::println(&format!("Session {id8} is in {} profiles:", copies.len()));
    for (i, line) in copy_lines(&copies, agent).iter().enumerate() {
        out::println(&format!("  {}. {line}", i + 1));
    }
    loop {
        let Some(answer) = read_answer("Which one? [1] ") else {
            out::println("");
            return Err("Nothing changed.".into());
        };
        let answer = py::strip(&answer).to_string();
        if answer.is_empty() {
            return Ok(copies[0].folder.clone());
        }
        if let Some(n) = answer.parse::<usize>().ok().filter(|n| {
            answer.bytes().all(|b| b.is_ascii_digit()) && (1..=copies.len()).contains(n)
        }) {
            return Ok(copies[n - 1].folder.clone());
        }
        for c in &copies {
            let base = ospath::basename(&c.folder);
            let short = base
                .split_once('-')
                .map_or("default".to_string(), |(_, rest)| rest.to_string());
            let names = [
                paths::shell_path(&c.folder),
                c.folder.clone(),
                base.clone(),
                short.clone(),
                format!("{}:{short}", agent.key()),
            ];
            if names.contains(&answer) {
                return Ok(c.folder.clone());
            }
        }
        report.insert("source_choice".into(), json!("asked"));
        out::println(&format!(
            "Answer with a number from 1 to {}, or the profile's name.",
            copies.len()
        ));
    }
}

/// Copy a file's or folder's times and permission bits, as shutil.copystat does: the times
/// first, so a read-only original's copy still gets them.
pub(crate) fn copy_stat(src: &str, dst: &str) -> std::io::Result<()> {
    let meta = fs::metadata(src)?;
    let times = fs::FileTimes::new()
        .set_accessed(meta.accessed()?)
        .set_modified(meta.modified()?);
    open_to_stamp(dst, meta.is_dir())?.set_times(times)?;
    fs::set_permissions(dst, meta.permissions())
}

#[cfg(unix)]
fn open_to_stamp(path: &str, dir: bool) -> std::io::Result<fs::File> {
    if dir {
        fs::File::open(path)
    } else {
        fs::OpenOptions::new().write(true).open(path)
    }
}

#[cfg(windows)]
fn open_to_stamp(path: &str, dir: bool) -> std::io::Result<fs::File> {
    use std::os::windows::fs::OpenOptionsExt;
    const FILE_WRITE_ATTRIBUTES: u32 = 0x100;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000; // what opening a folder takes
    fs::OpenOptions::new()
        .access_mode(FILE_WRITE_ATTRIBUTES)
        .custom_flags(if dir { FILE_FLAG_BACKUP_SEMANTICS } else { 0 })
        .open(path)
}

/// Copy a file or folder with its timestamps, under a temporary name until it's whole.
fn copy(
    src: &str,
    dst: &str,
    pairs: &[(String, String)],
    ids: Option<&(String, String)>,
) -> std::io::Result<()> {
    fn copy_file(
        a: &str,
        b: &str,
        pairs: &[(String, String)],
        ids: Option<&(String, String)>,
    ) -> std::io::Result<()> {
        fs::write(b, copy_bytes(a, pairs, ids)?)?;
        copy_stat(a, b)
    }
    fn copy_tree(
        a: &str,
        b: &str,
        pairs: &[(String, String)],
        ids: Option<&(String, String)>,
    ) -> std::io::Result<()> {
        fs::create_dir_all(b)?;
        let mut entries: Vec<_> = fs::read_dir(a)?.flatten().collect();
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let name = entry.file_name().to_string_lossy().into_owned();
            let (from, to) = (ospath::join(a, &name), ospath::join(b, &name));
            if entry.path().is_dir() {
                copy_tree(&from, &to, pairs, ids)?;
            } else {
                copy_file(&from, &to, pairs, ids)?;
            }
        }
        copy_stat(a, b)
    }
    let tmp = format!("{dst}{TMP}");
    if ospath::isdir(&tmp) {
        fs::remove_dir_all(&tmp)?; // left by an interrupted run
    }
    fs::create_dir_all(ospath::dirname(dst))?;
    if ospath::isdir(src) {
        copy_tree(src, &tmp, pairs, ids)?;
    } else {
        copy_file(src, &tmp, pairs, ids)?;
    }
    fs::rename(&tmp, dst)
}

/// Turn an older copy of a file or folder into a copy of src: each file it lacks, or has less
/// of, is written whole, under a temporary name until it's complete.
fn bring_up_to_date(src: &str, dst: &str, pairs: &[(String, String)]) -> std::io::Result<()> {
    for name in files_in(src) {
        let (a, b) = (join(src, &name), join(dst, &name));
        if !ospath::isfile(&b) || read(&b)? != copy_bytes(&a, pairs, None)? {
            copy(&a, &b, pairs, None)?;
        }
    }
    Ok(())
}

/// (old, new), plus on Windows the same with forward slashes, which tools print too.
fn path_pairs(old: &str, new: &str) -> Vec<(String, String)> {
    let mut pairs = vec![(old.to_string(), new.to_string())];
    if ospath::WINDOWS {
        pairs.push((old.replace('\\', "/"), new.replace('\\', "/")));
    }
    pairs
}

/// Names of the plan files (plans/<slug>.md) the transcripts use.
fn plan_slugs(transcripts: &[String]) -> Vec<String> {
    let mut slugs: Vec<String> = Vec::new();
    for path in transcripts {
        let Ok(data) = read(path) else { continue };
        for line in data.split(|b| *b == b'\n') {
            if memchr::memmem::find(line, b"\"slug\":\"").is_none() {
                continue;
            }
            let Ok(record) = serde_json::from_slice::<Value>(line) else {
                continue;
            };
            if let Some(slug) =
                s(get(&record, "slug")).filter(|s| !s.is_empty() && !s.contains('/'))
            {
                if !slugs.iter().any(|x| x == slug) {
                    slugs.push(slug.to_string());
                }
            }
        }
    }
    slugs.sort();
    slugs
}

fn size_text(n: u64) -> String {
    if n < 1024 {
        format!("{n} bytes")
    } else if n < 1024 * 1024 {
        format!("{:.1} KB", n as f64 / 1024.0)
    } else {
        format!("{:.1} MB", n as f64 / (1024.0 * 1024.0))
    }
}

/// Start the agent on the session in its new profile, from the folder it ran in. For agy and
/// zcode, sid is the import file. DSH's official web UI opens its session history instead.
fn resume_now(target: &str, sid: &str, cwd: &str, agent: Agent) -> i32 {
    let name = agent.program();
    let Some(program) = ospath::which(name) else {
        return out::fail(&format!(
            "{name} isn't on your PATH; once it is, resume it with the command above"
        ));
    };
    let mut cmd = std::process::Command::new(&program);
    match agent {
        Agent::Codex => cmd.args(["resume", sid]),
        Agent::Claude => cmd.args(["--resume", sid]),
        Agent::Agy => cmd.args(["--add-dir", &conv::imports_dir(), "-i", &agy::prompt(sid)]),
        Agent::Kimi => cmd.args(["--session", &Agent::Kimi.full_id(sid)]),
        Agent::Zcode => cmd.args(["--attach", sid, "--mode", "plan", "-p", &zcode::prompt()]),
        Agent::Dsh => cmd.arg("web"),
        Agent::Mcode => cmd.args(["--session", &Agent::Mcode.full_id(sid)]),
        Agent::Qwen => cmd.args(["--resume", sid]),
    };
    let default = paths::default_folder(agent);
    match agent.variable() {
        Some(variable) if ospath::realpath(target) == ospath::realpath(&default) => {
            cmd.env_remove(variable); // where plain claude, codex or kimi looks
        }
        Some(variable) => {
            cmd.env(variable, target); // spelled as cl and cx spell it: Claude Code files the login under this string
        }
        None => {} // agy and zcode read their own folder, the only one there is
    }
    cmd.current_dir(cwd);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let e = cmd.exec();
        out::fail(&e.to_string())
    }
    #[cfg(not(unix))]
    {
        // No exec on Windows: run it and leave Ctrl+C to the agent.
        #[cfg(windows)]
        // SAFETY: no handler, only the flag that has this process ignore Ctrl+C.
        unsafe {
            windows_sys::Win32::System::Console::SetConsoleCtrlHandler(None, 1);
        }
        match cmd.status() {
            Ok(st) => st.code().unwrap_or(1),
            Err(e) => out::fail(&e.to_string()),
        }
    }
}

/// The lock Codex holds on a session while it's open.
enum Lock {
    /// Taken, so the session doesn't change while it's copied; dropping the file lets go.
    Free(#[allow(dead_code)] Option<fs::File>),
    /// Codex has the session open.
    Held,
}

/// Take the lock Codex holds on a session while it's open. Not checked on Windows.
fn lock_codex_session(home: &str, sid: &str) -> std::io::Result<Lock> {
    let path = ospath::join_all(home, &["thread-writer-locks", &format!("{sid}.lock")]);
    if ospath::WINDOWS || !ospath::isfile(&path) {
        return Ok(Lock::Free(None)); // Codex makes the file when it opens the session
    }
    let file = fs::OpenOptions::new().read(true).write(true).open(&path)?;
    Ok(match file.try_lock() {
        Ok(()) => Lock::Free(Some(file)),
        Err(_) => Lock::Held,
    })
}

/// The lock on a Codex session, or the stop that says Codex has it open: in the home it's
/// taken from, or with there, the one it would go to.
fn lock_or_stop(home: &str, sid: &str, there: bool) -> Result<Lock, Stop> {
    match lock_codex_session(home, sid).map_err(|e| e.to_string())? {
        Lock::Held => Err(format!(
            "Codex session {} is open in codex{}; quit it first, so it stops changing",
            py::head(sid, 8),
            where_it_would_go(home, there)
        )),
        free => Ok(free),
    }
}

/// " in ~/.claude-b, where it would go" for the profile or home a session is going to.
fn where_it_would_go(place: &str, there: bool) -> String {
    if there {
        format!(" in {}, where it would go", paths::shell_path(place))
    } else {
        String::new()
    }
}

fn random_bytes<const N: usize>() -> [u8; N] {
    let mut buf = [0u8; N];
    let _ = getrandom::fill(&mut buf);
    buf
}

/// A random (version 4) UUID, as Python's uuid.uuid4 makes one.
pub(crate) fn uuid4() -> String {
    let mut b = random_bytes::<16>();
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    format_uuid(u128::from_be_bytes(b))
}

/// A version 7 UUID for time t, the kind Codex gives its sessions: they sort by time.
fn uuid7(t: f64) -> String {
    let ms = ((t * 1000.0).trunc() as u128) & ((1 << 48) - 1);
    let rand = random_bytes::<10>()
        .iter()
        .fold(0u128, |acc, b| acc << 8 | u128::from(*b));
    format_uuid(
        ms << 80 | 0x7 << 76 | ((rand >> 62) & 0xFFF) << 64 | 0b10 << 62 | (rand & ((1 << 62) - 1)),
    )
}

fn format_uuid(n: u128) -> String {
    let h = format!("{n:032x}");
    format!(
        "{}-{}-{}-{}-{}",
        &h[..8],
        &h[8..12],
        &h[12..16],
        &h[16..20],
        &h[20..]
    )
}

/// datetime.fromtimestamp(t, timezone.utc).strftime("%Y-%m-%dT%H:%M:%S.") with the
/// milliseconds after it, as the Python version wrote converted sessions' times.
pub(crate) fn iso_ms(t: f64) -> String {
    let seconds = times::utc(t)
        .map(|d| d.format("%Y-%m-%dT%H:%M:%S.").to_string())
        .unwrap_or_default();
    format!(
        "{seconds}{:03}Z",
        ((t * 1000.0).trunc() as i64).rem_euclid(1000)
    )
}

/// The first value of key in the newest of these JSONL files that has one.
fn newest_value(files: &[String], key: &str) -> Option<String> {
    let mut dated: Vec<(f64, &String)> = files
        .iter()
        .filter_map(|p| ospath::getmtime(p).ok().map(|t| (t, p)))
        .collect();
    dated.sort_by(|a, b| {
        b.0.partial_cmp(&a.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.1.cmp(a.1))
    });
    for (_, path) in dated.into_iter().take(20) {
        let Ok(file) = fs::File::open(path) else {
            continue;
        };
        for line in std::io::BufRead::split(std::io::BufReader::new(file), b'\n').take(50) {
            let Ok(line) = line else { break };
            let Ok(record) = serde_json::from_str::<Value>(&String::from_utf8_lossy(&line)) else {
                continue;
            };
            let mut value = get(&record, key);
            if value.is_null() && get(&record, "payload").is_object() {
                value = get(get(&record, "payload"), key);
            }
            if let Some(v) = s(value).filter(|v| !v.is_empty()) {
                return Some(v.to_string());
            }
        }
    }
    None
}

fn all_transcripts(profile: &str) -> Vec<String> {
    ospath::glob(
        profile,
        &[
            Part::Name("projects"),
            Part::Wild("", ""),
            Part::Wild("", ".jsonl"),
        ],
    )
}

/// The conversation's turns as the new agent should get them: a note about where they came
/// from first, the oldest turns after the first left out past BUDGET, alternating between you
/// and the assistant, ending on a reply. Also how many turns were left out, and whether the
/// start of the newest was too.
fn fit(conv: &Conversation) -> (Vec<Turn>, usize, bool) {
    fit_within(conv, BUDGET)
}

/// The newest turn cut to room characters, its latest parts kept: what it ended on is where the
/// conversation carries on. When even its last part is too long, that part, cut.
fn fit_turn((role, texts, when): &Turn, room: usize) -> Turn {
    let mut kept: Vec<String> = Vec::new();
    let mut used = 0;
    for text in texts.iter().rev() {
        used += text.chars().count();
        if used > room {
            break;
        }
        kept.push(text.clone());
    }
    if kept.is_empty() {
        kept.push(conv::clip(
            texts.last().map_or("", String::as_str),
            (usize::MAX, room),
        ));
    }
    kept.reverse();
    let left = texts.len() - kept.len();
    if left > 0 {
        kept.insert(
            0,
            format!(
                "[{left} earlier part{} of this turn left out here, to fit the context window.]",
                if left != 1 { "s" } else { "" }
            ),
        );
    }
    (role.clone(), kept, when.clone())
}

fn fit_within(conv: &Conversation, budget: usize) -> (Vec<Turn>, usize, bool) {
    let mut turns: Vec<Turn> = conv.turns.clone();
    let sizes: Vec<usize> = turns
        .iter()
        .map(|(_, texts, _)| texts.iter().map(|t| t.chars().count()).sum())
        .collect();
    let mut dropped = 0;
    let mut cut = false;
    if sizes.iter().sum::<usize>() > budget && turns.len() > 2 {
        // The first turn and the newest stay whatever their size, the newest cut when it's too
        // long alone: without it, the conversation wouldn't carry on from where it ended.
        let last = turns.len() - 1;
        let (mut start, mut total) = (last, sizes[0] + sizes[last]);
        while start > 1 && total + sizes[start - 1] <= budget {
            start -= 1;
            total += sizes[start];
        }
        if sizes[0] + sizes[last] > budget {
            turns[last] = fit_turn(&turns[last], budget.saturating_sub(sizes[0]));
            cut = true;
        }
        dropped = start - 1;
        let gap: Turn = (
            "user".into(),
            vec![format!(
                "[{dropped} earlier turn{} left out here, to fit the context window.]",
                if dropped != 1 { "s" } else { "" }
            )],
            turns[start - 1].2.clone(),
        );
        let mut kept = vec![turns[0].clone(), gap];
        kept.extend(turns[start..].iter().cloned());
        turns = kept;
    }
    let started = s(&conv.started)
        .map(|t| py::head(t, 16).replace('T', " "))
        .filter(|t| !t.is_empty());
    let model = s(&conv.model)
        .filter(|m| !m.is_empty())
        .map_or(String::new(), |m| format!(" with {m}"));
    let note = format!(
        "[Imported from {} session {}, started {} in {}{model}. Its tool calls are shown as text, with their output shortened, and {} isn't included.]",
        conv.agent.name(),
        conv.id,
        started.as_deref().unwrap_or("?"),
        conv.cwd.as_deref().filter(|c| !c.is_empty()).unwrap_or("?"),
        conv.reasoning
    );
    let mut merged: Vec<Turn> = Vec::new();
    let first: Turn = ("user".into(), vec![note], conv.started.clone());
    for (role, texts, when) in std::iter::once(first).chain(turns) {
        for text in texts {
            add_turn(&mut merged, &role, &text, &when);
        }
    }
    if merged.last().is_some_and(|t| t.0 == "user") {
        let when = merged.last().map(|t| t.2.clone()).unwrap_or(Value::Null);
        add_turn(
            &mut merged,
            "assistant",
            &format!("(The {} session ended here.)", conv.agent.name()),
            &when,
        );
    }
    (merged, dropped, cut)
}

fn write_lines(path: &str, lines: &[String]) -> std::io::Result<()> {
    fs::create_dir_all(ospath::dirname(path))?;
    let tmp = format!("{path}{TMP}");
    fs::write(
        &tmp,
        lines.iter().map(|l| format!("{l}\n")).collect::<String>(),
    )?;
    fs::rename(&tmp, path)
}

/// Write a converted conversation as a new Claude Code session in profile: its ID and file.
fn write_claude(
    conv: &Conversation,
    turns: &[Turn],
    profile: &str,
    cwd: &str,
) -> std::io::Result<(String, String)> {
    let (sid, now) = (uuid4(), times::now());
    let version = newest_value(&all_transcripts(profile), "version")
        .or_else(|| {
            newest_value(
                &paths::find_profiles()
                    .iter()
                    .flat_map(|p| all_transcripts(p))
                    .collect::<Vec<_>>(),
                "version",
            )
        })
        .unwrap_or_else(|| "2.1.0".into());
    let base = json!({"isSidechain": false, "userType": "external", "entrypoint": "cli", "cwd": cwd, "sessionId": sid,
                      "version": version, "gitBranch": if truthy(&conv.branch) { conv.branch.clone() } else { json!("") }});
    let with = |extra: Vec<(&str, Value)>| -> Map<String, Value> {
        let mut entry = base.as_object().cloned().unwrap_or_default();
        for (k, v) in extra {
            entry.insert(k.to_string(), v);
        }
        entry
    };
    let mut entries: Vec<Value> = Vec::new();
    let mut parent = Value::Null;
    for (n, (role, texts, when)) in turns.iter().enumerate() {
        let me = uuid4();
        let stamp = if truthy(when) {
            when.clone()
        } else {
            json!(iso_ms(now))
        };
        let mut entry = with(vec![
            ("parentUuid", parent.clone()),
            ("type", json!(role)),
            ("uuid", json!(me)),
            ("timestamp", stamp),
        ]);
        let text = texts.join("\n\n");
        if role == "user" {
            entry.insert("message".into(), json!({"role": "user", "content": text}));
        } else {
            entry.insert("requestId".into(), json!(format!("req_imported_{n}")));
            let model = if truthy(&conv.model) {
                conv.model.clone()
            } else {
                json!(conv.agent.name())
            };
            entry.insert("message".into(), json!({"model": model, "id": format!("msg_imported_{n}"), "type": "message", "role": "assistant",
                                                  "content": [{"type": "text", "text": text}], "stop_reason": "end_turn", "stop_sequence": null,
                                                  "usage": {"input_tokens": 0, "output_tokens": 0}}));
        }
        entries.push(Value::Object(entry));
        parent = json!(me);
    }
    // Shown when it's resumed; also dates the session to now, so it's listed as recent.
    entries.push(Value::Object(with(vec![
        ("parentUuid", parent),
        ("type", json!("system")),
        ("subtype", json!("informational")),
        ("isMeta", json!(false)),
        ("uuid", json!(uuid4())),
        ("timestamp", json!(iso_ms(now))),
        ("level", json!("notice")),
        (
            "content",
            json!(format!(
                "Converted from {} session {} with hatcheck move",
                conv.agent.name(),
                conv.id
            )),
        ),
    ])));
    if let Some(title) = conv.title.as_deref().filter(|t| !t.is_empty()) {
        entries.push(json!({"type": "ai-title", "aiTitle": title, "sessionId": sid}));
    }
    let path = ospath::join_all(
        profile,
        &[
            "projects",
            &paths::encode_project(cwd),
            &format!("{sid}.jsonl"),
        ],
    );
    write_lines(
        &path,
        &entries.iter().map(py::dumps_tight).collect::<Vec<_>>(),
    )?;
    Ok((sid, path))
}

/// The model_provider a Codex home's config.toml sets, or openai.
fn codex_provider(home: &str) -> String {
    let Ok(text) = fs::read_to_string(ospath::join(home, "config.toml")) else {
        return "openai".into();
    };
    for line in text.lines() {
        let t = line.trim_start_matches(py::is_space);
        if t.starts_with('[') {
            break; // past the top-level settings
        }
        let quoted = t
            .strip_prefix("model_provider")
            .map(|r| r.trim_start_matches(py::is_space))
            .and_then(|r| r.strip_prefix('='))
            .map(|r| r.trim_start_matches(py::is_space))
            .and_then(|r| r.strip_prefix(['"', '\'']));
        if let Some(rest) = quoted {
            let value = &rest[..rest.find(['"', '\'']).unwrap_or(0)];
            if !value.is_empty() {
                return value.to_string(); // one or more characters, then a closing quote
            }
        }
    }
    "openai".into()
}

/// The path of a new Codex session file for time now and ID sid.
fn codex_file_path(home: &str, now: f64, sid: &str) -> String {
    let local = times::local(now.floor()).map(|d| {
        (
            d.format("%Y").to_string(),
            d.format("%m").to_string(),
            d.format("%d").to_string(),
            d.format("%Y-%m-%dT%H-%M-%S").to_string(),
        )
    });
    let (y, m, d, stamp) = local.unwrap_or_default();
    ospath::join_all(
        home,
        &[
            "sessions",
            &y,
            &m,
            &d,
            &format!("rollout-{stamp}-{sid}.jsonl"),
        ],
    )
}

/// Write a converted conversation as a new Codex session in home, the way Codex's own /import
/// writes a Claude Code session: its ID and file.
fn write_codex(
    conv: &Conversation,
    turns: &[Turn],
    home: &str,
    cwd: &str,
) -> std::io::Result<(String, String)> {
    let now = times::now();
    let (sid, stamp) = (uuid7(now), iso_ms(now));
    let mut homes = vec![home.to_string()];
    homes.extend(paths::find_codex_homes().into_iter().filter(|h| h != home));
    let files: Vec<String> = homes
        .iter()
        .flat_map(|h| paths::codex_session_files(h))
        .collect();
    let started = now.trunc() as i64;
    let meta = json!({"id": sid, "timestamp": stamp, "cwd": cwd, "originator": "codex-tui",
                      "cli_version": newest_value(&files, "cli_version").unwrap_or_else(|| "0.155.1".into()), "source": "cli",
                      "model_provider": codex_provider(home), "history_mode": "legacy"});
    let mut records = vec![
        json!({"timestamp": stamp, "type": "session_meta", "payload": meta}),
        json!({"timestamp": stamp, "type": "event_msg", "payload": {"type": "task_started", "turn_id": "external-import-turn-1", "started_at": started, "model_context_window": null, "collaboration_mode_kind": "default"}}),
    ];
    for (role, texts, when) in turns {
        let (text, when) = (
            texts.join("\n\n"),
            if truthy(when) {
                when.clone()
            } else {
                json!(stamp)
            },
        );
        if role == "user" {
            records.push(json!({"timestamp": when, "type": "event_msg", "payload": {"type": "user_message", "message": text, "local_images": [], "local_audio": [], "text_elements": []}}));
            records.push(json!({"timestamp": when, "type": "response_item", "payload": {"type": "message", "role": "user", "content": [{"type": "input_text", "text": text}]}}));
        } else {
            records.push(json!({"timestamp": when, "type": "event_msg", "payload": {"type": "agent_message", "message": text, "phase": null, "memory_citation": null}}));
            records.push(json!({"timestamp": when, "type": "response_item", "payload": {"type": "message", "role": "assistant", "content": [{"type": "output_text", "text": text}]}}));
        }
    }
    let zero = json!({"input_tokens": 0, "cached_input_tokens": 0, "output_tokens": 0, "reasoning_output_tokens": 0, "total_tokens": 0});
    records.push(json!({"timestamp": stamp, "type": "event_msg", "payload": {"type": "agent_message", "message": "<EXTERNAL SESSION IMPORTED>", "phase": null, "memory_citation": null}}));
    records.push(json!({"timestamp": stamp, "type": "event_msg", "payload": {"type": "token_count", "info": {"total_token_usage": zero, "last_token_usage": zero, "model_context_window": null}, "rate_limits": null}}));
    records.push(json!({"timestamp": stamp, "type": "event_msg", "payload": {"type": "task_complete", "turn_id": "external-import-turn-1", "last_agent_message": null, "started_at": started, "completed_at": started}}));
    let path = codex_file_path(home, now, &sid);
    write_lines(
        &path,
        &records.iter().map(py::dumps_tight).collect::<Vec<_>>(),
    )?;
    if let Some(title) = conv.title.as_deref().filter(|t| !t.is_empty()) {
        name_session(home, &sid, title)?;
    }
    Ok((sid, path))
}

/// Give a Codex session a name, as /rename does: a line in the home's session_index.jsonl.
fn name_session(home: &str, sid: &str, name: &str) -> std::io::Result<()> {
    let stamp = chrono::Utc::now()
        .format("%Y-%m-%dT%H:%M:%S.%6fZ")
        .to_string();
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(ospath::join(home, "session_index.jsonl"))?;
    file.write_all(
        (py::dumps_compact(&json!({"id": sid, "thread_name": name, "updated_at": stamp})) + "\n")
            .as_bytes(),
    )
}

/// An item to copy: what it is, the original, the copy, and whether a move deletes it.
struct Item {
    what: &'static str,
    src: String,
    dst: String,
    deleted: bool,
}

/// A move: delete what was copied from the old profile, except plan files.
fn delete_originals(items: &[Item]) -> std::io::Result<()> {
    for item in items.iter().filter(|i| i.deleted) {
        if ospath::isdir(&item.src) {
            fs::remove_dir_all(&item.src)?;
        } else if ospath::exists(&item.src) {
            fs::remove_file(&item.src)?;
        }
    }
    Ok(())
}

/// What the session list knows about a session, from the newest of its transcripts.
fn read_info(transcripts: &[String]) -> Option<Summary> {
    let found: Vec<Summary> = transcripts
        .iter()
        .filter_map(|p| facts::read_session(p).ok().flatten())
        .collect();
    facts::save_cache();
    found
        .into_iter()
        .fold(None, |best: Option<Summary>, s| match best {
            Some(b) if (s.when, s.canonical, s.size) <= (b.when, b.canonical, b.size) => Some(b),
            _ => Some(s),
        })
}

/// Stop: the session went on in both places after one was copied from the other.
#[allow(clippy::too_many_arguments)]
fn went_on_in_both(
    agent: Agent,
    sid: &str,
    source: &str,
    target: &str,
    both: Option<(&[u8], &[u8])>,
    fork_cmd: &str,
    files: &[String],
    keep: bool,
) -> Stop {
    let show = paths::shell_path;
    let id8 = py::head(sid, 8);
    let first = match both.map(|(ours, theirs)| since_split(ours, theirs, agent)) {
        Some((0, ..)) => format!(
            "{} has a different session with the ID {id8}, with nothing in common with this one",
            show(target)
        ),
        Some((_, when, mine, other)) => {
            let mut first = format!(
                "Session {id8} went on in both {} and {} after {}",
                show(source),
                show(target),
                time_text(&when)
            );
            if mine > 0 || other > 0 {
                first += &format!(
                    ": {mine} new prompt{} in {}, {other} in {}",
                    if mine != 1 { "s" } else { "" },
                    show(source),
                    show(target)
                );
            }
            first
        }
        None => format!(
            "The copies of session {id8} in {} and {} each have files the other lacks",
            show(source),
            show(target)
        ),
    };
    let (doing, done) = if keep {
        ("Copying", "copied")
    } else {
        ("Moving", "moved")
    };
    format!(
        "{first}. {doing} this one over would lose what's only in {}, so nothing was {done}.\nTo keep both, bring this one in as a new session with its own ID:\n  {}\nOr delete the copy in {} first, to replace it:\n{}",
        show(target),
        fork_cmd,
        show(target),
        files.iter().map(|p| format!("  {}", show(p))).collect::<Vec<_>>().join("\n")
    )
}

/// Where a move goes, as PROFILE names it: a folder and its agent, or a bare NAME, which is
/// the session's own agent's profile of that name when it exists (~/.codex-NAME for a Codex
/// session, ~/.kimi-code-NAME for a Kimi one) and Claude Code's otherwise; claude:NAME,
/// codex:NAME and kimi:NAME spell it out.
enum Target {
    Fixed(Agent, String),
    Bare(String),
}

impl Target {
    fn parse(spec: &str) -> Target {
        let bare = !spec.contains(':')
            && !spec.contains('/')
            && !spec.contains(ospath::SEP)
            && !spec.starts_with('~')
            && spec != "default"
            && Agent::from_key(spec).is_none();
        if bare {
            Target::Bare(spec.to_string())
        } else {
            let (agent, path) = place(spec);
            Target::Fixed(agent, path)
        }
    }

    /// The folders it may mean: Claude Code's, and the profile of that name of each agent that
    /// has profiles, for a session of that agent.
    fn candidates(&self) -> Vec<(Agent, String)> {
        match self {
            Target::Fixed(agent, path) => vec![(*agent, path.clone())],
            Target::Bare(name) => Agent::ALL
                .iter()
                .filter(|a| a.has_profiles())
                .map(|a| (*a, paths::profile_folder(*a, name)))
                .collect(),
        }
    }

    /// What it means for a session of agent, or, when both the session's own agent's folder
    /// and Claude Code's exist, those two (own first), which has to be asked.
    fn resolve(&self, agent: Agent) -> Result<(Agent, String), (String, String)> {
        match self {
            Target::Fixed(a, p) => Ok((*a, p.clone())),
            Target::Bare(name) => {
                let claude = paths::profile_folder(Agent::Claude, name);
                if agent != Agent::Claude && agent.has_profiles() {
                    let own = paths::profile_folder(agent, name);
                    match (ospath::isdir(&own), ospath::isdir(&claude)) {
                        (true, true) => return Err((own, claude)),
                        (true, false) => return Ok((agent, own)),
                        _ => {}
                    }
                }
                Ok((Agent::Claude, claude))
            }
        }
    }
}

/// Which of two folders a bare NAME means, asked in a terminal: the session's own agent's,
/// a move, or Claude Code's, a conversion. Enter takes the first.
fn choose_target(
    agent: Agent,
    name: &str,
    own: &str,
    claude: &str,
    report: &mut Map<String, Value>,
) -> Result<(Agent, String), Stop> {
    let show = paths::shell_path;
    out::println(&format!(
        "{name} is both {} and {}:",
        show(own),
        show(claude)
    ));
    out::println(&format!(
        "  1. {}  (a move, as a {} session)",
        show(own),
        agent.name()
    ));
    out::println(&format!(
        "  2. {}  (a conversion, into a Claude Code session)",
        show(claude)
    ));
    report.insert("target_choice".into(), json!("asked"));
    loop {
        let Some(answer) = read_answer("Which one? [1] ") else {
            out::println("");
            return Err("Nothing changed.".into());
        };
        match py::strip(&answer) {
            "" | "1" => return Ok((agent, own.to_string())),
            "2" => return Ok((Agent::Claude, claude.to_string())),
            other
                if other == own
                    || other == show(own)
                    || other == format!("{}:{name}", agent.key()) =>
            {
                return Ok((agent, own.to_string()))
            }
            other
                if other == claude
                    || other == show(claude)
                    || other == format!("claude:{name}") =>
            {
                return Ok((Agent::Claude, claude.to_string()))
            }
            _ => out::println("Answer 1 or 2."),
        }
    }
}

/// Fill in what move or fork was given nothing for: the newest session started from this
/// folder, and the best account of its agent now, as hatcheck best picks it, shown with its
/// limits, which a terminal is asked to confirm; a dry run just says. A script, a pipe or
/// --json has to say both. One word alone that names a profile or home that exists, and starts
/// no session's ID, is where the newest session started here goes.
fn fill_in(a: &mut Ask, prog: &str) -> Result<(), Stop> {
    let show = paths::shell_path;
    let cwd = ospath::cwd();
    let mut about = String::new();
    if !a.session.is_empty() && a.profile.is_empty() {
        let word = a.session.clone();
        // What the word means for the session it would move, or for a Claude Code one when none
        // started here, so a bare NAME can be the session's own agent's home.
        let agent = crate::find::newest_here(&cwd, false).map_or(Agent::Claude, |s| s.agent);
        let names = match Target::parse(&word).resolve(agent) {
            Ok((_, folder)) => ospath::isdir(&folder).then_some(folder),
            Err((own, _)) => Some(own),
        };
        if let Some(folder) = names {
            if !crate::find::session_copies(&word).is_empty() {
                return Err(format!(
                    "{word} starts a session's ID and names the profile {}; give more of the ID, or say both: {prog} <session> {word}",
                    show(&folder)
                ));
            }
            a.profile = word;
            a.session.clear();
        }
    }
    // Where the session is: (agent, folder) of each copy, to notice a move to where it already is.
    let mut copies: Vec<(Agent, String)> = Vec::new();
    if a.session.is_empty() {
        let Some(s) = crate::find::newest_here(&cwd, false) else {
            return Err(format!(
                "No session was started from this folder. Give its ID: {prog} <session> <profile>"
            ));
        };
        about = format!(" \"{}\" from {}", truncate(&s.title, 60), show(&s.profile));
        copies.push((s.agent, s.profile.clone()));
        a.session = s.id.clone();
        a.source = Some(s.profile.clone());
    } else {
        for ((agent, _), places) in crate::find::session_copies(&a.session) {
            copies.extend(places.into_iter().map(|(folder, _)| (agent, folder)));
        }
    }
    let inferred_profile = a.profile.is_empty();
    if inferred_profile {
        // Where it goes: the best account of its agent now, as hatcheck best picks it, other
        // than its own, shown with its limits before the question or the dry run's report.
        let Some(&(agent, _)) = copies.first() else {
            return Ok(()); // no such session: the run says so
        };
        let agent = if agent.has_profiles() {
            agent
        } else {
            Agent::Claude
        };
        let me = prog.split_whitespace().next().unwrap_or("hatcheck");
        let now = times::now();
        let loaded = usage::load_usage(
            &paths::find_profiles(),
            false,
            &paths::find_codex_homes(),
            &paths::find_agy_roots(),
            &paths::find_kimi_homes(),
            &paths::find_zcode_roots(),
            &paths::find_dsh_homes(),
            &paths::find_mcode_homes(),
            &paths::find_qwen_homes(),
        );
        let accounts = crate::best::ranked_for(agent, &loaded, now);
        let mine: Vec<usize> = copies
            .iter()
            .flat_map(|(_, folder)| crate::best::holding(&accounts, folder))
            .collect();
        // The copy an account holds: its folder.
        let held_by = |k: usize| {
            copies
                .iter()
                .find(|(_, f)| {
                    accounts[k]
                        .2
                        .profiles
                        .iter()
                        .any(|p| paths::same_folder(p, f))
                })
                .map_or(accounts[k].2.profiles[0].clone(), |(_, f)| f.clone())
        };
        let id8 = py::head(&a.session, 8).to_string();
        let title = about.split(" from ").next().unwrap_or("").to_string();
        match crate::best::pick(&accounts, &mine) {
            crate::best::Pick::Own(k) => {
                return Err(format!(
                    "Session {id8}{title} is already in {}, the best {} account now. Say where it goes: {prog} {id8} <profile>",
                    show(&held_by(k)),
                    agent.name()
                ));
            }
            crate::best::Pick::Other(b) => {
                let shown = a.dry_run || interactive();
                if shown && !a.json {
                    if let Some(&k) = mine.first() {
                        out::println(&format!(
                            "Session {id8}{title} is in {}: {}.",
                            show(&held_by(k)),
                            crate::best::summary(&accounts[k].3, now, accounts[k].1)
                        ));
                    }
                    crate::best::print_account(
                        &out::Paint::new(),
                        &format!("Best {} account now: ", agent.name()),
                        &accounts[b],
                        now,
                    );
                }
                a.profile = accounts[b].2.profiles[0].clone();
                if shown && !about.is_empty() && !a.json {
                    // The session is the newest started from this folder: the folder's
                    // binding, when that's another profile, as the list flags it.
                    let bound = paths::bound_profile(Some(&cwd), agent, &paths::load_bindings());
                    if let Some(bound) = bound.filter(|w| !paths::same_folder(w, &a.profile)) {
                        out::warn(&format!("folder bound to {}", show(&bound)));
                    }
                }
            }
            crate::best::Pick::Nothing => {
                return Err(format!(
                    "No {}{} account has room now; {me} best says when one will. Say where it goes: {prog} {id8} <profile>",
                    if mine.is_empty() { "" } else { "other " },
                    agent.name()
                ));
            }
        }
    }
    if about.is_empty() && !inferred_profile {
        return Ok(());
    }
    // Nothing to do when its only copy is already there: say so rather than ask.
    let wanted = Target::parse(&a.profile);
    let settled = copies
        .first()
        .and_then(|(agent, _)| wanted.resolve(*agent).ok());
    let there = match &settled {
        Some((target_agent, target)) => copies
            .iter()
            .all(|(agent, folder)| agent == target_agent && paths::same_folder(folder, target)),
        None => false,
    };
    if there {
        let target = settled.map(|(_, t)| t).unwrap_or_default();
        return Err(format!(
            "Session {}{} is already in {}. Say where it goes: {prog} {} <profile>",
            py::head(&a.session, 8),
            about.split(" from ").next().unwrap_or(""),
            show(&target),
            py::head(&a.session, 8)
        ));
    }
    if a.dry_run {
        return Ok(());
    }
    if a.json || !interactive() {
        return Err(format!(
            "Say which session and where, when there's no terminal to ask: {prog} <session> <profile>"
        ));
    }
    let to = settled.map_or(a.profile.clone(), |(_, t)| show(&t));
    let question = format!(
        "{} session {}{about} to {to}? [y/N] ",
        if a.fork { "Fork" } else { "Move" },
        py::head(&a.session, 8)
    );
    let answer = read_answer(&question);
    if answer.is_none() {
        out::println("");
    }
    match answer
        .as_deref()
        .map(|x| py::strip(x).to_lowercase())
        .as_deref()
    {
        Some("y" | "yes") => Ok(()),
        _ => Err("Nothing changed.".into()),
    }
}

/// What move or fork was asked: the command line. A fork always keeps the original.
struct Ask {
    session: String,
    profile: String,
    source: Option<String>,
    keep: bool,
    fork: bool,
    dry_run: bool,
    json: bool,
}

/// What the first line says was done, or would be.
fn heading_of(a: &Ask) -> &'static str {
    match (a.fork, a.keep, a.dry_run) {
        (true, _, false) => "Forked",
        (true, _, true) => "Would fork",
        (false, false, false) => "Moved",
        (false, false, true) => "Would move",
        (false, true, false) => "Copied",
        (false, true, true) => "Would copy",
    }
}

fn set(report: &mut Map<String, Value>, entries: Vec<(&str, Value)>) {
    for (k, v) in entries {
        report.insert(k.to_string(), v);
    }
}

fn opt(v: Option<String>) -> Value {
    v.map_or(Value::Null, Value::String)
}

/// The stop for a Claude Code session open in a profile: the one it's taken from, or with
/// there, the one it would go to, whose copy would be written over while it's open.
fn open_refusal(profile: &str, sid: &str, there: bool) -> Option<Stop> {
    let live = sessions::open_sessions(&[profile.to_string()]);
    let entry = live.get(&(ospath::realpath(profile), sid.to_string()))?;
    let status = get(entry, "status");
    Some(format!(
        "Session {} is open in pid {}{}{}; quit it first, so it stops changing",
        py::head(sid, 8),
        py::str_of(get(entry, "pid")),
        if truthy(status) {
            format!(" ({})", py::str_of(status))
        } else {
            String::new()
        },
        where_it_would_go(profile, there)
    ))
}

/// How the copy at dst stands, from the standings worked out for each item.
fn state_of(states: &[(String, &'static str)], dst: &str) -> &'static str {
    states
        .iter()
        .find(|(d, _)| d == dst)
        .map_or("new", |(_, s)| *s)
}

/// Move (or with --keep, copy) a Claude Code session to another profile, or fork it in as a new
/// session: the session to resume and the directory to resume it from.
fn move_claude(
    a: &Ask,
    sid: &str,
    source: &str,
    transcripts: &[String],
    target: &str,
    fork_cmd: &str,
    report: &mut Map<String, Value>,
) -> Result<(String, Option<String>), Stop> {
    let show = paths::shell_path;
    let io = |e: std::io::Error| e.to_string();
    if let Some(stop) = open_refusal(source, sid, false) {
        return Err(stop);
    }
    // A fork gets a new ID; a move or a copy would write over the copy there.
    if let Some(stop) = (!a.fork).then(|| open_refusal(target, sid, true)).flatten() {
        return Err(stop);
    }
    let new_sid = if a.fork { uuid4() } else { sid.to_string() };
    let ids = a.fork.then(|| (sid.to_string(), new_sid.clone()));
    let ids = ids.as_ref();
    let mut items: Vec<Item> = Vec::new();
    let mut pairs: Vec<(String, String)> = Vec::new();
    for path in transcripts {
        let old = path[..path.len() - ".jsonl".len()].to_string();
        let new = ospath::join_all(
            target,
            &[
                "projects",
                &ospath::basename(&ospath::dirname(path)),
                &new_sid,
            ],
        );
        pairs.extend(path_pairs(&old, &new)); // covers <id>.jsonl and the <id>/ folder beside it
        items.push(Item {
            what: "transcript",
            src: path.clone(),
            dst: format!("{new}.jsonl"),
            deleted: true,
        });
        if ospath::isdir(&old) || ospath::isdir(&new) {
            items.push(Item {
                what: "subagents and tool output",
                src: old,
                dst: new,
                deleted: true,
            });
        }
    }
    for (sub, what) in [
        ("file-history", "/rewind file snapshots"),
        ("session-env", "session environment"),
    ] {
        let (old, new) = (
            ospath::join_all(source, &[sub, sid]),
            ospath::join_all(target, &[sub, &new_sid]),
        );
        if [&old, &new]
            .iter()
            .any(|d| ospath::isdir(d) && !files_in(d).is_empty())
        {
            pairs.extend(path_pairs(&old, &new));
            items.push(Item {
                what,
                src: old,
                dst: new,
                deleted: true,
            });
        }
    }
    for slug in plan_slugs(transcripts) {
        let (old, new) = (
            ospath::join_all(source, &["plans", &format!("{slug}.md")]),
            ospath::join_all(target, &["plans", &format!("{slug}.md")]),
        );
        if ospath::isfile(&old) {
            pairs.extend(path_pairs(&old, &new));
            items.push(Item {
                what: "plan",
                src: old,
                dst: new,
                deleted: false,
            }); // kept: sessions can share a plan
        }
    }
    let mut states: Vec<(String, &'static str)> = Vec::new();
    for item in &items {
        states.push((
            item.dst.clone(),
            item_standing(&item.src, &item.dst, &pairs).map_err(io)?,
        ));
    }
    if a.fork {
        // Its own new paths; but a plan already there is the copy there's too: left as it is.
        for item in items.iter().filter(|i| i.what == "plan") {
            if let Some(entry) = states.iter_mut().find(|(d, _)| *d == item.dst) {
                if !matches!(entry.1, "new" | "same") {
                    entry.1 = "kept";
                }
            }
        }
    }
    let verdict = overall(states.iter().map(|(_, s)| *s).filter(|s| *s != "kept"));
    let transcript = items
        .iter()
        .find(|i| i.what == "transcript")
        .map(|i| (i.src.clone(), i.dst.clone()))
        .unwrap_or_default();
    let theirs = if ospath::isfile(&transcript.1) {
        Some(read(&transcript.1).map_err(io)?)
    } else {
        None
    };
    if verdict == "diverged" {
        let split = state_of(&states, &transcript.1) == "diverged";
        let ours = if split {
            Some(copy_bytes(&transcript.0, &pairs, None).map_err(io)?)
        } else {
            None
        };
        let files: Vec<String> = items
            .iter()
            .filter(|i| {
                ospath::lexists(&i.dst)
                    && (i.what != "plan" || state_of(&states, &i.dst) == "diverged")
            })
            .map(|i| i.dst.clone())
            .collect();
        let both = match (&ours, &theirs) {
            (Some(o), Some(t)) => Some((o.as_slice(), t.as_slice())),
            _ => None,
        };
        return Err(went_on_in_both(
            Agent::Claude,
            sid,
            source,
            target,
            both,
            fork_cmd,
            &files,
            a.keep,
        ));
    }
    set(
        report,
        vec![
            ("agent", json!(Agent::Claude.key())),
            ("session", json!(sid)),
            ("source", json!(source)),
            ("target", json!(target)),
            ("fork", json!(a.fork)),
            (
                "new_session",
                if a.fork { json!(new_sid) } else { Value::Null },
            ),
        ],
    );
    let id8 = py::head(sid, 8);
    if verdict == "newer" {
        // It went on there: this copy has nothing the copy there lacks, and putting it there
        // would lose what came after, so stop.
        let info = read_info(std::slice::from_ref(&transcript.1));
        let cwd = info.as_ref().and_then(|i| i.cwd.clone());
        let title = info
            .as_ref()
            .map_or("?".to_string(), |i| truncate(&i.title, 60));
        let mut text = format!(
            "{} already has session {id8} \"{title}\", with everything in this copy and more: the session went on there. This copy is older, so nothing was {}.\nResume it there with:",
            show(target),
            if a.keep { "copied" } else { "moved" }
        );
        if let Some(c) = cwd.as_deref().filter(|c| !c.is_empty()) {
            text += &format!("\n  cd {}", show(c));
        }
        text += &format!("\n  {}", paths::resume_command(target, sid, false, None));
        let older: Vec<String> = items
            .iter()
            .filter(|i| i.deleted && ospath::lexists(&i.src))
            .map(|i| format!("  {}", show(&i.src)))
            .collect();
        if !older.is_empty() {
            text += &format!(
                "\nTo drop the older copy in {}, delete:\n{}",
                show(source),
                older.join("\n")
            );
        }
        return Err(text);
    }
    let mut repointed = false;
    'files: for item in &items {
        for n in files_in(&item.src) {
            let f = join(&item.src, &n);
            if f.ends_with(".jsonl")
                && read(&f).map_err(io)? != copy_bytes(&f, &pairs, ids).map_err(io)?
            {
                repointed = true;
                break 'files;
            }
        }
    }
    let details: Vec<String> = items
        .iter()
        .map(|i| {
            if i.what == "plan" {
                ospath::basename(&i.src)
            } else if ospath::isfile(&i.src) {
                size_text(fs::metadata(&i.src).map(|m| m.len()).unwrap_or(0))
            } else {
                let n = files_in(&i.src).len();
                format!("{n} file{}", if n != 1 { "s" } else { "" })
            }
        })
        .collect();
    let info = read_info(transcripts);
    let older_since = if state_of(&states, &transcript.1) == "older" {
        theirs.as_deref().map(last_time)
    } else {
        None
    };
    let had_it = a.fork
        && transcripts.iter().any(|t| {
            ospath::isfile(&ospath::join_all(
                target,
                &[
                    "projects",
                    &ospath::basename(&ospath::dirname(t)),
                    &format!("{sid}.jsonl"),
                ],
            ))
        });
    if !a.dry_run {
        for item in &items {
            match state_of(&states, &item.dst) {
                "new" => copy(&item.src, &item.dst, &pairs, ids).map_err(io)?,
                "older" => bring_up_to_date(&item.src, &item.dst, &pairs).map_err(io)?,
                _ => {}
            }
        }
        let wrong: Vec<&String> = items
            .iter()
            .filter(|i| {
                state_of(&states, &i.dst) != "kept" && !matches(&i.src, &i.dst, &pairs, ids)
            })
            .map(|i| &i.dst)
            .collect();
        if !wrong.is_empty() {
            return Err(format!(
                "These copies don't match their originals, so nothing was deleted:\n{}",
                wrong
                    .iter()
                    .map(|w| format!("  {}", show(w)))
                    .collect::<Vec<_>>()
                    .join("\n")
            ));
        }
        if !a.keep {
            delete_originals(&items).map_err(io)?;
        }
    }
    let any_older = states.iter().any(|(_, s)| *s == "older");
    let action = if a.fork {
        "fork"
    } else if any_older {
        "update"
    } else if a.keep {
        "copy"
    } else {
        "move"
    };
    let cwd = info.as_ref().and_then(|i| i.cwd.clone());
    set(report, vec![
        ("action", json!(action)),
        ("title", info.as_ref().map_or(Value::Null, |i| json!(i.title))),
        ("deleted_original", json!(!a.keep)),
        ("repointed", json!(repointed)),
        ("items", Value::Array(items.iter().zip(&details).map(|(i, d)| json!({"what": i.what, "from": i.src, "to": i.dst, "state": state_of(&states, &i.dst), "detail": d})).collect())),
        ("cwd", opt(cwd.clone())),
        ("resume", json!(paths::resume_command(target, &new_sid, false, None))),
    ]);
    let heading = heading_of(a);
    let title = info
        .as_ref()
        .map_or("(no conversation)".to_string(), |i| truncate(&i.title, 60));
    let text = format!(
        "{heading} session {id8} \"{title}\" from {} to {}{}:",
        show(source),
        show(target),
        if a.fork { " as a new session" } else { "" }
    );
    if a.dry_run {
        out::preview(&text);
    } else {
        out::success(&text);
    }
    let paint = out::Paint::new();
    for (item, detail) in items.iter().zip(&details) {
        let note = match state_of(&states, &item.dst) {
            "same" => "  (already there)".to_string(),
            "older" => format!(
                "  ({} an older copy)",
                if a.dry_run { "would update" } else { "updated" }
            ),
            "kept" => "  (the one there stays: the copy there uses it too)".to_string(),
            _ => String::new(),
        };
        out::println(&format!(
            "  {} {detail}{note}",
            paint.muted(&format!("{:<26}", item.what))
        ));
    }
    if a.fork {
        out::println(&format!(
            "  {} {new_sid}",
            paint.muted(&format!("{:<26}", "new session"))
        ));
    }
    if repointed {
        out::println(&format!(
            "Paths in the conversation to these files {} to the copies.",
            if a.dry_run {
                "would point"
            } else {
                "now point"
            }
        ));
    }
    if any_older {
        let since = older_since.filter(truthy).map_or(String::new(), |t| {
            format!(", as it was on {}", time_text(&t))
        });
        out::println(&format!(
            "{} had an older copy of this session{since}; it {} up to date.",
            show(target),
            if a.dry_run {
                "would be brought"
            } else {
                "is now"
            }
        ));
    }
    if had_it {
        out::println(&format!(
            "The copy of session {id8} that was already in {} stays as it was.",
            show(target)
        ));
    }
    let accounts: Vec<Value> = [source, target]
        .iter()
        .map(|p| get(&usage::read_config(p).0, "accountUuid").clone())
        .collect();
    if py::same(&accounts[0], &accounts[1]) && !accounts[0].is_null() {
        out::println(
            "Both profiles are signed in to the same account, so its plan limits stay the same.",
        );
    }
    if !a.keep {
        let plan = if items.iter().any(|i| i.what == "plan") {
            "; the plan file stays, since sessions can share one"
        } else {
            ""
        };
        out::println(&format!(
            "{} the original from {}{plan}.",
            if a.dry_run { "Would delete" } else { "Deleted" },
            show(source)
        ));
    } else {
        out::println(&format!("The original stays in {}.", show(source)));
    }
    out::heading(if a.dry_run {
        "Then resume it with:"
    } else {
        "Resume it with:"
    });
    if let Some(c) = cwd.as_deref().filter(|c| !c.is_empty()) {
        out::command(&format!("  cd {}", show(c)));
    }
    out::command(&format!(
        "  {}",
        paths::resume_command(target, &new_sid, false, None)
    ));
    if let Some(info) = &info {
        if let Some(provider) = paths::provider_of(s(&info.model)) {
            out::println(&format!(
                "Its last reply came from {}; to carry on there rather than on this profile's Claude plan:",
                py::str_of(&info.model)
            ));
            out::command(&format!(
                "  {}",
                paths::resume_command(target, &new_sid, false, Some(provider))
            ));
        }
    }
    Ok((new_sid, cwd))
}

/// [(session ID, file)] for the Codex sessions in a home whose ID starts with prefix,
/// archived ones too.
fn codex_files(home: &str, prefix: &str) -> Vec<(String, String)> {
    let mut files = ospath::glob(
        home,
        &[
            Part::Name("sessions"),
            Part::Wild("", ""),
            Part::Wild("", ""),
            Part::Wild("", ""),
            Part::Wild("rollout-", ".jsonl"),
        ],
    );
    files.extend(ospath::glob(
        home,
        &[
            Part::Name("archived_sessions"),
            Part::Wild("rollout-", ".jsonl"),
        ],
    ));
    files.sort();
    let lower = prefix.to_lowercase();
    files
        .into_iter()
        .filter_map(|p| {
            paths::codex_file_id(&p)
                .filter(|id| id.starts_with(&lower))
                .map(|id| (id.to_string(), p.clone()))
        })
        .collect()
}

/// The ChatGPT account and user a Codex home is signed in to, or None.
fn codex_account(home: &str) -> Option<(String, String)> {
    let login = usage::read_codex_login(home);
    truthy(get(&login, "account_id")).then(|| {
        (
            py::repr(get(&login, "account_id")),
            py::repr(get(&login, "user_id")),
        )
    })
}

/// Move (or with --keep, copy) a Codex session to another home on the same account, or fork it
/// in as a new session.
fn copy_codex(
    a: &Ask,
    sid: &str,
    source: &str,
    files: &[String],
    target: &str,
    fork_cmd: &str,
    report: &mut Map<String, Value>,
) -> Result<(String, Option<String>), Stop> {
    let show = paths::shell_path;
    let io = |e: std::io::Error| e.to_string();
    let src = newest(files); // an archived copy and a live one: the newer
    let name = facts::codex_names(source).get(sid).cloned().filter(truthy);
    let mut names = Map::new();
    if let Some(n) = &name {
        names.insert(sid.to_string(), n.clone());
    }
    let info = facts::read_codex_session(&src, &names).ok().flatten();
    facts::save_cache();
    let mut theirs: Vec<String> = codex_files(target, sid)
        .into_iter()
        .filter(|(s_, _)| s_ == sid)
        .map(|(_, p)| p)
        .collect();
    theirs.sort_by(|x, y| {
        ospath::getmtime(x)
            .unwrap_or(0.0)
            .partial_cmp(&ospath::getmtime(y).unwrap_or(0.0))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let (new_sid, dst) = if a.fork {
        let now = times::now();
        let new_sid = uuid7(now);
        let dst = codex_file_path(target, now, &new_sid);
        (new_sid, dst)
    } else {
        // the copy already there, wherever it is, archived or not
        (
            sid.to_string(),
            theirs
                .last()
                .cloned()
                .unwrap_or_else(|| ospath::join(target, &ospath::relpath(&src, source))),
        )
    };
    let ids = a.fork.then(|| (sid.to_string(), new_sid.clone()));
    let ids = ids.as_ref();
    let id8 = py::head(sid, 8);
    // Both copies held still while one is written over the other: a fork's is a new ID.
    let lock = (
        lock_or_stop(source, sid, false)?,
        if a.fork {
            None
        } else {
            Some(lock_or_stop(target, sid, true)?)
        },
    );
    let title = info.as_ref().map(|i| i.title.clone());
    let cwd = info.as_ref().and_then(|i| i.cwd.clone());
    let state = item_standing(&src, &dst, &[]).map_err(io)?;
    if state == "diverged" {
        let (o, t) = (read(&src).map_err(io)?, read(&dst).map_err(io)?);
        return Err(went_on_in_both(
            Agent::Codex,
            sid,
            source,
            target,
            Some((&o, &t)),
            fork_cmd,
            std::slice::from_ref(&dst),
            a.keep,
        ));
    }
    set(
        report,
        vec![
            ("agent", json!(Agent::Codex.key())),
            ("session", json!(sid)),
            ("source", json!(source)),
            ("target", json!(target)),
            ("fork", json!(a.fork)),
            (
                "new_session",
                if a.fork { json!(new_sid) } else { Value::Null },
            ),
            ("title", opt(title.clone())),
            ("cwd", opt(cwd.clone())),
        ],
    );
    let title_text = truncate(
        title.as_deref().filter(|t| !t.is_empty()).unwrap_or("?"),
        60,
    );
    if state == "newer" {
        // It went on there: putting this older copy there would lose what came after, so stop.
        drop(lock);
        let mut text = format!(
            "{} already has Codex session {id8} \"{title_text}\", with everything in this copy and more: the session went on there. This copy is older, so nothing was {}.\nResume it there with:",
            show(target),
            if a.keep { "copied" } else { "moved" }
        );
        if let Some(c) = cwd.as_deref().filter(|c| !c.is_empty()) {
            text += &format!("\n  cd {}", show(c));
        }
        text += &format!("\n  {}", paths::codex_resume_command(target, sid, false));
        text += &format!(
            "\nTo drop the older copy in {}, delete:\n  {}",
            show(source),
            show(&src)
        );
        return Err(text);
    }
    let older_since = if state == "older" {
        Some(last_time(&read(&dst).map_err(io)?))
    } else {
        None
    };
    let size = size_text(fs::metadata(&src).map(|m| m.len()).unwrap_or(0));
    let action = if a.fork {
        "fork"
    } else if state == "older" {
        "update"
    } else if a.keep {
        "copy"
    } else {
        "move"
    };
    set(
        report,
        vec![
            ("action", json!(action)),
            ("deleted_original", json!(!a.keep)),
            (
                "items",
                json!([{"what": "session file", "from": src, "to": dst, "state": state, "detail": size}]),
            ),
            (
                "resume",
                json!(paths::codex_resume_command(target, &new_sid, false)),
            ),
        ],
    );
    if !a.dry_run {
        if matches!(state, "new" | "older") {
            copy(&src, &dst, &[], ids).map_err(io)?;
        }
        if read(&dst).map_err(io)? != copy_bytes(&src, &[], ids).map_err(io)? {
            return Err(format!(
                "The copy doesn't match the original, so nothing was deleted:\n  {}",
                show(&dst)
            ));
        }
        if let Some(n) = &name {
            if facts::codex_names(target).get(&new_sid) != Some(n) {
                name_session(target, &new_sid, &py::str_of(n)).map_err(io)?;
            }
        }
        if !a.keep {
            fs::remove_file(&src).map_err(io)?;
        }
    }
    drop(lock);
    let heading = heading_of(a);
    let text = format!(
        "{heading} Codex session {id8} \"{title_text}\" from {} to {}{}:",
        show(source),
        show(target),
        if a.fork { " as a new session" } else { "" }
    );
    if a.dry_run {
        out::preview(&text);
    } else {
        out::success(&text);
    }
    let paint = out::Paint::new();
    let note = match state {
        "same" => "  (already there)".to_string(),
        "older" => format!(
            "  ({} an older copy)",
            if a.dry_run { "would update" } else { "updated" }
        ),
        _ => String::new(),
    };
    out::println(&format!(
        "  {} {size}{note}",
        paint.muted(&format!("{:<26}", "session file"))
    ));
    if let Some(n) = &name {
        out::println(&format!(
            "  {} {}",
            paint.muted(&format!("{:<26}", "name")),
            py::str_of(n)
        ));
    }
    if a.fork {
        out::println(&format!(
            "  {} {new_sid}",
            paint.muted(&format!("{:<26}", "new session"))
        ));
    }
    if state == "older" {
        let since = older_since.filter(truthy).map_or(String::new(), |t| {
            format!(", as it was on {}", time_text(&t))
        });
        out::println(&format!(
            "{} had an older copy of this session{since}; it {} up to date.",
            show(target),
            if a.dry_run {
                "would be brought"
            } else {
                "is now"
            }
        ));
    }
    if a.fork && !theirs.is_empty() {
        out::println(&format!(
            "The copy of session {id8} that was already in {} stays as it was.",
            show(target)
        ));
    }
    out::println(
        "Both homes are signed in to the same ChatGPT account, so its plan limits stay the same.",
    );
    if !a.keep {
        out::println(&format!(
            "{} the original from {}.",
            if a.dry_run { "Would delete" } else { "Deleted" },
            show(source)
        ));
    } else {
        out::println(&format!("The original stays in {}.", show(source)));
    }
    out::heading(if a.dry_run {
        "Then resume it with:"
    } else {
        "Resume it with:"
    });
    if let Some(c) = cwd.as_deref().filter(|c| !c.is_empty()) {
        out::command(&format!("  cd {}", show(c)));
    }
    out::command(&format!(
        "  {}",
        paths::codex_resume_command(target, &new_sid, false)
    ));
    Ok((new_sid, cwd))
}

/// The time of a Kimi wire's last event, as text for a turn (events carry ms since the epoch).
fn last_kimi_time(data: &[u8]) -> Value {
    let lines = lines_of(data);
    records_of(&lines)
        .filter_map(|r| py::Num::of(get(&r, "time")).map(py::Num::f))
        .last()
        .and_then(|ms| times::iso_utc(ms / 1000.0))
        .map_or(Value::Null, Value::String)
}

/// Move (or with --keep, copy) a Kimi session to another home, or fork it in as a new session:
/// the session folder is copied whole, its state.json made to point into the copy (and given
/// the new ID for a fork), and the target's index given a line for it. Nothing in the folder is
/// signed or encrypted, so the copy runs on any account. A copy already there is compared by
/// its agents/ folder, the wires and file snapshots; state.json changes with every use.
fn copy_kimi(
    a: &Ask,
    sid: &str,
    source: &str,
    files: &[String],
    target: &str,
    fork_cmd: &str,
    report: &mut Map<String, Value>,
) -> Result<(String, Option<String>), Stop> {
    let show = paths::shell_path;
    let io = |e: std::io::Error| e.to_string();
    let id8 = py::head(sid, 8);
    let src = newest(files); // the session folder
    let state: Value = fs::read_to_string(join(&src, "state.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .filter(Value::is_object)
        .ok_or_else(|| format!("Kimi session {id8} has no state.json hatcheck can read"))?;
    let info = kimi::summarize(source, &src);
    facts::save_cache();
    let title = info.as_ref().map(|i| i.title.clone());
    let cwd = info.as_ref().and_then(|i| i.cwd.clone());
    let new_sid = if a.fork { uuid4() } else { sid.to_string() };
    // Under the same working-directory key as the original: kimi's own, whatever it hashed.
    let wd = ospath::basename(&ospath::dirname(src.trim_end_matches(ospath::is_sep)));
    let dst = ospath::join_all(target, &["sessions", &wd, &Agent::Kimi.full_id(&new_sid)]);
    let (ours, theirs) = (join(&src, "agents"), join(&dst, "agents"));
    let had_it = ospath::isdir(&dst);
    let state_of_copy = item_standing(&ours, &theirs, &[]).map_err(io)?;
    if state_of_copy == "diverged" {
        let both = match (read(&kimi::wire_path(&src)), read(&kimi::wire_path(&dst))) {
            (Ok(o), Ok(t)) => Some((o, t)),
            _ => None,
        };
        return Err(went_on_in_both(
            Agent::Kimi,
            sid,
            source,
            target,
            both.as_ref().map(|(o, t)| (o.as_slice(), t.as_slice())),
            fork_cmd,
            std::slice::from_ref(&dst),
            a.keep,
        ));
    }
    set(
        report,
        vec![
            ("agent", json!(Agent::Kimi.key())),
            ("session", json!(sid)),
            ("source", json!(source)),
            ("target", json!(target)),
            ("fork", json!(a.fork)),
            (
                "new_session",
                if a.fork { json!(new_sid) } else { Value::Null },
            ),
            ("title", opt(title.clone())),
            ("cwd", opt(cwd.clone())),
        ],
    );
    let title_text = truncate(
        title.as_deref().filter(|t| !t.is_empty()).unwrap_or("?"),
        60,
    );
    if state_of_copy == "newer" {
        let mut text = format!(
            "{} already has Kimi session {id8} \"{title_text}\", with everything in this copy and more: the session went on there. This copy is older, so nothing was {}.\nResume it there with:",
            show(target),
            if a.keep { "copied" } else { "moved" }
        );
        if let Some(c) = cwd.as_deref().filter(|c| !c.is_empty()) {
            text += &format!("\n  cd {}", show(c));
        }
        text += &format!("\n  {}", paths::kimi_resume_command(target, sid, false));
        text += &format!(
            "\nTo drop the older copy in {}, delete:\n  {}",
            show(source),
            show(&src)
        );
        return Err(text);
    }
    let older_since = if state_of_copy == "older" {
        read(&kimi::wire_path(&dst))
            .ok()
            .map(|d| last_kimi_time(&d))
    } else {
        None
    };
    let n = files_in(&src).len();
    let detail = format!("{n} file{}", if n != 1 { "s" } else { "" });
    let action = if a.fork {
        "fork"
    } else if state_of_copy == "older" {
        "update"
    } else if a.keep {
        "copy"
    } else {
        "move"
    };
    set(
        report,
        vec![
            ("action", json!(action)),
            ("deleted_original", json!(!a.keep)),
            (
                "items",
                json!([{"what": "session folder", "from": src, "to": dst, "state": state_of_copy, "detail": detail}]),
            ),
            (
                "resume",
                json!(paths::kimi_resume_command(target, &new_sid, false)),
            ),
        ],
    );
    let new_state = kimi::restate(&state, &src, &dst, (sid, &new_sid));
    if !a.dry_run {
        match state_of_copy {
            "new" => copy(&src, &dst, &[], None).map_err(io)?,
            "older" => bring_up_to_date(&src, &dst, &[]).map_err(io)?,
            _ => {}
        }
        fs::write(join(&dst, "state.json"), py::dumps_tight(&new_state)).map_err(io)?;
        // Every file of the original is in the copy as it was, and state.json as made.
        let mut wrong: Vec<String> = Vec::new();
        for name in files_in(&src) {
            let (from, to) = (join(&src, &name), join(&dst, &name));
            let want = if name == "state.json" {
                py::dumps_tight(&new_state).into_bytes()
            } else {
                read(&from).map_err(io)?
            };
            if read(&to).ok() != Some(want) {
                wrong.push(to);
            }
        }
        if !wrong.is_empty() {
            return Err(format!(
                "These copies don't match their originals, so nothing was deleted:\n{}",
                wrong
                    .iter()
                    .map(|w| format!("  {}", show(w)))
                    .collect::<Vec<_>>()
                    .join("\n")
            ));
        }
        // kimi resumes it through its line there, so that's checked too before anything goes.
        kimi::index_put(target, &new_sid, &dst, cwd.as_deref().unwrap_or("")).map_err(io)?;
        if kimi::indexed_at(target, &new_sid).as_deref() != Some(dst.as_str()) {
            return Err(format!(
                "The index in {} doesn't list the copy, so nothing was deleted",
                show(target)
            ));
        }
        if !a.keep {
            fs::remove_dir_all(&src).map_err(io)?;
            kimi::index_remove(source, sid).map_err(io)?;
        }
    }
    let heading = heading_of(a);
    let text = format!(
        "{heading} Kimi session {id8} \"{title_text}\" from {} to {}{}:",
        show(source),
        show(target),
        if a.fork { " as a new session" } else { "" }
    );
    if a.dry_run {
        out::preview(&text);
    } else {
        out::success(&text);
    }
    let paint = out::Paint::new();
    let note = match state_of_copy {
        "same" => "  (already there)".to_string(),
        "older" => format!(
            "  ({} an older copy)",
            if a.dry_run { "would update" } else { "updated" }
        ),
        _ => String::new(),
    };
    out::println(&format!(
        "  {} {detail}{note}",
        paint.muted(&format!("{:<26}", "session folder"))
    ));
    if a.fork {
        out::println(&format!(
            "  {} {new_sid}",
            paint.muted(&format!("{:<26}", "new session"))
        ));
    }
    out::println(&format!(
        "Its state.json {} into {}, whose index {} it.",
        if a.dry_run { "would point" } else { "points" },
        show(target),
        if a.dry_run { "would list" } else { "lists" }
    ));
    if state_of_copy == "older" {
        let since = older_since.filter(truthy).map_or(String::new(), |t| {
            format!(", as it was on {}", time_text(&t))
        });
        out::println(&format!(
            "{} had an older copy of this session{since}; it {} up to date.",
            show(target),
            if a.dry_run {
                "would be brought"
            } else {
                "is now"
            }
        ));
    }
    if a.fork && had_it {
        out::println(&format!(
            "The copy of session {id8} that was already in {} stays as it was.",
            show(target)
        ));
    }
    if !a.keep {
        out::println(&format!(
            "{} the original from {}, and its line in the index there.",
            if a.dry_run { "Would delete" } else { "Deleted" },
            show(source)
        ));
    } else {
        out::println(&format!("The original stays in {}.", show(source)));
    }
    out::heading(if a.dry_run {
        "Then resume it with:"
    } else {
        "Resume it with:"
    });
    if let Some(c) = cwd.as_deref().filter(|c| !c.is_empty()) {
        out::command(&format!("  cd {}", show(c)));
    }
    out::command(&format!(
        "  {}",
        paths::kimi_resume_command(target, &new_sid, false)
    ));
    Ok((new_sid, cwd))
}

/// Convert a session for the other agent, or for a Codex home on another account, into a new
/// session in target.
#[allow(clippy::too_many_arguments)]
fn convert(
    a: &Ask,
    agent: Agent,
    sid: &str,
    source: &str,
    files: &[String],
    target_agent: Agent,
    target: &str,
    why: Option<&str>,
    report: &mut Map<String, Value>,
) -> Result<(Option<String>, Option<String>), Stop> {
    let show = paths::shell_path;
    let io = |e: std::io::Error| e.to_string();
    let path = newest(files);
    let id8 = py::head(sid, 8);
    let conv = match agent {
        Agent::Codex => {
            let lock = lock_or_stop(source, sid, false)?;
            let conv = conv::read_codex(&path, source).map_err(io)?;
            drop(lock);
            conv
        }
        Agent::Claude => conv::read_claude(&path).map_err(io)?,
        Agent::Agy => {
            if agy::session_open(source, sid) {
                return Err(format!(
                    "Antigravity session {id8} is open in {}; quit it first, so it stops changing",
                    agy::holder(source)
                ));
            }
            agy::read_conversation(source, sid).map_err(|e| {
                format!(
                    "agy wrote no transcript hatcheck can read for session {id8} ({}): {e}",
                    show(&agy::transcript_path(source, sid))
                )
            })?
        }
        Agent::Kimi => kimi::read_conversation(source, sid).map_err(|e| {
            format!(
                "kimi's session {id8} in {} can't be read: {e}",
                show(source)
            )
        })?,
        Agent::Zcode => zcode::read_conversation(source, sid).map_err(|e| {
            format!(
                "zcode's session {id8} in {} can't be read: {e}",
                show(source)
            )
        })?,
        Agent::Dsh => {
            if dsh::session_open(&path) {
                return Err(format!(
                    "DeepSeek Harness session {id8} is open in dsh; close it first, so it stops changing"
                ));
            }
            dsh::read_conversation(source, sid).map_err(|e| {
                format!("dsh's session {id8} in {} can't be read: {e}", show(source))
            })?
        }
        Agent::Mcode => {
            if crate::mcode::session_open(source, sid) {
                return Err(format!(
                    "MiniMax Code session {id8} is open or has an active writer; close it or let that work finish first"
                ));
            }
            crate::mcode::read_conversation(source, sid).map_err(|e| {
                format!(
                    "mcode's session {id8} in {} can't be read: {e}",
                    show(source)
                )
            })?
        }
        Agent::Qwen => {
            if crate::qwen::session_open(source, &path, sid) {
                return Err(format!(
                    "Qwen Code session {id8} is open in qwen; quit it first, so it stops changing"
                ));
            }
            crate::qwen::read_conversation(source, sid).map_err(|e| {
                format!(
                    "qwen's session {id8} in {} can't be read: {e}",
                    show(source)
                )
            })?
        }
    };
    if conv.turns.is_empty() {
        return Err(format!(
            "{} session {id8} holds no conversation to convert",
            agent.name()
        ));
    }
    let (turns, dropped, newest_cut) = fit(&conv);
    let cwd = conv
        .cwd
        .clone()
        .filter(|c| !c.is_empty())
        .unwrap_or_else(paths::home);
    // What was written: a new session's ID and file; for agy and zcode, an ID for the file the
    // conversation was written to, which the agent starts a new conversation of its own on.
    let to_file = matches!(target_agent, Agent::Agy | Agent::Zcode);
    let mut written: Option<(String, String)> = None;
    if !a.dry_run {
        written = Some(
            match target_agent {
                Agent::Codex => write_codex(&conv, &turns, target, &cwd),
                Agent::Claude => write_claude(&conv, &turns, target, &cwd),
                Agent::Kimi => kimi::write_session(&conv, &turns, target, &cwd),
                Agent::Agy | Agent::Zcode => {
                    let id = uuid4();
                    conv::write_import(&conv, &turns, &id).map(|path| (id, path))
                }
                Agent::Dsh => {
                    return Err(
                        "DeepSeek Harness's session store isn't one hatcheck can write; choose another agent as the target".into(),
                    )
                }
                Agent::Mcode => {
                    return Err(
                        "MiniMax Code's session store isn't one hatcheck can write; choose another agent as the target".into(),
                    )
                }
                Agent::Qwen => {
                    return Err(
                        "Qwen Code's session store isn't one hatcheck can write; choose another agent as the target".into(),
                    )
                }
            }
            .map_err(io)?,
        );
    }
    let new_sid = written.as_ref().filter(|_| !to_file).map(|w| w.0.clone());
    let prompts = conv.turns.iter().filter(|t| t.0 == "user").count();
    let shown_id = new_sid.clone().unwrap_or_else(|| "<new id>".into());
    let file = written
        .as_ref()
        .map_or_else(|| conv::import_path("<new id>"), |w| w.1.clone());
    let resume = match target_agent {
        Agent::Codex => paths::codex_resume_command(target, &shown_id, false),
        Agent::Claude => paths::resume_command(target, &shown_id, false, None),
        Agent::Kimi => paths::kimi_resume_command(target, &shown_id, false),
        Agent::Agy => agy::start_command(target, &file, false),
        Agent::Zcode => zcode::start_command(target, &file, false),
        Agent::Dsh => {
            return Err(
                "DeepSeek Harness's session store isn't one hatcheck can write; choose another agent as the target".into(),
            )
        }
        Agent::Mcode => {
            return Err(
                "MiniMax Code's session store isn't one hatcheck can write; choose another agent as the target".into(),
            )
        }
        Agent::Qwen => {
            return Err(
                "Qwen Code's session store isn't one hatcheck can write; choose another agent as the target".into(),
            )
        }
    };
    set(
        report,
        vec![
            ("action", json!("convert")),
            ("agent", json!(agent.key())),
            ("session", json!(sid)),
            ("source", json!(source)),
            ("target", json!(target)),
            ("target_agent", json!(target_agent.key())),
            ("new_session", opt(new_sid.clone())),
            ("file", if to_file { json!(file) } else { Value::Null }),
            ("title", opt(conv.title.clone())),
            ("why", why.map_or(Value::Null, |w| json!(w))),
            ("cwd", json!(cwd)),
            ("prompts", json!(prompts)),
            ("tool_calls", json!(conv.calls)),
            ("left_out", json!(conv.left_out)),
            ("turns_dropped", json!(dropped)),
            ("newest_turn_cut", json!(newest_cut)),
            ("resume", json!(resume)),
        ],
    );
    let into = match target_agent {
        Agent::Agy | Agent::Zcode => format!(
            "into a conversation for {} in {}",
            target_agent.name(),
            show(&conv::imports_dir())
        ),
        other => format!("into a {} session in {}", other.name(), show(target)),
    };
    let text = format!(
        "{} {} session {id8} \"{}\" from {} {into}:",
        if a.dry_run {
            "Would convert"
        } else {
            "Converted"
        },
        agent.name(),
        truncate(
            conv.title
                .as_deref()
                .filter(|t| !t.is_empty())
                .unwrap_or("?"),
            60
        ),
        show(source)
    );
    if a.dry_run {
        out::preview(&text);
    } else {
        out::success(&text);
    }
    let paint = out::Paint::new();
    if let Some(w) = why {
        out::println(&format!("  ({w})"));
    }
    out::println(&format!(
        "  {} {prompts} of your messages, with the replies",
        paint.muted(&format!("{:<26}", "conversation"))
    ));
    out::println(&format!(
        "  {} {}, as text with their output shortened",
        paint.muted(&format!("{:<26}", "tool calls")),
        conv.calls
    ));
    out::println(&format!(
        "  {} {}",
        paint.muted(&format!("{:<26}", "left out")),
        conv.left_out
    ));
    if dropped > 0 {
        out::println(&format!(
            "  {} the oldest {dropped} turns after the first are left out{}, to fit the context window",
            paint.muted(&format!("{:<26}", "too long")),
            if newest_cut {
                ", and the start of the newest"
            } else {
                ""
            }
        ));
    }
    if let Some((sid_new, path_new)) = &written {
        let size = if target_agent == Agent::Kimi {
            // a folder: its wire is the conversation
            size_text(
                fs::metadata(kimi::wire_path(path_new))
                    .map(|m| m.len())
                    .unwrap_or(0),
            )
        } else {
            size_text(fs::metadata(path_new).map(|m| m.len()).unwrap_or(0))
        };
        if to_file {
            out::println(&format!(
                "  {} {} ({size})",
                paint.muted(&format!("{:<26}", "written to")),
                show(path_new)
            ));
        } else {
            out::println(&format!(
                "  {} {sid_new} ({size})",
                paint.muted(&format!("{:<26}", "new session"))
            ));
        }
    }
    match target_agent {
        Agent::Agy => out::println("Antigravity CLI keeps its conversations in a form hatcheck can't write, so the session starts when agy reads the file."),
        Agent::Zcode => out::println("ZCode keeps its sessions in a database hatcheck doesn't write, so the session starts when zcode reads the file."),
        _ => {}
    }
    out::println(&format!(
        "The original stays in {}, since a conversion leaves things out.",
        show(source)
    ));
    out::heading(match (a.dry_run, to_file) {
        (true, false) => "Then resume it with:",
        (false, false) => "Resume it with:",
        (true, true) => "Then start it with:",
        (false, true) => "Start it with:",
    });
    out::command(&format!("  cd {}", show(&cwd)));
    out::command(&format!("  {resume}"));
    if target_agent == Agent::Zcode {
        out::println("Once it has answered, carry on in the terminal UI with:");
        out::command(&format!("  {}", paths::zcode_command(target, "-c", false)));
    }
    // What to start, when asked to: the new session, or for agy and zcode the file.
    let handle = if to_file {
        written.as_ref().map(|w| w.1.clone())
    } else {
        new_sid
    };
    Ok((handle, Some(cwd)))
}

/// The fork command for what was asked, which a move that stops prints: hatcheck fork (or hat
/// fork) with the same session, profile, --from and --dry-run. The values are quoted for the
/// shell the command is printed for; the flags stay bare.
fn fork_command(prog: &str, a: &Ask) -> String {
    let me = prog.rsplit_once(' ').map_or(prog, |(me, _)| me);
    let mut words = vec![
        format!("{me} fork"),
        paths::quote(&a.session),
        paths::quote(&a.profile),
    ];
    if let Some(source) = &a.source {
        words.push("--from".into());
        words.push(paths::quote(source));
    }
    if a.dry_run {
        words.push("--dry-run".into());
    }
    words.join(" ")
}

/// (agent, session ID) -> [(profile or home, [files])] for the sessions in sources whose
/// ID starts with prefix, in the order found.
fn find_sessions(sources: &[(Agent, String)], prefix: &str) -> Vec<((Agent, String), Places)> {
    let mut found: Vec<((Agent, String), Places)> = Vec::new();
    let mut add = |key: (Agent, String), folder: &str, path: String| {
        let entry = match found.iter_mut().position(|(k, _)| *k == key) {
            Some(k) => &mut found[k].1,
            None => {
                found.push((key, Vec::new()));
                &mut found.last_mut().unwrap().1
            }
        };
        match entry.iter_mut().find(|(f, _)| f == folder) {
            Some((_, files)) => files.push(path),
            None => entry.push((folder.to_string(), vec![path])),
        }
    };
    for (agent, folder) in sources {
        match agent {
            Agent::Claude => {
                let mut files = ospath::glob(
                    folder,
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
                        folder,
                        path,
                    );
                }
            }
            Agent::Codex => {
                for (sid, path) in codex_files(folder, prefix) {
                    add((Agent::Codex, sid), folder, path);
                }
            }
            Agent::Agy => {
                for row in agy::rows_starting_with(folder, prefix) {
                    let path = agy::conversation_path(folder, &row.id);
                    add((Agent::Agy, row.id), folder, path);
                }
            }
            Agent::Kimi => {
                for x in kimi::sessions_starting_with(folder, prefix) {
                    add((Agent::Kimi, x.id), folder, x.dir);
                }
            }
            Agent::Zcode => {
                for row in zcode::rows_starting_with(folder, prefix) {
                    let path = zcode::artifacts_path(folder, &row.id);
                    add((Agent::Zcode, row.id), folder, path);
                }
            }
            Agent::Dsh => {
                for x in dsh::sessions_starting_with(folder, prefix) {
                    add((Agent::Dsh, x.id), folder, x.dir);
                }
            }
            Agent::Mcode => {
                for x in crate::mcode::sessions_starting_with(folder, prefix) {
                    add((Agent::Mcode, x.row.id), folder, x.dir);
                }
            }
            Agent::Qwen => {
                for x in crate::qwen::sessions_starting_with(folder, prefix) {
                    add((Agent::Qwen, x.id), folder, x.file);
                }
            }
        }
    }
    found
}

/// What a move leaves to resume: the session, the directory it ran in, and where it went.
struct Done {
    session: Option<String>,
    cwd: Option<String>,
    target_agent: Agent,
    target: String,
}

/// The move itself, saying what it does as it goes and filling in the report.
fn run(a: &Ask, prog: &str, report: &mut Map<String, Value>) -> Result<Done, Stop> {
    let show = paths::shell_path;
    out::setup_console();
    let wanted = Target::parse(&a.profile);
    // Look in every profile, home and folder but where it may go, or just in --from.
    let excluded: Vec<String> = wanted
        .candidates()
        .iter()
        .map(|(_, p)| ospath::realpath(p))
        .collect();
    let sources: Vec<(Agent, String)> = match &a.source {
        Some(source) => {
            let from = Target::parse(source);
            let mut folders: Vec<(Agent, String)> = from
                .candidates()
                .into_iter()
                .filter(|(_, p)| ospath::isdir(p))
                .collect();
            if folders.is_empty() {
                folders = from.candidates().into_iter().take(1).collect();
            }
            folders
        }
        None => paths::find_all()
            .into_iter()
            .filter(|(_, p)| !excluded.contains(&ospath::realpath(p)))
            .collect(),
    };
    let found = find_sessions(&sources, &a.session);
    if found.is_empty() {
        if a.source.is_some() {
            return Err(format!(
                "No session starting with {} in {}",
                a.session,
                show(&sources[0].1)
            ));
        }
        for (agent, folder) in wanted.candidates() {
            if ospath::isdir(&folder)
                && !find_sessions(&[(agent, folder.clone())], &a.session).is_empty()
            {
                return Err(format!(
                    "Session {} is already in {}, and in no other profile",
                    a.session,
                    show(&folder)
                ));
            }
        }
        return Err(format!("No session starting with {}", a.session));
    }
    let mut found = found;
    if found.len() > 1 {
        found.sort_by(|x, y| x.0.cmp(&y.0));
        let lines: Vec<String> = found
            .iter()
            .map(|((_, sid), places)| {
                format!(
                    "  {sid}  {}",
                    places
                        .iter()
                        .map(|(p, _)| show(p))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            })
            .collect();
        let mut shown: Vec<String> = lines.iter().take(10).cloned().collect();
        if lines.len() > 10 {
            shown.push("  …".into());
        }
        return Err(format!(
            "{} matches {} sessions; give more of the ID:\n{}",
            a.session,
            found.len(),
            shown.join("\n")
        ));
    }
    let ((agent, sid), places) = found.remove(0);
    let (target_agent, target) = match wanted.resolve(agent) {
        Ok(t) => t,
        Err((own, claude)) => {
            if a.json || !interactive() {
                return Err(format!(
                    "{} is both {} and {}; say {}:{} or claude:{}",
                    a.profile,
                    show(&own),
                    show(&claude),
                    agent.key(),
                    a.profile,
                    a.profile
                ));
            }
            choose_target(agent, &a.profile, &own, &claude, report)?
        }
    };
    if target_agent == Agent::Agy && agy::is_app_root(&target) {
        return Err(format!(
            "{} is the Antigravity app's folder, which only the app writes; convert the session for agy instead: {prog} {} agy",
            show(&target),
            a.session
        ));
    }
    if target_agent == Agent::Dsh {
        return Err(
            "DeepSeek Harness's session store isn't one hatcheck can write; choose another agent as the target".into(),
        );
    }
    if target_agent == Agent::Mcode {
        return Err(
            "MiniMax Code's session store isn't one hatcheck can write; choose another agent as the target".into(),
        );
    }
    if target_agent == Agent::Qwen {
        return Err(
            "Qwen Code's session store isn't one hatcheck can write; choose another agent as the target".into(),
        );
    }
    if !ospath::isdir(&target) {
        match target_agent {
            Agent::Codex => {
                let base = ospath::basename(&target);
                let name = base.strip_prefix(".codex-").unwrap_or("");
                return Err(format!(
                    "{} isn't a Codex home. Sign in there first, e.g. with: {}",
                    show(&target),
                    if name.is_empty() {
                        "codex login".to_string()
                    } else {
                        format!("cx {name} login")
                    }
                ));
            }
            Agent::Agy => {
                return Err(format!(
                    "{} isn't an Antigravity CLI folder. Run agy once to sign in",
                    show(&target)
                ));
            }
            Agent::Kimi => {
                return Err(format!(
                    "{} isn't a Kimi home. Sign in there first, e.g. with: {}",
                    show(&target),
                    paths::kimi_command(&target, "login", false)
                ));
            }
            Agent::Zcode => {
                return Err(format!(
                    "{} isn't a ZCode data folder. Run zcode once to sign in",
                    show(&target)
                ));
            }
            Agent::Dsh => {
                return Err(format!(
                    "{} isn't a DeepSeek Harness home. Run dsh web there once to sign in",
                    show(&target)
                ));
            }
            Agent::Mcode => {
                return Err(format!(
                    "{} isn't a MiniMax Code data home. Sign in there first, e.g. with: mcode login",
                    show(&target)
                ));
            }
            Agent::Qwen => {
                return Err(format!(
                    "{} isn't a Qwen Code home. Run qwen there once to sign in",
                    show(&target)
                ));
            }
            Agent::Claude => {
                return Err(format!(
                    "{} isn't a profile folder. Sign in there first, e.g. with: cl {}",
                    show(&target),
                    a.profile
                ));
            }
        }
    }
    let source = if places.len() > 1 {
        choose_source(&sid, &places, agent, a.json, report)?
    } else {
        places[0].0.clone()
    };
    let files = places
        .iter()
        .find(|(f, _)| *f == source)
        .map(|(_, f)| f.clone())
        .unwrap_or_default();
    if agent == target_agent && ospath::realpath(&source) == ospath::realpath(&target) {
        return Err(format!("Session {sid} is already in {}", show(&target)));
    }
    let mut why: Option<String> = None;
    if agent == Agent::Codex && target_agent == Agent::Codex {
        let accounts = (codex_account(&source), codex_account(&target));
        if accounts.0.is_none() || accounts.1.is_none() || accounts.0 != accounts.1 {
            why = Some(format!("{} and {} aren't signed in to the same ChatGPT account, and Codex encrypts its reasoning for the account that wrote it, so the session is converted", show(&source), show(&target)));
        }
    }
    // A conversion leaves things out, so its original is always kept, moving or not.
    let converting = agent != target_agent || why.is_some();
    let fork_cmd = fork_command(prog, a);
    let (resume_sid, cwd) = match agent {
        _ if converting => {
            if agent == Agent::Claude {
                if let Some(stop) = open_refusal(&source, &sid, false) {
                    return Err(stop);
                }
            }
            convert(
                a,
                agent,
                &sid,
                &source,
                &files,
                target_agent,
                &target,
                why.as_deref(),
                report,
            )?
        }
        Agent::Claude => {
            let (s_, c) = move_claude(a, &sid, &source, &files, &target, &fork_cmd, report)?;
            (Some(s_), c)
        }
        Agent::Codex => {
            let (s_, c) = copy_codex(a, &sid, &source, &files, &target, &fork_cmd, report)?;
            (Some(s_), c)
        }
        Agent::Kimi => {
            let (s_, c) = copy_kimi(a, &sid, &source, &files, &target, &fork_cmd, report)?;
            (Some(s_), c)
        }
        Agent::Agy => {
            return Err(format!(
                "Antigravity CLI keeps its conversations in a form hatcheck can't copy; convert session {} for another agent instead",
                py::head(&sid, 8)
            ))
        }
        Agent::Zcode => {
            return Err(format!(
                "ZCode keeps its sessions in a database hatcheck doesn't write; convert session {} for another agent instead",
                py::head(&sid, 8)
            ))
        }
        Agent::Dsh => {
            return Err(format!(
                "DeepSeek Harness's session logs and workspace state aren't safe for hatcheck to copy; convert session {} for another agent instead",
                py::head(&sid, 8)
            ))
        }
        Agent::Mcode => {
            return Err(format!(
                "MiniMax Code's shared SQLite state, manifest paths and session assets aren't safe for hatcheck to copy; convert session {} for another agent instead",
                py::head(&sid, 8)
            ))
        }
        Agent::Qwen => {
            return Err(format!(
                "Qwen Code's transcripts, registry and file history aren't copied between its homes; convert session {} for another agent instead",
                py::head(&sid, 8)
            ))
        }
    };
    Ok(Done {
        session: resume_sid,
        cwd,
        target_agent,
        target,
    })
}

pub fn main(argv: &[String], prog: &str) -> i32 {
    let parser = Parser::new(prog, "Move a Claude Code, Codex, Antigravity CLI, Kimi Code CLI, ZCode, DeepSeek Harness, MiniMax Code or Qwen Code session to another profile, to carry on with it there, or convert it for another agent. With a profile alone, the newest session started from this folder goes to it; with nothing, or a session alone, to the best account of its agent now, as hatcheck best picks it; either after asking.")
        .positional("session", Count::Optional, "the session ID, or the start of it (default: the newest session started from this folder; one word that names a profile is PROFILE)")
        .positional("profile", Count::Optional, "where it goes: NAME for ~/.claude-NAME, default or claude for ~/.claude, codex for ~/.codex, codex:NAME for ~/.codex-NAME, agy for ~/.gemini/antigravity-cli, kimi for ~/.kimi-code, kimi:NAME for ~/.kimi-code-NAME, zcode for ~/.zcode, dsh for ~/.dsh, mcode for ~/.minimax or qwen for ~/.qwen (not supported targets), or a path (default: the best account of the session's agent now, as hatcheck best picks it)")
        .option(&["--from"], "source", "PROFILE", "the profile or home to take it from, when more than one has it")
        .flag(&["--keep"], "keep", "keep the original, which copies the session rather than moving it (a converted session's original is always kept)")
        // Still taken, so scripts that pass them run: what --no-keep asked for is the default
        // now, and --fork is hatcheck fork, which keeps the original as --fork always did.
        .hidden(&["--no-keep"], "no_keep", Takes::Nothing)
        .hidden(&["--fork"], "fork", Takes::Nothing)
        .flag(&["--dry-run"], "dry_run", "show what would be moved, copied, converted and deleted, and change nothing")
        .flag(&["--json"], "json", "say what was or would be done as JSON, and never ask to resume it");
    let args = match parser.parse(argv) {
        Ok(a) => a,
        Err(stop) => return stop.exit_code(),
    };
    let fork = args.flag("fork");
    let mut a = Ask {
        session: args.value("session").unwrap_or("").to_string(),
        profile: args.value("profile").unwrap_or("").to_string(),
        source: args
            .value("source")
            .filter(|s| !s.is_empty())
            .map(String::from),
        keep: fork || args.flag("keep"),
        fork,
        dry_run: args.flag("dry_run"),
        json: args.flag("json"),
    };
    if let Err(stop) = fill_in(&mut a, prog) {
        return if stop == "Nothing changed." {
            out::cancel(&stop)
        } else {
            out::fail(&stop)
        };
    }
    answer(&a, prog)
}

/// hatcheck fork SESSION PROFILE ...: bring a session into another profile as a new one, with its
/// own ID, and keep the original; for the other agent, convert it, as move does.
pub fn fork_main(argv: &[String], prog: &str) -> i32 {
    let parser = Parser::new(prog, "Fork a Claude Code, Codex, Antigravity CLI, Kimi Code CLI, ZCode, DeepSeek Harness, MiniMax Code or Qwen Code session into another profile: bring it in as a new session with its own ID, beside any copy already there, and keep the original. For another agent, it's converted. With a profile alone, the newest session started from this folder goes into it; with nothing, or a session alone, into the best account of its agent now, as hatcheck best picks it; either after asking.")
        .positional("session", Count::Optional, "the session ID, or the start of it (default: the newest session started from this folder; one word that names a profile is PROFILE)")
        .positional("profile", Count::Optional, "where it goes: NAME for ~/.claude-NAME, default or claude for ~/.claude, codex for ~/.codex, codex:NAME for ~/.codex-NAME, agy for ~/.gemini/antigravity-cli, kimi for ~/.kimi-code, kimi:NAME for ~/.kimi-code-NAME, zcode for ~/.zcode, dsh for ~/.dsh, mcode for ~/.minimax or qwen for ~/.qwen (not supported targets), or a path (default: the best account of the session's agent now, as hatcheck best picks it)")
        .option(&["--from"], "source", "PROFILE", "the profile or home to take it from, when more than one has it")
        .flag(&["--dry-run"], "dry_run", "show what would be copied or converted, and change nothing")
        .flag(&["--json"], "json", "say what was or would be done as JSON, and never ask to resume it");
    let args = match parser.parse(argv) {
        Ok(a) => a,
        Err(stop) => return stop.exit_code(),
    };
    let mut a = Ask {
        session: args.value("session").unwrap_or("").to_string(),
        profile: args.value("profile").unwrap_or("").to_string(),
        source: args
            .value("source")
            .filter(|s| !s.is_empty())
            .map(String::from),
        keep: true,
        fork: true,
        dry_run: args.flag("dry_run"),
        json: args.flag("json"),
    };
    if let Err(stop) = fill_in(&mut a, prog) {
        return if stop == "Nothing changed." {
            out::cancel(&stop)
        } else {
            out::fail(&stop)
        };
    }
    answer(&a, prog)
}

/// Do what move or fork was asked: the text as it goes, or with --json the report at the end; then
/// in a terminal, the offer to resume it.
fn answer(a: &Ask, prog: &str) -> i32 {
    let mut report = Map::new();
    report.insert("version".into(), json!(1));
    report.insert("dry_run".into(), json!(a.dry_run));
    if a.json {
        // The text goes nowhere; the report is printed at the end.
        let (result, _, err) = out::captured(|| run(a, prog, &mut report));
        out::eprint(&err);
        return match result {
            Ok(_) => {
                out::println(&py::dumps(&Value::Object(report)));
                0
            }
            Err(stop) => out::fail(&stop),
        };
    }
    match run(a, prog, &mut report) {
        Err(stop) if stop == "Nothing changed." => out::cancel(&stop),
        Err(stop) => out::fail(&stop),
        Ok(Done {
            session,
            cwd,
            target_agent,
            target,
        }) => {
            // In a terminal, offer to run that now. Never when piped, so scripts don't stop to ask.
            if let (Some(sid), Some(cwd)) = (session, cwd) {
                if !a.dry_run && ospath::isdir(&cwd) && interactive() {
                    let answer = read_answer(&match target_agent {
                        Agent::Agy | Agent::Zcode => format!(
                            "Start it now with {}, in {}? [y/N] ",
                            target_agent.program(),
                            paths::shell_path(&cwd)
                        ),
                        _ => format!("Resume it now in {}? [y/N] ", paths::shell_path(&target)),
                    });
                    if answer.is_none() {
                        out::println("");
                    }
                    if matches!(
                        answer
                            .as_deref()
                            .map(|x| py::strip(x).to_lowercase())
                            .as_deref(),
                        Some("y" | "yes")
                    ) {
                        return resume_now(&target, &sid, &cwd, target_agent);
                    }
                }
            }
            0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standings() {
        assert_eq!(standing(b"a\n", b"a\n", true, (0.0, 0.0)), "same");
        assert_eq!(standing(b"a\n", b"a\nb\n", true, (0.0, 0.0)), "older");
        assert_eq!(standing(b"a\nb\n", b"a\n", true, (0.0, 0.0)), "newer");
        assert_eq!(standing(b"a\nc\n", b"a\nb\n", true, (0.0, 0.0)), "diverged");
        assert_eq!(
            standing(b"{\"x\": 1}\n", b"{\"x\":1.0}\n", true, (0.0, 0.0)),
            "same"
        );
        assert_eq!(overall(["new", "same"].into_iter()), "older");
        assert_eq!(overall(["older", "newer"].into_iter()), "diverged");
        // How a copy already in the profile compares with the one being copied.
        let a: &[u8] = b"{\"a\":1,\"b\":\"x\"}\n{\"c\":2}\n";
        let (more, other) = ([a, b"{\"d\":3}\n"].concat(), [a, b"{\"e\":4}\n"].concat());
        assert_eq!(standing(a, a, true, (0.0, 0.0)), "same");
        assert_eq!(standing(a, &more, true, (0.0, 0.0)), "older");
        assert_eq!(standing(&more, a, true, (0.0, 0.0)), "newer");
        assert_eq!(standing(&more, &other, true, (0.0, 0.0)), "diverged");
        assert_eq!(
            standing(b"{\"a\": 1, \"b\": \"x\"}\n", a, true, (0.0, 0.0)),
            "older"
        ); // the same record, spaced out
        assert_eq!(
            standing(b"output", b"output and more", false, (9.0, 1.0)),
            "older"
        ); // added to
        assert_eq!(
            standing(b"# Plan\n", b"# Better plan\n", false, (1.0, 2.0)),
            "older"
        ); // written over: the newer
        assert_eq!(
            standing(b"# Better plan\n", b"# Plan\n", false, (2.0, 1.0)),
            "newer"
        );
        assert_eq!(overall(["older", "same", "newer"].into_iter()), "diverged");
        assert_eq!(overall(["same"].into_iter()), "same");
    }

    #[test]
    fn ids_and_times() {
        let v4 = uuid4();
        assert_eq!((v4.len(), &v4[14..15]), (36, "4"));
        assert!(matches!(&v4[19..20], "8" | "9" | "a" | "b"));
        let v7 = uuid7(1792274316.5);
        assert!(v7.starts_with("01a14bdf-bcd4-7")); // the time comes first, so these sort by it
        assert!(matches!(&v7[19..20], "8" | "9" | "a" | "b"));
        assert_eq!(iso_ms(1792274316.5), "2026-10-17T21:58:36.500Z");
        // fromtimestamp rounds to the next second; the milliseconds come from the time as it is
        assert_eq!(iso_ms(1792274316.9999996), "2026-10-17T21:58:37.999Z");
    }

    #[test]
    fn sizes_as_percent_f_gives_them() {
        let shown: Vec<String> = [100, 1075, 1280, 1331, 3584, 2621440, 1153433]
            .into_iter()
            .map(size_text)
            .collect();
        assert_eq!(
            shown,
            [
                "100 bytes",
                "1.0 KB",
                "1.2 KB",
                "1.3 KB",
                "3.5 KB",
                "2.5 MB",
                "1.1 MB"
            ]
        );
    }

    #[test]
    fn long_sessions_keep_the_first_prompt_and_the_newest_turns() {
        let when = json!("2026-10-17T21:58:36.500Z");
        let turns: Vec<Turn> = (0..10)
            .map(|n| {
                (
                    if n % 2 == 0 { "user" } else { "assistant" }.to_string(),
                    vec![format!("{n} {}", "x".repeat(1000))],
                    when.clone(),
                )
            })
            .collect();
        let conv = Conversation {
            agent: Agent::Codex,
            id: "019f0000-aaaa-7000-8000-000000000001".into(),
            cwd: Some("/w".into()),
            model: Value::Null,
            turns,
            calls: 0,
            title: None,
            started: when.clone(),
            branch: Value::Null,
            reasoning: "Codex's reasoning",
            left_out: "",
        };
        let (fitted, dropped, cut) = fit_within(&conv, 4500);
        assert_eq!((dropped, cut), (6, false));
        assert_eq!(
            fitted.iter().map(|t| t.0.as_str()).collect::<Vec<_>>(),
            ["user", "assistant", "user", "assistant"]
        );
        let first = fitted[0].1.join("\n");
        assert!(first.starts_with("[Imported from Codex session 019f0000-aaaa-7000-8000-000000000001, started 2026-10-17 21:58 in /w. "));
        assert!(
            first.contains("0 x")
                && first.contains("[6 earlier turns left out here, to fit the context window.]")
        );
        assert!(fitted[1].1.concat().contains("7 x"));
        assert!(fitted.last().unwrap().1.last().unwrap().starts_with("9 x"));
    }

    #[test]
    fn the_newest_turn_stays_when_it_alone_is_too_long() {
        let when = json!("2026-10-17T21:58:36.500Z");
        let conv = |newest: Vec<String>| {
            let mut turns: Vec<Turn> = (0..5)
                .map(|n| {
                    (
                        if n % 2 == 0 { "user" } else { "assistant" }.to_string(),
                        vec![format!("{n} {}", "x".repeat(1000))],
                        when.clone(),
                    )
                })
                .collect();
            turns.push(("assistant".into(), newest, when.clone()));
            Conversation {
                agent: Agent::Codex,
                id: "019f0000-aaaa-7000-8000-000000000001".into(),
                cwd: Some("/w".into()),
                model: Value::Null,
                turns,
                calls: 0,
                title: None,
                started: when.clone(),
                branch: Value::Null,
                reasoning: "Codex's reasoning",
                left_out: "",
            }
        };
        // Twenty tool calls and the answer, together far past the budget: the latest of them kept.
        let mut parts: Vec<String> = (0..20)
            .map(|n| format!("call {n} {}", "y".repeat(500)))
            .collect();
        parts.push("the final answer".into());
        let fitted = fit_within(&conv(parts), 4500);
        assert_eq!((fitted.1, fitted.2), (4, true));
        let newest = &fitted.0.last().unwrap().1;
        assert_eq!(newest.last().unwrap(), "the final answer");
        assert_eq!(
            newest[0],
            "[14 earlier parts of this turn left out here, to fit the context window.]"
        );
        assert!(newest[1].starts_with("call 14 "));
        // One part alone too long: cut to fit.
        let fitted = fit_within(&conv(vec!["z".repeat(10_000)]), 4500);
        let newest = fitted.0.last().unwrap().1.concat();
        assert!(
            newest.starts_with("zzz") && newest.chars().count() < 3600,
            "{}",
            newest.len()
        );
    }

    #[test]
    fn model_provider_from_config() {
        let dir = std::env::temp_dir().join(format!("hatcheck-provider-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let home = dir.to_string_lossy().into_owned();
        let cases = [
            (
                "model = \"x\"\n  model_provider = 'azure' # set by hand\n",
                "azure",
            ),
            ("model_provider=\"oss\"\n", "oss"),
            ("[profiles.a]\nmodel_provider = \"later\"\n", "openai"), // past the top-level settings
            ("model_provider = \"\"\n", "openai"),
            ("model_provider = \"open\n", "openai"),
        ];
        for (text, want) in cases {
            fs::write(dir.join("config.toml"), text).unwrap();
            assert_eq!(codex_provider(&home), want, "{text:?}");
        }
        fs::remove_dir_all(&dir).unwrap();
        assert_eq!(codex_provider(&home), "openai");
    }

    #[test]
    fn the_fork_command_quotes_values_not_flags() {
        let mut a = Ask {
            session: "11111111".into(),
            profile: "b".into(),
            source: None,
            keep: false,
            fork: false,
            dry_run: false,
            json: false,
        };
        assert_eq!(
            fork_command("hatcheck move", &a),
            "hatcheck fork 11111111 b"
        );
        // --keep isn't the fork's: it always keeps the original. The same in PowerShell, which
        // would quote --dry-run as '--dry-run'.
        (a.source, a.keep, a.dry_run) = (Some("my profile".into()), true, true);
        assert_eq!(
            fork_command("hat move", &a),
            "hat fork 11111111 b --from 'my profile' --dry-run"
        );
    }

    #[test]
    fn windows_paths_in_transcripts_are_repointed() {
        let id = "abcdef12-0000-4000-8000-000000000001";
        let (old, new) = (
            format!("C:\\Users\\me\\.claude\\projects\\X\\{id}"),
            format!("C:\\Users\\me\\.claude-b\\projects\\X\\{id}"),
        );
        let json_path = |p: &str| p.replace('\\', "\\\\"); // as it's written in JSON
        let line = format!("{{\"type\":\"user\",\"message\":{{\"role\":\"user\",\"content\":\"saved to {}\\\\tool-results\\\\a.txt\"}}}}", json_path(&old));
        let dir = std::env::temp_dir().join(format!("hatcheck-repoint-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("{id}.jsonl"));
        fs::write(&path, format!("{line}\n")).unwrap();
        let out = copy_bytes(&path.to_string_lossy(), &[(old.clone(), new.clone())], None);
        fs::remove_dir_all(&dir).unwrap();
        let out = String::from_utf8(out.unwrap()).unwrap();
        assert!(out.contains(&json_path(&new)), "{out}");
        assert!(!out.contains(&json_path(&old)), "{out}");
    }

    #[test]
    fn repointing_leaves_thinking_alone() {
        let pairs = vec![("/a/x".to_string(), "/b/x".to_string())];
        let v = json!({"text": "saved to /a/x/out.txt", "c": [{"type": "thinking", "thinking": "/a/x"}], "sessionId": "S"});
        let ids = ("S".to_string(), "T".to_string());
        assert_eq!(
            repoint(&v, &pairs, Some(&ids)),
            json!({"text": "saved to /b/x/out.txt", "c": [{"type": "thinking", "thinking": "/a/x"}], "sessionId": "T"})
        );
    }
}
