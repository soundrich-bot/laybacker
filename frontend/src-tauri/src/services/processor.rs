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
    let mut loudnorm_filter: Option<String> = None;

    // Step 1: Measure and calculate normalization if enabled
    log::info!("Processing pair: normalization_enabled={}, target_lufs={}, true_peak_limit={}",
        pair.normalization_enabled,
        pair.normalization_settings.target_lufs,
        pair.normalization_settings.true_peak_limit,
    );
    if pair.normalization_enabled {
        progress_callback(ProcessingProgress {
            pair_id: pair_id.clone(),
            state: "measuring".to_string(),
            progress: 0.1,
            message: "Measuring loudness...".to_string(),
        });

        let is_lufs_mode = pair.normalization_settings.target_lufs < 0.0;

        if is_lufs_mode {
            // LUFS mode: use FFmpeg's loudnorm filter (two-pass) for gating-accurate targeting.
            // FFmpeg's loudnorm reads ~0.1dB louder than industry-standard meters (YouLean, Nugen),
            // so we target 0.1dB quieter to compensate.
            const LOUDNORM_CALIBRATION_OFFSET: f64 = 0.1;
            let calibrated_target = pair.normalization_settings.target_lufs - LOUDNORM_CALIBRATION_OFFSET;

            // Both passes must use the same calibrated target for consistency
            match ffmpeg::measure_loudnorm_full(
                &pair.audio.path,
                calibrated_target,
                pair.normalization_settings.true_peak_limit,
            ) {
                Ok(measurement) => {
                    measured_lufs = Some(measurement.input_i);
                    measured_true_peak = Some(measurement.input_tp);

                    let filter = ffmpeg::build_loudnorm_filter(
                        &measurement,
                        calibrated_target,
                        pair.normalization_settings.true_peak_limit,
                    );
                    log::info!("Loudnorm two-pass filter: {}", &filter);
                    loudnorm_filter = Some(filter);

                    progress_callback(ProcessingProgress {
                        pair_id: pair_id.clone(),
                        state: "measured".to_string(),
                        progress: 0.3,
                        message: format!(
                            "Measured: {:.1} LUFS, {:.1} dBTP → target {:.0} LUFS (loudnorm)",
                            measurement.input_i,
                            measurement.input_tp,
                            pair.normalization_settings.target_lufs,
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
        } else {
            // Full-scale mode: simple gain to true peak limit (this works perfectly)
            match loudness::measure(&pair.audio.path) {
                Ok(measurement) => {
                    measured_lufs = Some(measurement.integrated_lufs);
                    measured_true_peak = Some(measurement.true_peak_dbtp);

                    let gain = loudness::calculate_gain(&measurement, &pair.normalization_settings);
                    log::info!("Full-scale mode: {:.2} LUFS, {:.2} dBTP. Gain: {:.3} dB",
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
            loudnorm_filter.as_deref(),
            pair.timecode_offset_secs,
            compliance,
        )
    } else {
        ffmpeg::build_audio_only_command(
            &pair.audio.path,
            &output_path,
            settings,
            audio_gain_db,
            loudnorm_filter.as_deref(),
            compliance,
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

    format!("{}/{}", directory, filename)
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
        assert_eq!(path, "/projects/MyMix.wav");
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
