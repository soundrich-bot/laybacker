use std::process::Command;

use crate::models::*;

/// Find the ffmpeg binary
pub fn find_ffmpeg() -> String {
    let candidates = if cfg!(target_os = "windows") {
        vec![
            "ffmpeg".to_string(),
            "C:\\ffmpeg\\bin\\ffmpeg.exe".to_string(),
        ]
    } else {
        vec![
            "ffmpeg".to_string(),
            "/usr/local/bin/ffmpeg".to_string(),
            "/opt/homebrew/bin/ffmpeg".to_string(),
            "/usr/bin/ffmpeg".to_string(),
        ]
    };

    for candidate in &candidates {
        if Command::new(candidate).arg("-version").output().is_ok() {
            return candidate.clone();
        }
    }

    "ffmpeg".to_string()
}

/// Check if ffmpeg is available
pub fn is_ffmpeg_available() -> bool {
    let ffmpeg = find_ffmpeg();
    Command::new(&ffmpeg).arg("-version").output().is_ok()
}

/// Get ffmpeg version string
pub fn get_ffmpeg_version() -> Option<String> {
    let ffmpeg = find_ffmpeg();
    let output = Command::new(&ffmpeg).arg("-version").output().ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout.lines().next().map(|l| l.to_string())
}

/// Build the ffmpeg command arguments for a muxing job
pub fn build_mux_command(
    video_path: &str,
    audio_path: &str,
    output_path: &str,
    settings: &ExportSettings,
    audio_gain_db: Option<f64>,
    timecode_offset_secs: f64,
    compliance: Option<(f64, f64, f64)>, // (duration_secs, silence_ms, fade_ms)
) -> Vec<String> {
    let mut args: Vec<String> = Vec::new();

    // Input files
    args.extend(["-y".to_string()]); // Overwrite output
    args.extend(["-i".to_string(), video_path.to_string()]);
    args.extend(["-i".to_string(), audio_path.to_string()]);

    // Map video from first input, audio from second input
    args.extend(["-map".to_string(), "0:v:0".to_string()]);
    args.extend(["-map".to_string(), "1:a:0".to_string()]);

    // Video codec settings
    match settings.video_codec {
        VideoCodecOption::Original => {
            args.extend(["-c:v".to_string(), "copy".to_string()]);
        }
        VideoCodecOption::H264 => {
            args.extend(["-c:v".to_string(), "libx264".to_string()]);
            args.extend(["-crf".to_string(), "18".to_string()]);
            args.extend(["-preset".to_string(), "medium".to_string()]);
            args.extend(["-pix_fmt".to_string(), "yuv420p".to_string()]);
        }
    }

    // Audio filter chain
    let mut audio_filters: Vec<String> = Vec::new();

    // Timecode offset (delay audio start)
    if timecode_offset_secs > 0.0 {
        let delay_ms = (timecode_offset_secs * 1000.0) as i64;
        audio_filters.push(format!("adelay={}|{}", delay_ms, delay_ms));
    }

    // Gain adjustment for normalization
    if let Some(gain) = audio_gain_db {
        if gain.abs() > 0.001 {
            audio_filters.push(format!("volume={}dB", gain));
        }
    }

    // Silence compliance (UK broadcast: 6 frames silence at head/tail + fade)
    if let Some((duration, silence_ms, fade_ms)) = compliance {
        audio_filters.extend(build_compliance_filters(duration, silence_ms, fade_ms));
    }

    if !audio_filters.is_empty() {
        args.extend(["-af".to_string(), audio_filters.join(",")]);
    }

    // Audio codec settings
    // Note: stream copy (-c:a copy) is incompatible with audio filters.
    // If filters are applied (normalization, offset), fall back to PCM encoding.
    let has_audio_filters = !audio_filters.is_empty();

    match settings.audio_format {
        AudioFormatOption::Original => {
            if has_audio_filters {
                // Can't stream copy with filters — use lossless PCM instead
                args.extend(["-c:a".to_string(), "pcm_s24le".to_string()]);
            } else {
                args.extend(["-c:a".to_string(), "copy".to_string()]);
            }
        }
        AudioFormatOption::Aac => {
            args.extend(["-c:a".to_string(), "aac".to_string()]);
            args.extend(["-b:a".to_string(), format!("{}", settings.aac_bitrate)]);
        }
    }

    // Shortest: trim output to shortest stream
    args.push("-shortest".to_string());

    // Output file
    args.push(output_path.to_string());

    args
}

