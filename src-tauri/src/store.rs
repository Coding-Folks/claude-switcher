//! accounts.json: atomic writes, recovery from a corrupt file. Never stores passwords.

use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountKind {
    Main,
    WindowsUser,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    pub id: String,
    pub label: String,
    pub kind: AccountKind,
    pub windows_user: Option<String>,
    pub created_at: String,
}

impl Account {
    pub fn main() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            label: "Utama".into(),
            kind: AccountKind::Main,
            windows_user: None,
            created_at: chrono::Utc::now().to_rfc3339(),
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum LoadOutcome {
    Ok(Vec<Account>),
    RecoveredFromCorrupt { accounts: Vec<Account>, backup: PathBuf },
}

#[derive(Serialize, Deserialize)]
struct FileV1 {
    version: u32,
    accounts: Vec<Account>,
}

pub struct Store {
    path: PathBuf,
}

impl Store {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load(&self) -> AppResult<LoadOutcome> {
        let text = match std::fs::read_to_string(&self.path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(LoadOutcome::Ok(vec![Account::main()])),
            Err(e) => return Err(AppError::from(e)),
        };
        match serde_json::from_str::<FileV1>(&text) {
            Ok(f) if f.version == 1 => Ok(LoadOutcome::Ok(f.accounts)),
            _ => {
                let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
                let backup = self.path.with_file_name(format!("accounts.json.bad-{stamp}"));
                std::fs::rename(&self.path, &backup)?;
                Ok(LoadOutcome::RecoveredFromCorrupt { accounts: vec![Account::main()], backup })
            }
        }
    }

    /// Writes `accounts.json.tmp` then renames it over the real file, so a crash never leaves a half file.
    pub fn save(&self, accounts: &[Account]) -> AppResult<()> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = self.path.with_file_name("accounts.json.tmp");
        let body = serde_json::to_string_pretty(&FileV1 { version: 1, accounts: accounts.to_vec() }).map_err(AppError::io)?;
        std::fs::write(&tmp, body)?;
        std::fs::rename(&tmp, &self.path)?;
        Ok(())
    }
}


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
