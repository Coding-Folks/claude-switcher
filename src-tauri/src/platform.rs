//! The OS operations the account flows need, as a trait (real: WinPlatform; tests: a fake).

use crate::claude_pkg::ClaudePkg;
use crate::elevated::ElevatedOp;
use crate::error::{AppError, AppResult, ErrorCode};
use crate::launcher::BootstrapReport;
use crate::monitor::Snapshot;
use crate::vault::Zeroizing;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub trait Platform: Send + Sync {
    fn user_exists(&self, name: &str) -> bool;
    fn new_password(&self) -> Zeroizing<String>;
    fn vault_put(&self, user: &str, secret: &Zeroizing<String>) -> AppResult<()>;
    fn vault_get(&self, user: &str) -> AppResult<Zeroizing<String>>;
    fn vault_delete(&self, user: &str) -> AppResult<()>;
    /// Runs one admin operation through UAC. `DeleteUser` also removes the profile folder.
    fn elevated(&self, op: ElevatedOp, secret: Option<&Zeroizing<String>>) -> AppResult<()>;
    fn grant(&self, user: &str) -> AppResult<()>;
    fn revoke(&self, user: &str) -> AppResult<()>;
    fn bootstrap(&self, user: &str, pw: &Zeroizing<String>) -> AppResult<BootstrapReport>;
    fn junction_ok(&self, user: &str) -> bool;
    /// Starts Claude, or brings its existing window forward.
    fn launch_main(&self) -> AppResult<()>;
    fn launch_as(&self, user: &str, pw: &Zeroizing<String>) -> AppResult<()>;
    /// Ends the instance rooted at `root_pid`; `as_user` is the hidden user that owns it.
    fn close(&self, root_pid: u32, as_user: Option<&str>) -> AppResult<()>;
    fn snapshot(&self) -> Snapshot;
    fn sid_of(&self, user: &str) -> Option<String>;
    fn main_sid(&self) -> Option<String>;

    // Same-user slots (slot.rs). None of these needs admin.
    /// Main's own Claude data folder: %LOCALAPPDATA%Claude-Dataclaude_<family>.
    fn main_data_dir(&self) -> PathBuf;
    fn slots_dir(&self) -> PathBuf;
    fn slot_data_dir(&self, data_root: &Path) -> PathBuf;
    fn slot_prepare(&self, data_root: &Path) -> AppResult<()>;
    fn slot_ready(&self, data_root: &Path) -> bool;
    /// Deletes the whole slot: the parent of `data_root`, i.e. `<slots>\<id>`.
    fn slot_remove(&self, data_root: &Path) -> AppResult<()>;
    fn launch_slot(&self, data_root: &Path) -> AppResult<()>;
    fn slot_move_label(&self, data_root: &Path) -> Option<String>;
    fn slot_import(&self, data_root: &Path) -> AppResult<usize>;
}

pub struct WinPlatform {
    pkg: ClaudePkg,
    main_claude_dir: PathBuf,
    main_sid: Option<String>,
    sids: Mutex<HashMap<String, Option<String>>>,
    /// The real %LOCALAPPDATA% (each slot gets its own one).
    real_local: PathBuf,
}

impl WinPlatform {
    pub fn new() -> AppResult<Self> {
        let pkg = crate::claude_pkg::detect()?;
        let home = std::env::var_os("USERPROFILE").map(PathBuf::from).ok_or_else(|| AppError::io("USERPROFILE not set"))?;
        let main_sid = std::env::var("USERNAME").ok().and_then(|u| crate::winuser::sid_of(&u));
        let real_local = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).ok_or_else(|| AppError::io("LOCALAPPDATA not set"))?;
        Ok(Self { pkg, main_claude_dir: home.join(".claude"), main_sid, sids: Mutex::new(HashMap::new()), real_local })
    }

    fn real_windowsapps(&self) -> PathBuf {
        self.real_local.join(r"Microsoft\WindowsApps")
    }

    pub fn install_dir(&self) -> PathBuf {
        self.pkg.install_dir.clone()
    }

    fn profile(&self, user: &str) -> AppResult<PathBuf> {
        crate::winuser::profile_path(user).ok_or_else(|| AppError::new(ErrorCode::JunctionBroken, "err.junction_broken"))
    }
}

