//! `claude://` links for the right account.
//!
//! Claude Desktop signs in with Google through the system browser (its log says
//! `[Auth] Using system browser for: /login/app-google-auth`) and the browser hands the result back
//! with a `claude://` link. Windows opens that link with Claude's own protocol handler, which runs
//! with the real LOCALAPPDATA, so the link lands in the Main account: a same-user slot that started
//! the sign-in never gets it.
//!
//! So Claude Switcher registers itself as the `claude://` handler (HKCU, re-asserted while it runs,
//! since Claude writes its own handler back when it starts). `claude-switcher.exe --open-url <url>`
//! picks the account whose Claude most recently sent a sign-in to the browser and starts Claude for
//! that account with the link; Claude's single-instance handler passes it to the running window.
//! With no recent sign-in the link goes to Main, as before.

use chrono::NaiveDateTime;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// How long after a sign-in was sent to the browser its link still belongs to that account.
pub const SIGN_IN_WINDOW: Duration = Duration::from_secs(15 * 60);
const AUTH_TAG: &str = "[Auth] Using system browser for:";
const PROTOCOL_KEY: &str = r"Software\Classes\claude";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Main,
    /// A same-user slot, by its LOCALAPPDATA root (`…\slots\<id>\Local`).
    Slot(PathBuf),
}

/// Only `claude://` links are forwarded; anything else is dropped.
pub fn is_claude_link(url: &str) -> bool {
    url.len() > "claude://".len()
        && url
            .get(..9)
            .is_some_and(|p| p.eq_ignore_ascii_case("claude://"))
        && !url.chars().any(|c| c.is_control() || c == '"')
}

/// The time of the last "sent the sign-in to the browser" line in a Claude log.
pub fn last_sign_in(log: &str) -> Option<NaiveDateTime> {
    log.lines()
        .rev()
        .filter(|l| l.contains(AUTH_TAG))
        .find_map(|l| NaiveDateTime::parse_from_str(l.get(..19)?, "%Y-%m-%d %H:%M:%S").ok())
}

/// The account with the most recent sign-in inside the window; Main when there is none.
pub fn pick(candidates: &[(Target, Option<NaiveDateTime>)], now: NaiveDateTime) -> Target {
    let window = chrono::Duration::from_std(SIGN_IN_WINDOW).expect("window fits");
    candidates
        .iter()
        .filter_map(|(t, at)| at.map(|at| (t, at)))
        // a clock a little ahead (log written just now) still counts; older than the window does not
        .filter(|(_, at)| *at <= now + chrono::Duration::seconds(60) && now - *at <= window)
        .max_by_key(|(_, at)| *at)
        .map(|(t, _)| t.clone())
        .unwrap_or(Target::Main)
}

/// The command Windows runs for `claude://` links.
pub fn handler_command(exe: &Path) -> String {
    format!("\"{}\" --open-url \"%1\"", exe.display())
}

fn read_tail(path: &Path) -> Option<String> {
    use std::io::{Read, Seek, SeekFrom};
    const TAIL: u64 = 256 * 1024;
    let mut f = std::fs::File::open(path).ok()?;
    let len = f.metadata().ok()?.len();
    f.seek(SeekFrom::Start(len.saturating_sub(TAIL))).ok()?;
    let mut bytes = Vec::new();
    f.read_to_end(&mut bytes).ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

fn log_of(local_root: &Path) -> PathBuf {
    local_root.join(r"Claude\logs\main.log")
}

/// Main plus every same-user slot, each with the time of its last sign-in.
fn candidates(real_local: &Path) -> Vec<(Target, Option<NaiveDateTime>)> {
    let sign_in = |root: &Path| read_tail(&log_of(root)).and_then(|s| last_sign_in(&s));
    let mut out = vec![(Target::Main, sign_in(real_local))];
    if let Ok(dir) = std::fs::read_dir(real_local.join(r"ClaudeSwitcher\slots")) {
        for e in dir.flatten() {
            let root = e.path().join("Local");
            if root.is_dir() {
                let at = sign_in(&root);
                out.push((Target::Slot(root), at));
            }
        }
    }
    out
}

/// `claude-switcher.exe --open-url <url>`: forwards the link to the right account. Returns the exit code.
pub fn main_open_url(args: &[String]) -> i32 {
    let Some(real_local) = std::env::var_os("LOCALAPPDATA").map(PathBuf::from) else {
        return 1;
    };
    crate::applog::init(&real_local.join(r"ClaudeSwitcher\logs"));
    let url = args
        .iter()
        .skip_while(|a| *a != "--open-url")
        .nth(1)
        .cloned()
        .unwrap_or_default();
    if !is_claude_link(&url) {
        crate::applog::error("open-url: not a claude:// link", &[]);
        return 2;
    }
    let target = pick(&candidates(&real_local), chrono::Local::now().naive_local());
    let alias = real_local.join(r"Microsoft\WindowsApps\claude-desktop.exe");
    let mut cmd = std::process::Command::new(&alias);
    cmd.arg("--").arg(&url);
    let who = match &target {
        Target::Main => "main".to_string(),
        Target::Slot(root) => {
            cmd.env("LOCALAPPDATA", root);
            root.parent()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default()
        }
    };
    // The link itself can carry a sign-in code: log only where it went.
    match cmd.spawn() {
        Ok(_) => {
            crate::applog::info("open-url forwarded", &[("to", &who)]);
            0
        }
        Err(e) => {
            crate::applog::error(
                "open-url: Claude did not start",
                &[("to", &who), ("detail", &e.to_string())],
            );
            1
        }
    }
}

/// The ProgID through which Windows can hand `claude:` links to Claude Switcher.
pub const PROG_ID: &str = "ClaudeSwitcher.claude";
const CAPABILITIES: &str = r"Software\CodingFolks\ClaudeSwitcher\Capabilities";

/// One registry value under HKCU.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegValue {
    pub key: String,
    pub name: &'static str,
    pub value: String,
}

