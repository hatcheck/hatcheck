//! hatcheck usage [--cached] [--email] [--json]: how much of each plan limit is left, for
//! every Claude Code profile (~/.claude*) and every Codex home (~/.codex*); the session list
//! uses it too. docs/commands.md's `hatcheck usage` section says what it shows.
//!
//! A Claude Code profile's numbers come from Claude Code itself: `claude -p /usage` is run in
//! the profile, which asks Anthropic with the profile's own login and saves the answer in the
//! profile's .claude.json (cachedUsageUtilization), and that snapshot is read, or read as it is
//! when under a minute old, which Claude Code's /usage shows again without asking. hatcheck
//! never reads a Claude login. When that gives nothing new (claude not on the PATH, the profile not
//! signed in, offline) the last snapshot saved is used, with its age. A Codex home's ChatGPT
//! token goes to the endpoint Codex's /status asks, or else the newest limits Codex wrote in
//! the home's session files are used.

use crate::agent::Agent;
use crate::args::Parser;
use crate::ospath::{self, Part};
use crate::out::{self, Paint};
use crate::paths;
use crate::py::{self, get, s, truthy, Num};
use crate::times;
use base64::Engine;
use serde_json::{json, Map, Value};
use std::time::Duration;

pub const CODEX_USAGE_URL: &str = "https://chatgpt.com/backend-api/wham/usage";
/// What Claude Code's /usage is run with: no tools, no MCP servers, no settings or hooks, and
/// no transcript, so it only asks Anthropic what's left and saves the answer in the profile.
pub const CLAUDE_USAGE_ARGS: [&str; 10] = [
    "-p",
    "/usage",
    "--output-format",
    "json",
    "--tools",
    "",
    "--strict-mcp-config",
    "--setting-sources",
    "",
    "--no-session-persistence",
];
/// Variables that run of Claude Code must not get from hatcheck's own environment: every
/// ANTHROPIC_* one (a Z.ai or Kimi key would send /usage there), the marks of a Claude Code
/// session hatcheck may itself be running in, and the setting that has /usage skip its request.
const CLAUDE_USAGE_UNSET: [&str; 5] = [
    "CLAUDECODE",
    "CLAUDE_CODE_ENTRYPOINT",
    "CLAUDE_CODE_SSE_PORT",
    "CLAUDE_CODE_OAUTH_TOKEN",
    "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC",
];
/// How long that run may take before it's given up on.
const CLAUDE_USAGE_TIMEOUT: Duration = Duration::from_secs(45);
/// Claude Code's /usage shows the profile's snapshot again while it's under a minute old, with
/// no request (`nt=60000` beside `cachedUsageUtilization` in 2.1.307's binary: five runs a
/// minute after the last left their snapshots where they were, and one 139 seconds after moved
/// it, 2026-10-21), so a run within the minute isn't made: the snapshot is what it would show.
const CLAUDE_REUSES: f64 = 60.0;
const CLAUDE_REUSE_NOTE: &str = "Claude Code's /usage reuses a reading under a minute old";
/// Codex's limit windows: (length, group, name, short name). It reports some a minute short.
const CODEX_WINDOWS: [(i64, &str, &str, &str); 3] = [
    (5 * 3600, "session", "session (5h)", "5h"),
    (7 * 86400, "weekly", "week (all models)", "week"),
    (30 * 86400, "monthly", "month (30 days)", "month"),
];
/// Time-until-reset bars, from red at the reset through orange and gold to blue at the start
/// of the window (xterm 256-colour palette). No green, so they don't look like the plan bars.
pub const URGENCY_COLORS: [u8; 11] = [160, 166, 172, 178, 143, 109, 110, 74, 68, 33, 27];
/// Per-model weekly limits such as Fable are named in violet.
pub const SCOPED_COLOR: u8 = 134;

pub fn severity_color(severity: &Value) -> Option<&'static str> {
    match s(severity) {
        Some("normal") => Some("32"),
        Some("warning") => Some("33"),
        Some("critical") => Some("31"),
        _ => None,
    }
}

fn default_profile() -> String {
    ospath::join(&paths::home(), ".claude")
}

fn default_codex_home() -> String {
    ospath::join(&paths::home(), ".codex")
}

/// ~/.claude plus every ~/.claude-* (and $CLAUDE_CONFIG_DIR) Claude Code config dir.
pub fn find_profiles() -> Vec<String> {
    let home = paths::home();
    let default = default_profile();
    let mut candidates = vec![default.clone()];
    let mut others = ospath::glob(&home, &[Part::Wild(".claude-", "")]);
    others.sort();
    candidates.extend(others);
    if let Ok(dir) = std::env::var("CLAUDE_CONFIG_DIR") {
        if !dir.is_empty() {
            candidates.push(ospath::abspath(&ospath::expanduser(&dir)));
        }
    }
    let mut profiles: Vec<String> = Vec::new();
    for d in candidates {
        if !profiles.contains(&d)
            && ospath::isdir(&d)
            && (d == default
                || ospath::isdir(&ospath::join(&d, "projects"))
                || ospath::isfile(&ospath::join(&d, ".claude.json")))
        {
            profiles.push(d);
        }
    }
    profiles
}

/// ~/.codex plus every ~/.codex-* (and $CODEX_HOME) Codex home: a folder with Codex's login,
/// config or sessions in it.
pub fn find_codex_homes() -> Vec<String> {
    let home = paths::home();
    let mut candidates = vec![default_codex_home()];
    let mut others = ospath::glob(&home, &[Part::Wild(".codex-", "")]);
    others.sort();
    candidates.extend(others);
    if let Ok(dir) = std::env::var("CODEX_HOME") {
        if !dir.is_empty() {
            candidates.push(ospath::abspath(&ospath::expanduser(&dir)));
        }
    }
    let mut homes: Vec<String> = Vec::new();
    for d in candidates {
        if !homes.contains(&d)
            && ospath::isdir(&d)
            && ["auth.json", "config.toml", "sessions"]
                .iter()
                .any(|n| ospath::exists(&ospath::join(&d, n)))
        {
            homes.push(d);
        }
    }
    homes
}

/// Without CLAUDE_CONFIG_DIR, Claude Code keeps its config beside ~/.claude, not in it.
fn config_path(profile: &str) -> String {
    if profile == default_profile() {
        ospath::join(&paths::home(), ".claude.json")
    } else {
        ospath::join(profile, ".claude.json")
    }
}

/// Where Claude Code is run for /usage: a folder of hatcheck's own, so the one project folder
/// Claude Code makes for the folder it runs in (projects/<its name>, empty) is the same every
/// time, instead of one per run.
pub fn usage_run_dir() -> String {
    ospath::join(&crate::facts::cache_dir(), "claude-usage")
}

/// Which stream of a run to keep.
#[derive(Clone, Copy)]
pub(crate) enum Keep {
    Stdout,
    Stderr,
}

/// Run a prepared command, with no terminal on its stdin, and give whether it exited 0 and the
/// kept stream, or the reason it gave nothing: it couldn't be started, or didn't finish within
/// timeout and was killed.
pub(crate) fn run_within(
    mut command: std::process::Command,
    timeout: Duration,
    keep: Keep,
) -> Result<(bool, String), String> {
    use std::io::Read;
    use std::process::Stdio;
    use std::sync::{mpsc, Arc, Mutex};
    let (stdout, stderr) = match keep {
        Keep::Stdout => (Stdio::piped(), Stdio::null()),
        Keep::Stderr => (Stdio::null(), Stdio::piped()),
    };
    let mut child = command
        .stdin(Stdio::null())
        .stdout(stdout)
        .stderr(stderr)
        .spawn()
        .map_err(|e| format!("couldn't be run: {e}"))?;
    let pipe: Option<Box<dyn Read>> = match keep {
        Keep::Stdout => child.stdout.take().map(|p| Box::new(p) as Box<dyn Read>),
        Keep::Stderr => child.stderr.take().map(|p| Box::new(p) as Box<dyn Read>),
    };
    let Some(mut pipe) = pipe else {
        return Err("gave no output".into());
    };
    let child = Arc::new(Mutex::new(child));
    let (done, finished) = mpsc::channel::<()>();
    // A watchdog kills the program if it hangs, which ends the read below.
    let watchdog = {
        let child = Arc::clone(&child);
        std::thread::spawn(move || {
            if finished.recv_timeout(timeout) == Err(mpsc::RecvTimeoutError::Timeout) {
                if let Ok(mut c) = child.lock() {
                    let _ = c.kill();
                }
                return true;
            }
            false
        })
    };
    let mut text = String::new();
    let read = pipe.read_to_string(&mut text).is_ok();
    let _ = done.send(());
    let killed = watchdog.join().unwrap_or(true);
    let status = child.lock().ok().and_then(|mut c| c.wait().ok());
    if killed {
        return Err(format!("didn't finish within {}s", timeout.as_secs()));
    }
    if !read {
        return Err("gave no output".into());
    }
    Ok((status.is_some_and(|s| s.success()), text))
}

/// A run's stdout, whatever its exit status: see run_within.
pub(crate) fn output_within(
    command: std::process::Command,
    timeout: Duration,
) -> Result<String, String> {
    run_within(command, timeout, Keep::Stdout).map(|(_, text)| text)
}

