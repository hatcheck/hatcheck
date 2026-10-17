//! claude-z: Claude Code on Z.ai, with ~/.claude or CLAUDE_CONFIG_DIR.
fn main() {
    let args = std::env::args_os()
        .skip(1)
        .map(|a| a.to_string_lossy().into_owned());
    std::process::exit(hatcheck::launch::main("claude-z", args.collect()));
}
