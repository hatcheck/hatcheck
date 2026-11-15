//! hatcheck insights: local plan-limit and spending history, with no usage requests. The report
//! uses Hatcheck's 30 days of readings and the sessions' own recorded costs; missing readings are
//! unknown, never zero. docs/commands.md's `hatcheck insights` section describes the report.

use crate::args::Parser;
use crate::facts::{self, Summary};
use crate::find;
use crate::history::Reading;
use crate::ospath;
use crate::out;
use crate::py;
use crate::sessions;
use crate::times;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
struct Spend {
    sessions: BTreeSet<String>,
    cost: f64,
}

type Spending = (
    BTreeSet<String>,
    BTreeSet<String>,
    f64,
    Vec<Value>,
    Vec<Value>,
    Vec<Value>,
);

fn account(account: &str) -> (String, String) {
    account
        .split_once(':')
        .map(|(agent, profile)| (agent.to_string(), profile.to_string()))
        .unwrap_or_else(|| ("?".into(), account.to_string()))
}

fn limit_series(days: u64, unused: f64, now: f64) -> (Vec<Value>, Vec<Value>) {
    let since = now - days as f64 * 86400.0;
    let mut grouped: BTreeMap<(String, String), Vec<Reading>> = BTreeMap::new();
    for reading in crate::history::readings()
        .into_iter()
        .filter(|r| r.t >= since)
    {
        grouped
            .entry((reading.account.clone(), reading.label.clone()))
            .or_default()
            .push(reading);
    }
    let mut series = Vec::new();
    let mut forecasts = Vec::new();
    for ((account_key, label), mut readings) in grouped {
        readings.sort_by(|a, b| a.t.total_cmp(&b.t));
        let trend = crate::history::trend_of(&readings, now);
        let Some(newest) = readings.last() else {
            continue;
        };
        let (agent, profile) = account(&account_key);
        let base = json!({"agent": agent, "profile": profile, "label": label});
        if let Some(trend) = &trend {
            if trend.before_reset == Some(true) {
                forecasts.push(json!({"kind": "runs_out", "account": base, "pace_percent_per_hour": trend.pace,
                    "runs_out_at": trend.runs_out_at.and_then(times::iso_utc), "resets_at": newest.resets_at.and_then(times::iso_utc)}));
            }
        }
        let seconds_left = newest.resets_at.map(|reset| reset - now).unwrap_or(-1.0);
        let left = 100.0 - newest.used;
        if seconds_left > 0.0
            && left >= unused
            && seconds_left
                <= newest
                    .window
                    .map_or(86400.0, |window| window / 5.0)
                    .min(86400.0)
        {
            forecasts.push(json!({"kind": "unused", "account": base, "left_percent": left,
                "resets_at": newest.resets_at.and_then(times::iso_utc), "pace_percent_per_hour": trend.as_ref().map(|t| t.pace)}));
        }
        series.push(json!({"account": base, "readings": readings.iter().map(|r|
            json!({"time": times::iso_utc(r.t), "used_percent": r.used, "resets_at": r.resets_at.and_then(times::iso_utc)})).collect::<Vec<_>>(),
            "trend": crate::history::to_json(trend.as_ref())}));
    }
    (series, forecasts)
}

fn add_spend(map: &mut BTreeMap<String, Spend>, key: String, s: &Summary) {
    let entry = map.entry(key).or_default();
    entry.sessions.insert(format!("{}:{}", s.agent.key(), s.id));
    entry.cost += s.cost.unwrap_or(0.0);
}

fn spend_json(name: &str, map: BTreeMap<String, Spend>) -> Vec<Value> {
    let mut rows: Vec<(String, Spend)> = map.into_iter().collect();
    rows.sort_by(|a, b| b.1.cost.total_cmp(&a.1.cost).then_with(|| a.0.cmp(&b.0)));
    rows.into_iter()
        .map(|(key, spend)| {
            json!({name: key, "session_count": spend.sessions.len(), "cost_usd": spend.cost})
        })
        .collect()
}

fn spending(days: u64) -> Spending {
    let cutoff = times::now() - days as f64 * 86400.0;
    let mut sessions_by_id: BTreeMap<String, Summary> = BTreeMap::new();
    for ((agent, id), places) in find::session_copies("") {
        for (profile, path) in places {
            let Some(s) = facts::summarize(agent, &profile, &path) else {
                continue;
            };
            if !s.has_messages || s.when < cutoff || s.entrypoint_text() == "subagent" {
                continue;
            }
            let key = format!("{}:{}", agent.key(), id);
            if sessions_by_id.get(&key).is_none_or(|old| {
                (s.when, s.canonical, s.size) > (old.when, old.canonical, old.size)
            }) {
                sessions_by_id.insert(key, s);
            }
        }
    }
    facts::save_cache();
    let mut costed = BTreeSet::new();
    let mut total = 0.0;
    let (mut profiles, mut projects, mut models) =
        (BTreeMap::new(), BTreeMap::new(), BTreeMap::new());
    for (key, s) in &sessions_by_id {
        if let Some(cost) = s.cost {
            costed.insert(key.clone());
            total += cost;
        }
        add_spend(&mut profiles, ospath::abspath(&s.profile), s);
        add_spend(
            &mut projects,
            s.cwd.clone().unwrap_or_else(|| "unrecorded".into()),
            s,
        );
        let model = py::s(&s.model)
            .filter(|m| !m.is_empty())
            .unwrap_or("unrecorded");
        add_spend(&mut models, model.to_string(), s);
    }
    let session_ids = sessions_by_id.into_keys().collect();
    (
        session_ids,
        costed,
        total,
        spend_json("profile", profiles),
        spend_json("project", projects),
        spend_json("model", models),
    )
}

