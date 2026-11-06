//! Desktop integration: the global shortcut and starting at login.
//!
//! GNOME on Wayland doesn't let apps grab global keys, so the shortcut is a
//! GNOME custom keyboard shortcut that runs `zemzeme toggle`; the second
//! launch forwards the command to the running app (single-instance).

use std::path::PathBuf;
use std::process::Command;

pub const DEFAULT_SHORTCUT: &str = "<Super><Alt>space";

const MEDIA_KEYS: &str = "org.gnome.settings-daemon.plugins.media-keys";
const CUSTOM_SCHEMA: &str = "org.gnome.settings-daemon.plugins.media-keys.custom-keybinding";
const OUR_PATH: &str = "/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/zemzeme/";

fn exe() -> String {
    std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_else(|_| "zemzeme".into())
}

fn gsettings(args: &[&str]) -> Result<String, String> {
    let out = Command::new("gsettings").args(args).output().map_err(|e| format!("gsettings: {e}"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Strip GVariant string quoting: `'<Super>space'` → `<Super>space`.
fn unquote(s: &str) -> String {
    s.trim().trim_matches('\'').to_string()
}

/// Parse a GVariant string array such as `@as []` or `['/a/', '/b/']`.
fn parse_list(s: &str) -> Vec<String> {
    s.trim_start_matches("@as").split('\'').skip(1).step_by(2).map(String::from).collect()
}

fn format_list(items: &[String]) -> String {
    let quoted: Vec<String> = items.iter().map(|i| format!("'{i}'")).collect();
    format!("[{}]", quoted.join(", "))
}

pub mod shortcut {
    use super::*;

    fn schema() -> String {
        format!("{CUSTOM_SCHEMA}:{OUR_PATH}")
    }

    fn registered() -> Result<bool, String> {
        Ok(parse_list(&gsettings(&["get", MEDIA_KEYS, "custom-keybindings"])?).iter().any(|p| p == OUR_PATH))
    }

    /// Current binding in GNOME accelerator form, e.g. `<Super><Alt>space`.
    pub fn get() -> Result<Option<String>, String> {
        if !registered()? {
            return Ok(None);
        }
        let b = unquote(&gsettings(&["get", &schema(), "binding"])?);
        Ok((!b.is_empty()).then_some(b))
    }

    /// Registers (or updates) our shortcut to run this executable's `toggle`.
    /// An empty binding disables the shortcut but keeps the entry.
    pub fn set(binding: &str) -> Result<(), String> {
        let mut list = parse_list(&gsettings(&["get", MEDIA_KEYS, "custom-keybindings"])?);
        if !list.iter().any(|p| p == OUR_PATH) {
            list.push(OUR_PATH.to_string());
            gsettings(&["set", MEDIA_KEYS, "custom-keybindings", &format_list(&list)])?;
        }
        let s = schema();
        gsettings(&["set", &s, "name", "Zemzeme: start / stop recording"])?;
        gsettings(&["set", &s, "command", &format!("{} toggle", exe())])?;
        gsettings(&["set", &s, "binding", binding])?;
        Ok(())
    }

    /// Removes another custom shortcut entry (the app's old name) and returns
    /// its keys, or `None` if it isn't registered.
    pub fn take_legacy(path: &str) -> Option<String> {
        let mut list = parse_list(&gsettings(&["get", MEDIA_KEYS, "custom-keybindings"]).ok()?);
        if !list.iter().any(|p| p == path) {
            return None;
        }
        let binding = gsettings(&["get", &format!("{CUSTOM_SCHEMA}:{path}"), "binding"]).map(|b| unquote(&b)).ok();
        list.retain(|p| p != path);
        gsettings(&["set", MEDIA_KEYS, "custom-keybindings", &format_list(&list)]).ok()?;
        binding.filter(|b| !b.is_empty())
    }

    /// At startup of an installed (release) build: make sure the shortcut
    /// exists and runs this binary, keeping the user's chosen keys.
    pub fn ensure() {
        if cfg!(debug_assertions) || !cfg!(target_os = "linux") {
            return;
        }
        let binding = match get() {
            Ok(Some(b)) => b,
            Ok(None) => DEFAULT_SHORTCUT.to_string(),
            Err(e) => return crate::log::warn(&format!("can't read shortcut: {e}")),
        };
        if let Err(e) = set(&binding) {
            crate::log::warn(&format!("can't register shortcut: {e}"));
        }
    }
}

pub mod autostart {
    use super::*;

    fn path() -> PathBuf {
        dirs::config_dir().unwrap_or_else(|| PathBuf::from(".")).join("autostart").join("zemzeme.desktop")
    }

    pub fn enabled() -> bool {
        path().exists()
    }

    pub fn set(on: bool) -> Result<(), String> {
        let p = path();
        if !on {
            return match std::fs::remove_file(&p) {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(e.to_string()),
            };
        }
        if let Some(dir) = p.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let entry = format!(
            "[Desktop Entry]\nType=Application\nName=Zemzeme\nComment=Speech to text at your cursor\n\
             Exec={}\nIcon=zemzeme\nTerminal=false\nX-GNOME-Autostart-enabled=true\n",
            exe()
        );
        std::fs::write(&p, entry).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_gvariant_lists() {
        assert!(parse_list("@as []").is_empty());
        assert_eq!(parse_list("['/a/', '/b/']"), ["/a/", "/b/"]);
    }

    #[test]
    fn list_round_trip() {
        let items = vec!["/a/".to_string(), OUR_PATH.to_string()];
        assert_eq!(parse_list(&format_list(&items)), items);
    }

    #[test]
    fn unquotes() {
        assert_eq!(unquote("'<Super><Alt>space'\n"), "<Super><Alt>space");
    }
}