/// Run Claude Code's /usage in a profile: it asks Anthropic with the profile's own login and
/// saves the answer in the profile's .claude.json. The text it printed as its result, or why it
/// couldn't be run.
pub fn run_claude_usage(profile: &str) -> Result<String, String> {
    let Some(claude) = ospath::which("claude") else {
        return Err("claude isn't on your PATH".into());
    };
    let run_dir = usage_run_dir();
    let _ = std::fs::create_dir_all(&run_dir);
    let mut command = std::process::Command::new(claude);
    command.args(CLAUDE_USAGE_ARGS);
    if ospath::isdir(&run_dir) {
        command.current_dir(&run_dir);
    }
    for (name, _) in std::env::vars_os() {
        let name = name.to_string_lossy();
        if name.starts_with("ANTHROPIC_") || CLAUDE_USAGE_UNSET.contains(&name.as_ref()) {
            command.env_remove(name.as_ref());
        }
    }
    command
        .env("DISABLE_TELEMETRY", "1")
        .env("DISABLE_ERROR_REPORTING", "1")
        .env("DISABLE_AUTOUPDATER", "1");
    if profile == default_profile() {
        command.env_remove("CLAUDE_CONFIG_DIR"); // its login is filed under no CLAUDE_CONFIG_DIR
    } else {
        command.env("CLAUDE_CONFIG_DIR", profile);
    }
    let out = output_within(command, CLAUDE_USAGE_TIMEOUT).map_err(|e| format!("claude {e}"))?;
    Ok(claude_result(&out).unwrap_or(out))
}

/// The result text in what `claude -p --output-format json` printed: one object, or, with
/// Claude Code's verbose output on, an array of every message with the result last.
fn claude_result(stdout: &str) -> Option<String> {
    let v: Value = serde_json::from_str(stdout.trim()).ok()?;
    let result = match &v {
        Value::Array(items) => items
            .iter()
            .rev()
            .find(|m| s(get(m, "type")) == Some("result"))?,
        other => other,
    };
    s(get(result, "result")).map(String::from)
}

/// The profile's signed-in account (oauthAccount), and the usage snapshot Claude Code saved.
pub fn read_config(profile: &str) -> (Value, Value) {
    let Some(config) = std::fs::read_to_string(config_path(profile))
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
    else {
        return (json!({}), Value::Null);
    };
    let account = get(&config, "oauthAccount");
    (
        if truthy(account) {
            account.clone()
        } else {
            json!({})
        },
        get(&config, "cachedUsageUtilization").clone(),
    )
}

/// (response body, None) or (None, why it failed).
fn fetch_json(url: &str, headers: &[(&str, String)]) -> (Option<Value>, Option<String>) {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(10)))
        .http_status_as_error(false)
        .proxy(ureq::Proxy::try_from_env())
        .build()
        .into();
    let mut request = agent.get(url);
    for (k, v) in headers {
        request = request.header(*k, v.as_str());
    }
    let mut response = match request.call() {
        Ok(r) => r,
        Err(ureq::Error::Timeout(_)) => return (None, Some("TimeoutError".into())),
        Err(_) => return (None, Some("offline".into())),
    };
    let code = response.status().as_u16();
    if code == 429 {
        let retry = response
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        let wait = retry
            .filter(|r| !r.is_empty() && r.bytes().all(|b| b.is_ascii_digit()))
            .and_then(|r| r.parse::<f64>().ok());
        return (
            None,
            Some(format!(
                "rate limited{}",
                wait.map_or(String::new(), |w| format!(
                    ", retry in {}",
                    times::duration(w)
                ))
            )),
        );
    }
    if code >= 400 {
        return (
            None,
            Some(if code == 401 || code == 403 {
                "token rejected".into()
            } else {
                format!("HTTP {code}")
            }),
        );
    }
    match response.body_mut().read_json::<Value>() {
        Ok(v) => (Some(v), None),
        Err(_) => (None, Some("JSONDecodeError".into())),
    }
}

/// The claims in a JWT, decoded without checking its signature, or {}.
pub fn jwt_claims(token: &Value) -> Value {
    let Some(payload) = s(token).and_then(|t| t.split('.').nth(1)) else {
        return json!({});
    };
    // As base64.urlsafe_b64decode reads it: characters outside the alphabet are skipped.
    let clean: String = payload
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    let engine = base64::engine::GeneralPurpose::new(
        &base64::alphabet::URL_SAFE,
        base64::engine::GeneralPurposeConfig::new()
            .with_decode_padding_mode(base64::engine::DecodePaddingMode::Indifferent),
    );
    match engine
        .decode(clean)
        .ok()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
    {
        Some(v) if v.is_object() => v,
        _ => json!({}),
    }
}

/// A Codex home's login, from its auth.json: the ChatGPT account, plan and token expiry,
/// {"api_key": true} for an API key, or null when it isn't signed in (or keeps its login in
/// the system keyring, which isn't read).
pub fn read_codex_login(home: &str) -> Value {
    let Some(auth) = std::fs::read_to_string(ospath::join(home, "auth.json"))
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
    else {
        return Value::Null;
    };
    let tokens = if auth.is_object() {
        get(&auth, "tokens")
    } else {
        py::null()
    };
    if !tokens.is_object() || !truthy(get(tokens, "access_token")) {
        return if auth.is_object() && truthy(get(&auth, "OPENAI_API_KEY")) {
            json!({"api_key": true})
        } else {
            Value::Null
        };
    }
    let (access, identity) = (
        jwt_claims(get(tokens, "access_token")),
        jwt_claims(get(tokens, "id_token")),
    );
    let mut claims = Map::new();
    for source in [&identity, &access] {
        if let Value::Object(auth) = get(source, "https://api.openai.com/auth") {
            claims.extend(auth.clone()); // the access token's claims are the fresher
        }
    }
    let claims = Value::Object(claims);
    let exp = get(&access, "exp");
    json!({"access_token": get(tokens, "access_token"),
           "account_id": py::or(get(tokens, "account_id"), get(&claims, "chatgpt_account_id")),
           "user_id": py::or(get(&claims, "chatgpt_user_id"), get(&claims, "user_id")),
           "email": get(&identity, "email"), "plan": get(&claims, "chatgpt_plan_type"),
           "expires": if exp.is_number() { exp.clone() } else { json!(0) }})
}

fn fetch_codex_usage(login: &Value) -> (Option<Value>, Option<String>) {
    fetch_json(
        CODEX_USAGE_URL,
        &[
            (
                "Authorization",
                format!("Bearer {}", s(get(login, "access_token")).unwrap_or("")),
            ),
            ("ChatGPT-Account-Id", py::str_of(get(login, "account_id"))),
            ("Accept", "application/json".into()),
            ("User-Agent", "hatcheck".into()),
        ],
    )
}

/// GPT-5.3-Codex-Spark -> Spark: re.sub(r"^GPT-[\d.]+-(Codex-)?", "", name).
fn model_short(name: &str) -> &str {
    let Some(rest) = name.strip_prefix("GPT-") else {
        return name;
    };
    let digits = rest
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .count();
    if digits == 0 || !rest[digits..].starts_with('-') {
        return name;
    }
    let rest = &rest[digits + 1..];
    rest.strip_prefix("Codex-").unwrap_or(rest)
}

/// A Codex limit as a limits[] row like Claude's, with its window and its names. name is a
/// single model's, as in GPT-5.3-Codex-Spark; resets_at is in seconds.
pub fn codex_row(
    seconds: &Value,
    used: &Value,
    resets_at: &Value,
    name: Option<&str>,
    reached: bool,
) -> Value {
    let seconds = if truthy(seconds) {
        Num::of(seconds).map_or(0, |n| n.f().trunc() as i64)
    } else {
        0
    };
    let known = CODEX_WINDOWS
        .iter()
        .find(|w| seconds != 0 && (seconds - w.0).abs() <= 120);
    let (seconds, group, mut label, mut short) = match known {
        Some(&(len, group, label, short)) => {
            (len, group.to_string(), label.to_string(), short.to_string())
        }
        None if seconds != 0 => (
            seconds,
            "window".into(),
            format!("{} window", times::duration(seconds as f64)),
            times::duration(seconds as f64),
        ),
        None => (0, "window".into(), "limit".into(), "limit".into()),
    };
    if let Some(name) = name.filter(|n| !n.is_empty()) {
        let model = if model_short(name).is_empty() {
            name
        } else {
            model_short(name)
        };
        label = format!("{short} ({name})");
        short = if group == "weekly" {
            model.to_string()
        } else {
            format!("{model} {short}")
        };
    }
    let used = Num::of(used)
        .filter(|_| used.is_number())
        .unwrap_or(Num::Int(0));
    let reset = match Num::of(resets_at) {
        Some(r) if resets_at.is_number() && r.f() > 0.0 => {
            times::isoformat_utc(r.f()).map_or(Value::Null, Value::String)
        }
        _ => Value::Null,
    };
    let scoped = name.is_some_and(|n| !n.is_empty());
    let severity = if reached || used.f() >= 90.0 {
        "critical"
    } else if used.f() >= 75.0 {
        "warning"
    } else {
        "normal"
    };
    json!({"kind": format!("{group}{}", if scoped { "_scoped" } else { "" }), "group": group, "label": label, "short": short,
           "window_seconds": if seconds != 0 { json!(seconds) } else { Value::Null }, "percent": used.value(),
           "resets_at": reset, "severity": severity})
}

/// The Codex usage endpoint's answer as a usage body with limits[] rows like Claude's.
pub fn codex_usage_body(body: &Value) -> Value {
    let mut rows = Vec::new();
    let mut add = |limit: &Value, name: Option<String>| {
        let limit = if limit.is_object() { limit } else { py::null() };
        for key in ["primary_window", "secondary_window"] {
            let window = get(limit, key);
            if window.is_object() {
                rows.push(codex_row(
                    get(window, "limit_window_seconds"),
                    get(window, "used_percent"),
                    get(window, "reset_at"),
                    name.as_deref(),
                    truthy(get(limit, "limit_reached")),
                ));
            }
        }
    };
    add(get(body, "rate_limit"), None);
    if let Value::Array(extras) = get(body, "additional_rate_limits") {
        for extra in extras.iter().filter(|e| e.is_object()) {
            let name = py::or(
                py::or(get(extra, "limit_name"), get(extra, "metered_feature")),
                &Value::Null,
            );
            let name = if truthy(name) {
                py::str_of(name)
            } else {
                "other".into()
            };
            add(get(extra, "rate_limit"), Some(name));
        }
    }
    add(
        get(body, "code_review_rate_limit"),
        Some("code review".into()),
    );
    json!({"limits": rows, "plan_type": get(body, "plan_type"), "codex_credits": get(body, "credits")})
}

