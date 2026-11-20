//! Updates from GitHub releases: the panel says a new version is out, the user presses Perbarui.
//!
//! `tauri-plugin-updater` reads `latest.json` from the newest published release, downloads the
//! installer and checks its signature against `updater.pub` before running it (NSIS, passive: one
//! UAC prompt). With an empty `updater.pub` the plugin is not registered and nothing is checked.
//!
//! Nothing is checked without the user's consent either: the panel asks once ("Beri tahu saya bila
//! ada versi baru?") and then offers a toggle; the answer is kept in `%APPDATA%\ClaudeSwitcher\updates.txt`.

use crate::error::{AppError, AppResult, ErrorCode};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

const KEY: &str = include_str!("../updater.pub");
const REPO: &str = "https://github.com/Coding-Folks/claude-switcher";
const FIRST_CHECK: Duration = Duration::from_secs(10);
const EVERY: Duration = Duration::from_secs(6 * 60 * 60);
/// How often the loop looks at the consent and the clock.
const TICK: Duration = Duration::from_secs(60);
/// A stalled connection must not hang the check loop or the panel's "Mengunduh…" forever.
const CHECK_TIMEOUT: Duration = Duration::from_secs(30);
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(15 * 60);

/// The public key in a key file, or None when it is empty (updates off).
pub fn key_from(s: &str) -> Option<&str> {
    let t = s.trim();
    (!t.is_empty()).then_some(t)
}

/// The update signing key built into this exe, if any.
pub fn pubkey() -> Option<&'static str> {
    key_from(KEY)
}

/// The release page of a version (what "Lihat perubahan" opens).
pub fn notes_url(version: &str) -> String {
    format!("{REPO}/releases/tag/v{}", version.trim_start_matches('v'))
}

/// Whole percent of a download, when its size is known.
pub fn percent(got: u64, total: Option<u64>) -> Option<u8> {
    let total = total.filter(|t| *t > 0)?;
    Some((got.saturating_mul(100) / total).min(100) as u8)
}

/// Whether the user lets Switcher look for new versions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Consent {
    /// Not answered yet: no checks, the panel asks.
    Unasked,
    On,
    Off,
}

impl Consent {
    pub fn parse(s: &str) -> Self {
        match s.trim() {
            "on" => Self::On,
            "off" => Self::Off,
            _ => Self::Unasked,
        }
    }
    fn code(self) -> &'static str {
        match self {
            Self::Unasked => "",
            Self::On => "on",
            Self::Off => "off",
        }
    }
    pub fn load(file: &Path) -> Self {
        std::fs::read_to_string(file)
            .map(|s| Self::parse(&s))
            .unwrap_or(Self::Unasked)
    }
    pub fn save(self, file: &Path) -> std::io::Result<()> {
        if let Some(dir) = file.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(file, self.code())
    }
}

/// `%APPDATA%\ClaudeSwitcher\updates.txt`.
pub fn consent_file() -> PathBuf {
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("ClaudeSwitcher")
        .join("updates.txt")
}

/// A check is due when the user agreed and the last one is six hours old (or there was none).
pub fn due(consent: Consent, last: Option<Instant>, now: Instant) -> bool {
    consent == Consent::On && last.map_or(true, |l| now.duration_since(l) >= EVERY)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UpdateInfo {
    pub version: String,
    pub notes_url: String,
}

/// The newest update found by the last check, and the user's consent to checks.
pub struct UpdateState {
    offered: Mutex<Option<UpdateInfo>>,
    consent: Mutex<Consent>,
}

impl UpdateState {
    pub fn new(consent: Consent) -> Self {
        Self {
            offered: Mutex::new(None),
            consent: Mutex::new(consent),
        }
    }
}

/// What the panel shows for update checks: "unavailable" (no key in this build), "unasked", "on", "off".
pub fn checks_setting(app: &AppHandle) -> &'static str {
    match (pubkey(), app.try_state::<UpdateState>()) {
        (Some(_), Some(st)) => match *st.consent.lock().unwrap() {
            Consent::Unasked => "unasked",
            Consent::On => "on",
            Consent::Off => "off",
        },
        _ => "unavailable",
    }
}

