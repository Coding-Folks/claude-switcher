//! Hidden local Windows users: create, hide from the sign-in screen, change password, delete.
//! Everything except `exists` needs an elevated (admin) process.

use crate::error::{AppError, AppResult};
use crate::vault::Zeroizing;
use windows::core::{HSTRING, PCWSTR, PWSTR};
use windows::Win32::NetworkManagement::NetManagement::{
    NetApiBufferFree, NetUserAdd, NetUserDel, NetUserGetInfo, NetUserSetInfo, UF_DONT_EXPIRE_PASSWD,
    UF_PASSWD_CANT_CHANGE, UF_SCRIPT, USER_INFO_1, USER_INFO_1003, USER_INFO_1008, USER_PRIV_USER,
};

const USER_LIST: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Winlogon\SpecialAccounts\UserList";
const NERR_USER_NOT_FOUND: u32 = 2221;

fn check(code: u32, what: &str) -> AppResult<()> {
    if code == 0 { Ok(()) } else { Err(AppError::win32(code, what)) }
}

/// NUL-terminated UTF-16 copy of a secret, wiped on drop.
fn wide_secret(s: &Zeroizing<String>) -> Zeroizing<Vec<u16>> {
    Zeroizing::new(s.encode_utf16().chain(std::iter::once(0)).collect())
}

/// `USER_PRIV_USER` makes Windows add the account to the local Users group (SID S-1-5-32-545).
pub fn create(name: &str, password: &Zeroizing<String>) -> AppResult<()> {
    let n = HSTRING::from(name);
    let mut p = wide_secret(password);
    let info = USER_INFO_1 {
        usri1_name: PWSTR(n.as_ptr() as *mut u16),
        usri1_password: PWSTR(p.as_mut_ptr()),
        usri1_password_age: 0,
        usri1_priv: USER_PRIV_USER,
        usri1_home_dir: PWSTR::null(),
        usri1_comment: PWSTR::null(),
        usri1_flags: UF_SCRIPT | UF_DONT_EXPIRE_PASSWD | UF_PASSWD_CANT_CHANGE,
        usri1_script_path: PWSTR::null(),
    };
    check(unsafe { NetUserAdd(PCWSTR::null(), 1, &info as *const _ as *const u8, None) }, "NetUserAdd")
}

pub fn hide(name: &str) -> AppResult<()> {
    let hklm = winreg::RegKey::predef(winreg::enums::HKEY_LOCAL_MACHINE);
    let (key, _) = hklm.create_subkey(USER_LIST)?;
    key.set_value(name, &0u32)?;
    Ok(())
}

pub fn set_password(name: &str, password: &Zeroizing<String>) -> AppResult<()> {
    let n = HSTRING::from(name);
    let mut p = wide_secret(password);
    let info = USER_INFO_1003 { usri1003_password: PWSTR(p.as_mut_ptr()) };
    check(
        unsafe { NetUserSetInfo(PCWSTR::null(), PCWSTR(n.as_ptr()), 1003, &info as *const _ as *const u8, None) },
        "NetUserSetInfo",
    )?;
    // Same flags as at creation: this also clears "disabled" and "locked out", so Perbaiki fixes those too.
    let flags = USER_INFO_1008 { usri1008_flags: UF_SCRIPT | UF_DONT_EXPIRE_PASSWD | UF_PASSWD_CANT_CHANGE };
    check(
        unsafe { NetUserSetInfo(PCWSTR::null(), PCWSTR(n.as_ptr()), 1008, &flags as *const _ as *const u8, None) },
        "NetUserSetInfo(flags)",
    )
}

pub fn delete(name: &str) -> AppResult<()> {
    let n = HSTRING::from(name);
    let code = unsafe { NetUserDel(PCWSTR::null(), PCWSTR(n.as_ptr())) };
    if code != 0 && code != NERR_USER_NOT_FOUND {
        return Err(AppError::win32(code, "NetUserDel"));
    }
    let hklm = winreg::RegKey::predef(winreg::enums::HKEY_LOCAL_MACHINE);
    if let Ok(key) = hklm.open_subkey_with_flags(USER_LIST, winreg::enums::KEY_SET_VALUE) {
        let _ = key.delete_value(name);
    }
    Ok(())
}