/// (usage body, when Codex saw it): the newest limits Codex recorded in a home's session
/// files, one row set per limit it reported there.
pub fn codex_snapshot(home: &str) -> Option<(Value, f64)> {
    let mut files: Vec<(f64, String)> = ospath::glob(
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
    .filter_map(|p| ospath::getmtime(&p).ok().map(|t| (t, p)))
    .collect();
    files.sort_by(|a, b| {
        b.0.partial_cmp(&a.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.1.cmp(&a.1))
    });
    let key = b"\"rate_limits\":{";
    let finder = memchr::memmem::FinderRev::new(key);
    for (_, path) in files.iter().take(10) {
        let Ok(file) = std::fs::File::open(path) else {
            continue;
        };
        if file.metadata().map(|m| m.len()).unwrap_or(0) == 0 {
            continue;
        }
        // SAFETY: session files are only appended to.
        let Ok(map) = (unsafe { memmap2::Mmap::map(&file) }) else {
            continue;
        };
        let data: &[u8] = &map;
        // limit ID -> (when, rate_limits), newest first
        let mut latest: Vec<(Value, Option<f64>, Value)> = Vec::new();
        let (mut end, mut seen) = (data.len(), 0);
        while seen < 500 {
            let Some(i) = finder.rfind(&data[..end]) else {
                break;
            };
            let start = memchr::memrchr(b'\n', &data[..i]).map_or(0, |j| j + 1);
            let stop = memchr::memchr(b'\n', &data[i..]).map_or(data.len(), |j| i + j);
            seen += 1;
            if let Ok(record) = serde_json::from_slice::<Value>(&data[start..stop]) {
                let limits = get(get(&record, "payload"), "rate_limits");
                if limits.is_object() {
                    let id = if truthy(get(limits, "limit_id")) {
                        get(limits, "limit_id").clone()
                    } else {
                        json!("codex")
                    };
                    if !latest.iter().any(|(k, _, _)| *k == id) {
                        latest.push((
                            id,
                            s(get(&record, "timestamp")).and_then(times::parse_time),
                            limits.clone(),
                        ));
                    }
                }
            }
            if start == 0 {
                break;
            }
            end = start - 1;
        }
        let latest: Vec<(Value, f64, Value)> = latest
            .into_iter()
            .filter_map(|(k, t, l)| t.map(|t| (k, t, l)))
            .collect();
        if latest.is_empty() {
            continue;
        }
        let mut ordered: Vec<&(Value, f64, Value)> = latest.iter().collect();
        ordered.sort_by_key(|(k, _, _)| s(k) != Some("codex"));
        let mut rows = Vec::new();
        for (key, _, limits) in ordered {
            let name = if s(key) == Some("codex") {
                None
            } else if truthy(get(limits, "limit_name")) {
                Some(py::str_of(get(limits, "limit_name")))
            } else {
                Some(py::str_of(key))
            };
            for window in [get(limits, "primary"), get(limits, "secondary")] {
                if window.is_object() {
                    let minutes = get(window, "window_minutes");
                    let seconds = match Num::of(minutes).filter(|_| truthy(minutes)) {
                        Some(Num::Int(m)) => json!(m * 60),
                        Some(Num::Float(m)) => json!(m * 60.0),
                        None => json!(0),
                    };
                    rows.push(codex_row(
                        &seconds,
                        get(window, "used_percent"),
                        get(window, "resets_at"),
                        name.as_deref(),
                        false,
                    ));
                }
            }
        }
        let newest = latest.iter().fold(
            &latest[0],
            |best, item| if item.1 > best.1 { item } else { best },
        );
        let credits_from = latest
            .iter()
            .find(|(k, _, _)| s(k) == Some("codex"))
            .unwrap_or(newest);
        return Some((
            json!({"limits": rows, "plan_type": get(&newest.2, "plan_type"), "codex_credits": get(&credits_from.2, "credits")}),
            newest.1,
        ));
    }
    None
}

/// A bar `cells` wide, filled to fraction (0 to 1) in eighths of a cell.
pub fn bar(fraction: f64, cells: usize) -> String {
    let fraction = if fraction < 0.0 {
        0.0
    } else if fraction > 1.0 || fraction.is_nan() {
        1.0
    } else {
        fraction
    };
    let mut eighths = py::round(fraction * cells as f64 * 8.0) as usize;
    if 0.0 < fraction && fraction < 1.0 {
        eighths = eighths.clamp(1, cells * 8 - 1); // only exactly 0 or 1 looks empty or full
    }
    const PARTS: [&str; 8] = [" ", "▏", "▎", "▍", "▌", "▋", "▊", "▉"];
    let mut text = "█".repeat(eighths / 8);
    text.push_str(PARTS[eighths % 8]);
    let text = text.trim_end().to_string();
    let len = text.chars().count();
    text + &"░".repeat(cells.saturating_sub(len))
}

pub fn has_256_colors() -> bool {
    std::env::var("TERM").unwrap_or_default().contains("256")
        || std::env::var("COLORTERM").is_ok_and(|v| !v.is_empty())
}

/// Colour for a time-until-reset bar `fraction` full: red as the reset nears, through orange
/// and gold, to blue with most of the window to go.
pub fn urgency_color(fraction: f64, wide: bool) -> String {
    let n = URGENCY_COLORS.len();
    let step = ((fraction.max(0.0) * n as f64) as usize).min(n - 1);
    if wide {
        return format!("38;5;{}", URGENCY_COLORS[step]);
    }
    // A 16-colour terminal: the nearest basic colour.
    (if step < 2 {
        "31"
    } else if step < 4 {
        "33"
    } else if step < 7 {
        "36"
    } else {
        "34"
    })
    .into()
}

/// Colour for a limit's name: violet for a per-model limit such as Fable, else default. wide:
/// the terminal shows 256 colours.
pub fn label_color(kind: &Value, default: &str, wide: bool) -> String {
    if !s(kind).unwrap_or("").ends_with("_scoped") {
        return default.into();
    }
    if wide {
        format!("38;5;{SCOPED_COLOR}")
    } else {
        "35".into()
    }
}

/// How long a meter's window runs, from a row's group or kind or a quota field's name.
pub fn window_seconds(name: &str) -> Option<i64> {
    if name == "session" || name == "five_hour" {
        Some(5 * 3600)
    } else if name.starts_with("weekly") || name.starts_with("seven_day") {
        Some(7 * 86400)
    } else {
        None
    }
}

/// How long a limits[] row's window runs: its window_seconds, or what its group or kind says.
pub fn row_window(row: &Value) -> Value {
    let w = get(row, "window_seconds");
    if truthy(w) {
        return w.clone();
    }
    let name = py::or(get(row, "group"), get(row, "kind"));
    window_seconds(s(name).unwrap_or("")).map_or(Value::Null, |n| json!(n))
}

/// How many blocks a bar for a window gets: one per hour of a 5-hour window, one per day of a
/// weekly one, and 10 for a longer one, such as Codex's 30 days.
pub fn bar_cells(window: &Value) -> usize {
    let Some(w) = Num::of(window).filter(|_| truthy(window)) else {
        return 5;
    };
    let w = w.f();
    let days = (w / 86400.0).floor();
    let n = if days != 0.0 {
        days
    } else {
        (w / 3600.0).floor()
    };
    n.clamp(0.0, 10.0) as usize
}

/// The plan an account is on, from the oauthAccount Claude Code keeps in the profile's
/// .claude.json: its organization's kind (claude_team, claude_pro, claude_max) and its rate
/// limit tier (default_claude_max_5x), as "team max 5x" or "pro".
pub fn plan_text(account: &Value) -> String {
    if !truthy(account) {
        return "not signed in".into();
    }
    let org = s(get(account, "organizationType")).unwrap_or("");
    let mut plan = org.strip_prefix("claude_").unwrap_or(org).to_string();
    if plan.is_empty() {
        plan = "?".into();
    }
    let tier = py::or(
        get(account, "userRateLimitTier"),
        get(account, "organizationRateLimitTier"),
    );
    let tier = s(tier).unwrap_or("");
    for multiple in ["20x", "5x"] {
        if tier.ends_with(&format!("max_{multiple}")) {
            plan += &format!(" max {multiple}");
        }
    }
    plan
}

/// How much of a limit can be used per hour until it resets without running out: what's left
/// over the hours to go (at least one), as a share of the limit. None without a reset ahead.
pub fn pace(left: f64, reset: Option<f64>, now: f64) -> Option<f64> {
    let reset = reset.filter(|r| *r > now)?;
    Some(left / ((reset - now) / 3600.0).max(1.0))
}

pub fn pace_text(p: f64) -> String {
    format!("{p:.1}%/h")
}

pub fn row_label(row: &Value, short: bool) -> String {
    if truthy(get(row, "label")) {
        let pick = if short {
            py::or(get(row, "short"), get(row, "label"))
        } else {
            get(row, "label")
        };
        return py::str_of(pick);
    }
    let scope = get(row, "scope");
    let name = py::or(
        get(get(scope, "model"), "display_name"),
        get(get(scope, "surface"), "display_name"),
    );
    let name = truthy(name).then(|| py::str_of(name));
    let kind = if truthy(get(row, "kind")) {
        py::str_of(get(row, "kind"))
    } else {
        "?".into()
    };
    match (kind.as_str(), &name) {
        ("session", _) => (if short { "5h" } else { "session (5h)" }).into(),
        ("weekly_all", _) => (if short { "week" } else { "week (all models)" }).into(),
        ("weekly_scoped", Some(n)) => {
            if short {
                n.clone()
            } else {
                format!("week ({n})")
            }
        }
        _ => format!(
            "{}{}",
            kind.replace('_', " "),
            name.map_or(String::new(), |n| format!(" ({n})"))
        ),
    }
}

/// What's left of a meter, or None when it comes from a snapshot saved before the meter reset.
pub fn percent_left(
    used: &Value,
    resets_at: &Value,
    saved_at: Option<f64>,
    now: f64,
) -> Option<Num> {
    let reset = s(resets_at).and_then(times::parse_time);
    if saved_at.is_some() && reset.is_some_and(|r| r <= now) {
        return None;
    }
    let used = if truthy(used) {
        Num::of(used).unwrap_or(Num::Int(0))
    } else {
        Num::Int(0)
    };
    Some(Num::max(
        Num::Int(0),
        Num::min(Num::Int(100), Num::minus(Num::Int(100), used)),
    ))
}

/// Whether a limits[] row is the weekly limit for Fable.
pub fn is_fable(row: &Value) -> bool {
    let model = s(get(get(get(row, "scope"), "model"), "display_name")).unwrap_or("");
    s(get(row, "kind")) == Some("weekly_scoped") && model.to_lowercase().contains("fable")
}

/// (% left, next reset) of a limits[] row. A snapshot's meter that has reset since it was
/// saved counts as full, until the reset after.
pub fn meter_state(row: &Value, saved_at: Option<f64>, now: f64) -> (f64, Option<f64>) {
    let mut reset = s(get(row, "resets_at")).and_then(times::parse_time);
    let window = Num::of(&row_window(row)).map(Num::f).filter(|w| *w != 0.0);
    match percent_left(get(row, "percent"), get(row, "resets_at"), saved_at, now) {
        Some(left) => (left.f(), reset),
        None => {
            if let Some(w) = window {
                while let Some(r) = reset.filter(|r| *r <= now) {
                    reset = Some(r + w);
                }
            }
            (100.0, reset)
        }
    }
}

/// Where an account goes in the list, by its Fable limit: the bands with Fable left, from
/// FABLE_FRESH (under 90% used) to FABLE_NEARLY_OUT (98% or more used), each with the soonest
/// Fable reset on top; then FABLE_OUT, out of Fable or of the weekly limit for all models it
/// counts against, the soonest back on top; then NO_FABLE, without a Fable limit.
pub const FABLE_FRESH: u8 = 0;
pub const FABLE_LOW: u8 = 1;
pub const FABLE_NEARLY_OUT: u8 = 2;
pub const FABLE_OUT: u8 = 3;
pub const NO_FABLE: u8 = 4;
/// Past this share used, an account goes behind the fresher ones; past the second, behind
/// those too: nearly out, it's the one to use last.
pub const LOW_USED: f64 = 90.0;
pub const NEARLY_OUT_USED: f64 = 98.0;

/// The band's name, for --json.
pub fn band_name(band: u8) -> &'static str {
    match band {
        FABLE_FRESH => "fresh",
        FABLE_LOW => "low",
        FABLE_NEARLY_OUT => "nearly out",
        FABLE_OUT => "out",
        _ => "none",
    }
}

