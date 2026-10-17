//! hatcheck in Rust: Claude Code and Codex accounts on one computer, each kept apart. This
//! crate is replacing the Python CLI in hatcheck/ one subcommand at a time, with the same
//! output (docs/rust-port.md); the desktop app is to link it in place of its bundled engine.

pub mod args;
pub mod cli;
pub mod facts;
pub mod ospath;
pub mod out;
pub mod paths;
pub mod py;
pub mod sessions;
pub mod times;
pub mod usage;
pub mod where_cmd;
pub mod widths;
