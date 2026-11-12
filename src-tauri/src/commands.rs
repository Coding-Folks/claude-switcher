//! Tauri commands called by ui/app.js. Thin: each forwards to `Flows` on a blocking thread,
//! then tells the UI and the tray to refresh.

use crate::error::{AppError, AppResult, ErrorCode};
use crate::flows::{AccountView, Flows, OpenOutcome, RepairReport};
use crate::platform::{Platform, WinPlatform};
use crate::store::Account;
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_autostart::ManagerExt;

pub type AppFlows = Flows<WinPlatform>;

pub struct AppState {
    pub flows: Result<Arc<AppFlows>, AppError>,
    /// True while an operation runs, so the panel stays open when a UAC prompt takes focus.
    pub busy: Arc<AtomicBool>,
}

impl AppState {
    fn flows(&self) -> AppResult<Arc<AppFlows>> {
        self.flows.clone()
    }
}

#[derive(Serialize)]
pub struct Settings {
    autostart: bool,
    free_ram: u64,
    total_ram: u64,
    recovered_backup: Option<String>,
}

#[derive(Clone, Serialize)]
struct Progress {
    step: String,
    account_label: String,
}

async fn blocking<T: Send + 'static>(
    app: &AppHandle,
    busy: &Arc<AtomicBool>,
    job: impl FnOnce() -> AppResult<T> + Send + 'static,
) -> AppResult<T> {
    busy.store(true, Ordering::SeqCst);
    let out = tauri::async_runtime::spawn_blocking(job).await.map_err(AppError::io).and_then(|r| r);
    busy.store(false, Ordering::SeqCst);
    if let Err(e) = &out {
        crate::applog::error("command failed", &[("code", &format!("{:?}", e.code)), ("detail", e.detail.as_deref().unwrap_or(""))]);
    }
    refresh(app);
    out
}

/// Tells the panel to reload and rebuilds the tray menu.
pub fn refresh(app: &AppHandle) {
    let _ = app.emit("status", ());
    crate::tray::rebuild(app);
}

#[tauri::command]
pub async fn list_accounts(state: State<'_, AppState>) -> AppResult<Vec<AccountView>> {
    let f = state.flows()?;
    tauri::async_runtime::spawn_blocking(move || f.list()).await.map_err(AppError::io)?
}

#[tauri::command]
pub async fn add_account(app: AppHandle, state: State<'_, AppState>, label: String) -> AppResult<Account> {
    let f = state.flows()?;
    let emitter = app.clone();
    let shown = label.clone();
    crate::applog::info("add account", &[("label", &label)]);
    blocking(&app, &state.busy, move || {
        f.add(&label, &|step| {
            let _ = emitter.emit("progress", Progress { step: step.into(), account_label: shown.clone() });
        })
    })
    .await
}

#[tauri::command]
pub async fn open_account(app: AppHandle, state: State<'_, AppState>, id: String, confirmed_low_ram: bool) -> AppResult<OpenOutcome> {
    let f = state.flows()?;
    blocking(&app, &state.busy, move || f.open(&id, confirmed_low_ram)).await
}

#[tauri::command]
pub async fn close_account(app: AppHandle, state: State<'_, AppState>, id: String) -> AppResult<()> {
    let f = state.flows()?;
    blocking(&app, &state.busy, move || f.close(&id)).await
}

#[tauri::command]
pub async fn rename_account(app: AppHandle, state: State<'_, AppState>, id: String, label: String) -> AppResult<()> {
    let f = state.flows()?;
    blocking(&app, &state.busy, move || f.rename(&id, &label)).await
}

#[tauri::command]
pub async fn remove_account(app: AppHandle, state: State<'_, AppState>, id: String) -> AppResult<()> {
    let f = state.flows()?;
    crate::applog::info("remove account", &[("id", &id)]);
    blocking(&app, &state.busy, move || f.remove(&id)).await
}

#[tauri::command]
pub async fn import_history(app: AppHandle, state: State<'_, AppState>, id: String) -> AppResult<()> {
    let f = state.flows()?;
    crate::applog::info("import history", &[("id", &id)]);
    blocking(&app, &state.busy, move || f.import_history(&id)).await
}

#[tauri::command]
pub async fn repair_account(app: AppHandle, state: State<'_, AppState>, id: String) -> AppResult<RepairReport> {
    let f = state.flows()?;
    blocking(&app, &state.busy, move || f.repair(&id)).await
}

#[tauri::command]
pub async fn get_settings(app: AppHandle, state: State<'_, AppState>) -> AppResult<Settings> {
    let f = state.flows()?;
    let autostart = app.autolaunch().is_enabled().unwrap_or(false);
    let snap = tauri::async_runtime::spawn_blocking({
        let f = f.clone();
        move || f.platform().snapshot()
    })
    .await
    .map_err(AppError::io)?;
    let recovered_backup = f.recovered_backup.lock().unwrap().take().map(|p| p.display().to_string());
    Ok(Settings { autostart, free_ram: snap.free_ram, total_ram: snap.total_ram, recovered_backup })
}

#[tauri::command]
pub fn set_autostart(app: AppHandle, enabled: bool) -> AppResult<()> {
    let m = app.autolaunch();
    let r = if enabled { m.enable() } else { m.disable() };
    r.map_err(|e| AppError::new(ErrorCode::Io, "err.generic").with_detail(e.to_string()))
}

#[tauri::command]
pub fn hide_panel(app: AppHandle) {
    if let Some(w) = app.get_webview_window("panel") {
        let _ = w.hide();
    }
}
