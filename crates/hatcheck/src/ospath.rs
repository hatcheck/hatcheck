//! Python's os.path, as far as hatcheck uses it, on strings: paths are joined, split and
//! normalized the way the Python code did, so they print the same. POSIX follows
//! posixpath exactly; Windows follows ntpath for drives, both separators and case.
//! Also glob.glob for the patterns hatcheck uses: a literal part or one `*` per component.

use std::fs;

pub const WINDOWS: bool = cfg!(windows);
pub const SEP: &str = if WINDOWS { "\\" } else { "/" };

fn is_sep(c: char) -> bool {
    c == '/' || (WINDOWS && c == '\\')
}

/// The drive of a Windows path (C: or \\server\share), and the rest; nothing elsewhere.
pub fn splitdrive(p: &str) -> (&str, &str) {
    if !WINDOWS {
        return ("", p);
    }
    let b = p.as_bytes();
    if b.len() >= 2 && b[1] == b':' && b[0].is_ascii_alphabetic() {
        return p.split_at(2);
    }
    if b.len() >= 2
        && is_sep(b[0] as char)
        && is_sep(b[1] as char)
        && !(b.len() > 2 && is_sep(b[2] as char))
    {
        // \\server\share: the drive runs to the end of the share's name.
        let rest = &p[2..];
        if let Some(i) = rest.find(is_sep) {
            let after = &rest[i + 1..];
            let j = after.find(is_sep).unwrap_or(after.len());
            let end = 2 + i + 1 + j;
            return p.split_at(end);
        }
        return (p, "");
    }
    ("", p)
}

pub fn isabs(p: &str) -> bool {
    if WINDOWS {
        // As ntpath says since Python 3.13: a drive and a root, or a UNC path.
        let b = p.as_bytes();
        (b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && is_sep(b[2] as char))
            || (b.len() >= 2 && is_sep(b[0] as char) && is_sep(b[1] as char))
    } else {
        p.starts_with('/')
    }
}

/// os.path.join(a, b).
pub fn join(a: &str, b: &str) -> String {
    if WINDOWS {
        let (bd, brest) = splitdrive(b);
        if !bd.is_empty() {
            return b.to_string();
        }
        if brest.starts_with(is_sep) {
            return format!("{}{}", splitdrive(a).0, b);
        }
        if a.is_empty() || a.ends_with(is_sep) || (a.len() == 2 && a.ends_with(':')) {
            return format!("{a}{b}");
        }
        return format!("{a}\\{b}");
    }
    if b.starts_with('/') {
        b.to_string()
    } else if a.is_empty() || a.ends_with('/') {
        format!("{a}{b}")
    } else {
        format!("{a}/{b}")
    }
}

/// os.path.join(a, *parts).
pub fn join_all(a: &str, parts: &[&str]) -> String {
    parts.iter().fold(a.to_string(), |acc, p| join(&acc, p))
}

/// os.path.split(p).
pub fn split(p: &str) -> (String, String) {
    let (drive, rest) = splitdrive(p);
    let i = rest.rfind(is_sep).map_or(0, |i| i + 1);
    let (head, tail) = rest.split_at(i);
    let head = if !head.is_empty() && !head.chars().all(is_sep) {
        head.trim_end_matches(is_sep)
    } else {
        head
    };
    (format!("{drive}{head}"), tail.to_string())
}

pub fn dirname(p: &str) -> String {
    split(p).0
}

pub fn basename(p: &str) -> String {
    split(p).1
}

/// os.path.normpath(p).
pub fn normpath(p: &str) -> String {
    if p.is_empty() {
        return ".".into();
    }
    let (drive, rest) = splitdrive(p);
    let sep = if WINDOWS { '\\' } else { '/' };
    let rest_owned: String = if WINDOWS {
        rest.replace('/', "\\")
    } else {
        rest.to_string()
    };
    let rest = rest_owned.as_str();
    let mut initial = if rest.starts_with(sep) { 1 } else { 0 };
    if !WINDOWS && initial == 1 && rest.starts_with("//") && !rest.starts_with("///") {
        initial = 2;
    }
    let mut comps: Vec<&str> = Vec::new();
    for comp in rest.split(sep) {
        if comp.is_empty() || comp == "." {
            continue;
        }
        if comp != ".." || (initial == 0 && comps.is_empty()) || comps.last() == Some(&"..") {
            comps.push(comp);
        } else if !comps.is_empty() {
            comps.pop();
        }
    }
    let body = comps.join(&sep.to_string());
    let out = format!("{drive}{}{body}", sep.to_string().repeat(initial));
    if out.is_empty() {
        ".".into()
    } else {
        out
    }
}

