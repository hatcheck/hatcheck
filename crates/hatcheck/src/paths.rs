//! Where hatcheck finds things, and how it names them in commands: the home folder, Claude
//! Code's profiles (~/.claude, ~/.claude-NAME) and Codex's homes (~/.codex, ~/.codex-NAME),
//! paths as a shell should get them, the commands that run or resume Claude Code and Codex
//! with a profile, and the folders bound to profiles (hatcheck bind).

use crate::ospath::{self, Part, SEP};
use crate::py;
use serde_json::Value;

/// The home folder: os.path.expanduser("~").
pub fn home() -> String {
    if ospath::WINDOWS {
        if let Ok(p) = std::env::var("USERPROFILE") {
            return p;
        }
        let (drive, path) = (
            std::env::var("HOMEDRIVE").unwrap_or_default(),
            std::env::var("HOMEPATH").unwrap_or_default(),
        );
        return ospath::join(&drive, &path);
    }
    let home = std::env::var("HOME")
        .ok()
        .or_else(|| std::env::home_dir().map(|p| p.to_string_lossy().into_owned()))
        .unwrap_or_default();
    let trimmed = home.trim_end_matches('/');
    if trimmed.is_empty() {
        "/".into()
    } else {
        trimmed.to_string()
    }
}

pub fn default_profile() -> String {
    ospath::realpath(&ospath::join(&home(), ".claude"))
}

pub fn default_codex_home() -> String {
    ospath::realpath(&ospath::join(&home(), ".codex"))
}

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

/// Every Claude Code config dir with sessions: ~/.claude* dirs (and $CLAUDE_CONFIG_DIR) that
/// hold a projects/ folder, ~/.claude first, the rest by name.
pub fn find_profiles() -> Vec<String> {
    let mut candidates = ospath::glob(&home(), &[Part::Wild(".claude", "")]);
    if let Some(dir) = env("CLAUDE_CONFIG_DIR") {
        candidates.push(ospath::expanduser(&dir));
    }
    let mut seen: Vec<(String, String)> = Vec::new(); // (realpath, abspath), the first of each
    for d in candidates {
        if ospath::isdir(&ospath::join(&d, "projects")) {
            let real = ospath::realpath(&d);
            if !seen.iter().any(|(r, _)| *r == real) {
                seen.push((real, ospath::abspath(&d)));
            }
        }
    }
    let default = default_profile();
    let mut profiles: Vec<(bool, String)> =
        seen.into_iter().map(|(r, a)| (r != default, a)).collect();
    profiles.sort();
    profiles.into_iter().map(|(_, a)| a).collect()
}

/// Every Codex home with sessions: ~/.codex and ~/.codex-* dirs (and $CODEX_HOME) that hold
/// a sessions/ folder, ~/.codex first.
pub fn find_codex_homes() -> Vec<String> {
    let h = home();
    let mut candidates = ospath::glob(&h, &[Part::Name(".codex")]);
    candidates.extend(ospath::glob(&h, &[Part::Wild(".codex-", "")]));
    if let Some(dir) = env("CODEX_HOME") {
        candidates.push(ospath::expanduser(&dir));
    }
    let mut seen: Vec<(String, String)> = Vec::new();
    for d in candidates {
        if ospath::isdir(&ospath::join(&d, "sessions")) {
            let real = ospath::realpath(&d);
            if !seen.iter().any(|(r, _)| *r == real) {
                seen.push((real, ospath::abspath(&d)));
            }
        }
    }
    let default = default_codex_home();
    let mut homes: Vec<(bool, String)> = seen.into_iter().map(|(r, a)| (r != default, a)).collect();
    homes.sort();
    homes.into_iter().map(|(_, a)| a).collect()
}

