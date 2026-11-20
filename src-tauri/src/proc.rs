//! Starting console programs (PowerShell, icacls, taskkill) from this GUI app without a console
//! window flashing up: Windows gives a console child of a GUI process its own new window unless
//! it is created with CREATE_NO_WINDOW.

use std::ffi::OsStr;
use std::os::windows::process::CommandExt;
use std::process::Command;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// A command for a console program that runs without showing a console window.
pub fn console(program: impl AsRef<OsStr>) -> Command {
    let mut cmd = Command::new(program);
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hidden_console_program_still_runs_and_reports() {
        let system = std::env::var("SystemRoot").unwrap();
        let out = console(std::path::Path::new(&system).join(r"System32\cmd.exe"))
            .args(["/C", "echo hidden"])
            .output()
            .unwrap();
        assert!(out.status.success());
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "hidden");
    }

    /// Every program this app starts outside tests goes through `console` (or is a GUI app).
    #[test]
    fn no_console_program_is_started_with_a_visible_window() {
        // GUI programs: they have no console, so nothing flashes.
        const GUI: [&str; 4] = [
            "Command::new(alias)",      // Claude Desktop
            "Command::new(real_alias)", // Claude Desktop in a slot
            "Command::new(&alias)",     // Claude Desktop for a claude:// link
            "Command::new(\"explorer.exe\")",
        ];
        let sources = [
            ("claude_pkg.rs", include_str!("claude_pkg.rs")),
            ("deeplink.rs", include_str!("deeplink.rs")),
            ("elevated.rs", include_str!("elevated.rs")),
            ("launcher.rs", include_str!("launcher.rs")),
            ("share.rs", include_str!("share.rs")),
            ("slot.rs", include_str!("slot.rs")),
            ("update.rs", include_str!("update.rs")),
            ("monitor.rs", include_str!("monitor.rs")),
            ("flows.rs", include_str!("flows.rs")),
            ("winuser.rs", include_str!("winuser.rs")),
            ("vault.rs", include_str!("vault.rs")),
            ("history.rs", include_str!("history.rs")),
            ("taskbar/shell.rs", include_str!("taskbar/shell.rs")),
        ];
        let mut bad = Vec::new();
        for (name, src) in sources {
            let live = src.split("#[cfg(test)]").next().unwrap(); // tests may use plain commands
            for (i, line) in live.lines().enumerate() {
                if line.contains("Command::new(") && !GUI.iter().any(|g| line.contains(g)) {
                    bad.push(format!("{name}:{} {}", i + 1, line.trim()));
                }
            }
        }
        assert!(bad.is_empty(), "start these with proc::console: {bad:#?}");
    }
}
