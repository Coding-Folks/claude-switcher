pub mod applog;
pub mod claude_pkg;
pub mod commands;
pub mod elevated;
pub mod error;
pub mod flows;
pub mod history;
pub mod i18n;
pub mod launcher;
pub mod monitor;
pub mod naming;
pub mod platform;
pub mod share;
pub mod slot;
pub mod store;
pub mod tray;
pub mod vault;
pub mod winuser;

use commands::AppState;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tauri::{Manager, WindowEvent};

fn data_dir(var: &str) -> PathBuf {
    std::env::var_os(var)
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("ClaudeSwitcher")
}

pub fn run() {
    applog::init(&data_dir("LOCALAPPDATA").join("logs"));
    let store_path = data_dir("APPDATA").join("accounts.json");
    i18n::set(i18n::load(&data_dir("APPDATA").join("lang.txt")));
    let first_run = !store_path.exists();
    let flows = platform::WinPlatform::new()
        .map(|p| Arc::new(flows::Flows::new(p, store::Store::new(&store_path))));
    if let Err(e) = &flows {
        applog::error(
            "startup",
            &[
                ("code", &format!("{:?}", e.code)),
                ("detail", e.detail.as_deref().unwrap_or("")),
            ],
        );
    }
    let busy = Arc::new(AtomicUsize::new(0));
    let install_dir = flows.as_ref().ok().map(|f| f.platform().install_dir());

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            tray::show_panel(app, None)
        }))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state() == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                        tray::toggle_panel(app, None);
                    }
                })
                .build(),
        )
        .manage(AppState {
            flows,
            busy: busy.clone(),
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_accounts,
            commands::add_account,
            commands::open_account,
            commands::close_account,
            commands::rename_account,
            commands::remove_account,
            commands::repair_account,
            commands::import_history,
            commands::get_settings,
            commands::set_autostart,
            commands::set_language,
            commands::hide_panel,
        ])
        .setup(move |app| {
            let handle = app.handle().clone();
            tray::create(&handle)?;
            {
                use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut};
                let open_panel =
                    Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyK);
                if let Err(e) = handle.global_shortcut().register(open_panel) {
                    applog::error(
                        "shortcut Ctrl+Alt+K not registered",
                        &[("detail", &e.to_string())],
                    );
                }
            }
            if let Some(w) = app.get_webview_window("panel") {
                let (w2, busy) = (w.clone(), busy.clone());
                w.on_window_event(move |e| {
                    if let WindowEvent::Focused(false) = e {
                        if busy.load(Ordering::SeqCst) == 0 {
                            let _ = w2.hide();
                        }
                    }
                });
            }
            if let Ok(f) = handle.state::<AppState>().flows.clone() {
                // While an account is opening, keep refreshing so the 30 s timeout shows up (spec §4).
                let h = handle.clone();
                std::thread::spawn(move || loop {
                    std::thread::sleep(std::time::Duration::from_secs(5));
                    if f.has_opening() {
                        commands::refresh(&h);
                    }
                });
            }
            commands::refresh(&handle);
            if let Some(dir) = install_dir.clone() {
                let h = handle.clone();
                monitor::spawn_poller(dir, std::time::Duration::from_secs(5), move |_| {
                    commands::refresh(&h)
                });
            }
            if first_run {
                tray::show_panel(&handle, None); // first launch: show where the app lives
            }
            {
                // The autostart entry stores an exe path. After an install, update or move it can point
                // at an old copy, so while autostart is on, rewrite it to the exe that is running now.
                use tauri_plugin_autostart::ManagerExt;
                let auto = handle.autolaunch();
                if auto.is_enabled().unwrap_or(false) {
                    if let Err(e) = auto.enable() {
                        applog::error(
                            "autostart path not refreshed",
                            &[("detail", &e.to_string())],
                        );
                    }
                }
            }
            applog::info(
                "started",
                &[("first_run", if first_run { "yes" } else { "no" })],
            );
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Claude Switcher");
}
