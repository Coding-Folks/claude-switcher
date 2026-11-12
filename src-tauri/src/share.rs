//! Sharing the main user's `~/.claude` with hidden users: an ACE on the folder and a junction in
//! each hidden profile. Deleting a profile never follows a junction into the shared folder.

use crate::error::{AppError, AppResult};
use std::os::windows::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The built-in Users group, in icacls' `*SID` form (used by tests; real calls pass a user name).
pub const USERS_SID: &str = "*S-1-5-32-545";
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
const FILE_ATTRIBUTE_READONLY: u32 = 0x1;
const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x10;

fn icacls(args: &[&std::ffi::OsStr]) -> AppResult<String> {
    let system = std::env::var_os("SystemRoot").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
    let out = Command::new(system.join("System32").join("icacls.exe")).args(args).output()?;
    if !out.status.success() {
        return Err(AppError::io(format!("icacls failed: {}", String::from_utf8_lossy(&out.stderr).trim())));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Modify rights on `dir` (and everything inheriting from it) for `trustee`. Re-granting is a no-op.
pub fn grant_shared_access(dir: &Path, trustee: &str) -> AppResult<()> {
    let ace = format!("{trustee}:(OI)(CI)M");
    icacls(&[dir.as_os_str(), "/grant".as_ref(), ace.as_ref(), "/Q".as_ref()]).map(|_| ())
}

pub fn revoke_shared_access(dir: &Path, trustee: &str) -> AppResult<()> {
    icacls(&[dir.as_os_str(), "/remove:g".as_ref(), trustee.as_ref(), "/Q".as_ref()]).map(|_| ())
}

pub fn acl_text(dir: &Path) -> AppResult<String> {
    icacls(&[dir.as_os_str()])
}

fn is_reparse(md: &std::fs::Metadata) -> bool {
    md.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

fn plain(p: &Path) -> String {
    let s = p.to_string_lossy();
    s.strip_prefix(r"\\?\").unwrap_or(&s).trim_end_matches('\\').to_lowercase()
}

/// Creates directory junction `link` → `target` with FSCTL_SET_REPARSE_POINT: no shell involved,
/// so characters such as `&`, `^` or `%` in the paths are taken literally. `link` must not exist.
pub fn create_junction(link: &Path, target: &Path) -> AppResult<()> {
    use windows::core::HSTRING;
    use windows::Win32::Foundation::{CloseHandle, GENERIC_WRITE};
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_MODE, OPEN_EXISTING,
    };
    use windows::Win32::System::IO::DeviceIoControl;
    const IO_REPARSE_TAG_MOUNT_POINT: u32 = 0xA000_0003;
    const FSCTL_SET_REPARSE_POINT: u32 = 0x0009_00A4;

    let target = std::fs::canonicalize(target)?; // absolute, with a \\?\ prefix
    let plain = target.to_string_lossy().trim_start_matches(r"\\?\").to_string();
    let substitute: Vec<u16> = format!(r"\??\{plain}").encode_utf16().collect();
    let print: Vec<u16> = plain.encode_utf16().collect();
    std::fs::create_dir(link)?; // fails if something is already there

    // REPARSE_DATA_BUFFER (MountPointReparseBuffer): header, then "<substitute>\0<print>\0".
    let path_bytes = (substitute.len() + 1 + print.len() + 1) * 2;
    let mut buf: Vec<u8> = Vec::with_capacity(16 + path_bytes);
    buf.extend_from_slice(&IO_REPARSE_TAG_MOUNT_POINT.to_le_bytes());
    buf.extend_from_slice(&((8 + path_bytes) as u16).to_le_bytes()); // ReparseDataLength
    buf.extend_from_slice(&0u16.to_le_bytes()); // Reserved
    buf.extend_from_slice(&0u16.to_le_bytes()); // SubstituteNameOffset
    buf.extend_from_slice(&((substitute.len() * 2) as u16).to_le_bytes());
    buf.extend_from_slice(&(((substitute.len() + 1) * 2) as u16).to_le_bytes()); // PrintNameOffset
    buf.extend_from_slice(&((print.len() * 2) as u16).to_le_bytes());
    for u in substitute.iter().chain([0u16].iter()).chain(print.iter()).chain([0u16].iter()) {
        buf.extend_from_slice(&u.to_le_bytes());
    }

    let fail = |what: &str, link: &Path| {
        let _ = std::fs::remove_dir(link);
        AppError::io(format!("{what} failed for {}", link.display()))
    };
    unsafe {
        let h = CreateFileW(
            &HSTRING::from(link.as_os_str()),
            GENERIC_WRITE.0,
            FILE_SHARE_MODE(0),
            None,
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
            None,
        )
        .map_err(|_| fail("CreateFileW", link))?;
        let mut returned = 0u32;
        let ok = DeviceIoControl(h, FSCTL_SET_REPARSE_POINT, Some(buf.as_ptr() as *const _), buf.len() as u32, None, 0, Some(&mut returned), None);
        let _ = CloseHandle(h);
        ok.map_err(|_| fail("FSCTL_SET_REPARSE_POINT", link))?;
    }
    Ok(())
}

pub fn is_junction_to(link: &Path, target: &Path) -> bool {
    let Ok(md) = std::fs::symlink_metadata(link) else { return false };
    if !is_reparse(&md) {
        return false;
    }
    let Ok(dest) = std::fs::read_link(link) else { return false };
    let want = std::fs::canonicalize(target).unwrap_or_else(|_| target.to_path_buf());
    plain(&dest) == plain(&want)
}

/// Deletes `profile_dir`. Reparse points (junctions, symlinks) are unlinked, never descended into.
pub fn remove_profile_safely(profile_dir: &Path) -> AppResult<()> {
    fn remove(path: &Path) -> std::io::Result<()> {
        let md = std::fs::symlink_metadata(path)?;
        if md.file_attributes() & FILE_ATTRIBUTE_READONLY != 0 {
            let mut perms = md.permissions();
            #[allow(clippy::permissions_set_readonly_false)]
            perms.set_readonly(false);
            std::fs::set_permissions(path, perms)?;
        }
        if is_reparse(&md) {
            // Rust reports a junction as a symlink, not a dir; the attribute says which kind of link it is.
            // Removing a directory link removes only the link.
            let dir_link = md.file_attributes() & FILE_ATTRIBUTE_DIRECTORY != 0;
            return if dir_link { std::fs::remove_dir(path) } else { std::fs::remove_file(path) };
        }
        if md.is_dir() {
            for entry in std::fs::read_dir(path)? {
                remove(&entry?.path())?;
            }
            std::fs::remove_dir(path)
        } else {
            std::fs::remove_file(path)
        }
    }
    match remove(profile_dir) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(AppError::from(e)),
    }
}

fn ps_quote(p: &Path) -> String {
    format!("'{}'", p.to_string_lossy().replace('\'', "''"))
}

/// PowerShell run as the hidden user: makes `%USERPROFILE%\.claude` a junction to the main `.claude`.
/// A real folder in the way is renamed to `.claude.bak-<time>`; a junction to elsewhere is replaced.
/// Prints `junction=ok` or `junction=fail`.
pub fn junction_ps_snippet(main_claude_dir: &Path) -> String {
    let t = ps_quote(main_claude_dir);
    format!(
        "$ErrorActionPreference='Stop'; $t={t}; $l=Join-Path $env:USERPROFILE '.claude'; \
         try {{ \
           if (Test-Path -LiteralPath $l) {{ $i=Get-Item -LiteralPath $l -Force; \
             if ($i.LinkType -eq 'Junction') {{ if ($i.Target -ne $t) {{ $i.Delete() }} }} \
             else {{ Rename-Item -LiteralPath $l -NewName ('.claude.bak-' + (Get-Date -Format 'yyyyMMdd-HHmmss')) }} }}; \
           if (-not (Test-Path -LiteralPath $l)) {{ New-Item -ItemType Junction -Path $l -Target $t | Out-Null }}; \
           if ((Get-Item -LiteralPath $l -Force).Target -eq $t) {{ 'junction=ok' }} else {{ 'junction=fail' }} \
         }} catch {{ 'junction=fail:' + $_.Exception.Message }}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    fn tmp() -> PathBuf {
        let d = std::env::temp_dir().join(format!("cs-share-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }
    fn mklink_j(link: &Path, target: &Path) {
        let ok = Command::new("cmd").args(["/C", "mklink", "/J"]).arg(link).arg(target).output().unwrap().status.success();
        assert!(ok, "mklink /J failed");
    }

    #[test]
    fn remove_profile_never_follows_junction() {
        let root = tmp();
        let target = root.join("main-claude");
        std::fs::create_dir_all(target.join("skills")).unwrap();
        std::fs::write(target.join("keep.txt"), "keep").unwrap();
        std::fs::write(target.join("skills").join("s.md"), "skill").unwrap();
        let profile = root.join("profile");
        std::fs::create_dir_all(profile.join("AppData").join("Local")).unwrap();
        std::fs::write(profile.join("AppData").join("Local").join("x.txt"), "x").unwrap();
        mklink_j(&profile.join(".claude"), &target);
        mklink_j(&profile.join("AppData").join("nested-link"), &target); // a junction deeper in the tree

        remove_profile_safely(&profile).unwrap();

        assert!(!profile.exists());
        assert_eq!(std::fs::read_to_string(target.join("keep.txt")).unwrap(), "keep");
        assert_eq!(std::fs::read_to_string(target.join("skills").join("s.md")).unwrap(), "skill");
    }

    #[test]
    fn create_junction_natively_even_with_odd_characters() {
        let root = tmp().join("a&b^c%PATH% d");
        let target = root.join("target");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("f.txt"), "x").unwrap();
        let link = root.join("link");
        create_junction(&link, &target).unwrap();
        assert!(is_junction_to(&link, &target));
        assert_eq!(std::fs::read_to_string(link.join("f.txt")).unwrap(), "x");
        assert!(create_junction(&link, &target).is_err()); // never over an existing entry
    }

    #[test]
    fn junction_detection() {
        let root = tmp();
        let target = root.join("t");
        std::fs::create_dir_all(&target).unwrap();
        let plain = root.join("plain");
        std::fs::create_dir_all(&plain).unwrap();
        let link = root.join("link");
        mklink_j(&link, &target);
        assert!(is_junction_to(&link, &target));
        assert!(!is_junction_to(&plain, &target));
        assert!(!is_junction_to(&link, &plain));
        assert!(!is_junction_to(&root.join("missing"), &target));
    }

    #[test]
    fn grant_is_idempotent_and_revocable() {
        let dir = tmp();
        let before = acl_text(&dir).unwrap();
        grant_shared_access(&dir, USERS_SID).unwrap();
        let once = acl_text(&dir).unwrap();
        grant_shared_access(&dir, USERS_SID).unwrap();
        assert_ne!(before, once);
        assert_eq!(once, acl_text(&dir).unwrap());
        revoke_shared_access(&dir, USERS_SID).unwrap();
        revoke_shared_access(&dir, USERS_SID).unwrap();
    }

    #[test]
    fn snippet_quotes_paths_with_apostrophes() {
        let s = junction_ps_snippet(Path::new(r"C:\Users\O'Brien\.claude"));
        assert!(s.contains(r"'C:\Users\O''Brien\.claude'"), "{s}");
    }

    #[test]
    fn snippet_creates_junction_and_backs_up_a_real_folder() {
        let root = tmp();
        let target = root.join("main-claude");
        std::fs::create_dir_all(&target).unwrap();
        let profile = root.join("profile");
        std::fs::create_dir_all(profile.join(".claude")).unwrap(); // a real folder is in the way
        std::fs::write(profile.join(".claude").join("old.txt"), "old").unwrap();

        let out = Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", &junction_ps_snippet(&target)])
            .env("USERPROFILE", &profile)
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&out.stdout);
        assert!(text.contains("junction=ok"), "{text} {}", String::from_utf8_lossy(&out.stderr));
        assert!(is_junction_to(&profile.join(".claude"), &target));
        let backups: Vec<_> = std::fs::read_dir(&profile).unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|n| n.starts_with(".claude.bak-"))
            .collect();
        assert_eq!(backups.len(), 1);
        assert!(profile.join(&backups[0]).join("old.txt").exists());
    }
}
