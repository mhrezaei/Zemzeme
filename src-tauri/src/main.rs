// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // GNOME on Wayland ignores window positions set by apps, so the overlay
    // could not sit at the screen edge. Running under XWayland fixes that.
    #[cfg(target_os = "linux")]
    if std::env::var_os("MY_WHISPER_NATIVE_WAYLAND").is_none() {
        // SAFETY: called before any other thread starts.
        unsafe { std::env::set_var("GDK_BACKEND", "x11") };
    }

    my_whisper_lib::run()
}
