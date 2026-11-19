#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--elevated") {
        // Started through UAC by elevated::request: do the one admin operation and exit, no UI.
        std::process::exit(claude_switcher_lib::elevated::main_elevated(&args));
    }
    if args.iter().any(|a| a == "--open-url") {
        // Windows runs this for claude:// links: pass the link to the account that asked for it, no UI.
        std::process::exit(claude_switcher_lib::deeplink::main_open_url(&args));
    }
    claude_switcher_lib::run()
}
