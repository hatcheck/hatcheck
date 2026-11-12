//! hatcheck search WORDS... [--since HOURS] [--here] [--from PROFILE] [--only AGENT]
//! [--ignore AGENT] [-a] [-m N] [-f] [-j]: the sessions whose title or conversation has every
//! word, in the selected agents' profiles and homes, newest first, each with the command that
//! resumes it and the turns the words are in.
//! docs/commands.md's `hatcheck search` section has the rest.
//!
//! A conversation is what hatcheck show reads, so a word only in a message an agent adds itself,
//! such as a system reminder, doesn't count. Claude Code's and Codex's transcripts, by far the
//! most and the largest, are looked through as bytes first: a word of ASCII letters, digits and
//! punctuation that the file doesn't hold in either case can't be in its conversation, so most of
//! them are passed over without being read as one. The other agents' sessions are read from their
//! stores. Nothing is written but hatcheck's own cache of what the session list knows of a file.

use crate::agent::Agent;
use crate::args::{Count, Parser};
use crate::conv;
use crate::facts::{self, Summary};
use crate::ospath::{self, Part};
use crate::out::{self, Paint};
use crate::paths;
use crate::py;
use crate::sessions;
use crate::times;
use crate::widths::{char_width, truncate};
use memchr::memmem;
use serde_json::{json, Map, Value};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

/// How many sessions are shown when --max doesn't say.
const DEFAULT_MAX: usize = 20;
/// How many of a session's turns with the words are shown.
const SHOWN_TURNS: usize = 2;
/// How much of a file is lowercased and looked through at a time.
const CHUNK: usize = 1 << 20;
/// How wide a turn's excerpt is in --json, in columns.
const JSON_COLS: usize = 160;

/// The words, as asked for and as matched.
struct Query {
    words: Vec<String>,
    lower: Vec<String>,
    /// Finders for the words a transcript has to hold as bytes: those of ASCII letters, digits
    /// and the punctuation JSON writes as it is, lowercased.
    raw: Vec<memmem::Finder<'static>>,
}

impl Query {
    fn new(words: &[String]) -> Query {
        let lower: Vec<String> = words.iter().map(|w| w.to_lowercase()).collect();
        let raw = lower
            .iter()
            .filter(|w| {
                w.bytes()
                    .all(|b| (0x20..0x7f).contains(&b) && b != b'"' && b != b'\\')
            })
            .map(|w| memmem::Finder::new(w.as_bytes()).into_owned())
            .collect();
        Query {
            words: words.to_vec(),
            lower,
            raw,
        }
    }

    /// The words as a heading names them: "rate", "rate" and "limiter", or "a", "b" and "c".
    fn named(&self) -> String {
        let quoted: Vec<String> = self.words.iter().map(|w| format!("\"{w}\"")).collect();
        match quoted.split_last() {
            Some((last, rest)) if !rest.is_empty() => format!("{} and {last}", rest.join(", ")),
            _ => quoted.join(""),
        }
    }
}

/// Which sessions to look in, from the options.
struct Scope {
    all: bool,
    cutoff: f64,
    here: Option<String>,
}

/// A session to look in: a Claude Code transcript or a Codex session file, looked through as
/// bytes before it's read (its profile or home, and for Codex the thread names of that home), or
/// a session another agent's store summed up.
enum Job {
    Claude(String, String),
    Codex(String, usize, String),
    Store(Box<Summary>),
}

/// A session with every word: how many of its turns have any of them, and the ones shown, as
/// (role, time, the turn's text).
struct Hit {
    s: Summary,
    turns: usize,
    shown: Vec<(String, Value, String)>,
}

enum Outcome {
    /// Not looked in: out of the window or the folder, left out by config.toml, a subagent's,
    /// empty, unreadable, or from a hidden host and without the words.
    Skipped,
    /// Looked in, without every word.
    Miss,
    /// From a host the list hides without --all, with the words in its bytes but not read as
    /// a conversation: the label it counts under.
    Hidden(String),
    Hit(Box<Hit>),
}

