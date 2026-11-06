//! One-time move from the app's old name, "my-whisper": settings file,
//! keyring entry, GNOME shortcut and autostart entry. Safe to run every
//! start; it does nothing once the old pieces are gone.

use std::path::PathBuf;

const OLD_SETTINGS_DIR: &str = "dev.hadi.mywhisper";
const OLD_KEYRING_SERVICE: &str = "my-whisper";
const OLD_SHORTCUT_PATH: &str = "/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/my-whisper/";

pub fn migrate() {
    let Some(config) = dirs::config_dir() else { return };
    move_settings(&config.join(OLD_SETTINGS_DIR).join("settings.json"));
    move_api_key();
    if !cfg!(debug_assertions) {
        // The new shortcut entry is created by `system::shortcut::ensure`, keeping these keys.
        let old_binding = crate::system::shortcut::take_legacy(OLD_SHORTCUT_PATH);
        if let Some(b) = old_binding
            && let Err(e) = crate::system::shortcut::set(&b)
        {
            crate::log::warn(&format!("migrate shortcut: {e}"));
        }
        let old_autostart = config.join("autostart").join("my-whisper.desktop");
        if old_autostart.exists() {
            let _ = std::fs::remove_file(&old_autostart);
            let _ = crate::system::autostart::set(true);
        }
    }
}

fn move_settings(old: &PathBuf) {
    let new = crate::config::settings_path();
    if new.exists() || !old.exists() {
        return;
    }
    if let Some(dir) = new.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    match std::fs::copy(old, &new) {
        Ok(_) => {
            let _ = std::fs::remove_dir_all(old.parent().unwrap());
            crate::log::info("moved settings from the old my-whisper folder");
        }
        Err(e) => crate::log::warn(&format!("migrate settings: {e}")),
    }
}

fn move_api_key() {
    let Ok(old) = keyring::Entry::new(OLD_KEYRING_SERVICE, "openrouter-api-key") else { return };
    let Ok(key) = old.get_password() else { return };
    if crate::config::api_key::get().trim().is_empty() && crate::config::api_key::set(&key).is_err() {
        return; // keep the old entry so nothing is lost
    }
    let _ = old.delete_credential();
    crate::log::info("moved API key from the old my-whisper keyring entry");
}