/// The ID in a Codex session file's name, rollout-<local start time>-<ID>.jsonl, at the end
/// of a path, as the Python version's CODEX_FILE pattern found it.
pub fn codex_file_id(path: &str) -> Option<&str> {
    let stem = path.strip_suffix(".jsonl")?;
    if stem.len() < 36 + 1 + 19 + 8 {
        return None;
    }
    let (rest, id) = stem.split_at(stem.len() - 36);
    let hex = |s: &str| {
        s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    };
    let groups: Vec<&str> = id.split('-').collect();
    if groups.len() != 5
        || groups.iter().map(|g| g.len()).collect::<Vec<_>>() != [8, 4, 4, 4, 12]
        || !groups.iter().all(|g| hex(g))
    {
        return None;
    }
    let rest = rest.strip_suffix('-')?;
    let (rest, stamp) = rest.split_at(rest.len().checked_sub(19)?);
    let b = stamp.as_bytes();
    let digit = |i: usize| b[i].is_ascii_digit();
    let shape = (0..4).all(digit)
        && b[4] == b'-'
        && (5..7).all(digit)
        && b[7] == b'-'
        && (8..10).all(digit)
        && b[10] == b'T'
        && (11..13).all(digit)
        && b[13] == b'-'
        && (14..16).all(digit)
        && b[16] == b'-'
        && (17..19).all(digit);
    (shape && rest.ends_with("rollout-")).then_some(id)
}

/// The session files of a Codex home, archived ones left out.
pub fn codex_session_files(home: &str) -> Vec<String> {
    ospath::glob(
        home,
        &[
            Part::Name("sessions"),
            Part::Wild("", ""),
            Part::Wild("", ""),
            Part::Wild("", ""),
            Part::Wild("rollout-", ".jsonl"),
        ],
    )
    .into_iter()
    .filter(|p| codex_file_id(p).is_some())
    .collect()
}

/// Whether to print commands for PowerShell: on Windows, unless this runs in a POSIX shell
/// such as Git Bash, which sets SHELL.
pub fn powershell() -> bool {
    ospath::WINDOWS && env("SHELL").is_none()
}

/// Needs no quotes in a PowerShell command.
fn ps_safe(text: &str) -> bool {
    !text.is_empty()
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._~:\\/-".contains(c))
}

pub fn ps_quote(text: &str) -> String {
    if ps_safe(text) && !text.starts_with('-') {
        text.to_string()
    } else {
        format!("'{}'", text.replace('\'', "''"))
    }
}

/// shlex.quote(text).
pub fn sh_quote(text: &str) -> String {
    if text.is_empty() {
        return "''".into();
    }
    if text
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "_@%+=:,./-".contains(c))
    {
        return text.to_string();
    }
    format!("'{}'", text.replace('\'', "'\"'\"'"))
}

/// How paths and commands are printed: with this computer's home folder, path style and shell,
/// or, in tests, another computer's, so that Windows and PowerShell are checked on any OS.
pub struct Shell {
    home: String,
    windows: bool,
    powershell: bool,
    /// For tests: profiles are compared with ~/.claude and ~/.codex as written, not resolved
    /// on this computer's disk.
    as_written: bool,
}

impl Shell {
    pub fn here() -> Shell {
        Shell {
            home: home(),
            windows: ospath::WINDOWS,
            powershell: powershell(),
            as_written: false,
        }
    }