/// Which of the finders' words the file holds as bytes, ASCII letters in either case: all of
/// them for a file that can't be read, so reading it as a conversation says what's wrong, and
/// none for an empty one.
fn file_holds(path: &str, finders: &[memmem::Finder<'static>]) -> Vec<bool> {
    let mut found = vec![false; finders.len()];
    if finders.is_empty() {
        return found;
    }
    let all = vec![true; finders.len()];
    let Ok(mut file) = std::fs::File::open(path) else {
        return all;
    };
    // Read, not mapped: a file is looked through once, front to back, and on macOS the page
    // faults of a map cost more than the reading.
    let overlap = finders.iter().map(|f| f.needle().len()).max().unwrap_or(1) - 1;
    let mut left = finders.len();
    let mut chunk = vec![0u8; overlap + CHUNK];
    let mut kept = 0; // the end of the last read, already lowercased, for a word across reads
    loop {
        let n = match std::io::Read::read(&mut file, &mut chunk[kept..]) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => return all,
        };
        let end = kept + n;
        chunk[kept..end].make_ascii_lowercase();
        for (i, finder) in finders.iter().enumerate() {
            if !found[i] && finder.find(&chunk[..end]).is_some() {
                found[i] = true;
                left -= 1;
            }
        }
        if left == 0 {
            break;
        }
        kept = overlap.min(end);
        chunk.copy_within(end - kept..end, 0);
    }
    found
}

/// Why a summed-up session isn't looked in: None to pass it over, or the label it's hidden under
/// without --all. Ok when it's looked in.
fn admit(s: &Summary, scope: &Scope) -> Result<(), Option<String>> {
    let entrypoint = s.entrypoint_text();
    if entrypoint == "subagent" || !s.has_messages || s.when < scope.cutoff {
        return Err(None);
    }
    if crate::config::session_left_out(s.cwd.as_deref()) {
        return Err(None); // as if it weren't there, as for every command
    }
    if let Some(here) = &scope.here {
        if !s
            .cwd
            .as_deref()
            .is_some_and(|c| !c.is_empty() && paths::same_folder(c, here))
        {
            return Err(None);
        }
    }
    if entrypoint != "cli" && !scope.all {
        let label = if entrypoint == "claude-desktop" {
            "Desktop".to_string()
        } else {
            entrypoint
        };
        return Err(Some(label));
    }
    Ok(())
}

/// The conversation's turns that have any of the words, the ones to show first: none when the
/// title and the turns together don't have every word.
fn matched(title: &str, turns: &[conv::Turn], q: &Query) -> Option<(usize, Vec<usize>)> {
    let title = title.to_lowercase();
    let mut have: Vec<bool> = q.lower.iter().map(|w| title.contains(w.as_str())).collect();
    let mut with: Vec<(usize, usize)> = Vec::new(); // (turn, how many of the words it has)
    for (i, (_, texts, _)) in turns.iter().enumerate() {
        let text = texts.join("\n\n").to_lowercase();
        let mut count = 0;
        for (k, w) in q.lower.iter().enumerate() {
            if text.contains(w.as_str()) {
                have[k] = true;
                count += 1;
            }
        }
        if count > 0 {
            with.push((i, count));
        }
    }
    if !have.iter().all(|h| *h) {
        return None;
    }
    let total = with.len();
    // The turns with the most of the words, the earliest of those first, then in their order.
    let mut best = with.clone();
    best.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let mut shown: Vec<usize> = best.iter().take(SHOWN_TURNS).map(|(i, _)| *i).collect();
    shown.sort_unstable();
    Some((total, shown))
}

/// text's characters lowercased, with the index of the character each came from.
fn fold(text: &[char]) -> (Vec<char>, Vec<usize>) {
    let mut lower = Vec::with_capacity(text.len());
    let mut from = Vec::with_capacity(text.len());
    for (i, c) in text.iter().enumerate() {
        for l in c.to_lowercase() {
            lower.push(l);
            from.push(i);
        }
    }
    (lower, from)
}

/// Where the words are in text, as character ranges, in order and joined where they overlap.
fn spans(text: &[char], words: &[String]) -> Vec<(usize, usize)> {
    let (lower, from) = fold(text);
    let mut found: Vec<(usize, usize)> = Vec::new();
    for w in words {
        let w: Vec<char> = w.chars().collect();
        if w.is_empty() || w.len() > lower.len() {
            continue;
        }
        let mut i = 0;
        while i + w.len() <= lower.len() {
            if lower[i..i + w.len()] == w[..] {
                found.push((from[i], from[i + w.len() - 1] + 1));
                i += w.len();
            } else {
                i += 1;
            }
        }
    }
    found.sort_unstable();
    let mut joined: Vec<(usize, usize)> = Vec::new();
    for (a, b) in found {
        match joined.last_mut() {
            Some(last) if a <= last.1 => last.1 = last.1.max(b),
            _ => joined.push((a, b)),
        }
    }
    joined
}

