//! cl: Claude Code with a ~/.claude-<profile> folder, or on Z.ai or Kimi with -z or -k.
fn main() {
    let args = std::env::args_os()
        .skip(1)
        .map(|a| a.to_string_lossy().into_owned());
    std::process::exit(hatcheck::launch::main("cl", args.collect()));
}
