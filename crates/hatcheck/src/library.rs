//! hatcheck library: favorites, aliases, tags, archive state and saved session filters, kept in
//! Hatcheck's own library.json. Agent transcripts and configuration are never changed.
//! docs/commands.md's `hatcheck library` section describes the command and storage.

use crate::args::{Count, Parser};
use crate::facts::Summary;
use crate::open_cmd::{pick, Pick};
use crate::ospath;
use crate::out;
use crate::paths;
use crate::py;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;

const VERSION: u64 = 1;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
struct SessionMetadata {
    alias: Option<String>,
    favorite: bool,
    tags: Vec<String>,
    archived: bool,
}

impl SessionMetadata {
    fn empty(&self) -> bool {
        self.alias.is_none() && !self.favorite && self.tags.is_empty() && !self.archived
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
struct SavedFilter {
    query: String,
    agent: String,
    profile: String,
    hours: u64,
    model: String,
    cost: String,
    tag: String,
    favorites: bool,
    archived: bool,
}

impl Default for SavedFilter {
    fn default() -> Self {
        Self {
            query: String::new(),
            agent: String::new(),
            profile: String::new(),
            hours: 168,
            model: String::new(),
            cost: String::new(),
            tag: String::new(),
            favorites: false,
            archived: false,
        }
    }
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct Library {
    sessions: BTreeMap<String, SessionMetadata>,
    filters: BTreeMap<String, SavedFilter>,
}

pub fn path() -> String {
    ospath::join(&paths::config_dir(), "library.json")
}

fn load() -> Result<Library, String> {
    let path = path();
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Library::default()),
        Err(e) => return Err(format!("can't read {}: {e}", paths::shell_path(&path))),
    };
    let data: Value = serde_json::from_str(&text)
        .map_err(|e| format!("can't read {}: {e}", paths::shell_path(&path)))?;
    if data.get("version").and_then(Value::as_u64) != Some(VERSION) {
        return Err(format!(
            "{} isn't a version-{} session library",
            paths::shell_path(&path),
            VERSION
        ));
    }
    serde_json::from_value(data)
        .map_err(|e| format!("can't read {}: {e}", paths::shell_path(&path)))
}

fn save(library: &Library) -> Result<(), String> {
    let data =
        json!({"version": VERSION, "sessions": library.sessions, "filters": library.filters});
    crate::bind::write_json(&path(), &data)
        .map_err(|e| format!("can't write {}: {e}", paths::shell_path(&path())))
}

fn key(s: &Summary) -> String {
    format!("{}:{}:{}", s.agent.key(), ospath::abspath(&s.profile), s.id)
}

fn metadata_json(s: &Summary, metadata: &SessionMetadata) -> Value {
    json!({"key": key(s), "agent": s.agent.key(), "profile": ospath::abspath(&s.profile),
        "id": s.id, "alias": metadata.alias, "favorite": metadata.favorite,
        "tags": metadata.tags, "archived": metadata.archived})
}

fn listed_json(library: &Library) -> Value {
    json!({"version": VERSION, "sessions": library.sessions, "filters": library.filters})
}

fn session(session: &str, source: Option<&str>) -> Result<Summary, String> {
    pick(&Pick {
        session: Some(session),
        source,
        all: true,
    })
}

fn changed(
    id: &str,
    source: Option<&str>,
    args: &crate::args::Args,
    parser: &Parser,
) -> Result<(Summary, SessionMetadata), i32> {
    let s = session(id, source).map_err(|message| out::fail(&message))?;
    let mut library = load().map_err(|message| out::fail(&message))?;
    let mut metadata = library.sessions.get(&key(&s)).cloned().unwrap_or_default();
    if args.flag("clear_alias") {
        metadata.alias = None;
    }
    if let Some(alias) = args.value("alias") {
        let alias = alias.trim();
        if alias.is_empty() {
            return Err(parser
                .error("--alias can't be empty; use --clear-alias")
                .exit_code());
        }
        metadata.alias = Some(alias.to_string());
    }
    if args.flag("favorite") {
        metadata.favorite = true;
    }
    if args.flag("no_favorite") {
        metadata.favorite = false;
    }
    if args.flag("archive") {
        metadata.archived = true;
    }
    if args.flag("unarchive") {
        metadata.archived = false;
    }
    for tag in args.values("untag") {
        metadata.tags.retain(|t| t != tag);
    }
    for tag in args.values("tag") {
        let tag = tag.trim();
        if tag.is_empty() {
            return Err(parser.error("--tag can't be empty").exit_code());
        }
        if !metadata.tags.iter().any(|t| t == tag) {
            metadata.tags.push(tag.to_string());
        }
    }
    metadata.tags.sort_by_key(|tag| tag.to_lowercase());
    if metadata.empty() {
        library.sessions.remove(&key(&s));
    } else {
        library.sessions.insert(key(&s), metadata.clone());
    }
    save(&library).map_err(|message| out::fail(&message))?;
    Ok((s, metadata))
}

fn filter_from(args: &crate::args::Args, parser: &Parser) -> Result<SavedFilter, i32> {
    let hours = match args.value("hours") {
        None => 168,
        Some(text) => match text.parse::<u64>() {
            Ok(hours) if hours > 0 => hours,
            _ => {
                return Err(parser
                    .error("argument --hours: expected a positive whole number of hours")
                    .exit_code());
            }
        },
    };
    let cost = args.value("cost").unwrap_or("");
    if !matches!(cost, "" | "under-1" | "1-10" | "10-plus") {
        return Err(parser
            .error("argument --cost: choose under-1, 1-10 or 10-plus")
            .exit_code());
    }
    let mut filter = SavedFilter {
        query: args.value("query").unwrap_or("").to_string(),
        agent: args.value("agent").unwrap_or("").to_string(),
        profile: args.value("profile").unwrap_or("").to_string(),
        hours,
        model: args.value("model").unwrap_or("").to_string(),
        cost: cost.to_string(),
        tag: args.value("tag").unwrap_or("").to_string(),
        favorites: args.flag("favorites"),
        archived: args.flag("archived"),
    };
    filter.query = filter.query.trim().to_string();
    Ok(filter)
}

pub fn main(argv: &[String], prog: &str) -> i32 {
    let parser = Parser::new(
        prog,
        "Keep favorites, aliases, tags, archive state and saved session filters in Hatcheck's own library.json. No agent transcript or setting is changed, and archiving never deletes a session.",
    )
    .flag(&["-j", "--json"], "json", "print the library or changed item as JSON")
    .subcommands(
        "action",
        &[
            ("set", "change a session's alias, favorite, tags or archive state"),
            ("clear", "remove all Hatcheck metadata for a session"),
            ("save-filter", "save a reusable desktop session filter"),
            ("remove-filter", "remove a saved filter"),
        ],
    );
    let args = match parser.parse(argv) {
        Ok(args) => args,
        Err(stop) => return stop.exit_code(),
    };
    let sub = |name: &str| format!("{prog} {name}");
    let session_parser = |name: &str| {
        Parser::new(&sub(name), "")
            .positional("session", Count::One, "the session ID, or the start of it")
            .option(
                &["--from"],
                "source",
                "PROFILE",
                "the profile or home that has the session",
            )
    };
    let set_parser = session_parser("set")
        .option(
            &["--alias"],
            "alias",
            "NAME",
            "a personal display name, kept only by Hatcheck",
        )
        .flag(
            &["--clear-alias"],
            "clear_alias",
            "remove the personal display name",
        )
        .flag(
            &["--favorite"],
            "favorite",
            "mark the session as a favorite",
        )
        .flag(
            &["--no-favorite"],
            "no_favorite",
            "remove the favorite mark",
        )
        .option(
            &["--tag"],
            "tag",
            "TAG",
            "add a tag; repeat for more than one",
        )
        .option(
            &["--untag"],
            "untag",
            "TAG",
            "remove a tag; repeat for more than one",
        )
        .flag(
            &["--archive"],
            "archive",
            "archive the session without deleting it",
        )
        .flag(
            &["--unarchive"],
            "unarchive",
            "return an archived session to the library",
        )
        .exclusive(&["alias", "clear_alias"])
        .exclusive(&["favorite", "no_favorite"])
        .exclusive(&["archive", "unarchive"]);
    let clear_parser = session_parser("clear");
    let filter_parser = Parser::new(&sub("save-filter"), "")
        .positional("name", Count::One, "the saved filter's name")
        .option(
            &["--query"],
            "query",
            "WORDS",
            "words in the title, alias, folder or tags",
        )
        .option(
            &["--agent"],
            "agent",
            "AGENT",
            "one agent, or empty for every agent",
        )
        .option(
            &["--profile"],
            "profile",
            "PROFILE",
            "one profile path, or empty for every profile",
        )
        .option(
            &["--hours"],
            "hours",
            "HOURS",
            "the time range in whole hours (default 168)",
        )
        .option(
            &["--model"],
            "model",
            "MODEL",
            "text the model name contains",
        )
        .option(
            &["--cost"],
            "cost",
            "RANGE",
            "under-1, 1-10 or 10-plus US dollars",
        )
        .option(&["--tag"], "tag", "TAG", "require this Hatcheck tag")
        .flag(&["--favorites"], "favorites", "show favorites only")
        .flag(&["--archived"], "archived", "include archived sessions");
    let remove_filter_parser = Parser::new(&sub("remove-filter"), "").positional(
        "name",
        Count::One,
        "the saved filter's name",
    );
    let action = args.value("action");
    let sub_args = match action {
        Some("set") => set_parser.parse_known(&args.rest),
        Some("clear") => clear_parser.parse_known(&args.rest),
        Some("save-filter") => filter_parser.parse_known(&args.rest),
        Some("remove-filter") => remove_filter_parser.parse_known(&args.rest),
        _ => Ok((Default::default(), Vec::new())),
    };
    let sub_args = match sub_args {
        Ok((_, extras)) if !extras.is_empty() => {
            return parser
                .error(&format!("unrecognized arguments: {}", extras.join(" ")))
                .exit_code();
        }
        Ok((args, _)) => args,
        Err(stop) => return stop.exit_code(),
    };
    match action {
        Some("set") => {
            let changed = changed(
                sub_args.value("session").unwrap_or(""),
                sub_args.value("source"),
                &sub_args,
                &set_parser,
            );
            let (s, metadata) = match changed {
                Ok(changed) => changed,
                Err(code) => return code,
            };
            if args.flag("json") {
                out::println(&py::dumps(&metadata_json(&s, &metadata)));
            } else {
                out::success(&format!(
                    "Updated Hatcheck's library entry for {}.",
                    &s.id[..8.min(s.id.len())]
                ));
            }
            0
        }
        Some("clear") => {
            let s = match session(
                sub_args.value("session").unwrap_or(""),
                sub_args.value("source"),
            ) {
                Ok(s) => s,
                Err(message) => return out::fail(&message),
            };
            let mut library = match load() {
                Ok(library) => library,
                Err(message) => return out::fail(&message),
            };
            library.sessions.remove(&key(&s));
            if let Err(message) = save(&library) {
                return out::fail(&message);
            }
            if args.flag("json") {
                out::println(&py::dumps(&json!({"cleared": key(&s)})));
            } else {
                out::success(
                    "Cleared the session's Hatcheck metadata. Its transcript is unchanged.",
                );
            }
            0
        }
        Some("save-filter") => {
            let filter = match filter_from(&sub_args, &filter_parser) {
                Ok(filter) => filter,
                Err(code) => return code,
            };
            let name = sub_args.value("name").unwrap_or("").trim();
            if name.is_empty() {
                return parser.error("a saved filter needs a name").exit_code();
            }
            let mut library = match load() {
                Ok(library) => library,
                Err(message) => return out::fail(&message),
            };
            library.filters.insert(name.to_string(), filter.clone());
            if let Err(message) = save(&library) {
                return out::fail(&message);
            }
            if args.flag("json") {
                out::println(&py::dumps(&json!({"name": name, "filter": filter})));
            } else {
                out::success(&format!("Saved filter \"{name}\"."));
            }
            0
        }
        Some("remove-filter") => {
            let name = sub_args.value("name").unwrap_or("");
            let mut library = match load() {
                Ok(library) => library,
                Err(message) => return out::fail(&message),
            };
            if library.filters.remove(name).is_none() {
                return out::fail(&format!("No saved filter named {name}"));
            }
            if let Err(message) = save(&library) {
                return out::fail(&message);
            }
            if args.flag("json") {
                out::println(&py::dumps(&json!({"removed": name})));
            } else {
                out::success(&format!("Removed filter \"{name}\"."));
            }
            0
        }
        _ => match load() {
            Ok(library) => {
                if args.flag("json") {
                    out::println(&py::dumps(&listed_json(&library)));
                } else {
                    out::human(&format!(
                        "{} session entries, {} saved filters, in {}.",
                        library.sessions.len(),
                        library.filters.len(),
                        paths::shell_path(&path())
                    ));
                }
                0
            }
            Err(message) => out::fail(&message),
        },
    }
}
