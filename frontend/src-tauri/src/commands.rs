use tauri::{Emitter, Manager, Window};

use crate::models::*;
use crate::services::{channels, ffmpeg, inspector, loudness, matcher, namer, processing, processor, waveform};

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
    settings: Option<ExportSettings>,
) -> Vec<MatchedPair> {
    // The audio-only output spec (container / rate / depth) shapes the names
    // of audio-only outputs; video laybacks only need the container extension.
    let audio = settings.map(|s| s.audio_output_spec());
    namer::generate_names_with_audio(&mut pairs, remove_duplicates, &output_extension, audio.as_ref());
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

/// Slate a solo video (dropped without audio): prepend the user's text card,
/// keep the video's own soundtrack (delayed to match), saved next to the source
/// as "<name>_Slated.mov". Emits `slate-progress` events keyed by videoPath.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn slate_video(
    window: Window,
    video_path: String,
    video_duration_secs: f64,
    slate_image: String,
    slate_duration_secs: f64,
    frame_rate: Option<f64>,
    has_audio: bool,
    slate_black_secs: Option<f64>,
    slate_matte: Option<String>,
) -> Result<String, String> {
    let black_secs = slate_black_secs.unwrap_or(0.0).max(0.0);
    let video_for_event = video_path.clone();
    tokio::task::spawn_blocking(move || {
        // The card arrives as a base64 JPEG from the frontend canvas (the
        // bundled ffmpeg decodes mjpeg but not PNG, and can't draw text).
        // Overlay mode sends a second JPEG: the text's greyscale matte.
        let id = uuid::Uuid::new_v4();
        let img_path = processor::write_b64_jpeg(&slate_image, &format!("laybacker_slate_solo_{}.jpg", id))?;
        let matte_path = match slate_matte {
            Some(ref m) => Some(processor::write_b64_jpeg(m, &format!("laybacker_slate_solo_{}_matte.jpg", id))?),
            None => None,
        };

        let path = std::path::Path::new(&video_path);
        let dir = path
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| ".".to_string());
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("video");
        let output = format!("{}/{}_Slated.mov", dir, stem);

        let spec = ffmpeg::SlateSpec {
            image_path: img_path.clone(),
            duration_secs: slate_duration_secs.max(0.5),
            black_secs,
            matte_path: matte_path.clone(),
        };
        let args =
            ffmpeg::build_solo_slate_command(&video_path, &output, &spec, frame_rate, has_audio);
        let total = video_duration_secs + spec.preroll_secs();
        let result = ffmpeg::run_ffmpeg_with_progress(&args, total, |pct| {
            let _ = window.emit(
                "slate-progress",
                serde_json::json!({ "videoPath": video_for_event, "progress": pct }),
            );
        });
        let _ = std::fs::remove_file(&img_path);
        if let Some(ref m) = matte_path {
            let _ = std::fs::remove_file(m);
        }
        result?;
        Ok(output)
    })
    .await
    .map_err(|e| format!("Task failed: {}", e))?
}

/// Read a user-chosen image (for a slate background) and hand it to the
/// webview as a data URL. Read here rather than via the asset protocol because
/// an asset:// image would taint the slate canvas and block toDataURL.
#[tauri::command]
pub async fn read_image_data_url(path: String) -> Result<String, String> {
    tokio::task::spawn_blocking(move || {
        let bytes = std::fs::read(&path).map_err(|e| format!("Could not read image: {}", e))?;
        if bytes.len() > 40 * 1024 * 1024 {
            return Err("That image is over 40 MB — please use a smaller one".to_string());
        }
        let ext = std::path::Path::new(&path)
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .unwrap_or_default();
        let mime = match ext.as_str() {
            "png" => "image/png",
            "jpg" | "jpeg" => "image/jpeg",
            "webp" => "image/webp",
            "gif" => "image/gif",
            "bmp" => "image/bmp",
            _ => return Err(format!("Unsupported image type .{} — use PNG, JPEG, WebP, GIF or BMP", ext)),
        };
        use base64::Engine as _;
        let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
        Ok(format!("data:{};base64,{}", mime, b64))
    })
    .await
    .map_err(|e| format!("Task failed: {}", e))?
}

