pub mod applog;
pub mod claude_pkg;
pub mod commands;
pub mod deeplink;
pub mod elevated;
pub mod error;
pub mod flows;
pub mod history;
pub mod i18n;
pub mod launcher;
pub mod monitor;
pub mod naming;
pub mod platform;
pub mod proc;
pub mod sessions;
pub mod share;
pub mod slot;
pub mod store;
pub mod taskbar;
pub mod tray;
pub mod update;
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

/// Opens an account for a pinned taskbar button or Start Menu shortcut (`--launch <id>`). When it
/// needs the user (low RAM, an error), the panel opens instead.
fn launch_account(app: &tauri::AppHandle, id: String) {
    let Ok(f) = app.state::<AppState>().flows.clone() else {
        return tray::show_panel(app, None);
    };
    let app = app.clone();
    std::thread::spawn(move || {
        let res = f.open(&id, false);
        commands::refresh(&app);
        match res {
            Ok(flows::OpenOutcome::NeedsLowRamConfirm) => tray::show_panel(&app, None),
            Ok(_) => {}
            Err(e) => {
                applog::error(
                    "launch from taskbar",
                    &[("account", &id), ("code", &format!("{:?}", e.code))],
                );
                tray::show_panel(&app, None);
            }
        }
    });
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

    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(
            |app, args, _cwd| match taskbar::launch_arg(&args) {
                Some(id) => launch_account(app, id),
                None => tray::show_panel(app, None),
            },
        ))
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
        );
    // Updates only when this build carries a signing key (src-tauri/updater.pub).
    let builder = match update::pubkey() {
        Some(key) => builder.plugin(tauri_plugin_updater::Builder::new().pubkey(key).build()),
        None => builder,
    };
    builder
        .manage(update::UpdateState::new(update::Consent::load(
            &update::consent_file(),
        )))
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
            commands::get_update,
            commands::install_update,
            commands::open_update_notes,
            commands::set_update_checks,
            commands::fit_panel,
            commands::open_link_settings,
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
            // Google sign-in comes back as a claude:// link; route it to the account that started it.
            // Claude writes its own handler back each time it starts, so keep re-asserting ours.
            if let Ok(exe) = std::env::current_exe() {
                std::thread::spawn(move || loop {
                    match deeplink::ensure_handler(&exe) {
                        Ok(true) => applog::info("claude:// handler set", &[]),
                        Ok(false) => {}
                        Err(e) => applog::error(
                            "claude:// handler not set",
                            &[("detail", &e.to_string())],
                        ),
                    }
                    std::thread::sleep(std::time::Duration::from_secs(2));
                });
            }
            if let (Ok(f), Ok(exe)) = (
                handle.state::<AppState>().flows.clone(),
                std::env::current_exe(),
            ) {
                // Each account's Claude windows get their own taskbar button; Claude drops the identity
                // when it recreates a window, so this runs again and again.
                std::thread::spawn(move || {
                    let mut st = taskbar::TickState::default();
                    loop {
                        taskbar::tick(&f, &exe, &mut st);
                        std::thread::sleep(std::time::Duration::from_secs(2));
                    }
                });
            }
            if update::pubkey().is_some() {
                let h = handle.clone();
                update::spawn_checks(handle.clone(), move || commands::refresh(&h));
            }
            if let Some(id) = taskbar::launch_arg(&std::env::args().collect::<Vec<_>>()) {
                launch_account(&handle, id); // started by a pinned button or shortcut
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
