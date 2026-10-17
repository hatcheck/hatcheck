//! hatcheck usage [--cached] [--email] [--json]: how much of each plan limit is left, for
//! every Claude Code profile (~/.claude*) and every Codex home (~/.codex*). The port of
//! hatcheck/usage.py, whose docstring says what it shows; the session list uses it too.
//!
//! A profile's OAuth token, from the macOS Keychain or the profile's .credentials.json, is
//! sent as is to the endpoint Claude Code's /usage asks, and never refreshed: a refresh
//! rotates it and would sign out the copy Claude Code holds. Without a live answer the
//! snapshot Claude Code saved in the profile's .claude.json is used. A Codex home's ChatGPT
//! token goes to the endpoint Codex's /status asks, or else the newest limits Codex wrote in
//! the home's session files are used.

use crate::args::Parser;
use crate::ospath::{self, Part};
use crate::out::{self, Paint};
use crate::paths;
use crate::py::{self, get, s, truthy, Num};
use crate::times;
use base64::Engine;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::time::Duration;

pub const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
pub const CODEX_USAGE_URL: &str = "https://chatgpt.com/backend-api/wham/usage";
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

/// Claude Code's Keychain item for a profile, suffixed with a hash of its CLAUDE_CONFIG_DIR.
pub fn keychain_service(profile: &str) -> String {
    if profile == default_profile() {
        return "Claude Code-credentials".into();
    }
    let digest = Sha256::digest(profile.as_bytes());
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    format!("Claude Code-credentials-{}", &hex[..8])
}

/// Run a program and give its stdout, or None when it can't be run or doesn't finish in time.
fn output_within(program: &str, args: &[&str], timeout: Duration) -> Option<String> {
    use std::process::{Command, Stdio};
    use std::sync::{mpsc, Arc, Mutex};
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut stdout = child.stdout.take()?;
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
    let read = std::io::Read::read_to_string(&mut stdout, &mut text).is_ok();
    let _ = done.send(());
    let killed = watchdog.join().unwrap_or(true);
    if let Ok(mut c) = child.lock() {
        let _ = c.wait();
    }
    (read && !killed).then_some(text)
}

