//! Tray icon: left click toggles the panel above the icon; right click shows a native menu
//! (accounts with a check when running, "Tambah akun…", "Keluar").

use crate::commands::AppState;
use crate::flows::{AccountState, OpenOutcome};
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, PhysicalPosition, Rect};

const TRAY_ID: &str = "main";

fn build_menu(app: &AppHandle) -> tauri::Result<(Menu<tauri::Wry>, usize)> {
    let menu = Menu::new(app)?;
    let mut running = 0;
    if let Ok(flows) = &app.state::<AppState>().flows {
        for v in flows.list().unwrap_or_default() {
            let on = v.state == AccountState::Running;
            running += on as usize;
            menu.append(&CheckMenuItem::with_id(app, format!("open:{}", v.account.id), &v.account.label, true, on, None::<&str>)?)?;
        }
    }
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&MenuItem::with_id(app, "add", "Tambah akun…", true, None::<&str>)?)?;
    menu.append(&MenuItem::with_id(app, "quit", "Keluar", true, None::<&str>)?)?;
    Ok((menu, running))
}

fn tooltip(running: usize) -> String {
    format!("Claude Switcher — {running} berjalan")
}

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let (menu, running) = build_menu(app)?;
    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip(tooltip(running))
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| on_menu(app, event.id().as_ref()))
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, rect, .. } = event {
                toggle_panel(tray.app_handle(), Some(rect));
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}

/// Rebuilds the menu and tooltip (called whenever the status changes).
pub fn rebuild(app: &AppHandle) {
    let app2 = app.clone();
    // Menus must be built on the main thread.
    let _ = app.run_on_main_thread(move || {
        if let (Some(tray), Ok((menu, running))) = (app2.tray_by_id(TRAY_ID), build_menu(&app2)) {
            let _ = tray.set_menu(Some(menu));
            let _ = tray.set_tooltip(Some(tooltip(running)));
        }
    });
}

fn on_menu(app: &AppHandle, id: &str) {
    match id {
        "quit" => app.exit(0),
        "add" => show_panel(app, None),
        _ => {
            if let Some(acc) = id.strip_prefix("open:") {
                let app = app.clone();
                let acc = acc.to_string();
                std::thread::spawn(move || {
                    let outcome = app.state::<AppState>().flows.as_ref().ok().map(|f| f.open(&acc, false));
                    // Low RAM needs the panel's confirmation; errors are shown there too.
                    if !matches!(outcome, Some(Ok(OpenOutcome::Launched | OpenOutcome::Focused))) {
                        show_panel(&app, None);
                    }
                    crate::commands::refresh(&app);
                });
            }
        }
    }
}

pub fn toggle_panel(app: &AppHandle, anchor: Option<Rect>) {
    if let Some(w) = app.get_webview_window("panel") {
        if w.is_visible().unwrap_or(false) {
            let _ = w.hide();
            return;
        }
    }
    show_panel(app, anchor);
}

/// Shows the panel above the tray icon (or at the bottom-right of the work area), kept on that monitor.
pub fn show_panel(app: &AppHandle, anchor: Option<Rect>) {
    let Some(w) = app.get_webview_window("panel") else { return };
    let size = w.outer_size().unwrap_or_default();
    let (w_w, w_h) = (size.width as i32, size.height as i32);
    let monitor = anchor
        .as_ref()
        .and_then(|r| {
            let p = r.position.to_physical::<i32>(1.0);
            w.monitor_from_point(p.x as f64, p.y as f64).ok().flatten()
        })
        .or_else(|| w.primary_monitor().ok().flatten());
    if let Some(m) = monitor {
        let area = m.work_area();
        let (ax, ay, aw, ah) = (area.position.x, area.position.y, area.size.width as i32, area.size.height as i32);
        let margin = 8;
        let (mut x, mut y) = (ax + aw - w_w - margin, ay + ah - w_h - margin);
        if let Some(r) = anchor {
            let p = r.position.to_physical::<i32>(m.scale_factor());
            let s = r.size.to_physical::<i32>(m.scale_factor());
            x = p.x + s.width / 2 - w_w / 2;
            // Taskbar at the top: open below the icon; otherwise above it.
            y = if p.y < ay + ah / 2 { p.y + s.height + margin } else { p.y - w_h - margin };
        }
        x = x.clamp(ax + margin, ax + aw - w_w - margin);
        y = y.clamp(ay + margin, ay + ah - w_h - margin);
        let _ = w.set_position(PhysicalPosition::new(x, y));
    }
    let _ = w.show();
    let _ = w.set_focus();
    crate::commands::refresh(app);
}
