//! Interface language for the Rust side (tray menu, tooltip, the default name of the main account).
//! The panel has its own copy of every string in ui/strings.js; both follow the same choice.
//! Default: Bahasa Indonesia when Windows is in Indonesian, English otherwise. A choice made in the
//! panel is saved to `lang.txt` next to accounts.json and wins over the Windows language.

use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::RwLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    Id,
    En,
}

static CURRENT: RwLock<Lang> = RwLock::new(Lang::En);

impl Lang {
    pub fn code(self) -> &'static str {
        match self {
            Lang::Id => "id",
            Lang::En => "en",
        }
    }

    pub fn parse(s: &str) -> Option<Lang> {
        match s.trim().to_ascii_lowercase().as_str() {
            "id" => Some(Lang::Id),
            "en" => Some(Lang::En),
            _ => None,
        }
    }

    /// Windows primary language id 0x21 is Indonesian.
    pub fn from_langid(langid: u16) -> Lang {
        if langid & 0x3ff == 0x21 {
            Lang::Id
        } else {
            Lang::En
        }
    }
}

/// Every name the main account gets by default, in any language. A main account still carrying one of
/// these follows the language; a name the user typed is kept.
pub const MAIN_DEFAULTS: [&str; 2] = ["Utama", "Main"];

pub struct Text {
    pub open_panel: &'static str,
    pub add: &'static str,
    pub quit: &'static str,
    pub running: &'static str,
    pub main_label: &'static str,
}

const ID: Text = Text {
    open_panel: "Buka panel\tCtrl+Alt+K",
    add: "Tambah akun…",
    quit: "Keluar",
    running: "berjalan",
    main_label: "Utama",
};

const EN: Text = Text {
    open_panel: "Open panel\tCtrl+Alt+K",
    add: "Add account…",
    quit: "Quit",
    running: "running",
    main_label: "Main",
};

pub fn text_for(lang: Lang) -> &'static Text {
    match lang {
        Lang::Id => &ID,
        Lang::En => &EN,
    }
}

pub fn current() -> Lang {
    *CURRENT.read().unwrap()
}

pub fn text() -> &'static Text {
    text_for(current())
}

pub fn set(lang: Lang) {
    *CURRENT.write().unwrap() = lang;
}

/// The saved choice if there is one, else the Windows display language.
pub fn load(file: &Path) -> Lang {
    std::fs::read_to_string(file)
        .ok()
        .and_then(|s| Lang::parse(&s))
        .unwrap_or_else(system)
}

pub fn save(file: &Path, lang: Lang) -> std::io::Result<()> {
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(file, lang.code())
}

#[cfg(windows)]
pub fn system() -> Lang {
    // SAFETY: no arguments, returns a LANGID.
    Lang::from_langid(unsafe { windows::Win32::Globalization::GetUserDefaultUILanguage() })
}

#[cfg(not(windows))]
pub fn system() -> Lang {
    Lang::En
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indonesian_windows_maps_to_id_everything_else_to_en() {
        assert_eq!(Lang::from_langid(0x0421), Lang::Id); // id-ID
        assert_eq!(Lang::from_langid(0x0409), Lang::En); // en-US
        assert_eq!(Lang::from_langid(0x0413), Lang::En); // nl-NL
    }

    #[test]
    fn saved_choice_wins_and_bad_files_fall_back() {
        let dir = std::env::temp_dir().join(format!("cs-lang-{}", uuid::Uuid::new_v4()));
        let f = dir.join("sub").join("lang.txt");
        save(&f, Lang::Id).unwrap();
        assert_eq!(load(&f), Lang::Id);
        save(&f, Lang::En).unwrap();
        assert_eq!(load(&f), Lang::En);
        std::fs::write(&f, "xx").unwrap();
        assert_eq!(load(&f), system());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn parse_is_lenient_about_case_and_space() {
        assert_eq!(Lang::parse(" EN\n"), Some(Lang::En));
        assert_eq!(Lang::parse("fr"), None);
    }

    #[test]
    fn both_languages_name_the_main_account_with_a_known_default() {
        for l in [Lang::Id, Lang::En] {
            assert!(MAIN_DEFAULTS.contains(&text_for(l).main_label));
        }
    }
}