/// (band, time): where an account goes and the time its band sorts by (see the bands above).
pub fn fable_rank(usage: Option<&Value>, saved_at: Option<f64>, now: f64) -> (u8, f64) {
    let limits: Vec<&Value> = match usage.map(|u| get(u, "limits")) {
        Some(Value::Array(rows)) => rows.iter().filter(|r| r.is_object()).collect(),
        _ => Vec::new(),
    };
    let Some(fable) = limits.iter().find(|r| is_fable(r)) else {
        return (NO_FABLE, 0.0);
    };
    let mut needed = vec![meter_state(fable, saved_at, now)];
    needed.extend(
        limits
            .iter()
            .filter(|r| s(get(r, "kind")) == Some("weekly_all"))
            .map(|r| meter_state(r, saved_at, now)),
    );
    if needed.iter().all(|(left, _)| *left > 0.0) {
        let used = 100.0 - needed[0].0;
        let band = if used >= NEARLY_OUT_USED {
            FABLE_NEARLY_OUT
        } else if used >= LOW_USED {
            FABLE_LOW
        } else {
            FABLE_FRESH
        };
        return (
            band,
            needed[0].1.filter(|r| *r != 0.0).unwrap_or(f64::INFINITY),
        );
    }
    let back = needed
        .iter()
        .filter(|(left, _)| *left <= 0.0)
        .map(|(_, r)| r.filter(|r| *r != 0.0).unwrap_or(f64::INFINITY))
        .fold(f64::NEG_INFINITY, f64::max);
    (FABLE_OUT, back)
}

/// Python's == for the values JSON holds: numbers by value, 1 and 1.0 and true alike.
fn py_eq(a: &Value, b: &Value) -> bool {
    match (Num::of(a), Num::of(b)) {
        (Some(x), Some(y)) => x.f() == y.f(),
        _ => a == b,
    }
}

/// A meter: (label, percent used, resets_at, window, severity, note, labeled, kind).
pub struct Row {
    pub label: String,
    pub used: Value,
    pub resets_at: Value,
    pub window: Value,
    pub severity: Value,
    pub note: String,
    pub labeled: bool,
    pub kind: Value,
}

/// Every meter in a usage body: its limits[] rows, then the quota fields Claude Code doesn't
/// display, by their raw names, leaving out ones that repeat a row.
pub fn usage_rows(usage: &Value) -> Vec<Row> {
    let limits: Vec<&Value> = match get(usage, "limits") {
        Value::Array(rows) => rows.iter().filter(|r| r.is_object()).collect(),
        _ => Vec::new(),
    };
    let mut rows: Vec<Row> = limits
        .iter()
        .map(|r| Row {
            label: row_label(r, false),
            used: get(r, "percent").clone(),
            resets_at: get(r, "resets_at").clone(),
            window: row_window(r),
            severity: get(r, "severity").clone(),
            note: String::new(),
            labeled: true,
            kind: get(r, "kind").clone(),
        })
        .collect();
    let Value::Object(fields) = usage else {
        return rows;
    };
    for (key, value) in fields {
        if !value.is_object() || get(value, "utilization").is_null() {
            continue;
        }
        let pair = (get(value, "utilization"), get(value, "resets_at"));
        if limits
            .iter()
            .any(|r| py_eq(get(r, "percent"), pair.0) && py_eq(get(r, "resets_at"), pair.1))
        {
            continue;
        }
        let mut note = Vec::new();
        if !get(value, "limit_dollars").is_null() {
            let used = Num::of(get(value, "used_dollars"))
                .filter(|_| truthy(get(value, "used_dollars")))
                .map_or(0.0, Num::f);
            let limit = Num::of(get(value, "limit_dollars")).map_or(0.0, Num::f);
            note.push(format!("${used:.2} of ${} used", py::g(limit)));
        }
        if truthy(get(value, "locked_reason")) {
            note.push(format!(
                "locked: {}",
                py::str_of(get(value, "locked_reason"))
            ));
        }
        rows.push(Row {
            label: key.clone(),
            used: get(value, "utilization").clone(),
            resets_at: get(value, "resets_at").clone(),
            window: window_seconds(key).map_or(Value::Null, |n| json!(n)),
            severity: Value::Null,
            note: note.join(" · "),
            labeled: false,
            kind: Value::Null,
        });
    }
    rows
}

/// Extra usage or Codex credits, as a line under an account's meters, or "".
pub fn extra_usage_text(usage: &Value) -> String {
    let credits = get(usage, "codex_credits");
    if credits.is_object()
        && (truthy(get(credits, "has_credits")) || truthy(get(credits, "unlimited")))
    {
        let amount = if truthy(get(credits, "unlimited")) {
            "unlimited".into()
        } else if truthy(get(credits, "balance")) {
            py::str_of(get(credits, "balance"))
        } else {
            "0".into()
        };
        return format!("credits: {amount}");
    }
    let extra = get(usage, "extra_usage");
    if !truthy(get(extra, "is_enabled")) && !truthy(get(extra, "used_credits")) {
        return String::new();
    }
    let places = Num::of(get(extra, "decimal_places"))
        .filter(|_| truthy(get(extra, "decimal_places")))
        .map_or(2.0, Num::f);
    let scale = 10f64.powf(places);
    let currency = if truthy(get(extra, "currency")) {
        py::str_of(get(extra, "currency"))
    } else {
        "USD".into()
    };
    let used = Num::of(get(extra, "used_credits"))
        .filter(|_| truthy(get(extra, "used_credits")))
        .map_or(0.0, Num::f)
        / scale;
    let mut text = format!("extra usage: {used:.2} {currency} used");
    if let Some(limit) =
        Num::of(get(extra, "monthly_limit")).filter(|_| truthy(get(extra, "monthly_limit")))
    {
        text += &format!(" of {:.2} this month", limit.f() / scale);
    }
    text
}

/// A Claude Code account or a ChatGPT one, with the profiles or homes signed in to it.
#[derive(Clone, Debug)]
pub struct Entry {
    pub agent: Agent,
    pub profiles: Vec<String>,
    pub email: Value,
    /// The oauthAccount Claude Code keeps in a profile's .claude.json: who it is, its plan.
    pub account: Value,
    pub cache: Value,
    pub login: Value,
}

/// What an account's numbers are: the usage body, when it was saved (None when live), and why
/// it isn't live.
pub type Usage = (Option<Value>, Option<f64>, Option<String>);

fn num_or_zero(v: &Value) -> f64 {
    Num::of(v).map_or(0.0, Num::f)
}

