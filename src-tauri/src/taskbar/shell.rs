//! Start Menu shortcuts and taskbar identity on Claude's windows (Win32 shell).

use crate::error::{AppError, AppResult};
use std::path::Path;

/// The taskbar identity of an account's windows and shortcut. Stable across renames.
pub fn aumid(id: &str) -> String {
    format!("CodingFolks.ClaudeSwitcher.Account.{id}")
}

/// What a pinned button runs to reopen the account.
pub fn relaunch_command(exe: &Path, id: &str) -> String {
    format!("\"{}\" --launch {id}", exe.display())
}

/// The name on the taskbar button and the shortcut.
pub fn display_name(label: &str) -> String {
    format!("Claude – {label}")
}

/// One `.lnk` file name per `(id, label)`, in order: characters Windows refuses in file names become
/// `_`, and labels that collide (ignoring case) get ` (2)`, ` (3)`.
pub fn shortcut_names(accounts: &[(&str, &str)]) -> Vec<(String, String)> {
    const REFUSED: &str = "<>:\"/\\|?*";
    let mut taken: Vec<String> = Vec::new();
    accounts
        .iter()
        .map(|(id, label)| {
            let clean: String = label
                .chars()
                .map(|c| {
                    if c.is_control() || REFUSED.contains(c) {
                        '_'
                    } else {
                        c
                    }
                })
                .collect();
            let clean = clean.trim_matches(|c: char| c == '.' || c.is_whitespace());
            let base = display_name(clean);
            let mut name = base.clone();
            let mut n = 1;
            while taken
                .iter()
                .any(|t| t.to_lowercase() == name.to_lowercase())
            {
                n += 1;
                name = format!("{base} ({n})");
            }
            taken.push(name.clone());
            (id.to_string(), format!("{name}.lnk"))
        })
        .collect()
}

fn win(e: windows::core::Error, what: &str) -> AppError {
    AppError::win32(e.code().0 as u32, what)
}

fn com_init() {
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
    // S_FALSE (already initialised) and RPC_E_CHANGED_MODE (another model on this thread) both leave
    // COM usable here.
    let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
}

/// A VT_LPWSTR PROPVARIANT whose string PropVariantClear can free.
struct PropString(windows::Win32::System::Com::StructuredStorage::PROPVARIANT);

impl PropString {
    fn new(s: &str) -> AppResult<Self> {
        use windows::Win32::System::Com::CoTaskMemAlloc;
        use windows::Win32::System::Com::StructuredStorage::PROPVARIANT;
        use windows::Win32::System::Variant::VT_LPWSTR;
        let w: Vec<u16> = s.encode_utf16().chain(std::iter::once(0)).collect();
        unsafe {
            let p = CoTaskMemAlloc(w.len() * 2) as *mut u16;
            if p.is_null() {
                return Err(AppError::io("out of memory"));
            }
            std::ptr::copy_nonoverlapping(w.as_ptr(), p, w.len());
            let mut pv = PROPVARIANT::default();
            (*pv.Anonymous.Anonymous).vt = VT_LPWSTR;
            (*pv.Anonymous.Anonymous).Anonymous.pwszVal = windows::core::PWSTR(p);
            Ok(Self(pv))
        }
    }
}

impl Drop for PropString {
    fn drop(&mut self) {
        let _ = unsafe {
            windows::Win32::System::Com::StructuredStorage::PropVariantClear(&mut self.0)
        };
    }
}