/// The user's answer: saved, and an offered update disappears when checks are turned off.
pub fn set_consent(app: &AppHandle, on: bool) -> AppResult<()> {
    let consent = if on { Consent::On } else { Consent::Off };
    consent.save(&consent_file())?;
    if let Some(st) = app.try_state::<UpdateState>() {
        *st.consent.lock().unwrap() = consent;
        if !on {
            *st.offered.lock().unwrap() = None;
        }
    }
    crate::applog::info("update checks", &[("on", if on { "yes" } else { "no" })]);
    Ok(())
}

fn failed(e: impl std::fmt::Display) -> AppError {
    AppError::new(ErrorCode::Io, "err.update_failed").with_detail(e.to_string())
}

/// The update the panel should offer, if any.
pub fn current(app: &AppHandle) -> Option<UpdateInfo> {
    app.try_state::<UpdateState>()?
        .offered
        .lock()
        .unwrap()
        .clone()
}

async fn check(app: &AppHandle) -> AppResult<Option<UpdateInfo>> {
    use tauri_plugin_updater::UpdaterExt;
    let found = app
        .updater_builder()
        .timeout(CHECK_TIMEOUT)
        .build()
        .map_err(failed)?
        .check()
        .await
        .map_err(failed)?;
    Ok(found.map(|u| UpdateInfo {
        notes_url: notes_url(&u.version),
        version: u.version,
    }))
}

/// "Cek sekarang": checks right away (only when this build has an update key) and offers what it finds.
pub async fn check_now(app: &AppHandle) -> AppResult<Option<UpdateInfo>> {
    if pubkey().is_none() {
        return Err(AppError::new(ErrorCode::Invalid, "err.update_unavailable"));
    }
    let found = check(app).await?;
    if let Some(st) = app.try_state::<UpdateState>() {
        *st.offered.lock().unwrap() = found.clone();
    }
    Ok(found)
}

/// With consent: a check shortly after start (or within a minute of the user agreeing), then every six
/// hours; a change redraws the panel.
pub fn spawn_checks(app: AppHandle, on_change: impl Fn() + Send + 'static) {
    std::thread::spawn(move || {
        std::thread::sleep(FIRST_CHECK);
        let mut last: Option<Instant> = None;
        loop {
            let consent = app
                .try_state::<UpdateState>()
                .map(|st| *st.consent.lock().unwrap())
                .unwrap_or(Consent::Unasked);
            if consent != Consent::On {
                last = None; // agreeing later checks right away
            }
            if !due(consent, last, Instant::now()) {
                std::thread::sleep(TICK);
                continue;
            }
            last = Some(Instant::now());
            match tauri::async_runtime::block_on(check(&app)) {
                Ok(found) => {
                    if let Some(st) = app.try_state::<UpdateState>() {
                        let mut cur = st.offered.lock().unwrap();
                        if *cur != found {
                            if let Some(u) = &found {
                                crate::applog::info("update available", &[("version", &u.version)]);
                            }
                            *cur = found;
                            drop(cur);
                            on_change();
                        }
                    }
                }
                Err(e) => crate::applog::error(
                    "update check failed",
                    &[("detail", e.detail.as_deref().unwrap_or(""))],
                ),
            }
            std::thread::sleep(TICK);
        }
    });
}