/// Profiles grouped by the account they're signed in to, since they share its limits.
pub fn load_accounts(profiles: &[String]) -> Vec<Entry> {
    let mut keys: Vec<String> = Vec::new();
    let mut entries: Vec<Entry> = Vec::new();
    for profile in profiles {
        let (account, mut cache) = read_config(profile);
        let uuid = get(&account, "accountUuid");
        let key = if truthy(uuid) {
            format!("account {}", py::repr(uuid))
        } else {
            format!("profile {profile}")
        };
        let k = match keys.iter().position(|k2| *k2 == key) {
            Some(k) => k,
            None => {
                keys.push(key);
                entries.push(Entry {
                    agent: Agent::Claude,
                    profiles: Vec::new(),
                    email: get(&account, "emailAddress").clone(),
                    account: Value::Null,
                    cache: Value::Null,
                    login: Value::Null,
                });
                entries.len() - 1
            }
        };
        let entry = &mut entries[k];
        entry.profiles.push(profile.clone());
        if truthy(&cache) {
            let saved_for = get(&cache, "accountUuid");
            if !saved_for.is_null() && !py_eq(saved_for, uuid) {
                cache = Value::Null; // saved while the profile was signed in to another account
            }
        }
        if truthy(&account)
            && (entry.account.is_null()
                || num_or_zero(get(&account, "profileFetchedAt"))
                    > num_or_zero(get(&entry.account, "profileFetchedAt")))
        {
            entry.account = account.clone();
        }
        if truthy(&cache)
            && (entry.cache.is_null()
                || num_or_zero(get(&cache, "fetchedAtMs"))
                    > num_or_zero(get(&entry.cache, "fetchedAtMs")))
        {
            entry.cache = cache;
        }
    }
    entries
}

/// An account's numbers fetched now: Claude Code's /usage run in the first of its profiles,
/// and the snapshot it saved there read back; or why that gave nothing new.
fn claude_live(entry: &Entry) -> Result<Value, String> {
    let Some(profile) = entry.profiles.first() else {
        return Err("no profile".into());
    };
    let started = times::now();
    if ospath::which("claude").is_some() && claude_reuses(entry, started) {
        return Err(CLAUDE_REUSE_NOTE.into());
    }
    let result = run_claude_usage(profile)?;
    let (_, cache) = read_config(profile);
    let fetched = num_or_zero(get(&cache, "fetchedAtMs")) / 1000.0;
    let utilization = get(&cache, "utilization");
    if fetched >= started - 2.0 && truthy(utilization) {
        return Ok(utilization.clone());
    }
    // Claude Code prints its last reading when it can't get a new one, or when that reading
    // is under a minute old, and a cost summary instead of the limits when the profile isn't
    // signed in.
    Err(
        if result.contains("Current session") || result.contains("Current week") {
            if (0.0..CLAUDE_REUSES).contains(&(times::now() - fetched)) {
                CLAUDE_REUSE_NOTE.into()
            } else {
                "Claude Code's /usage couldn't fetch new numbers; offline, or rate limited?".into()
            }
        } else if !truthy(&entry.account) {
            "not signed in".into()
        } else {
            "Claude Code's /usage gave no numbers; is this profile still signed in?".into()
        },
    )
}

/// Whether Claude Code's /usage would show the profile's snapshot again instead of asking: it's
/// the signed-in account's, has numbers, and is under a minute old.
fn claude_reuses(entry: &Entry, now: f64) -> bool {
    let cache = &entry.cache;
    if !truthy(get(cache, "utilization"))
        || get(cache, "accountUuid") != get(&entry.account, "accountUuid")
    {
        return false;
    }
    let age = now - num_or_zero(get(cache, "fetchedAtMs")) / 1000.0;
    (0.0..CLAUDE_REUSES).contains(&age)
}

/// The live usage body, or else Claude Code's snapshot and when it was saved, and why it isn't live.
pub fn account_usage(entry: &Entry, cached: bool) -> Usage {
    let why: String = if cached {
        "--cached".into()
    } else {
        match claude_live(entry) {
            Ok(usage) => return (Some(usage), None, None),
            Err(why) => why,
        }
    };
    if entry.cache.is_null() {
        return (None, None, Some(why));
    }
    let utilization = get(&entry.cache, "utilization");
    let usage = if truthy(utilization) {
        utilization.clone()
    } else {
        json!({})
    };
    (
        Some(usage),
        Some(num_or_zero(get(&entry.cache, "fetchedAtMs")) / 1000.0),
        Some(why),
    )
}

/// Codex homes grouped by the ChatGPT account and user they're signed in to.
pub fn load_codex_accounts(homes: &[String]) -> Vec<Entry> {
    let mut keys: Vec<String> = Vec::new();
    let mut entries: Vec<Entry> = Vec::new();
    for home in homes {
        let login = read_codex_login(home);
        let key = if truthy(&login) && truthy(get(&login, "account_id")) {
            format!(
                "codex {} {}",
                py::repr(get(&login, "account_id")),
                py::repr(get(&login, "user_id"))
            )
        } else {
            format!("home {home}")
        };
        let k = match keys.iter().position(|k2| *k2 == key) {
            Some(k) => k,
            None => {
                keys.push(key);
                entries.push(Entry {
                    agent: Agent::Codex,
                    profiles: Vec::new(),
                    email: Value::Null,
                    account: Value::Null,
                    cache: Value::Null,
                    login: Value::Null,
                });
                entries.len() - 1
            }
        };
        let entry = &mut entries[k];
        entry.profiles.push(home.clone());
        if truthy(&login)
            && (entry.login.is_null()
                || num_or_zero(get(&login, "expires")) > num_or_zero(get(&entry.login, "expires")))
        {
            entry.login = login.clone();
        }
        if !truthy(&entry.email) {
            entry.email = get(&login, "email").clone();
        }
    }
    entries
}

/// Usage for a ChatGPT account: live, or else the newest limits Codex recorded in its homes'
/// session files.
pub fn codex_account_usage(entry: &Entry, cached: bool) -> Usage {
    let login = &entry.login;
    let why: String = if cached {
        "--cached".into()
    } else if !truthy(login) {
        "not signed in".into()
    } else if truthy(get(login, "api_key")) {
        "signed in with an API key, which has no plan limits".into()
    } else if !truthy(get(login, "account_id")) {
        "not signed in with ChatGPT".into()
    } else if num_or_zero(get(login, "expires")) < times::now() + 60.0 {
        "token expired; run codex in this profile to renew it".into()
    } else {
        match fetch_codex_usage(login) {
            (Some(body), _) if body.is_object() => {
                return (Some(codex_usage_body(&body)), None, None)
            }
            (_, why) => why
                .filter(|w| !w.is_empty())
                .unwrap_or_else(|| "unexpected answer".into()),
        }
    };
    let snapshots: Vec<(Value, f64)> = entry
        .profiles
        .iter()
        .filter_map(|h| codex_snapshot(h))
        .collect();
    let Some(newest) = snapshots.iter().fold(None::<&(Value, f64)>, |best, s| {
        if best.is_none_or(|b| s.1 > b.1) {
            Some(s)
        } else {
            best
        }
    }) else {
        return (None, None, Some(why));
    };
    (Some(newest.0.clone()), Some(newest.1), Some(why))
}

pub fn codex_plan_text(entry: &Entry, usage: Option<&Value>) -> String {
    let login = &entry.login;
    if !truthy(login) {
        return "not signed in".into();
    }
    if truthy(get(login, "api_key")) {
        return "API key".into();
    }
    let plan = usage
        .map(|u| get(u, "plan_type"))
        .filter(|p| truthy(p))
        .unwrap_or_else(|| get(login, "plan"));
    format!(
        "ChatGPT {}",
        if truthy(plan) {
            py::str_of(plan)
        } else {
            "?".into()
        }
    )
}

fn iso(t: Option<f64>) -> Value {
    t.and_then(times::iso_utc)
        .map_or(Value::Null, Value::String)
}

/// An account's limits for --json: what the text shows, as numbers. left_percent is null for
/// a snapshot's limit that has reset since it was saved; pace is what's left per hour until
/// the reset (see `pace`), null without one.
pub fn limits_json(usage: &Value, saved_at: Option<f64>, now: f64) -> Value {
    let mut out = Vec::new();
    let pace_of = |left: Option<Num>, resets_at: &Value| -> Value {
        left.and_then(|l| pace(l.f(), s(resets_at).and_then(times::parse_time), now))
            .map_or(Value::Null, |p| json!((p * 100.0).round() / 100.0))
    };
    if let Value::Array(rows) = get(usage, "limits") {
        for r in rows.iter().filter(|r| r.is_object()) {
            let left = percent_left(get(r, "percent"), get(r, "resets_at"), saved_at, now);
            out.push(json!({"name": row_label(r, false), "short": row_label(r, true), "kind": get(r, "kind"),
                            "used_percent": get(r, "percent"),
                            "left_percent": left.map_or(Value::Null, Num::value),
                            "resets_at": iso(s(get(r, "resets_at")).and_then(times::parse_time)), "window_seconds": row_window(r),
                            "severity": get(r, "severity"), "per_model": py::str_of(get(r, "kind")).ends_with("_scoped") && !get(r, "kind").is_null(),
                            "pace": pace_of(left, get(r, "resets_at"))}));
        }
    }
    for row in usage_rows(usage).into_iter().filter(|r| !r.labeled) {
        let left = percent_left(&row.used, &row.resets_at, saved_at, now);
        out.push(json!({"name": row.label, "short": row.label, "kind": null, "used_percent": row.used,
                        "left_percent": left.map_or(Value::Null, Num::value),
                        "resets_at": iso(s(&row.resets_at).and_then(times::parse_time)), "window_seconds": row.window, "severity": null,
                        "per_model": false, "raw": true, "note": if row.note.is_empty() { Value::Null } else { json!(row.note) },
                        "pace": pace_of(left, &row.resets_at)}));
    }
    Value::Array(out)
}

