//! Same-user "slots": Claude Desktop run as the main Windows user with LOCALAPPDATA pointed at
//! `%LOCALAPPDATA%\ClaudeSwitcher\slots\<account-id>\Local`. Creating `Claude-Data\claude_<family>`
//! there first makes Claude use it (and lock it) instead of falling back to the shared MSIX profile.
//! `Microsoft\WindowsApps` inside the slot is a junction to the real one, so Claude still finds its
//! execution alias and does not drop the user's "Open with" entry.

use crate::error::{AppError, AppResult, ErrorCode};
use std::path::{Path, PathBuf};

pub fn root_for(slots_dir: &Path, account_id: &str) -> PathBuf {
    slots_dir.join(account_id).join("Local")
}

pub fn data_dir(data_root: &Path, family: &str) -> PathBuf {
    data_root.join("Claude-Data").join(family.to_lowercase())
}

fn apps_link(data_root: &Path) -> PathBuf {
    data_root.join(r"Microsoft\WindowsApps")
}

fn prepare_failed(detail: impl std::fmt::Display) -> AppError {
    AppError::new(ErrorCode::Io, "err.slot_prepare").with_detail(detail.to_string())
}

/// Creates the slot's data folder and the WindowsApps junction. Safe to run again.
pub fn prepare(data_root: &Path, family: &str, real_windowsapps: &Path) -> AppResult<()> {
    std::fs::create_dir_all(data_dir(data_root, family)).map_err(prepare_failed)?;
    let link = apps_link(data_root);
    if crate::share::is_junction_to(&link, real_windowsapps) {
        return Ok(());
    }
    if let Ok(md) = std::fs::symlink_metadata(&link) {
        use std::os::windows::fs::MetadataExt;
        let is_link = md.file_attributes() & 0x400 != 0;
        let empty_dir = md.is_dir()
            && std::fs::read_dir(&link)
                .map(|mut r| r.next().is_none())
                .unwrap_or(false);
        if is_link || empty_dir {
            std::fs::remove_dir(&link).map_err(prepare_failed)?; // a link or an empty folder: removing it touches nothing else
        } else {
            return Err(prepare_failed(format!("{} is in the way", link.display())));
        }
    }
    std::fs::create_dir_all(link.parent().expect("link has a parent")).map_err(prepare_failed)?;
    crate::share::create_junction(&link, real_windowsapps)
        .map_err(|e| prepare_failed(e.detail.unwrap_or_default()))?;
    if !crate::share::is_junction_to(&link, real_windowsapps) {
        return Err(prepare_failed(format!(
            "{} did not become a junction",
            link.display()
        )));
    }
    Ok(())
}

/// The slot's Claude data folder exists (its login is still there), whatever the junction's state.
pub fn has_data(data_root: &Path, family: &str) -> bool {
    data_dir(data_root, family).is_dir()
}

pub fn is_ready(data_root: &Path, family: &str, real_windowsapps: &Path) -> bool {
    data_dir(data_root, family).is_dir()
        && crate::share::is_junction_to(&apps_link(data_root), real_windowsapps)
}

/// Deletes `<slots>\<id>`. Junctions inside (WindowsApps) are unlinked, never followed, and a path
/// that is not directly inside a `slots` folder is refused, so a wrong argument deletes nothing.
pub fn remove(slots_dir: &Path, slot_dir: &Path) -> AppResult<()> {
    let in_slots = slot_dir
        .parent()
        .map(|p| crate::monitor::same_dir(p, slots_dir))
        .unwrap_or(false)
        && !crate::monitor::same_dir(slot_dir, slots_dir);
    if !in_slots {
        return Err(AppError::new(ErrorCode::Invalid, "err.generic")
            .with_detail(format!("not a slot folder: {}", slot_dir.display())));
    }
    crate::share::remove_profile_safely(slot_dir)
}

/// Starts Claude with LOCALAPPDATA pointed at the slot. Also brings an already running slot forward.
pub fn launch(data_root: &Path, real_alias: &Path) -> AppResult<()> {
    if !real_alias.exists() {
        return Err(AppError::new(
            ErrorCode::ClaudeMissing,
            "err.claude_missing",
        ));
    }
    std::process::Command::new(real_alias)
        .env("LOCALAPPDATA", data_root)
        .spawn()?;
    Ok(())
}

