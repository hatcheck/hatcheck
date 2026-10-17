//! hatcheck in Rust: Claude Code and Codex accounts on one computer, each kept apart. This
//! crate is replacing the Python CLI in hatcheck/ one subcommand at a time, with the same
//! output (docs/rust-port.md); the desktop app is to link it in place of its bundled engine.

pub mod args;
pub mod best;
pub mod bind;
pub mod cli;
pub mod conv;
pub mod desktop;
pub mod facts;
pub mod find;
pub mod move_cmd;
pub mod open_cmd;
pub mod ospath;
pub mod out;
pub mod paths;
pub mod provider;
pub mod py;
pub mod serve;
pub mod sessions;
pub mod show;
pub mod times;
pub mod usage;
pub mod where_cmd;
pub mod widths;
