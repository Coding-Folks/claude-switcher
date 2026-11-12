//! Starting Claude Desktop as the main user or as a hidden user, first-run bootstrap, focus and close.
//!
//! "Focus" is a relaunch: Claude's own single-instance handler brings its existing window forward,
//! which also works for windows owned by another user.

use crate::claude_pkg::ClaudePkg;
use crate::error::{AppError, AppResult, ErrorCode};
use crate::vault::Zeroizing;
use base64::Engine;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Logons Windows refuses for the account itself (wrong password, restriction, expired, disabled,
/// locked out): all fixed by Perbaiki, which sets a new password and re-enables / unlocks the account.
const LOGON_REFUSALS: [u32; 5] = [1326, 1327, 1330, 1331, 1909];

pub fn is_logon_refusal(hresult: u32) -> bool {
    hresult >> 16 == 0x8007 && LOGON_REFUSALS.contains(&(hresult & 0xFFFF))
}
const BOOTSTRAP_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BootstrapReport {
    pub registered: bool,
    pub junction_ok: bool,
    pub alias_present: bool,
}

impl BootstrapReport {
    pub fn ok(&self) -> bool {
        self.registered && self.junction_ok && self.alias_present
    }
}

pub fn parse_bootstrap_log(s: &str) -> BootstrapReport {
    let mut r = BootstrapReport::default();
    for line in s.trim_start_matches('\u{feff}').lines() {
        match line.trim() {
            "register=ok" => r.registered = true,
            "junction=ok" => r.junction_ok = true,
            "alias=ok" => r.alias_present = true,
            _ => {}
        }
    }
    r
}

pub fn alias_for_profile(profile: &Path) -> PathBuf {
    profile.join(r"AppData\Local\Microsoft\WindowsApps\claude-desktop.exe")
}

fn ps_quote(p: &str) -> String {
    format!("'{}'", p.replace('\'', "''"))
}

/// PowerShell run once as the hidden user: register Claude, link `.claude`, check the alias.
pub fn bootstrap_script(family: &str, main_claude_dir: &Path, log: &Path) -> String {
    let junction = crate::share::junction_ps_snippet(main_claude_dir);
    format!(
        "$log={log}; function W($s) {{ Add-Content -LiteralPath $log -Value $s -Encoding utf8 }}\n\
         try {{ Add-AppxPackage -RegisterByFamilyName -MainPackage {family} -ErrorAction Stop; W 'register=ok' }} \
         catch {{ W ('register=fail:0x{{0:X8}}' -f $_.Exception.HResult) }}\n\
         W (& {{ {junction} }})\n\
         if (Test-Path (Join-Path $env:LOCALAPPDATA 'Microsoft\\WindowsApps\\claude-desktop.exe')) {{ W 'alias=ok' }} else {{ W 'alias=fail' }}\n",
        log = ps_quote(&log.to_string_lossy()),
        family = ps_quote(family),
    )
}

pub fn encode_ps(script: &str) -> String {
    let bytes: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// CreateProcessWithLogonW with the user's profile loaded. Returns the pid; waits when `wait` is set.
fn run_as(
    user: &str,
    pw: &Zeroizing<String>,
    app: &Path,
    args: &str,
    wait: Option<Duration>,
) -> AppResult<u32> {
    use windows::core::{PCWSTR, PWSTR};
    use windows::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0};
    use windows::Win32::System::Threading::{
        CreateProcessWithLogonW, WaitForSingleObject, CREATE_NO_WINDOW, LOGON_WITH_PROFILE,
        PROCESS_INFORMATION, STARTUPINFOW,
    };
    let u = wide(user);
    let d = wide(".");
    let p = Zeroizing::new(wide(pw));
    let a = wide(&app.to_string_lossy());
    let mut cmd = wide(&format!("\"{}\" {}", app.to_string_lossy(), args));
    let cwd = wide(&std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into()));
    let si = STARTUPINFOW {
        cb: std::mem::size_of::<STARTUPINFOW>() as u32,
        ..Default::default()
    };
    let mut pi = PROCESS_INFORMATION::default();
    let res = unsafe {
        CreateProcessWithLogonW(
            PCWSTR(u.as_ptr()),
            PCWSTR(d.as_ptr()),
            PCWSTR(p.as_ptr()),
            LOGON_WITH_PROFILE,
            PCWSTR(a.as_ptr()),
            Some(PWSTR(cmd.as_mut_ptr())),
            CREATE_NO_WINDOW,
            None,
            PCWSTR(cwd.as_ptr()),
            &si,
            &mut pi,
        )
    };
    if let Err(e) = res {
        return Err(if is_logon_refusal(e.code().0 as u32) {
            AppError::new(ErrorCode::LogonFailed, "err.logon_failed")
        } else {
            AppError::win32(e.code().0 as u32, "CreateProcessWithLogonW")
        });
    }
    let pid = pi.dwProcessId;
    let timed_out = wait.map(
        |w| unsafe { WaitForSingleObject(pi.hProcess, w.as_millis() as u32) } != WAIT_OBJECT_0,
    );
    unsafe {
        let _ = CloseHandle(pi.hThread);
        let _ = CloseHandle(pi.hProcess);
    }
    if timed_out == Some(true) {
        return Err(AppError::io(format!(
            "{} did not finish in time",
            app.display()
        )));
    }
    Ok(pid)
}

