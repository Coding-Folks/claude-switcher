//! Passwords of hidden users live only in Windows Credential Manager (DPAPI, per main user).

use crate::error::{AppError, AppResult, ErrorCode};
use rand::{rngs::OsRng, seq::SliceRandom, Rng};
pub use zeroize::Zeroizing;
use zeroize::Zeroize;

use windows::core::{HSTRING, PCWSTR, PWSTR};
use windows::Win32::Foundation::{ERROR_NOT_FOUND, FILETIME};
use windows::Win32::Security::Credentials::{
    CredDeleteW, CredFree, CredReadW, CredWriteW, CREDENTIALW, CRED_FLAGS, CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC,
};

const UPPER: &str = "ABCDEFGHJKLMNPQRSTUVWXYZ";
const LOWER: &str = "abcdefghijkmnopqrstuvwxyz";
const DIGITS: &str = "23456789";
pub(crate) const SYMBOLS: &str = "!#%+-=?@_~";

fn target(user: &str) -> HSTRING {
    HSTRING::from(format!("ClaudeSwitcher/{user}"))
}

fn not_found(e: &windows::core::Error) -> bool {
    e.code() == ERROR_NOT_FOUND.to_hresult()
}

pub fn put(user: &str, secret: &Zeroizing<String>) -> AppResult<()> {
    let mut blob: Zeroizing<Vec<u8>> = Zeroizing::new(secret.encode_utf16().flat_map(u16::to_le_bytes).collect());
    let name = target(user);
    let cred = CREDENTIALW {
        Flags: CRED_FLAGS(0),
        Type: CRED_TYPE_GENERIC,
        TargetName: PWSTR(name.as_ptr() as *mut u16),
        Comment: PWSTR::null(),
        LastWritten: FILETIME::default(),
        CredentialBlobSize: blob.len() as u32,
        CredentialBlob: blob.as_mut_ptr(),
        Persist: CRED_PERSIST_LOCAL_MACHINE,
        AttributeCount: 0,
        Attributes: std::ptr::null_mut(),
        TargetAlias: PWSTR::null(),
        UserName: PWSTR::null(),
    };
    unsafe { CredWriteW(&cred, 0) }.map_err(|e| AppError::win32(e.code().0 as u32, "CredWriteW"))
}

pub fn get(user: &str) -> AppResult<Zeroizing<String>> {
    let name = target(user);
    let mut out: *mut CREDENTIALW = std::ptr::null_mut();
    match unsafe { CredReadW(PCWSTR(name.as_ptr()), CRED_TYPE_GENERIC, None, &mut out) } {
        Ok(()) => {}
        Err(e) if not_found(&e) => return Err(AppError::new(ErrorCode::NotFound, "err.not_found")),
        Err(e) => return Err(AppError::win32(e.code().0 as u32, "CredReadW")),
    }
    let secret = unsafe {
        let c = &*out;
        let bytes = std::slice::from_raw_parts(c.CredentialBlob, c.CredentialBlobSize as usize);
        let mut units: Vec<u16> = bytes.chunks_exact(2).map(|b| u16::from_le_bytes([b[0], b[1]])).collect();
        let s = String::from_utf16_lossy(&units);
        units.zeroize();
        CredFree(out as *const _);
        s
    };
    Ok(Zeroizing::new(secret))
}

pub fn delete(user: &str) -> AppResult<()> {
    let name = target(user);
    match unsafe { CredDeleteW(PCWSTR(name.as_ptr()), CRED_TYPE_GENERIC, None) } {
        Ok(()) => Ok(()),
        Err(e) if not_found(&e) => Ok(()),
        Err(e) => Err(AppError::win32(e.code().0 as u32, "CredDeleteW")),
    }
}

/// 32 characters from the OS CSPRNG with at least one of each class; no look-alike characters.
pub fn generate_password() -> Zeroizing<String> {
    let mut rng = OsRng;
    let classes = [UPPER, LOWER, DIGITS, SYMBOLS];
    let all: Vec<char> = classes.concat().chars().collect();
    let mut chars: Zeroizing<Vec<char>> = Zeroizing::new(
        classes.iter().map(|c| { let v: Vec<char> = c.chars().collect(); v[rng.gen_range(0..v.len())] }).collect(),
    );
    while chars.len() < 32 {
        chars.push(all[rng.gen_range(0..all.len())]);
    }
    chars.shuffle(&mut rng);
    Zeroizing::new(chars.iter().collect())
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorCode;

    #[test]
    fn vault_roundtrip_and_delete() {
        let user = format!("test-{}", uuid::Uuid::new_v4());
        let secret = Zeroizing::new("pässwörd-✓-123".to_string());
        put(&user, &secret).unwrap();
        assert_eq!(*get(&user).unwrap(), *secret);
        delete(&user).unwrap();
        assert_eq!(get(&user).unwrap_err().code, ErrorCode::NotFound);
        delete(&user).unwrap(); // deleting twice is fine
    }

    #[test]
    fn put_overwrites() {
        let user = format!("test-{}", uuid::Uuid::new_v4());
        put(&user, &Zeroizing::new("one".into())).unwrap();
        put(&user, &Zeroizing::new("two".into())).unwrap();
        assert_eq!(*get(&user).unwrap(), "two");
        delete(&user).unwrap();
    }

    #[test]
    fn password_meets_policy() {
        for _ in 0..50 {
            let p = generate_password();
            assert_eq!(p.chars().count(), 32);
            assert!(p.chars().any(|c| c.is_ascii_uppercase()));
            assert!(p.chars().any(|c| c.is_ascii_lowercase()));
            assert!(p.chars().any(|c| c.is_ascii_digit()));
            assert!(p.chars().any(|c| SYMBOLS.contains(c)));
        }
        assert_ne!(*generate_password(), *generate_password());
    }
}