/// The part of a turn around the first of the words in it, its whitespace folded, cut to about
/// cols columns with … where it was cut, and where the words are in that part.
fn excerpt(text: &str, words: &[String], cols: usize) -> (String, Vec<(usize, usize)>) {
    let flat: Vec<char> = py::collapse(text).chars().collect();
    let (first, first_end) = spans(&flat, words).first().copied().unwrap_or((0, 0));
    let width =
        |range: std::ops::Range<usize>| flat[range].iter().map(|c| char_width(*c)).sum::<usize>();
    let cols = cols.max(20);
    // Some of what comes before the first word, so it's read in context.
    let mut start = first;
    while start > 0 && width(start - 1..first) <= cols / 4 {
        start -= 1;
    }
    let mut end = first;
    while end < flat.len() && width(start..end + 1) + 2 <= cols {
        end += 1;
    }
    // Near the end of the turn, the room left goes to what comes before.
    while start > 0 && width(start - 1..end) + 2 <= cols {
        start -= 1;
    }
    // A cut lands between words, keeping the word found whole.
    if start > 0 && flat[start - 1] != ' ' {
        if let Some(i) = flat[start..first].iter().position(|c| *c == ' ') {
            start += i + 1;
        }
    }
    if end < flat.len() && flat[end] != ' ' {
        if let Some(i) = flat[first_end.min(end)..end]
            .iter()
            .rposition(|c| *c == ' ')
        {
            end = first_end.min(end) + i;
        }
    }
    let mut shown: String = flat[start..end].iter().collect();
    let lead = usize::from(start > 0);
    if start > 0 {
        shown.insert(0, '…');
    }
    if end < flat.len() {
        shown.push('…');
    }
    let chars: Vec<char> = shown.chars().collect();
    let found = spans(
        &chars[lead..chars.len() - usize::from(end < flat.len())],
        words,
    )
    .into_iter()
    .map(|(a, b)| (a + lead, b + lead))
    .collect();
    (shown, found)
}

/// The excerpt with the words in bold.
fn painted(text: &str, found: &[(usize, usize)], paint: &Paint) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    let mut at = 0;
    for &(a, b) in found {
        out.extend(&chars[at..a]);
        out += &paint.p("1", &chars[a..b].iter().collect::<String>());
        at = b;
    }
    out.extend(&chars[at..]);
    out
}

/// Whether a transcript's bytes, or the title kept beside it, hold every word the bytes are
/// asked for: a Codex thread's name is in the home's index, not its file.
fn file_or_title_has(path: &str, title: &str, q: &Query) -> bool {
    let held = file_holds(path, &q.raw);
    let title = title.to_lowercase();
    q.raw
        .iter()
        .zip(&held)
        .all(|(f, h)| *h || title.contains(&*String::from_utf8_lossy(f.needle())))
}

/// Look in one session: what the session list knows of it first (cached), which says whether
/// it's in scope; then, for a transcript, its bytes; then its conversation.
fn look(job: &Job, q: &Query, scope: &Scope, names: &[Map<String, Value>]) -> Outcome {
    let (s, path) = match job {
        Job::Claude(profile, path) => match facts::read_session(path) {
            Ok(Some(mut s)) => {
                s.profile = profile.clone();
                (s, Some(path))
            }
            _ => return Outcome::Skipped,
        },
        Job::Codex(home, at, path) => match facts::read_codex_session(path, &names[*at]) {
            Ok(Some(mut s)) => {
                s.profile = home.clone();
                (s, Some(path))
            }
            _ => return Outcome::Skipped,
        },
        Job::Store(s) => ((**s).clone(), None),
    };
    let hidden = match admit(&s, scope) {
        Ok(()) => None,
        Err(None) => return Outcome::Skipped,
        Err(Some(label)) => Some(label),
    };
    if let Some(path) = path {
        if !file_or_title_has(path, &s.title, q) {
            return if hidden.is_some() {
                Outcome::Skipped
            } else {
                Outcome::Miss
            };
        }
    }
    if let Some(label) = hidden {
        if path.is_some() {
            // Not read: the SDK's sessions alone can be most of a profile, and --all is
            // what asks for them.
            return Outcome::Hidden(label);
        }
        // A store's session is read, since nothing cheaper says whether the words are in it.
        return match conv::read_for(&s) {
            Ok(c) if matched(&s.title, &c.turns, q).is_some() => Outcome::Hidden(label),
            _ => Outcome::Skipped,
        };
    }
    let Ok(conversation) = conv::read_for(&s) else {
        return Outcome::Miss;
    };
    let Some((turns, shown)) = matched(&s.title, &conversation.turns, q) else {
        return Outcome::Miss;
    };
    let shown = shown
        .into_iter()
        .map(|i| {
            let (role, texts, when) = &conversation.turns[i];
            (role.clone(), when.clone(), texts.join("\n\n"))
        })
        .collect();
    Outcome::Hit(Box::new(Hit { s, turns, shown }))
}

