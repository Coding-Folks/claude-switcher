//! An own taskbar button per account: colour, recoloured Claude icon, Start Menu shortcut, and the
//! taskbar identity stamped on that account's Claude windows. See docs/design/taskbar-per-account.md.

pub mod icon;
pub mod shell;

use crate::applog;
use crate::flows::Flows;
use crate::platform::WinPlatform;
use crate::store::{Account, AccountKind};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Taskbar colours for non-Main accounts, in the order they are given out.
pub const PALETTE: [&str; 8] = [
    "#2F6FDE", // blue
    "#1E9E6A", // green
    "#8B5CF6", // violet
    "#D9468C", // pink
    "#0E9AA7", // teal
    "#C9A227", // gold
    "#6B7280", // slate
    "#B91C1C", // red
];

/// The colour for the `n`-th coloured account: the first palette entry nobody uses yet, or the palette
/// again in order once all are taken.
pub fn next_color(used: &[&str], n: usize) -> &'static str {
    PALETTE
        .iter()
        .find(|c| !used.iter().any(|u| u.eq_ignore_ascii_case(c)))
        .copied()
        .unwrap_or(PALETTE[n % PALETTE.len()])
}

/// `--launch <id>` from a pinned button or Start Menu shortcut.
pub fn launch_arg(args: &[String]) -> Option<String> {
    let at = args.iter().position(|a| a == "--launch")?;
    args.get(at + 1).filter(|v| !v.starts_with("--")).cloned()
}

/// Our `.lnk` / `.ico` files in a folder that no account wants any more (case-insensitive, like Windows).
pub fn stale(existing: &[String], wanted: &[String]) -> Vec<String> {
    existing
        .iter()
        .filter(|f| {
            let lower = f.to_lowercase();
            lower.ends_with(".lnk") || lower.ends_with(".ico")
        })
        .filter(|f| !wanted.iter().any(|w| w.to_lowercase() == f.to_lowercase()))
        .cloned()
        .collect()
}

/// `<Public>\ClaudeSwitcher\icons\<user>`: the Windows 11 taskbar reads button icons from a sandboxed
/// process that cannot see the user's own folders, so icons live under Public, one folder per Windows
/// user so that two people on one PC never sweep each other's icons.
pub fn icons_dir_for(public: &Path, user: &str) -> PathBuf {
    public.join("ClaudeSwitcher").join("icons").join(user)
}

fn icons_dir() -> PathBuf {
    let public = std::env::var_os("PUBLIC")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Users\Public"));
    icons_dir_for(&public, &std::env::var("USERNAME").unwrap_or_default())
}

/// What a window was given by this run of Claude Switcher.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stamp {
    pub aumid: String,
    pub name: String,
    pub icon: PathBuf,
}

/// A window is stamped when this run has not stamped it yet (its icon handle may have died with an
/// earlier Switcher process, even though the AUMID is still on it) or when its name or icon changed.
pub fn needs_stamp(had: Option<&Stamp>, want: &Stamp) -> bool {
    had != Some(want)
}

fn shortcuts_dir() -> Option<PathBuf> {
    std::env::var_os("APPDATA")
        .map(|a| PathBuf::from(a).join(r"Microsoft\Windows\Start Menu\Programs\Claude Switcher"))
}

