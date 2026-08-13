use std::process::Command;
use std::sync::OnceLock;

use crate::models::*;

/// Create a Command that hides the console window on Windows.
/// On macOS/Linux this is just a normal Command.
pub fn silent_command(program: &str) -> Command {
    // `mut` is needed on Windows (creation_flags below mutates cmd); on other
    // platforms that branch is compiled out, so silence the unused-mut warning.
    #[cfg_attr(not(target_os = "windows"), allow(unused_mut))]
    let mut cmd = Command::new(program);
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    cmd
}

/// Find a binary by name, checking the bundled sidecar location first,
/// then common system installation paths.
/// Result is cached so subprocess is only spawned once per binary per session.
pub fn find_binary(name: &str) -> String {
    // Check for bundled sidecar binary next to the executable first
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let sidecar = if cfg!(target_os = "windows") {
                exe_dir.join(format!("{}.exe", name))
            } else {
                exe_dir.join(name)
            };
            if sidecar.exists() {
                if let Some(path_str) = sidecar.to_str() {
                    if silent_command(path_str).arg("-version").output().is_ok() {
                        log::info!("{}: using bundled sidecar at {}", name, path_str);
                        return path_str.to_string();
                    }
                }
            }
        }
    }

    // Fall back to system-installed binaries
    let candidates = if cfg!(target_os = "windows") {
        vec![
            name.to_string(),
            format!("C:\\ffmpeg\\bin\\{}.exe", name),
        ]
    } else {
        vec![
            name.to_string(),
            format!("/usr/local/bin/{}", name),
            format!("/opt/homebrew/bin/{}", name),
            format!("/usr/bin/{}", name),
        ]
    };

    for candidate in &candidates {
        if silent_command(candidate).arg("-version").output().is_ok() {
            log::info!("{}: using system binary at {}", name, candidate);
            return candidate.clone();
        }
    }

    name.to_string()
}

static FFMPEG_PATH: OnceLock<String> = OnceLock::new();
static FFPROBE_PATH: OnceLock<String> = OnceLock::new();

/// Find the ffmpeg binary (cached)
pub fn find_ffmpeg() -> String {
    FFMPEG_PATH.get_or_init(|| find_binary("ffmpeg")).clone()
}

/// Find the ffprobe binary (cached)
pub fn find_ffprobe() -> String {
    FFPROBE_PATH.get_or_init(|| find_binary("ffprobe")).clone()
}

/// Check if ffmpeg is available
pub fn is_ffmpeg_available() -> bool {
    let ffmpeg = find_ffmpeg();
    silent_command(&ffmpeg).arg("-version").output().is_ok()
}

/// Get ffmpeg version string
pub fn get_ffmpeg_version() -> Option<String> {
    let ffmpeg = find_ffmpeg();
    let output = silent_command(&ffmpeg).arg("-version").output().ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout.lines().next().map(|l| l.to_string())
}

/// Frames to fade the audio over when the user picks the "fade" length fix.
pub const LENGTH_FADE_FRAMES: f64 = 12.0;
/// fps to assume when the video's real frame rate is unknown.
const FALLBACK_FPS: f64 = 25.0;
/// How much longer (seconds) the audio must be before a length fix kicks in.
const LENGTH_MISMATCH_TOLERANCE: f64 = 0.04;

/// A slate card to prepend to the video: a pre-rendered image (the frontend
/// draws the text — the bundled ffmpeg has no freetype) shown for
/// `duration_secs` with silence underneath.
#[derive(Debug, Clone, PartialEq)]
pub struct SlateSpec {
    pub image_path: String,
    pub duration_secs: f64,
}

