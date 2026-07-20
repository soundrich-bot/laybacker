use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::models::*;
use crate::services::{ffmpeg, loudness};

/// Global cancellation flag
static CANCEL_FLAG: AtomicBool = AtomicBool::new(false);

pub fn request_cancel() {
    CANCEL_FLAG.store(true, Ordering::SeqCst);
}

pub fn reset_cancel() {
    CANCEL_FLAG.store(false, Ordering::SeqCst);
}

fn is_cancelled() -> bool {
    CANCEL_FLAG.load(Ordering::SeqCst)
}

/// Process a single matched pair into an output video
pub fn process_pair(
    pair: &MatchedPair,
    settings: &ExportSettings,
    progress_callback: impl Fn(ProcessingProgress),
) -> ProcessingResult {
    let pair_id = pair.id.clone();

    // Determine output path
    let output_path = resolve_output_path(pair, settings);

    // Ensure output directory exists
    if let Some(parent) = Path::new(&output_path).parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            return ProcessingResult {
                pair_id,
                success: false,
                output_path: None,
                error: Some(format!("Failed to create output directory: {}", e)),
                measured_lufs: None,
                measured_true_peak: None,
            };
        }
    }

    let mut measured_lufs = None;
    let mut measured_true_peak = None;
    let mut audio_gain_db: Option<f64> = None;

    // Step 1: Measure and calculate normalization if enabled
    log::info!("Processing pair: normalization_enabled={}, target_lufs={}, true_peak_limit={}",
        pair.normalization_enabled,
        pair.normalization_settings.target_lufs,
        pair.normalization_settings.true_peak_limit,
    );
    // NORM and Clock compose: the programme is levelled first, then the silent
    // clock handles go on the ends (the filter chain applies them last), so
    // clocking never alters the programme level.
    if pair.normalization_enabled {
        progress_callback(ProcessingProgress {
            pair_id: pair_id.clone(),
            state: "measuring".to_string(),
            progress: 0.1,
            message: "Measuring loudness...".to_string(),
        });

        // ONE meter, one method — for both LUFS and full-scale modes: measure
        // with ebur128 (the same meter QC and Pro Tools agree with) and apply a
        // plain static gain. A static gain shifts integrated loudness exactly,
        // with no dynamic processing. The previous loudnorm two-pass used
        // loudnorm's own internal measurement, which reads a few tenths lower
        // than ebur128 — outputs targeted at -23 landed at -22.8 by every
        // honest meter. calculate_gain handles both modes and caps the gain at
        // the true-peak ceiling.
        match loudness::measure(&pair.audio.path) {
            Ok(measurement) => {
                measured_lufs = Some(measurement.integrated_lufs);
                measured_true_peak = Some(measurement.true_peak_dbtp);

                let gain = loudness::calculate_gain(&measurement, &pair.normalization_settings);
                log::info!("Normalize: {:.2} LUFS, {:.2} dBTP. Gain: {:.3} dB",
                    measurement.integrated_lufs, measurement.true_peak_dbtp, gain);

                if gain.abs() > 0.001 {
                    audio_gain_db = Some(gain);
                }

                progress_callback(ProcessingProgress {
                    pair_id: pair_id.clone(),
                    state: "measured".to_string(),
                    progress: 0.3,
                    message: format!(
                        "Measured: {:.1} LUFS, {:.1} dBTP. Gain: {:.1} dB",
                        measurement.integrated_lufs,
                        measurement.true_peak_dbtp,
                        gain
                    ),
                });
            }
            Err(e) => {
                return ProcessingResult {
                    pair_id,
                    success: false,
                    output_path: None,
                    error: Some(format!("Loudness measurement failed: {}", e)),
                    measured_lufs: None,
                    measured_true_peak: None,
                };
            }
        }
    }

    // Step 2: Build and run ffmpeg command
    let is_audio_only = pair.video.is_none();
    progress_callback(ProcessingProgress {
        pair_id: pair_id.clone(),
        state: "muxing".to_string(),
        progress: 0.5,
        message: if is_audio_only { "Processing audio...".to_string() } else { "Muxing video and audio...".to_string() },
    });

    let compliance = if pair.silence_compliance {
        Some((pair.audio.duration_secs, pair.silence_ms, pair.fade_ms))
    } else {
        None
    };

    let args = if let Some(ref video) = pair.video {
        ffmpeg::build_mux_command(
            &video.path,
            &pair.audio.path,
            &output_path,
            settings,
            audio_gain_db,
            pair.timecode_offset_secs,
            compliance,
        )
    } else {
        ffmpeg::build_audio_only_command(
            &pair.audio.path,
            &output_path,
            settings,
            audio_gain_db,
            compliance,
            pair.clock_enabled,
        )
    };

    match ffmpeg::run_ffmpeg(&args) {
        Ok(()) => {
            progress_callback(ProcessingProgress {
                pair_id: pair_id.clone(),
                state: "complete".to_string(),
                progress: 1.0,
                message: "Complete!".to_string(),
            });

            ProcessingResult {
                pair_id,
                success: true,
                output_path: Some(output_path),
                error: None,
                measured_lufs,
                measured_true_peak,
            }
        }
        Err(e) => ProcessingResult {
            pair_id,
            success: false,
            output_path: None,
            error: Some(e),
            measured_lufs,
            measured_true_peak,
        },
    }
}

