//! hatcheck export: one session or a project/date batch as a self-contained Markdown, HTML or
//! JSON document. Source and destination metadata and a private-content warning travel with each.
//! docs/commands.md's `hatcheck export` section describes selection, filenames and --json reports.

use crate::args::{Count, Parser};
use crate::conv::{self, Conversation};
use crate::facts::{self, Summary};
use crate::find;
use crate::open_cmd::{pick, Pick};
use crate::ospath;
use crate::out;
use crate::paths;
use crate::py;
use crate::show;
use crate::times;
use serde_json::{json, Value};
use std::collections::BTreeMap;

const WARNING: &str =
    "This session may contain secrets or private source code. Review it before sharing.";

#[derive(Clone, Copy, PartialEq)]
enum Format {
    Markdown,
    Html,
    Json,
}

impl Format {
    fn parse(text: &str) -> Option<Self> {
        match text {
            "markdown" | "md" => Some(Self::Markdown),
            "html" => Some(Self::Html),
            "json" => Some(Self::Json),
            _ => None,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Markdown => "markdown",
            Self::Html => "html",
            Self::Json => "json",
        }
    }

    fn extension(self) -> &'static str {
        match self {
            Self::Markdown => ".md",
            Self::Html => ".html",
            Self::Json => ".json",
        }
    }
}

