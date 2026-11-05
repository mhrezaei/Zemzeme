pub mod audio;
pub mod config;
pub mod inject;
mod overlay;
pub mod pipeline;
mod settings_cmd;
mod state;
pub mod stt;

use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, WindowEvent,
};

use state::{AppState, Phase};

/// Commands accepted on the command line, e.g. `my-whisper toggle`.
/// A second launch forwards its arguments to the running instance.
fn handle_cli(app: &AppHandle, args: &[String]) {
    match args.get(1).map(String::as_str) {
        Some("toggle") => state::toggle(app),
        Some("cancel") => state::cancel(app),
        Some("settings") => open_settings(app),
        Some("quit") => app.exit(0),
        Some(other) => eprintln!("my-whisper: unknown command `{other}`"),
        None => {}
    }
}

fn open_settings(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("settings") {
        let _ = win.show();
        let _ = win.unminimize();
        let _ = win.set_focus();
    }
}

#[tauri::command]
fn toggle(app: AppHandle) {
    state::toggle(&app);
}

#[tauri::command]
fn cancel(app: AppHandle) {
    state::cancel(&app);
}

#[tauri::command]
fn show_settings(app: AppHandle) {
    open_settings(&app);
}

#[tauri::command]
fn current_phase(app: AppHandle) -> Phase {
    app.state::<AppState>().phase()
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let toggle_i = MenuItem::with_id(app, "toggle", "Start / stop recording", true, None::<&str>)?;
    let settings_i = MenuItem::with_id(app, "settings", "Settings…", true, None::<&str>)?;
    let quit_i = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&toggle_i, &settings_i, &sep, &quit_i])?;

    TrayIconBuilder::with_id("main")
        .icon(app.default_window_icon().unwrap().clone())
        .tooltip("my-whisper")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "toggle" => state::toggle(app),
            "settings" => open_settings(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                open_settings(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Must be registered first so a second launch is caught early.
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            handle_cli(app, &args);
        }))
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::new())
        .manage(inject::Injector::new())
        .invoke_handler(tauri::generate_handler![
            toggle,
            cancel,
            show_settings,
            current_phase,
            settings_cmd::get_settings,
            settings_cmd::save_settings,
            settings_cmd::set_api_key,
            settings_cmd::test_api_key,
            settings_cmd::list_microphones,
            settings_cmd::test_microphone,
        ])
        .setup(|app| {
            build_tray(app.handle())?;
            // Allow `my-whisper toggle` as the very first launch too.
            let args: Vec<String> = std::env::args().collect();
            handle_cli(app.handle(), &args);
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the settings window only hides it; the app keeps running in the tray.
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
