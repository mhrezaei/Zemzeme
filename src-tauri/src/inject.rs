//! Put text at the user's cursor: save the clipboard, copy the text, press the
//! paste shortcut, then restore the clipboard.
//!
//! Pasting (instead of typing key by key) is the only reliable way to insert
//! Persian and other non-Latin text into any app.

use std::sync::Mutex;
use std::time::Duration;

use arboard::{Clipboard, ImageData};

/// Time for the target app to read the clipboard before we restore it.
const RESTORE_AFTER: Duration = Duration::from_millis(700);

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PasteKey {
    /// Most apps.
    #[serde(rename = "ctrl+v")]
    CtrlV,
    /// Terminals.
    #[serde(rename = "ctrl+shift+v")]
    CtrlShiftV,
    /// Works in most apps and many terminals, and does not depend on keyboard layout.
    #[serde(rename = "shift+insert")]
    ShiftInsert,
}

impl PasteKey {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().replace(['-', '_', ' '], "+").as_str() {
            "ctrl+shift+v" => Self::CtrlShiftV,
            "shift+insert" => Self::ShiftInsert,
            _ => Self::CtrlV,
        }
    }
}

/// What the clipboard held before we replaced it.
enum Saved {
    Text(String),
    Image(ImageData<'static>),
}

pub enum Outcome {
    Pasted,
    /// The text is on the clipboard but the key press was not possible.
    CopiedOnly(String),
}

pub struct Injector {
    keyboard: Mutex<Option<keyboard::Keyboard>>,
    keyboard_error: Mutex<Option<String>>,
}

impl Injector {
    /// Creates the virtual keyboard now: the desktop needs a moment to pick up
    /// a new input device, so it must exist before the first paste.
    pub fn new() -> Self {
        let (kb, err) = match keyboard::Keyboard::new() {
            Ok(k) => (Some(k), None),
            Err(e) => {
                crate::log::warn(&format!("virtual keyboard unavailable: {e}"));
                (None, Some(e))
            }
        };
        Self { keyboard: Mutex::new(kb), keyboard_error: Mutex::new(err) }
    }

    pub fn paste(&self, text: &str, key: PasteKey) -> Result<Outcome, String> {
        let mut clipboard = Clipboard::new().map_err(|e| format!("Clipboard: {e}"))?;
        let saved = clipboard
            .get_text()
            .map(Saved::Text)
            .ok()
            .or_else(|| clipboard.get_image().ok().map(Saved::Image));

        clipboard.set_text(text).map_err(|e| format!("Clipboard: {e}"))?;
        // Let the desktop see the new clipboard owner before the paste key arrives.
        std::thread::sleep(Duration::from_millis(60));

        let pressed = match self.keyboard.lock().unwrap().as_mut() {
            Some(kb) => kb.press(key),
            None => Err(self.keyboard_error.lock().unwrap().clone().unwrap_or_default()),
        };

        if let Err(e) = pressed {
            // Leave the text on the clipboard so the user can paste it by hand.
            crate::log::warn(&format!("paste key failed: {e}"));
            return Ok(Outcome::CopiedOnly(e));
        }

        if let Some(saved) = saved {
            std::thread::spawn(move || {
                std::thread::sleep(RESTORE_AFTER);
                if let Ok(mut cb) = Clipboard::new() {
                    let _ = match saved {
                        Saved::Text(t) => cb.set_text(t),
                        Saved::Image(i) => cb.set_image(i),
                    };
                }
            });
        }
        Ok(Outcome::Pasted)
    }
}

#[cfg(target_os = "linux")]
mod keyboard {
    use std::time::Duration;

    use evdev::uinput::VirtualDevice;
    use evdev::{AttributeSet, KeyCode, KeyEvent};

    use super::PasteKey;

    pub struct Keyboard {
        device: VirtualDevice,
    }

    impl Keyboard {
        pub fn new() -> Result<Self, String> {
            let mut keys = AttributeSet::<KeyCode>::new();
            for k in [KeyCode::KEY_LEFTCTRL, KeyCode::KEY_LEFTSHIFT, KeyCode::KEY_V, KeyCode::KEY_INSERT] {
                keys.insert(k);
            }
            let device = VirtualDevice::builder()
                .and_then(|b| b.name("zemzeme virtual keyboard").with_keys(&keys))
                .and_then(|b| b.build())
                .map_err(|e| {
                    if e.kind() == std::io::ErrorKind::PermissionDenied {
                        "No access to /dev/uinput (run scripts/setup-linux.sh)".to_string()
                    } else {
                        format!("/dev/uinput: {e}")
                    }
                })?;
            Ok(Self { device })
        }

        pub fn press(&mut self, key: PasteKey) -> Result<(), String> {
            let combo: &[KeyCode] = match key {
                PasteKey::CtrlV => &[KeyCode::KEY_LEFTCTRL, KeyCode::KEY_V],
                PasteKey::CtrlShiftV => &[KeyCode::KEY_LEFTCTRL, KeyCode::KEY_LEFTSHIFT, KeyCode::KEY_V],
                PasteKey::ShiftInsert => &[KeyCode::KEY_LEFTSHIFT, KeyCode::KEY_INSERT],
            };
            // Press in order, release in reverse, with small gaps: some apps
            // drop events that arrive in the same instant.
            let gap = Duration::from_millis(12);
            for &k in combo {
                self.emit(k, 1)?;
                std::thread::sleep(gap);
            }
            for &k in combo.iter().rev() {
                self.emit(k, 0)?;
                std::thread::sleep(gap);
            }
            Ok(())
        }

        fn emit(&mut self, key: KeyCode, value: i32) -> Result<(), String> {
            self.device.emit(&[*KeyEvent::new(key, value)]).map_err(|e| e.to_string())
        }
    }
}

#[cfg(not(target_os = "linux"))]
mod keyboard {
    use super::PasteKey;

    /// macOS / Windows support comes later (enigo).
    pub struct Keyboard;

    impl Keyboard {
        pub fn new() -> Result<Self, String> {
            Err("Automatic paste is not supported on this OS yet".into())
        }

        pub fn press(&mut self, _key: PasteKey) -> Result<(), String> {
            unreachable!()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::PasteKey;

    #[test]
    fn parses_paste_keys() {
        assert_eq!(PasteKey::parse("ctrl+v"), PasteKey::CtrlV);
        assert_eq!(PasteKey::parse("Ctrl-Shift-V"), PasteKey::CtrlShiftV);
        assert_eq!(PasteKey::parse("shift_insert"), PasteKey::ShiftInsert);
        assert_eq!(PasteKey::parse("nonsense"), PasteKey::CtrlV);
    }
}