/// Downloads the offered update (progress as `update-progress` {percent}), installs it and restarts.
pub async fn install(app: &AppHandle) -> AppResult<()> {
    use tauri_plugin_updater::UpdaterExt;
    let update = app
        .updater_builder()
        .timeout(DOWNLOAD_TIMEOUT)
        .build()
        .map_err(failed)?
        .check()
        .await
        .map_err(failed)?
        .ok_or_else(|| failed("no update any more"))?;
    crate::applog::info("update install", &[("version", &update.version)]);
    let mut got: u64 = 0;
    let mut last: Option<u8> = None;
    let progress = app.clone();
    update
        .download_and_install(
            move |chunk, total| {
                got += chunk as u64;
                let p = percent(got, total);
                if p.is_some() && p != last {
                    last = p;
                    let _ = progress.emit("update-progress", serde_json::json!({ "percent": p }));
                }
            },
            || {},
        )
        .await
        .map_err(failed)?;
    app.restart()
}

/// Opens the offered version's release page in the default browser (only that URL, never one from the UI).
pub fn open_notes(app: &AppHandle) -> AppResult<()> {
    let url = current(app)
        .map(|u| u.notes_url)
        .ok_or_else(|| AppError::new(ErrorCode::NotFound, "err.not_found"))?;
    crate::proc::open_link(&url)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_or_whitespace_key_turns_updates_off() {
        assert_eq!(key_from(""), None);
        assert_eq!(key_from("  \r\n"), None);
        assert_eq!(key_from(" dW50cnVzdGVk\n"), Some("dW50cnVzdGVk"));
    }

    #[test]
    fn notes_url_points_at_the_release_tag() {
        let want = "https://github.com/Coding-Folks/claude-switcher/releases/tag/v0.3.0";
        assert_eq!(notes_url("0.3.0"), want);
        assert_eq!(notes_url("v0.3.0"), want);
    }

    #[test]
    fn download_percent_is_whole_and_capped() {
        assert_eq!(percent(50, Some(200)), Some(25));
        assert_eq!(percent(300, Some(200)), Some(100));
        assert_eq!(percent(10, None), None);
        assert_eq!(percent(10, Some(0)), None);
    }

    #[test]
    fn the_updater_config_in_tauri_conf_is_valid() {
        let conf: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let updater = conf["plugins"]["updater"].clone();
        let parsed: tauri_plugin_updater::Config = serde_json::from_value(updater).unwrap();
        assert_eq!(
            parsed.endpoints[0].as_str(),
            "https://github.com/Coding-Folks/claude-switcher/releases/latest/download/latest.json"
        );
        // `cargo tauri signer sign` does not record the version in the signature, so this must stay off.
        assert!(!parsed.require_signed_version);
        let windows = parsed.windows.expect("windows config");
        assert_eq!(format!("{:?}", windows.install_mode), "Passive");
    }

    #[test]
    fn consent_is_unasked_until_the_user_answers() {
        assert_eq!(Consent::parse("on"), Consent::On);
        assert_eq!(
            Consent::parse(
                " off
"
            ),
            Consent::Off
        );
        assert_eq!(Consent::parse(""), Consent::Unasked);
        assert_eq!(Consent::parse("maybe"), Consent::Unasked);
    }

    #[test]
    fn consent_is_saved_and_read_back() {
        let dir = std::env::temp_dir().join(format!("cs-consent-{}", uuid::Uuid::new_v4()));
        let file = dir.join("updates.txt");
        assert_eq!(Consent::load(&file), Consent::Unasked);
        Consent::On.save(&file).unwrap();
        assert_eq!(Consent::load(&file), Consent::On);
        Consent::Off.save(&file).unwrap();
        assert_eq!(Consent::load(&file), Consent::Off);
    }

    #[test]
    fn checks_run_only_with_consent_and_when_due() {
        let now = std::time::Instant::now();
        let six_h = Duration::from_secs(6 * 60 * 60);
        assert!(!due(Consent::Unasked, None, now));
        assert!(!due(Consent::Off, None, now));
        assert!(due(Consent::On, None, now));
        assert!(!due(Consent::On, Some(now), now));
        assert!(due(Consent::On, Some(now - six_h), now));
    }
}
