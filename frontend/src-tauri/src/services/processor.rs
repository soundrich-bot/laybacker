use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

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
    if pair.normalization_enabled {
        progress_callback(ProcessingProgress {
            pair_id: pair_id.clone(),
            state: "measuring".to_string(),
            progress: 0.1,
            message: "Measuring loudness...".to_string(),
        });

        let is_broadcast_mode = pair.normalization_settings.target_lufs < 0.0;

        if is_broadcast_mode {
            // Two-pass loudnorm for precise LUFS targeting
            match ffmpeg::measure_loudnorm_full(&pair.audio.path) {
                Ok(measurement) => {
                    measured_lufs = Some(measurement.input_i);
                    measured_true_peak = Some(measurement.input_tp);

                    // Build the second-pass loudnorm filter with measured values
                    loudnorm_filter = Some(ffmpeg::build_loudnorm_filter(
                        &measurement,
                        pair.normalization_settings.target_lufs,
                        pair.normalization_settings.true_peak_limit,
                    ));

                    progress_callback(ProcessingProgress {
                        pair_id: pair_id.clone(),
                        state: "measured".to_string(),
                        progress: 0.3,
                        message: format!(
                            "Measured: {:.1} LUFS, {:.1} dBTP → target {:.0} LUFS",
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
            // Full-scale mode: simple gain to true peak limit
            match loudness::measure(&pair.audio.path) {
                Ok(measurement) => {
                    measured_lufs = Some(measurement.integrated_lufs);
                    measured_true_peak = Some(measurement.true_peak_dbtp);

                    let gain = loudness::calculate_gain(&measurement, &pair.normalization_settings);
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
    let results = Arc::new(Mutex::new(Vec::new()));

    reset_cancel();

    for pair in pairs {
        if is_cancelled() {
            results.lock().unwrap().push(ProcessingResult {
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
        results.lock().unwrap().push(result);
    }

    Arc::try_unwrap(results)
        .unwrap_or_else(|_| panic!("Failed to unwrap results"))
        .into_inner()
        .unwrap()
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
