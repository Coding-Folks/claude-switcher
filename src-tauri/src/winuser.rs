//! Hidden local Windows users: create, hide from the sign-in screen, change password, delete.
//! Everything except `exists` needs an elevated (admin) process.

use crate::error::{AppError, AppResult};
use crate::vault::Zeroizing;
use windows::core::{HSTRING, PCWSTR, PWSTR};
use windows::Win32::NetworkManagement::NetManagement::{
    NetApiBufferFree, NetUserAdd, NetUserDel, NetUserGetInfo, NetUserSetInfo, UF_DONT_EXPIRE_PASSWD,
    UF_PASSWD_CANT_CHANGE, UF_SCRIPT, USER_INFO_1, USER_INFO_1003, USER_PRIV_USER,
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
}