/// Writes (or overwrites) a shortcut that runs `exe --launch <id>` with the account's icon and AUMID.
pub fn write_shortcut(
    path: &Path,
    exe: &Path,
    id: &str,
    icon: &Path,
    aumid: &str,
) -> AppResult<()> {
    use windows::core::{Interface, HSTRING};
    use windows::Win32::Storage::EnhancedStorage::PKEY_AppUserModel_ID;
    use windows::Win32::System::Com::{CoCreateInstance, IPersistFile, CLSCTX_INPROC_SERVER};
    use windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore;
    use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};
    com_init();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    unsafe {
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)
            .map_err(|e| win(e, "CoCreateInstance(ShellLink)"))?;
        link.SetPath(&HSTRING::from(exe.as_os_str()))
            .map_err(|e| win(e, "IShellLinkW::SetPath"))?;
        link.SetArguments(&HSTRING::from(format!("--launch {id}")))
            .map_err(|e| win(e, "IShellLinkW::SetArguments"))?;
        link.SetIconLocation(&HSTRING::from(icon.as_os_str()), 0)
            .map_err(|e| win(e, "IShellLinkW::SetIconLocation"))?;
        let store: IPropertyStore = link.cast().map_err(|e| win(e, "IPropertyStore"))?;
        let v = PropString::new(aumid)?;
        store
            .SetValue(&PKEY_AppUserModel_ID, &v.0)
            .map_err(|e| win(e, "SetValue(AppUserModel_ID)"))?;
        store
            .Commit()
            .map_err(|e| win(e, "IPropertyStore::Commit"))?;
        let file: IPersistFile = link.cast().map_err(|e| win(e, "IPersistFile"))?;
        file.Save(&HSTRING::from(path.as_os_str()), true)
            .map_err(|e| win(e, "IPersistFile::Save"))?;
    }
    Ok(())
}

/// Visible top-level windows of `pid` that no other window owns (the ones with a taskbar button).
pub fn top_windows_of(pid: u32) -> Vec<isize> {
    use windows::core::BOOL;
    use windows::Win32::Foundation::{HWND, LPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindow, GetWindowThreadProcessId, IsWindowVisible, GW_OWNER,
    };
    struct Acc {
        pid: u32,
        out: Vec<isize>,
    }
    unsafe extern "system" fn each(hwnd: HWND, l: LPARAM) -> BOOL {
        let acc = unsafe { &mut *(l.0 as *mut Acc) };
        let mut owner_pid = 0u32;
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut owner_pid)) };
        let owned = unsafe { GetWindow(hwnd, GW_OWNER) }.is_ok_and(|o| !o.is_invalid());
        if owner_pid == acc.pid && unsafe { IsWindowVisible(hwnd) }.as_bool() && !owned {
            acc.out.push(hwnd.0 as isize);
        }
        true.into()
    }
    let mut acc = Acc {
        pid,
        out: Vec::new(),
    };
    let _ = unsafe { EnumWindows(Some(each), LPARAM(&mut acc as *mut Acc as isize)) };
    acc.out
}

fn window_store(
    hwnd: isize,
) -> AppResult<windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::Shell::PropertiesSystem::SHGetPropertyStoreForWindow;
    unsafe { SHGetPropertyStoreForWindow(HWND(hwnd as *mut _)) }
        .map_err(|e| win(e, "SHGetPropertyStoreForWindow"))
}

/// The AUMID set on a window, if any.
pub fn window_aumid(hwnd: isize) -> Option<String> {
    use windows::Win32::Storage::EnhancedStorage::PKEY_AppUserModel_ID;
    use windows::Win32::System::Com::CoTaskMemFree;
    use windows::Win32::System::Com::StructuredStorage::{
        PropVariantClear, PropVariantToStringAlloc,
    };
    com_init();
    let store = window_store(hwnd).ok()?;
    unsafe {
        let mut pv = store.GetValue(&PKEY_AppUserModel_ID).ok()?;
        let s = PropVariantToStringAlloc(&pv).ok();
        let _ = PropVariantClear(&mut pv);
        let s = s?;
        let out = s.to_string().ok();
        CoTaskMemFree(Some(s.0 as *const _));
        out.filter(|v| !v.is_empty())
    }
}

/// Icons loaded once per file and size, kept for the life of the app (windows keep using them).
fn load_icon(path: &Path, size: i32) -> Option<isize> {
    use std::collections::HashMap;
    use std::sync::Mutex;
    use windows::core::HSTRING;
    use windows::Win32::UI::WindowsAndMessaging::{LoadImageW, IMAGE_ICON, LR_LOADFROMFILE};
    type Cache = Option<HashMap<(std::path::PathBuf, i32), isize>>;
    static CACHE: Mutex<Cache> = Mutex::new(None);
    let mut cache = CACHE.lock().unwrap();
    let map = cache.get_or_insert_with(HashMap::new);
    if let Some(h) = map.get(&(path.to_path_buf(), size)) {
        return Some(*h);
    }
    let h = unsafe {
        LoadImageW(
            None,
            &HSTRING::from(path.as_os_str()),
            IMAGE_ICON,
            size,
            size,
            LR_LOADFROMFILE,
        )
    }
    .ok()?;
    map.insert((path.to_path_buf(), size), h.0 as isize);
    Some(h.0 as isize)
}