/// f of each item, on a thread per core (at most 16), in the items' order.
fn each<T: Sync, R: Send>(items: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let threads = std::thread::available_parallelism()
        .map_or(4, |n| n.get())
        .clamp(1, 16)
        .min(items.len().max(1));
    let next = AtomicUsize::new(0);
    let done: Mutex<Vec<(usize, R)>> = Mutex::new(Vec::with_capacity(items.len()));
    std::thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                if i >= items.len() {
                    break;
                }
                let r = f(&items[i]);
                done.lock().unwrap_or_else(|e| e.into_inner()).push((i, r));
            });
        }
    });
    let mut done = done.into_inner().unwrap_or_else(|e| e.into_inner());
    done.sort_by_key(|(i, _)| *i);
    done.into_iter().map(|(_, r)| r).collect()
}

/// The project folders Claude Code files a folder's sessions under: its name as given and as
/// its real path, cut as Claude Code cuts a long one.
fn project_dirs(folder: &str) -> Vec<String> {
    let mut names = vec![paths::encode_project(folder)];
    let real = paths::encode_project(&ospath::realpath(folder));
    if !names.contains(&real) {
        names.push(real);
    }
    names
}

/// Every session in scope, as jobs, and the thread names of each Codex home.
fn jobs(scope: &Scope, from: Option<&(Agent, String)>) -> (Vec<Job>, Vec<Map<String, Value>>) {
    let wanted = |agent: Agent, folder: &str| {
        from.is_none_or(|(a, f)| *a == agent && paths::same_folder(f, folder))
    };
    let recent =
        |path: &str| scope.cutoff <= 0.0 || ospath::getmtime(path).is_ok_and(|t| t >= scope.cutoff);
    let mut jobs = Vec::new();
    for profile in paths::find_profiles() {
        if !wanted(Agent::Claude, &profile) {
            continue;
        }
        let names = scope.here.as_deref().map(project_dirs);
        let mut files = Vec::new();
        match &names {
            None => {
                files = ospath::glob(
                    &profile,
                    &[
                        Part::Name("projects"),
                        Part::Wild("", ""),
                        Part::Wild("", ".jsonl"),
                    ],
                )
            }
            Some(names) => {
                for name in names {
                    // Claude Code cuts a folder's name at 200 characters and adds a hash.
                    let short: String = name.chars().take(200).collect();
                    let dir = if name.chars().count() > 200 {
                        Part::Wild(&short, "")
                    } else {
                        Part::Name(name)
                    };
                    files.extend(ospath::glob(
                        &profile,
                        &[Part::Name("projects"), dir, Part::Wild("", ".jsonl")],
                    ));
                }
            }
        }
        for path in files {
            if recent(&path) {
                jobs.push(Job::Claude(profile.clone(), path));
            }
        }
    }
    let mut names = Vec::new();
    for home in paths::find_codex_homes() {
        if !wanted(Agent::Codex, &home) {
            continue;
        }
        names.push(facts::codex_names(&home));
        for path in paths::codex_session_files(&home) {
            if recent(&path) {
                jobs.push(Job::Codex(home.clone(), names.len() - 1, path));
            }
        }
    }
    let mut stored: Vec<Summary> = Vec::new();
    for root in paths::find_agy_stores() {
        if wanted(Agent::Agy, &root) {
            stored.extend(
                crate::agy::rows(&root)
                    .iter()
                    .map(|r| crate::agy::summary(&root, r)),
            );
        }
    }
    for home in paths::find_kimi_homes() {
        if wanted(Agent::Kimi, &home) {
            stored.extend(
                crate::kimi::sessions(&home)
                    .iter()
                    .map(|x| crate::kimi::summary(&home, x)),
            );
        }
    }
    for root in paths::find_zcode_roots() {
        if wanted(Agent::Zcode, &root) {
            stored.extend(
                crate::zcode::rows(&root)
                    .iter()
                    .map(|r| crate::zcode::summary(&root, r)),
            );
        }
    }
    for home in paths::find_dsh_homes() {
        if wanted(Agent::Dsh, &home) {
            stored.extend(
                crate::dsh::sessions(&home)
                    .iter()
                    .map(|x| crate::dsh::summary(&home, x)),
            );
        }
    }
    for home in paths::find_mcode_homes() {
        if wanted(Agent::Mcode, &home) {
            stored.extend(
                crate::mcode::sessions(&home)
                    .iter()
                    .map(|x| crate::mcode::summary(&home, x)),
            );
        }
    }
    for home in paths::find_qwen_homes() {
        if wanted(Agent::Qwen, &home) {
            stored.extend(
                crate::qwen::sessions(&home)
                    .iter()
                    .map(|x| crate::qwen::summary(&home, x)),
            );
        }
    }
    for home in paths::find_copilot_homes() {
        if wanted(Agent::Copilot, &home) {
            stored.extend(
                crate::copilot::sessions(&home)
                    .iter()
                    .map(|x| crate::copilot::summary(&home, x)),
            );
        }
    }
    for home in paths::find_opencode_homes() {
        if wanted(Agent::Opencode, &home) {
            stored.extend(
                crate::opencode::sessions(&home)
                    .iter()
                    .map(|x| crate::opencode::summary(&home, x)),
            );
        }
    }
    // What can be told without reading a conversation is told here, so as not to read it.
    jobs.extend(
        stored
            .into_iter()
            .filter(|s| !matches!(admit(s, scope), Err(None)))
            .map(|s| Job::Store(Box::new(s))),
    );
    (jobs, names)
}

