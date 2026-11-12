//! accounts.json: atomic writes, recovery from a corrupt file.

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("cs-store-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }
    fn acct(label: &str, kind: AccountKind, user: Option<&str>) -> Account {
        Account { id: uuid::Uuid::new_v4().to_string(), label: label.into(), kind, windows_user: user.map(Into::into), created_at: "2026-11-12T08:00:00Z".into() }
    }

    #[test]
    fn store_roundtrip() {
        let s = Store::new(tmp().join("accounts.json"));
        let list = vec![acct("Utama", AccountKind::Main, None), acct("Kantor", AccountKind::WindowsUser, Some("claude-kantor"))];
        s.save(&list).unwrap();
        match s.load().unwrap() { LoadOutcome::Ok(got) => assert_eq!(got, list), other => panic!("{other:?}") }
    }
    #[test]
    fn store_missing_file_gives_main() {
        let s = Store::new(tmp().join("accounts.json"));
        match s.load().unwrap() {
            LoadOutcome::Ok(got) => { assert_eq!(got.len(), 1); assert_eq!(got[0].kind, AccountKind::Main); assert_eq!(got[0].label, "Utama"); }
            other => panic!("{other:?}"),
        }
    }
    #[test]
    fn store_corrupt_is_backed_up() {
        let dir = tmp();
        let path = dir.join("accounts.json");
        std::fs::write(&path, "{oops").unwrap();
        match Store::new(path).load().unwrap() {
            LoadOutcome::RecoveredFromCorrupt { accounts, backup } => {
                assert_eq!(accounts.len(), 1);
                assert_eq!(std::fs::read_to_string(&backup).unwrap(), "{oops");
                assert!(backup.file_name().unwrap().to_string_lossy().starts_with("accounts.json.bad-"));
            }
            other => panic!("{other:?}"),
        }
    }
    #[test]
    fn store_atomic_write_leaves_no_tmp() {
        let dir = tmp();
        let s = Store::new(dir.join("accounts.json"));
        s.save(&[acct("Utama", AccountKind::Main, None)]).unwrap();
        s.save(&[acct("Utama", AccountKind::Main, None)]).unwrap();
        let names: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        assert_eq!(names, vec!["accounts.json".to_string()]);
    }
    #[test]
    fn json_shape_is_versioned_snake_case() {
        let dir = tmp();
        let s = Store::new(dir.join("accounts.json"));
        s.save(&[acct("Kantor", AccountKind::WindowsUser, Some("claude-kantor"))]).unwrap();
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join("accounts.json")).unwrap()).unwrap();
        assert_eq!(v["version"], 1);
        assert_eq!(v["accounts"][0]["kind"], "windows_user");
    }
}