fn source(s: &Summary) -> Value {
    json!({"agent": s.agent.key(), "id": s.id, "profile": ospath::abspath(&s.profile),
        "title": s.title, "cwd": s.cwd, "model": s.model})
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn document(
    s: &Summary,
    conv: &Conversation,
    format: Format,
    brief: bool,
    path: Option<&str>,
) -> String {
    let turns = if brief {
        conv::brief_turns(conv)
    } else {
        conv.turns.clone()
    };
    let total = turns.len();
    match format {
        Format::Markdown => {
            let mut text = show::markdown(s, conv, &turns, total, brief);
            text += &format!(
                "\nDestination: {}. Format: markdown.\n\n> {WARNING}\n",
                path.unwrap_or("stdout")
            );
            text
        }
        Format::Html => {
            let title = if s.title.is_empty() {
                "Untitled session"
            } else {
                &s.title
            };
            let mut html = format!("<!doctype html>\n<html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>{}</title>\n<style>body{{max-width:900px;margin:32px auto;padding:0 18px;font:15px/1.55 system-ui,sans-serif;color:#1d1d1f}}.meta,.warning,.time{{color:#6e6e73}}.warning{{border-left:3px solid #d99000;padding-left:10px}}.turn{{border-top:1px solid #ddd;padding-top:14px;margin-top:14px}}pre{{white-space:pre-wrap;overflow-wrap:anywhere;font:13px/1.5 ui-monospace,monospace}}.tool{{background:#f5f5f7;padding:10px;border-radius:6px}}</style></head><body>\n<h1>{}</h1>\n<p class=\"meta\">{} session <code>{}</code> in <code>{}</code>; started from <code>{}</code>; {} turns; {} tool calls.</p>\n<p class=\"meta\">Exported by hatcheck {} on {}. Left out: {}.</p>\n<p class=\"warning\">{}</p>\n",
                escape(title), escape(title), s.agent.name(), escape(&s.id), escape(&s.profile),
                escape(s.cwd.as_deref().unwrap_or("?")), total, conv.calls,
                crate::cli::version(), times::iso_utc(times::now()).unwrap_or_else(|| "?".into()), escape(conv.left_out), WARNING);
            for (role, texts, when) in turns {
                let who = if role == "user" {
                    "You"
                } else {
                    s.agent.speaker()
                };
                let at = py::s(&when)
                    .filter(|w| !w.is_empty())
                    .map_or(String::new(), |w| {
                        format!(" · {}", py::head(w, 16).replace('T', " "))
                    });
                html += &format!(
                    "<section class=\"turn\"><h2>{}{}</h2>\n",
                    escape(who),
                    escape(&at)
                );
                for part in texts {
                    let class = if part.contains("[external_agent_tool_") {
                        " class=\"tool\""
                    } else {
                        ""
                    };
                    html += &format!("<pre{class}>{}</pre>\n", escape(part.trim_matches('\n')));
                }
                html += "</section>\n";
            }
            html += &format!("<p class=\"meta\">Source: {}:{}:{}. Destination: {}. Format: html.</p>\n</body></html>\n",
                s.agent.key(), escape(&s.profile), escape(&s.id), escape(path.unwrap_or("stdout")));
            html
        }
        Format::Json => {
            py::dumps(
                &json!({"version": 1, "generated_at": times::iso_utc(times::now()),
            "source": source(s), "export": {"format": format.name(), "path": path.map(ospath::abspath)},
            "warning": WARNING, "brief": brief, "turns_total": total, "tool_calls": conv.calls,
            "left_out": conv.left_out,
            "turns": turns.iter().map(|(role, texts, when)| json!({"role": role, "text": texts.join("\n\n"), "time": when})).collect::<Vec<_>>() }),
            ) + "\n"
        }
    }
}

fn collect(project: Option<&str>, hours: f64, all: bool) -> Result<Vec<Summary>, String> {
    let cutoff = (hours > 0.0).then(|| times::now() - hours * 3600.0);
    let project = project.map(|p| ospath::realpath(&ospath::abspath(&ospath::expanduser(p))));
    let mut found: BTreeMap<String, Summary> = BTreeMap::new();
    for ((agent, id), places) in find::session_copies("") {
        for (profile, path) in places {
            let Some(s) = facts::summarize(agent, &profile, &path) else {
                continue;
            };
            if !s.has_messages
                || cutoff.is_some_and(|cutoff| s.when < cutoff)
                || project.as_ref().is_some_and(|p| {
                    s.cwd
                        .as_deref()
                        .is_none_or(|cwd| !paths::same_folder(cwd, p))
                })
                || (!all && s.entrypoint_text() != "cli")
                || s.entrypoint_text() == "subagent"
            {
                continue;
            }
            let key = format!("{}:{}:{}", agent.key(), ospath::realpath(&profile), id);
            if found.get(&key).is_none_or(|old| {
                (s.when, s.canonical, s.size) > (old.when, old.canonical, old.size)
            }) {
                found.insert(key, s);
            }
        }
    }
    facts::save_cache();
    let mut sessions: Vec<Summary> = found.into_values().collect();
    sessions.sort_by(|a, b| {
        b.when
            .total_cmp(&a.when)
            .then_with(|| a.agent.key().cmp(b.agent.key()))
            .then_with(|| a.id.cmp(&b.id))
    });
    Ok(sessions)
}

fn slug(text: &str) -> String {
    let slug: String = text
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let slug = slug.trim_matches('-');
    if slug.is_empty() {
        "session".into()
    } else {
        slug.chars().take(36).collect()
    }
}

fn filename(s: &Summary, format: Format) -> String {
    let profile = slug(&ospath::basename(&s.profile));
    format!(
        "{}-{}-{}-{}-{}{}",
        s.agent.key(),
        profile,
        py::head(&s.id, 8),
        slug(&s.title),
        format.extension(),
        ""
    )
}

fn write_file(path: &str, text: &str, replace: bool, dry_run: bool) -> Result<u64, String> {
    if ospath::lexists(path) && !replace {
        return Err(format!(
            "{} already exists; choose another folder or use --replace",
            paths::shell_path(path)
        ));
    }
    if dry_run {
        return Ok(text.len() as u64);
    }
    std::fs::create_dir_all(ospath::dirname(path))
        .map_err(|e| format!("can't make the export folder: {e}"))?;
    let tmp = format!("{path}.hatcheck-export-tmp");
    std::fs::write(&tmp, text)
        .map_err(|e| format!("can't write {}: {e}", paths::shell_path(&tmp)))?;
    let renamed = (|| {
        if replace && ospath::isfile(path) {
            std::fs::remove_file(path)?; // Windows can't rename over an existing file
        }
        std::fs::rename(&tmp, path)
    })();
    if renamed.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    renamed.map_err(|e| format!("can't write {}: {e}", paths::shell_path(path)))?;
    std::fs::metadata(path)
        .map(|m| m.len())
        .map_err(|e| format!("can't check {}: {e}", paths::shell_path(path)))
}

pub fn main(argv: &[String], prog: &str) -> i32 {
    let parser = Parser::new(prog, "Export one session or a project/date batch as a self-contained Markdown, HTML or JSON document. Every document includes source and destination metadata and a warning that sessions may contain secrets or private source code.")
        .positional("session", Count::Optional, "one session ID or its start; leave out for a batch")
        .option(&["--from"], "source", "PROFILE", "the profile or home to read SESSION from")
        .option(&["--project"], "project", "FOLDER", "export only sessions started in this project folder")
        .option(&["--hours"], "hours", "N", "export sessions active in the last N hours (default 24; 0 for all time; 3d works too)")
        .option(&["--format"], "format", "KIND", "markdown (the default), html or json")
        .option(&["--output"], "output", "PATH", "a file for one session, or a directory for a batch or one session")
        .flag(&["--replace"], "replace", "replace files that already exist")
        .flag(&["--brief"], "brief", "export the handover brief instead of the whole conversation")
        .flag(&["-a", "--all"], "all", "include Desktop, SDK and other non-CLI sessions in a batch")
        .flag(&["--dry-run"], "dry_run", "report what would be exported without writing files")
        .flag(&["-j", "--json"], "json", "print the export report as JSON; --output is required");
    let args = match parser.parse(argv) {
        Ok(args) => args,
        Err(stop) => return stop.exit_code(),
    };
    let format = match Format::parse(args.value("format").unwrap_or("markdown")) {
        Some(format) => format,
        None => {
            return parser
                .error("argument --format: choose markdown, html or json")
                .exit_code()
        }
    };
    let hours = match sessions_hours(args.value("hours").unwrap_or("24")) {
        Some(hours) => hours,
        None => {
            return parser
                .error("argument --hours: expected 0 or a positive number of hours")
                .exit_code()
        }
    };
    let session = args.value("session").filter(|s| !s.is_empty());
    if session.is_some()
        && (args.value("project").is_some() || args.value("hours").is_some() || args.flag("all"))
    {
        return parser
            .error("SESSION can't be combined with --project, --hours or --all")
            .exit_code();
    }
    if args.flag("json") && args.value("output").is_none() {
        return parser
            .error("--json reports need --output so the document has a destination")
            .exit_code();
    }
    if args.flag("dry_run") && args.value("output").is_none() {
        return parser
            .error("--dry-run needs --output to describe the destination")
            .exit_code();
    }
    let batch = session.is_none();
    let sessions = if let Some(id) = session {
        match pick(&Pick {
            session: Some(id),
            source: args.value("source"),
            all: true,
        }) {
            Ok(s) => vec![s],
            Err(message) => return out::fail(&message),
        }
    } else {
        match collect(args.value("project"), hours, args.flag("all")) {
            Ok(sessions) if sessions.is_empty() => {
                return out::fail("No sessions match this export")
            }
            Ok(sessions) => sessions,
            Err(message) => return out::fail(&message),
        }
    };
    let output = args.value("output");
    let output_is_dir =
        batch || output.is_some_and(|p| ospath::isdir(p) || p.ends_with(['/', '\\']));
    if !output_is_dir && sessions.len() > 1 {
        return parser
            .error("a batch needs a directory for --output")
            .exit_code();
    }
    if output_is_dir && output.is_none() {
        return parser.error("a batch needs --output DIRECTORY").exit_code();
    }
    let dry_run = args.flag("dry_run");
    let mut exported = Vec::new();
    let mut paths_seen = BTreeMap::new();
    for s in &sessions {
        let path = output.map(|base| {
            if output_is_dir {
                ospath::join(
                    &ospath::abspath(&ospath::expanduser(base)),
                    &filename(s, format),
                )
            } else {
                ospath::abspath(&ospath::expanduser(base))
            }
        });
        if let Some(path) = &path {
            if paths_seen.insert(path.clone(), ()).is_some() {
                return out::fail(&format!(
                    "Two sessions would export to the same file: {path}"
                ));
            }
        }
        let conv = match conv::read_for(s) {
            Ok(conv) => conv,
            Err(e) => return out::fail(&format!("Can't read {}: {e}", s.id)),
        };
        let text = document(s, &conv, format, args.flag("brief"), path.as_deref());
        let bytes = if let Some(path) = &path {
            match write_file(path, &text, args.flag("replace"), dry_run) {
                Ok(bytes) => bytes,
                Err(message) => return out::fail(&message),
            }
        } else {
            out::print(&text);
            text.len() as u64
        };
        exported.push(
            json!({"source": source(s), "path": path, "format": format.name(), "bytes": bytes}),
        );
    }
    let report =
        json!({"version": 1, "dry_run": dry_run, "exported": exported, "warning": WARNING});
    if args.flag("json") {
        out::println(&py::dumps(&report));
    } else if output.is_some() {
        let count = report["exported"].as_array().map_or(0, Vec::len);
        let message = format!(
            "Exported {count} session{} as {}. {WARNING}",
            if count == 1 { "" } else { "s" },
            format.name()
        );
        if dry_run {
            out::preview(&message);
        } else {
            out::success(&message);
        }
    }
    0
}

fn sessions_hours(text: &str) -> Option<f64> {
    let hours = crate::sessions::parse_hours(text)?;
    (hours >= 0.0).then_some(hours)
}