fn file_names(dir: &Path) -> Vec<String> {
    std::fs::read_dir(dir)
        .map(|r| {
            r.flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default()
}

/// What the loop remembers between ticks.
#[derive(Default)]
pub struct TickState {
    /// Shortcuts written this session, with the icon they point at (rewritten when it changes).
    written: HashMap<PathBuf, PathBuf>,
    /// Windows stamped by this run, with what they were given.
    stamped: HashMap<isize, Stamp>,
    /// Windows we could not stamp, logged once each.
    failed: HashSet<(isize, String)>,
    /// Accounts whose icon could not be made, logged once each.
    icon_failed: HashSet<String>,
    /// Claude's logo was missing (logged once).
    no_logo: bool,
    /// Claude's package folder when it moved since start (Claude updates itself into a new folder).
    claude_dir: Option<PathBuf>,
    /// When Claude was last looked up again.
    redetected_at: Option<std::time::Instant>,
}

/// How often Claude's folder may be looked up again while its logo is missing.
const REDETECT_EVERY: std::time::Duration = std::time::Duration::from_secs(10 * 60);

/// Look Claude up again (it updated into a new folder)? At most every ten minutes.
fn should_redetect(last: Option<std::time::Instant>, now: std::time::Instant) -> bool {
    last.map_or(true, |l| now.duration_since(l) >= REDETECT_EVERY)
}

/// Where Claude's logo comes from: the folder found after an update, else the one found at start.
fn logo_dir<'a>(st: &'a TickState, started: &'a Path) -> &'a Path {
    st.claude_dir.as_deref().unwrap_or(started)
}

/// One pass: colours, icons, shortcuts, stale files, then the windows of running accounts.
pub fn tick(flows: &Flows<WinPlatform>, exe: &Path, st: &mut TickState) {
    let accounts = match flows.ensure_colors() {
        Ok(a) => a,
        Err(e) => {
            applog::error("taskbar: accounts", &[("detail", &format!("{e:?}"))]);
            return;
        }
    };
    let mine: Vec<&Account> = accounts
        .iter()
        .filter(|a| a.kind != AccountKind::Main)
        .collect();
    let started_dir = flows.platform().install_dir();
    let install_dir = logo_dir(st, &started_dir).to_path_buf();
    let icons = icons_dir();
    let mut icon_of: HashMap<String, PathBuf> = HashMap::new();
    for a in &mine {
        let Some(color) = a.color.as_deref() else {
            continue;
        };
        match icon::ensure(&icons, &install_dir, &a.id, color) {
            Ok(p) => {
                icon_of.insert(a.id.clone(), p);
            }
            Err(e) if e.code == crate::error::ErrorCode::NotFound => {
                // Claude updates itself into a new versioned folder: look it up again before giving up.
                let now = std::time::Instant::now();
                if should_redetect(st.redetected_at, now) {
                    st.redetected_at = Some(now);
                    if let Ok(pkg) = crate::claude_pkg::detect() {
                        if pkg.install_dir != install_dir {
                            applog::info(
                                "taskbar: Claude moved",
                                &[("to", &pkg.install_dir.display().to_string())],
                            );
                            st.claude_dir = Some(pkg.install_dir);
                            st.no_logo = false;
                            return; // the next tick uses the new folder
                        }
                    }
                }
                if !st.no_logo {
                    applog::error(
                        "taskbar: Claude's logo not found",
                        &[("detail", &format!("{e:?}"))],
                    );
                    st.no_logo = true;
                }
                return; // no logo (Claude missing or changed): leave the taskbar alone
            }
            Err(e) => {
                // Only this account goes without; the others carry on.
                if st.icon_failed.insert(a.id.clone()) {
                    applog::error(
                        "taskbar: icon not made",
                        &[("account", &a.id), ("detail", &format!("{e:?}"))],
                    );
                }
            }
        }
    }
    st.no_logo = false;

    let pairs: Vec<(&str, &str)> = mine
        .iter()
        .map(|a| (a.id.as_str(), a.label.as_str()))
        .collect();
    let names = shell::shortcut_names(&pairs);
    if let Some(dir) = shortcuts_dir() {
        for (id, name) in &names {
            let (Some(icon), path) = (icon_of.get(id), dir.join(name)) else {
                continue;
            };
            if path.exists() && st.written.get(&path) == Some(icon) {
                continue;
            }
            match shell::write_shortcut(&path, exe, id, icon, &shell::aumid(id)) {
                Ok(()) => {
                    st.written.insert(path, icon.clone());
                }
                Err(e) => applog::error("taskbar: shortcut", &[("detail", &format!("{e:?}"))]),
            }
        }
        let wanted: Vec<String> = names.iter().map(|(_, n)| n.clone()).collect();
        for f in stale(&file_names(&dir), &wanted) {
            let _ = std::fs::remove_file(dir.join(&f));
            st.written.remove(&dir.join(&f));
        }
    }
    let wanted_icons: Vec<String> = icon_of
        .values()
        .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .collect();
    for f in stale(&file_names(&icons), &wanted_icons) {
        let _ = std::fs::remove_file(icons.join(f)); // an icon still in use by a window stays locked; next tick
    }

    let mut seen: HashSet<isize> = HashSet::new();
    for (a, pid) in flows.running_roots() {
        let Some(icon) = icon_of.get(&a.id) else {
            continue;
        };
        let want = Stamp {
            aumid: shell::aumid(&a.id),
            name: shell::display_name(&a.label),
            icon: icon.clone(),
        };
        for hwnd in shell::top_windows_of(pid) {
            seen.insert(hwnd);
            if !needs_stamp(st.stamped.get(&hwnd), &want) {
                continue;
            }
            let res = shell::stamp(
                hwnd,
                &want.aumid,
                &shell::relaunch_command(exe, &a.id),
                &want.name,
                icon,
            );
            match res {
                Ok(()) => {
                    st.stamped.insert(hwnd, want.clone());
                }
                Err(e) => {
                    if st.failed.insert((hwnd, a.id.clone())) {
                        applog::error(
                            "taskbar: window not stamped",
                            &[("account", &a.id), ("detail", &format!("{e:?}"))],
                        );
                    }
                }
            }
        }
    }
    // Closed windows: forget them (a new window may reuse the handle).
    st.stamped.retain(|h, _| seen.contains(h));
    st.failed.retain(|(h, _)| seen.contains(h));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_color_takes_the_first_unused_palette_entry() {
        assert_eq!(next_color(&[], 0), "#2F6FDE");
        assert_eq!(next_color(&["#2F6FDE"], 1), "#1E9E6A");
        assert_eq!(next_color(&["#1E9E6A"], 1), "#2F6FDE");
    }

    fn strings(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn launch_arg_reads_the_account_id() {
        assert_eq!(
            launch_arg(&strings(&["cs.exe", "--launch", "id1"])),
            Some("id1".into())
        );
        assert_eq!(launch_arg(&strings(&["cs.exe", "--launch"])), None);
        assert_eq!(
            launch_arg(&strings(&["cs.exe", "--launch", "--other"])),
            None
        );
        assert_eq!(launch_arg(&strings(&["cs.exe"])), None);
    }

    #[test]
    fn stale_lists_our_old_files_and_leaves_others() {
        let existing = strings(&[
            "Claude – A.lnk",
            "Claude – Old.lnk",
            "notes.txt",
            "id9-0000abcd.ico",
        ]);
        let wanted = strings(&["Claude – A.lnk", "id1-1234abcd.ico"]);
        assert_eq!(
            stale(&existing, &wanted),
            strings(&["Claude – Old.lnk", "id9-0000abcd.ico"])
        );
    }

    #[test]
    fn stale_ignores_case_like_windows() {
        let existing = strings(&["claude – a.lnk"]);
        assert!(stale(&existing, &strings(&["Claude – A.lnk"])).is_empty());
    }

    fn stamp_of(label: &str, icon: &str) -> Stamp {
        Stamp {
            aumid: "A".into(),
            name: shell::display_name(label),
            icon: PathBuf::from(icon),
        }
    }

    #[test]
    fn a_window_not_stamped_by_this_run_is_stamped_even_if_its_aumid_matches() {
        // After a restart the window keeps the AUMID, but its icon handle died with the old process.
        assert!(needs_stamp(None, &stamp_of("Kerja", "a.ico")));
    }

    #[test]
    fn a_window_is_stamped_again_after_a_rename_or_a_new_icon_only() {
        let had = stamp_of("Kerja", "a.ico");
        assert!(!needs_stamp(Some(&had), &stamp_of("Kerja", "a.ico")));
        assert!(needs_stamp(Some(&had), &stamp_of("Kantor", "a.ico")));
        assert!(needs_stamp(Some(&had), &stamp_of("Kerja", "b.ico")));
    }

    #[test]
    fn icons_are_kept_per_windows_user_under_public() {
        assert_eq!(
            icons_dir_for(Path::new(r"C:\Users\Public"), "alice"),
            PathBuf::from(r"C:\Users\Public\ClaudeSwitcher\icons\alice")
        );
    }

    #[test]
    fn claude_is_looked_up_again_when_its_logo_is_gone_but_not_on_every_tick() {
        let now = std::time::Instant::now();
        assert!(should_redetect(None, now));
        assert!(!should_redetect(Some(now), now));
        assert!(!should_redetect(Some(now - REDETECT_EVERY / 2), now));
        assert!(should_redetect(Some(now - REDETECT_EVERY), now));
    }

    #[test]
    fn the_logo_comes_from_the_newly_found_claude_after_an_update() {
        let started =
            Path::new(r"C:\Program Files\WindowsApps\Claude_6.6395.0.0_x64__pzs8sxrjxfjjc");
        let updated =
            PathBuf::from(r"C:\Program Files\WindowsApps\Claude_6.28899.2.0_x64__pzs8sxrjxfjjc");
        let mut st = TickState::default();
        assert_eq!(logo_dir(&st, started), started);
        st.claude_dir = Some(updated.clone());
        assert_eq!(logo_dir(&st, started), updated.as_path());
    }

    #[test]
    fn next_color_repeats_in_order_when_the_palette_is_used_up() {
        assert_eq!(next_color(&PALETTE, 8), "#2F6FDE");
        assert_eq!(next_color(&PALETTE, 9), "#1E9E6A");
    }

    #[test]
    #[ignore] // live: real accounts.json, Start Menu, %PUBLIC% icons and the running Claude windows
    fn live_tick() {
        let store = std::path::PathBuf::from(std::env::var_os("APPDATA").unwrap())
            .join("ClaudeSwitcher")
            .join("accounts.json");
        let f = Flows::new(
            WinPlatform::new().unwrap(),
            crate::store::Store::new(&store),
        );
        let exe = std::path::PathBuf::from(r"C:\Program Files\Claude Switcher\claude-switcher.exe");
        let mut st = TickState::default();
        println!(
            "ensure_colors: {:?}",
            f.ensure_colors()
                .map(|l| l.into_iter().map(|a| a.color).collect::<Vec<_>>())
        );
        println!("install_dir: {}", f.platform().install_dir().display());
        tick(&f, &exe, &mut st);
        for (a, pid) in f.running_roots() {
            for h in shell::top_windows_of(pid) {
                println!(
                    "{} {:?} pid={pid} hwnd={h:#x} aumid={:?}",
                    a.label,
                    a.color,
                    shell::window_aumid(h)
                );
            }
        }
        println!("failed: {}", st.failed.len());
    }
}
