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

    // Step 1: Measure and calculate normalization if enabled
    if pair.normalization_enabled {
        progress_callback(ProcessingProgress {
            pair_id: pair_id.clone(),
            state: "measuring".to_string(),
            progress: 0.1,
            message: "Measuring loudness...".to_string(),
        });

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

    // Step 2: Build and run ffmpeg command
    progress_callback(ProcessingProgress {
        pair_id: pair_id.clone(),
        state: "muxing".to_string(),
        progress: 0.5,
        message: "Muxing video and audio...".to_string(),
    });

    let args = ffmpeg::build_mux_command(
        &pair.video.path,
        &pair.audio.path,
        &output_path,
        settings,
        audio_gain_db,
        pair.timecode_offset_secs,
    );

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
        format!(
            "{}_with audio.{}",
            pair.video.filename_no_ext,
            settings.output_extension()
        )
    } else {
        pair.output_filename.clone()
    };

    format!("{}/{}", directory, filename)
}
