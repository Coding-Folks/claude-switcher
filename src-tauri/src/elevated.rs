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
    CreateUser {
        name: String,
    },
    SetPassword {
        name: String,
    },
    DeleteUser {
        name: String,
    },
    /// Copies the main user's Code session list into the hidden user's Claude data (history.rs).
    ImportSessions {
        name: String,
    },
}

impl ElevatedOp {
    fn name(&self) -> &str {
        match self {
            Self::CreateUser { name }
            | Self::SetPassword { name }
            | Self::DeleteUser { name }
            | Self::ImportSessions { name } => name,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct ElevatedResult {
    ok: bool,
    win32: Option<u32>,
    detail: Option<String>,
    #[serde(default)]
    message_id: Option<String>,
}

/// UI messages that may come back from the admin process (AppError keeps &'static str ids).
const PASSED_MESSAGES: &[(&str, ErrorCode)] = &[("err.import_no_signin", ErrorCode::NotFound)];

fn from_result(r: ElevatedResult) -> AppResult<()> {
    if r.ok {
        return Ok(());
    }
    if let Some((id, code)) = r
        .message_id
        .as_deref()
        .and_then(|m| PASSED_MESSAGES.iter().find(|(id, _)| *id == m))
    {
        return Err(AppError::new(*code, id));
    }
    Err(match r.win32 {
        Some(c) => AppError::win32(c, r.detail.as_deref().unwrap_or("elevated")),
        None => AppError::io(r.detail.unwrap_or_default()),
    })
}

/// Elevated: copy the session list of every Claude account the hidden user signed in to.
fn import_sessions(name: &str) -> AppResult<()> {
    let no_signin = || AppError::new(ErrorCode::NotFound, "err.import_no_signin");
    let profile = crate::winuser::profile_path(name).ok_or_else(no_signin)?;
    let dests: Vec<PathBuf> = crate::history::sessions_dirs(&profile.join(r"AppData\Local"))
        .into_iter()
        .filter(|d| {
            std::fs::read_dir(d)
                .map(|mut r| r.next().is_some())
                .unwrap_or(false)
        })
        .collect();
    if dests.is_empty() {
        return Err(no_signin());
    }
    let main_local = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .ok_or_else(|| AppError::io("LOCALAPPDATA not set"))?;
    let sources = crate::history::sessions_dirs(&main_local);
    for dest in &dests {
        crate::history::copy_plan(&crate::history::import_plan(&sources, dest))?;
        // Copied by the admin token: hand the files to the hidden user.
        let system = std::env::var_os("SystemRoot")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
        let _ = std::process::Command::new(system.join(r"System32\icacls.exe"))
            .arg(dest)
            .args(["/setowner", name, "/T", "/C", "/Q"])
            .output();
    }
    Ok(())
}

fn invalid(why: &str) -> AppError {
    AppError::new(ErrorCode::Invalid, "err.generic").with_detail(why.to_string())
}

pub fn parse_args(args: &[String]) -> AppResult<(ElevatedOp, Option<PathBuf>, PathBuf)> {
    let after = |flag: &str| {
        args.iter()
            .position(|a| a == flag)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let json = after("--elevated").ok_or_else(|| invalid("missing --elevated"))?;
    let op: ElevatedOp = serde_json::from_str(&json).map_err(|e| invalid(&e.to_string()))?;
    if !is_valid_windows_user(op.name()) {
        return Err(invalid("invalid user name"));
    }
    let result = after("--result").ok_or_else(|| invalid("missing --result"))?;
    let result = exchange_path(&result, ".result.json")?;
    let secret = after("--secret")
        .map(|s| exchange_path(&s, ".secret"))
        .transpose()?;
    Ok((op, secret, result))
}

/// The admin process only touches files the normal app created for it: `<uuid><suffix>` directly in
/// `%LOCALAPPDATA%\ClaudeSwitcher\tmp`, which must not be a junction or symlink. Anything else is refused,
/// so a caller cannot make the elevated copy overwrite, read or delete other files.
fn exchange_path(raw: &str, suffix: &str) -> AppResult<PathBuf> {
    use std::os::windows::fs::MetadataExt;
    use std::path::Component;
    let p = PathBuf::from(raw);
    if !p.is_absolute()
        || p.components()
            .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
    {
        return Err(invalid("exchange path must be absolute and plain"));
    }
    let name = p
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| invalid("bad exchange file name"))?;
    let stem = name
        .strip_suffix(suffix)
        .ok_or_else(|| invalid("bad exchange file kind"))?;
    if uuid::Uuid::parse_str(stem).is_err() {
        return Err(invalid("bad exchange file name"));
    }
    let dir = temp_dir()?;
    let same_dir = p.parent().map(|d| d.to_string_lossy().to_lowercase())
        == Some(dir.to_string_lossy().to_lowercase());
    if !same_dir {
        return Err(invalid("exchange file outside the exchange folder"));
    }
    for d in [dir.as_path(), dir.parent().unwrap_or(&dir)] {
        let md = std::fs::symlink_metadata(d)?;
        if md.file_attributes() & 0x400 != 0 {
            return Err(invalid("exchange folder is a reparse point"));
        }
    }
    Ok(p)
}

fn read_secret(path: Option<&Path>) -> AppResult<Zeroizing<String>> {
    use std::os::windows::fs::MetadataExt;
    let path = path.ok_or_else(|| invalid("missing --secret"))?;
    let md = std::fs::symlink_metadata(path)?;
    if !md.is_file() || md.file_attributes() & 0x400 != 0 {
        return Err(invalid("secret must be a plain file"));
    }
    let bytes = Zeroizing::new(std::fs::read(path)?);
    let _ = std::fs::remove_file(path);
    std::str::from_utf8(&bytes)
        .map(|s| Zeroizing::new(s.to_string()))
        .map_err(|_| invalid("secret is not UTF-8"))
}

/// Runs inside the elevated process.
pub fn run_elevated(op: &ElevatedOp, secret: Option<&Path>) -> AppResult<()> {
    match op {
        ElevatedOp::CreateUser { name } => {
            let pw = read_secret(secret)?;
            crate::winuser::create(name, &pw)?;
            if let Err(e) = crate::winuser::hide(name) {
                let _ = crate::winuser::delete(name); // never leave a visible account with an unknown password
                return Err(e);
            }
            Ok(())
        }
        ElevatedOp::SetPassword { name } => {
            crate::winuser::set_password(name, &read_secret(secret)?)
        }
        ElevatedOp::DeleteUser { name } => crate::winuser::delete_with_profile(name),
        ElevatedOp::ImportSessions { name } => import_sessions(name),
    }
}

/// Entry point for `--elevated`; returns the process exit code.
pub fn main_elevated(args: &[String]) -> i32 {
    // Nothing is written unless the arguments, including the result path, passed validation.
    let Ok((op, secret, result_path)) = parse_args(args) else {
        return 2;
    };
    let outcome = {
        let r = run_elevated(&op, secret.as_deref());
        if let Some(s) = &secret {
            let _ = std::fs::remove_file(s);
        }
        r
    };
    let report = match &outcome {
        Ok(()) => ElevatedResult {
            ok: true,
            win32: None,
            detail: None,
            message_id: None,
        },
        Err(e) => ElevatedResult {
            ok: false,
            win32: if let ErrorCode::Win32(c) = e.code {
                Some(c)
            } else {
                None
            },
            detail: e.detail.clone(),
            message_id: Some(e.message_id.to_string()),
        },
    };
    // create_new: never follow or overwrite something already at that name.
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&result_path)
    {
        use std::io::Write;
        let _ = f.write_all(&serde_json::to_vec(&report).unwrap_or_default());
    }
    if outcome.is_ok() {
        0
    } else {
        1
    }
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
    let mut parts = vec![
        "--elevated".to_string(),
        quote(&json),
        "--result".into(),
        quote(&result.to_string_lossy()),
    ];
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
        let v = (1..n as usize)
            .map(|i| (*argv.add(i)).to_string().unwrap_or_default())
            .collect();
        let _ = LocalFree(Some(HLOCAL(argv as *mut _)));
        v
    }
}

fn temp_dir() -> AppResult<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
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
            AppError::new(ErrorCode::Io, "err.generic").with_detail(format!(
                "ShellExecuteExW failed (HRESULT 0x{:08X})",
                e.code().0 as u32
            ))
        });
    }
    unsafe {
        WaitForSingleObject(info.hProcess, INFINITE);
        let _ = CloseHandle(info.hProcess);
    }
    let report: Option<ElevatedResult> = std::fs::read(&result)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok());
    cleanup();
    match report {
        Some(r) => from_result(r),
        None => Err(AppError::io("elevated process returned no result")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exchange(name: &str) -> String {
        temp_dir()
            .unwrap()
            .join(name)
            .to_string_lossy()
            .into_owned()
    }
    fn result_path() -> String {
        exchange(&format!("{}.result.json", uuid::Uuid::new_v4()))
    }
    fn args(json: &str) -> Vec<String> {
        vec![
            "claude-switcher.exe".into(),
            "--elevated".into(),
            json.into(),
            "--result".into(),
            result_path(),
        ]
    }

    #[test]
    fn rejects_bad_name() {
        assert!(parse_args(&args(r#"{"op":"delete_user","name":"Administrator"}"#)).is_err());
        assert!(parse_args(&args(r#"{"op":"create_user","name":"claude-../x"}"#)).is_err());
    }
    #[test]
    fn accepts_import_sessions_for_valid_names_only() {
        let (op, _, _) =
            parse_args(&args(r#"{"op":"import_sessions","name":"claude-kantor"}"#)).unwrap();
        assert_eq!(
            op,
            ElevatedOp::ImportSessions {
                name: "claude-kantor".into()
            }
        );
        assert!(parse_args(&args(r#"{"op":"import_sessions","name":"alice"}"#)).is_err());
    }
    #[test]
    fn known_message_ids_survive_the_elevated_boundary() {
        let e = from_result(ElevatedResult {
            ok: false,
            win32: None,
            detail: None,
            message_id: Some("err.import_no_signin".into()),
        })
        .unwrap_err();
        assert_eq!(
            (e.code, e.message_id),
            (ErrorCode::NotFound, "err.import_no_signin")
        );
        assert!(from_result(ElevatedResult {
            ok: true,
            win32: None,
            detail: None,
            message_id: None
        })
        .is_ok());
    }
    #[test]
    fn rejects_unknown_op() {
        assert!(parse_args(&args(r#"{"op":"run","cmd":"x"}"#)).is_err());
    }
    #[test]
    fn rejects_missing_result_file() {
        let a: Vec<String> = vec![
            "x".into(),
            "--elevated".into(),
            r#"{"op":"delete_user","name":"claude-kantor"}"#.into(),
        ];
        assert!(parse_args(&a).is_err());
    }
    #[test]
    fn accepts_create() {
        let a = args(r#"{"op":"create_user","name":"claude-kantor"}"#);
        let (op, secret, result) = parse_args(&a).unwrap();
        assert_eq!(
            op,
            ElevatedOp::CreateUser {
                name: "claude-kantor".into()
            }
        );
        assert_eq!(secret, None);
        assert_eq!(result, std::path::PathBuf::from(&a[4]));
    }
    #[test]
    fn accepts_secret_file_in_exchange_dir() {
        let mut a = args(r#"{"op":"set_password","name":"claude-kantor"}"#);
        let s = exchange(&format!("{}.secret", uuid::Uuid::new_v4()));
        a.extend(["--secret".into(), s.clone()]);
        assert_eq!(parse_args(&a).unwrap().1, Some(std::path::PathBuf::from(s)));
    }
    #[test]
    fn rejects_result_outside_exchange_dir() {
        let mut a = args(r#"{"op":"delete_user","name":"claude-kantor"}"#);
        a[4] = r"C:\Windows\System32\drivers\etc\hosts".into();
        assert!(parse_args(&a).is_err());
        a[4] = format!(r"C:\Windows\{}.result.json", uuid::Uuid::new_v4());
        assert!(parse_args(&a).is_err());
    }
    #[test]
    fn rejects_traversal_and_wrong_names_in_exchange_dir() {
        let mut a = args(r#"{"op":"delete_user","name":"claude-kantor"}"#);
        a[4] = exchange(r"..\..\evil.result.json");
        assert!(parse_args(&a).is_err());
        a[4] = exchange("not-a-uuid.result.json");
        assert!(parse_args(&a).is_err());
    }
    #[test]
    fn rejects_secret_outside_exchange_dir() {
        let mut a = args(r#"{"op":"set_password","name":"claude-kantor"}"#);
        a.extend(["--secret".into(), r"C:\Users\Public\x.secret".into()]);
        assert!(parse_args(&a).is_err());
        let n = a.len();
        a[n - 1] = exchange(&format!("{}.result.json", uuid::Uuid::new_v4())); // right dir, wrong kind
        assert!(parse_args(&a).is_err());
    }
    #[test]
    fn create_and_set_password_require_secret_at_run() {
        let op = ElevatedOp::SetPassword {
            name: "claude-kantor".into(),
        };
        assert!(run_elevated(&op, None).is_err());
    }
    #[test]
    fn command_line_roundtrips() {
        let op = ElevatedOp::DeleteUser {
            name: "claude-kantor".into(),
        };
        let r = result_path();
        let line = command_line(&op, None, std::path::Path::new(&r));
        let parsed: Vec<String> = std::iter::once("x".to_string())
            .chain(split_args(&line))
            .collect();
        assert_eq!(parse_args(&parsed).unwrap().0, op);
    }
}
