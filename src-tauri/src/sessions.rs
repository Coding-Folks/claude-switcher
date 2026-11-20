//! Claude Code sessions of one account, and copying one of them to another account so it can be
//! continued there (for when an account hits its usage limit). A session is listed by a small
//! `local_<id>.json` under `claude-code-sessions\<account>\<org>\`; its conversation lives in the
//! shared `~/.claude`, so copying that file is all it takes.

use crate::error::{AppError, AppResult, ErrorCode};
use serde::Serialize;
use serde_json::Value;
use std::path::{Path, PathBuf};

/// One session as the panel lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SessionSummary {
    /// `local_<id>`: also the file name.
    pub id: String,
    pub title: String,
    /// The last folder of its working directory (e.g. "Coding Folks").
    pub folder: String,
    /// ms since 1970.
    pub last_activity: i64,
    #[serde(skip)]
    pub path: PathBuf,
}

/// A timestamp stored as a number or as a string of digits.
fn millis(v: &Value) -> Option<i64> {
    v.as_i64().or_else(|| v.as_str()?.trim().parse().ok())
}

fn subdirs(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .map(|e| e.path())
        .collect()
}

/// `<root>\<account>\<org>` folders under the `claude-code-sessions` roots.
fn org_dirs(roots: &[PathBuf]) -> Vec<PathBuf> {
    roots
        .iter()
        .flat_map(|r| subdirs(r))
        .flat_map(|a| subdirs(&a))
        .collect()
}

fn summary(path: &Path) -> Option<SessionSummary> {
    let name = path.file_stem()?.to_string_lossy().into_owned();
    if !name.starts_with("local_") {
        return None;
    }
    // A file Claude is writing right now may not parse yet: skip it.
    let v: Value = serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()?;
    if v.get("isArchived").and_then(Value::as_bool) == Some(true) {
        return None;
    }
    let cwd = v.get("cwd").and_then(Value::as_str).unwrap_or("");
    Some(SessionSummary {
        id: name,
        title: v
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        folder: Path::new(cwd)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        last_activity: v.get("lastActivityAt").and_then(millis).unwrap_or(0),
        path: path.to_path_buf(),
    })
}

/// The account's sessions, newest first, without archived or unreadable ones, at most `max`.
pub fn list(roots: &[PathBuf], max: usize) -> Vec<SessionSummary> {
    let mut out: Vec<SessionSummary> = org_dirs(roots)
        .iter()
        .flat_map(|d| std::fs::read_dir(d).into_iter().flatten().flatten())
        .filter_map(|e| summary(&e.path()))
        .collect();
    out.sort_by(|a, b| b.last_activity.cmp(&a.last_activity).then(a.id.cmp(&b.id)));
    out.truncate(max);
    out
}

/// The `<account>\<org>` folder Claude uses now in an account: the one changed most recently.
pub fn target_dir(roots: &[PathBuf]) -> Option<PathBuf> {
    org_dirs(roots)
        .into_iter()
        .filter_map(|d| Some((std::fs::metadata(&d).ok()?.modified().ok()?, d)))
        .max_by_key(|(t, _)| *t)
        .map(|(_, d)| d)
}

pub enum CopyPlan {
    Write(String),
    DestinationNewer,
}

/// The JSON to write in the destination: focused now (so it is at the top of the list) and not
/// archived; nothing when the destination already has a newer copy of this session.
pub fn plan_copy(src_json: &str, dest_existing: Option<&str>, now_ms: i64) -> AppResult<CopyPlan> {
    let mut v: Value = serde_json::from_str(src_json)
        .map_err(|e| AppError::new(ErrorCode::Invalid, "err.generic").with_detail(e.to_string()))?;
    let activity = |v: &Value| v.get("lastActivityAt").and_then(millis).unwrap_or(0);
    if let Some(dest) = dest_existing.and_then(|d| serde_json::from_str::<Value>(d).ok()) {
        if activity(&dest) > activity(&v) {
            return Ok(CopyPlan::DestinationNewer);
        }
    }
    let obj = v
        .as_object_mut()
        .ok_or_else(|| AppError::new(ErrorCode::Invalid, "err.generic"))?;
    // Keep the type Claude uses there (a number today, a string in older files).
    let focused = match obj.get("lastFocusedAt") {
        Some(Value::String(_)) => Value::String(now_ms.to_string()),
        _ => Value::from(now_ms),
    };
    obj.insert("lastFocusedAt".into(), focused);
    obj.insert("isArchived".into(), Value::Bool(false));
    Ok(CopyPlan::Write(
        serde_json::to_string(&v).expect("a JSON value serializes"),
    ))
}

