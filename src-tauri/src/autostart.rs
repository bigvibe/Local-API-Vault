//! Optional "start at login" support.
//!
//! It only registers a per-user registry entry that launches the app with
//! `--autostart`. The app still starts locked: auto start never auto unlocks.

use std::sync::Mutex;

#[cfg(windows)]
const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
#[cfg(windows)]
const VALUE_NAME: &str = "LocalApiVault";

/// Holds the tray check menu item so both the tray menu and the app UI can
/// keep its checked state in sync with the registry.
pub struct MenuToggle(pub Mutex<Option<tauri::menu::CheckMenuItem<tauri::Wry>>>);

#[cfg(windows)]
pub fn is_enabled() -> Result<bool, String> {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let key = match hkcu.open_subkey(RUN_KEY) {
        Ok(key) => key,
        Err(_) => return Ok(false),
    };
    match key.get_value::<String, _>(VALUE_NAME) {
        Ok(value) => Ok(!value.trim().is_empty()),
        Err(_) => Ok(false),
    }
}

#[cfg(windows)]
pub fn set_enabled(enabled: bool) -> Result<(), String> {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (key, _) = hkcu
        .create_subkey(RUN_KEY)
        .map_err(|error| format!("无法访问开机启动注册表：{error}"))?;

    if enabled {
        let executable =
            std::env::current_exe().map_err(|error| format!("无法获取程序路径：{error}"))?;
        let command = format!("\"{}\" --autostart", executable.display());
        key.set_value(VALUE_NAME, &command)
            .map_err(|error| format!("无法写入开机启动项：{error}"))?;
    } else {
        match key.delete_value(VALUE_NAME) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("无法删除开机启动项：{error}")),
        }
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn is_enabled() -> Result<bool, String> {
    Ok(false)
}

#[cfg(not(windows))]
pub fn set_enabled(_enabled: bool) -> Result<(), String> {
    Err("开机自启动目前仅支持 Windows。".to_owned())
}

/// Refresh the tray check item from the registry, if it has been created.
pub fn sync_menu(app: &tauri::AppHandle) {
    use tauri::Manager;

    let Some(state) = app.try_state::<MenuToggle>() else {
        return;
    };
    let Ok(guard) = state.0.lock() else {
        return;
    };
    if let Some(item) = guard.as_ref() {
        let enabled = is_enabled().unwrap_or(false);
        let _ = item.set_checked(enabled);
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    fn read_raw() -> Option<String> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let key = hkcu.open_subkey(RUN_KEY).ok()?;
        key.get_value::<String, _>(VALUE_NAME).ok()
    }

    fn restore(value: Option<&str>) {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let (key, _) = hkcu.create_subkey(RUN_KEY).unwrap();
        match value {
            Some(value) => key.set_value(VALUE_NAME, &value.to_owned()).unwrap(),
            None => {
                let _ = key.delete_value(VALUE_NAME);
            }
        }
    }

    #[test]
    fn toggles_run_key_and_restores() {
        let original = read_raw();

        set_enabled(true).unwrap();
        assert!(is_enabled().unwrap());

        set_enabled(false).unwrap();
        assert!(!is_enabled().unwrap());

        restore(original.as_deref());
    }
}