    fn sep(&self) -> &'static str {
        if self.windows {
            "\\"
        } else {
            "/"
        }
    }

    fn is_sep(&self, c: char) -> bool {
        c == '/' || (self.windows && c == '\\')
    }

    /// A word for the shell commands are printed for: PowerShell's quoting there, else sh's.
    pub fn quote(&self, text: &str) -> String {
        if self.powershell {
            ps_quote(text)
        } else {
            sh_quote(text)
        }
    }

    /// A path to print in a command, as ~/... when it's in the home folder.
    pub fn shell_path(&self, path: &str) -> String {
        if path == self.home {
            return "~".into();
        }
        let prefix = format!("{}{}", self.home, self.sep());
        let rest = path.strip_prefix(&prefix);
        if self.powershell {
            return match rest {
                Some(r) if ps_safe(r) => format!("~\\{r}"),
                _ => ps_quote(path),
            };
        }
        if self.windows {
            // A POSIX shell on Windows reads forward slashes.
            let path = path.replace('\\', "/");
            return match rest {
                Some(r) => format!("~/{}", sh_quote(&r.replace('\\', "/"))),
                None => sh_quote(&path),
            };
        }
        match rest {
            Some(r) => format!("~/{}", sh_quote(r)),
            None => sh_quote(path),
        }
    }

    /// $HOME\rest for a PowerShell command, or the path quoted, for CLAUDE_CONFIG_DIR or CODEX_HOME.
    fn ps_dir(&self, path: &str) -> String {
        match path.strip_prefix(&format!("{}{}", self.home, self.sep())) {
            Some(rest) if !rest.contains(['"', '$', '`']) => format!("\"$HOME\\{rest}\""),
            _ => format!("'{}'", path.replace('\'', "''")),
        }
    }

    /// The folder and name of a profile path, as os.path.split(path.rstrip("/" + os.sep)) gives them.
    fn parent_and_name(&self, path: &str) -> (String, String) {
        let path = path.trim_end_matches(|c| self.is_sep(c));
        if self.windows == ospath::WINDOWS {
            return ospath::split(path);
        }
        // Another computer's paths, in tests: ntpath's or posixpath's split, for drive letters.
        let b = path.as_bytes();
        let (drive, rest) = if self.windows && b.len() >= 2 && b[1] == b':' {
            path.split_at(2)
        } else {
            ("", path)
        };
        let i = rest.rfind(|c| self.is_sep(c)).map_or(0, |i| i + 1);
        let (head, tail) = rest.split_at(i);
        let trimmed = head.trim_end_matches(|c| self.is_sep(c));
        let head = if trimmed.is_empty() { head } else { trimmed };
        (format!("{drive}{head}"), tail.to_string())
    }

    /// Whether path is ~/<name>, the folder plain claude or codex uses.
    fn is_default(&self, path: &str, name: &str) -> bool {
        if self.as_written {
            return path == format!("{}{}{name}", self.home, self.sep());
        }
        ospath::realpath(path) == ospath::realpath(&ospath::join(&self.home, name))
    }

    /// The command that runs Claude Code with a profile, then tail: claude for ~/.claude,
    /// cl <name> for ~/.claude-<name>, claude-z or claude-k on Z.ai or Kimi.
    pub fn claude_command(
        &self,
        profile: &str,
        tail: &str,
        full: bool,
        provider: Option<&str>,
    ) -> String {
        let command = provider.map_or("claude".to_string(), |p| format!("claude-{p}"));
        let tail = if tail.is_empty() {
            String::new()
        } else {
            format!(" {tail}")
        };
        if self.is_default(profile, ".claude") {
            return format!("{command}{tail}");
        }
        let (parent, name) = self.parent_and_name(profile);
        if !full && parent == self.home {
            if let Some(short) = name.strip_prefix(".claude-") {
                let flag = provider.map_or(String::new(), |p| format!("-{p} "));
                return format!("cl {flag}{}{tail}", self.quote(short));
            }
        }
        if self.powershell {
            // Set for this one command and cleared after, so plain claude in the same window
            // goes back to ~\.claude.
            return format!(
                "$env:CLAUDE_CONFIG_DIR={}; {command}{tail}; $env:CLAUDE_CONFIG_DIR=$null",
                self.ps_dir(profile)
            );
        }
        format!(
            "CLAUDE_CONFIG_DIR={} {command}{tail}",
            self.shell_path(profile)
        )
    }

    /// The command that runs Codex with a home, then tail: codex for ~/.codex, cx <name> for
    /// ~/.codex-<name>.
    pub fn codex_command(&self, home_dir: &str, tail: &str, full: bool) -> String {
        let tail = if tail.is_empty() {
            String::new()
        } else {
            format!(" {tail}")
        };
        if self.is_default(home_dir, ".codex") {
            return format!("codex{tail}");
        }
        let (parent, name) = self.parent_and_name(home_dir);
        if !full && parent == self.home {
            if let Some(short) = name.strip_prefix(".codex-") {
                return format!("cx {}{tail}", self.quote(short));
            }
        }
        if self.powershell {
            return format!(
                "$env:CODEX_HOME={}; codex{tail}; $env:CODEX_HOME=$null",
                self.ps_dir(home_dir)
            );
        }
        format!("CODEX_HOME={} codex{tail}", self.shell_path(home_dir))
    }
}