/// Gives a window the account's taskbar identity: relaunch command, name and icon first, then the
/// AUMID (which makes the taskbar re-read them), then the window icon for Alt+Tab and thumbnails.
pub fn stamp(hwnd: isize, aumid: &str, relaunch: &str, name: &str, icon: &Path) -> AppResult<()> {
    use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
    use windows::Win32::Storage::EnhancedStorage::{
        PKEY_AppUserModel_ID, PKEY_AppUserModel_RelaunchCommand,
        PKEY_AppUserModel_RelaunchDisplayNameResource, PKEY_AppUserModel_RelaunchIconResource,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GetSystemMetrics, SendMessageTimeoutW, ICON_BIG, ICON_SMALL, SMTO_ABORTIFHUNG, SM_CXICON,
        SM_CXSMICON, WM_SETICON,
    };
    com_init();
    let store = window_store(hwnd)?;
    let icon_res = format!("{},0", icon.display());
    for (key, value) in [
        (&PKEY_AppUserModel_RelaunchCommand, relaunch),
        (&PKEY_AppUserModel_RelaunchDisplayNameResource, name),
        (&PKEY_AppUserModel_RelaunchIconResource, icon_res.as_str()),
        (&PKEY_AppUserModel_ID, aumid),
    ] {
        let v = PropString::new(value)?;
        unsafe { store.SetValue(key, &v.0) }.map_err(|e| win(e, "SetValue(window)"))?;
    }
    for (kind, metric) in [(ICON_SMALL, SM_CXSMICON), (ICON_BIG, SM_CXICON)] {
        let size = unsafe { GetSystemMetrics(metric) };
        if let Some(h) = load_icon(icon, size) {
            // A hung Claude window must not hang the loop.
            unsafe {
                SendMessageTimeoutW(
                    HWND(hwnd as *mut _),
                    WM_SETICON,
                    WPARAM(kind as usize),
                    LPARAM(h),
                    SMTO_ABORTIFHUNG,
                    500,
                    None,
                )
            };
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn identity_strings() {
        assert_eq!(aumid("abc"), "CodingFolks.ClaudeSwitcher.Account.abc");
        assert_eq!(display_name("Kerja"), "Claude – Kerja");
        assert_eq!(
            relaunch_command(Path::new(r"C:\P F\cs.exe"), "x"),
            r#""C:\P F\cs.exe" --launch x"#
        );
    }

    #[test]
    fn shortcut_names_replace_characters_windows_refuses() {
        assert_eq!(
            shortcut_names(&[("a", "Kerja/Klien: A?")]),
            vec![("a".to_string(), "Claude – Kerja_Klien_ A_.lnk".to_string())]
        );
        assert_eq!(
            shortcut_names(&[("a", " Tim. ")]),
            vec![("a".to_string(), "Claude – Tim.lnk".to_string())]
        );
        assert_eq!(
            shortcut_names(&[("a", "x\u{7}y")]),
            vec![("a".to_string(), "Claude – x_y.lnk".to_string())]
        );
    }

    #[test]
    fn shortcut_names_keep_same_labels_apart() {
        let got = shortcut_names(&[("a", "Kerja"), ("b", "kerja"), ("c", "Kerja")]);
        let names: Vec<&str> = got.iter().map(|(_, n)| n.as_str()).collect();
        assert_eq!(
            names,
            [
                "Claude – Kerja.lnk",
                "Claude – kerja (2).lnk",
                "Claude – Kerja (3).lnk"
            ]
        );
    }

    #[test]
    #[ignore] // real COM: writes a shortcut to %TEMP%
    fn writes_a_real_shortcut() {
        let dir = std::env::temp_dir().join(format!("cs-lnk-{}", uuid::Uuid::new_v4()));
        let lnk = dir.join("Claude – Test.lnk");
        let exe = std::env::current_exe().unwrap();
        write_shortcut(&lnk, &exe, "id1", &exe, &aumid("id1")).unwrap();
        assert!(std::fs::metadata(&lnk).unwrap().len() > 0);
        write_shortcut(&lnk, &exe, "id1", &exe, &aumid("id1")).unwrap(); // overwrite works
    }
}
