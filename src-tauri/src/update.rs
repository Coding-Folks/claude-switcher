//! Updates from GitHub releases: the panel says a new version is out, the user presses Perbarui.
//!
//! `tauri-plugin-updater` reads `latest.json` from the newest published release, downloads the
//! installer and checks its signature against `updater.pub` before running it (NSIS, passive: one
//! UAC prompt). With an empty `updater.pub` the plugin is not registered and nothing is checked.

use crate::error::{AppError, AppResult, ErrorCode};
use serde::Serialize;
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

const KEY: &str = include_str!("../updater.pub");
const REPO: &str = "https://github.com/Coding-Folks/claude-switcher";
const FIRST_CHECK: Duration = Duration::from_secs(10);
const EVERY: Duration = Duration::from_secs(6 * 60 * 60);

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UpdateInfo {
    pub version: String,
    pub notes_url: String,
}

/// The newest update found by the last check.
#[derive(Default)]
pub struct UpdateState(Mutex<Option<UpdateInfo>>);

fn failed(e: impl std::fmt::Display) -> AppError {
    AppError::new(ErrorCode::Io, "err.update_failed").with_detail(e.to_string())
}

/// The update the panel should offer, if any.
pub fn current(app: &AppHandle) -> Option<UpdateInfo> {
    app.try_state::<UpdateState>()?.0.lock().unwrap().clone()
}

async fn check(app: &AppHandle) -> AppResult<Option<UpdateInfo>> {
    use tauri_plugin_updater::UpdaterExt;
    let found = app
        .updater()
        .map_err(failed)?
        .check()
        .await
        .map_err(failed)?;
    Ok(found.map(|u| UpdateInfo {
        notes_url: notes_url(&u.version),
        version: u.version,
    }))
}

/// Checks after a short wait, then every six hours; a change redraws the panel.
pub fn spawn_checks(app: AppHandle, on_change: impl Fn() + Send + 'static) {
    std::thread::spawn(move || {
        std::thread::sleep(FIRST_CHECK);
        loop {
            match tauri::async_runtime::block_on(check(&app)) {
                Ok(found) => {
                    if let Some(st) = app.try_state::<UpdateState>() {
                        let mut cur = st.0.lock().unwrap();
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
            std::thread::sleep(EVERY);
        }
    });
}

/// Downloads the offered update (progress as `update-progress` {percent}), installs it and restarts.
pub async fn install(app: &AppHandle) -> AppResult<()> {
    use tauri_plugin_updater::UpdaterExt;
    let update = app
        .updater()
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
    std::process::Command::new("explorer.exe")
        .arg(&url)
        .spawn()?;
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
}
