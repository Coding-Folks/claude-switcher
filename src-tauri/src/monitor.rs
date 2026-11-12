//! Which Claude Desktop instances run, under which Windows account (by SID), and how much RAM each uses.
//!
//! A non-admin process cannot open another user's process token, so owners and working sets come from
//! WTSEnumerateProcessesEx (works without admin) and parents from a Toolhelp snapshot.

use crate::error::{AppError, AppResult};
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcInfo {
    pub pid: u32,
    pub parent: u32,
    /// Full image path when readable; just the image name for other users' processes.
    pub exe: PathBuf,
    pub owner_sid: String,
    pub working_set: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InstanceStatus {
    pub owner_sid: String,
    pub root_pid: u32,
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Snapshot {
    pub instances: Vec<InstanceStatus>,
    pub free_ram: u64,
    pub total_ram: u64,
}


fn is_desktop_claude(p: &ProcInfo, install_dir: &Path) -> bool {
    let name_ok = p.exe.file_name().map(|n| n.to_string_lossy().eq_ignore_ascii_case("claude.exe")).unwrap_or(false);
    if !name_ok {
        return false;
    }
    if p.exe.parent().map(|d| d.as_os_str().is_empty()).unwrap_or(true) {
        return true; // path unreadable (another user's process): the name is all we have
    }
    in_claude_package(&p.exe, install_dir)
}

/// `…\WindowsApps\Claude_<any version>_<arch>__<publisher>\app\Claude.exe`. Matching the package pattern
/// instead of the startup install folder keeps instances visible after Claude updates itself.
fn in_claude_package(exe: &Path, install_dir: &Path) -> bool {
    let publisher = install_dir
        .file_name()
        .and_then(|n| n.to_str())
        .and_then(|n| n.rsplit_once("__").map(|(_, p)| p.to_lowercase()))
        .unwrap_or_else(|| crate::claude_pkg::FALLBACK_FAMILY.rsplit_once('_').map(|(_, p)| p.to_lowercase()).unwrap_or_default());
    let parts: Vec<String> = exe.components().map(|c| c.as_os_str().to_string_lossy().to_lowercase()).collect();
    let Some(i) = parts.iter().position(|c| c == "windowsapps") else { return false };
    match &parts[i + 1..] {
        [pkg, app, file] => pkg.starts_with("claude_") && pkg.ends_with(&format!("__{publisher}")) && app == "app" && file == "claude.exe",
        _ => false,
    }
}

/// Roots are desktop Claude processes whose parent is not one; each root's bytes sum its whole tree.
pub fn instances(procs: &[ProcInfo], install_dir: &Path) -> Vec<InstanceStatus> {
    let claude: HashMap<u32, &ProcInfo> =
        procs.iter().filter(|p| is_desktop_claude(p, install_dir)).map(|p| (p.pid, p)).collect();
    let mut children: HashMap<u32, Vec<&ProcInfo>> = HashMap::new();
    for p in procs {
        children.entry(p.parent).or_default().push(p);
    }
    let tree_bytes = |root: u32| -> u64 {
        let (mut sum, mut stack, mut seen) = (0u64, vec![root], std::collections::HashSet::new());
        while let Some(pid) = stack.pop() {
            if !seen.insert(pid) {
                continue;
            }
            if let Some(p) = procs.iter().find(|p| p.pid == pid) {
                sum += p.working_set;
            }
            for c in children.get(&pid).into_iter().flatten() {
                stack.push(c.pid);
            }
        }
        sum
    };
    claude
        .values()
        .filter(|p| !claude.contains_key(&p.parent))
        .map(|p| InstanceStatus { owner_sid: p.owner_sid.clone(), root_pid: p.pid, bytes: tree_bytes(p.pid) })
        .collect()
}

const STEP: u64 = 100 << 20; // compare in 100 MB steps so the UI is not refreshed for noise

fn key(list: &[InstanceStatus]) -> Vec<(String, u32, u64)> {
    let mut k: Vec<_> = list.iter().map(|i| (i.owner_sid.clone(), i.root_pid, i.bytes / STEP)).collect();
    k.sort();
    k
}

pub fn changed(prev: &[InstanceStatus], prev_free: u64, next: &[InstanceStatus], next_free: u64) -> bool {
    key(prev) != key(next) || prev_free / STEP != next_free / STEP
}

fn sid_string(sid: windows::Win32::Security::PSID) -> String {
    use windows::Win32::Foundation::{LocalFree, HLOCAL};
    use windows::Win32::Security::Authorization::ConvertSidToStringSidW;
    if sid.0.is_null() {
        return String::new();
    }
    let mut s = windows::core::PWSTR::null();
    unsafe {
        if ConvertSidToStringSidW(sid, &mut s).is_err() {
            return String::new();
        }
        let out = s.to_string().unwrap_or_default();
        let _ = LocalFree(Some(HLOCAL(s.0 as *mut _)));
        out
    }
}

fn image_path(pid: u32) -> Option<PathBuf> {
    use windows::core::PWSTR;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION};
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len).is_ok();
        let _ = CloseHandle(h);
        ok.then(|| PathBuf::from(String::from_utf16_lossy(&buf[..len as usize])))
    }
}