/// Copies a session file into the destination's `<account>\<org>` folder under its own name.
pub fn copy(src: &Path, dest_dir: &Path, now_ms: i64) -> AppResult<()> {
    let name = src
        .file_name()
        .ok_or_else(|| AppError::new(ErrorCode::Invalid, "err.generic"))?;
    let dest = dest_dir.join(name);
    let existing = std::fs::read_to_string(&dest).ok();
    match plan_copy(&std::fs::read_to_string(src)?, existing.as_deref(), now_ms)? {
        CopyPlan::DestinationNewer => Err(AppError::new(ErrorCode::Invalid, "err.continue_newer")),
        CopyPlan::Write(json) => {
            let tmp = dest.with_extension("json.tmp");
            std::fs::write(&tmp, json)?;
            std::fs::rename(&tmp, &dest)?;
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    fn tmp() -> PathBuf {
        let d = std::env::temp_dir().join(format!("cs-sessions-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }
    fn write(p: &Path, body: &str) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, body).unwrap();
    }
    fn session(id: &str, title: &str, cwd: &str, last: &str, archived: bool) -> String {
        serde_json::json!({
            "sessionId": id, "cliSessionId": "cli-1", "cwd": cwd, "title": title,
            "lastActivityAt": serde_json::from_str::<serde_json::Value>(last).unwrap(),
            "lastFocusedAt": "1", "isArchived": archived, "model": "claude-opus-5-5"
        })
        .to_string()
    }

    #[test]
    fn lists_recent_sessions_newest_first_without_archived_or_broken_files() {
        let root = tmp().join("claude-code-sessions");
        let org = root.join("acc").join("org");
        write(
            &org.join("local_a.json"),
            &session("local_a", "Old", r"E:\x\Coding Folks", "\"100\"", false),
        );
        write(
            &org.join("local_b.json"),
            &session(
                "local_b",
                "",
                r"E:\Folder Kerjaan\Coding Folks",
                "\"300\"",
                false,
            ),
        );
        write(
            &org.join("local_c.json"),
            &session("local_c", "Gone", r"E:\y", "\"500\"", true),
        );
        write(&org.join("local_d.json"), "{ broken");
        write(&org.join("scheduled-tasks.json"), "[]");
        let got = list(&[root], 8);
        let ids: Vec<&str> = got.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, ["local_b", "local_a"]);
        assert_eq!(got[0].folder, "Coding Folks");
        assert_eq!(got[0].title, "");
        assert_eq!(got[0].last_activity, 300);
    }

    #[test]
    fn timestamps_as_numbers_or_strings_sort_the_same() {
        let root = tmp().join("claude-code-sessions");
        let org = root.join("acc").join("org");
        write(
            &org.join("local_n.json"),
            &session("local_n", "Number", "C:\\", "200", false),
        );
        write(
            &org.join("local_s.json"),
            &session("local_s", "String", "C:\\", "\"150\"", false),
        );
        let ids: Vec<String> = list(&[root], 8).into_iter().map(|s| s.id).collect();
        assert_eq!(ids, ["local_n", "local_s"]);
    }

    #[test]
    fn the_list_is_capped() {
        let root = tmp().join("claude-code-sessions");
        let org = root.join("acc").join("org");
        for i in 0..12 {
            write(
                &org.join(format!("local_{i}.json")),
                &session(
                    &format!("local_{i}"),
                    "t",
                    "C:\\",
                    &format!("\"{i}\""),
                    false,
                ),
            );
        }
        assert_eq!(list(&[root], 8).len(), 8);
    }

    #[test]
    fn the_most_recently_changed_account_folder_is_the_target() {
        let root = tmp().join("claude-code-sessions");
        assert_eq!(target_dir(std::slice::from_ref(&root)), None);
        let old = root.join("acc1").join("org1");
        let new = root.join("acc2").join("org2");
        std::fs::create_dir_all(&old).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(50));
        std::fs::create_dir_all(&new).unwrap();
        assert_eq!(target_dir(&[root]), Some(new));
    }

    #[test]
    fn a_copy_is_focused_now_and_unarchived() {
        let src = session("local_a", "Wedding", "C:\\", "\"300\"", true);
        let CopyPlan::Write(out) = plan_copy(&src, None, 9_999).unwrap() else {
            panic!("expected a write")
        };
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["lastFocusedAt"], "9999");
        assert_eq!(v["isArchived"], false);
        assert_eq!(v["title"], "Wedding");
        assert_eq!(v["cliSessionId"], "cli-1");
        assert_eq!(v["model"], "claude-opus-5-5");
    }

    #[test]
    fn a_newer_copy_in_the_destination_is_kept() {
        let src = session("local_a", "t", "C:\\", "\"300\"", false);
        let dest = session("local_a", "t", "C:\\", "\"400\"", false);
        assert!(matches!(
            plan_copy(&src, Some(&dest), 1).unwrap(),
            CopyPlan::DestinationNewer
        ));
    }

    #[test]
    fn an_older_or_equal_copy_is_replaced() {
        let src = session("local_a", "t", "C:\\", "\"300\"", false);
        let same = session("local_a", "t", "C:\\", "\"300\"", false);
        let older = session("local_a", "t", "C:\\", "\"200\"", false);
        assert!(matches!(
            plan_copy(&src, Some(&same), 1).unwrap(),
            CopyPlan::Write(_)
        ));
        assert!(matches!(
            plan_copy(&src, Some(&older), 1).unwrap(),
            CopyPlan::Write(_)
        ));
        assert!(matches!(
            plan_copy(&src, Some("{ broken"), 1).unwrap(),
            CopyPlan::Write(_)
        ));
    }

    #[test]
    fn copy_writes_the_file_under_its_own_name() {
        let base = tmp();
        let src = base.join("local_a.json");
        write(&src, &session("local_a", "t", "C:\\", "\"300\"", false));
        let dest = base.join("dest").join("acc").join("org");
        std::fs::create_dir_all(&dest).unwrap();
        copy(&src, &dest, 42).unwrap();
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dest.join("local_a.json")).unwrap())
                .unwrap();
        assert_eq!(v["lastFocusedAt"], "42");
        // a newer copy there is reported, not overwritten
        write(
            &dest.join("local_a.json"),
            &session("local_a", "t", "C:\\", "\"900\"", false),
        );
        let e = copy(&src, &dest, 43).unwrap_err();
        assert_eq!(e.message_id, "err.continue_newer");
    }
}