/// The claudeAiOauth entry (token, expiry, plan), or null: from the macOS Keychain, or where
/// there's no keychain (Linux, Windows) from the profile's .credentials.json.
pub fn read_oauth(profile: &str) -> Value {
    if cfg!(target_os = "macos") {
        let service = keychain_service(profile);
        if let Some(out) = output_within(
            "security",
            &["find-generic-password", "-s", &service, "-w"],
            Duration::from_secs(15),
        ) {
            if let Ok(v) = serde_json::from_str::<Value>(&out) {
                let oauth = get(&v, "claudeAiOauth");
                if truthy(oauth) {
                    return oauth.clone();
                }
            }
        }
    }
    std::fs::read_to_string(ospath::join(profile, ".credentials.json"))
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .map_or(Value::Null, |v| get(&v, "claudeAiOauth").clone())
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

fn fetch_usage(token: &str) -> (Option<Value>, Option<String>) {
    fetch_json(
        USAGE_URL,
        &[
            ("Authorization", format!("Bearer {token}")),
            ("anthropic-beta", "oauth-2025-04-20".into()),
            ("Content-Type", "application/json".into()),
            ("User-Agent", "hatcheck".into()),
        ],
    )
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
pub fn urgency_color(fraction: f64) -> String {
    let n = URGENCY_COLORS.len();
    let step = ((fraction.max(0.0) * n as f64) as usize).min(n - 1);
    if has_256_colors() {
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

/// Colour for a limit's name: violet for a per-model limit such as Fable, else default.
pub fn label_color(kind: &Value, default: &str) -> String {
    if !s(kind).unwrap_or("").ends_with("_scoped") {
        return default.into();
    }
    if has_256_colors() {
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

pub fn plan_text(oauth: &Value) -> String {
    if !truthy(oauth) {
        return "not signed in".into();
    }
    let sub = get(oauth, "subscriptionType");
    let mut plan = if truthy(sub) {
        py::str_of(sub)
    } else {
        "?".into()
    };
    let tier = s(get(oauth, "rateLimitTier")).unwrap_or("");
    for multiple in ["20x", "5x"] {
        if tier.ends_with(&format!("max_{multiple}")) {
            plan += &format!(" max {multiple}");
        }
    }
    plan
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

/// Where an account goes in the list: 0 with Fable left, the soonest Fable reset on top; 1 out
/// of Fable or of the weekly limit for all models it counts against, the soonest back on top;
/// 2 without a Fable limit, in the usual order.
pub fn fable_rank(usage: Option<&Value>, saved_at: Option<f64>, now: f64) -> (u8, f64) {
    let limits: Vec<&Value> = match usage.map(|u| get(u, "limits")) {
        Some(Value::Array(rows)) => rows.iter().filter(|r| r.is_object()).collect(),
        _ => Vec::new(),
    };
    let Some(fable) = limits.iter().find(|r| is_fable(r)) else {
        return (2, 0.0);
    };
    let mut needed = vec![meter_state(fable, saved_at, now)];
    needed.extend(
        limits
            .iter()
            .filter(|r| s(get(r, "kind")) == Some("weekly_all"))
            .map(|r| meter_state(r, saved_at, now)),
    );
    if needed.iter().all(|(left, _)| *left > 0.0) {
        return (
            0,
            needed[0].1.filter(|r| *r != 0.0).unwrap_or(f64::INFINITY),
        );
    }
    let back = needed
        .iter()
        .filter(|(left, _)| *left <= 0.0)
        .map(|(_, r)| r.filter(|r| *r != 0.0).unwrap_or(f64::INFINITY))
        .fold(f64::NEG_INFINITY, f64::max);
    (1, back)
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
    pub codex: bool,
    pub profiles: Vec<String>,
    pub email: Value,
    pub oauth: Value,
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
    // The logins first, all at once: each is a Keychain lookup on macOS, a program of its own.
    let logins: Vec<Value> = std::thread::scope(|scope| {
        let handles: Vec<_> = profiles
            .iter()
            .map(|p| scope.spawn(move || read_oauth(p)))
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().unwrap_or(Value::Null))
            .collect()
    });
    for (profile, oauth) in profiles.iter().zip(logins) {
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
                    codex: false,
                    profiles: Vec::new(),
                    email: get(&account, "emailAddress").clone(),
                    oauth: Value::Null,
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
        if truthy(&oauth)
            && (entry.oauth.is_null()
                || num_or_zero(get(&oauth, "expiresAt"))
                    > num_or_zero(get(&entry.oauth, "expiresAt")))
        {
            entry.oauth = oauth;
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

/// The live usage body, or else Claude Code's snapshot and when it was saved, and why it isn't live.
pub fn account_usage(entry: &Entry, cached: bool) -> Usage {
    let oauth = &entry.oauth;
    let why: String = if cached {
        "--cached".into()
    } else if !truthy(oauth) || !truthy(get(oauth, "accessToken")) {
        "not signed in".into()
    } else if num_or_zero(get(oauth, "expiresAt")) / 1000.0 < times::now() + 60.0 {
        "token expired; run claude in this profile to renew it".into()
    } else {
        match fetch_usage(s(get(oauth, "accessToken")).unwrap_or("")) {
            (Some(usage), _) if usage.is_object() => return (Some(usage), None, None),
            (Some(_), _) => "unexpected answer".into(),
            (None, why) => why.unwrap_or_default(),
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
                    codex: true,
                    profiles: Vec::new(),
                    email: Value::Null,
                    oauth: Value::Null,
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
/// a snapshot's limit that has reset since it was saved.
pub fn limits_json(usage: &Value, saved_at: Option<f64>, now: f64) -> Value {
    let mut out = Vec::new();
    if let Value::Array(rows) = get(usage, "limits") {
        for r in rows.iter().filter(|r| r.is_object()) {
            out.push(json!({"name": row_label(r, false), "short": row_label(r, true), "kind": get(r, "kind"),
                            "used_percent": get(r, "percent"),
                            "left_percent": percent_left(get(r, "percent"), get(r, "resets_at"), saved_at, now).map_or(Value::Null, Num::value),
                            "resets_at": iso(s(get(r, "resets_at")).and_then(times::parse_time)), "window_seconds": row_window(r),
                            "severity": get(r, "severity"), "per_model": py::str_of(get(r, "kind")).ends_with("_scoped") && !get(r, "kind").is_null()}));
        }
    }
    for row in usage_rows(usage).into_iter().filter(|r| !r.labeled) {
        out.push(json!({"name": row.label, "short": row.label, "kind": null, "used_percent": row.used,
                        "left_percent": percent_left(&row.used, &row.resets_at, saved_at, now).map_or(Value::Null, Num::value),
                        "resets_at": iso(s(&row.resets_at).and_then(times::parse_time)), "window_seconds": row.window, "severity": null,
                        "per_model": false, "raw": true, "note": if row.note.is_empty() { Value::Null } else { json!(row.note) }}));
    }
    Value::Array(out)
}

/// One account for --json: its profiles or homes, plan, where the numbers come from, its
/// limits, and where it stands on Fable (null without a Fable limit).
pub fn account_json(entry: &Entry, result: &Usage, now: f64, email: bool) -> Value {
    let (usage, saved_at, why) = result;
    let mut out = Map::new();
    out.insert(
        "agent".into(),
        json!(if entry.codex { "codex" } else { "claude" }),
    );
    out.insert(
        "profiles".into(),
        json!(entry
            .profiles
            .iter()
            .map(|p| ospath::abspath(p))
            .collect::<Vec<_>>()),
    );
    out.insert(
        "plan".into(),
        json!(if entry.codex {
            codex_plan_text(entry, usage.as_ref())
        } else {
            plan_text(&entry.oauth)
        }),
    );
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
    let (group, at) = fable_rank(usage.as_ref(), *saved_at, now);
    let at = if at != 0.0 && at.is_finite() {
        iso(Some(at))
    } else {
        Value::Null
    };
    out.insert(
        "fable".into(),
        match group {
            2 => Value::Null,
            0 => json!({"left": true, "resets_at": at}),
            _ => json!({"left": false, "back_at": at}),
        },
    );
    Value::Object(out)
}

/// The accounts behind profiles and codex_homes with their usage, fetched in parallel.
pub fn load_usage(
    profiles: &[String],
    cached: bool,
    codex_homes: &[String],
) -> Vec<(Entry, Usage)> {
    let mut entries = load_accounts(profiles);
    entries.extend(load_codex_accounts(codex_homes));
    let results: Vec<Usage> = std::thread::scope(|scope| {
        let handles: Vec<_> = entries
            .iter()
            .map(|e| {
                scope.spawn(move || {
                    if e.codex {
                        codex_account_usage(e, cached)
                    } else {
                        account_usage(e, cached)
                    }
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().unwrap_or((None, None, Some("failed".into()))))
            .collect()
    });
    entries.into_iter().zip(results).collect()
}

/// Accounts sorted by their Fable limit, as hatcheck usage lists them, and whether any has one.
pub fn sorted_by_fable(accounts: Vec<(Entry, Usage)>, now: f64) -> (Vec<(Entry, Usage)>, bool) {
    let mut ranked: Vec<((u8, f64), (Entry, Usage))> = accounts
        .into_iter()
        .map(|a| (fable_rank(a.1 .0.as_ref(), a.1 .1, now), a))
        .collect();
    let any = ranked.iter().any(|(r, _)| r.0 < 2);
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
    let parser = Parser::new(prog, "Show how much of each plan limit is left, for every Claude Code profile (~/.claude*) and Codex home (~/.codex*).")
        .flag(&["--cached"], "cached", "don't call the APIs; show the snapshots Claude Code and Codex last saved")
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
        load_usage(&find_profiles(), args.flag("cached"), &find_codex_homes()),
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
        let plan = if entry.codex {
            codex_plan_text(entry, usage.as_ref())
        } else {
            plan_text(&entry.oauth)
        };
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
        out::println(&p("2", "Sorted by Fable: those with some left first, the soonest reset on top; then those out of it, the soonest back on top."));
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
                                urgency_color(to_go)
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
                    &label_color(&row.kind, if row.labeled { "" } else { "2" }),
                    &label
                ),
                p(&code, &pct),
                p(&label_color(&row.kind, &code), &plan_bar),
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

    #[test]
    fn keychain_names() {
        // The hash is of the profile's path, as Claude Code makes it.
        assert!(keychain_service("/x/.claude-work").starts_with("Claude Code-credentials-"));
        assert_eq!(
            keychain_service("/x/.claude-work").len(),
            "Claude Code-credentials-".len() + 8
        );
    }
}
