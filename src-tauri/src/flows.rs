//! Account flows (add, open, close, rename, remove, repair, list) over the `Platform` trait,
//! so they can be tested without touching Windows.

use crate::elevated::ElevatedOp;
use crate::error::{AppError, AppResult, ErrorCode};
pub use crate::platform::Platform;
use crate::store::{Account, AccountKind, LoadOutcome, Store};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
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
    /// A same-user slot's folder was missing; it was created again and Claude started (needs a new login).
    SlotRecreated,
}

/// Where "Tambah akun" runs the new account.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AddMode {
    /// This Windows user, own LOCALAPPDATA slot: no UAC.
    SameUser,
    /// A hidden Windows user of its own: full isolation, one UAC prompt.
    WindowsUser,
}

const HEALTHY_MOVE_LABEL: &str = "already_moved";

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
    /// How long close_and_wait waits for Claude to go (10 s; shorter in tests).
    close_wait: Mutex<Duration>,
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
            close_wait: Mutex::new(Duration::from_secs(10)),
            recovered_backup: Mutex::new(None),
        }
    }

    pub fn set_close_wait(&self, d: Duration) {
        *self.close_wait.lock().unwrap() = d;
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
            (AccountKind::Main | AccountKind::SameUser, _) => self.p.main_sid(),
            (AccountKind::WindowsUser, Some(u)) => self.p.sid_of(u),
            _ => None,
        }
    }

    /// The Claude data folders of every same-user slot (all share the main user's SID).
    fn slot_dirs(&self, accounts: &[Account]) -> Vec<PathBuf> {
        accounts.iter().filter_map(|a| a.data_root.as_deref()).map(|r| self.p.slot_data_dir(r)).collect()
    }

    /// The running instance of `a`: by SID for hidden users; for this user's accounts by data folder.
    /// Main is the instance of the main SID that no slot claims (also when its folder is unreadable).
    fn running_in(&self, a: &Account, snap: &crate::monitor::Snapshot, slot_dirs: &[PathBuf]) -> Option<(u32, u64)> {
        let sid = self.sid(a)?;
        let mine = snap.instances.iter().filter(|i| i.owner_sid == sid);
        let found = match a.kind {
            AccountKind::WindowsUser => mine.into_iter().next(),
            AccountKind::SameUser => {
                let want = self.p.slot_data_dir(a.data_root.as_deref()?);
                mine.into_iter().find(|i| i.data_dir.as_deref().map(|d| crate::monitor::same_dir(d, &want)).unwrap_or(false))
            }
            AccountKind::Main => {
                // Prefer the instance on Main's real data folder. An instance with no readable folder
                // may be a slot still starting, so it only counts when nothing matches exactly.
                let real = self.p.main_data_dir();
                let mine: Vec<_> = mine.collect();
                mine.iter()
                    .find(|i| i.data_dir.as_deref().map(|d| crate::monitor::same_dir(d, &real)).unwrap_or(false))
                    .or_else(|| {
                        mine.iter().find(|i| {
                            !i.data_dir.as_deref().map(|d| slot_dirs.iter().any(|s| crate::monitor::same_dir(d, s))).unwrap_or(false)
                        })
                    })
                    .copied()
            }
        };
        found.map(|i| (i.root_pid, i.bytes))
    }

    fn running_root(&self, a: &Account, snap: &crate::monitor::Snapshot) -> Option<(u32, u64)> {
        let slots = self.slot_dirs(&self.accounts().unwrap_or_default());
        self.running_in(a, snap, &slots)
    }

    fn user(a: &Account) -> AppResult<&str> {
        a.windows_user.as_deref().ok_or_else(|| invalid("account has no windows user"))
    }

    fn data_root(a: &Account) -> AppResult<&Path> {
        a.data_root.as_deref().ok_or_else(|| invalid("account has no slot folder"))
    }

    /// Closes `a` if it runs and waits (up to 10 s) until it is gone, so its files can be written.
    fn close_and_wait(&self, a: &Account) -> AppResult<()> {
        if let Some((pid, _)) = self.running_root(a, &self.p.snapshot()) {
            self.p.close(pid, a.windows_user.as_deref())?;
            let deadline = Instant::now() + *self.close_wait.lock().unwrap();
            while self.running_root(a, &self.p.snapshot()).is_some() {
                if Instant::now() >= deadline {
                    // Writing or deleting its files now would hit locked files: stop and say so.
                    return Err(AppError::new(ErrorCode::Io, "err.still_running"));
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        }
        Ok(())
    }

    pub fn list(&self) -> AppResult<Vec<AccountView>> {
        let snap = self.p.snapshot();
        let accounts = self.accounts()?;
        let slots = self.slot_dirs(&accounts);
        let mut opening = self.opening.lock().unwrap();
        let mut problems = self.problems.lock().unwrap();
        Ok(accounts
            .into_iter()
            .map(|a| {
                let run = self.running_in(&a, &snap, &slots);
                // A slot Claude did not take as is (an update changed its data-folder logic) is a problem.
                if run.is_some() && a.kind == AccountKind::SameUser {
                    let label = a.data_root.as_deref().and_then(|r| self.p.slot_move_label(r));
                    if label.map(|l| !l.starts_with(HEALTHY_MOVE_LABEL)).unwrap_or(false) {
                        problems.insert(a.id.clone());
                    }
                }
                if run.is_some() {
                    opening.remove(&a.id);
                } else if opening.get(&a.id).map(|t| t.elapsed() >= OPEN_TIMEOUT).unwrap_or(false) {
                    // Claude did not appear within 30 s (spec §4): report it instead of going quiet.
                    opening.remove(&a.id);
                    problems.insert(a.id.clone());
                }
                let state = if problems.contains(&a.id) && run.is_some() && a.kind == AccountKind::SameUser {
                    AccountState::Problem
                } else if run.is_some() {
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

    pub fn add(&self, label: &str, mode: AddMode, progress: &dyn Fn(&str)) -> AppResult<Account> {
        let label = label.trim();
        if label.is_empty() {
            return Err(invalid("empty label"));
        }
        if mode == AddMode::SameUser {
            return self.add_slot(label, progress);
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
            // Keep the password while the Windows user still exists (rollback UAC refused): forgetting it
            // would leave an account nobody can sign in to.
            match self.p.elevated(ElevatedOp::DeleteUser { name: user.clone() }, None) {
                Ok(()) => {
                    let _ = self.p.vault_delete(&user);
                }
                Err(e) => crate::applog::error("rollback left a Windows user", &[("user", &user), ("code", &format!("{:?}", e.code))]),
            }
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
            data_root: None,
        };
        let saved = {
            let _w = self.write.lock().unwrap();
            self.accounts().and_then(|mut list| {
                list.push(account.clone());
                self.store.save(&list)
            })
        };
        if let Err(e) = saved {
            let _ = self.p.revoke(&user);
            undo_user(); // nothing recorded it, so do not leave the Windows user behind
            return Err(e);
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

    /// Same-user account: a slot folder (named by the account id), saved, then launched. No UAC, no vault.
    fn add_slot(&self, label: &str, progress: &dyn Fn(&str)) -> AppResult<Account> {
        let id = uuid::Uuid::new_v4().to_string();
        let root = crate::slot::root_for(&self.p.slots_dir(), &id);
        progress("prepare_slot");
        if let Err(e) = self.p.slot_prepare(&root) {
            let _ = self.p.slot_remove(&root); // prepare may have made part of the folder
            return Err(e);
        }
        let account = Account {
            id,
            label: label.to_string(),
            kind: AccountKind::SameUser,
            windows_user: None,
            created_at: chrono::Utc::now().to_rfc3339(),
            data_root: Some(root.clone()),
        };
        let saved = {
            let _w = self.write.lock().unwrap();
            self.accounts().and_then(|mut list| {
                list.push(account.clone());
                self.store.save(&list)
            })
        };
        if let Err(e) = saved {
            let _ = self.p.slot_remove(&root); // nothing recorded, so leave no folder behind
            return Err(e);
        }
        progress("launch");
        match self.p.launch_slot(&root) {
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
        let mut recreated = false;
        let result = match a.kind {
            AccountKind::Main => self.p.launch_main(),
            AccountKind::SameUser => {
                let root = Self::data_root(&a)?;
                if !running && !self.p.slot_ready(root) {
                    // Only a missing data folder means the login is gone; a missing junction is fixed quietly.
                    recreated = !self.p.slot_has_data(root);
                    self.p.slot_prepare(root)?;
                }
                self.p.launch_slot(root)
            }
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
        Ok(if recreated { OpenOutcome::SlotRecreated } else { OpenOutcome::Launched })
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
        if a.kind == AccountKind::SameUser {
            let root = Self::data_root(&a)?.to_path_buf();
            self.close_and_wait(&a)?;
            self.p.slot_remove(&root)?;
            let _w = self.write.lock().unwrap();
            let list: Vec<Account> = self.accounts()?.into_iter().filter(|x| x.id != id).collect();
            self.store.save(&list)?;
            self.problems.lock().unwrap().remove(id);
            self.opening.lock().unwrap().remove(id);
            return Ok(());
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
        self.close_and_wait(&a)?;
        self.opening.lock().unwrap().remove(id);
        if a.kind == AccountKind::SameUser {
            return self.p.slot_import(Self::data_root(&a)?).map(|_| ()); // own files: no admin needed
        }
        let user = Self::user(&a)?.to_string();
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
        if a.kind == AccountKind::SameUser {
            let root = Self::data_root(&a)?;
            // Claude did not take the slot's folder as is (an update changed how it picks its data folder):
            // recreating the folder cannot help, so say so and point to the separate-user mode (spec §7).
            if self.p.slot_move_label(root).map(|l| !l.starts_with(HEALTHY_MOVE_LABEL)).unwrap_or(false) {
                return Err(AppError::new(ErrorCode::Invalid, "err.slot_unsupported"));
            }
            self.p.slot_prepare(root)?; // recreates the data folder and the WindowsApps junction
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
    use std::path::{Path, PathBuf};
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
        slot_missing: Mutex<HashSet<String>>,
        slot_no_data: Mutex<HashSet<String>>,
        stuck: Mutex<bool>,
        move_labels: Mutex<HashMap<String, String>>,
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
            if !*self.stuck.lock().unwrap() { self.running.lock().unwrap().retain(|i| i.root_pid != root_pid); }
            Ok(())
        }
        fn snapshot(&self) -> Snapshot {
            Snapshot { instances: self.running.lock().unwrap().clone(), free_ram: *self.free.lock().unwrap(), total_ram: 24 * GB }
        }
        fn sid_of(&self, user: &str) -> Option<String> { Some(format!("SID-{user}")) }
        fn main_sid(&self) -> Option<String> { Some("SID-main".into()) }
        fn main_data_dir(&self) -> PathBuf { PathBuf::from(r"C:\Users\me\AppData\Local\Claude-Data\claude_p") }
        fn slots_dir(&self) -> PathBuf { PathBuf::from(r"C:\slots") }
        fn slot_data_dir(&self, r: &Path) -> PathBuf { r.join(r"Claude-Data\claude_p") }
        fn slot_prepare(&self, r: &Path) -> AppResult<()> {
            self.check("slot_prepare")?; self.log(format!("slot_prepare {}", r.display()));
            self.slot_missing.lock().unwrap().remove(&r.display().to_string()); Ok(())
        }
        fn slot_ready(&self, r: &Path) -> bool { !self.slot_missing.lock().unwrap().contains(&r.display().to_string()) }
        fn slot_has_data(&self, r: &Path) -> bool { !self.slot_no_data.lock().unwrap().contains(&r.display().to_string()) }
        fn slot_remove(&self, r: &Path) -> AppResult<()> { self.check("slot_remove")?; self.log(format!("slot_remove {}", r.display())); Ok(()) }
        fn launch_slot(&self, r: &Path) -> AppResult<()> { self.check("launch_slot")?; self.log(format!("launch_slot {}", r.display())); Ok(()) }
        fn slot_move_label(&self, r: &Path) -> Option<String> {
            Some(self.move_labels.lock().unwrap().get(&r.display().to_string()).cloned().unwrap_or_else(|| "already_moved_package_emptied".into()))
        }
        fn slot_import(&self, r: &Path) -> AppResult<usize> { self.check("slot_import")?; self.log(format!("slot_import {}", r.display())); Ok(1) }
    }

    fn flows() -> (Flows<Fake>, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("cs-flows-{}", uuid::Uuid::new_v4()));
        let path = dir.join("accounts.json");
        (Flows::new(Fake::new(), Store::new(&path)), path)
    }
    fn add_existing(f: &Flows<Fake>, label: &str, user: &str) -> String {
        let mut list = f.accounts().unwrap();
        let a = Account { id: format!("id-{user}"), label: label.into(), kind: AccountKind::WindowsUser, windows_user: Some(user.into()), created_at: "2026-11-12T00:00:00Z".into(), data_root: None };
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

    /// A same-user account saved directly (no flow calls); returns (id, data_root as shown in calls).
    fn add_slot(f: &Flows<Fake>, label: &str) -> (String, String) {
        let id = format!("slot-{label}");
        let root = PathBuf::from(format!(r"C:\slots\{id}\Local"));
        let mut list = f.accounts().unwrap();
        list.push(Account { id: id.clone(), label: label.into(), kind: AccountKind::SameUser, windows_user: None, created_at: "x".into(), data_root: Some(root.clone()) });
        f.store.save(&list).unwrap();
        (id, root.display().to_string())
    }
    fn slot_instance(root: &str, pid: u32, gb: u64) -> InstanceStatus {
        InstanceStatus { owner_sid: "SID-main".into(), root_pid: pid, bytes: gb * GB, data_dir: Some(PathBuf::from(format!(r"{root}\Claude-Data\claude_p"))) }
    }
    fn state_of(f: &Flows<Fake>, id: &str) -> (AccountState, Option<u64>) {
        let v = f.list().unwrap().into_iter().find(|v| v.account.id == id).unwrap();
        (v.state, v.bytes)
    }

    #[test]
    fn add_same_user_prepares_saves_launches_without_uac() {
        let (f, _) = flows();
        let steps = Mutex::new(vec![]);
        let a = f.add("Slot A", AddMode::SameUser, &|s| steps.lock().unwrap().push(s.to_string())).unwrap();
        let root = format!(r"C:\slots\{}\Local", a.id);
        assert_eq!(f.p.calls(), vec![format!("slot_prepare {root}"), format!("launch_slot {root}")]);
        assert_eq!(*steps.lock().unwrap(), vec!["prepare_slot", "launch"]);
        assert_eq!((a.kind, a.windows_user.clone(), a.data_root.clone()), (AccountKind::SameUser, None, Some(PathBuf::from(&root))));
        assert!(f.accounts().unwrap().iter().any(|x| x.id == a.id));
    }

    #[test]
    fn add_same_user_prepare_failure_saves_nothing() {
        let (f, _) = flows();
        f.p.fail.lock().unwrap().insert("slot_prepare", AppError::new(ErrorCode::Io, "err.slot_prepare"));
        assert_eq!(f.add("Slot A", AddMode::SameUser, &|_| {}).unwrap_err().message_id, "err.slot_prepare");
        assert_eq!(f.accounts().unwrap().len(), 1);
        assert!(!f.p.calls().iter().any(|c| c.starts_with("launch_slot")));
        // prepare may have made part of the folder before failing: it is cleaned up
        assert!(f.p.calls().iter().any(|c| c.starts_with(r"slot_remove C:\slots\")), "{:?}", f.p.calls());
    }

    #[test]
    fn main_prefers_its_real_data_dir_over_an_unidentified_instance() {
        let (f, _) = flows();
        let main = main_id(&f);
        {
            let mut r = f.p.running.lock().unwrap();
            // a slot that is still starting has no children yet, so no data_dir; listed first on purpose
            r.push(InstanceStatus { owner_sid: "SID-main".into(), root_pid: 3, bytes: GB, data_dir: None });
            r.push(InstanceStatus { owner_sid: "SID-main".into(), root_pid: 1, bytes: 3 * GB, data_dir: Some(PathBuf::from(r"C:\Users\me\AppData\Local\Claude-Data\claude_p")) });
        }
        assert_eq!(state_of(&f, &main), (AccountState::Running, Some(3 * GB)));
        f.close(&main).unwrap();
        assert_eq!(f.p.calls(), vec!["close 1 main"]);
    }

    #[test]
    fn add_same_user_rolls_back_folder_when_save_fails() {
        let (f, path) = flows();
        f.accounts().unwrap(); // first run writes accounts.json
        std::fs::create_dir_all(path.with_file_name("accounts.json.tmp")).unwrap(); // the atomic write can no longer happen
        assert!(f.add("Slot A", AddMode::SameUser, &|_| {}).is_err());
        let calls = f.p.calls();
        assert!(calls[0].starts_with("slot_prepare "), "{calls:?}");
        assert!(calls.last().unwrap().starts_with("slot_remove "), "{calls:?}");
        assert_eq!(calls[0].replace("slot_prepare", ""), calls.last().unwrap().replace("slot_remove", ""));
    }

    #[test]
    fn open_same_user_launches_slot() {
        let (f, _) = flows();
        let (id, root) = add_slot(&f, "A");
        assert_eq!(f.open(&id, false).unwrap(), OpenOutcome::Launched);
        assert_eq!(f.p.calls(), vec![format!("launch_slot {root}")]);
    }

    #[test]
    fn open_same_user_recreates_missing_slot() {
        let (f, _) = flows();
        let (id, root) = add_slot(&f, "A");
        f.p.slot_missing.lock().unwrap().insert(root.clone());
        f.p.slot_no_data.lock().unwrap().insert(root.clone());
        assert_eq!(f.open(&id, false).unwrap(), OpenOutcome::SlotRecreated);
        assert_eq!(f.p.calls(), vec![format!("slot_prepare {root}"), format!("launch_slot {root}")]);
    }

    #[test]
    fn open_with_only_the_junction_missing_repairs_it_quietly() {
        let (f, _) = flows();
        let (id, root) = add_slot(&f, "A");
        f.p.slot_missing.lock().unwrap().insert(root.clone()); // not ready, but the login data is still there
        assert_eq!(f.open(&id, false).unwrap(), OpenOutcome::Launched);
        assert_eq!(f.p.calls(), vec![format!("slot_prepare {root}"), format!("launch_slot {root}")]);
    }

    #[test]
    fn remove_stops_when_claude_will_not_close() {
        let (f, _) = flows();
        let (id, root) = add_slot(&f, "A");
        f.set_close_wait(Duration::from_millis(20));
        *f.p.stuck.lock().unwrap() = true;
        f.p.running.lock().unwrap().push(slot_instance(&root, 2, 1));
        assert!(f.remove(&id).is_err());
        assert!(!f.p.calls().iter().any(|c| c.starts_with("slot_remove")), "{:?}", f.p.calls());
        assert!(f.accounts().unwrap().iter().any(|a| a.id == id));
    }

    #[test]
    fn open_running_slot_focuses() {
        let (f, _) = flows();
        let (id, root) = add_slot(&f, "A");
        f.p.running.lock().unwrap().push(slot_instance(&root, 2, 1));
        assert_eq!(f.open(&id, false).unwrap(), OpenOutcome::Focused);
    }

    #[test]
    fn main_is_the_instance_no_slot_claims() {
        let (f, _) = flows();
        let (slot, root) = add_slot(&f, "A");
        let main = main_id(&f);
        {
            let mut r = f.p.running.lock().unwrap();
            r.push(slot_instance(&root, 2, 2)); // the slot, listed first on purpose
            r.push(InstanceStatus { owner_sid: "SID-main".into(), root_pid: 1, bytes: 3 * GB, data_dir: Some(PathBuf::from(r"C:\Users\me\AppData\Local\Claude-Data\claude_p")) });
        }
        assert_eq!(state_of(&f, &main), (AccountState::Running, Some(3 * GB)));
        assert_eq!(state_of(&f, &slot), (AccountState::Running, Some(2 * GB)));
    }

    #[test]
    fn main_with_unreadable_data_dir_still_counts() {
        let (f, _) = flows();
        let (_, root) = add_slot(&f, "A");
        let main = main_id(&f);
        f.p.running.lock().unwrap().push(slot_instance(&root, 2, 2));
        f.p.running.lock().unwrap().push(InstanceStatus { owner_sid: "SID-main".into(), root_pid: 3, bytes: GB, data_dir: None });
        assert_eq!(state_of(&f, &main), (AccountState::Running, Some(GB)));
    }

    #[test]
    fn a_running_slot_alone_does_not_make_main_running() {
        let (f, _) = flows();
        let (_, root) = add_slot(&f, "A");
        let main = main_id(&f);
        f.p.running.lock().unwrap().push(slot_instance(&root, 2, 2));
        assert_eq!(state_of(&f, &main), (AccountState::Stopped, None));
    }

    #[test]
    fn close_same_user_kills_its_root_as_main() {
        let (f, _) = flows();
        let (id, root) = add_slot(&f, "A");
        f.p.running.lock().unwrap().push(slot_instance(&root, 2, 1));
        f.close(&id).unwrap();
        assert_eq!(f.p.calls(), vec!["close 2 main"]);
    }

    #[test]
    fn remove_same_user_closes_then_deletes_folder() {
        let (f, _) = flows();
        let (id, root) = add_slot(&f, "A");
        f.p.running.lock().unwrap().push(slot_instance(&root, 2, 1));
        f.remove(&id).unwrap();
        assert_eq!(f.p.calls(), vec!["close 2 main".to_string(), format!("slot_remove {root}")]);
        assert!(f.accounts().unwrap().iter().all(|a| a.id != id));
    }

    #[test]
    fn import_same_user_closes_then_copies_without_elevation() {
        let (f, _) = flows();
        let (id, root) = add_slot(&f, "A");
        f.p.running.lock().unwrap().push(slot_instance(&root, 2, 1));
        f.import_history(&id).unwrap();
        assert_eq!(f.p.calls(), vec!["close 2 main".to_string(), format!("slot_import {root}")]);
    }

    #[test]
    fn repair_same_user_reprepares_slot() {
        let (f, _) = flows();
        let (id, root) = add_slot(&f, "A");
        f.problems.lock().unwrap().insert(id.clone());
        f.repair(&id).unwrap();
        assert_eq!(f.p.calls(), vec![format!("slot_prepare {root}")]);
        assert_eq!(state_of(&f, &id).0, AccountState::Stopped);
    }

    #[test]
    fn unexpected_move_label_marks_problem() {
        let (f, _) = flows();
        let (id, root) = add_slot(&f, "A");
        f.p.running.lock().unwrap().push(slot_instance(&root, 2, 1));
        f.p.move_labels.lock().unwrap().insert(root, "stay_never_left".into());
        assert_eq!(state_of(&f, &id).0, AccountState::Problem);
    }

    #[test]
    fn repair_cannot_fix_an_unsupported_slot_and_says_why() {
        let (f, _) = flows();
        let (id, root) = add_slot(&f, "A");
        f.p.running.lock().unwrap().push(slot_instance(&root, 2, 1));
        f.p.move_labels.lock().unwrap().insert(root, "stay_never_left".into());
        assert_eq!(state_of(&f, &id).0, AccountState::Problem);
        let e = f.repair(&id).unwrap_err();
        assert_eq!(e.message_id, "err.slot_unsupported");
        assert_eq!(state_of(&f, &id).0, AccountState::Problem); // not shown as fixed
    }

    #[test]
    fn add_happy_path_order() {
        let (f, _) = flows();
        let steps = Mutex::new(vec![]);
        let a = f.add("Kantor", AddMode::WindowsUser, &|s| steps.lock().unwrap().push(s.to_string())).unwrap();
        assert_eq!(a.windows_user.as_deref(), Some("claude-kantor"));
        assert_eq!(f.p.calls(), vec!["vault_put claude-kantor", "create_user claude-kantor", "grant claude-kantor", "bootstrap claude-kantor", "launch_as claude-kantor"]);
        assert_eq!(*steps.lock().unwrap(), vec!["vault", "create_user", "grant", "bootstrap", "launch"]);
        assert!(f.accounts().unwrap().iter().any(|x| x.label == "Kantor"));
    }

    #[test]
    fn add_picks_free_name_when_user_exists() {
        let (f, _) = flows();
        f.p.users.lock().unwrap().insert("claude-kantor".into());
        assert_eq!(f.add("Kantor", AddMode::WindowsUser, &|_| {}).unwrap().windows_user.as_deref(), Some("claude-kantor-2"));
    }

    #[test]
    fn add_rejects_blank_label() {
        let (f, _) = flows();
        assert_eq!(f.add("   ", AddMode::WindowsUser, &|_| {}).unwrap_err().code, ErrorCode::Invalid);
        assert!(f.p.calls().is_empty());
    }

    #[test]
    fn add_rolls_back_when_bootstrap_fails() {
        let (f, _) = flows();
        f.p.fail.lock().unwrap().insert("bootstrap", err(ErrorCode::Io));
        assert!(f.add("Kantor", AddMode::WindowsUser, &|_| {}).is_err());
        assert_eq!(f.p.calls(), vec!["vault_put claude-kantor", "create_user claude-kantor", "grant claude-kantor", "revoke claude-kantor", "delete_user claude-kantor", "vault_delete claude-kantor"]);
        assert_eq!(f.accounts().unwrap().len(), 1);
    }

    #[test]
    fn add_rolls_back_when_bootstrap_reports_failure() {
        let (f, _) = flows();
        *f.p.report.lock().unwrap() = Some(BootstrapReport { registered: false, junction_ok: false, alias_present: false });
        assert!(f.add("Kantor", AddMode::WindowsUser, &|_| {}).is_err());
        assert!(f.p.calls().contains(&"delete_user claude-kantor".to_string()));
        assert_eq!(f.accounts().unwrap().len(), 1);
    }

    #[test]
    fn add_uac_cancel_before_user_created() {
        let (f, _) = flows();
        f.p.fail.lock().unwrap().insert("create_user", err(ErrorCode::UacCancelled));
        assert_eq!(f.add("Kantor", AddMode::WindowsUser, &|_| {}).unwrap_err().code, ErrorCode::UacCancelled);
        assert_eq!(f.p.calls(), vec!["vault_put claude-kantor", "vault_delete claude-kantor"]);
    }

    #[test]
    fn rollback_keeps_the_password_when_the_user_could_not_be_deleted() {
        let (f, _) = flows();
        f.p.fail.lock().unwrap().insert("bootstrap", err(ErrorCode::Io));
        f.p.fail.lock().unwrap().insert("delete_user", err(ErrorCode::UacCancelled)); // rollback UAC refused
        assert!(f.add("Kantor", AddMode::WindowsUser, &|_| {}).is_err());
        // the Windows user still exists, so its password must not be thrown away
        assert!(f.p.vault.lock().unwrap().contains_key("claude-kantor"));
        assert!(!f.p.calls().contains(&"vault_delete claude-kantor".to_string()), "{:?}", f.p.calls());
    }

    #[test]
    fn add_undoes_the_user_when_saving_the_account_fails() {
        let (f, path) = flows();
        f.accounts().unwrap();
        std::fs::create_dir_all(path.with_file_name("accounts.json.tmp")).unwrap(); // save will fail
        assert!(f.add("Kantor", AddMode::WindowsUser, &|_| {}).is_err());
        let calls = f.p.calls();
        assert!(calls.contains(&"delete_user claude-kantor".to_string()), "{calls:?}");
        assert!(calls.contains(&"revoke claude-kantor".to_string()), "{calls:?}");
        assert!(!calls.iter().any(|c| c.starts_with("launch_as")), "{calls:?}");
    }

    #[test]
    fn open_running_focuses_by_relaunch() {
        let (f, _) = flows();
        let id = add_existing(&f, "Kantor", "claude-kantor");
        f.p.running.lock().unwrap().push(InstanceStatus { owner_sid: "SID-claude-kantor".into(), root_pid: 7, bytes: GB, data_dir: None });
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
        f.p.running.lock().unwrap().push(InstanceStatus { owner_sid: "SID-main".into(), root_pid: 1, bytes: 2 * GB, data_dir: None });
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
        f.p.running.lock().unwrap().push(InstanceStatus { owner_sid: "SID-claude-kantor".into(), root_pid: 9, bytes: GB, data_dir: None });
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
        f.p.running.lock().unwrap().push(InstanceStatus { owner_sid: "SID-claude-kantor".into(), root_pid: 9, bytes: GB, data_dir: None });
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
        f.p.running.lock().unwrap().push(InstanceStatus { owner_sid: "SID-claude-kantor".into(), root_pid: 9, bytes: GB, data_dir: None });
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