fn system32(exe: &str) -> PathBuf {
    PathBuf::from(std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into()))
        .join("System32")
        .join(exe)
}

fn powershell() -> PathBuf {
    system32(r"WindowsPowerShell\v1.0\powershell.exe")
}

pub fn bootstrap_args(script: &Path) -> String {
    format!(
        "-NoProfile -NonInteractive -ExecutionPolicy Bypass -File \"{}\"",
        script.display()
    )
}

/// First run for a hidden user: creates its profile, registers Claude, links `.claude`.
pub fn bootstrap(
    user: &str,
    pw: &Zeroizing<String>,
    pkg: &ClaudePkg,
    main_claude_dir: &Path,
) -> AppResult<BootstrapReport> {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let dir = base
        .join("ClaudeSwitcher")
        .join("bootstrap")
        .join(uuid::Uuid::new_v4().to_string());
    std::fs::create_dir_all(&dir)?;
    crate::share::grant_shared_access(&dir, user)?;
    let log = dir.join("boot.log");
    // The script goes in a file: CreateProcessWithLogonW refuses command lines over 1024 characters
    // (ERROR_INVALID_PARAMETER), and an -EncodedCommand of this script is several thousand.
    let script = dir.join("boot.ps1");
    let mut body = vec![0xEF, 0xBB, 0xBF]; // UTF-8 BOM so Windows PowerShell 5 reads non-ASCII paths right
    body.extend_from_slice(bootstrap_script(&pkg.family, main_claude_dir, &log).as_bytes());
    std::fs::write(&script, body)?;
    let run = run_as(
        user,
        pw,
        &powershell(),
        &bootstrap_args(&script),
        Some(BOOTSTRAP_TIMEOUT),
    );
    let report = std::fs::read_to_string(&log)
        .map(|s| parse_bootstrap_log(&s))
        .unwrap_or_default();
    let _ = std::fs::remove_dir_all(&dir);
    run?;
    Ok(report)
}

/// Main account: the current user's own alias. Also brings an existing window forward.
pub fn launch_main() -> AppResult<()> {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .ok_or_else(|| AppError::io("LOCALAPPDATA not set"))?;
    let alias = base.join(r"Microsoft\WindowsApps\claude-desktop.exe");
    if !alias.exists() {
        return Err(AppError::new(
            ErrorCode::ClaudeMissing,
            "err.claude_missing",
        ));
    }
    std::process::Command::new(alias).spawn()?;
    Ok(())
}

/// Hidden account: Claude as that user. Also brings an existing window forward.
pub fn launch_as(user: &str, pw: &Zeroizing<String>) -> AppResult<u32> {
    let profile = crate::winuser::profile_path(user)
        .ok_or_else(|| AppError::new(ErrorCode::JunctionBroken, "err.junction_broken"))?;
    run_as(user, pw, &alias_for_profile(&profile), "", None)
}