/// A word for the shell commands are printed for: PowerShell's quoting there, else sh's.
pub fn quote(text: &str) -> String {
    Shell::here().quote(text)
}

/// A path to print in a command, as ~/... when it's in the home folder.
pub fn shell_path(path: &str) -> String {
    Shell::here().shell_path(path)
}

/// z or k when the model is served by Z.ai (GLM) or Kimi, else None.
pub fn provider_of(model: Option<&str>) -> Option<&'static str> {
    let model = model.unwrap_or("").to_lowercase();
    if model.starts_with("glm") {
        Some("z")
    } else if model.starts_with("k2") || model.starts_with("k3") || model.starts_with("kimi") {
        Some("k")
    } else {
        None
    }
}

pub fn claude_command(profile: &str, tail: &str, full: bool, provider: Option<&str>) -> String {
    Shell::here().claude_command(profile, tail, full, provider)
}

pub fn resume_command(
    profile: &str,
    session_id: &str,
    full: bool,
    provider: Option<&str>,
) -> String {
    claude_command(profile, &format!("--resume {session_id}"), full, provider)
}

pub fn codex_command(home_dir: &str, tail: &str, full: bool) -> String {
    Shell::here().codex_command(home_dir, tail, full)
}

pub fn codex_resume_command(home_dir: &str, session_id: &str, full: bool) -> String {
    codex_command(home_dir, &format!("resume {session_id}"), full)
}

/// The command that starts Claude Code or Codex with a profile or home.
pub fn start_command(agent: &str, profile: &str, full: bool) -> String {
    if agent == "codex" {
        codex_command(profile, "", full)
    } else {
        claude_command(profile, "", full, None)
    }
}