/// Build the ffmpeg command for audio-only processing (normalize + export)
pub fn build_audio_only_command(
    audio_path: &str,
    output_path: &str,
    settings: &ExportSettings,
    audio_gain_db: Option<f64>,
    compliance: Option<(f64, f64, f64)>,
) -> Vec<String> {
    let mut args: Vec<String> = Vec::new();

    args.extend(["-y".to_string()]);
    args.extend(["-i".to_string(), audio_path.to_string()]);

    // Strip any video streams (e.g. album art in M4A/MP3)
    args.push("-vn".to_string());

    // Audio filter chain
    let mut audio_filters: Vec<String> = Vec::new();

    if let Some(gain) = audio_gain_db {
        if gain.abs() > 0.001 {
            audio_filters.push(format!("volume={}dB", gain));
        }
    }

    // Silence compliance
    if let Some((duration, silence_ms, fade_ms)) = compliance {
        audio_filters.extend(build_compliance_filters(duration, silence_ms, fade_ms));
    }

    if !audio_filters.is_empty() {
        args.extend(["-af".to_string(), audio_filters.join(",")]);
    }

    let has_audio_filters = !audio_filters.is_empty();

    // Choose codec based on output format and container compatibility
    let output_ext = std::path::Path::new(output_path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match settings.audio_format {
        AudioFormatOption::Original => {
            if has_audio_filters {
                // Can't stream copy with filters — pick a lossless codec
                // that matches the output container
                let codec = match output_ext.as_str() {
                    "aif" | "aiff" => "pcm_s24be",
                    "flac" => "flac",
                    "m4a" | "mp4" => "alac",
                    _ => "pcm_s24le", // WAV, BWF, and others
                };
                args.extend(["-c:a".to_string(), codec.to_string()]);
            } else {
                args.extend(["-c:a".to_string(), "copy".to_string()]);
            }
        }
        AudioFormatOption::Aac => {
            args.extend(["-c:a".to_string(), "aac".to_string()]);
            args.extend(["-b:a".to_string(), format!("{}", settings.aac_bitrate)]);
        }
    }

    args.push(output_path.to_string());
    args
}

/// Extract a thumbnail frame from a video file as a base64 JPEG data URL
pub fn extract_thumbnail(video_path: &str, duration_secs: f64) -> Option<String> {
    use base64::{Engine as _, engine::general_purpose::STANDARD};

    let ffmpeg = find_ffmpeg();
    // Seek to 10% of duration or 1 second, whichever is larger
    let seek_secs = (duration_secs * 0.1).max(1.0).min(duration_secs - 0.1);

    let output = Command::new(&ffmpeg)
        .args([
            "-ss", &format!("{:.2}", seek_secs),
            "-i", video_path,
            "-vframes", "1",
            "-vf", "scale=80:-1",
            "-f", "image2pipe",
            "-vcodec", "mjpeg",
            "-q:v", "8",
            "pipe:1",
        ])
        .output()
        .ok()?;

    if !output.status.success() || output.stdout.is_empty() {
        return None;
    }

    let b64 = STANDARD.encode(&output.stdout);
    Some(format!("data:image/jpeg;base64,{}", b64))
}

/// Check if the first/last N ms of audio contain non-silence.
/// Returns (head_has_audio, tail_has_audio, head_peak_db, tail_peak_db).
pub fn check_silence_compliance(audio_path: &str, duration_secs: f64, silence_ms: f64) -> Result<(bool, bool, f64, f64), String> {
    let ffmpeg = find_ffmpeg();
    let silence_secs = silence_ms / 1000.0;
    let threshold_db = -60.0; // anything above -60dB is considered non-silent

    // Check head
    let head_output = Command::new(&ffmpeg)
        .args([
            "-i", audio_path,
            "-af", &format!("atrim=0:{},astats=metadata=1:reset=1", silence_secs),
            "-f", "null", "-",
        ])
        .output()
        .map_err(|e| format!("Failed to check head silence: {}", e))?;

    let head_stderr = String::from_utf8_lossy(&head_output.stderr);
    let head_peak = parse_peak_from_astats(&head_stderr).unwrap_or(-120.0);
    let head_has_audio = head_peak > threshold_db;

    // Check tail
    let tail_start = (duration_secs - silence_secs).max(0.0);
    let tail_output = Command::new(&ffmpeg)
        .args([
            "-i", audio_path,
            "-af", &format!("atrim={}:{},astats=metadata=1:reset=1", tail_start, duration_secs),
            "-f", "null", "-",
        ])
        .output()
        .map_err(|e| format!("Failed to check tail silence: {}", e))?;

    let tail_stderr = String::from_utf8_lossy(&tail_output.stderr);
    let tail_peak = parse_peak_from_astats(&tail_stderr).unwrap_or(-120.0);
    let tail_has_audio = tail_peak > threshold_db;

    Ok((head_has_audio, tail_has_audio, head_peak, tail_peak))
}

/// Parse the peak level from ffmpeg astats output
fn parse_peak_from_astats(stderr: &str) -> Option<f64> {
    // Look for "Peak level dB:" in the astats output
    for line in stderr.lines() {
        if line.contains("Peak level dB:") {
            let val = line.split(':').last()?.trim();
            if val == "-inf" {
                return Some(-120.0);
            }
            return val.parse::<f64>().ok();
        }
    }
    None
}

/// Build silence compliance audio filters:
/// - Replace first/last silence_ms with digital silence
/// - Apply fade_ms fade up/down to prevent clicks
pub fn build_compliance_filters(duration_secs: f64, silence_ms: f64, fade_ms: f64) -> Vec<String> {
    let silence_secs = silence_ms / 1000.0;
    let fade_secs = fade_ms / 1000.0;

    let mut filters = Vec::new();
    // Mute the first silence_ms
    filters.push(format!("volume=enable='lt(t,{:.4})':volume=0", silence_secs));
    // Mute the last silence_ms
    filters.push(format!("volume=enable='gt(t,{:.4})':volume=0", duration_secs - silence_secs));
    // Fade in after the head silence
    filters.push(format!("afade=t=in:st={:.4}:d={:.4}", silence_secs, fade_secs));
    // Fade out before the tail silence
    filters.push(format!("afade=t=out:st={:.4}:d={:.4}", duration_secs - silence_secs - fade_secs, fade_secs));

    filters
}

/// Build ffmpeg command to measure loudness (used as fallback if Rust measurement has issues)
pub fn build_loudness_measure_command(audio_path: &str) -> Vec<String> {
    vec![
        "-i".to_string(),
        audio_path.to_string(),
        "-af".to_string(),
        "loudnorm=print_format=json".to_string(),
        "-f".to_string(),
        "null".to_string(),
        "-".to_string(),
    ]
}

/// Parse loudness measurement from ffmpeg loudnorm output
pub fn parse_loudness_output(stderr: &str) -> Option<(f64, f64)> {
    // ffmpeg loudnorm outputs JSON in stderr
    let json_start = stderr.rfind('{')?;
    let json_end = stderr.rfind('}')? + 1;
    let json_str = &stderr[json_start..json_end];

    let json: serde_json::Value = serde_json::from_str(json_str).ok()?;

    let lufs = json["input_i"]
        .as_str()
        .and_then(|s| s.parse::<f64>().ok())?;
    let true_peak = json["input_tp"]
        .as_str()
        .and_then(|s| s.parse::<f64>().ok())?;

    Some((lufs, true_peak))
}

/// Run an ffmpeg command and return success/failure.
/// On failure, removes any 0-byte output file left behind by ffmpeg's -y flag.
pub fn run_ffmpeg(args: &[String]) -> Result<(), String> {
    let ffmpeg = find_ffmpeg();

    log::info!("Running: {} {}", ffmpeg, args.join(" "));

    // The output path is the last argument
    let output_path = args.last().map(|s| s.as_str());

    let output = Command::new(&ffmpeg)
        .args(args)
        .output()
        .map_err(|e| format!("Failed to run ffmpeg: {}", e))?;

    if !output.status.success() {
        // Clean up empty/broken output file left by -y flag
        if let Some(path) = output_path {
            let p = std::path::Path::new(path);
            if p.exists() {
                if let Ok(meta) = p.metadata() {
                    if meta.len() == 0 {
                        let _ = std::fs::remove_file(p);
                    }
                }
            }
        }
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("ffmpeg failed: {}", stderr));
    }

    // Also check that the output file was actually created and is non-empty
    if let Some(path) = output_path {
        let p = std::path::Path::new(path);
        if !p.exists() || p.metadata().map(|m| m.len() == 0).unwrap_or(true) {
            let _ = std::fs::remove_file(p);
            return Err("ffmpeg produced an empty output file".to_string());
        }
    }

    Ok(())
}

/// Measure loudness using ffmpeg's loudnorm filter
pub fn measure_loudness_ffmpeg(audio_path: &str) -> Result<(f64, f64), String> {
    let ffmpeg = find_ffmpeg();
    let args = build_loudness_measure_command(audio_path);

    let output = Command::new(&ffmpeg)
        .args(&args)
        .output()
        .map_err(|e| format!("Failed to run ffmpeg for loudness measurement: {}", e))?;

    let stderr = String::from_utf8_lossy(&output.stderr);
    parse_loudness_output(&stderr)
        .ok_or_else(|| "Failed to parse loudness measurement output".to_string())
}
