//! ck: Kimi Code CLI with a ~/.kimi-code-<name> home (see launch.rs).
fn main() {
    let args = std::env::args_os()
        .skip(1)
        .map(|a| a.to_string_lossy().into_owned());
    std::process::exit(hatcheck::launch::main("ck", args.collect()));
}