/// Everything that lets Windows offer Claude Switcher for `claude:` links (Settings → Default apps →
/// link type CLAUDE) and run it with `--open-url`. Claude Desktop registers `claude:` in its MSIX
/// manifest, and Windows prefers that over a plain `Software\Classes\claude` key, so the user has to
/// pick Claude Switcher once; the classic key stays as the fallback where nothing else claims it.
pub fn registration(exe: &Path) -> Vec<RegValue> {
    let cmd = handler_command(exe);
    let icon = format!("\"{}\",0", exe.display());
    let prog = format!(r"Software\Classes\{PROG_ID}");
    let v = |key: &str, name: &'static str, value: &str| RegValue {
        key: key.to_string(),
        name,
        value: value.to_string(),
    };
    vec![
        v(&prog, "", "URL:claude (Claude Switcher)"),
        v(&prog, "URL Protocol", ""),
        v(&format!(r"{prog}\DefaultIcon"), "", &icon),
        v(&format!(r"{prog}\shell\open\command"), "", &cmd),
        v(CAPABILITIES, "ApplicationName", "Claude Switcher"),
        v(
            CAPABILITIES,
            "ApplicationDescription",
            "Opens Claude sign-in links in the account that started the sign-in.",
        ),
        v(
            &format!(r"{CAPABILITIES}\URLAssociations"),
            "claude",
            PROG_ID,
        ),
        v(
            r"Software\RegisteredApplications",
            "Claude Switcher",
            CAPABILITIES,
        ),
        v(PROTOCOL_KEY, "", "URL:claude"),
        v(PROTOCOL_KEY, "URL Protocol", ""),
        v(&format!(r"{PROTOCOL_KEY}\shell\open\command"), "", &cmd),
    ]
}

/// Writes whatever of `registration` is missing or different. Returns true when something changed.
pub fn ensure_handler(exe: &Path) -> std::io::Result<bool> {
    use winreg::enums::HKEY_CURRENT_USER;
    let hkcu = winreg::RegKey::predef(HKEY_CURRENT_USER);
    let mut changed = false;
    for r in registration(exe) {
        let current: Option<String> = hkcu
            .open_subkey(&r.key)
            .and_then(|k| k.get_value(r.name))
            .ok();
        if current.as_deref() != Some(r.value.as_str()) {
            let (key, _) = hkcu.create_subkey(&r.key)?;
            key.set_value(r.name, &r.value)?;
            changed = true;
        }
    }
    Ok(changed)
}

/// The command Windows actually runs for a `claude:` link (the user's choice or the packaged app's).
pub fn effective_command() -> Option<String> {
    use windows::core::{HSTRING, PWSTR};
    use windows::Win32::UI::Shell::{AssocQueryStringW, ASSOCF_IS_PROTOCOL, ASSOCSTR_COMMAND};
    let mut buf = vec![0u16; 2048];
    let mut len = buf.len() as u32;
    unsafe {
        AssocQueryStringW(
            ASSOCF_IS_PROTOCOL,
            ASSOCSTR_COMMAND,
            &HSTRING::from("claude"),
            None,
            Some(PWSTR(buf.as_mut_ptr())),
            &mut len,
        )
    }
    .ok()
    .ok()?;
    let end = buf.iter().position(|c| *c == 0).unwrap_or(buf.len());
    Some(String::from_utf16_lossy(&buf[..end]))
}