const PROFILE_LIST: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\ProfileList";

/// The account's SID as `S-1-…`, or None if no such local account exists.
pub fn sid_of(name: &str) -> Option<String> {
    use windows::Win32::Foundation::{LocalFree, HLOCAL};
    use windows::Win32::Security::Authorization::ConvertSidToStringSidW;
    use windows::Win32::Security::{LookupAccountNameW, PSID, SID_NAME_USE};
    // Qualify with this computer's name: on a domain PC an unqualified name can resolve to a domain account.
    let qualified = match (name.contains('\\'), std::env::var("COMPUTERNAME")) {
        (false, Ok(pc)) => format!("{pc}\\{name}"),
        _ => name.to_string(),
    };
    let n = HSTRING::from(qualified.as_str());
    let (mut sid_len, mut dom_len) = (0u32, 0u32);
    let mut kind = SID_NAME_USE::default();
    unsafe {
        let _ = LookupAccountNameW(PCWSTR::null(), PCWSTR(n.as_ptr()), None, &mut sid_len, None, &mut dom_len, &mut kind);
        if sid_len == 0 {
            return None;
        }
        let mut sid = vec![0u8; sid_len as usize];
        let mut dom = vec![0u16; dom_len as usize];
        LookupAccountNameW(
            PCWSTR::null(), PCWSTR(n.as_ptr()), Some(PSID(sid.as_mut_ptr() as *mut _)), &mut sid_len,
            Some(PWSTR(dom.as_mut_ptr())), &mut dom_len, &mut kind,
        ).ok()?;
        let mut s = PWSTR::null();
        ConvertSidToStringSidW(PSID(sid.as_mut_ptr() as *mut _), &mut s).ok()?;
        let out = s.to_string().ok();
        let _ = LocalFree(Some(HLOCAL(s.0 as *mut _)));
        out
    }
}

/// The account's profile folder from the registry (exists once it has logged on).
pub fn profile_path(name: &str) -> Option<std::path::PathBuf> {
    let sid = sid_of(name)?;
    let hklm = winreg::RegKey::predef(winreg::enums::HKEY_LOCAL_MACHINE);
    let key = hklm.open_subkey(format!(r"{PROFILE_LIST}\{sid}")).ok()?;
    let raw: String = key.get_value("ProfileImagePath").ok()?;
    Some(std::path::PathBuf::from(expand_env(&raw)))
}

fn expand_env(s: &str) -> String {
    let mut out = s.to_string();
    for var in ["SystemDrive", "SystemRoot", "USERPROFILE"] {
        if let Ok(v) = std::env::var(var) {
            out = out.replace(&format!("%{var}%"), &v);
        }
    }
    out
}

/// Every (SID, profile folder) pair in the registry's ProfileList.
pub fn profile_list() -> Vec<(String, std::path::PathBuf)> {
    let hklm = winreg::RegKey::predef(winreg::enums::HKEY_LOCAL_MACHINE);
    let Ok(list) = hklm.open_subkey(PROFILE_LIST) else { return vec![] };
    list.enum_keys()
        .flatten()
        .filter_map(|sid| {
            let raw: String = list.open_subkey(&sid).ok()?.get_value("ProfileImagePath").ok()?;
            Some((sid, std::path::PathBuf::from(expand_env(&raw))))
        })
        .collect()
}

/// Profiles whose folder is `<name>` or Windows' collision form `<name>.<something>`.
pub fn profiles_named(name: &str, entries: &[(String, std::path::PathBuf)]) -> Vec<(String, std::path::PathBuf)> {
    let n = name.to_lowercase();
    entries
        .iter()
        .filter(|(_, p)| {
            let f = p.file_name().map(|f| f.to_string_lossy().to_lowercase()).unwrap_or_default();
            f == n || f.strip_prefix(&n).map(|rest| rest.starts_with('.')).unwrap_or(false)
        })
        .cloned()
        .collect()
}