/// One account for --json: its profiles or homes, plan, where the numbers come from, its
/// limits, and where it stands on Fable (null without a Fable limit).
pub fn account_json(entry: &Entry, result: &Usage, now: f64, email: bool) -> Value {
    let (usage, saved_at, why) = result;
    let mut out = Map::new();
    out.insert("agent".into(), json!(entry.agent.key()));
    out.insert(
        "profiles".into(),
        json!(entry
            .profiles
            .iter()
            .map(|p| ospath::abspath(p))
            .collect::<Vec<_>>()),
    );
    out.insert("plan".into(), json!(account_plan(entry, usage.as_ref())));
    out.insert(
        "source".into(),
        json!(if usage.is_none() {
            "none"
        } else if saved_at.is_none() {
            "live"
        } else {
            "cached"
        }),
    );
    out.insert("saved_at".into(), iso(*saved_at));
    out.insert("why".into(), why.as_ref().map_or(Value::Null, |w| json!(w)));
    if email {
        out.insert("email".into(), entry.email.clone());
    }
    let has_usage = usage.as_ref().is_some_and(truthy);
    out.insert(
        "limits".into(),
        if has_usage {
            limits_json(usage.as_ref().unwrap(), *saved_at, now)
        } else {
            json!([])
        },
    );
    let extra = extra_usage_text(usage.as_ref().unwrap_or(&json!({})));
    out.insert(
        "extra".into(),
        if extra.is_empty() {
            Value::Null
        } else {
            json!(extra)
        },
    );
    let (band, at) = fable_rank(usage.as_ref(), *saved_at, now);
    let at = if at != 0.0 && at.is_finite() {
        iso(Some(at))
    } else {
        Value::Null
    };
    out.insert(
        "fable".into(),
        match band {
            NO_FABLE => Value::Null,
            FABLE_OUT => json!({"left": false, "back_at": at, "band": band_name(band)}),
            _ => json!({"left": true, "resets_at": at, "band": band_name(band)}),
        },
    );
    Value::Object(out)
}

/// Antigravity CLI data folders, one account each: agy says nothing about whose it is
/// without a login, which hatcheck never reads.
pub fn load_agy_accounts(roots: &[String]) -> Vec<Entry> {
    roots
        .iter()
        .map(|root| Entry {
            agent: Agent::Agy,
            profiles: vec![root.clone()],
            email: Value::Null,
            account: Value::Null,
            cache: Value::Null,
            login: Value::Null,
        })
        .collect()
}

/// Kimi homes, one account each: the login says nothing about whose it is, and hatcheck asks
/// nothing else.
pub fn load_kimi_accounts(homes: &[String]) -> Vec<Entry> {
    homes
        .iter()
        .map(|home| Entry {
            agent: Agent::Kimi,
            profiles: vec![home.clone()],
            email: Value::Null,
            account: Value::Null,
            cache: Value::Null,
            login: crate::kimi::read_login(home),
        })
        .collect()
}

/// ZCode's data folder, one account: listed so its row can say why there's no reading.
pub fn load_zcode_accounts(roots: &[String]) -> Vec<Entry> {
    roots
        .iter()
        .map(|root| Entry {
            agent: Agent::Zcode,
            profiles: vec![root.clone()],
            email: Value::Null,
            account: Value::Null,
            cache: Value::Null,
            login: Value::Null,
        })
        .collect()
}

/// Why a ZCode folder has no reading: zcode has no usage command, and its bundle asks no usage
/// endpoint (ZCode 0.16.9); Z.ai shows the plan's usage on its site.
pub const ZCODE_NO_USAGE: &str =
    "zcode has no usage command; see z.ai/manage-apikey/subscription for the plan";

fn fetch_kimi_usage(login: &Value, url: &str) -> (Option<Value>, Option<String>) {
    fetch_json(
        url,
        &[
            (
                "Authorization",
                format!("Bearer {}", s(get(login, "access_token")).unwrap_or("")),
            ),
            ("Accept", "application/json".into()),
            ("User-Agent", "hatcheck".into()),
        ],
    )
}

/// Usage for a Kimi home: Kimi's usage endpoint, the one kimi's own /usage asks, with the
/// home's token as it is; or else the last reading hatcheck kept, since kimi saves none.
pub fn kimi_account_usage(entry: &Entry, cached: bool) -> Usage {
    let Some(home) = entry.profiles.first() else {
        return (None, None, Some("no home".into()));
    };
    let login = &entry.login;
    let why: String = if cached {
        "--cached".into()
    } else if !truthy(login) {
        "not signed in".into()
    } else if num_or_zero(get(login, "expires")) < times::now() + 60.0 {
        "token expired; run kimi in this home to renew it".into()
    } else {
        match fetch_kimi_usage(login, &crate::kimi::usage_url(home)) {
            (Some(body), _) if body.is_object() => {
                let usage = crate::kimi::usage_body(&body);
                if get(&usage, "limits")
                    .as_array()
                    .is_some_and(|l| !l.is_empty())
                {
                    crate::kimi::save_snapshot(home, &usage, times::now());
                    return (Some(usage), None, None);
                }
                "Kimi's usage endpoint reported no limits".into()
            }
            (_, why) => why
                .filter(|w| !w.is_empty())
                .unwrap_or_else(|| "unexpected answer".into()),
        }
    };
    match crate::kimi::read_snapshot(home) {
        Some((body, saved_at)) => (Some(body), Some(saved_at), Some(why)),
        None => (None, None, Some(why)),
    }
}

/// Usage for an Antigravity CLI folder: agy's own /usage run now, or else the last reading
/// hatcheck kept, since agy saves none.
pub fn agy_account_usage(entry: &Entry, cached: bool) -> Usage {
    let Some(root) = entry.profiles.first() else {
        return (None, None, Some("no folder".into()));
    };
    let why: String = if cached {
        "--cached".into()
    } else {
        // A reading from the last few minutes is handed on, so agy, the slowest to ask, isn't
        // asked by every run; and one agy run at a time across hatchecks, with the reading
        // another took while this one waited for it taken the same way.
        let fresh = |saved_at: f64| saved_at >= times::now() - crate::agy::FRESH;
        let note = format!(
            "asked at most once in {}",
            times::duration(crate::agy::FRESH)
        );
        if let Some((body, saved_at)) = crate::agy::read_snapshot(root) {
            if fresh(saved_at) {
                return (Some(body), Some(saved_at), Some(note));
            }
        }
        let (lock, waited) = crate::agy::take_run_lock();
        if waited {
            if let Some((body, saved_at)) = crate::agy::read_snapshot(root) {
                if fresh(saved_at) {
                    return (Some(body), Some(saved_at), Some(note));
                }
            }
        }
        let outcome = crate::agy::run_usage(root);
        drop(lock);
        match outcome {
            Ok(data) => {
                let body = crate::agy::usage_body(&data);
                if get(&body, "limits")
                    .as_array()
                    .is_some_and(|l| !l.is_empty())
                {
                    crate::agy::save_snapshot(root, &body, times::now());
                    return (Some(body), None, None);
                }
                "agy's /usage reported limits hatcheck couldn't read".into()
            }
            Err(why) => why,
        }
    };
    match crate::agy::read_snapshot(root) {
        Some((body, saved_at)) => (Some(body), Some(saved_at), Some(why)),
        None => (None, None, Some(why)),
    }
}

/// The plan an account is on, as the text and --json name it.
pub fn account_plan(entry: &Entry, usage: Option<&Value>) -> String {
    match entry.agent {
        Agent::Codex => codex_plan_text(entry, usage),
        Agent::Claude => plan_text(&entry.account),
        Agent::Agy => crate::agy::plan_text(usage),
        Agent::Kimi => crate::kimi::plan_text(),
        // zcode has no usage command (docs/proposals/kimi-zcode.md): the plan is the agent.
        Agent::Zcode => entry.agent.speaker().to_string(),
    }
}

/// An account's usage, fetched now or from what its agent last saved.
fn fetch_usage(entry: &Entry, cached: bool) -> Usage {
    match entry.agent {
        Agent::Codex => codex_account_usage(entry, cached),
        Agent::Claude => account_usage(entry, cached),
        Agent::Agy => agy_account_usage(entry, cached),
        Agent::Kimi => kimi_account_usage(entry, cached),
        Agent::Zcode => (None, None, Some(ZCODE_NO_USAGE.into())),
    }
}

/// The accounts behind profiles, codex_homes, agy_roots, kimi_homes and zcode_roots with their
/// usage, fetched in parallel.
pub fn load_usage(
    profiles: &[String],
    cached: bool,
    codex_homes: &[String],
    agy_roots: &[String],
    kimi_homes: &[String],
    zcode_roots: &[String],
) -> Vec<(Entry, Usage)> {
    let mut entries = load_accounts(profiles);
    entries.extend(load_codex_accounts(codex_homes));
    entries.extend(load_agy_accounts(agy_roots));
    entries.extend(load_kimi_accounts(kimi_homes));
    entries.extend(load_zcode_accounts(zcode_roots));
    // In a terminal, a line on stderr says what the wait is for, and goes when it's over.
    let note = if cached {
        None
    } else {
        asking_note(&entries, times::now())
    };
    if let Some(n) = &note {
        out::eprint(n);
    }
    let results: Vec<Usage> = std::thread::scope(|scope| {
        let handles: Vec<_> = entries
            .iter()
            .map(|e| scope.spawn(move || fetch_usage(e, cached)))
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().unwrap_or((None, None, Some("failed".into()))))
            .collect()
    });
    if let Some(n) = &note {
        out::eprint(&format!("\r{}\r", " ".repeat(n.chars().count())));
    }
    entries.into_iter().zip(results).collect()
}

/// Whether a live run asks over the network for this account's numbers, as the readers above
/// decide: not for a reading it would show again, an agent that isn't here, or a login that
/// can't be sent.
fn will_ask(entry: &Entry, now: f64) -> bool {
    let login = &entry.login;
    let token_good = || truthy(login) && num_or_zero(get(login, "expires")) >= now + 60.0;
    match entry.agent {
        Agent::Claude => ospath::which("claude").is_some() && !claude_reuses(entry, now),
        Agent::Codex => {
            token_good() && !truthy(get(login, "api_key")) && truthy(get(login, "account_id"))
        }
        Agent::Agy => {
            ospath::which("agy").is_some()
                && !entry.profiles.first().is_some_and(|root| {
                    crate::agy::read_snapshot(root)
                        .is_some_and(|(_, saved_at)| saved_at >= now - crate::agy::FRESH)
                })
        }
        Agent::Kimi => token_good(),
        Agent::Zcode => false,
    }
}

