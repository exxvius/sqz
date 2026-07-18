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

use tauri::Manager;

use crate::commands::AppState;

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
            commands::start_run,
            commands::pause_run,
            commands::resume_run,
            commands::cancel_run,
            commands::abort_file,
            commands::retry_file,
            commands::force_file,
            commands::is_running,
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
