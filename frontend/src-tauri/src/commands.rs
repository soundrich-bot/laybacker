use tauri::{Emitter, Manager, Window};

use crate::models::*;
use crate::services::{ffmpeg, inspector, loudness, matcher, namer, processor};

/// Cancel any in-progress processing
#[tauri::command]
pub fn cancel_processing() {
    processor::request_cancel();
}

/// Check if FFmpeg is available on the system
#[tauri::command]
pub fn check_ffmpeg() -> Result<String, String> {
    if ffmpeg::is_ffmpeg_available() {
        let version = ffmpeg::get_ffmpeg_version()
            .unwrap_or_else(|| "unknown".into());
        let path = ffmpeg::find_ffmpeg();
        let source = if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                if path.starts_with(dir.to_string_lossy().as_ref()) {
                    "bundled"
                } else {
                    "system"
                }
            } else {
                "system"
            }
        } else {
            "system"
        };
        Ok(format!("{} ({})", version, source))
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
            .arg(format!("/select,{}", path))
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

/// Open a URL in the default browser.
/// Only allows http, https, and mailto schemes to prevent command injection.
#[tauri::command]
pub fn open_url(url: String) -> Result<(), String> {
    // Validate URL scheme to prevent command injection
    let lower = url.to_lowercase();
    if !lower.starts_with("https://") && !lower.starts_with("http://") && !lower.starts_with("mailto:") {
        return Err(format!("Unsupported URL scheme: {}", url));
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&url)
            .spawn()
            .map_err(|e| format!("Failed to open URL: {}", e))?;
    }
    #[cfg(target_os = "windows")]
    {
        // Use explorer.exe instead of cmd /C start to avoid shell injection
        std::process::Command::new("explorer")
            .arg(&url)
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

/// Check silence compliance on an audio file (head/tail silence for broadcast)
#[tauri::command]
pub async fn check_silence(audio_path: String, duration_secs: f64, silence_ms: f64) -> Result<(bool, bool, f64, f64), String> {
    tokio::task::spawn_blocking(move || {
        ffmpeg::check_silence_compliance(&audio_path, duration_secs, silence_ms)
    })
    .await
    .map_err(|e| format!("Task failed: {}", e))?
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
        // Escape single quotes in path to prevent PowerShell injection
        let escaped_path = path.replace('\'', "''");
        ffmpeg::silent_command("powershell")
            .args(["-c", &format!("(New-Object Media.SoundPlayer '{}').PlaySync()", escaped_path)])
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

/// Transcode a video into an Apple ProRes 422 .mov "working file" for Pro Tools,
/// saved next to the source. Emits `prores-progress` events (keyed by videoPath)
/// as the encode advances. Returns the output path.
#[tauri::command]
pub async fn create_prores(
    window: Window,
    video_path: String,
    duration_secs: f64,
    profile: String,
) -> Result<String, String> {
    let video_for_event = video_path.clone();
    tokio::task::spawn_blocking(move || {
        let (num, label) = match profile.as_str() {
            "proxy" => (0u8, "Proxy"),
            "422" => (2u8, "422"),
            "hq" => (3u8, "HQ"),
            _ => (1u8, "LT"), // default: ProRes 422 LT
        };
        let path = std::path::Path::new(&video_path);
        let dir = path
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| ".".to_string());
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("video");
        let output = format!("{}/{}_ProRes_{}.mov", dir, stem, label);
        let args = ffmpeg::build_prores_command(&video_path, &output, num);
        ffmpeg::run_ffmpeg_with_progress(&args, duration_secs, |pct| {
            let _ = window.emit(
                "prores-progress",
                serde_json::json!({ "videoPath": video_for_event, "progress": pct }),
            );
        })?;
        Ok(output)
    })
    .await
    .map_err(|e| format!("Task failed: {}", e))?
}

#[cfg(test)]
mod tests {
    /// URL validation logic (extracted for testability)
    fn validate_url_scheme(url: &str) -> Result<(), String> {
        let lower = url.to_lowercase();
        if !lower.starts_with("https://") && !lower.starts_with("http://") && !lower.starts_with("mailto:") {
            return Err(format!("Unsupported URL scheme: {}", url));
        }
        Ok(())
    }

    #[test]
    fn test_url_https_allowed() {
        assert!(validate_url_scheme("https://example.com").is_ok());
    }

    #[test]
    fn test_url_http_allowed() {
        assert!(validate_url_scheme("http://example.com").is_ok());
    }

    #[test]
    fn test_url_mailto_allowed() {
        assert!(validate_url_scheme("mailto:user@example.com").is_ok());
    }

    #[test]
    fn test_url_case_insensitive() {
        assert!(validate_url_scheme("HTTPS://Example.COM").is_ok());
        assert!(validate_url_scheme("Http://example.com").is_ok());
    }

    #[test]
    fn test_url_file_blocked() {
        assert!(validate_url_scheme("file:///etc/passwd").is_err());
    }

    #[test]
    fn test_url_javascript_blocked() {
        assert!(validate_url_scheme("javascript:alert(1)").is_err());
    }

    #[test]
    fn test_url_shell_injection_blocked() {
        assert!(validate_url_scheme("calc.exe").is_err());
        assert!(validate_url_scheme("& calc.exe").is_err());
        assert!(validate_url_scheme("| rm -rf /").is_err());
    }

    #[test]
    fn test_url_data_uri_blocked() {
        assert!(validate_url_scheme("data:text/html,<script>alert(1)</script>").is_err());
    }
}
