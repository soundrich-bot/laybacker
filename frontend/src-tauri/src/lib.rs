mod commands;
pub mod models;
pub mod services;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            app.handle().plugin(
                tauri_plugin_log::Builder::default()
                    .level(log::LevelFilter::Info)
                    .build(),
            )?;
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
            commands::check_silence,
            commands::create_prores,
            commands::slate_video,
            commands::read_image_data_url,
            commands::check_stereo,
            commands::split_channels,
            commands::join_channels,
            commands::process_audio,
            commands::waveform_peaks,
            commands::chain_workdir,
            commands::remove_workdir,
            commands::chain_shape,
            commands::write_text_file,
            commands::video_frame,
            commands::detect_clicks,
            commands::scan_audio_issues,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
