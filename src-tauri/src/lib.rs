pub mod error;
pub mod naming;
pub mod store;
pub mod vault;
pub mod elevated;
pub mod winuser;
pub mod share;
pub mod claude_pkg;
pub mod launcher;
pub mod monitor;
pub mod platform;
pub mod flows;
pub mod applog;
pub mod commands;
pub mod tray;

use commands::AppState;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{Manager, WindowEvent};

fn data_dir(var: &str) -> PathBuf {
    std::env::var_os(var).map(PathBuf::from).unwrap_or_else(std::env::temp_dir).join("ClaudeSwitcher")
}

pub fn run() {
    applog::init(&data_dir("LOCALAPPDATA").join("logs"));
    let store_path = data_dir("APPDATA").join("accounts.json");
    let first_run = !store_path.exists();
    let flows = platform::WinPlatform::new().map(|p| Arc::new(flows::Flows::new(p, store::Store::new(&store_path))));
    if let Err(e) = &flows {
        applog::error("startup", &[("code", &format!("{:?}", e.code)), ("detail", e.detail.as_deref().unwrap_or(""))]);
    }
    let busy = Arc::new(AtomicBool::new(false));
    let install_dir = flows.as_ref().ok().map(|f| f.platform().install_dir());

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| tray::show_panel(app, None)))
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, None))
        .manage(AppState { flows, busy: busy.clone() })
        .invoke_handler(tauri::generate_handler![
            commands::list_accounts,
            commands::add_account,
            commands::open_account,
            commands::close_account,
            commands::rename_account,
            commands::remove_account,
            commands::repair_account,
            commands::get_settings,
            commands::set_autostart,
            commands::hide_panel,
        ])
        .setup(move |app| {
            let handle = app.handle().clone();
            tray::create(&handle)?;
            if let Some(w) = app.get_webview_window("panel") {
                let (w2, busy) = (w.clone(), busy.clone());
                w.on_window_event(move |e| {
                    if let WindowEvent::Focused(false) = e {
                        if !busy.load(Ordering::SeqCst) {
                            let _ = w2.hide();
                        }
                    }
                });
            }
            if let Some(dir) = install_dir.clone() {
                let h = handle.clone();
                monitor::spawn_poller(dir, std::time::Duration::from_secs(5), move |_| commands::refresh(&h));
            }
            if first_run {
                tray::show_panel(&handle, None); // first launch: show where the app lives
            }
            applog::info("started", &[("first_run", if first_run { "yes" } else { "no" })]);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Claude Switcher");
}
