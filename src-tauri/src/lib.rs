mod autostart;
pub mod bridge;
mod commands;
mod crypto;
pub mod model;
mod redact;
mod state;
pub mod vault;

use state::AppState;
use std::sync::{Arc, Mutex};
use tauri::menu::{CheckMenuItem, Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::Manager;

fn show_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn toggle_autostart(app: &tauri::AppHandle) {
    let current = autostart::is_enabled().unwrap_or(false);
    if autostart::set_enabled(!current).is_ok() {
        autostart::sync_menu(app);
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let state = Arc::new(AppState::default());
    if let Err(error) = bridge::start(Arc::clone(&state)) {
        eprintln!("[bridge] disabled: {error}");
    }

    let launched_at_startup = std::env::args().any(|argument| argument == "--autostart");

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(state)
        .setup(move |app| {
            let autostart_enabled = autostart::is_enabled().unwrap_or(false);
            let show_item =
                MenuItem::with_id(app, "show", "显示 Local API Vault", true, None::<&str>)?;
            let autostart_item = CheckMenuItem::with_id(
                app,
                "autostart",
                "开机自启动",
                true,
                autostart_enabled,
                None::<&str>,
            )?;
            let quit_item = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_item, &autostart_item, &quit_item])?;
            let _ = app.manage(autostart::MenuToggle(Mutex::new(Some(
                autostart_item.clone(),
            ))));
            let mut tray = TrayIconBuilder::with_id("main")
                .tooltip("Local API Vault")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "show" => show_main_window(app),
                    "autostart" => toggle_autostart(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        show_main_window(tray.app_handle());
                    }
                });
            if let Some(icon) = app.default_window_icon().cloned() {
                tray = tray.icon(icon);
            }
            tray.build(app)?;

            if !launched_at_startup {
                show_main_window(app.handle());
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::default_vault_path,
            commands::autostart_status,
            commands::set_autostart,
            commands::create_vault,
            commands::rebuild_vault,
            commands::open_vault,
            commands::vault_status,
            commands::list_entries,
            commands::save_entry,
            commands::reveal_entry,
            commands::entry_reference,
            commands::get_secret,
            commands::list_run_profiles,
            commands::save_run_profile,
            commands::delete_run_profile,
            commands::delete_entry,
            commands::import_env,
            commands::export_backup,
            commands::change_passphrase,
            commands::enable_recovery,
            commands::save_recovery_identity,
            commands::recover_vault,
            commands::lock_vault,
        ])
        .on_window_event(|window, event| {
            if matches!(event, tauri::WindowEvent::Resized(_))
                && window.is_minimized().unwrap_or(false)
            {
                let _ = window.hide();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running Local API Vault");
}
