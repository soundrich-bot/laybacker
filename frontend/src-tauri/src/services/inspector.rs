use std::path::Path;

use crate::models::*;
use crate::services::ffmpeg;

/// Inspects a media file using ffprobe to extract metadata
pub fn inspect_file(path: &str) -> Result<MediaFile, String> {
    let path_obj = Path::new(path);
    let extension = path_obj
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    let media_type = if is_video_extension(&extension) {
        MediaType::Video
    } else if is_audio_extension(&extension) {
        MediaType::Audio
    } else {
        return Err(format!("Unsupported file type: {}", extension));
    };

    let filename = path_obj
        .file_name()
        .and_then(|f| f.to_str())
        .unwrap_or("unknown")
        .to_string();

    let filename_no_ext = path_obj
        .file_stem()
        .and_then(|f| f.to_str())
        .unwrap_or("unknown")
        .to_string();

    // Use ffprobe to get duration and codec info
    let probe = probe_file(path)?;

    // Extract thumbnail for video files
    let thumbnail_data = if media_type == MediaType::Video {
        ffmpeg::extract_thumbnail(path, probe.duration)
    } else {
        None
    };

    Ok(MediaFile {
        id: MediaFile::new_id(),
        path: path.to_string(),
        filename,
        filename_no_ext,
        extension,
        media_type,
        duration_secs: probe.duration,
        codec_info: probe.codec_name,
        sample_rate: probe.sample_rate,
        channel_count: probe.channels,
        frame_rate: probe.frame_rate,
        thumbnail_data,
    })
}

/// Scans a directory or list of paths for supported media files
pub fn scan_paths(paths: &[String]) -> Result<Vec<MediaFile>, String> {
    let mut files = Vec::new();

    for path_str in paths {
        let path = Path::new(path_str);

        if path.is_dir() {
            // Recursively scan directory
            for entry in walkdir::WalkDir::new(path)
                .follow_links(true)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                let entry_path = entry.path();
                if entry_path.is_file() {
                    if let Some(ext) = entry_path.extension().and_then(|e| e.to_str()) {
                        if is_supported_extension(ext) {
                            match inspect_file(entry_path.to_str().unwrap_or("")) {
                                Ok(file) => files.push(file),
                                Err(e) => log::warn!("Skipping file {:?}: {}", entry_path, e),
                            }
                        }
                    }
                }
            }
        } else if path.is_file() {
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                if is_supported_extension(ext) {
                    match inspect_file(path_str) {
                        Ok(file) => files.push(file),
                        Err(e) => log::warn!("Skipping file {}: {}", path_str, e),
                    }
                }
            }
        }
    }

    Ok(files)
}

struct ProbeResult {
    duration: f64,
    codec_name: Option<String>,
    sample_rate: Option<f64>,
    channels: Option<u32>,
    frame_rate: Option<f64>,
}

/// Parse an ffprobe frame-rate field like "25/1" or "30000/1001" into fps.
fn parse_frame_rate(s: &str) -> Option<f64> {
    if let Some((num, den)) = s.split_once('/') {
        let n = num.parse::<f64>().ok()?;
        let d = den.parse::<f64>().ok()?;
        if d != 0.0 && n != 0.0 {
            return Some(n / d);
        }
        None
    } else {
        s.parse::<f64>().ok().filter(|v| *v != 0.0)
    }
}

fn probe_file(path: &str) -> Result<ProbeResult, String> {
    let ffprobe = find_ffprobe();

    let output = ffmpeg::silent_command(&ffprobe)
        .args([
            "-v", "quiet",
            "-print_format", "json",
            "-show_format",
            "-show_streams",
            path,
        ])
        .output()
        .map_err(|e| format!("Failed to run ffprobe: {}. Is FFmpeg installed?", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("ffprobe failed: {}", stderr));
    }

    let json: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("Failed to parse ffprobe output: {}", e))?;

    // Get duration from format
    let duration = json["format"]["duration"]
        .as_str()
        .and_then(|d| d.parse::<f64>().ok())
        .unwrap_or(0.0);

    // Get codec info from first relevant stream
    let streams = json["streams"].as_array();

    let mut codec_name = None;
    let mut sample_rate = None;
    let mut channels = None;
    let mut frame_rate = None;

    if let Some(streams) = streams {
        // Find the video stream for video files, audio stream for audio files
        for stream in streams {
            let codec_type = stream["codec_type"].as_str().unwrap_or("");

            match codec_type {
                "video" => {
                    if codec_name.is_none() {
                        codec_name = stream["codec_name"].as_str().map(|s| s.to_string());
                    }
                    // avg_frame_rate is the true average; fall back to r_frame_rate.
                    if frame_rate.is_none() {
                        frame_rate = stream["avg_frame_rate"]
                            .as_str()
                            .and_then(parse_frame_rate)
                            .or_else(|| {
                                stream["r_frame_rate"].as_str().and_then(parse_frame_rate)
                            });
                    }
                }
                "audio" => {
                    if sample_rate.is_none() {
                        sample_rate = stream["sample_rate"]
                            .as_str()
                            .and_then(|s| s.parse::<f64>().ok());
                    }
                    if channels.is_none() {
                        channels = stream["channels"].as_u64().map(|c| c as u32);
                    }
                    if codec_name.is_none()
                        && !streams.iter().any(|s| {
                            s["codec_type"].as_str() == Some("video")
                        })
                    {
                        codec_name = stream["codec_name"].as_str().map(|s| s.to_string());
                    }
                }
                _ => {}
            }
        }
    }

    Ok(ProbeResult {
        duration,
        codec_name,
        sample_rate,
        channels,
        frame_rate,
    })
}

/// Find ffprobe binary (delegates to shared cached lookup)
fn find_ffprobe() -> String {
    ffmpeg::find_ffprobe()
}

#[cfg(test)]
mod tests {
    use super::parse_frame_rate;

    #[test]
    fn test_parse_frame_rate() {
        assert_eq!(parse_frame_rate("25/1"), Some(25.0));
        assert_eq!(parse_frame_rate("30/1"), Some(30.0));
        assert_eq!(parse_frame_rate("24"), Some(24.0));
        // NTSC-style fractional rates.
        let r = parse_frame_rate("30000/1001").unwrap();
        assert!((r - 29.97).abs() < 0.01);
        // ffprobe uses 0/0 for "unknown" — must not divide by zero.
        assert_eq!(parse_frame_rate("0/0"), None);
        assert_eq!(parse_frame_rate("garbage"), None);
    }
}
