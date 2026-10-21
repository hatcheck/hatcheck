//! hatcheck: Claude Code, Codex, Antigravity CLI, Kimi Code CLI and ZCode accounts on one
//! computer, each kept apart. The hatcheck and hat commands, the cl, cx, claude-z and claude-k
//! launchers, and hatcheck serve, the engine the desktop app runs. It was ported from a Python
//! version, with the same output, which the command tests in tests/ still hold it to
//! (docs/rust-port.md).
//!
//! The library is how the binaries share code, not an interface for other crates: its modules
//! change with the commands, and keep no compatibility from one version to the next.

pub mod agent;
pub mod agy;
pub mod args;
pub mod best;
pub mod bind;
pub mod cli;
pub mod conv;
pub mod desktop;
pub mod doctor;
pub mod facts;
pub mod find;
pub mod kimi;
pub mod launch;
pub mod move_cmd;
pub mod open_cmd;
pub mod ospath;
pub mod out;
pub mod paths;
pub mod provider;
pub mod py;
pub mod relocate;
pub mod serve;
pub mod sessions;
pub mod show;
pub mod times;
pub mod usage;
pub mod where_cmd;
pub mod widths;
pub mod zcode;