fn parents() -> AppResult<HashMap<u32, u32>> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS};
    let mut map = HashMap::new();
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).map_err(|e| AppError::win32(e.code().0 as u32, "CreateToolhelp32Snapshot"))?;
        let mut e = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        let mut ok = Process32FirstW(snap, &mut e).is_ok();
        while ok {
            map.insert(e.th32ProcessID, e.th32ParentProcessID);
            ok = Process32NextW(snap, &mut e).is_ok();
        }
        let _ = CloseHandle(snap);
    }
    Ok(map)
}

fn processes() -> AppResult<Vec<ProcInfo>> {
    use windows::core::PWSTR;
    use windows::Win32::System::RemoteDesktop::{
        WTSEnumerateProcessesExW, WTSFreeMemoryExW, WTSTypeProcessInfoLevel1, WTS_CURRENT_SERVER_HANDLE, WTS_PROCESS_INFO_EXW,
    };
    const WTS_ANY_SESSION: u32 = 0xFFFF_FFFE;
    let parent_of = parents()?;
    let mut level = 1u32;
    let mut ptr = PWSTR::null();
    let mut count = 0u32;
    let mut out = Vec::new();
    unsafe {
        WTSEnumerateProcessesExW(Some(WTS_CURRENT_SERVER_HANDLE), &mut level, WTS_ANY_SESSION, &mut ptr, &mut count)
            .map_err(|e| AppError::win32(e.code().0 as u32, "WTSEnumerateProcessesExW"))?;
        let items = std::slice::from_raw_parts(ptr.0 as *const WTS_PROCESS_INFO_EXW, count as usize);
        for it in items {
            let name = it.pProcessName.to_string().unwrap_or_default();
            let exe = if name.eq_ignore_ascii_case("claude.exe") {
                image_path(it.ProcessId).unwrap_or_else(|| PathBuf::from(&name))
            } else {
                PathBuf::from(&name)
            };
            out.push(ProcInfo {
                pid: it.ProcessId,
                parent: parent_of.get(&it.ProcessId).copied().unwrap_or(0),
                exe,
                owner_sid: sid_string(it.pUserSid),
                working_set: it.WorkingSetSize as u64,
            });
        }
        let _ = WTSFreeMemoryExW(WTSTypeProcessInfoLevel1, ptr.0 as *const _, count);
    }
    Ok(out)
}

fn memory() -> (u64, u64) {
    use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    let mut m = MEMORYSTATUSEX { dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32, ..Default::default() };
    if unsafe { GlobalMemoryStatusEx(&mut m) }.is_ok() { (m.ullAvailPhys, m.ullTotalPhys) } else { (0, 0) }
}

pub fn snapshot(install_dir: &Path) -> AppResult<Snapshot> {
    let instances = instances(&processes()?, install_dir);
    let (free_ram, total_ram) = memory();
    Ok(Snapshot { instances, free_ram, total_ram })
}