/// "1 session" or "3 sessions".
fn count(n: usize, what: &str) -> String {
    format!("{n} {what}{}", if n == 1 { "" } else { "s" })
}

pub fn main(argv: &[String], prog: &str) -> i32 {
    let parser = crate::agent::AgentSelection::options(Parser::new(
        prog,
        "Find the sessions whose title or conversation has every word, in every profile and home of every agent: newest first, each with the command that resumes it and the turns the words are in. Case doesn't matter, and a word can be part of a longer one.",
    ))
    .positional("words", Count::Any, "the words to look for")
    .option(&["--since"], "since", "HOURS", "only sessions active in the last HOURS: 36, 1.5, 12h or 3d")
    .flag(&["--here"], "here", "only sessions started from this folder")
    .option(&["--from"], "source", "PROFILE", "only this profile or home: NAME, codex:NAME, kimi:NAME and so on, or a path")
    .flag(&["-a", "--all"], "all", "also look in sessions from Claude Desktop, the SDK, the Codex app and the other hosts the session list hides without --all")
    .option(&["-m", "--max"], "max", "N", &format!("show at most N sessions (default {DEFAULT_MAX}; 0 for every one)"))
    .flag(&["-f", "--full"], "full", "print CLAUDE_CONFIG_DIR=... claude instead of the cl shortcut, and the same for the other launchers")
    .flag(&["-j", "--json"], "json", "print the sessions found as JSON");
    let args = match parser.parse(argv) {
        Ok(a) => a,
        Err(stop) => return stop.exit_code(),
    };
    let selection = match crate::agent::AgentSelection::parse(&args) {
        Ok(s) => s,
        Err(e) => return parser.error(&e).exit_code(),
    };
    crate::config::select_agents(selection);
    let words: Vec<String> = args
        .values("words")
        .iter()
        .flat_map(|w| w.split(py::is_space))
        .filter(|w| !w.is_empty())
        .map(String::from)
        .collect();
    if words.is_empty() {
        return parser
            .error("the following arguments are required: words")
            .exit_code();
    }
    let max = match args.value("max") {
        None => Some(DEFAULT_MAX),
        Some(n) => match n.parse::<usize>() {
            Ok(0) => None,
            Ok(n) => Some(n),
            Err(_) => {
                return parser
                    .error("argument -m/--max: expected a whole number (0 for every one)")
                    .exit_code()
            }
        },
    };
    let now = times::now();
    let since = match args.value("since") {
        None => None,
        Some(text) => match sessions::parse_hours(text) {
            Some(h) => Some(h),
            None => {
                return parser
                    .error("argument --since: expected hours like 24, 1.5, 12h or 3d")
                    .exit_code()
            }
        },
    };
    out::setup_console();
    let from = match args
        .value("source")
        .filter(|s| !s.is_empty())
        .map(crate::find::place_seen)
    {
        None => None,
        Some(Ok(place)) => Some(place),
        Some(Err(message)) => return out::fail(&message),
    };
    let here = args.flag("here").then(ospath::cwd);
    if let Some(folder) = &here {
        if crate::config::session_left_out(Some(folder)) {
            return out::fail(&format!(
                "{} leaves this folder out, so no session of it is looked in. Leave --here off to look in every other folder",
                crate::config::shown_path()
            ));
        }
    }
    let scope = Scope {
        all: args.flag("all"),
        cutoff: since.map_or(0.0, |h| now - h * 3600.0),
        here,
    };
    let (json_out, full) = (args.flag("json"), args.flag("full"));
    let query = Query::new(&words);
    let (jobs, names) = jobs(&scope, from.as_ref());
    let outcomes = each(&jobs, |job| look(job, &query, &scope, &names));
    facts::save_cache();

    let mut hits: Vec<Hit> = Vec::new();
    let mut hidden: Vec<(String, usize)> = Vec::new();
    let mut searched = 0; // looked in: the sessions in scope, less those from hidden hosts
    for outcome in outcomes {
        match outcome {
            Outcome::Skipped => {}
            Outcome::Miss => searched += 1,
            Outcome::Hidden(label) => match hidden.iter_mut().find(|(l, _)| *l == label) {
                Some((_, n)) => *n += 1,
                None => hidden.push((label, 1)),
            },
            Outcome::Hit(hit) => {
                searched += 1;
                // One copy of a session per profile: the newest, as the list keeps it.
                let key = |s: &Summary| (s.agent, ospath::realpath(&s.profile), s.id.clone());
                let rank = |s: &Summary| (s.when, s.canonical, s.size);
                match hits.iter_mut().find(|h| key(&h.s) == key(&hit.s)) {
                    Some(old) if rank(&hit.s) > rank(&old.s) => *old = *hit,
                    Some(_) => {}
                    None => hits.push(*hit),
                }
            }
        }
    }
    hits.sort_by(|a, b| {
        b.s.when
            .partial_cmp(&a.s.when)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let matched = hits.len();
    let mut folders: Vec<&str> = hits
        .iter()
        .map(|h| h.s.cwd.as_deref().unwrap_or(""))
        .collect();
    folders.sort_unstable();
    folders.dedup();
    let matched_folders = folders.len();
    if let Some(n) = max {
        hits.truncate(n);
    }
    let more = matched - hits.len();
    let hidden_total: usize = hidden.iter().map(|(_, n)| n).sum();

    if json_out {
        let rows: Vec<Value> = hits
            .iter()
            .map(|h| {
                let s = &h.s;
                json!({"agent": s.agent.key(), "id": s.id, "profile": ospath::abspath(&s.profile), "title": s.title,
                    "cwd": s.cwd, "last_active": times::iso_utc(s.when), "entrypoint": s.entrypoint, "branch": s.branch,
                    "model": s.model, "cost_usd": s.cost, "resume": sessions::session_command(s, full),
                    "turns_matched": h.turns,
                    "matches": h.shown.iter().map(|(role, when, text)| json!({"role": role, "time": when,
                        "text": excerpt(text, &query.lower, JSON_COLS).0})).collect::<Vec<_>>()})
            })
            .collect();
        let data = json!({"version": 1, "generated_at": times::iso_utc(now), "words": query.words,
            "since_hours": since, "this_folder_only": scope.here.is_some(), "searched": searched,
            "matched": matched, "more": more,
            "hidden": Value::Object(hidden.iter().map(|(k, n)| (k.clone(), json!(n))).collect()),
            "sessions": rows});
        out::println(&py::dumps(&data));
        return if hits.is_empty() { 1 } else { 0 };
    }

    let paint = Paint::new();
    let p = |code: &str, text: &str| paint.p(code, text);
    let window = since.map_or(String::new(), |h| {
        format!(" active in the last {}h", py::g(h))
    });
    let place = if scope.here.is_some() {
        " started from this folder"
    } else {
        ""
    };
    // The sessions the list hides without --all may have the words too: a transcript's bytes
    // have them, or a store's session was read. Said so, since they weren't searched.
    let hidden_note = || {
        let labels: Vec<String> = hidden.iter().map(|(l, n)| format!("{n} {l}")).collect();
        format!(
            "{} more from other hosts may have them ({}), which --all searches",
            hidden_total,
            labels.join(", ")
        )
    };
    if hits.is_empty() {
        let mut message = format!(
            "No session{window}{place} has {} in its title or conversation, of {} looked in",
            query.named(),
            searched
        );
        if hidden_total > 0 {
            message += &format!("; {}", hidden_note());
        }
        out::println(&format!("{message}."));
        return 1;
    }
    out::println(
        &(paint.heading(&format!("Sessions{window}{place} with {}", query.named()))
            + &p("2", " (by folder, newest first)")),
    );
    // Folders in the order of their newest session, each newest first.
    let home = paths::home();
    let mut groups: Vec<(String, Vec<&Hit>)> = Vec::new();
    for h in &hits {
        let folder =
            h.s.cwd
                .clone()
                .filter(|c| !c.is_empty())
                .unwrap_or_else(|| home.clone());
        match groups.iter_mut().find(|(f, _)| *f == folder) {
            Some((_, list)) => list.push(h),
            None => groups.push((folder, vec![h])),
        }
    }
    let width = out::columns(100);
    for (folder, list) in &groups {
        let missing = if ospath::isdir(folder) {
            String::new()
        } else {
            format!("  {}", p("31", "(directory missing)"))
        };
        out::println(&(paint.accent(&format!("cd {}", paths::shell_path(folder))) + &missing));
        for h in list {
            let s = &h.s;
            let mut meta: Vec<(String, String)> = Vec::new();
            if py::truthy(&s.branch) {
                meta.push((py::str_of(&s.branch), "2".into()));
            }
            if let Some(cost) = s.cost {
                meta.push((
                    sessions::dollars(cost),
                    sessions::cost_color(cost, paint.wide),
                ));
            }
            let entrypoint = s.entrypoint_text();
            if entrypoint != "cli" {
                meta.push((format!("via {entrypoint}"), "33".into()));
            }
            let room = width as i64
                - 8
                - meta
                    .iter()
                    .map(|(t, _)| 3 + t.chars().count() as i64)
                    .sum::<i64>();
            let tail: String = meta
                .iter()
                .map(|(text, c)| p("2", " · ") + &p(c, text))
                .collect();
            out::println(&format!(
                "  {}  {}{}",
                p("2", &format!("{:>4}", times::ago(now - s.when))),
                p("1", &truncate(&s.title, room.max(20) as usize)),
                tail
            ));
            let command = sessions::session_command(s, full);
            out::println(&format!(
                "{}{}",
                " ".repeat(8),
                sessions::paint_program(&command, s.agent, &paint)
            ));
            for (role, _, text) in &h.shown {
                let who = if role == "user" {
                    "You"
                } else {
                    s.agent.speaker()
                };
                let room = width.saturating_sub(8 + who.chars().count() + 2).max(20);
                let (shown, found) = excerpt(text, &query.lower, room);
                out::println(&format!(
                    "{}{} {}",
                    " ".repeat(8),
                    p("2", &format!("{who}:")),
                    painted(&shown, &found, &paint)
                ));
            }
            if h.turns > h.shown.len() {
                let rest = h.turns - h.shown.len();
                out::println(&format!(
                    "{}{}",
                    " ".repeat(8),
                    p("2", &format!("and {} more with them", count(rest, "turn")))
                ));
            }
        }
    }
    let mut notes = vec![format!(
        "{} in {}, of {searched} looked in",
        count(matched, "session"),
        count(matched_folders, "folder")
    )];
    if more > 0 {
        notes.push(format!(
            "the {} not shown, which --max 0 shows",
            if more == 1 {
                "oldest".to_string()
            } else {
                format!("{more} oldest")
            }
        ));
    }
    if hidden_total > 0 {
        notes.push(hidden_note());
    }
    out::println(&p("2", &(notes.join("; ") + ".")));
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(list: &[&str]) -> Vec<String> {
        list.iter().map(|w| w.to_string()).collect()
    }

    #[test]
    fn a_file_is_looked_through_in_either_case_across_its_chunks() {
        let dir = std::env::temp_dir().join(format!("hatcheck-search-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("t.jsonl");
        // A word split across the boundary between two chunks is still found.
        let mut text = "x".repeat(CHUNK - 3);
        text += "Rate-LIMITER\n{\"text\":\"tokens\"}\n";
        std::fs::write(&path, &text).unwrap();
        let p = path.to_string_lossy();
        let q = Query::new(&words(&["rate-limiter", "TOKENS", "bucket"]));
        assert_eq!(file_holds(&p, &q.raw), [true, true, false]);
        assert!(file_or_title_has(&p, "a token BUCKET", &q)); // the one missing is in the title
        assert!(!file_or_title_has(&p, "a token", &q));
        // Words JSON would write differently aren't asked of the bytes.
        let q = Query::new(&words(&["\"quoted\"", "naïve", "C:\\src"]));
        assert!(q.raw.is_empty());
        assert!(file_or_title_has(&p, "", &q));
        std::fs::write(&path, "").unwrap();
        assert_eq!(file_holds(&p, &Query::new(&words(&["x"])).raw), [false]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn every_word_has_to_be_in_the_title_or_a_turn() {
        let turn = |role: &str, text: &str| (role.to_string(), vec![text.to_string()], Value::Null);
        let turns = vec![
            turn("user", "Add a rate limiter to /login"),
            turn("assistant", "The limiter keeps a token bucket per IP"),
            turn("user", "and log what it drops"),
            turn("assistant", "Done: the RATE is 5 a second"),
        ];
        let q = Query::new(&words(&["Rate", "limiter"]));
        assert_eq!(
            matched("Login hardening", &turns, &q),
            Some((3, vec![0, 1]))
        );
        let q = Query::new(&words(&["bucket", "hardening"]));
        assert_eq!(matched("Login hardening", &turns, &q), Some((1, vec![1])));
        assert_eq!(matched("Login", &turns, &q), None);
    }

    #[test]
    fn an_excerpt_starts_before_the_first_word_and_marks_each() {
        let text = format!(
            "{} the rate limiter\nkeeps a token bucket {}",
            "word ".repeat(40),
            "per IP ".repeat(20)
        );
        let (shown, found) = excerpt(&text, &words(&["rate", "limiter"]), 60);
        assert!(shown.starts_with('…') && shown.ends_with('…'), "{shown}");
        assert!(shown.chars().count() <= 60, "{shown}");
        assert!(shown.contains("the rate limiter keeps a token"), "{shown}"); // whitespace folded
        let chars: Vec<char> = shown.chars().collect();
        let marked: Vec<String> = found
            .iter()
            .map(|(a, b)| chars[*a..*b].iter().collect())
            .collect();
        assert_eq!(marked, ["rate", "limiter"]);
        // A short turn is shown whole.
        let (shown, found) = excerpt("Fix the RATE limiter", &words(&["rate"]), 60);
        assert_eq!(
            (shown.as_str(), found),
            ("Fix the RATE limiter", vec![(8, 12)])
        );
        // Case folding that changes a character's length still marks the right characters.
        let (shown, found) = excerpt("İstanbul rate", &words(&["rate"]), 60);
        let chars: Vec<char> = shown.chars().collect();
        assert_eq!(
            chars[found[0].0..found[0].1].iter().collect::<String>(),
            "rate"
        );
    }

    #[test]
    fn the_words_are_named_as_a_list() {
        assert_eq!(Query::new(&words(&["rate"])).named(), "\"rate\"");
        assert_eq!(
            Query::new(&words(&["a", "b", "c"])).named(),
            "\"a\", \"b\" and \"c\""
        );
    }
}
