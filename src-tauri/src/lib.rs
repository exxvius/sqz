//! sqz — Tauri application entry point.
//!
//! The headless engine lives in [`core`]; this module wires it to the Tauri
//! shell (state, plugins, command handlers). `run()` is invoked by both the
//! desktop binary (`main.rs`) and the mobile entry points Tauri generates.

pub mod commands;
pub mod core;
pub mod events;
pub mod ffsetup;
pub mod run;

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::Manager;

use crate::commands::AppState;

/// Bring the main window back to the foreground (from the tray).
fn show_main_window(app: &tauri::AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.show();
        let _ = win.unminimize();
        let _ = win.set_focus();
    }
}

/// Build the system-tray icon + menu. Clicking the icon (or "Show sqz") restores
/// the window; "Quit" exits. The tray is what makes "minimize to tray" usable.
fn setup_tray(app: &tauri::App) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Show sqz", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &quit])?;

    let mut builder = TrayIconBuilder::new()
        .tooltip("sqz")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_main_window(app),
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
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            // Per-user data dir holds the manifest, settings, logs, and the
            // downloaded FFmpeg binaries.
            let data_dir = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| std::env::temp_dir().join("sqz"));
            std::fs::create_dir_all(&data_dir).ok();

            init_logging(&data_dir);
            app.manage(AppState::new(data_dir));
            setup_tray(app)?;
            // Start the unattended supervisor (watches saved libraries, runs on
            // schedule/idle). No-op until the user enables automation.
            crate::commands::spawn_supervisor(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::ffmpeg_status,
            commands::download_ffmpeg,
            commands::set_ffmpeg_paths,
            commands::clear_ffmpeg_override,
            commands::open_path,
            commands::reveal_path,
            commands::detect_encoders,
            commands::scan_inputs,
            commands::project_reclaim,
            commands::scan_health,
            commands::get_library,
            commands::delete_library_paths,
            commands::list_libraries,
            commands::save_library,
            commands::delete_library,
            commands::get_automation,
            commands::set_automation_enabled,
            commands::run_library_now,
            commands::start_run,
            commands::pause_run,
            commands::resume_run,
            commands::cancel_run,
            commands::cancel_scan,
            commands::abort_file,
            commands::retry_file,
            commands::force_file,
            commands::is_running,
            commands::quit_app,
            commands::get_history,
            commands::delete_history_item,
            commands::delete_history_matching,
            commands::clear_history,
            commands::get_settings,
            commands::save_settings,
            commands::restore_original,
            commands::export_settings,
            commands::import_settings,
            commands::export_history,
            commands::environment,
            commands::lock_status,
            commands::lock_setup,
            commands::lock_app,
            commands::unlock_app,
            commands::lock_change_password,
        ])
        .run(tauri::generate_context!())
        .expect("error while running sqz");
}

fn init_logging(data_dir: &std::path::Path) {
    use tracing_subscriber::{fmt, EnvFilter};
    let file_appender = tracing_appender::rolling::never(data_dir, "sqz.log");
    let filter = std::env::var("SQZ_LOG")
        .ok()
        .and_then(|s| EnvFilter::try_new(s).ok())
        .unwrap_or_else(|| EnvFilter::new("info"));
    let _ = fmt()
        .with_env_filter(filter)
        .with_writer(file_appender)
        .with_ansi(false)
        .try_init();
}