/// Polls every `every`; calls `on_change` for the first snapshot and whenever it changes by a 100 MB step.
pub fn spawn_poller(install_dir: PathBuf, every: Duration, on_change: impl Fn(&Snapshot) + Send + 'static) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut last: Option<Snapshot> = None;
        loop {
            if let Ok(s) = snapshot(&install_dir) {
                let differs = last.as_ref().map(|l| changed(&l.instances, l.free_ram, &s.instances, s.free_ram)).unwrap_or(true);
                if differs {
                    on_change(&s);
                    last = Some(s);
                }
            }
            std::thread::sleep(every);
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    const INSTALL: &str = r"C:\Program Files\WindowsApps\Claude_5.1231.2.0_x64__pzs8sxrjxfjjc";
    fn claude(pid: u32, parent: u32, sid: &str, mb: u64) -> ProcInfo {
        ProcInfo { pid, parent, exe: PathBuf::from(INSTALL).join(r"app\Claude.exe"), owner_sid: sid.into(), working_set: mb << 20 }
    }

    #[test]
    fn picks_roots_and_sums_tree() {
        let procs = vec![
            ProcInfo { pid: 4, parent: 0, exe: PathBuf::from(r"C:\Windows\explorer.exe"), owner_sid: "S-main".into(), working_set: 900 << 20 },
            claude(100, 4, "S-main", 300),
            claude(101, 100, "S-main", 500), // renderer
            claude(102, 101, "S-main", 200), // grandchild (utility)
            claude(200, 50, "S-kantor", 400),
            claude(201, 200, "S-kantor", 100),
        ];
        let mut got = instances(&procs, std::path::Path::new(INSTALL));
        got.sort_by_key(|i| i.root_pid);
        assert_eq!(got, vec![
            InstanceStatus { owner_sid: "S-main".into(), root_pid: 100, bytes: 1000 << 20 },
            InstanceStatus { owner_sid: "S-kantor".into(), root_pid: 200, bytes: 500 << 20 },
        ]);
    }

    #[test]
    fn ignores_cli_claude_exe() {
        let cli = ProcInfo {
            pid: 300, parent: 4,
            exe: PathBuf::from(r"C:\Users\Irsal\AppData\Local\Claude-Data\claude_x\claude-code\2.1.321\claude.exe"),
            owner_sid: "S-main".into(), working_set: 80 << 20,
        };
        assert!(instances(&[cli], std::path::Path::new(INSTALL)).is_empty());
    }

    #[test]
    fn path_match_is_case_insensitive() {
        let mut p = claude(100, 4, "S-main", 10);
        p.exe = PathBuf::from(INSTALL.to_uppercase()).join(r"APP\CLAUDE.EXE");
        assert_eq!(instances(&[p], std::path::Path::new(INSTALL)).len(), 1);
    }

    #[test]
    fn counts_claude_after_an_update_moved_its_folder() {
        let mut p = claude(100, 4, "S-main", 10);
        p.exe = PathBuf::from(r"C:\Program Files\WindowsApps\Claude_5.1300.0.0_x64__pzs8sxrjxfjjc\app\Claude.exe");
        assert_eq!(instances(&[p], std::path::Path::new(INSTALL)).len(), 1);
    }

    #[test]
    fn ignores_claude_exe_outside_the_package() {
        let mut p = claude(100, 4, "S-main", 10);
        p.exe = PathBuf::from(r"C:\Tools\Claude.exe");
        assert!(instances(&[p], std::path::Path::new(INSTALL)).is_empty());
        let mut q = claude(101, 4, "S-main", 10);
        q.exe = PathBuf::from(r"C:\Program Files\WindowsApps\SomethingElse_1.0_x64__abc\app\Claude.exe");
        assert!(instances(&[q], std::path::Path::new(INSTALL)).is_empty());
    }

    #[test]
    fn rounded_change_detection() {
        let a = vec![InstanceStatus { owner_sid: "S".into(), root_pid: 1, bytes: 1_000 << 20 }];
        let b = vec![InstanceStatus { owner_sid: "S".into(), root_pid: 1, bytes: 1_040 << 20 }];
        let c = vec![InstanceStatus { owner_sid: "S".into(), root_pid: 1, bytes: 1_200 << 20 }];
        assert!(!changed(&a, 9 << 30, &b, 9 << 30));
        assert!(changed(&a, 9 << 30, &c, 9 << 30));
        assert!(changed(&a, 9 << 30, &a, 7 << 30));
    }

    #[test]
    fn live_snapshot_sees_this_claude() {
        let pkg = crate::claude_pkg::detect().unwrap();
        let snap = snapshot(&pkg.install_dir).unwrap();
        let me = crate::winuser::sid_of(&std::env::var("USERNAME").unwrap()).unwrap();
        assert!(snap.instances.iter().any(|i| i.owner_sid == me && i.bytes > 0), "{:?}", snap.instances);
        assert!(snap.free_ram > 0 && snap.total_ram >= snap.free_ram);
    }
}