/// Process all pairs in batch
pub fn process_batch(
    pairs: &[MatchedPair],
    settings: &ExportSettings,
    progress_callback: impl Fn(ProcessingProgress) + Send + Sync + 'static,
) -> Vec<ProcessingResult> {
    let callback = Arc::new(progress_callback);
    let mut results = Vec::new();

    reset_cancel();

    for pair in pairs {
        if is_cancelled() {
            results.push(ProcessingResult {
                pair_id: pair.id.clone(),
                success: false,
                output_path: None,
                error: Some("Cancelled".to_string()),
                measured_lufs: None,
                measured_true_peak: None,
            });
            continue;
        }
        let cb = callback.clone();
        let result = process_pair(pair, settings, move |p| cb(p));
        results.push(result);
    }

    results
}

fn resolve_output_path(pair: &MatchedPair, settings: &ExportSettings) -> String {
    let directory = if settings.use_audio_file_location {
        Path::new(&pair.audio.path)
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| ".".to_string())
    } else if let Some(ref dir) = settings.output_directory {
        dir.clone()
    } else {
        Path::new(&pair.audio.path)
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| ".".to_string())
    };

    let filename = if pair.output_filename.is_empty() {
        if let Some(ref video) = pair.video {
            format!(
                "{}_with audio.{}",
                video.filename_no_ext,
                settings.output_extension()
            )
        } else {
            if pair.normalization_enabled {
                let spec = if pair.normalization_settings.target_lufs >= 0.0 {
                    format!("{}dBTP", pair.normalization_settings.true_peak_limit)
                } else {
                    format!("{}LUFS_{}dBTP", pair.normalization_settings.target_lufs, pair.normalization_settings.true_peak_limit)
                };
                format!("{}_normalised_{}.wav", pair.audio.filename_no_ext, spec)
            } else {
                format!("{}.wav", pair.audio.filename_no_ext)
            }
        }
    } else {
        pair.output_filename.clone()
    };

    let candidate = format!("{}/{}", directory, filename);
    avoid_source_collision(candidate, pair)
}

/// Normalise a path for comparison: canonicalise the parent directory (so
/// different spellings of the same folder match) and lowercase the whole thing
/// (macOS and Windows filesystems are case-insensitive). Falls back to a plain
/// lowercase of the input when the parent can't be canonicalised (e.g. it
/// doesn't exist on disk yet).
fn normalize_path(path: &str) -> String {
    let p = Path::new(path);
    match p.parent().and_then(|parent| parent.canonicalize().ok()) {
        Some(dir) => dir
            .join(p.file_name().unwrap_or_default())
            .to_string_lossy()
            .to_lowercase(),
        None => path.to_lowercase(),
    }
}