#[allow(clippy::too_many_arguments)]
pub fn build_mux_command(
    video_path: &str,
    audio_path: &str,
    output_path: &str,
    settings: &ExportSettings,
    audio_gain_db: Option<f64>,
    timecode_offset_secs: f64,
    compliance: Option<(f64, f64, f64)>, // (duration_secs, silence_ms, fade_ms)
    length_fix: LengthFix,
    video_duration_secs: f64,
    audio_duration_secs: f64,
    video_fps: Option<f64>,
    slate: Option<&SlateSpec>,
) -> Vec<String> {
    let mut args: Vec<String> = Vec::new();

    // A length fix only applies when the audio genuinely runs past the video.
    // Otherwise fall back to a plain cut (which -shortest already handles).
    let audio_longer =
        audio_duration_secs > video_duration_secs + LENGTH_MISMATCH_TOLERANCE;
    let effective_fix = if audio_longer { length_fix } else { LengthFix::Cut };
    let freeze = effective_fix == LengthFix::Freeze;
    let fps = video_fps.filter(|f| *f > 0.0).unwrap_or(FALLBACK_FPS);

    // Input files
    args.extend(["-y".to_string()]); // Overwrite output
    args.extend(["-i".to_string(), video_path.to_string()]);
    args.extend(["-i".to_string(), audio_path.to_string()]);
    if let Some(s) = slate {
        // Input 2: the slate image looped at the video's frame rate for the
        // slate duration. These are input options, so they precede its -i.
        args.extend([
            "-loop".to_string(), "1".to_string(),
            "-framerate".to_string(), format!("{:.5}", fps),
            "-t".to_string(), format!("{:.4}", s.duration_secs),
            "-i".to_string(), s.image_path.clone(),
        ]);
    }

    // Video path. A slate needs filter_complex (concat), and freeze needs a
    // filter too — when both apply, the freeze tpad folds into the same graph
    // (ffmpeg forbids mixing -vf with -filter_complex).
    let freeze_pad = (audio_duration_secs - video_duration_secs).max(0.0);
    if slate.is_some() {
        let main_chain = if freeze {
            format!(
                "tpad=stop_mode=clone:stop_duration={:.4},format=yuv420p,setsar=1",
                freeze_pad
            )
        } else {
            "format=yuv420p,setsar=1".to_string()
        };
        args.extend([
            "-filter_complex".to_string(),
            format!(
                "[2:v]format=yuv420p,setsar=1[sl];[0:v]{}[mv];[sl][mv]concat=n=2:v=1:a=0[v]",
                main_chain
            ),
        ]);
        args.extend(["-map".to_string(), "[v]".to_string()]);
    } else {
        args.extend(["-map".to_string(), "0:v:0".to_string()]);
        // Freeze: pad the video by cloning its final frame to match the audio.
        if freeze {
            args.extend([
                "-vf".to_string(),
                format!("tpad=stop_mode=clone:stop_duration={:.4}", freeze_pad),
            ]);
        }
    }
    args.extend(["-map".to_string(), "1:a:0".to_string()]);

    // Video codec settings. Slate (concat) and freeze (tpad) both filter the
    // stream, so it can't be copied and must re-encode.
    let reencode_video =
        slate.is_some() || freeze || settings.video_codec == VideoCodecOption::H264;
    if reencode_video {
        args.extend(["-c:v".to_string(), "libx264".to_string()]);
        args.extend(["-crf".to_string(), "18".to_string()]);
        args.extend(["-preset".to_string(), "medium".to_string()]);
        args.extend(["-pix_fmt".to_string(), "yuv420p".to_string()]);
    } else {
        args.extend(["-c:v".to_string(), "copy".to_string()]);
    }

    // Audio filter chain
    let mut audio_filters: Vec<String> = Vec::new();

    // Timecode offset (delay audio start)
    if timecode_offset_secs > 0.0 {
        let delay_ms = (timecode_offset_secs * 1000.0) as i64;
        audio_filters.push(format!("adelay={}|{}", delay_ms, delay_ms));
    }

    // Normalization gain (computed from the ebur128 measurement upstream)
    if let Some(gain) = audio_gain_db {
        if gain.abs() > 0.001 {
            audio_filters.push(format!("volume={}dB", gain));
        }
    }

    // Silence compliance (UK broadcast: 6 frames silence at head/tail + fade)
    if let Some((duration, silence_ms, fade_ms)) = compliance {
        audio_filters.extend(build_compliance_filters(duration, silence_ms, fade_ms));
    }

    // Fade length fix: the output stays video-length (via -shortest), but the
    // audio fades out over the final 12 frames instead of stopping dead.
    if effective_fix == LengthFix::Fade {
        let fade_secs = (LENGTH_FADE_FRAMES / fps).min(video_duration_secs);
        let start = (video_duration_secs - fade_secs).max(0.0);
        audio_filters.push(format!("afade=t=out:st={:.4}:d={:.4}", start, fade_secs));
    }

    // Slate: push the programme audio back by the slate duration so it still
    // starts with the original first frame of picture. Applied LAST — the
    // compliance/fade filters above time against the original audio timeline.
    if let Some(s) = slate {
        let delay_ms = (s.duration_secs * 1000.0).round() as i64;
        audio_filters.push(format!("adelay={}|{}", delay_ms, delay_ms));
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

    // Trim to the shortest stream — EXCEPT when freezing, where the whole point
    // is to run to the (longer) audio length with the video held on its last frame.
    if !freeze {
        args.push("-shortest".to_string());
    }

    // Output file
    args.push(output_path.to_string());

    args
}

/// Build the ffmpeg command for audio-only processing (normalize + export)
/// "Clock" delivery handles added to the head/tail of an audio-only export.
pub const CLOCK_HEAD_SECS: f64 = 10.0;
pub const CLOCK_TAIL_SECS: f64 = 5.0;

pub fn build_audio_only_command(
    audio_path: &str,
    output_path: &str,
    settings: &ExportSettings,
    audio_gain_db: Option<f64>,
    compliance: Option<(f64, f64, f64)>,
    clock: bool,
) -> Vec<String> {
    let mut args: Vec<String> = Vec::new();

    args.extend(["-y".to_string()]);
    args.extend(["-i".to_string(), audio_path.to_string()]);

    // Strip any video streams (e.g. album art in M4A/MP3)
    args.push("-vn".to_string());

    // Audio filter chain
    let mut audio_filters: Vec<String> = Vec::new();

    // Normalization gain (computed from the ebur128 measurement upstream)
    if let Some(gain) = audio_gain_db {
        if gain.abs() > 0.001 {
            audio_filters.push(format!("volume={}dB", gain));
        }
    }

    // Silence compliance
    if let Some((duration, silence_ms, fade_ms)) = compliance {
        audio_filters.extend(build_compliance_filters(duration, silence_ms, fade_ms));
    }

    // Clock delivery handles: prepend CLOCK_HEAD_SECS and append CLOCK_TAIL_SECS
    // of pure silence. Applied last so the handles sit outside any normalisation
    // / compliance processing done on the programme audio.
    if clock {
        audio_filters.push(format!("adelay={}:all=1", (CLOCK_HEAD_SECS * 1000.0) as i64));
        audio_filters.push(format!("apad=pad_dur={}", CLOCK_TAIL_SECS));
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

    // Only encode AAC when the output container can actually hold it. Forcing
    // AAC into a .wav (e.g. when the AAC setting lingers from a video layback)
    // produces a file that won't open, so fall back to a container-appropriate
    // codec instead.
    let aac_container = matches!(output_ext.as_str(), "m4a" | "mp4" | "aac");

    if settings.audio_format == AudioFormatOption::Aac && aac_container {
        args.extend(["-c:a".to_string(), "aac".to_string()]);
        args.extend(["-b:a".to_string(), format!("{}", settings.aac_bitrate)]);
    } else if has_audio_filters {
        // Can't stream copy with filters — pick a lossless codec for the container
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

    args.push(output_path.to_string());
    args
}

/// Build the ffmpeg command to transcode a video into an Apple ProRes 422 .mov
/// "working file" (with PCM audio) — a smooth-playing guide picture for Pro Tools.
/// `profile` is the prores_ks profile: 0 = Proxy, 1 = LT, 2 = 422, 3 = HQ.
/// Slate a solo video (no replacement audio): prepend the card and keep the
/// video's OWN soundtrack, delayed by the slate duration. Re-encodes to H.264
/// with PCM audio in a .mov, mirroring the main slate path.
pub fn build_solo_slate_command(
    video_path: &str,
    output_path: &str,
    slate: &SlateSpec,
    video_fps: Option<f64>,
    has_audio: bool,
) -> Vec<String> {
    let fps = video_fps.filter(|f| *f > 0.0).unwrap_or(FALLBACK_FPS);
    let mut args: Vec<String> = vec![
        "-y".to_string(),
        "-i".to_string(), video_path.to_string(),
        "-loop".to_string(), "1".to_string(),
        "-framerate".to_string(), format!("{:.5}", fps),
        "-t".to_string(), format!("{:.4}", slate.duration_secs),
        "-i".to_string(), slate.image_path.clone(),
        "-filter_complex".to_string(),
        "[1:v]format=yuv420p,setsar=1[sl];[0:v]format=yuv420p,setsar=1[mv];[sl][mv]concat=n=2:v=1:a=0[v]".to_string(),
        "-map".to_string(), "[v]".to_string(),
    ];
    if has_audio {
        let delay_ms = (slate.duration_secs * 1000.0).round() as i64;
        args.extend([
            "-map".to_string(), "0:a:0".to_string(),
            "-af".to_string(), format!("adelay={}|{}", delay_ms, delay_ms),
            "-c:a".to_string(), "pcm_s24le".to_string(),
        ]);
    }
    args.extend([
        "-c:v".to_string(), "libx264".to_string(),
        "-crf".to_string(), "18".to_string(),
        "-preset".to_string(), "medium".to_string(),
        "-pix_fmt".to_string(), "yuv420p".to_string(),
        output_path.to_string(),
    ]);
    args
}

pub fn build_prores_command(video_path: &str, output_path: &str, profile: u8) -> Vec<String> {
    vec![
        "-y".to_string(),
        "-i".to_string(),
        video_path.to_string(),
        "-c:v".to_string(),
        "prores_ks".to_string(),
        "-profile:v".to_string(),
        profile.to_string(),
        "-vendor".to_string(),
        "apl0".to_string(),
        "-pix_fmt".to_string(),
        "yuv422p10le".to_string(),
        "-c:a".to_string(),
        "pcm_s16le".to_string(), // PCM audio imports cleanly into Pro Tools
        output_path.to_string(),
    ]
}

/// Extract a thumbnail frame from a video file as a base64 JPEG data URL
pub fn extract_thumbnail(video_path: &str, duration_secs: f64) -> Option<String> {
    use base64::{Engine as _, engine::general_purpose::STANDARD};

    let ffmpeg = find_ffmpeg();
    // Seek to 10% of duration or 1 second, whichever is larger
    let seek_secs = (duration_secs * 0.1).max(1.0).min(duration_secs - 0.1);

    let output = silent_command(&ffmpeg)
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
    let head_output = silent_command(&ffmpeg)
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
    let tail_output = silent_command(&ffmpeg)
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
            let val = line.split(':').next_back()?.trim();
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

// NOTE: the loudnorm two-pass machinery that used to live here was removed
// deliberately. loudnorm's internal measurement reads a few tenths lower than
// ebur128 (the meter QC and Pro Tools agree with), which made normalised
// output land ~0.2 LU hot. All levelling now goes through loudness::measure
// (ebur128) + a static volume gain. Don't reintroduce loudnorm.

/// Run an ffmpeg command and return success/failure.
/// On failure, removes any 0-byte output file left behind by ffmpeg's -y flag.
pub fn run_ffmpeg(args: &[String]) -> Result<(), String> {
    let ffmpeg = find_ffmpeg();

    log::info!("Running: {} {}", ffmpeg, args.join(" "));

    // The output path is the last argument
    let output_path = args.last().map(|s| s.as_str());

    let output = silent_command(&ffmpeg)
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

/// Parse an ffmpeg `out_time=HH:MM:SS.ffffff` value into seconds.
/// ffmpeg emits "N/A" before the first frame — that returns None.
pub fn parse_ffmpeg_time(s: &str) -> Option<f64> {
    let s = s.trim();
    if s.is_empty() || s == "N/A" {
        return None;
    }
    let mut parts = s.split(':');
    let h: f64 = parts.next()?.parse().ok()?;
    let m: f64 = parts.next()?.parse().ok()?;
    let sec: f64 = parts.next()?.parse().ok()?;
    Some(h * 3600.0 + m * 60.0 + sec)
}

/// Run an ffmpeg command, reporting progress (0.0–1.0) via `on_progress` as the
/// encode advances. `total_secs` is the media duration used to compute percent;
/// pass 0.0 if unknown (progress simply won't be reported, but the encode runs).
/// Behaves like `run_ffmpeg` for cleanup/validation on failure.
pub fn run_ffmpeg_with_progress(
    args: &[String],
    total_secs: f64,
    on_progress: impl Fn(f64),
) -> Result<(), String> {
    use std::io::{BufRead, BufReader, Read};
    use std::process::Stdio;

    let ffmpeg = find_ffmpeg();
    let output_path = args.last().map(|s| s.to_string());

    // Ask ffmpeg to stream machine-readable progress to stdout.
    let mut full: Vec<String> = vec![
        "-progress".to_string(),
        "pipe:1".to_string(),
        "-nostats".to_string(),
    ];
    full.extend(args.iter().cloned());

    log::info!("Running: {} {}", ffmpeg, full.join(" "));

    let mut child = silent_command(&ffmpeg)
        .args(&full)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to run ffmpeg: {}", e))?;

    // Drain stderr on a thread so its pipe never fills and deadlocks the encode.
    let stderr_handle = child.stderr.take().map(|mut s| {
        std::thread::spawn(move || {
            let mut buf = String::new();
            let _ = s.read_to_string(&mut buf);
            buf
        })
    });

    if let Some(stdout) = child.stdout.take() {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if let Some(rest) = line.strip_prefix("out_time=") {
                if total_secs > 0.0 {
                    if let Some(secs) = parse_ffmpeg_time(rest) {
                        // Hold just below 100% until the process actually exits.
                        on_progress((secs / total_secs).clamp(0.0, 0.99));
                    }
                }
            }
        }
    }

    let status = child
        .wait()
        .map_err(|e| format!("ffmpeg wait failed: {}", e))?;
    let stderr = stderr_handle
        .and_then(|h| h.join().ok())
        .unwrap_or_default();

    if !status.success() {
        // Clean up empty/broken output file left by -y flag
        if let Some(path) = &output_path {
            let p = std::path::Path::new(path);
            if p.exists() && p.metadata().map(|m| m.len() == 0).unwrap_or(false) {
                let _ = std::fs::remove_file(p);
            }
        }
        return Err(format!("ffmpeg failed: {}", stderr));
    }

    if let Some(path) = &output_path {
        let p = std::path::Path::new(path);
        if !p.exists() || p.metadata().map(|m| m.len() == 0).unwrap_or(true) {
            let _ = std::fs::remove_file(p);
            return Err("ffmpeg produced an empty output file".to_string());
        }
    }

    on_progress(1.0);
    Ok(())
}

/// Measure loudness using ffmpeg's ebur128 filter (ITU-R BS.1770-4 compliant)
/// This is more precise than loudnorm's built-in measurement
pub fn measure_loudness_ffmpeg(audio_path: &str) -> Result<(f64, f64), String> {
    let ffmpeg = find_ffmpeg();

    // Use ebur128 for integrated loudness + true peak measurement
    let args = vec![
        "-i".to_string(),
        audio_path.to_string(),
        "-af".to_string(),
        "ebur128=peak=true".to_string(),
        "-f".to_string(),
        "null".to_string(),
        "-".to_string(),
    ];

    let output = silent_command(&ffmpeg)
        .args(&args)
        .output()
        .map_err(|e| format!("Failed to run ffmpeg for loudness measurement: {}", e))?;

    let stderr = String::from_utf8_lossy(&output.stderr);
    parse_ebur128_output(&stderr)
        .ok_or_else(|| "Failed to parse ebur128 measurement output".to_string())
}

/// Parse the summary block from ebur128 filter output.
/// Only reads values from the "Summary:" section at the end to avoid
/// picking up per-frame values from the running output.
pub fn parse_ebur128_output(stderr: &str) -> Option<(f64, f64)> {
    // Find the Summary section
    let summary_start = stderr.find("Summary:")?;
    let summary = &stderr[summary_start..];

    let mut integrated_lufs: Option<f64> = None;
    let mut true_peak_dbtp: Option<f64> = None;

    for line in summary.lines() {
        let trimmed = line.trim();
        // "I:         -31.9 LUFS"
        if trimmed.starts_with("I:") && trimmed.contains("LUFS") {
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            if parts.len() >= 2 {
                if let Ok(v) = parts[1].parse::<f64>() {
                    integrated_lufs = Some(v);
                }
            }
        }
        // "Peak:      -15.3 dBFS"
        if trimmed.starts_with("Peak:") && trimmed.contains("dBFS") {
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            if parts.len() >= 2 {
                if let Ok(v) = parts[1].parse::<f64>() {
                    true_peak_dbtp = Some(v);
                }
            }
        }
    }

    match (integrated_lufs, true_peak_dbtp) {
        (Some(lufs), Some(tp)) => Some((lufs, tp)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_settings() -> ExportSettings {
        ExportSettings::default()
    }

    fn aac_settings() -> ExportSettings {
        ExportSettings {
            audio_format: AudioFormatOption::Aac,
            aac_bitrate: 320000,
            ..ExportSettings::default()
        }
    }

    // ── build_mux_command ──

    #[test]
    fn test_mux_command_basic_copy() {
        let args = build_mux_command(
            "/video.mov", "/audio.wav", "/out.mov",
            &default_settings(), None, 0.0, None,
            LengthFix::Cut, 30.0, 30.0, Some(25.0), None,
        );
        assert!(args.contains(&"-y".to_string()));
        assert!(args.contains(&"/video.mov".to_string()));
        assert!(args.contains(&"/audio.wav".to_string()));
        assert!(args.contains(&"copy".to_string())); // video copy
        assert!(args.contains(&"-shortest".to_string()));
        assert_eq!(args.last().unwrap(), "/out.mov");
        // No -af when no filters
        assert!(!args.contains(&"-af".to_string()));
    }

    #[test]
    fn test_mux_command_with_gain() {
        let args = build_mux_command(
            "/video.mov", "/audio.wav", "/out.mov",
            &default_settings(), Some(3.5), 0.0, None,
            LengthFix::Cut, 30.0, 30.0, Some(25.0), None,
        );
        assert!(args.contains(&"-af".to_string()));
        let af_idx = args.iter().position(|a| a == "-af").unwrap();
        assert_eq!(args[af_idx + 1], "volume=3.5dB");
        // With filters, codec should be pcm_s24le not copy
        assert!(args.contains(&"pcm_s24le".to_string()));
    }

    #[test]
    fn test_mux_command_with_timecode_offset() {
        let args = build_mux_command(
            "/video.mov", "/audio.wav", "/out.mov",
            &default_settings(), None, 0.5, None,
            LengthFix::Cut, 30.0, 30.0, Some(25.0), None,
        );
        let af_idx = args.iter().position(|a| a == "-af").unwrap();
        assert!(args[af_idx + 1].contains("adelay=500|500"));
    }

    #[test]
    fn test_mux_command_h264_encoding() {
        let settings = ExportSettings {
            video_codec: VideoCodecOption::H264,
            ..ExportSettings::default()
        };
        let args = build_mux_command(
            "/video.mov", "/audio.wav", "/out.mov",
            &settings, None, 0.0, None,
            LengthFix::Cut, 30.0, 30.0, Some(25.0), None,
        );
        assert!(args.contains(&"libx264".to_string()));
        assert!(args.contains(&"yuv420p".to_string()));
    }

    #[test]
    fn test_mux_command_aac_audio() {
        let args = build_mux_command(
            "/video.mov", "/audio.wav", "/out.mp4",
            &aac_settings(), None, 0.0, None,
            LengthFix::Cut, 30.0, 30.0, Some(25.0), None,
        );
        assert!(args.contains(&"aac".to_string()));
        assert!(args.contains(&"320000".to_string()));
    }

    #[test]
    fn test_mux_command_with_compliance() {
        let args = build_mux_command(
            "/video.mov", "/audio.wav", "/out.mov",
            &default_settings(), None, 0.0,
            Some((30.0, 240.0, 5.0)),
            LengthFix::Cut, 30.0, 30.0, Some(25.0), None,
        );
        let af_idx = args.iter().position(|a| a == "-af").unwrap();
        let filters = &args[af_idx + 1];
        assert!(filters.contains("volume=enable="));
        assert!(filters.contains("afade=t=in"));
        assert!(filters.contains("afade=t=out"));
    }

    // ── length fixes (audio longer than video) ──

    #[test]
    fn test_length_fix_cut_is_plain_shortest() {
        // Audio longer, fix = Cut: just -shortest, no fade, video still copied.
        let args = build_mux_command(
            "/video.mov", "/audio.wav", "/out.mov",
            &default_settings(), None, 0.0, None,
            LengthFix::Cut, 20.0, 30.0, Some(25.0), None,
        );
        assert!(args.contains(&"-shortest".to_string()));
        assert!(args.contains(&"copy".to_string()));
        assert!(!args.contains(&"-af".to_string()), "cut adds no audio filter");
        assert!(!args.contains(&"-vf".to_string()), "cut adds no video filter");
    }

    #[test]
    fn test_length_fix_fade_adds_afade_out() {
        // Audio longer, fix = Fade: 12-frame fade out ending at the video's end.
        let args = build_mux_command(
            "/video.mov", "/audio.wav", "/out.mov",
            &default_settings(), None, 0.0, None,
            LengthFix::Fade, 20.0, 30.0, Some(25.0), None,
        );
        let af_idx = args.iter().position(|a| a == "-af").unwrap();
        let filters = &args[af_idx + 1];
        // 12 frames @ 25fps = 0.48s, so fade starts at 20 - 0.48 = 19.52.
        assert!(filters.contains("afade=t=out:st=19.5200:d=0.4800"), "got {filters}");
        assert!(args.contains(&"-shortest".to_string()), "fade keeps video length");
        // Audio filter present -> lossless PCM, not stream copy.
        assert!(args.contains(&"pcm_s24le".to_string()));
    }

    #[test]
    fn test_length_fix_freeze_pads_video_and_reencodes() {
        // Audio longer, fix = Freeze: hold last frame, re-encode, no -shortest.
        let args = build_mux_command(
            "/video.mov", "/audio.wav", "/out.mov",
            &default_settings(), None, 0.0, None,
            LengthFix::Freeze, 20.0, 30.0, Some(25.0), None,
        );
        let vf_idx = args.iter().position(|a| a == "-vf").expect("freeze sets -vf");
        // Pad = 30 - 20 = 10s of cloned final frame.
        assert!(args[vf_idx + 1].contains("tpad=stop_mode=clone:stop_duration=10.0000"), "got {}", args[vf_idx + 1]);
        assert!(args.contains(&"libx264".to_string()), "freeze must re-encode video");
        assert!(!args.contains(&"-shortest".to_string()), "freeze runs to audio length");
    }

    #[test]
    fn test_length_fix_ignored_when_audio_not_longer() {
        // Fix requested but audio isn't actually longer -> behaves like a cut.
        let args = build_mux_command(
            "/video.mov", "/audio.wav", "/out.mov",
            &default_settings(), None, 0.0, None,
            LengthFix::Freeze, 30.0, 30.0, Some(25.0), None,
        );
        assert!(args.contains(&"copy".to_string()), "no re-encode when not longer");
        assert!(!args.contains(&"-vf".to_string()));
        assert!(args.contains(&"-shortest".to_string()));
    }

    // ── slate ──

    #[test]
    fn test_slate_concats_delays_audio_and_reencodes() {
        let slate = SlateSpec { image_path: "/tmp/slate.png".into(), duration_secs: 5.0 };
        let args = build_mux_command(
            "/video.mov", "/audio.wav", "/out.mov",
            &default_settings(), None, 0.0, None,
            LengthFix::Cut, 30.0, 30.0, Some(25.0), Some(&slate),
        );
        // Slate image is a third input, looped for its duration at video fps.
        assert!(args.contains(&"/tmp/slate.png".to_string()));
        assert!(args.contains(&"-loop".to_string()));
        let fc_idx = args.iter().position(|a| a == "-filter_complex").expect("slate uses filter_complex");
        assert!(args[fc_idx + 1].contains("concat=n=2:v=1:a=0"), "got {}", args[fc_idx + 1]);
        // Video must re-encode; audio is pushed back by the slate duration.
        assert!(args.contains(&"libx264".to_string()));
        let af_idx = args.iter().position(|a| a == "-af").expect("slate delays audio");
        assert!(args[af_idx + 1].contains("adelay=5000|5000"), "got {}", args[af_idx + 1]);
        // Programme video is mapped through the graph, not directly.
        assert!(args.contains(&"[v]".to_string()));
        assert!(!args.contains(&"0:v:0".to_string()));
    }

    #[test]
    fn test_slate_with_freeze_folds_tpad_into_graph() {
        // Slate + freeze together: tpad must live inside filter_complex (ffmpeg
        // forbids mixing -vf and -filter_complex), and -shortest is dropped.
        let slate = SlateSpec { image_path: "/tmp/slate.png".into(), duration_secs: 3.0 };
        let args = build_mux_command(
            "/video.mov", "/audio.wav", "/out.mov",
            &default_settings(), None, 0.0, None,
            LengthFix::Freeze, 20.0, 30.0, Some(25.0), Some(&slate),
        );
        assert!(!args.contains(&"-vf".to_string()));
        let fc_idx = args.iter().position(|a| a == "-filter_complex").unwrap();
        assert!(args[fc_idx + 1].contains("tpad=stop_mode=clone:stop_duration=10.0000"), "got {}", args[fc_idx + 1]);
        assert!(!args.contains(&"-shortest".to_string()));
    }

    // ── build_audio_only_command ──

    #[test]
    fn test_audio_only_basic() {
        let args = build_audio_only_command(
            "/audio.wav", "/out.wav",
            &default_settings(), None, None, false,
        );
        assert!(args.contains(&"-vn".to_string())); // strip video
        assert!(args.contains(&"copy".to_string())); // no filters = copy
        assert!(!args.contains(&"-af".to_string()));
    }

    #[test]
    fn test_audio_only_with_gain_wav() {
        let args = build_audio_only_command(
            "/audio.wav", "/out.wav",
            &default_settings(), Some(-2.5), None, false,
        );
        let af_idx = args.iter().position(|a| a == "-af").unwrap();
        assert_eq!(args[af_idx + 1], "volume=-2.5dB");
        assert!(args.contains(&"pcm_s24le".to_string()));
    }

    #[test]
    fn test_audio_only_with_gain_aiff() {
        let args = build_audio_only_command(
            "/audio.aiff", "/out.aiff",
            &default_settings(), Some(1.0), None, false,
        );
        assert!(args.contains(&"pcm_s24be".to_string()));
    }

    #[test]
    fn test_audio_only_tiny_gain_ignored() {
        let args = build_audio_only_command(
            "/audio.wav", "/out.wav",
            &default_settings(), Some(0.0005), None, false,
        );
        // Gain too small — should be treated as no-op
        assert!(!args.contains(&"-af".to_string()));
        assert!(args.contains(&"copy".to_string()));
    }

    #[test]
    fn test_audio_only_aac_setting_never_breaks_wav() {
        // Regression: the AAC audio-format setting (a video-mux concern) must NOT
        // be forced into a .wav output — AAC-in-WAV produces a file that won't open.
        let no_gain = build_audio_only_command(
            "/audio.wav", "/out.wav", &aac_settings(), None, None, false,
        );
        assert!(!no_gain.contains(&"aac".to_string()), "must not put AAC in a .wav");
        assert!(no_gain.contains(&"copy".to_string()), "no filters -> stream copy");

        let with_gain = build_audio_only_command(
            "/audio.wav", "/out.wav", &aac_settings(), Some(-3.0), None, false,
        );
        assert!(!with_gain.contains(&"aac".to_string()));
        assert!(with_gain.contains(&"pcm_s24le".to_string()), "filtered .wav -> PCM");
    }

    #[test]
    fn test_audio_only_aac_to_m4a_still_uses_aac() {
        // AAC into a container that supports it is fine and stays AAC.
        let args = build_audio_only_command(
            "/audio.m4a", "/out.m4a", &aac_settings(), None, None, false,
        );
        assert!(args.contains(&"aac".to_string()), "m4a supports AAC");
        assert!(args.contains(&"320000".to_string()));
    }

    #[test]
    fn test_audio_only_clock_adds_handles() {
        // Clock delivery: prepend 10s and append 5s of silence via adelay/apad.
        let args = build_audio_only_command(
            "/audio.wav", "/out.wav", &default_settings(), None, None, true,
        );
        let af_idx = args.iter().position(|a| a == "-af").unwrap();
        let filters = &args[af_idx + 1];
        assert!(filters.contains("adelay=10000:all=1"), "10s head silence");
        assert!(filters.contains("apad=pad_dur=5"), "5s tail silence");
        // Filtering forces a re-encode, never a stream copy.
        assert!(!args.contains(&"copy".to_string()));
    }

    #[test]
    fn test_prores_command() {
        let args = build_prores_command("/clip.mp4", "/clip_ProRes_LT.mov", 1);
        assert!(args.contains(&"prores_ks".to_string()));
        let p = args.iter().position(|a| a == "-profile:v").unwrap();
        assert_eq!(args[p + 1], "1");
        assert!(args.contains(&"yuv422p10le".to_string()));
        assert!(args.contains(&"pcm_s16le".to_string()));
        assert_eq!(args.last().unwrap(), "/clip_ProRes_LT.mov");
    }

    #[test]
    fn test_parse_ffmpeg_time() {
        assert_eq!(parse_ffmpeg_time("00:00:00.000000"), Some(0.0));
        assert_eq!(parse_ffmpeg_time("00:01:30.500000"), Some(90.5));
        assert_eq!(parse_ffmpeg_time("01:00:00.000000"), Some(3600.0));
        // ffmpeg emits N/A before the first frame, and junk should be ignored.
        assert_eq!(parse_ffmpeg_time("N/A"), None);
        assert_eq!(parse_ffmpeg_time(""), None);
        assert_eq!(parse_ffmpeg_time("nonsense"), None);
    }

    // ── parse_ebur128_output ──

    #[test]
    fn test_parse_ebur128_output_normal() {
        let stderr = r#"
[Parsed_ebur128_0 @ 0x600001234] t: 0.0  M: -120.7  S: -120.7
[Parsed_ebur128_0 @ 0x600001234] t: 0.4  M:  -25.3  S:  -25.3

Summary:

  Integrated loudness:
    I:         -23.0 LUFS
    Threshold: -33.0 LUFS

  Loudness range:
    LRA:         8.5 LU
    Threshold:  -43.0 LUFS
    LRA low:   -28.0 LUFS
    LRA high:  -19.5 LUFS

  True peak:
    Peak:       -1.5 dBFS
"#;
        let result = parse_ebur128_output(stderr);
        assert!(result.is_some());
        let (lufs, tp) = result.unwrap();
        assert!((lufs - (-23.0)).abs() < 0.01);
        assert!((tp - (-1.5)).abs() < 0.01);
    }

    #[test]
    fn test_parse_ebur128_output_no_summary() {
        let stderr = "some random ffmpeg output without summary";
        assert!(parse_ebur128_output(stderr).is_none());
    }

    #[test]
    fn test_parse_ebur128_output_missing_peak() {
        let stderr = "Summary:\n  Integrated loudness:\n    I: -23.0 LUFS\n";
        assert!(parse_ebur128_output(stderr).is_none());
    }

    // ── build_compliance_filters ──

    #[test]
    fn test_compliance_filters_structure() {
        let filters = build_compliance_filters(30.0, 240.0, 5.0);
        assert_eq!(filters.len(), 4);
        // Head silence (first 0.24s)
        assert!(filters[0].contains("volume=enable='lt(t,0.2400)':volume=0"));
        // Tail silence (last 0.24s of 30s)
        assert!(filters[1].contains("volume=enable='gt(t,29.7600)':volume=0"));
        // Fade in after head
        assert!(filters[2].contains("afade=t=in:st=0.2400:d=0.0050"));
        // Fade out before tail
        assert!(filters[3].contains("afade=t=out:st=29.7550:d=0.0050"));
    }

    // ── parse_peak_from_astats ──

    #[test]
    fn test_parse_peak_from_astats_normal() {
        let stderr = "Peak level dB:   -12.5\n";
        assert!((parse_peak_from_astats(stderr).unwrap() - (-12.5)).abs() < 0.01);
    }

    #[test]
    fn test_parse_peak_from_astats_silence() {
        let stderr = "Peak level dB: -inf\n";
        assert!((parse_peak_from_astats(stderr).unwrap() - (-120.0)).abs() < 0.01);
    }

    #[test]
    fn test_parse_peak_from_astats_missing() {
        assert!(parse_peak_from_astats("no peak info here").is_none());
    }
}
