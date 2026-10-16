// Hatcheck's desktop app: no console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // hatcheck-desktop --selftest runs the bundled engine a few times, prints what it got and
    // exits, without opening a window: a check that the app and its engine work together.
    if std::env::args().any(|a| a == "--selftest") {
        std::process::exit(hatcheck_desktop_lib::selftest());
    }
    hatcheck_desktop_lib::run()
}
