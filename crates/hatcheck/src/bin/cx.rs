//! cx: Codex with a ~/.codex-<profile> home.
fn main() {
    let args = std::env::args_os()
        .skip(1)
        .map(|a| a.to_string_lossy().into_owned());
    std::process::exit(hatcheck::launch::main("cx", args.collect()));
}