impl Platform for WinPlatform {
    fn user_exists(&self, name: &str) -> bool {
        crate::winuser::exists(name)
    }
    fn new_password(&self) -> Zeroizing<String> {
        crate::vault::generate_password()
    }
    fn vault_put(&self, user: &str, secret: &Zeroizing<String>) -> AppResult<()> {
        crate::vault::put(user, secret)
    }
    fn vault_get(&self, user: &str) -> AppResult<Zeroizing<String>> {
        crate::vault::get(user)
    }
    fn vault_delete(&self, user: &str) -> AppResult<()> {
        crate::vault::delete(user)
    }
    fn elevated(&self, op: ElevatedOp, secret: Option<&Zeroizing<String>>) -> AppResult<()> {
        crate::elevated::request(op, secret)
    }
    fn grant(&self, user: &str) -> AppResult<()> {
        std::fs::create_dir_all(&self.main_claude_dir)?;
        crate::share::grant_shared_access(&self.main_claude_dir, user)
    }
    fn revoke(&self, user: &str) -> AppResult<()> {
        crate::share::revoke_shared_access(&self.main_claude_dir, user)
    }
    fn bootstrap(&self, user: &str, pw: &Zeroizing<String>) -> AppResult<BootstrapReport> {
        crate::launcher::bootstrap(user, pw, &self.pkg, &self.main_claude_dir)
    }
    fn junction_ok(&self, user: &str) -> bool {
        self.profile(user).map(|p| crate::share::is_junction_to(&p.join(".claude"), &self.main_claude_dir)).unwrap_or(false)
    }
    fn launch_main(&self) -> AppResult<()> {
        crate::launcher::launch_main()
    }
    fn launch_as(&self, user: &str, pw: &Zeroizing<String>) -> AppResult<()> {
        crate::launcher::launch_as(user, pw).map(|_| ())
    }
    fn close(&self, root_pid: u32, as_user: Option<&str>) -> AppResult<()> {
        match as_user {
            None => crate::launcher::close(root_pid, None),
            Some(user) => {
                let pw = crate::vault::get(user)?;
                crate::launcher::close(root_pid, Some((user, &pw)))
            }
        }
    }
    fn snapshot(&self) -> Snapshot {
        crate::monitor::snapshot(&self.pkg.install_dir).unwrap_or_default()
    }
    fn sid_of(&self, user: &str) -> Option<String> {
        let mut cache = self.sids.lock().unwrap();
        if let Some(Some(s)) = cache.get(user) {
            return Some(s.clone());
        }
        let s = crate::winuser::sid_of(user);
        cache.insert(user.to_string(), s.clone());
        s
    }
    fn main_sid(&self) -> Option<String> {
        self.main_sid.clone()
    }
    fn main_data_dir(&self) -> PathBuf {
        crate::slot::data_dir(&self.real_local, &self.pkg.family)
    }
    fn slots_dir(&self) -> PathBuf {
        self.real_local.join(r"ClaudeSwitcher\slots")
    }
    fn slot_data_dir(&self, data_root: &Path) -> PathBuf {
        crate::slot::data_dir(data_root, &self.pkg.family)
    }
    fn slot_prepare(&self, data_root: &Path) -> AppResult<()> {
        crate::slot::prepare(data_root, &self.pkg.family, &self.real_windowsapps())
    }
    fn slot_ready(&self, data_root: &Path) -> bool {
        crate::slot::is_ready(data_root, &self.pkg.family, &self.real_windowsapps())
    }
    fn slot_remove(&self, data_root: &Path) -> AppResult<()> {
        let slot_dir = data_root.parent().ok_or_else(|| AppError::io("slot has no parent folder"))?;
        crate::slot::remove(slot_dir)
    }
    fn launch_slot(&self, data_root: &Path) -> AppResult<()> {
        crate::slot::launch(data_root, &self.real_windowsapps().join("claude-desktop.exe"))
    }
    fn slot_move_label(&self, data_root: &Path) -> Option<String> {
        crate::slot::move_label(data_root)
    }
    fn slot_import(&self, data_root: &Path) -> AppResult<usize> {
        crate::slot::import_sessions(&self.real_local, data_root, &self.pkg.family)
    }
}
