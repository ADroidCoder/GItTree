//! 关掉窗口后继续在后台运行，好让定时提醒照常发出：窗口只是隐藏，菜单栏 / 托盘里留一个图标。

use std::sync::atomic::{AtomicBool, Ordering};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

const TRAY: &str = "main";
static KEEP_RUNNING: AtomicBool = AtomicBool::new(false);

pub fn keep_running() -> bool {
    KEEP_RUNNING.load(Ordering::Relaxed)
}

pub fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

/// 打开或关闭后台运行；菜单文字由界面按当前语言传进来
#[tauri::command]
pub fn set_keep_running(app: AppHandle, on: bool, show_label: String, quit_label: String) -> Result<(), String> {
    KEEP_RUNNING.store(on, Ordering::Relaxed);
    let _ = app.remove_tray_by_id(TRAY);
    if !on {
        return Ok(());
    }
    let build = || -> tauri::Result<()> {
        let show = MenuItem::with_id(&app, "show", show_label, true, None::<&str>)?;
        let quit = MenuItem::with_id(&app, "quit", quit_label, true, None::<&str>)?;
        let menu = Menu::with_items(&app, &[&show, &quit])?;
        let mut tray = TrayIconBuilder::with_id(TRAY)
            .tooltip("GitTree")
            .menu(&menu)
            .show_menu_on_left_click(false)
            .on_menu_event(|app, e| match e.id().as_ref() {
                "show" => show_main(app),
                "quit" => app.exit(0),
                _ => {}
            })
            .on_tray_icon_event(|tray, e| {
                if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e {
                    show_main(tray.app_handle());
                }
            });
        if let Some(icon) = app.default_window_icon() {
            tray = tray.icon(icon.clone());
        }
        tray.build(&app)?;
        Ok(())
    };
    build().map_err(|e| e.to_string())
}