/// The last `[user-data-move] <label>` in the slot's Claude log; `already_moved*` means the slot's
/// data folder was taken as is.
pub fn move_label(data_root: &Path) -> Option<String> {
    // Only the end of the log matters (the last label); the log can grow large and this runs on every refresh.
    use std::io::{Read, Seek, SeekFrom};
    const TAIL: u64 = 64 * 1024;
    let mut f = std::fs::File::open(data_root.join(r"Claude\logs\main.log")).ok()?;
    let len = f.metadata().ok()?.len();
    f.seek(SeekFrom::Start(len.saturating_sub(TAIL))).ok()?;
    let mut bytes = Vec::new();
    f.read_to_end(&mut bytes).ok()?;
    let log = String::from_utf8_lossy(&bytes);
    const TAG: &str = "[user-data-move] ";
    let at = log.rfind(TAG)? + TAG.len();
    log[at..].split_whitespace().next().map(str::to_string)
}

/// Copies this Claude account's Code session list from the real data folders into the slot.
pub fn import_sessions(
    real_local_appdata: &Path,
    data_root: &Path,
    family: &str,
) -> AppResult<usize> {
    let dest = data_dir(data_root, family).join("claude-code-sessions");
    let signed_in = std::fs::read_dir(&dest)
        .map(|r| r.flatten().any(|e| e.path().is_dir()))
        .unwrap_or(false);
    if !signed_in {
        return Err(AppError::new(ErrorCode::NotFound, "err.import_no_signin"));
    }
    let sources: Vec<PathBuf> = crate::history::sessions_dirs(real_local_appdata)
        .into_iter()
        .filter(|s| !crate::monitor::same_dir(s, &dest))
        .collect();
    crate::history::copy_plan(&crate::history::import_plan(&sources, &dest))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    fn tmp() -> PathBuf {
        let d = std::env::temp_dir().join(format!("cs-slot-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }
    fn tmp_with_file(name: &str) -> PathBuf {
        let d = tmp();
        std::fs::write(d.join(name), "alias").unwrap();
        d
    }
    fn write(p: PathBuf, body: &str) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, body).unwrap();
    }

    #[test]
    fn paths_follow_the_layout() {
        assert_eq!(
            root_for(Path::new(r"C:\s"), "id-1"),
            PathBuf::from(r"C:\s\id-1\Local")
        );
        assert_eq!(
            data_dir(Path::new(r"C:\s\id-1\Local"), "Claude_PZS"),
            PathBuf::from(r"C:\s\id-1\Local\Claude-Data\claude_pzs")
        );
    }

    #[test]
    fn prepare_makes_data_dir_and_junction_and_is_idempotent() {
        let (root, real_apps) = (tmp().join("Local"), tmp_with_file("claude-desktop.exe"));
        assert!(!is_ready(&root, "Claude_PZS", &real_apps));
        prepare(&root, "Claude_PZS", &real_apps).unwrap();
        prepare(&root, "Claude_PZS", &real_apps).unwrap();
        assert!(root.join(r"Claude-Data\claude_pzs").is_dir());
        assert!(crate::share::is_junction_to(
            &root.join(r"Microsoft\WindowsApps"),
            &real_apps
        ));
        assert!(is_ready(&root, "Claude_PZS", &real_apps));
    }

    #[test]
    fn prepare_replaces_an_empty_plain_folder_where_the_junction_goes() {
        let (root, real_apps) = (tmp().join("Local"), tmp_with_file("claude-desktop.exe"));
        std::fs::create_dir_all(root.join(r"Microsoft\WindowsApps")).unwrap();
        prepare(&root, "Claude_PZS", &real_apps).unwrap();
        assert!(crate::share::is_junction_to(
            &root.join(r"Microsoft\WindowsApps"),
            &real_apps
        ));
    }

    #[test]
    fn remove_keeps_junction_target() {
        let (root, real_apps) = (
            tmp().join("slots").join("id-1").join("Local"),
            tmp_with_file("claude-desktop.exe"),
        );
        prepare(&root, "Claude_PZS", &real_apps).unwrap();
        write(root.join(r"Claude-Data\claude_pzs\Local State"), "{}");
        let slot_dir = root.parent().unwrap();
        remove(slot_dir.parent().unwrap(), slot_dir).unwrap();
        assert!(!root.exists());
        assert!(real_apps.join("claude-desktop.exe").exists());
    }

    #[test]
    fn remove_refuses_paths_outside_a_slot() {
        let slots = tmp().join("slots");
        std::fs::create_dir_all(&slots).unwrap();
        let d = tmp();
        assert!(remove(&slots, &d).is_err()); // not inside the slots folder
        assert!(d.exists());
        // a folder that merely happens to sit in another folder named "slots"
        let elsewhere = tmp().join("slots").join("Documents");
        std::fs::create_dir_all(&elsewhere).unwrap();
        assert!(remove(&slots, &elsewhere).is_err());
        assert!(elsewhere.exists());
        assert!(remove(&slots, &slots).is_err()); // never the slots folder itself
    }

    #[test]
    fn prepare_works_when_the_path_has_cmd_special_characters() {
        let (root, real_apps) = (
            tmp().join("a&b^c%d%").join("Local"),
            tmp_with_file("claude-desktop.exe"),
        );
        prepare(&root, "Claude_PZS", &real_apps).unwrap();
        assert!(crate::share::is_junction_to(
            &root.join(r"Microsoft\WindowsApps"),
            &real_apps
        ));
    }

    #[test]
    fn has_data_tells_a_missing_junction_from_a_missing_data_folder() {
        let (root, real_apps) = (tmp().join("Local"), tmp_with_file("claude-desktop.exe"));
        prepare(&root, "Claude_PZS", &real_apps).unwrap();
        std::fs::remove_dir(root.join(r"Microsoft\WindowsApps")).unwrap(); // only the link goes
        assert!(!is_ready(&root, "Claude_PZS", &real_apps));
        assert!(has_data(&root, "Claude_PZS"));
        assert!(!has_data(&tmp().join("Local"), "Claude_PZS"));
    }

    #[test]
    fn move_label_reads_only_the_end_of_a_big_log() {
        let root = tmp().join("Local");
        let mut log = String::from("[user-data-move] stay_never_left\n");
        log.push_str(&"x".repeat(300_000));
        log.push_str("\n[user-data-move] already_moved_package_emptied {}\n");
        write(root.join(r"Claude\logs\main.log"), &log);
        assert_eq!(
            move_label(&root).as_deref(),
            Some("already_moved_package_emptied")
        );
    }

    #[test]
    fn move_label_reads_the_last_label() {
        let root = tmp().join("Local");
        write(
            root.join(r"Claude\logs\main.log"),
            "2026 [info] [user-data-move] stay_never_left { blocked: null }\n2026 [info] [user-data-move] already_moved_package_emptied { blocked: null }\n",
        );
        assert_eq!(
            move_label(&root).as_deref(),
            Some("already_moved_package_emptied")
        );
        assert_eq!(move_label(&tmp()), None);
    }

    #[test]
    fn import_copies_sessions_without_overwriting() {
        let real = tmp();
        let root = tmp().join("Local");
        write(
            real.join(r"Claude-Data\claude_pzs\claude-code-sessions\acct\org\a.json"),
            "real-a",
        );
        write(
            real.join(r"Claude-Data\claude_pzs\claude-code-sessions\acct\org\b.json"),
            "real-b",
        );
        write(
            root.join(r"Claude-Data\claude_pzs\claude-code-sessions\acct\org\a.json"),
            "slot-a",
        );
        assert_eq!(import_sessions(&real, &root, "Claude_PZS").unwrap(), 1);
        let slot = root.join(r"Claude-Data\claude_pzs\claude-code-sessions\acct\org");
        assert_eq!(
            std::fs::read_to_string(slot.join("a.json")).unwrap(),
            "slot-a"
        );
        assert_eq!(
            std::fs::read_to_string(slot.join("b.json")).unwrap(),
            "real-b"
        );
    }

    #[test]
    fn import_before_sign_in_is_not_found() {
        let real = tmp();
        write(
            real.join(r"Claude-Data\claude_pzs\claude-code-sessions\acct\org\a.json"),
            "x",
        );
        let root = tmp().join("Local");
        std::fs::create_dir_all(root.join(r"Claude-Data\claude_pzs")).unwrap();
        let e = import_sessions(&real, &root, "Claude_PZS").unwrap_err();
        assert_eq!(
            (e.code, e.message_id),
            (crate::error::ErrorCode::NotFound, "err.import_no_signin")
        );
    }

    #[test]
    fn launch_without_alias_is_claude_missing() {
        let e = launch(&tmp(), Path::new(r"C:\nope\claude-desktop.exe")).unwrap_err();
        assert_eq!(e.code, crate::error::ErrorCode::ClaudeMissing);
    }
}