/// Profile files can stay locked for a moment after the user's processes end (hive unload).
fn remove_profile_with_retry(p: &std::path::Path) -> AppResult<()> {
    let mut last = Ok(());
    for _ in 0..10 {
        last = crate::share::remove_profile_safely(p);
        if last.is_ok() {
            return last;
        }
        std::thread::sleep(std::time::Duration::from_millis(1500));
    }
    last
}

/// Elevated: deletes the account's profile folder (never following junctions), its ProfileList entry,
/// then the account. The profile goes first so a failure leaves the account in place to retry from;
/// a retry after the account is gone still finds the profile by its folder name.
pub fn delete_with_profile(name: &str) -> AppResult<()> {
    let targets: Vec<(String, std::path::PathBuf)> = match (sid_of(name), profile_path(name)) {
        (Some(sid), Some(p)) => vec![(sid, p)],
        (Some(_), None) => vec![], // never logged on: no profile yet
        (None, _) => profiles_named(name, &profile_list()),
    };
    let hklm = winreg::RegKey::predef(winreg::enums::HKEY_LOCAL_MACHINE);
    for (sid, path) in &targets {
        remove_profile_with_retry(path)?;
        let _ = hklm.delete_subkey_all(format!(r"{PROFILE_LIST}\{sid}"));
    }
    delete(name)
}

/// Works without admin.
pub fn exists(name: &str) -> bool {
    let n = HSTRING::from(name);
    let mut buf: *mut u8 = std::ptr::null_mut();
    let code = unsafe { NetUserGetInfo(PCWSTR::null(), PCWSTR(n.as_ptr()), 0, &mut buf) };
    if !buf.is_null() {
        unsafe { NetApiBufferFree(Some(buf as *const _)) };
    }
    code == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exists_finds_builtin_and_rejects_unknown() {
        assert!(exists("Administrator"));
        assert!(!exists("claude-nope-zz9"));
    }

    #[test]
    fn sid_and_profile_of_current_user() {
        let me = std::env::var("USERNAME").unwrap();
        let sid = sid_of(&me).unwrap();
        assert!(sid.starts_with("S-1-5-21-"), "{sid}");
        let profile = profile_path(&me).unwrap();
        assert_eq!(profile.to_string_lossy().to_lowercase(), std::env::var("USERPROFILE").unwrap().to_lowercase());
    }

    #[test]
    fn unknown_user_has_no_sid_or_profile() {
        assert!(sid_of("claude-nope-zz9").is_none());
        assert!(profile_path("claude-nope-zz9").is_none());
    }

    #[test]
    fn finds_profiles_left_by_a_deleted_user() {
        use std::path::PathBuf;
        let entries = vec![
            ("S-1-5-21-1-1001".to_string(), PathBuf::from(r"C:\Users\Irsal")),
            ("S-1-5-21-1-1002".to_string(), PathBuf::from(r"C:\Users\claude-kantor")),
            ("S-1-5-21-1-1003".to_string(), PathBuf::from(r"C:\Users\CLAUDE-KANTOR.MSI")),
            ("S-1-5-21-1-1004".to_string(), PathBuf::from(r"C:\Users\claude-kantor-2")),
        ];
        let found = profiles_named("claude-kantor", &entries);
        let got: Vec<&str> = found.iter().map(|(s, _)| s.as_str()).collect();
        assert_eq!(got, vec!["S-1-5-21-1-1002", "S-1-5-21-1-1003"]);
    }

    #[test]
    fn profile_list_includes_current_user() {
        let me = std::env::var("USERPROFILE").unwrap().to_lowercase();
        assert!(profile_list().iter().any(|(_, p)| p.to_string_lossy().to_lowercase() == me));
    }
}
