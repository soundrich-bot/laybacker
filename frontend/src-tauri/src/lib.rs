mod commands;
mod models;
mod services;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::check_ffmpeg,
            commands::scan_files,
            commands::match_files,
            commands::generate_names,
            commands::measure_loudness,
            commands::process_pairs,
            commands::reveal_in_finder,
            commands::play_sound,
            commands::open_url,
            commands::get_resource_path,
            commands::cancel_processing,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
