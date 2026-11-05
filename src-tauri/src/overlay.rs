//! The small floating window near the bottom of the screen.

use tauri::{AppHandle, Manager, PhysicalPosition};

// Logical size; keep in sync with the "overlay" window in tauri.conf.json.
const WIDTH: f64 = 300.0;
const HEIGHT: f64 = 72.0;
const BOTTOM_MARGIN: f64 = 48.0;

pub fn show(app: &AppHandle) {
    let Some(win) = app.get_webview_window("overlay") else { return };

    // Bottom-center of the monitor under the cursor (falls back to primary).
    let monitor = app
        .cursor_position()
        .ok()
        .and_then(|p| app.monitor_from_point(p.x, p.y).ok().flatten())
        .or_else(|| win.primary_monitor().ok().flatten());

    // A hidden window reports size 0, so use the configured size instead.
    if let Some(m) = monitor {
        let scale = m.scale_factor();
        let w = (WIDTH * scale) as i32;
        let h = (HEIGHT * scale) as i32;
        let margin = (BOTTOM_MARGIN * scale) as i32;
        let x = m.position().x + (m.size().width as i32 - w) / 2;
        let y = m.position().y + m.size().height as i32 - h - margin;
        let _ = win.set_position(PhysicalPosition::new(x, y));
    }

    let _ = win.show();
    let _ = win.set_always_on_top(true);
}

pub fn hide(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("overlay") {
        let _ = win.hide();
    }
}