/// What a live run is about to ask, for a line on stderr while it waits: "Asking Claude Code
/// (5 accounts), Codex, agy and Kimi for usage…", clipped to the terminal's width so one `\r`
/// erases it; None when stderr isn't a terminal, under hatcheck serve, or when nothing is
/// asked over the network.
fn asking_note(entries: &[Entry], now: f64) -> Option<String> {
    use std::io::IsTerminal;
    if out::capturing() || !std::io::stderr().is_terminal() {
        return None;
    }
    let mut parts: Vec<String> = Vec::new();
    for agent in Agent::ALL {
        let n = entries
            .iter()
            .filter(|e| e.agent == agent && will_ask(e, now))
            .count();
        if n == 0 {
            continue;
        }
        let who = match agent {
            Agent::Agy => "agy".to_string(),
            Agent::Kimi => "Kimi".to_string(),
            other => other.name().to_string(),
        };
        parts.push(if n > 1 {
            format!("{who} ({n} accounts)")
        } else {
            who
        });
    }
    if parts.is_empty() {
        return None;
    }
    let names = match parts.len() {
        1 => parts[0].clone(),
        n => format!("{} and {}", parts[..n - 1].join(", "), parts[n - 1]),
    };
    let note = format!("Asking {names} for usage…");
    Some(
        note.chars()
            .take(out::columns(80).saturating_sub(1))
            .collect(),
    )
}

/// Accounts sorted by their Fable limit, as hatcheck usage lists them, and whether any has one.
pub fn sorted_by_fable(accounts: Vec<(Entry, Usage)>, now: f64) -> (Vec<(Entry, Usage)>, bool) {
    let mut ranked: Vec<((u8, f64), (Entry, Usage))> = accounts
        .into_iter()
        .map(|a| (fable_rank(a.1 .0.as_ref(), a.1 .1, now), a))
        .collect();
    let any = ranked.iter().any(|(r, _)| r.0 < NO_FABLE);
    ranked.sort_by(|a, b| {
        a.0 .0.cmp(&b.0 .0).then(
            a.0 .1
                .partial_cmp(&b.0 .1)
                .unwrap_or(std::cmp::Ordering::Equal),
        )
    });
    (ranked.into_iter().map(|(_, a)| a).collect(), any)
}

