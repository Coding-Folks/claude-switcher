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

/// Opens a link (https:, ms-settings:) the way the Start menu would, through the shell. Not through
/// `explorer.exe <link>`: explorer reads a link with "?a=b" as a folder and opens Documents.
pub fn open_link(link: &str) -> std::io::Result<()> {
    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    // SAFETY: plain strings, no window handle; ShellExecuteW does not keep them.
    let r = unsafe {
        ShellExecuteW(
            None,
            &HSTRING::from("open"),
            &HSTRING::from(link),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    // Values above 32 mean success.
    if r.0 as isize > 32 {
        Ok(())
    } else {
        Err(std::io::Error::other(format!(
            "ShellExecute failed ({})",
            r.0 as isize
        )))
    }
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

    /// Links (https:, ms-settings:) open through the shell: given to explorer.exe as an argument, a
    /// link with "?a=b" opens the Documents folder instead (seen on Windows 11 26300).
    #[test]
    fn links_are_opened_by_the_shell_not_by_explorer_arguments() {
        let sources = [
            ("commands.rs", include_str!("commands.rs")),
            ("update.rs", include_str!("update.rs")),
            ("tray.rs", include_str!("tray.rs")),
        ];
        let bad: Vec<String> = sources
            .iter()
            .filter(|(_, src)| src.contains(concat!("Command::new(", "\"explorer.exe\")")))
            .map(|(name, _)| name.to_string())
            .collect();
        assert!(bad.is_empty(), "use proc::open_link: {bad:?}");
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
            ("commands.rs", include_str!("commands.rs")),
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