/// Never let an output file land on top of one of this pair's own source files.
/// FFmpeg cannot read from and write to the same file in-place — the job would
/// fail (and older FFmpeg builds would destroy the source). This happens when a
/// previously-generated output is re-added and processed again with the same
/// settings. If a collision is detected, append `_1`, `_2`, … until the output
/// path is clear of every source.
fn avoid_source_collision(path: String, pair: &MatchedPair) -> String {
    let mut sources = vec![normalize_path(&pair.audio.path)];
    if let Some(ref video) = pair.video {
        sources.push(normalize_path(&video.path));
    }
    let collides = |candidate: &str| {
        let normalized = normalize_path(candidate);
        sources.iter().any(|s| s == &normalized)
    };

    if !collides(&path) {
        return path;
    }

    let p = Path::new(&path);
    let dir = p.parent().map(|d| d.to_string_lossy().to_string());
    let stem = p
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output")
        .to_string();
    let ext = p.extension().and_then(|e| e.to_str());

    let mut n = 1;
    loop {
        let name = match ext {
            Some(e) => format!("{}_{}.{}", stem, n, e),
            None => format!("{}_{}", stem, n),
        };
        let candidate = match &dir {
            Some(d) if !d.is_empty() => format!("{}/{}", d, name),
            _ => name,
        };
        if !collides(&candidate) {
            return candidate;
        }
        n += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_audio(name: &str, path: &str) -> MediaFile {
        MediaFile {
            id: "1".into(),
            path: path.to_string(),
            filename: format!("{}.wav", name),
            filename_no_ext: name.to_string(),
            extension: "wav".into(),
            media_type: MediaType::Audio,
            duration_secs: 30.0,
            codec_info: None,
            sample_rate: None,
            channel_count: None,
            thumbnail_data: None,
        }
    }

    fn make_video(name: &str, path: &str) -> MediaFile {
        MediaFile {
            id: "2".into(),
            path: path.to_string(),
            filename: format!("{}.mov", name),
            filename_no_ext: name.to_string(),
            extension: "mov".into(),
            media_type: MediaType::Video,
            duration_secs: 30.0,
            codec_info: None,
            sample_rate: None,
            channel_count: None,
            thumbnail_data: None,
        }
    }

    fn make_pair(video: Option<MediaFile>, audio: MediaFile, output_filename: &str) -> MatchedPair {
        MatchedPair {
            id: "test".into(),
            video,
            audio,
            output_filename: output_filename.to_string(),
            normalization_enabled: false,
            normalization_settings: NormalizationSettings::default(),
            timecode_offset_secs: 0.0,
            match_confidence: 1.0,
            silence_compliance: false,
            silence_ms: 240.0,
            fade_ms: 5.0,
            clock_enabled: false,
        }
    }

    #[test]
    fn test_output_path_uses_audio_directory() {
        let pair = make_pair(
            Some(make_video("Video", "/projects/Video.mov")),
            make_audio("Audio", "/projects/Audio.wav"),
            "Output.mov",
        );
        let settings = ExportSettings { use_audio_file_location: true, ..ExportSettings::default() };
        let path = resolve_output_path(&pair, &settings);
        assert_eq!(path, "/projects/Output.mov");
    }

    #[test]
    fn test_output_path_uses_custom_directory() {
        let pair = make_pair(
            Some(make_video("Video", "/projects/Video.mov")),
            make_audio("Audio", "/projects/Audio.wav"),
            "Output.mov",
        );
        let settings = ExportSettings {
            use_audio_file_location: false,
            output_directory: Some("/custom/output".to_string()),
            ..ExportSettings::default()
        };
        let path = resolve_output_path(&pair, &settings);
        assert_eq!(path, "/custom/output/Output.mov");
    }

    #[test]
    fn test_output_path_fallback_when_no_custom_dir() {
        let pair = make_pair(
            Some(make_video("Video", "/projects/Video.mov")),
            make_audio("Audio", "/projects/Audio.wav"),
            "Output.mov",
        );
        let settings = ExportSettings {
            use_audio_file_location: false,
            output_directory: None,
            ..ExportSettings::default()
        };
        let path = resolve_output_path(&pair, &settings);
        // Falls back to audio file location
        assert_eq!(path, "/projects/Output.mov");
    }

    #[test]
    fn test_output_path_generates_name_when_empty() {
        let pair = make_pair(
            Some(make_video("MyVideo", "/projects/MyVideo.mov")),
            make_audio("Audio", "/projects/Audio.wav"),
            "", // empty = auto-generate
        );
        let settings = ExportSettings::default();
        let path = resolve_output_path(&pair, &settings);
        assert!(path.contains("MyVideo_with audio.mov"));
    }

    #[test]
    fn test_output_path_audio_only_no_norm() {
        let pair = make_pair(
            None,
            make_audio("MyMix", "/projects/MyMix.wav"),
            "",
        );
        let settings = ExportSettings::default();
        let path = resolve_output_path(&pair, &settings);
        // Passthrough with no norm would resolve to the source path itself; the
        // guard bumps it so FFmpeg never edits the file in-place.
        assert_eq!(path, "/projects/MyMix_1.wav");
    }

    #[test]
    fn test_output_path_never_overwrites_own_audio_source() {
        // Re-adding a previously-normalised file and processing it again with the
        // same settings produces an output name identical to the source. The guard
        // must bump it so FFmpeg never tries to edit the file in-place.
        let pair = make_pair(
            None,
            make_audio(
                "MyMix_normalised_-1dBTP",
                "/projects/MyMix_normalised_-1dBTP.wav",
            ),
            "MyMix_normalised_-1dBTP.wav", // namer would regenerate this exact name
        );
        let settings = ExportSettings::default();
        let path = resolve_output_path(&pair, &settings);
        assert_eq!(path, "/projects/MyMix_normalised_-1dBTP_1.wav");
    }

    #[test]
    fn test_output_path_never_overwrites_video_source() {
        // A laid-back output named after the video must not overwrite the video itself.
        let pair = make_pair(
            Some(make_video("Clip", "/projects/Clip.mov")),
            make_audio("Mix", "/projects/Mix.wav"),
            "Clip.mov", // collides with the source video
        );
        let settings = ExportSettings::default();
        let path = resolve_output_path(&pair, &settings);
        assert_eq!(path, "/projects/Clip_1.mov");
    }

    #[test]
    fn test_output_path_no_collision_passes_through() {
        // Normal case: output differs from sources, returned unchanged.
        let pair = make_pair(
            Some(make_video("Clip", "/projects/Clip.mov")),
            make_audio("Mix", "/projects/Mix.wav"),
            "Clip_with audio.mov",
        );
        let settings = ExportSettings::default();
        let path = resolve_output_path(&pair, &settings);
        assert_eq!(path, "/projects/Clip_with audio.mov");
    }

    #[test]
    fn test_output_path_audio_only_with_norm() {
        let mut pair = make_pair(
            None,
            make_audio("MyMix", "/projects/MyMix.wav"),
            "",
        );
        pair.normalization_enabled = true;
        pair.normalization_settings = NormalizationSettings { target_lufs: -23.0, true_peak_limit: -1.0 };
        let settings = ExportSettings::default();
        let path = resolve_output_path(&pair, &settings);
        assert_eq!(path, "/projects/MyMix_normalised_-23LUFS_-1dBTP.wav");
    }
}
