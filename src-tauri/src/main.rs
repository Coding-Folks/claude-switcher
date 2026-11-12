#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--elevated") {
        // Started through UAC by elevated::request: do the one admin operation and exit, no UI.
        std::process::exit(claude_switcher_lib::elevated::main_elevated(&args));
    }
    claude_switcher_lib::run()
}