fn report(days: u64, unused: f64) -> Value {
    let now = times::now();
    let (series, forecasts) = limit_series(days, unused, now);
    let (session_ids, costed, cost, profiles, projects, models) = spending(days);
    json!({"version": 1, "generated_at": times::iso_utc(now), "days": days,
        "unused_threshold_percent": unused, "series": series, "forecasts": forecasts,
        "weekly": {"session_count": session_ids.len(), "costed_session_count": costed.len(), "cost_usd": cost},
        "profiles": profiles, "projects": projects, "models": models})
}

pub fn main(argv: &[String], prog: &str) -> i32 {
    let parser = Parser::new(prog, "Show local plan-limit history, run-out and unused-quota forecasts, spending by profile, project and model, and a weekly session summary. Reads only Hatcheck's history and agents' session files; asks for no live usage.")
        .option(&["--days"], "days", "N", "whole days of history to include, 1 to 30 (default 7)")
        .option(&["--unused"], "unused", "PERCENT", "flag a limit resetting soon with at least this much left (default 50; 100 effectively disables it)")
        .flag(&["-j", "--json"], "json", "print the report as JSON");
    let args = match parser.parse(argv) {
        Ok(args) => args,
        Err(stop) => return stop.exit_code(),
    };
    let days = match args.value("days") {
        None => 7,
        Some(text) => match text.parse::<u64>() {
            Ok(days) if (1..=30).contains(&days) => days,
            _ => return parser.error("argument --days: choose 1 to 30").exit_code(),
        },
    };
    let unused = match args.value("unused") {
        None => 50.0,
        Some(text) => match text.parse::<f64>() {
            Ok(value) if (0.0..=100.0).contains(&value) => value,
            _ => {
                return parser
                    .error("argument --unused: choose 0 to 100")
                    .exit_code()
            }
        },
    };
    let report = report(days, unused);
    if args.flag("json") {
        out::println(&py::dumps(&report));
        return 0;
    }
    out::heading(&format!("Usage insights, last {days} days (local only)"));
    let series = report["series"].as_array().map_or(&[][..], Vec::as_slice);
    if series.is_empty() {
        out::human(
            "No plan readings in this period; hatcheck usage records them as it reads plans.",
        );
    }
    for forecast in report["forecasts"].as_array().into_iter().flatten() {
        let account = &forecast["account"];
        let kind = py::s(py::get(forecast, "kind")).unwrap_or("");
        let profile = py::s(py::get(account, "profile")).unwrap_or("?");
        let label = py::s(py::get(account, "label")).unwrap_or("?");
        if kind == "runs_out" {
            out::warn(&format!(
                "{profile} · {label}: runs out {}, at {}%/h",
                py::s(py::get(forecast, "runs_out_at")).unwrap_or("?"),
                forecast["pace_percent_per_hour"]
            ));
        } else {
            out::warn(&format!(
                "{profile} · {label}: {}% left, resets {}",
                py::g(forecast["left_percent"].as_f64().unwrap_or(0.0)),
                py::s(py::get(forecast, "resets_at")).unwrap_or("?")
            ));
        }
    }
    let weekly = &report["weekly"];
    out::human(&format!(
        "Sessions: {} · recorded costs: {} · {}",
        weekly["session_count"],
        weekly["costed_session_count"],
        sessions::dollars(weekly["cost_usd"].as_f64().unwrap_or(0.0))
    ));
    for (title, key, name) in [
        ("Profiles", "profiles", "profile"),
        ("Projects", "projects", "project"),
        ("Models", "models", "model"),
    ] {
        let rows = report[key].as_array().map_or(&[][..], Vec::as_slice);
        if rows.is_empty() {
            continue;
        }
        out::human(&format!("{title}:"));
        for row in rows.iter().take(5) {
            out::human(&format!(
                "  {}  {} · {} sessions",
                py::s(py::get(row, name)).unwrap_or("?"),
                sessions::dollars(row["cost_usd"].as_f64().unwrap_or(0.0)),
                row["session_count"]
            ));
        }
    }
    0
}
