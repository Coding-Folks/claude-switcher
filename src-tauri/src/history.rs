//! "Impor riwayat": copies Claude Desktop's Code session list for a Claude account from the main
//! user's data folders into a hidden user's data folder. Only accounts the hidden user has already
//! signed in to are copied, and existing files are never overwritten.

use crate::error::AppResult;
use std::os::windows::fs::MetadataExt;
use std::path::{Path, PathBuf};

const REPARSE: u32 = 0x400;

fn subdirs(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.metadata().map(|m| m.is_dir() && m.file_attributes() & REPARSE == 0).unwrap_or(false))
        .map(|e| e.path())
        .collect();
    v.sort();
    v
}

fn name_starts(p: &Path, prefix: &str) -> bool {
    p.file_name().map(|n| n.to_string_lossy().to_lowercase().starts_with(prefix)).unwrap_or(false)
}

/// The Code session folders Claude Desktop may use under a `%LOCALAPPDATA%`: its own data folder
/// (`Claude-Data\claude_*`) and the MSIX-virtualized roaming folder (`Packages\Claude_*\LocalCache\Roaming\Claude`).
pub fn sessions_dirs(local_appdata: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for d in subdirs(&local_appdata.join("Claude-Data")).into_iter().filter(|d| name_starts(d, "claude_")) {
        out.push(d.join("claude-code-sessions"));
    }
    for d in subdirs(&local_appdata.join("Packages")).into_iter().filter(|d| name_starts(d, "claude_")) {
        out.push(d.join(r"LocalCache\Roaming\Claude\claude-code-sessions"));
    }
    out.retain(|p| p.is_dir());
    out
}

fn files_under(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir).into_iter().flatten().flatten().collect();
    entries.sort_by_key(|e| e.path());
    for e in entries {
        let Ok(md) = e.metadata() else { continue };
        if md.file_attributes() & REPARSE != 0 {
            continue; // never follow links
        }
        if md.is_dir() {
            files_under(&e.path(), out);
        } else {
            out.push(e.path());
        }
    }
}

/// (from, to) pairs: every file of each account folder the destination already has, from the
/// sources in order, skipping names that exist in the destination or were planned from an earlier source.
pub fn import_plan(sources: &[PathBuf], dest: &Path) -> Vec<(PathBuf, PathBuf)> {
    let accounts: Vec<PathBuf> = subdirs(dest);
    let mut plan: Vec<(PathBuf, PathBuf)> = Vec::new();
    for src in sources {
        for acct in &accounts {
            let name = acct.file_name().expect("account dir has a name");
            let from_root = src.join(name);
            let mut files = Vec::new();
            files_under(&from_root, &mut files);
            for f in files {
                let rel = f.strip_prefix(&from_root).expect("file under its root");
                let to = acct.join(rel);
                if !to.exists() && !plan.iter().any(|(_, t)| *t == to) {
                    plan.push((f, to));
                }
            }
        }
    }
    plan
}

/// Copies the plan; a target that appeared meanwhile is left alone. Returns how many files were copied.
pub fn copy_plan(plan: &[(PathBuf, PathBuf)]) -> AppResult<usize> {
    let mut n = 0;
    for (from, to) in plan {
        if to.exists() {
            continue;
        }
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(from, to)?;
        n += 1;
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    fn tmp() -> PathBuf {
        let d = std::env::temp_dir().join(format!("cs-hist-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }
    fn touch(p: &Path, body: &str) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, body).unwrap();
    }

    #[test]
    fn finds_sessions_dirs_in_claude_data_and_msix_cache() {
        let local = tmp();
        let a = local.join(r"Claude-Data\claude_pzs8sxrjxfjjc\claude-code-sessions");
        let b = local.join(r"Packages\Claude_pzs8sxrjxfjjc\LocalCache\Roaming\Claude\claude-code-sessions");
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        std::fs::create_dir_all(local.join(r"Packages\Other_123\LocalCache\Roaming\Claude\claude-code-sessions")).unwrap();
        let mut got = sessions_dirs(&local);
        got.sort();
        let mut want = vec![a, b];
        want.sort();
        assert_eq!(got, want);
    }

    #[test]
    fn plan_copies_only_accounts_present_in_destination_and_skips_existing() {
        let root = tmp();
        let src1 = root.join("src1");
        let src2 = root.join("src2");
        let dest = root.join("dest");
        // destination signed in to account A only, with one session already there
        touch(&dest.join(r"acct-A\org-1\local_existing.json"), "dest");
        // sources: account A sessions (one duplicate), and account B which must not be copied
        touch(&src1.join(r"acct-A\org-1\local_existing.json"), "src");
        touch(&src1.join(r"acct-A\org-1\local_one.json"), "1");
        touch(&src2.join(r"acct-A\org-2\local_two.json"), "2");
        touch(&src2.join(r"acct-A\org-1\local_one.json"), "1-dup-in-second-source");
        touch(&src1.join(r"acct-B\org-9\local_private.json"), "B");

        let plan = import_plan(&[src1.clone(), src2.clone()], &dest);
        let targets: Vec<String> = plan.iter().map(|(_, to)| to.strip_prefix(&dest).unwrap().display().to_string()).collect();
        assert_eq!(targets, vec![r"acct-A\org-1\local_one.json".to_string(), r"acct-A\org-2\local_two.json".to_string()]);
        assert_eq!(plan[0].0, src1.join(r"acct-A\org-1\local_one.json")); // first source wins
    }

    #[test]
    fn plan_is_empty_when_destination_has_no_signed_in_account() {
        let root = tmp();
        touch(&root.join(r"src\acct-A\org\local_x.json"), "x");
        std::fs::create_dir_all(root.join("dest")).unwrap();
        assert!(import_plan(&[root.join("src")], &root.join("dest")).is_empty());
    }

    #[test]
    fn copy_plan_copies_and_never_overwrites() {
        let root = tmp();
        let (from, to) = (root.join("a.json"), root.join(r"d\x\a.json"));
        touch(&from, "new");
        assert_eq!(copy_plan(&[(from.clone(), to.clone())]).unwrap(), 1);
        assert_eq!(std::fs::read_to_string(&to).unwrap(), "new");
        std::fs::write(&from, "changed").unwrap();
        assert_eq!(copy_plan(&[(from, to.clone())]).unwrap(), 0);
        assert_eq!(std::fs::read_to_string(&to).unwrap(), "new");
    }
}