/// Claude Code's name for a project folder: the cwd with every character that isn't a
/// letter or digit made a dash.
pub fn encode_project(path: &str) -> String {
    path.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

/// The directory `claude --resume` must run from: the one that maps to the transcript's folder.
pub fn resume_dir(folder: &str, candidates: &[Option<&str>]) -> Option<String> {
    let mut paths: Vec<String> = Vec::new();
    for c in candidates.iter().flatten() {
        if !c.is_empty() {
            paths.push(c.to_string());
            paths.push(ospath::realpath(c));
        }
    }
    for p in &paths {
        let enc = encode_project(p);
        if folder == enc
            || (enc.chars().count() > 200
                && folder.starts_with(&enc.chars().take(200).collect::<String>()))
        {
            return Some(p.clone());
        }
    }
    paths.into_iter().next()
}

pub fn same_folder(a: &str, b: &str) -> bool {
    ospath::normcase(&ospath::realpath(a)) == ospath::normcase(&ospath::realpath(b))
}

// Folder bindings (hatcheck bind): which profile and Codex home a folder's work belongs to.

/// ~/.config/hatcheck/bindings.json (%APPDATA%\hatcheck\bindings.json on Windows).
pub fn bindings_path() -> String {
    let base = if ospath::WINDOWS {
        env("APPDATA")
    } else {
        None
    };
    ospath::join_all(
        &base.unwrap_or_else(|| ospath::join(&home(), ".config")),
        &["hatcheck", "bindings.json"],
    )
}

/// Bound folder -> {"claude": name, "codex": name}.
pub fn load_bindings() -> py::Dict {
    let Ok(text) = std::fs::read_to_string(bindings_path()) else {
        return py::Dict::new();
    };
    let Ok(data) = serde_json::from_str::<Value>(&text) else {
        return py::Dict::new();
    };
    match py::get(&data, "folders") {
        Value::Object(folders) => folders
            .iter()
            .filter(|(_, b)| b.is_object())
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
        _ => py::Dict::new(),
    }
}

/// The folder a bound name stands for: default for ~/.claude or ~/.codex, NAME for
/// ~/.claude-NAME or ~/.codex-NAME, or a path as it is.
pub fn profile_folder(agent: &str, name: &str) -> String {
    let base = if agent == "codex" {
        ".codex"
    } else {
        ".claude"
    };
    if name == "default" {
        return ospath::join(&home(), base);
    }
    if name.contains('/') || name.contains(SEP) {
        return name.to_string();
    }
    ospath::join(&home(), &format!("{base}-{name}"))
}

/// The deepest bound folder that holds path, path itself included.
pub fn bound_folder(path: Option<&str>, bindings: &py::Dict) -> Option<String> {
    let path = path.filter(|p| !p.is_empty())?;
    if bindings.is_empty() {
        return None;
    }
    let keys: Vec<(String, &String)> = bindings
        .keys()
        .map(|f| (ospath::normcase(&ospath::realpath(f)), f))
        .collect();
    let mut path = ospath::normcase(&ospath::realpath(path));
    loop {
        if let Some((_, folder)) = keys.iter().rev().find(|(k, _)| *k == path) {
            return Some((*folder).clone());
        }
        let parent = ospath::dirname(&path);
        if parent == path {
            return None;
        }
        path = parent;
    }
}

/// The profile (or Codex home) the folder at path is bound to.
pub fn bound_profile(path: Option<&str>, agent: &str, bindings: &py::Dict) -> Option<String> {
    let folder = bound_folder(path, bindings)?;
    let name = py::s(py::get(bindings.get(&folder)?, agent))?;
    (!name.is_empty()).then(|| profile_folder(agent, name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_file_names() {
        let id = "019f0000-aaaa-7000-8000-000000000001";
        assert_eq!(
            codex_file_id(&format!(
                "/h/.codex/sessions/2026/10/17/rollout-2026-10-17T03-35-35-{id}.jsonl"
            )),
            Some(id)
        );
        assert_eq!(
            codex_file_id(&format!("/h/rollout-2026-10-17T03-35-35-{id}.json")),
            None
        );
        assert_eq!(
            codex_file_id(&format!("/h/rollout-2026-10-17-03-35-35-{id}.jsonl")),
            None
        );
    }

    #[test]
    fn shell_words() {
        assert_eq!(sh_quote("work"), "work");
        assert_eq!(sh_quote("my work"), "'my work'");
        assert_eq!(sh_quote("it's"), "'it'\"'\"'s'");
        assert_eq!(sh_quote(""), "''");
        assert_eq!(ps_quote("--fork"), "'--fork'");
        assert_eq!(encode_project("/Users/me/code/api"), "-Users-me-code-api");
        assert_eq!(provider_of(Some("GLM-5.3")), Some("z"));
        assert_eq!(provider_of(Some("k3")), Some("k"));
        assert_eq!(provider_of(Some("claude-opus-5-5")), None);
    }

    const ID: &str = "abcdef12-0000-4000-8000-000000000001";
    const CODEX_ID: &str = "019f0000-aaaa-7000-8000-000000000001";

    /// A Windows computer's printing, PowerShell's or (powershell false) Git Bash's.
    fn windows(powershell: bool) -> Shell {
        Shell {
            home: "C:\\Users\\me".into(),
            windows: true,
            powershell,
            as_written: true,
        }
    }

    #[test]
    fn powershell_paths_and_commands() {
        let s = windows(true);
        assert_eq!(s.shell_path("C:\\Users\\me"), "~");
        assert_eq!(s.shell_path("C:\\Users\\me\\code\\api"), "~\\code\\api");
        assert_eq!(
            s.shell_path("C:\\Users\\me\\My Code"),
            "'C:\\Users\\me\\My Code'"
        ); // quoted: no ~
        assert_eq!(s.shell_path("D:\\work\\api"), "D:\\work\\api");
        assert_eq!(s.shell_path("D:\\it's"), "'D:\\it''s'");
        let resume = |profile: &str, full: bool, provider: Option<&str>| {
            s.claude_command(profile, &format!("--resume {ID}"), full, provider)
        };
        assert_eq!(
            resume("C:\\Users\\me\\.claude", false, None),
            format!("claude --resume {ID}")
        );
        assert_eq!(
            resume("C:\\Users\\me\\.claude-work", false, None),
            format!("cl work --resume {ID}")
        );
        assert_eq!(
            resume("C:\\Users\\me\\.claude-work", true, None),
            format!("$env:CLAUDE_CONFIG_DIR=\"$HOME\\.claude-work\"; claude --resume {ID}; $env:CLAUDE_CONFIG_DIR=$null")
        );
        assert_eq!(
            resume("D:\\p\\.claude-x", false, None),
            format!("$env:CLAUDE_CONFIG_DIR='D:\\p\\.claude-x'; claude --resume {ID}; $env:CLAUDE_CONFIG_DIR=$null")
        );
        // On Z.ai or Kimi
        assert_eq!(
            resume("C:\\Users\\me\\.claude", false, Some("z")),
            format!("claude-z --resume {ID}")
        );
        assert_eq!(
            resume("C:\\Users\\me\\.claude-work", false, Some("k")),
            format!("cl -k work --resume {ID}")
        );
        assert_eq!(
            resume("C:\\Users\\me\\.claude-work", true, Some("k")),
            format!("$env:CLAUDE_CONFIG_DIR=\"$HOME\\.claude-work\"; claude-k --resume {ID}; $env:CLAUDE_CONFIG_DIR=$null")
        );
    }

    #[test]
    fn posix_shell_on_windows() {
        // Git Bash: forward slashes, sh's quoting
        let s = windows(false);
        assert_eq!(s.shell_path("C:\\Users\\me\\code\\api"), "~/code/api");
        assert_eq!(s.shell_path("D:\\x y"), "'D:/x y'");
        assert_eq!(
            s.claude_command(
                "C:\\Users\\me\\.claude-work",
                &format!("--resume {ID}"),
                true,
                None
            ),
            format!("CLAUDE_CONFIG_DIR=~/.claude-work claude --resume {ID}")
        );
        assert_eq!(
            s.claude_command(
                "C:\\Users\\me\\.claude-work",
                &format!("--resume {ID}"),
                true,
                Some("z")
            ),
            format!("CLAUDE_CONFIG_DIR=~/.claude-work claude-z --resume {ID}")
        );
        assert_eq!(
            s.codex_command(
                "C:\\Users\\me\\.codex-work",
                &format!("resume {CODEX_ID}"),
                true
            ),
            format!("CODEX_HOME=~/.codex-work codex resume {CODEX_ID}")
        );
        assert_eq!(s.quote("my profile"), "'my profile'");
    }

    #[test]
    fn codex_commands_on_windows() {
        let s = windows(true);
        let resume =
            |home: &str, full: bool| s.codex_command(home, &format!("resume {CODEX_ID}"), full);
        assert_eq!(
            resume("C:\\Users\\me\\.codex", false),
            format!("codex resume {CODEX_ID}")
        );
        assert_eq!(
            resume("C:\\Users\\me\\.codex-work", false),
            format!("cx work resume {CODEX_ID}")
        );
        assert_eq!(
            resume("C:\\Users\\me\\.codex-work", true),
            format!("$env:CODEX_HOME=\"$HOME\\.codex-work\"; codex resume {CODEX_ID}; $env:CODEX_HOME=$null")
        );
        assert_eq!(
            resume("D:\\p\\.codex-x", false),
            format!(
                "$env:CODEX_HOME='D:\\p\\.codex-x'; codex resume {CODEX_ID}; $env:CODEX_HOME=$null"
            )
        );
        assert_eq!(s.quote("--fork"), "'--fork'"); // which is why move's own flags aren't quoted
    }

    #[test]
    fn posix_paths_and_commands() {
        let s = Shell {
            home: "/home/me".into(),
            windows: false,
            powershell: false,
            as_written: true,
        };
        assert_eq!(s.shell_path("/home/me/code/api"), "~/code/api");
        assert_eq!(s.shell_path("/home/me/My Code"), "~/'My Code'");
        assert_eq!(s.shell_path("/srv/x"), "/srv/x");
        assert_eq!(
            s.claude_command("/home/me/.claude", "", false, None),
            "claude"
        );
        assert_eq!(
            s.claude_command("/home/me/.claude-my work", "", false, None),
            "cl 'my work'"
        );
        assert_eq!(
            s.claude_command("/srv/p/.claude-x", "--resume 1", false, None),
            "CLAUDE_CONFIG_DIR=/srv/p/.claude-x claude --resume 1"
        );
        assert_eq!(
            s.codex_command("/home/me/.codex-work/", "", false),
            "cx work"
        ); // a trailing slash
    }
}
