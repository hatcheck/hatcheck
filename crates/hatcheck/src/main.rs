fn main() {
    let mut args = std::env::args_os().map(|a| a.to_string_lossy().into_owned());
    let argv0 = args.next().unwrap_or_else(|| "hatcheck".into());
    std::process::exit(hatcheck::cli::main(&argv0, args.collect()));
}
