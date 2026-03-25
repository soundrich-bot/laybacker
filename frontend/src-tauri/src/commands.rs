use tauri::{Emitter, Manager, Window};

use crate::models::*;
use crate::services::{ffmpeg, inspector, loudness, matcher, namer, processor};

/// Check if FFmpeg is available on the system
#[tauri::command]
pub fn check_ffmpeg() -> Result<String, String> {
    if ffmpeg::is_ffmpeg_available() {
        ffmpeg::get_ffmpeg_version().ok_or_else(|| "FFmpeg found but version unknown".into())
    } else {
        Err("FFmpeg not found. Please install FFmpeg to use Laybacker.".into())
    }
}

/// Scan dropped files/folders and return inspected media files
#[tauri::command]
pub async fn scan_files(paths: Vec<String>) -> Result<Vec<MediaFile>, String> {
    tokio::task::spawn_blocking(move || inspector::scan_paths(&paths))
        .await
        .map_err(|e| format!("Task failed: {}", e))?
}

/// Auto-match audio files to video files
#[tauri::command]
pub fn match_files(files: Vec<MediaFile>) -> Vec<MatchedPair> {
    matcher::match_files(&files)
}

/// Generate output filenames for matched pairs
#[tauri::command]
pub fn generate_names(
    mut pairs: Vec<MatchedPair>,
    remove_duplicates: bool,
    output_extension: String,
) -> Vec<MatchedPair> {
    namer::generate_names(&mut pairs, remove_duplicates, &output_extension);
    pairs
}

/// Measure loudness of a single audio file
#[tauri::command]
pub async fn measure_loudness(audio_path: String) -> Result<(f64, f64), String> {
    tokio::task::spawn_blocking(move || {
        let measurement = loudness::measure(&audio_path)?;
        Ok((measurement.integrated_lufs, measurement.true_peak_dbtp))
    })
    .await
    .map_err(|e| format!("Task failed: {}", e))?
}

/// Process all matched pairs — emits progress events to the frontend
#[tauri::command]
pub async fn process_pairs(
    window: Window,
    pairs: Vec<MatchedPair>,
    settings: ExportSettings,
) -> Result<Vec<ProcessingResult>, String> {
    let window_clone = window.clone();

    let results = tokio::task::spawn_blocking(move || {
        processor::process_batch(&pairs, &settings, move |progress| {
            let _ = window_clone.emit("processing-progress", &progress);
        })
    })
    .await
    .map_err(|e| format!("Task failed: {}", e))?;

    Ok(results)
}

/// Open a folder in the system file manager
#[tauri::command]
pub fn reveal_in_finder(path: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("-R")
            .arg(&path)
            .spawn()
            .map_err(|e| format!("Failed to reveal in Finder: {}", e))?;
    }

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(format!("/select,{}", &path))
            .spawn()
            .map_err(|e| format!("Failed to reveal in Explorer: {}", e))?;
    }

    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(std::path::Path::new(&path).parent().unwrap_or(std::path::Path::new(&path)))
            .spawn()
            .map_err(|e| format!("Failed to open file manager: {}", e))?;
    }

    Ok(())
}

/// Open a URL in the default browser
#[tauri::command]
pub fn open_url(url: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&url)
            .spawn()
            .map_err(|e| format!("Failed to open URL: {}", e))?;
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", &url])
            .spawn()
            .map_err(|e| format!("Failed to open URL: {}", e))?;
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(&url)
            .spawn()
            .map_err(|e| format!("Failed to open URL: {}", e))?;
    }
    Ok(())
}

/// Resolve a bundled resource path
#[tauri::command]
pub fn get_resource_path(app: tauri::AppHandle, resource: String) -> Result<String, String> {
    let path = app
        .path()
        .resolve(&resource, tauri::path::BaseDirectory::Resource)
        .map_err(|e| format!("Failed to resolve resource: {}", e))?;
    Ok(path.to_string_lossy().to_string())
}

/// Play a sound file (for completion notification)
#[tauri::command]
pub fn play_sound(path: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("afplay")
            .arg(&path)
            .spawn()
            .map_err(|e| format!("Failed to play sound: {}", e))?;
    }

    #[cfg(target_os = "windows")]
    {
        // Use PowerShell to play audio on Windows
        std::process::Command::new("powershell")
            .args(["-c", &format!("(New-Object Media.SoundPlayer '{}').PlaySync()", path)])
            .spawn()
            .map_err(|e| format!("Failed to play sound: {}", e))?;
    }

    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("aplay")
            .arg(&path)
            .spawn()
            .map_err(|e| format!("Failed to play sound: {}", e))?;
    }

    Ok(())
}
