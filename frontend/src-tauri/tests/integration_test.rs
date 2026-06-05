//! Integration tests for Laybacker's audio processing pipeline.
//! These tests require ffmpeg to be installed and use real audio files.
//!
//! Test fixture: test_tone.wav is a 2-second 1kHz stereo sine at ~-14 LUFS, -14 dBTP.

use std::path::Path;

use app_lib::models::*;
use app_lib::services::{ffmpeg, inspector, loudness, processor};

fn test_fixture(name: &str) -> String {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    format!("{}/tests/fixtures/{}", manifest_dir, name)
}

fn output_dir() -> String {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let dir = format!("{}/tests/fixtures/output", manifest_dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn temp_output(name: &str) -> String {
    format!("{}/{}", output_dir(), name)
}

fn cleanup(path: &str) {
    let _ = std::fs::remove_file(path);
}

/// Copy test fixture to output dir and create a pair that references it there.
/// processor::process_pair resolves output from audio.path's parent directory.
fn make_audio_pair(fixture_name: &str, output_filename: &str, norm_enabled: bool, target_lufs: f64, tp_limit: f64) -> MatchedPair {
    let dir = output_dir();
    let audio_path_in_output = format!("{}/{}", dir, fixture_name);
    let _ = std::fs::copy(test_fixture(fixture_name), &audio_path_in_output);

    MatchedPair {
        id: "integration-test".into(),
        video: None,
        audio: MediaFile {
            id: "audio-1".into(),
            path: audio_path_in_output,
            filename: fixture_name.to_string(),
            filename_no_ext: Path::new(fixture_name).file_stem().unwrap().to_string_lossy().to_string(),
            extension: "wav".into(),
            media_type: MediaType::Audio,
            duration_secs: 2.0,
            codec_info: None,
            sample_rate: Some(48000.0),
            channel_count: Some(2),
            thumbnail_data: None,
        },
        output_filename: output_filename.to_string(),
        normalization_enabled: norm_enabled,
        normalization_settings: NormalizationSettings { target_lufs, true_peak_limit: tp_limit },
        timecode_offset_secs: 0.0,
        match_confidence: 1.0,
        silence_compliance: false,
        silence_ms: 240.0,
        fade_ms: 5.0,
    }
}

// ── Measurement tests ──

#[test]
fn test_ffmpeg_is_available() {
    assert!(ffmpeg::is_ffmpeg_available(), "ffmpeg must be installed to run integration tests");
}

#[test]
fn test_loudness_measurement() {
    let path = test_fixture("test_tone.wav");
    let result = loudness::measure(&path);
    assert!(result.is_ok(), "Measurement failed: {:?}", result.err());

    let m = result.unwrap();
    // Test tone is ~-14 LUFS, ~-14 dBTP
    assert!((m.integrated_lufs - (-14.0)).abs() < 1.0,
        "Expected ~-14 LUFS, got {:.1}", m.integrated_lufs);
    assert!((m.true_peak_dbtp - (-14.0)).abs() < 1.0,
        "Expected ~-14 dBTP, got {:.1}", m.true_peak_dbtp);
}

#[test]
fn test_loudnorm_measurement() {
    let path = test_fixture("test_tone.wav");
    let result = ffmpeg::measure_loudnorm_full(&path, -23.0, -1.0);
    assert!(result.is_ok(), "Loudnorm measurement failed: {:?}", result.err());

    let m = result.unwrap();
    assert!((m.input_i - (-14.0)).abs() < 1.0,
        "Expected ~-14 LUFS, got {:.1}", m.input_i);
    assert!(m.input_lra >= 0.0, "LRA should be non-negative");
}

// ── Processing tests ──

#[test]
fn test_process_audio_only_no_norm() {
    let output = temp_output("test_passthrough.wav");
    cleanup(&output);

    let pair = make_audio_pair("test_tone.wav", "test_passthrough.wav", false, 0.0, -1.0);
    let settings = ExportSettings::default();

    let result = processor::process_pair(&pair, &settings, |_| {});
    assert!(result.success, "Processing failed: {:?}", result.error);
    assert!(Path::new(&output).exists(), "Output file not created");
    assert!(Path::new(&output).metadata().unwrap().len() > 0, "Output file is empty");

    cleanup(&output);
}

#[test]
fn test_process_audio_only_fullscale_norm() {
    let output = temp_output("test_fullscale.wav");
    cleanup(&output);

    let pair = make_audio_pair("test_tone.wav", "test_fullscale.wav", true, 0.0, -1.0);
    let settings = ExportSettings::default();

    let progress_states = std::cell::RefCell::new(Vec::new());
    let result = processor::process_pair(&pair, &settings, |p| {
        progress_states.borrow_mut().push(p.state.clone());
    });
    assert!(result.success, "Processing failed: {:?}", result.error);
    assert!(result.measured_lufs.is_some(), "Should report measured LUFS");
    assert!(result.measured_true_peak.is_some(), "Should report measured true peak");

    // Verify progress callback fired with expected states
    let states = progress_states.borrow();
    assert!(states.contains(&"measuring".to_string()), "Should have measuring state");
    assert!(states.contains(&"complete".to_string()), "Should have complete state");

    // Verify the output was actually normalized
    let m = loudness::measure(&output).expect("Failed to measure output");
    // Full-scale mode: true peak should be near the -1.0 dBTP limit
    assert!(m.true_peak_dbtp <= -0.5, "True peak too high: got {:.1} dBTP", m.true_peak_dbtp);
    assert!(m.true_peak_dbtp >= -2.0, "True peak too low: got {:.1} dBTP", m.true_peak_dbtp);

    cleanup(&output);
}

#[test]
fn test_process_audio_only_lufs_norm() {
    let output = temp_output("test_lufs.wav");
    cleanup(&output);

    let pair = make_audio_pair("test_tone.wav", "test_lufs.wav", true, -23.0, -1.0);
    let settings = ExportSettings::default();

    let result = processor::process_pair(&pair, &settings, |_| {});
    assert!(result.success, "Processing failed: {:?}", result.error);
    assert!(Path::new(&output).exists(), "Output file not created");

    // Verify normalized to target LUFS
    let m = loudness::measure(&output).expect("Failed to measure output");
    assert!(
        (m.integrated_lufs - (-23.0)).abs() < 2.0,
        "Expected ~-23 LUFS, got {:.1} LUFS (delta: {:.1} dB)",
        m.integrated_lufs, m.integrated_lufs + 23.0,
    );

    cleanup(&output);
}

// ── Cancellation test ──
// Note: process_batch calls reset_cancel() at the start, so we can't pre-cancel.
// Instead, test that a batch of 2 pairs completes normally (verifying reset works).

#[test]
fn test_batch_processes_multiple_pairs() {
    let output1 = temp_output("test_batch_1.wav");
    let output2 = temp_output("test_batch_2.wav");
    cleanup(&output1);
    cleanup(&output2);

    let pairs = vec![
        make_audio_pair("test_tone.wav", "test_batch_1.wav", false, 0.0, -1.0),
        make_audio_pair("test_tone.wav", "test_batch_2.wav", false, 0.0, -1.0),
    ];
    let settings = ExportSettings::default();

    let results = processor::process_batch(&pairs, &settings, |_| {});

    assert_eq!(results.len(), 2);
    assert!(results[0].success, "Pair 1 failed: {:?}", results[0].error);
    assert!(results[1].success, "Pair 2 failed: {:?}", results[1].error);
    assert!(Path::new(&output1).exists(), "Output 1 not created");
    assert!(Path::new(&output2).exists(), "Output 2 not created");

    cleanup(&output1);
    cleanup(&output2);
}

// ── Regression: re-processing a generated output ──

#[test]
fn test_reprocessing_generated_output_does_not_fail() {
    // Re-adding a file Laybacker previously generated and processing it again
    // with the same settings used to resolve the output path to the file itself.
    // FFmpeg refuses to edit a file in-place, so the job failed. The collision
    // guard must bump the name so the job succeeds and the source is untouched.
    let dir = output_dir();
    let src_name = "regress_normalised_-1dBTP.wav";
    let src_path = format!("{}/{}", dir, src_name);
    std::fs::copy(test_fixture("test_tone.wav"), &src_path).unwrap();
    let before_len = std::fs::metadata(&src_path).unwrap().len();

    let pair = MatchedPair {
        id: "regress".into(),
        video: None,
        audio: MediaFile {
            id: "a".into(),
            path: src_path.clone(),
            filename: src_name.into(),
            filename_no_ext: "regress_normalised_-1dBTP".into(),
            extension: "wav".into(),
            media_type: MediaType::Audio,
            duration_secs: 2.0,
            codec_info: None,
            sample_rate: Some(48000.0),
            channel_count: Some(2),
            thumbnail_data: None,
        },
        output_filename: src_name.into(), // namer regenerates a name identical to the source
        normalization_enabled: true,
        normalization_settings: NormalizationSettings { target_lufs: 0.0, true_peak_limit: -1.0 },
        timecode_offset_secs: 0.0,
        match_confidence: 1.0,
        silence_compliance: false,
        silence_ms: 240.0,
        fade_ms: 5.0,
    };
    let settings = ExportSettings::default();
    let result = processor::process_pair(&pair, &settings, |_| {});

    assert!(result.success, "Reprocessing should succeed, got error: {:?}", result.error);
    let out = result.output_path.clone().expect("output path");
    assert!(out.ends_with("regress_normalised_-1dBTP_1.wav"), "Output should be bumped, got: {}", out);
    assert!(Path::new(&out).exists(), "Bumped output not created");
    assert!(Path::new(&out).metadata().unwrap().len() > 0, "Bumped output is empty");
    // The original source must be left completely untouched.
    assert_eq!(
        std::fs::metadata(&src_path).unwrap().len(),
        before_len,
        "Source file was modified — guard failed to protect it",
    );

    cleanup(&src_path);
    cleanup(&out);
}

// ── Compliance check ──

#[test]
fn test_silence_compliance_check() {
    let path = test_fixture("test_tone.wav");
    let result = ffmpeg::check_silence_compliance(&path, 2.0, 240.0);
    assert!(result.is_ok(), "Compliance check failed: {:?}", result.err());
    let (head_has_audio, tail_has_audio, head_peak, tail_peak) = result.unwrap();
    assert!(head_has_audio, "Test tone should have audio at head");
    assert!(tail_has_audio, "Test tone should have audio at tail");
    assert!(head_peak > -60.0, "Head peak should be above silence threshold");
    assert!(tail_peak > -60.0, "Tail peak should be above silence threshold");
}

// ── Inspector (file probing) ──

#[test]
fn test_inspect_audio_and_scan_directory() {
    // Inspect a single audio file — covers inspect_file + probe_file (audio path).
    let path = test_fixture("test_tone.wav");
    let mf = inspector::inspect_file(&path).expect("inspect audio failed");
    assert_eq!(mf.media_type, MediaType::Audio);
    assert_eq!(mf.extension, "wav");
    assert!((mf.duration_secs - 2.0).abs() < 0.5, "expected ~2s, got {}", mf.duration_secs);
    assert!(mf.sample_rate.is_some(), "audio should report a sample rate");
    assert!(mf.thumbnail_data.is_none(), "audio has no thumbnail");

    // Scan the fixtures directory — covers scan_paths' recursive directory walk.
    let dir = format!("{}/tests/fixtures", env!("CARGO_MANIFEST_DIR"));
    let files = inspector::scan_paths(&[dir]).expect("scan_paths failed");
    assert!(
        files.iter().any(|f| f.filename_no_ext == "test_tone" && f.media_type == MediaType::Audio),
        "scan should find test_tone.wav",
    );

    // Unsupported extension returns an error — covers that branch.
    assert!(inspector::inspect_file("/nonexistent/file.xyz").is_err());
}

#[test]
fn test_inspect_video_extracts_thumbnail() {
    // Generate a small video so we cover inspect_file's video branch + thumbnail.
    let dir = output_dir();
    let vid = format!("{}/gen_clip.mp4", dir);
    cleanup(&vid);
    let ff = ffmpeg::find_ffmpeg();
    let ok = std::process::Command::new(&ff)
        .args([
            "-y", "-f", "lavfi", "-i", "testsrc=duration=1:size=320x240:rate=10",
            "-c:v", "mpeg4", &vid,
        ])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    assert!(ok && Path::new(&vid).exists(), "could not generate a test video with ffmpeg");

    let mf = inspector::inspect_file(&vid).expect("inspect video failed");
    assert_eq!(mf.media_type, MediaType::Video);
    assert!(mf.duration_secs > 0.5, "video duration should be > 0.5s");
    let thumb = mf.thumbnail_data.expect("video should have a thumbnail");
    assert!(
        thumb.starts_with("data:image/jpeg;base64,"),
        "thumbnail should be a base64 jpeg data URL",
    );

    cleanup(&vid);
}