pub fn cwd() -> String {
    std::env::current_dir()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// os.path.abspath(p).
pub fn abspath(p: &str) -> String {
    if isabs(p) {
        normpath(p)
    } else {
        normpath(&join(&cwd(), p))
    }
}

/// A canonical path from the system, as Python shows it: without Windows' \\?\ prefix.
fn canonical(p: &str) -> Option<String> {
    let c = fs::canonicalize(p).ok()?;
    let s = c.to_string_lossy().into_owned();
    if WINDOWS {
        if let Some(unc) = s.strip_prefix("\\\\?\\UNC\\") {
            return Some(format!("\\\\{unc}"));
        }
        if let Some(local) = s.strip_prefix("\\\\?\\") {
            return Some(local.to_string());
        }
    }
    Some(s)
}

/// os.path.realpath(p): symlinks resolved as far as the path exists, the rest kept.
pub fn realpath(p: &str) -> String {
    let full = abspath(p);
    if let Some(c) = canonical(&full) {
        return c;
    }
    // Resolve the longest part that exists and keep what follows as it is.
    let mut head = full.clone();
    let mut tail: Vec<String> = Vec::new();
    loop {
        let (parent, name) = split(&head);
        if name.is_empty() || parent == head {
            return full;
        }
        tail.push(name);
        if let Some(c) = canonical(&parent) {
            tail.reverse();
            return normpath(&join_all(
                &c,
                &tail.iter().map(String::as_str).collect::<Vec<_>>(),
            ));
        }
        head = parent;
    }
}

/// os.path.normcase(p): lower case with backslashes on Windows, unchanged elsewhere.
pub fn normcase(p: &str) -> String {
    if WINDOWS {
        p.replace('/', "\\").to_lowercase()
    } else {
        p.to_string()
    }
}

/// os.path.expanduser(p), for ~ and ~/... (not ~user).
pub fn expanduser(p: &str) -> String {
    if p == "~" || p.starts_with("~/") || (WINDOWS && p.starts_with("~\\")) {
        return format!("{}{}", crate::paths::home(), &p[1..]);
    }
    p.to_string()
}

pub fn isdir(p: &str) -> bool {
    fs::metadata(p).map(|m| m.is_dir()).unwrap_or(false)
}

pub fn isfile(p: &str) -> bool {
    fs::metadata(p).map(|m| m.is_file()).unwrap_or(false)
}

pub fn exists(p: &str) -> bool {
    fs::metadata(p).is_ok()
}

pub fn lexists(p: &str) -> bool {
    fs::symlink_metadata(p).is_ok()
}

/// os.path.getmtime(p), as a float.
pub fn getmtime(p: &str) -> std::io::Result<f64> {
    let m = fs::metadata(p)?.modified()?;
    Ok(match m.duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => d.as_secs_f64(),
        Err(e) => -e.duration().as_secs_f64(),
    })
}

/// One component of a glob pattern: a name, or `prefix*suffix`.
#[derive(Clone, Copy)]
pub enum Part<'a> {
    Name(&'a str),
    Wild(&'a str, &'a str),
}

impl Part<'_> {
    fn matches(&self, name: &str) -> bool {
        match *self {
            Part::Name(n) => fold(n) == fold(name),
            Part::Wild(prefix, suffix) => {
                // As glob does: a pattern that doesn't start with a dot skips hidden names.
                if !prefix.starts_with('.') && name.starts_with('.') {
                    return false;
                }
                let (name, prefix, suffix) = (fold(name), fold(prefix), fold(suffix));
                name.len() >= prefix.len() + suffix.len()
                    && name.starts_with(&prefix)
                    && name.ends_with(&suffix)
            }
        }
    }
}

/// Names compare without case on Windows, as glob's fnmatch compares them there.
fn fold(s: &str) -> String {
    if WINDOWS {
        s.to_lowercase()
    } else {
        s.to_string()
    }
}

/// glob.glob(os.path.join(base, *parts)) for parts made of names and `prefix*suffix`
/// patterns, in the order the directory lists them (callers sort when it matters).
pub fn glob(base: &str, parts: &[Part]) -> Vec<String> {
    let mut found = vec![base.to_string()];
    for (i, part) in parts.iter().enumerate() {
        let last = i == parts.len() - 1;
        let mut next = Vec::new();
        for dir in &found {
            match part {
                Part::Name(name) => {
                    let path = join(dir, name);
                    if if last { lexists(&path) } else { isdir(&path) } {
                        next.push(path);
                    }
                }
                Part::Wild(..) => {
                    let Ok(entries) = fs::read_dir(if dir.is_empty() { "." } else { dir }) else {
                        continue;
                    };
                    for entry in entries.flatten() {
                        let name = entry.file_name().to_string_lossy().into_owned();
                        if !part.matches(&name) {
                            continue;
                        }
                        let path = join(dir, &name);
                        // Inner components must be directories (or links to them).
                        if last || isdir(&path) {
                            next.push(path);
                        }
                    }
                }
            }
        }
        found = next;
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(not(windows))]
    fn posix_paths() {
        assert_eq!(join("/a", "b"), "/a/b");
        assert_eq!(join("/a/", "b"), "/a/b");
        assert_eq!(join("/a", "/b"), "/b");
        assert_eq!(split("/a/b/c.jsonl"), ("/a/b".into(), "c.jsonl".into()));
        assert_eq!(split("/a"), ("/".into(), "a".into()));
        assert_eq!(split("a"), ("".into(), "a".into()));
        assert_eq!(dirname("/a/b/"), "/a/b");
        assert_eq!(normpath("/a/./b/../c//d/"), "/a/c/d");
        assert_eq!(normpath("//a"), "//a");
        assert_eq!(normpath("../a"), "../a");
        assert_eq!(normpath(""), ".");
    }
}