/// Stereo / phase QC for one file (see services::channels::StereoCheck).
#[tauri::command]
pub async fn check_stereo(audio_path: String, channels: u32) -> Result<channels::StereoCheck, String> {
    tokio::task::spawn_blocking(move || channels::check_stereo(&audio_path, channels))
        .await
        .map_err(|e| format!("Task failed: {}", e))?
}

/// Split a multichannel file into mono files beside it. Returns their paths
/// in channel order ("<stem>_L.wav", "<stem>_R.wav", …).
#[tauri::command]
pub async fn split_channels(
    audio_path: String,
    channel_layout: Option<String>,
    channels: u32,
) -> Result<Vec<String>, String> {
    tokio::task::spawn_blocking(move || {
        channels::split_channels(&audio_path, channel_layout.as_deref(), channels)
    })
    .await
    .map_err(|e| format!("Task failed: {}", e))?
}

/// Join mono files (in channel order) into one multichannel file.
#[tauri::command]
pub async fn join_channels(
    inputs: Vec<String>,
    channel_layout: String,
    output_path: String,
) -> Result<String, String> {
    tokio::task::spawn_blocking(move || {
        channels::join_channels(&inputs, &channel_layout, &output_path)?;
        Ok(output_path)
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

/// File Processing: fold to stereo / mono, fade, or trim silence. Writes a
/// new WAV beside the source and returns its path.
#[tauri::command]
pub async fn process_audio(
    op: String,
    audio_path: String,
    channel_layout: Option<String>,
    channels: u32,
    duration_secs: f64,
    param: Option<f64>,
) -> Result<String, String> {
    tokio::task::spawn_blocking(move || {
        processing::process_audio(&op, &audio_path, channel_layout.as_deref(), channels, duration_secs, param)
    })
    .await
    .map_err(|e| format!("Task failed: {}", e))?
}

/// Waveform overview for the preview player: `buckets` peak values in 0…1.
#[tauri::command]
pub async fn waveform_peaks(audio_path: String, buckets: u32) -> Result<Vec<f32>, String> {
    tokio::task::spawn_blocking(move || waveform::compute_peaks(&audio_path, buckets.clamp(1, 4000) as usize))
        .await
        .map_err(|e| format!("Task failed: {}", e))?
}

/// Multifunction Chain: a work folder for one run's intermediates.
#[tauri::command]
pub fn chain_workdir() -> Result<String, String> {
    processing::chain_workdir()
}

/// Multifunction Chain: throw a run's work folder away.
#[tauri::command]
pub fn remove_workdir(path: String) -> Result<(), String> {
    processing::remove_workdir(&path)
}

/// Multifunction Chain: the shaping steps (fold / trim / fade / split) on one
/// file, into the work folder. Returns the file(s) to carry on with.
#[tauri::command]
pub async fn chain_shape(
    audio_path: String,
    ops: Vec<processing::ShapeOp>,
    work_dir: String,
) -> Result<processing::ShapeResult, String> {
    tokio::task::spawn_blocking(move || processing::shape_file(&audio_path, &ops, &work_dir))
        .await
        .map_err(|e| format!("Task failed: {}", e))?
}

/// Write a text file (the chain's CSV report) where the user chose.
#[tauri::command]
pub fn write_text_file(path: String, contents: String) -> Result<(), String> {
    processing::write_text_file(&path, &contents)
}

/// One frame of a video as a JPEG data URL — the slate editor's backdrop.
#[tauri::command]
pub async fn video_frame(video_path: String, secs: f64, width: u32) -> Result<String, String> {
    tokio::task::spawn_blocking(move || ffmpeg::extract_frame(&video_path, secs, width))
        .await
        .map_err(|e| format!("Task failed: {}", e))?
}
