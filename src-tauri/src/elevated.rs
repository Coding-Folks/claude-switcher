//! `claude-switcher.exe --elevated <op-json> --result <file> [--secret <file>]`, started through UAC.
//! Accepts only a fixed list of operations on validated user names; never runs free-form commands.

use crate::error::{AppError, AppResult, ErrorCode};
use crate::naming::is_valid_windows_user;
use crate::vault::Zeroizing;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum ElevatedOp {
    CreateUser { name: String },
    SetPassword { name: String },
    DeleteUser { name: String },
}

impl ElevatedOp {
    fn name(&self) -> &str {
        match self {
            Self::CreateUser { name } | Self::SetPassword { name } | Self::DeleteUser { name } => name,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct ElevatedResult {
    ok: bool,
    win32: Option<u32>,
    detail: Option<String>,
}

fn invalid(why: &str) -> AppError {
    AppError::new(ErrorCode::Invalid, "err.generic").with_detail(why.to_string())
}

pub fn parse_args(args: &[String]) -> AppResult<(ElevatedOp, Option<PathBuf>, PathBuf)> {
    let after = |flag: &str| args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).cloned();
    let json = after("--elevated").ok_or_else(|| invalid("missing --elevated"))?;
    let op: ElevatedOp = serde_json::from_str(&json).map_err(|e| invalid(&e.to_string()))?;
    if !is_valid_windows_user(op.name()) {
        return Err(invalid("invalid user name"));
    }
    let result = after("--result").map(PathBuf::from).ok_or_else(|| invalid("missing --result"))?;
    let secret = after("--secret").map(PathBuf::from);
    Ok((op, secret, result))
}

fn read_secret(path: Option<&Path>) -> AppResult<Zeroizing<String>> {
    let path = path.ok_or_else(|| invalid("missing --secret"))?;
    let bytes = Zeroizing::new(std::fs::read(path)?);
    let _ = std::fs::remove_file(path);
    std::str::from_utf8(&bytes).map(|s| Zeroizing::new(s.to_string())).map_err(|_| invalid("secret is not UTF-8"))
}

/// Runs inside the elevated process.
pub fn run_elevated(op: &ElevatedOp, secret: Option<&Path>) -> AppResult<()> {
    match op {
        ElevatedOp::CreateUser { name } => {
            let pw = read_secret(secret)?;
            crate::winuser::create(name, &pw)?;
            crate::winuser::hide(name)
        }
        ElevatedOp::SetPassword { name } => crate::winuser::set_password(name, &read_secret(secret)?),
        ElevatedOp::DeleteUser { name } => crate::winuser::delete(name),
    }
}

/// Entry point for `--elevated`; returns the process exit code.
pub fn main_elevated(args: &[String]) -> i32 {
    let result_path = args.iter().position(|a| a == "--result").and_then(|i| args.get(i + 1)).map(PathBuf::from);
    let outcome = parse_args(args).and_then(|(op, secret, _)| {
        let r = run_elevated(&op, secret.as_deref());
        if let Some(s) = secret {
            let _ = std::fs::remove_file(s);
        }
        r
    });
    let report = match &outcome {
        Ok(()) => ElevatedResult { ok: true, win32: None, detail: None },
        Err(e) => ElevatedResult {
            ok: false,
            win32: if let ErrorCode::Win32(c) = e.code { Some(c) } else { None },
            detail: e.detail.clone(),
        },
    };
    if let Some(p) = result_path {
        let _ = std::fs::write(p, serde_json::to_vec(&report).unwrap_or_default());
    }
    if outcome.is_ok() { 0 } else { 1 }
}

/// Quotes one argument so CommandLineToArgvW returns it unchanged.
fn quote(arg: &str) -> String {
    if !arg.is_empty() && !arg.contains([' ', '\t', '"']) {
        return arg.to_string();
    }
    let mut out = String::from("\"");
    let mut backslashes = 0usize;
    for c in arg.chars() {
        match c {
            '\\' => backslashes += 1,
            '"' => {
                out.push_str(&"\\".repeat(backslashes * 2 + 1));
                out.push('"');
                backslashes = 0;
            }
            _ => {
                out.push_str(&"\\".repeat(backslashes));
                out.push(c);
                backslashes = 0;
            }
        }
    }
    out.push_str(&"\\".repeat(backslashes * 2));
    out.push('"');
    out
}

/// The parameter string for the elevated process (program name excluded).
pub fn command_line(op: &ElevatedOp, secret: Option<&Path>, result: &Path) -> String {
    let json = serde_json::to_string(op).expect("op serializes");
    let mut parts = vec!["--elevated".to_string(), quote(&json), "--result".into(), quote(&result.to_string_lossy())];
    if let Some(s) = secret {
        parts.push("--secret".into());
        parts.push(quote(&s.to_string_lossy()));
    }
    parts.join(" ")
}

/// Splits a parameter string the way Windows does (CommandLineToArgvW).
pub fn split_args(line: &str) -> Vec<String> {
    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::Foundation::{LocalFree, HLOCAL};
    use windows::Win32::UI::Shell::CommandLineToArgvW;
    let full = HSTRING::from(format!("x {line}"));
    let mut n = 0i32;
    unsafe {
        let argv = CommandLineToArgvW(PCWSTR(full.as_ptr()), &mut n);
        if argv.is_null() {
            return vec![];
        }
        let v = (1..n as usize).map(|i| (*argv.add(i)).to_string().unwrap_or_default()).collect();
        let _ = LocalFree(Some(HLOCAL(argv as *mut _)));
        v
    }
}

fn temp_dir() -> AppResult<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    let dir = base.join("ClaudeSwitcher").join("tmp");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Called by the normal (non-admin) app: runs `op` in an elevated copy of this exe and waits.
pub fn request(op: ElevatedOp, secret: Option<&Zeroizing<String>>) -> AppResult<()> {
    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::Foundation::{CloseHandle, ERROR_CANCELLED};
    use windows::Win32::System::Threading::{WaitForSingleObject, INFINITE};
    use windows::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};
    use windows::Win32::UI::WindowsAndMessaging::SW_HIDE;

    let dir = temp_dir()?;
    let id = uuid::Uuid::new_v4();
    let result = dir.join(format!("{id}.result.json"));
    let secret_path = match secret {
        Some(s) => {
            let p = dir.join(format!("{id}.secret"));
            std::fs::write(&p, s.as_bytes())?;
            Some(p)
        }
        None => None,
    };
    let cleanup = || {
        if let Some(p) = &secret_path {
            let _ = std::fs::remove_file(p);
        }
        let _ = std::fs::remove_file(&result);
    };

    let exe = HSTRING::from(std::env::current_exe()?.as_os_str());
    let params = HSTRING::from(command_line(&op, secret_path.as_deref(), &result));
    let verb = HSTRING::from("runas");
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS,
        lpVerb: PCWSTR(verb.as_ptr()),
        lpFile: PCWSTR(exe.as_ptr()),
        lpParameters: PCWSTR(params.as_ptr()),
        nShow: SW_HIDE.0,
        ..Default::default()
    };
    if let Err(e) = unsafe { ShellExecuteExW(&mut info) } {
        cleanup();
        return Err(if e.code() == ERROR_CANCELLED.to_hresult() {
            AppError::new(ErrorCode::UacCancelled, "err.uac_cancelled")
        } else {
            AppError::win32(e.code().0 as u32, "ShellExecuteExW")
        });
    }
    unsafe {
        WaitForSingleObject(info.hProcess, INFINITE);
        let _ = CloseHandle(info.hProcess);
    }
    let report: Option<ElevatedResult> = std::fs::read(&result).ok().and_then(|b| serde_json::from_slice(&b).ok());
    cleanup();
    match report {
        Some(r) if r.ok => Ok(()),
        Some(r) => Err(match r.win32 {
            Some(c) => AppError::win32(c, r.detail.as_deref().unwrap_or("elevated")),
            None => AppError::io(r.detail.unwrap_or_default()),
        }),
        None => Err(AppError::io("elevated process returned no result")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(json: &str) -> Vec<String> {
        vec!["claude-switcher.exe".into(), "--elevated".into(), json.into(), "--result".into(), r"C:\t\r.json".into()]
    }

    #[test]
    fn rejects_bad_name() {
        assert!(parse_args(&args(r#"{"op":"delete_user","name":"Administrator"}"#)).is_err());
        assert!(parse_args(&args(r#"{"op":"create_user","name":"claude-../x"}"#)).is_err());
    }
    #[test]
    fn rejects_unknown_op() {
        assert!(parse_args(&args(r#"{"op":"run","cmd":"x"}"#)).is_err());
    }
    #[test]
    fn rejects_missing_result_file() {
        let a: Vec<String> = vec!["x".into(), "--elevated".into(), r#"{"op":"delete_user","name":"claude-kantor"}"#.into()];
        assert!(parse_args(&a).is_err());
    }
    #[test]
    fn accepts_create() {
        let (op, secret, result) = parse_args(&args(r#"{"op":"create_user","name":"claude-kantor"}"#)).unwrap();
        assert_eq!(op, ElevatedOp::CreateUser { name: "claude-kantor".into() });
        assert_eq!(secret, None);
        assert_eq!(result, std::path::PathBuf::from(r"C:\t\r.json"));
    }
    #[test]
    fn accepts_secret_file() {
        let mut a = args(r#"{"op":"set_password","name":"claude-kantor"}"#);
        a.extend(["--secret".into(), r"C:\t\s.bin".into()]);
        assert_eq!(parse_args(&a).unwrap().1, Some(std::path::PathBuf::from(r"C:\t\s.bin")));
    }
    #[test]
    fn create_and_set_password_require_secret_at_run() {
        let op = ElevatedOp::SetPassword { name: "claude-kantor".into() };
        assert!(run_elevated(&op, None).is_err());
    }
    #[test]
    fn command_line_roundtrips() {
        let op = ElevatedOp::DeleteUser { name: "claude-kantor".into() };
        let line = command_line(&op, None, std::path::Path::new(r"C:\a b\r.json"));
        let parsed: Vec<String> = std::iter::once("x".to_string()).chain(split_args(&line)).collect();
        assert_eq!(parse_args(&parsed).unwrap().0, op);
    }
}