/// Whether a handler command runs this exe (Windows may change the case of the path).
pub fn handler_is_ours(command: &str, exe: &Path) -> bool {
    let exe = exe.to_string_lossy().to_lowercase();
    !command.is_empty() && command.to_lowercase().contains(&exe)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(s: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S").unwrap()
    }

    #[test]
    fn reads_the_last_sign_in_from_a_claude_log() {
        let log = "2026-11-19 21:20:37 [info] [Auth] Using system browser for: /login/app-google-auth\n\
                   2026-11-19 21:21:28 [info] second-instance: the main window is made; it is shown now\n\
                   2026-11-19 21:22:10 [info] [Auth] Using system browser for: /login/app-google-auth\n\
                   2026-11-19 21:22:11 [info] something else\n";
        assert_eq!(last_sign_in(log), Some(t("2026-11-19 21:22:10")));
        assert_eq!(last_sign_in("2026-11-19 21:22:11 [info] nothing\n"), None);
        assert_eq!(
            last_sign_in("garbage [Auth] Using system browser for: x\n"),
            None
        );
    }

    #[test]
    fn the_slot_that_started_the_sign_in_gets_the_link() {
        // What happened in the bug: the slot sent the sign-in, Main had none (or an old one).
        let slot = Target::Slot(PathBuf::from(r"C:\s\a\Local"));
        let c = vec![
            (Target::Main, Some(t("2026-09-24 12:41:04"))),
            (slot.clone(), Some(t("2026-11-19 21:20:37"))),
            (Target::Slot(PathBuf::from(r"C:\s\b\Local")), None),
        ];
        assert_eq!(pick(&c, t("2026-11-19 21:20:46")), slot);
    }

    #[test]
    fn the_most_recent_sign_in_wins() {
        let a = Target::Slot(PathBuf::from(r"C:\s\a\Local"));
        let c = vec![
            (Target::Main, Some(t("2026-11-19 21:25:00"))),
            (a, Some(t("2026-11-19 21:20:00"))),
        ];
        assert_eq!(pick(&c, t("2026-11-19 21:26:00")), Target::Main);
    }

    #[test]
    fn an_old_or_missing_sign_in_falls_back_to_main() {
        let c = vec![
            (Target::Main, None),
            (
                Target::Slot(PathBuf::from(r"C:\s\a\Local")),
                Some(t("2026-11-19 20:00:00")),
            ),
        ];
        assert_eq!(pick(&c, t("2026-11-19 21:00:00")), Target::Main);
        assert_eq!(pick(&[], t("2026-11-19 21:00:00")), Target::Main);
    }

    #[test]
    fn only_claude_links_are_forwarded() {
        assert!(is_claude_link("claude://claude.ai/login/callback?code=abc"));
        assert!(is_claude_link("CLAUDE://x"));
        assert!(!is_claude_link("claude://"));
        assert!(!is_claude_link("https://claude.ai/login"));
        assert!(!is_claude_link("claude://x\" --inject"));
        assert!(!is_claude_link("claude://x\n"));
        assert!(!is_claude_link(""));
    }

    #[test]
    fn handler_command_quotes_the_exe_and_the_link() {
        assert_eq!(
            handler_command(Path::new(
                r"C:\Program Files\Claude Switcher\claude-switcher.exe"
            )),
            r#""C:\Program Files\Claude Switcher\claude-switcher.exe" --open-url "%1""#
        );
    }

    #[test]
    fn switcher_registers_as_an_app_that_can_open_claude_links() {
        let exe = Path::new(r"C:\Program Files\Claude Switcher\claude-switcher.exe");
        let reg = registration(exe);
        let has = |key: &str, name: &str, value: &str| {
            reg.iter()
                .any(|r| r.key == key && r.name == name && r.value == value)
        };
        let cmd = handler_command(exe);
        assert!(
            has(
                r"Software\Classes\ClaudeSwitcher.claude\shell\open\command",
                "",
                &cmd
            ),
            "{reg:#?}"
        );
        assert!(has(
            r"Software\Classes\ClaudeSwitcher.claude",
            "URL Protocol",
            ""
        ));
        assert!(has(
            r"Software\CodingFolks\ClaudeSwitcher\Capabilities\URLAssociations",
            "claude",
            "ClaudeSwitcher.claude"
        ));
        assert!(has(
            r"Software\CodingFolks\ClaudeSwitcher\Capabilities",
            "ApplicationName",
            "Claude Switcher"
        ));
        assert!(has(
            r"Software\RegisteredApplications",
            "Claude Switcher",
            r"Software\CodingFolks\ClaudeSwitcher\Capabilities"
        ));
        // The classic handler stays as a fallback for Windows versions without the packaged one.
        assert!(has(r"Software\Classes\claude\shell\open\command", "", &cmd));
    }

    #[test]
    fn the_effective_handler_is_ours_only_when_it_runs_this_exe() {
        let exe = Path::new(r"C:\Program Files\Claude Switcher\claude-switcher.exe");
        assert!(handler_is_ours(
            r#""C:\Program Files\Claude Switcher\claude-switcher.exe" --open-url "%1""#,
            exe
        ));
        assert!(handler_is_ours(
            r#""c:\program files\claude switcher\CLAUDE-SWITCHER.EXE" --open-url "%1""#,
            exe
        ));
        assert!(!handler_is_ours(
            r#""C:\Program Files\WindowsApps\Claude_6.28899.2.0_x64__pzs8sxrjxfjjc\app\claude.exe" -- "%1""#,
            exe
        ));
        assert!(!handler_is_ours("", exe));
    }
}
