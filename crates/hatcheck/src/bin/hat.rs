//! hat, the short name for hatcheck: the same program, which names itself hat.
fn main() {
    let args = std::env::args_os()
        .skip(1)
        .map(|a| a.to_string_lossy().into_owned());
    std::process::exit(hatcheck::cli::main("hat", args.collect()));
}