pub fn main(argv: &[String], prog: &str) -> i32 {
    let parser = Parser::new(prog, "Show how much of each plan limit is left, for every Claude Code profile (~/.claude*), Codex home (~/.codex*), Antigravity CLI folder (~/.gemini/antigravity-cli) and Kimi home (~/.kimi-code*); ZCode's folder is listed with why zcode gives none.")
        .flag(&["--cached"], "cached", "don't call the APIs; show the snapshots Claude Code and Codex last saved, and hatcheck's last readings of agy's /usage and Kimi's usage")
        .flag(&["-e", "--email"], "email", "show each account's email address (hidden by default)")
        .flag(&["--json"], "json", "print the accounts and their limits as JSON, for scripts and status bars");
    let args = match parser.parse(argv) {
        Ok(a) => a,
        Err(stop) => return stop.exit_code(),
    };
    let paint = Paint::new();
    let p = |code: &str, text: &str| paint.p(code, text);
    let now = times::now();
    let (accounts, any_fable) = sorted_by_fable(
        load_usage(
            &find_profiles(),
            args.flag("cached"),
            &find_codex_homes(),
            &paths::find_agy_roots(),
            &paths::find_kimi_homes(),
            &paths::find_zcode_roots(),
        ),
        now,
    );
    if args.flag("json") {
        let data = json!({"version": 1, "generated_at": iso(Some(now)), "sorted_by": if any_fable { json!("fable") } else { Value::Null },
                          "accounts": accounts.iter().map(|(e, r)| account_json(e, r, now, args.flag("email"))).collect::<Vec<_>>()});
        out::println(&py::dumps(&data));
        return 0;
    }
    let home_len = paths::home().chars().count();
    let mut blocks = Vec::new();
    for (entry, (usage, saved_at, why)) in &accounts {
        let names = entry
            .profiles
            .iter()
            .map(|p| format!("~{}", p.chars().skip(home_len).collect::<String>()))
            .collect::<Vec<_>>()
            .join(", ");
        let why = why.clone().unwrap_or_else(|| "None".into());
        let source = match (usage, saved_at) {
            (None, _) => p("31", "no data") + &p("2", &format!(" ({why})")),
            (Some(_), None) => p("32", "live"),
            (Some(_), Some(t)) => {
                p("33", &format!("cached {} ago", times::duration(now - t)))
                    + &p("2", &format!(" ({why})"))
            }
        };
        let plan = account_plan(entry, usage.as_ref());
        let mut details = Vec::new();
        if args.flag("email") {
            details.push(if truthy(&entry.email) {
                py::str_of(&entry.email)
            } else {
                "?".into()
            });
        }
        details.push(plan);
        let header =
            p("1;36", &names) + "  " + &p("2", &format!("{} · ", details.join(" · "))) + &source;
        let rows = usage
            .as_ref()
            .filter(|u| truthy(u))
            .map_or_else(Vec::new, usage_rows);
        blocks.push((
            header,
            rows,
            extra_usage_text(usage.as_ref().unwrap_or(&json!({}))),
            *saved_at,
        ));
    }
    let width = blocks
        .iter()
        .flat_map(|b| b.1.iter().map(|r| r.label.chars().count()))
        .max()
        .unwrap_or(0);
    let mut unlabeled = false;
    out::println(
        &(p("1", "Plan usage by profile") + &p("2", " (% left, then time left until reset)")),
    );
    if any_fable {
        out::println(&p("2", "Sorted by Fable: those with some left first, the soonest reset on top, any past 90% used behind them; then those out of it, the soonest back on top."));
    }
    for (header, rows, extra, saved_at) in &blocks {
        out::println(header);
        for row in rows {
            let t = s(&row.resets_at).and_then(times::parse_time);
            let left = percent_left(&row.used, &row.resets_at, *saved_at, now);
            let mut time_bar = " ".repeat(10); // blank unless the reset time and the window's length are known
            let (pct, plan_bar, code, when);
            match left {
                None => {
                    // The snapshot predates this window's reset, so its number is gone.
                    pct = "   ?".to_string();
                    plan_bar = "·".repeat(10);
                    code = "2".to_string();
                    when = format!(
                        "reset {}, after this snapshot",
                        times::clock(t.unwrap_or(0.0), now)
                    );
                }
                Some(left) => {
                    pct = format!("{:>3}%", left.d());
                    plan_bar = bar(left.f() / 100.0, 10);
                    code = severity_color(&row.severity)
                        .map_or(if row.labeled { "" } else { "2" }.to_string(), String::from);
                    when = match t.filter(|t| *t != 0.0) {
                        Some(t) => format!(
                            "resets {} (in {})",
                            times::clock(t, now),
                            times::duration(t - now)
                        ),
                        None => String::new(),
                    };
                    if let (Some(t), Some(w)) = (
                        t.filter(|t| *t != 0.0),
                        Num::of(&row.window).filter(|_| truthy(&row.window)),
                    ) {
                        let to_go = (t - now) / w.f(); // fraction of the window left
                        time_bar = p(
                            &if row.labeled {
                                urgency_color(to_go, paint.wide)
                            } else {
                                "2".into()
                            },
                            &bar(to_go, 10),
                        );
                    }
                }
            }
            let tail = [when.as_str(), row.note.as_str()]
                .iter()
                .filter(|t| !t.is_empty())
                .copied()
                .collect::<Vec<_>>()
                .join(" · ");
            let label = format!("{:<width$}", row.label);
            let line = format!(
                "  {} {} {}  {} {}",
                p(
                    &label_color(&row.kind, if row.labeled { "" } else { "2" }, paint.wide),
                    &label
                ),
                p(&code, &pct),
                p(&label_color(&row.kind, &code, paint.wide), &plan_bar),
                time_bar,
                if tail.is_empty() {
                    String::new()
                } else {
                    p("2", &tail)
                }
            );
            out::println(line.trim_end_matches(py::is_space));
            unlabeled |= !row.labeled;
        }
        if !extra.is_empty() {
            out::println(&format!("  {}", p("2", extra)));
        }
    }
    if unlabeled {
        out::println(&p(
            "2",
            "Dim rows are quota fields Claude Code doesn't display, shown by their raw names.",
        ));
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bars() {
        assert_eq!(bar(0.0, 10), "░".repeat(10));
        assert_eq!(bar(1.0, 10), "█".repeat(10));
        assert_eq!(bar(0.59, 10), "█████▉░░░░");
        assert_eq!(bar(0.001, 5), "▏░░░░");
        assert_eq!(bar(0.999, 5), "████▉");
    }

    #[test]
    fn urgency_colours_have_no_green() {
        let green = [34u8, 35, 70, 106, 2, 10, 22, 28, 40, 46];
        for f in 0..=20 {
            let colour = URGENCY_COLORS[((f as f64 / 20.0 * 11.0) as usize).min(10)];
            assert!(!green.contains(&colour), "{colour}");
        }
        // Not a colour the time bars use, so a per-model limit's name doesn't read as one of them.
        assert!(!URGENCY_COLORS.contains(&SCOPED_COLOR));
        assert_eq!(urgency_color(0.0, true), "38;5;160"); // red: the reset is near
        assert_eq!(urgency_color(1.0, true), "38;5;27");
        assert_eq!(
            (urgency_color(0.0, false), urgency_color(1.0, false)),
            ("31".into(), "34".into())
        );
    }

    #[test]
    fn per_model_limits_are_named_in_violet() {
        assert_eq!(label_color(&json!("weekly_scoped"), "", true), "38;5;134");
        assert_eq!(label_color(&json!("weekly_all"), "", true), "");
        assert_eq!(label_color(&json!("session"), "2", true), "2");
        assert_eq!(label_color(&json!("weekly_scoped"), "", false), "35"); // magenta in 16 colours
    }

    #[test]
    fn window_lengths() {
        assert_eq!(window_seconds("session"), Some(5 * 3600));
        assert_eq!(window_seconds("weekly_scoped"), Some(7 * 86400));
        assert_eq!(window_seconds("seven_day_opus"), Some(7 * 86400));
        assert_eq!(window_seconds("iguana_necktie"), None);
    }

    #[test]
    fn codex_rows() {
        let body = json!({"plan_type": "plus", "rate_limit": {"limit_reached": false,
            "primary_window": {"used_percent": 20, "limit_window_seconds": 18000, "reset_at": 2000000000},
            "secondary_window": {"used_percent": 80, "limit_window_seconds": 604800, "reset_at": 2000000000}},
            "additional_rate_limits": [{"limit_name": "GPT-5.3-Codex-Spark", "metered_feature": "codex_bengalfox",
                "rate_limit": {"primary_window": {"used_percent": 95, "limit_window_seconds": 18000, "reset_at": 2000000000}}}],
            "code_review_rate_limit": null, "credits": {"has_credits": true, "unlimited": false, "balance": "12.50"}});
        let usage = codex_usage_body(&body);
        let labels: Vec<(String, String, String, Value)> = get(&usage, "limits")
            .as_array()
            .unwrap()
            .iter()
            .map(|r| {
                (
                    row_label(r, false),
                    row_label(r, true),
                    py::str_of(get(r, "severity")),
                    row_window(r),
                )
            })
            .collect();
        assert_eq!(
            labels,
            vec![
                (
                    "session (5h)".into(),
                    "5h".into(),
                    "normal".into(),
                    json!(18000)
                ),
                (
                    "week (all models)".into(),
                    "week".into(),
                    "warning".into(),
                    json!(604800)
                ),
                (
                    "5h (GPT-5.3-Codex-Spark)".into(),
                    "Spark 5h".into(),
                    "critical".into(),
                    json!(18000)
                ),
            ]
        );
        assert_eq!(extra_usage_text(&usage), "credits: 12.50");
        let spark = &get(&usage, "limits")[2];
        assert_eq!(label_color(get(spark, "kind"), "", true), "38;5;134"); // violet, like Fable
        let free = codex_usage_body(
            &json!({"plan_type": "free", "rate_limit": {"primary_window": {"used_percent": 0, "limit_window_seconds": 2592000, "reset_at": 2000000000}}}),
        );
        let row = &get(&free, "limits")[0];
        assert_eq!(
            (row_label(row, false), row_label(row, true)),
            ("month (30 days)".into(), "month".into())
        );
        assert_eq!(bar_cells(&row_window(row)), 10);
    }

    /// A token shaped like the JWTs in Codex's auth.json, unsigned.
    fn fake_jwt(claims: &Value) -> String {
        let part =
            |v: &Value| base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(v.to_string());
        format!("{}.{}.x", part(&json!({"alg": "none"})), part(claims))
    }

    #[test]
    fn codex_logins_and_an_expired_token() {
        let dir =
            std::env::temp_dir().join(format!("hatcheck-codex-logins-{}", std::process::id()));
        let home = |name: &str| {
            let p = dir.join(name);
            std::fs::create_dir_all(&p).unwrap();
            p.to_string_lossy().into_owned()
        };
        let now = times::now();
        let signed_in = |name: &str, account: &str, plan: &str, expires: f64| {
            let h = home(name);
            let auth = json!({"chatgpt_account_id": account, "chatgpt_user_id": format!("user-{account}"),
                              "chatgpt_plan_type": plan});
            let claims = json!({"https://api.openai.com/auth": auth, "exp": expires as i64});
            let mut identity = claims.clone();
            identity["email"] = json!(format!("{account}@example.com"));
            let tokens = json!({"access_token": fake_jwt(&claims), "account_id": account, "refresh_token": "x",
                                "id_token": fake_jwt(&identity)});
            let auth_json = json!({"OPENAI_API_KEY": null, "tokens": tokens});
            std::fs::write(ospath::join(&h, "auth.json"), auth_json.to_string()).unwrap();
            h
        };
        let login = read_codex_login(&signed_in("x", "acct-9", "pro", now + 86400.0));
        let field = |k: &str| s(get(&login, k)).map(String::from);
        assert_eq!(
            [
                field("account_id"),
                field("user_id"),
                field("plan"),
                field("email")
            ],
            [
                Some("acct-9".into()),
                Some("user-acct-9".into()),
                Some("pro".into()),
                Some("acct-9@example.com".into())
            ]
        );
        let key = home("k");
        std::fs::write(
            ospath::join(&key, "auth.json"),
            r#"{"OPENAI_API_KEY": "sk-test", "tokens": null}"#,
        )
        .unwrap();
        assert_eq!(read_codex_login(&key), json!({"api_key": true}));
        assert!(read_codex_login(&home("none")).is_null());
        // An expired token isn't sent anywhere: the limits Codex last saved are used instead.
        let old = signed_in("old", "acct-8", "plus", now - 3600.0);
        let saved = (now - 300.0).floor();
        let rate_limits = json!({"limit_id": "codex", "limit_name": null, "plan_type": "plus",
            "primary": {"used_percent": 40, "window_minutes": 300, "resets_at": (now + 7200.0) as i64},
            "secondary": {"used_percent": 10, "window_minutes": 10080, "resets_at": (now + 3.0 * 86400.0) as i64},
            "credits": {"has_credits": false, "unlimited": false, "balance": null}});
        let folder = ospath::join_all(&old, &["sessions", "2026", "10", "17"]);
        std::fs::create_dir_all(&folder).unwrap();
        let line = json!({"timestamp": times::iso_utc(saved).unwrap(), "type": "event_msg",
                          "payload": {"type": "token_count", "info": null, "rate_limits": rate_limits}});
        let session = ospath::join(
            &folder,
            "rollout-2026-10-17T00-00-00-019f0000-aaaa-7000-8000-000000000001.jsonl",
        );
        std::fs::write(session, format!("{line}\n")).unwrap();
        let accounts = load_codex_accounts(&[old]);
        let (usage, saved_at, why) = codex_account_usage(&accounts[0], false);
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(
            why.as_deref(),
            Some("token expired; run codex in this profile to renew it")
        );
        assert!((saved_at.unwrap() - saved).abs() < 2.0, "{saved_at:?}");
        assert_eq!(
            get(&usage.unwrap(), "limits").as_array().map(Vec::len),
            Some(2)
        );
    }

    #[test]
    fn plans_from_the_account() {
        let team = json!({"organizationType": "claude_team", "userRateLimitTier": "default_claude_max_5x",
                          "organizationRateLimitTier": "default_raven"});
        assert_eq!(plan_text(&team), "team max 5x");
        let pro = json!({"organizationType": "claude_pro", "userRateLimitTier": null,
                         "organizationRateLimitTier": "default_claude_ai"});
        assert_eq!(plan_text(&pro), "pro");
        assert_eq!(
            plan_text(
                &json!({"organizationType": "claude_max", "organizationRateLimitTier": "default_claude_max_20x"})
            ),
            "max max 20x"
        );
        assert_eq!(plan_text(&json!({"accountUuid": "x"})), "?");
        assert_eq!(plan_text(&json!({})), "not signed in");
    }

    #[test]
    fn pace_is_what_is_left_per_hour_to_the_reset() {
        let now = 1_800_000_000.0;
        assert_eq!(pace(60.0, Some(now + 20.0 * 3600.0), now), Some(3.0));
        assert_eq!(pace(50.0, Some(now + 600.0), now), Some(50.0)); // at least an hour
        assert_eq!(pace(50.0, Some(now - 1.0), now), None); // reset already
        assert_eq!(pace(50.0, None, now), None);
        assert_eq!(pace_text(0.456), "0.5%/h");
    }

    #[test]
    fn fable_bands() {
        let now = 1_800_000_000.0;
        let fable = |used: f64, days: f64| {
            json!({"limits": [{"kind": "weekly_scoped", "group": "weekly", "percent": used,
                               "resets_at": times::iso_utc(now + days * 86400.0).unwrap(),
                               "scope": {"model": {"display_name": "Fable"}}}]})
        };
        let band = |used: f64| fable_rank(Some(&fable(used, 2.0)), None, now).0;
        assert_eq!(band(0.0), FABLE_FRESH);
        assert_eq!(band(89.0), FABLE_FRESH);
        assert_eq!(band(90.0), FABLE_LOW);
        assert_eq!(band(97.0), FABLE_LOW);
        assert_eq!(band(98.0), FABLE_NEARLY_OUT);
        assert_eq!(band(99.5), FABLE_NEARLY_OUT);
        assert_eq!(band(100.0), FABLE_OUT);
        assert_eq!(
            fable_rank(Some(&json!({"limits": []})), None, now).0,
            NO_FABLE
        );
        // Within a band the soonest reset comes first, as before.
        let (a, b) = (
            fable_rank(Some(&fable(50.0, 1.0)), None, now),
            fable_rank(Some(&fable(10.0, 3.0)), None, now),
        );
        assert!(a < b);
        assert_eq!(band_name(FABLE_NEARLY_OUT), "nearly out");
    }

    #[test]
    fn the_claude_run_asks_for_nothing_but_usage() {
        assert_eq!(CLAUDE_USAGE_ARGS[..2], ["-p", "/usage"]);
        assert!(CLAUDE_USAGE_ARGS.contains(&"--no-session-persistence"));
        assert!(CLAUDE_USAGE_ARGS.contains(&"--strict-mcp-config"));
        // What claude -p --output-format json prints: one object, or every message when verbose.
        assert_eq!(
            claude_result(
                r#"{"type":"result","is_error":false,"result":"Current session: 11% used"}"#
            )
            .as_deref(),
            Some("Current session: 11% used")
        );
        assert_eq!(
            claude_result(r#"[{"type":"system"},{"type":"result","result":"x"}]"#).as_deref(),
            Some("x")
        );
        assert_eq!(claude_result("not json"), None);
    }
}
