// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    zemzeme_lib::config::load_dotenv();

    // GNOME on Wayland ignores window positions set by apps, so the overlay
    // could not sit at the screen edge. Running under XWayland fixes that.
    #[cfg(target_os = "linux")]
    if std::env::var_os("ZEMZEME_NATIVE_WAYLAND").is_none() {
        // SAFETY: called before any other thread starts.
        unsafe { std::env::set_var("GDK_BACKEND", "x11") };
    }

    // `zemzeme mics` lists microphones without starting the app.
    if std::env::args().nth(1).as_deref() == Some("mics") {
        for m in zemzeme_lib::audio::list_microphones() {
            let mark = if m.is_default { "*" } else { " " };
            println!("{mark} {}\n    id: {}", m.name, m.id);
        }
        return;
    }

    zemzeme_lib::run()
}