/// Ends a Claude instance. Claude hides to the tray on WM_CLOSE, so this terminates the process tree.
/// A hidden user's processes can only be ended by that user, so taskkill runs as them.
pub fn close(root_pid: u32, as_user: Option<(&str, &Zeroizing<String>)>) -> AppResult<()> {
    let args = format!("/PID {root_pid} /T /F");
    match as_user {
        None => {
            let out = std::process::Command::new(system32("taskkill.exe"))
                .args(args.split(' '))
                .output()?;
            // 128 = no such process: it already ended, which is what we wanted.
            match out.status.code() {
                Some(0) | Some(128) => Ok(()),
                _ => Err(AppError::io(format!(
                    "taskkill failed: {}",
                    String::from_utf8_lossy(&out.stderr).trim()
                ))),
            }
        }
        Some((user, pw)) => run_as(
            user,
            pw,
            &system32("taskkill.exe"),
            &args,
            Some(Duration::from_secs(15)),
        )
        .map(|_| ()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn bootstrap_log_parsing() {
        let r = parse_bootstrap_log("register=ok\njunction=ok\nalias=ok\n");
        assert!(r.registered && r.junction_ok && r.alias_present);
        let r = parse_bootstrap_log("register=fail:0x80073CF9\n");
        assert!(!r.registered && !r.junction_ok && !r.alias_present);
        let r = parse_bootstrap_log(
            "\u{feff}register=ok\r\njunction=fail:Access denied\r\nalias=ok\r\n",
        );
        assert!(r.registered && !r.junction_ok && r.alias_present);
        assert!(!r.ok());
    }
    #[test]
    fn alias_path_for_profile() {
        assert_eq!(
            alias_for_profile(Path::new(r"C:\Users\claude-kantor")),
            Path::new(
                r"C:\Users\claude-kantor\AppData\Local\Microsoft\WindowsApps\claude-desktop.exe"
            )
        );
    }
    #[test]
    fn bootstrap_script_registers_links_and_logs() {
        let s = bootstrap_script(
            "Claude_pzs8sxrjxfjjc",
            Path::new(r"C:\Users\alice\.claude"),
            Path::new(r"C:\x\boot.log"),
        );
        assert!(
            s.contains("Add-AppxPackage -RegisterByFamilyName -MainPackage 'Claude_pzs8sxrjxfjjc'"),
            "{s}"
        );
        assert!(s.contains(r"'C:\Users\alice\.claude'"), "{s}");
        assert!(s.contains(r"'C:\x\boot.log'"), "{s}");
        assert!(s.contains("alias="), "{s}");
    }
    #[test]
    fn encoded_command_is_utf16le_base64() {
        // "a" -> 61 00 -> "YQA="
        assert_eq!(encode_ps("a"), "YQA=");
    }

    #[test]
    fn every_refused_logon_leads_to_repair() {
        // wrong password, account restriction, password expired, account disabled, locked out
        for code in [1326u32, 1327, 1330, 1331, 1909] {
            assert!(is_logon_refusal(0x8007_0000 | code), "{code}");
        }
        assert!(!is_logon_refusal(0x8007_0057)); // invalid parameter is a bug, not a repair case
        assert!(!is_logon_refusal(1326)); // a bare Win32 code is not an HRESULT
    }

    #[test]
    fn run_as_unknown_user_is_a_logon_failure_not_a_bad_parameter() {
        // Windows validates the call before the logon: a parameter it rejects shows up as Win32 87
        // even for a real user, which is what broke "Tambah akun". An unknown user must reach the logon.
        let cmd = std::path::PathBuf::from(std::env::var("SystemRoot").unwrap())
            .join(r"System32\cmd.exe");
        let err = run_as(
            "claude-nope-zz9",
            &Zeroizing::new("x".into()),
            &cmd,
            "/c exit 0",
            Some(Duration::from_secs(5)),
        )
        .unwrap_err();
        assert_eq!(err.code, crate::error::ErrorCode::LogonFailed, "{err:?}");
    }

    #[test]
    fn bootstrap_command_line_fits_create_process_with_logon() {
        // CreateProcessWithLogonW rejects command lines over 1024 characters with ERROR_INVALID_PARAMETER.
        let log = Path::new(
            r"C:\Users\alice\AppData\Local\ClaudeSwitcher\bootstrap\0b7f0c8e-5a8e-4a43-9a1b-4a3a0f6d1c22\boot.log",
        );
        let args = bootstrap_args(Path::new(
            r"C:\Users\alice\AppData\Local\ClaudeSwitcher\bootstrap\0b7f0c8e-5a8e-4a43-9a1b-4a3a0f6d1c22\boot.ps1",
        ));
        let line = format!("\"{}\" {}", powershell().display(), args);
        assert!(line.len() <= 1024, "{} chars", line.len());
        let _ = log;
    }

    #[test]
    fn long_command_line_is_rejected_before_logon() {
        let cmd = std::path::PathBuf::from(std::env::var("SystemRoot").unwrap())
            .join(r"System32\cmd.exe");
        let long = format!("/c rem {}", "x".repeat(2000));
        let err = run_as(
            "claude-nope-zz9",
            &Zeroizing::new("x".into()),
            &cmd,
            &long,
            Some(Duration::from_secs(5)),
        )
        .unwrap_err();
        assert_eq!(
            err.code,
            crate::error::ErrorCode::Win32(0x8007_0057),
            "{err:?}"
        );
    }
}
