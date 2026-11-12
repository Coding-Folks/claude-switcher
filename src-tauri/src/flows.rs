//! Account flows (add, open, close, rename, remove, repair, list) over the `Platform` trait,
//! so they can be tested without touching Windows.

use crate::elevated::ElevatedOp;
use crate::error::{AppError, AppResult, ErrorCode};
pub use crate::platform::Platform;
use crate::store::{Account, AccountKind, LoadOutcome, Store};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub const LOW_RAM: u64 = 2 << 30;
pub const OPEN_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum AccountState {
    Running,
    Stopped,
    Opening,
    Problem,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum OpenOutcome {
    Focused,
    Launched,
    NeedsLowRamConfirm,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AccountView {
    pub account: Account,
    pub state: AccountState,
    pub bytes: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
pub struct RepairReport {
    pub password_reset: bool,
    pub rebootstrapped: bool,
}

pub struct Flows<P: Platform> {
    pub(crate) p: P,
    pub(crate) store: Store,
    /// Serializes read-modify-write of accounts.json.
    write: Mutex<()>,
    opening: Mutex<HashMap<String, Instant>>,
    problems: Mutex<HashSet<String>>,
    /// Set when accounts.json was corrupt at startup; the UI shows it once.
    pub recovered_backup: Mutex<Option<std::path::PathBuf>>,
}

fn invalid(why: &str) -> AppError {
    AppError::new(ErrorCode::Invalid, "err.generic").with_detail(why.to_string())
}

impl<P: Platform> Flows<P> {
    pub fn new(p: P, store: Store) -> Self {
        Self {
            p,
            store,
            write: Mutex::new(()),
            opening: Mutex::new(HashMap::new()),
            problems: Mutex::new(HashSet::new()),
            recovered_backup: Mutex::new(None),
        }
    }

    /// True while some account is waiting for its Claude window; the poller then keeps refreshing.
    pub fn has_opening(&self) -> bool {
        !self.opening.lock().unwrap().is_empty()
    }

    pub fn platform(&self) -> &P {
        &self.p
    }

    pub fn accounts(&self) -> AppResult<Vec<Account>> {
        let first_run = !self.store.path().exists();
        match self.store.load()? {
            LoadOutcome::Ok(list) => {
                if first_run {
                    self.store.save(&list)?; // persist the generated main account so its id stays stable
                }
                Ok(list)
            }
            LoadOutcome::RecoveredFromCorrupt { accounts, backup } => {
                self.store.save(&accounts)?;
                *self.recovered_backup.lock().unwrap() = Some(backup);
                Ok(accounts)
            }
        }
    }

    fn find(&self, id: &str) -> AppResult<Account> {
        self.accounts()?.into_iter().find(|a| a.id == id).ok_or_else(|| AppError::new(ErrorCode::NotFound, "err.not_found"))
    }

    fn sid(&self, a: &Account) -> Option<String> {
        match (&a.kind, &a.windows_user) {
            (AccountKind::Main, _) => self.p.main_sid(),
            (AccountKind::WindowsUser, Some(u)) => self.p.sid_of(u),
            _ => None,
        }
    }

    fn running_root(&self, a: &Account, snap: &crate::monitor::Snapshot) -> Option<(u32, u64)> {
        let sid = self.sid(a)?;
        snap.instances.iter().find(|i| i.owner_sid == sid).map(|i| (i.root_pid, i.bytes))
    }

    fn user(a: &Account) -> AppResult<&str> {
        a.windows_user.as_deref().ok_or_else(|| invalid("account has no windows user"))
    }

    pub fn list(&self) -> AppResult<Vec<AccountView>> {
        let snap = self.p.snapshot();
        let mut opening = self.opening.lock().unwrap();
        let mut problems = self.problems.lock().unwrap();
        Ok(self
            .accounts()?
            .into_iter()
            .map(|a| {
                let run = self.running_root(&a, &snap);
                if run.is_some() {
                    opening.remove(&a.id);
                } else if opening.get(&a.id).map(|t| t.elapsed() >= OPEN_TIMEOUT).unwrap_or(false) {
                    // Claude did not appear within 30 s (spec §4): report it instead of going quiet.
                    opening.remove(&a.id);
                    problems.insert(a.id.clone());
                }
                let state = if run.is_some() {
                    AccountState::Running
                } else if problems.contains(&a.id) {
                    AccountState::Problem
                } else if opening.get(&a.id).map(|t| t.elapsed() < OPEN_TIMEOUT).unwrap_or(false) {
                    AccountState::Opening
                } else {
                    AccountState::Stopped
                };
                AccountView { bytes: run.map(|r| r.1), account: a, state }
            })
            .collect())
    }

    pub fn add(&self, label: &str, progress: &dyn Fn(&str)) -> AppResult<Account> {
        let label = label.trim();
        if label.is_empty() {
            return Err(invalid("empty label"));
        }
        let existing = self.accounts()?;
        let taken = |n: &str| self.p.user_exists(n) || existing.iter().any(|a| a.windows_user.as_deref() == Some(n));
        let user = crate::naming::windows_user_for(label, &taken);
        let pw = self.p.new_password();

        progress("vault");
        self.p.vault_put(&user, &pw)?;
        progress("create_user");
        if let Err(e) = self.p.elevated(ElevatedOp::CreateUser { name: user.clone() }, Some(&pw)) {
            let _ = self.p.vault_delete(&user);
            return Err(e);
        }
        let undo_user = || {
            let _ = self.p.elevated(ElevatedOp::DeleteUser { name: user.clone() }, None);
            let _ = self.p.vault_delete(&user);
        };
        progress("grant");
        if let Err(e) = self.p.grant(&user) {
            undo_user();
            return Err(e);
        }
        progress("bootstrap");
        let report = self.p.bootstrap(&user, &pw);
        let failure = match report {
            Ok(r) if r.ok() => None,
            Ok(r) => Some(AppError::io(format!(
                "Claude setup failed (register={}, junction={}, alias={})",
                r.registered, r.junction_ok, r.alias_present
            ))),
            Err(e) => Some(e),
        };
        if let Some(e) = failure {
            let _ = self.p.revoke(&user);
            undo_user();
            return Err(e);
        }
        let account = Account {
            id: uuid::Uuid::new_v4().to_string(),
            label: label.to_string(),
            kind: AccountKind::WindowsUser,
            windows_user: Some(user.clone()),
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        {
            let _w = self.write.lock().unwrap();
            let mut list = self.accounts()?;
            list.push(account.clone());
            self.store.save(&list)?;
        }
        progress("launch");
        match self.p.launch_as(&user, &pw) {
            Ok(()) => {
                self.opening.lock().unwrap().insert(account.id.clone(), Instant::now());
            }
            Err(_) => {
                self.problems.lock().unwrap().insert(account.id.clone());
            }
        }
        Ok(account)
    }

    pub fn open(&self, id: &str, confirmed_low_ram: bool) -> AppResult<OpenOutcome> {
        let a = self.find(id)?;
        let snap = self.p.snapshot();
        let running = self.running_root(&a, &snap).is_some();
        if !running && snap.free_ram < LOW_RAM && !confirmed_low_ram {
            return Ok(OpenOutcome::NeedsLowRamConfirm);
        }
        let result = match a.kind {
            AccountKind::Main => self.p.launch_main(),
            AccountKind::WindowsUser => {
                let user = Self::user(&a)?;
                let pw = self.p.vault_get(user)?;
                if !running && !self.p.junction_ok(user) {
                    let r = self.p.bootstrap(user, &pw)?;
                    if !r.ok() {
                        self.problems.lock().unwrap().insert(a.id.clone());
                        return Err(AppError::new(ErrorCode::JunctionBroken, "err.junction_broken"));
                    }
                }
                self.p.launch_as(user, &pw)
            }
        };
        if let Err(e) = result {
            if e.code == ErrorCode::LogonFailed {
                self.problems.lock().unwrap().insert(a.id.clone());
            }
            return Err(e);
        }
        if running {
            return Ok(OpenOutcome::Focused);
        }
        self.opening.lock().unwrap().insert(a.id.clone(), Instant::now());
        Ok(OpenOutcome::Launched)
    }

    pub fn close(&self, id: &str) -> AppResult<()> {
        let a = self.find(id)?;
        if let Some((pid, _)) = self.running_root(&a, &self.p.snapshot()) {
            self.p.close(pid, a.windows_user.as_deref())?;
        }
        self.opening.lock().unwrap().remove(id);
        Ok(())
    }

    pub fn rename(&self, id: &str, label: &str) -> AppResult<()> {
        let label = label.trim();
        if label.is_empty() {
            return Err(invalid("empty label"));
        }
        let _w = self.write.lock().unwrap();
        let mut list = self.accounts()?;
        let a = list.iter_mut().find(|a| a.id == id).ok_or_else(|| AppError::new(ErrorCode::NotFound, "err.not_found"))?;
        a.label = label.to_string();
        self.store.save(&list)
    }

    pub fn remove(&self, id: &str) -> AppResult<()> {
        let a = self.find(id)?;
        if a.kind == AccountKind::Main {
            return Err(invalid("the main account cannot be removed"));
        }
        let user = Self::user(&a)?.to_string();
        if let Some((pid, _)) = self.running_root(&a, &self.p.snapshot()) {
            self.p.close(pid, Some(&user))?;
        }
        // Revoke while the name still resolves to a SID; icacls cannot remove an ACE by a deleted name.
        self.p.revoke(&user)?;
        if let Err(e) = self.p.elevated(ElevatedOp::DeleteUser { name: user.clone() }, None) {
            let _ = self.p.grant(&user); // the account stays, so give its access back
            return Err(e);
        }
        self.p.vault_delete(&user)?;
        let _w = self.write.lock().unwrap();
        let list: Vec<Account> = self.accounts()?.into_iter().filter(|x| x.id != id).collect();
        self.store.save(&list)?;
        self.problems.lock().unwrap().remove(id);
        self.opening.lock().unwrap().remove(id);
        Ok(())
    }

    /// Copies this Claude account's Code session list from the main user's data into the hidden user's.
    /// Claude must not be running as that user while its files are written, so it is closed first.
    pub fn import_history(&self, id: &str) -> AppResult<()> {
        let a = self.find(id)?;
        if a.kind == AccountKind::Main {
            return Err(invalid("the main account already has its own history"));
        }
        let user = Self::user(&a)?.to_string();
        if let Some((pid, _)) = self.running_root(&a, &self.p.snapshot()) {
            self.p.close(pid, Some(&user))?;
            for _ in 0..20 {
                if self.running_root(&a, &self.p.snapshot()).is_none() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(500));
            }
        }
        self.opening.lock().unwrap().remove(id);
        self.p.elevated(ElevatedOp::ImportSessions { name: user }, None)
    }

    /// Re-runs the bootstrap (which also proves the logon works); on a refused logon, sets a new password first.
    pub fn repair(&self, id: &str) -> AppResult<RepairReport> {
        let a = self.find(id)?;
        let mut report = RepairReport::default();
        if a.kind == AccountKind::Main {
            self.problems.lock().unwrap().remove(id);
            return Ok(report);
        }
        let user = Self::user(&a)?.to_string();
        // ~/.claude may have been recreated with a fresh ACL; re-granting is idempotent.
        self.p.grant(&user)?;
        let pw = match self.p.vault_get(&user) {
            Ok(pw) => Some(pw),
            Err(e) if e.code == ErrorCode::NotFound => None,
            Err(e) => return Err(e),
        };
        let attempt = match &pw {
            Some(pw) => self.p.bootstrap(&user, pw),
            None => Err(AppError::new(ErrorCode::LogonFailed, "err.logon_failed")),
        };
        let boot = match attempt {
            Err(e) if e.code == ErrorCode::LogonFailed => {
                let fresh = self.p.new_password();
                self.p.elevated(ElevatedOp::SetPassword { name: user.clone() }, Some(&fresh))?;
                self.p.vault_put(&user, &fresh)?;
                report.password_reset = true;
                self.p.bootstrap(&user, &fresh)?
            }
            other => other?,
        };
        report.rebootstrapped = true;
        if !boot.ok() {
            return Err(AppError::new(ErrorCode::JunctionBroken, "err.junction_broken"));
        }
        self.problems.lock().unwrap().remove(id);
        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::elevated::ElevatedOp;
    use crate::error::{AppError, ErrorCode};
    use crate::launcher::BootstrapReport;
    use crate::monitor::{InstanceStatus, Snapshot};
    use crate::store::{Account, AccountKind, Store};
    use crate::vault::Zeroizing;
    use std::collections::{HashMap, HashSet};
    use std::sync::Mutex;

    const GB: u64 = 1 << 30;

    #[derive(Default)]
    struct Fake {
        calls: Mutex<Vec<String>>,
        fail: Mutex<HashMap<&'static str, AppError>>,
        fail_once: Mutex<HashMap<&'static str, AppError>>,
        running: Mutex<Vec<InstanceStatus>>,
        free: Mutex<u64>,
        users: Mutex<HashSet<String>>,
        vault: Mutex<HashMap<String, String>>,
        junction_broken: Mutex<HashSet<String>>,
        report: Mutex<Option<BootstrapReport>>,
    }
    impl Fake {
        fn new() -> Self {
            let f = Self::default();
            *f.free.lock().unwrap() = 9 * GB;
            f
        }
        fn log(&self, s: impl Into<String>) {
            self.calls.lock().unwrap().push(s.into());
        }
        fn check(&self, step: &'static str) -> AppResult<()> {
            if let Some(e) = self.fail_once.lock().unwrap().remove(step) {
                return Err(e);
            }
            match self.fail.lock().unwrap().get(step) {
                Some(e) => Err(e.clone()),
                None => Ok(()),
            }
        }
        fn calls(&self) -> Vec<String> {
            self.calls.lock().unwrap().clone()
        }
    }
    impl Platform for Fake {
        fn user_exists(&self, name: &str) -> bool { self.users.lock().unwrap().contains(name) }
        fn new_password(&self) -> Zeroizing<String> { Zeroizing::new(format!("pw{}", self.calls.lock().unwrap().len())) }
        fn vault_put(&self, user: &str, s: &Zeroizing<String>) -> AppResult<()> {
            self.check("vault_put")?; self.log(format!("vault_put {user}")); self.vault.lock().unwrap().insert(user.into(), s.to_string()); Ok(())
        }
        fn vault_get(&self, user: &str) -> AppResult<Zeroizing<String>> {
            self.vault.lock().unwrap().get(user).map(|s| Zeroizing::new(s.clone())).ok_or_else(|| AppError::new(ErrorCode::NotFound, "err.not_found"))
        }
        fn vault_delete(&self, user: &str) -> AppResult<()> {
            self.log(format!("vault_delete {user}")); self.vault.lock().unwrap().remove(user); Ok(())
        }
        fn elevated(&self, op: ElevatedOp, _s: Option<&Zeroizing<String>>) -> AppResult<()> {
            let step = match &op { ElevatedOp::CreateUser { .. } => "create_user", ElevatedOp::SetPassword { .. } => "set_password", ElevatedOp::DeleteUser { .. } => "delete_user", ElevatedOp::ImportSessions { .. } => "import_sessions" };
            self.check(step)?;
            match &op {
                ElevatedOp::CreateUser { name } => { self.users.lock().unwrap().insert(name.clone()); }
                ElevatedOp::DeleteUser { name } => { self.users.lock().unwrap().remove(name); }
                ElevatedOp::SetPassword { .. } | ElevatedOp::ImportSessions { .. } => {}
            }
            self.log(format!("{step} {}", serde_json::to_value(&op).unwrap()["name"].as_str().unwrap()));
            Ok(())
        }
        fn grant(&self, user: &str) -> AppResult<()> { self.check("grant")?; self.log(format!("grant {user}")); Ok(()) }
        fn revoke(&self, user: &str) -> AppResult<()> { self.log(format!("revoke {user}")); Ok(()) }
        fn bootstrap(&self, user: &str, _pw: &Zeroizing<String>) -> AppResult<BootstrapReport> {
            self.check("bootstrap")?; self.log(format!("bootstrap {user}"));
            self.junction_broken.lock().unwrap().remove(user);
            Ok(self.report.lock().unwrap().unwrap_or(BootstrapReport { registered: true, junction_ok: true, alias_present: true }))
        }
        fn junction_ok(&self, user: &str) -> bool { !self.junction_broken.lock().unwrap().contains(user) }
        fn launch_main(&self) -> AppResult<()> { self.check("launch_main")?; self.log("launch_main"); Ok(()) }
        fn launch_as(&self, user: &str, _pw: &Zeroizing<String>) -> AppResult<()> { self.check("launch_as")?; self.log(format!("launch_as {user}")); Ok(()) }
        fn close(&self, root_pid: u32, as_user: Option<&str>) -> AppResult<()> {
            self.log(format!("close {root_pid} {}", as_user.unwrap_or("main")));
            self.running.lock().unwrap().retain(|i| i.root_pid != root_pid); Ok(())
        }
        fn snapshot(&self) -> Snapshot {
            Snapshot { instances: self.running.lock().unwrap().clone(), free_ram: *self.free.lock().unwrap(), total_ram: 24 * GB }
        }
        fn sid_of(&self, user: &str) -> Option<String> { Some(format!("SID-{user}")) }
        fn main_sid(&self) -> Option<String> { Some("SID-main".into()) }
    }

    fn flows() -> (Flows<Fake>, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("cs-flows-{}", uuid::Uuid::new_v4()));
        let path = dir.join("accounts.json");
        (Flows::new(Fake::new(), Store::new(&path)), path)
    }
    fn add_existing(f: &Flows<Fake>, label: &str, user: &str) -> String {
        let mut list = f.accounts().unwrap();
        let a = Account { id: format!("id-{user}"), label: label.into(), kind: AccountKind::WindowsUser, windows_user: Some(user.into()), created_at: "2026-11-12T00:00:00Z".into() };
        list.push(a.clone());
        f.store.save(&list).unwrap();
        f.p.users.lock().unwrap().insert(user.into());
        f.p.vault.lock().unwrap().insert(user.into(), "pw".into());
        a.id
    }
    fn main_id(f: &Flows<Fake>) -> String {
        f.accounts().unwrap().into_iter().find(|a| a.kind == AccountKind::Main).unwrap().id
    }
    fn err(code: ErrorCode) -> AppError { AppError::new(code, "x") }

    #[test]
    fn add_happy_path_order() {
        let (f, _) = flows();
        let steps = Mutex::new(vec![]);
        let a = f.add("Kantor", &|s| steps.lock().unwrap().push(s.to_string())).unwrap();
        assert_eq!(a.windows_user.as_deref(), Some("claude-kantor"));
        assert_eq!(f.p.calls(), vec!["vault_put claude-kantor", "create_user claude-kantor", "grant claude-kantor", "bootstrap claude-kantor", "launch_as claude-kantor"]);
        assert_eq!(*steps.lock().unwrap(), vec!["vault", "create_user", "grant", "bootstrap", "launch"]);
        assert!(f.accounts().unwrap().iter().any(|x| x.label == "Kantor"));
    }

    #[test]
    fn add_picks_free_name_when_user_exists() {
        let (f, _) = flows();
        f.p.users.lock().unwrap().insert("claude-kantor".into());
        assert_eq!(f.add("Kantor", &|_| {}).unwrap().windows_user.as_deref(), Some("claude-kantor-2"));
    }

    #[test]
    fn add_rejects_blank_label() {
        let (f, _) = flows();
        assert_eq!(f.add("   ", &|_| {}).unwrap_err().code, ErrorCode::Invalid);
        assert!(f.p.calls().is_empty());
    }

    #[test]
    fn add_rolls_back_when_bootstrap_fails() {
        let (f, _) = flows();
        f.p.fail.lock().unwrap().insert("bootstrap", err(ErrorCode::Io));
        assert!(f.add("Kantor", &|_| {}).is_err());
        assert_eq!(f.p.calls(), vec!["vault_put claude-kantor", "create_user claude-kantor", "grant claude-kantor", "revoke claude-kantor", "delete_user claude-kantor", "vault_delete claude-kantor"]);
        assert_eq!(f.accounts().unwrap().len(), 1);
    }

    #[test]
    fn add_rolls_back_when_bootstrap_reports_failure() {
        let (f, _) = flows();
        *f.p.report.lock().unwrap() = Some(BootstrapReport { registered: false, junction_ok: false, alias_present: false });
        assert!(f.add("Kantor", &|_| {}).is_err());
        assert!(f.p.calls().contains(&"delete_user claude-kantor".to_string()));
        assert_eq!(f.accounts().unwrap().len(), 1);
    }

    #[test]
    fn add_uac_cancel_before_user_created() {
        let (f, _) = flows();
        f.p.fail.lock().unwrap().insert("create_user", err(ErrorCode::UacCancelled));
        assert_eq!(f.add("Kantor", &|_| {}).unwrap_err().code, ErrorCode::UacCancelled);
        assert_eq!(f.p.calls(), vec!["vault_put claude-kantor", "vault_delete claude-kantor"]);
    }

    #[test]
    fn open_running_focuses_by_relaunch() {
        let (f, _) = flows();
        let id = add_existing(&f, "Kantor", "claude-kantor");
        f.p.running.lock().unwrap().push(InstanceStatus { owner_sid: "SID-claude-kantor".into(), root_pid: 7, bytes: GB });
        *f.p.free.lock().unwrap() = GB; // low RAM does not matter for an instance already running
        assert_eq!(f.open(&id, false).unwrap(), OpenOutcome::Focused);
        assert_eq!(f.p.calls(), vec!["launch_as claude-kantor"]);
    }

    #[test]
    fn open_low_ram_requires_confirm() {
        let (f, _) = flows();
        let id = add_existing(&f, "Kantor", "claude-kantor");
        *f.p.free.lock().unwrap() = 3 * GB / 2;
        assert_eq!(f.open(&id, false).unwrap(), OpenOutcome::NeedsLowRamConfirm);
        assert!(f.p.calls().is_empty());
        assert_eq!(f.open(&id, true).unwrap(), OpenOutcome::Launched);
        assert_eq!(f.list().unwrap().iter().find(|v| v.account.id == id).unwrap().state, AccountState::Opening);
    }

    #[test]
    fn open_main_uses_launch_main() {
        let (f, _) = flows();
        let id = main_id(&f);
        assert_eq!(f.open(&id, false).unwrap(), OpenOutcome::Launched);
        assert_eq!(f.p.calls(), vec!["launch_main"]);
    }

    #[test]
    fn open_logon_failure_marks_problem() {
        let (f, _) = flows();
        let id = add_existing(&f, "Kantor", "claude-kantor");
        f.p.fail.lock().unwrap().insert("launch_as", err(ErrorCode::LogonFailed));
        assert_eq!(f.open(&id, false).unwrap_err().code, ErrorCode::LogonFailed);
        assert_eq!(f.list().unwrap().iter().find(|v| v.account.id == id).unwrap().state, AccountState::Problem);
    }

    #[test]
    fn open_rebootstraps_broken_junction_first() {
        let (f, _) = flows();
        let id = add_existing(&f, "Kantor", "claude-kantor");
        f.p.junction_broken.lock().unwrap().insert("claude-kantor".into());
        f.open(&id, false).unwrap();
        assert_eq!(f.p.calls(), vec!["bootstrap claude-kantor", "launch_as claude-kantor"]);
    }

    #[test]
    fn list_states_and_bytes() {
        let (f, _) = flows();
        let kantor = add_existing(&f, "Kantor", "claude-kantor");
        f.p.running.lock().unwrap().push(InstanceStatus { owner_sid: "SID-main".into(), root_pid: 1, bytes: 2 * GB });
        let v = f.list().unwrap();
        let main = v.iter().find(|x| x.account.kind == AccountKind::Main).unwrap();
        assert_eq!((main.state, main.bytes), (AccountState::Running, Some(2 * GB)));
        let k = v.iter().find(|x| x.account.id == kantor).unwrap();
        assert_eq!((k.state, k.bytes), (AccountState::Stopped, None));
    }

    #[test]
    fn close_hidden_runs_as_that_user() {
        let (f, _) = flows();
        let id = add_existing(&f, "Kantor", "claude-kantor");
        f.p.running.lock().unwrap().push(InstanceStatus { owner_sid: "SID-claude-kantor".into(), root_pid: 9, bytes: GB });
        f.close(&id).unwrap();
        assert_eq!(f.p.calls(), vec!["close 9 claude-kantor"]);
    }

    #[test]
    fn rename_trims_and_rejects_blank() {
        let (f, _) = flows();
        let id = add_existing(&f, "Kantor", "claude-kantor");
        f.rename(&id, "  Kantor Pusat ").unwrap();
        assert!(f.accounts().unwrap().iter().any(|a| a.label == "Kantor Pusat"));
        assert_eq!(f.rename(&id, " ").unwrap_err().code, ErrorCode::Invalid);
    }

    #[test]
    fn remove_main_is_rejected() {
        let (f, _) = flows();
        let id = main_id(&f);
        assert_eq!(f.remove(&id).unwrap_err().code, ErrorCode::Invalid);
    }

    #[test]
    fn remove_order() {
        let (f, _) = flows();
        let id = add_existing(&f, "Kantor", "claude-kantor");
        f.p.running.lock().unwrap().push(InstanceStatus { owner_sid: "SID-claude-kantor".into(), root_pid: 9, bytes: GB });
        f.remove(&id).unwrap();
        // the ACE is revoked while the account name still resolves
        assert_eq!(f.p.calls(), vec!["close 9 claude-kantor", "revoke claude-kantor", "delete_user claude-kantor", "vault_delete claude-kantor"]);
        assert!(f.accounts().unwrap().iter().all(|a| a.id != id));
    }

    #[test]
    fn remove_uac_cancel_keeps_account() {
        let (f, _) = flows();
        let id = add_existing(&f, "Kantor", "claude-kantor");
        f.p.fail.lock().unwrap().insert("delete_user", err(ErrorCode::UacCancelled));
        assert_eq!(f.remove(&id).unwrap_err().code, ErrorCode::UacCancelled);
        assert!(f.accounts().unwrap().iter().any(|a| a.id == id));
        assert!(f.p.vault.lock().unwrap().contains_key("claude-kantor"));
        assert_eq!(f.p.calls(), vec!["revoke claude-kantor", "grant claude-kantor"]); // access restored
    }

    #[test]
    fn repair_logon_failure_resets_password() {
        let (f, _) = flows();
        let id = add_existing(&f, "Kantor", "claude-kantor");
        f.p.fail_once.lock().unwrap().insert("bootstrap", err(ErrorCode::LogonFailed));
        let r = f.repair(&id).unwrap();
        assert!(r.password_reset);
        assert_eq!(f.p.calls(), vec!["grant claude-kantor", "set_password claude-kantor", "vault_put claude-kantor", "bootstrap claude-kantor"]);
        assert_eq!(f.list().unwrap().iter().find(|v| v.account.id == id).unwrap().state, AccountState::Stopped);
    }

    #[test]
    fn repair_rebootstraps_broken_junction() {
        let (f, _) = flows();
        let id = add_existing(&f, "Kantor", "claude-kantor");
        f.p.junction_broken.lock().unwrap().insert("claude-kantor".into());
        let r = f.repair(&id).unwrap();
        assert!(!r.password_reset);
        assert_eq!(f.p.calls(), vec!["grant claude-kantor", "bootstrap claude-kantor"]);
        assert!(f.p.junction_ok("claude-kantor"));
    }

    #[test]
    fn open_timeout_marks_problem() {
        let (f, _) = flows();
        let id = add_existing(&f, "Kantor", "claude-kantor");
        assert_eq!(f.open(&id, false).unwrap(), OpenOutcome::Launched);
        assert_eq!(f.list().unwrap().iter().find(|v| v.account.id == id).unwrap().state, AccountState::Opening);
        f.opening.lock().unwrap().insert(id.clone(), Instant::now() - OPEN_TIMEOUT - Duration::from_secs(1));
        assert!(f.has_opening());
        assert_eq!(f.list().unwrap().iter().find(|v| v.account.id == id).unwrap().state, AccountState::Problem);
        assert!(!f.has_opening());
    }

    #[test]
    fn import_closes_a_running_account_first() {
        let (f, _) = flows();
        let id = add_existing(&f, "Kantor", "claude-kantor");
        f.p.running.lock().unwrap().push(InstanceStatus { owner_sid: "SID-claude-kantor".into(), root_pid: 9, bytes: GB });
        f.import_history(&id).unwrap();
        assert_eq!(f.p.calls(), vec!["close 9 claude-kantor", "import_sessions claude-kantor"]);
    }

    #[test]
    fn import_of_a_stopped_account_goes_straight_to_the_admin_step() {
        let (f, _) = flows();
        let id = add_existing(&f, "Kantor", "claude-kantor");
        f.import_history(&id).unwrap();
        assert_eq!(f.p.calls(), vec!["import_sessions claude-kantor"]);
    }

    #[test]
    fn import_into_main_is_rejected() {
        let (f, _) = flows();
        let id = main_id(&f);
        assert_eq!(f.import_history(&id).unwrap_err().code, ErrorCode::Invalid);
        assert!(f.p.calls().is_empty());
    }

    #[test]
    fn view_serializes_for_ui() {
        let (f, _) = flows();
        let v = serde_json::to_value(&f.list().unwrap()[0]).unwrap();
        assert_eq!(v["state"], "Stopped");
        assert_eq!(v["account"]["kind"], "main");
        assert!(v["bytes"].is_null());
        assert_eq!(serde_json::to_value(OpenOutcome::NeedsLowRamConfirm).unwrap(), "NeedsLowRamConfirm");
    }
}
