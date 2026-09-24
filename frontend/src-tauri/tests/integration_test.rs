//! Integration tests for Laybacker's audio processing pipeline.
//! These tests require ffmpeg to be installed and use real audio files.
//!
//! Test fixture: test_tone.wav is a 2-second 1kHz stereo sine at ~-14 LUFS, -14 dBTP.

use std::path::Path;

use app_lib::models::*;
use app_lib::services::{ffmpeg, inspector, loudness, namer, processor};

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
    // Give every test its OWN copy of the source. Cargo runs these tests in
    // parallel, so a shared path let one test truncate the file while another's
    // ffmpeg was reading it ("Invalid data found when processing input").
    let stem = Path::new(output_filename)
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy();
    let audio_path_in_output = format!("{}/src_{}_{}", dir, stem, fixture_name);
    std::fs::copy(test_fixture(fixture_name), &audio_path_in_output)
        .expect("failed to copy the test fixture into the output dir");

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
            frame_rate: None,
            width: None,
            height: None,
            slate_secs: None,
            channel_layout: None,
            bit_depth: None,
            bit_rate: None,
            thumbnail_data: None,        },
        output_filename: output_filename.to_string(),
        normalization_enabled: norm_enabled,
        normalization_settings: NormalizationSettings { target_lufs, true_peak_limit: tp_limit },
        timecode_offset_secs: 0.0,
        match_confidence: 1.0,
        silence_compliance: false,
        silence_ms: 240.0,
        fade_ms: 5.0,
        clock_enabled: false,
        length_fix: LengthFix::default(),
        slate_enabled: false,
        slate_duration_secs: 5.0,
        slate_text: String::new(),
        slate_image: None,
        slate_black_secs: 0.0,
            slate_overlay: false,
            slate_matte: None,    }
}

/// End-to-end peak-mode naming: the store sets target_lufs = 0 (full-scale) with
/// a dBTP ceiling, lets the namer fill the output name, then renders. Confirm the
/// file on disk is "..._-1dBTP.wav", and that reloading it (a plain passthrough)
/// keeps that name rather than flattening it back to the bare stem.
#[test]
fn test_peak_mode_names_and_survives_reload() {
    // 1. Peak-mode pair with an EMPTY output name — the namer computes it.
    let mut pairs = vec![make_audio_pair("test_tone.wav", "", true, 0.0, -1.0)];
    namer::generate_names(&mut pairs, true, "wav");
    assert_eq!(
        pairs[0].output_filename, "test_tone_-1dBTP.wav",
        "peak-mode output should carry the dBTP spec"
    );

    // 2. Render it and confirm the file on disk is named with the dBTP spec.
    let result = processor::process_pair(&pairs[0], &ExportSettings::default(), |_| {});
    assert!(result.success, "process failed: {:?}", result.error);
    let out = result.output_path.expect("no output path");
    assert!(
        out.ends_with("_-1dBTP.wav"),
        "written file should keep the dBTP suffix, got: {out}"
    );
    assert!(Path::new(&out).exists(), "output file missing on disk");

    // 2b. The LEVEL must actually move: test_tone is ~-14 dBTP, so peak mode
    //     should boost it to land on -1 dBTP. A neutral (unchanged) output is
    //     the bug we're hunting.
    let m = loudness::measure(&out).expect("measure output");
    eprintln!("OUTPUT_TP={:.2} dBTP (target -1.0)", m.true_peak_dbtp);
    assert!(
        (m.true_peak_dbtp - (-1.0)).abs() <= 0.5,
        "peak normalise should land near -1 dBTP, but output is {:.2} dBTP (neutral == not applied)",
        m.true_peak_dbtp
    );

    // 3. Reload the rendered file as a plain passthrough (no norm) — the name
    //    must survive, exactly as the batch list shows after a normalise pass.
    let reloaded = inspector::inspect_file(&out).expect("inspect reloaded output");
    let mut reload_pairs = vec![make_audio_pair("test_tone.wav", "", false, 0.0, -1.0)];
    reload_pairs[0].audio = reloaded;
    reload_pairs[0].normalization_enabled = false; // reloaded pairs arrive un-flagged
    namer::generate_names(&mut reload_pairs, true, "wav");
    assert_eq!(
        reload_pairs[0].output_filename, "test_tone_-1dBTP.wav",
        "reloaded passthrough must keep the dBTP name"
    );

    cleanup(&out);
}

/// End-to-end slate: build a real 2s video, attach a 3s slate card (base64 JPEG,
/// exactly as the frontend sends it), process, and confirm the output runs
/// slate + programme with the audio pushed back to the programme start.
#[test]
fn test_slate_prepends_card_and_delays_audio() {
    let dir = output_dir();

    // Build a small real video from the slate fixture (image2 loop → H.264),
    // the same mechanism the slate itself uses.
    let video_path = format!("{}/slate_test_video.mov", dir);
    let jpg = test_fixture("slate_320x180.jpg");
    let build_args: Vec<String> = [
        "-y", "-loop", "1", "-framerate", "25", "-t", "2", "-i", jpg.as_str(),
        "-c:v", "libx264", "-pix_fmt", "yuv420p", video_path.as_str(),
    ].iter().map(|s| s.to_string()).collect();
    ffmpeg::run_ffmpeg(&build_args).expect("failed to build test video fixture");

    let video = inspector::inspect_file(&video_path).expect("inspect test video");
    assert_eq!(video.width, Some(320), "probe should report frame width");

    use base64::Engine as _;
    let jpg_b64 = base64::engine::general_purpose::STANDARD
        .encode(std::fs::read(&jpg).unwrap());

    let mut pair = make_audio_pair("test_tone.wav", "slated_out.mov", false, 0.0, -1.0);
    pair.video = Some(video);
    pair.slate_enabled = true;
    pair.slate_duration_secs = 3.0;
    pair.slate_black_secs = 1.0; // 3s card + 1s black = 4s preroll
    pair.slate_image = Some(format!("data:image/jpeg;base64,{}", jpg_b64));

    let output = temp_output("slated_out.mov");
    cleanup(&output);
    let result = processor::process_pair(&pair, &ExportSettings::default(), |_| {});
    assert!(result.success, "slated process failed: {:?}", result.error);

    // 3s slate + 1s black + 2s programme ≈ 6s output.
    let out = inspector::inspect_file(&output).expect("inspect slated output");
    assert!(
        (out.duration_secs - 6.0).abs() < 0.2,
        "expected ~6s (3s slate + 1s black + 2s programme), got {:.2}s",
        out.duration_secs
    );
    // The render stamps the PREROLL (card + black) into the container — that's
    // where programme starts — and a re-drop reads it back.
    assert_eq!(out.slate_secs, Some(4.0), "slated output should carry the preroll tag");

    cleanup(&output);
    cleanup(&video_path);
}

/// One greyscale frame of a video at `secs`, as raw bytes (w*h).
fn grab_gray_frame(video: &str, secs: f64, out: &str) -> Vec<u8> {
    let args: Vec<String> = [
        "-y", "-ss", &format!("{:.3}", secs), "-i", video,
        "-frames:v", "1", "-f", "rawvideo", "-pix_fmt", "gray", out,
    ].iter().map(|s| s.to_string()).collect();
    ffmpeg::run_ffmpeg(&args).expect("grab frame");
    let bytes = std::fs::read(out).expect("read frame");
    let _ = std::fs::remove_file(out);
    bytes
}

/// Slate OVERLAY: text is laid over the first second of the picture. The
/// runtime must not change, the picture outside the text must not be hazed by
/// the matte, and the text must be gone once the overlay runs out.
#[test]
fn test_slate_overlay_keeps_runtime_and_leaves_picture_clean() {
    let dir = output_dir();
    // A flat mid-grey 2s video (the card faded to grey within one frame), so
    // any haze from the overlay shows up as a level shift.
    let video_path = format!("{}/overlay_test_video.mov", dir);
    let jpg = test_fixture("slate_320x180.jpg");
    let build_args: Vec<String> = [
        "-y", "-loop", "1", "-framerate", "25", "-t", "2", "-i", jpg.as_str(),
        "-vf", "fade=t=out:st=0:d=0.04:c=0x808080",
        "-c:v", "libx264", "-pix_fmt", "yuv420p", video_path.as_str(),
    ].iter().map(|s| s.to_string()).collect();
    ffmpeg::run_ffmpeg(&build_args).expect("failed to build grey test video");
    let video = inspector::inspect_file(&video_path).expect("inspect test video");
    // The editor's backdrop: a real frame comes back as a JPEG data URL.
    let frame = ffmpeg::extract_frame(&video_path, 0.5, 320).expect("frame grab");
    assert!(frame.starts_with("data:image/jpeg;base64,") && frame.len() > 200);
    assert!(ffmpeg::extract_frame(&video_path, 99.0, 320).is_err(), "past the end there is no frame");
    let source = grab_gray_frame(&video_path, 0.5, &format!("{}/ov_src.raw", dir));
    assert_eq!(source.len(), 320 * 180);

    // A synthetic "text" image: a white box in the middle of a black frame.
    // White-on-black is its own matte, exactly like rendered slate text.
    let raw_path = format!("{}/ov_text.raw", dir);
    let text_jpg = format!("{}/ov_text.jpg", dir);
    let mut raw = vec![0u8; 320 * 180];
    for y in 70..110 { for x in 110..210 { raw[y * 320 + x] = 255; } }
    std::fs::write(&raw_path, &raw).unwrap();
    let enc: Vec<String> = [
        "-y", "-f", "rawvideo", "-pix_fmt", "gray", "-s", "320x180", "-i", raw_path.as_str(),
        "-frames:v", "1", "-q:v", "2", text_jpg.as_str(),
    ].iter().map(|s| s.to_string()).collect();
    ffmpeg::run_ffmpeg(&enc).expect("encode synthetic text jpeg");

    use base64::Engine as _;
    let b64 = format!("data:image/jpeg;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(std::fs::read(&text_jpg).unwrap()));
    cleanup(&raw_path);
    cleanup(&text_jpg);

    let mut pair = make_audio_pair("test_tone.wav", "overlay_out.mov", false, 0.0, -1.0);
    pair.video = Some(video);
    pair.slate_enabled = true;
    pair.slate_overlay = true;
    pair.slate_duration_secs = 1.0;
    pair.slate_black_secs = 1.0; // ignored in overlay mode
    // White text on black is its own matte.
    pair.slate_image = Some(b64.clone());
    pair.slate_matte = Some(b64);

    let output = temp_output("overlay_out.mov");
    cleanup(&output);
    let result = processor::process_pair(&pair, &ExportSettings::default(), |_| {});
    assert!(result.success, "overlay process failed: {:?}", result.error);

    let out = inspector::inspect_file(&output).expect("inspect overlay output");
    assert!((out.duration_secs - 2.0).abs() < 0.15, "runtime must not change, got {:.2}s", out.duration_secs);
    assert_eq!(out.slate_secs, None, "an overlay moves nothing, so no preroll tag");

    // During the overlay: text is there (bright pixels), corner is untouched.
    let during = grab_gray_frame(&output, 0.4, &format!("{}/ov_during.raw", dir));
    let corner = |f: &[u8]| f[5 * 320 + 5] as i32;
    let centre = |f: &[u8]| f[90 * 320 + 160] as i32;
    assert!(centre(&during) > 215, "slate text should be solid white during the overlay, got {}", centre(&during));
    assert!((corner(&during) - corner(&source)).abs() <= 6,
        "picture outside the text must not be hazed: source {} vs overlaid {}", corner(&source), corner(&during));
    // After it: the text is gone.
    let after = grab_gray_frame(&output, 1.7, &format!("{}/ov_after.raw", dir));
    assert!((centre(&after) - corner(&source)).abs() <= 6, "text should be gone after the overlay, centre {}", centre(&after));

    cleanup(&output);
    cleanup(&video_path);
    cleanup(&pair.audio.path);
}

/// Solo slate: a video with its OWN soundtrack gets the card prepended and its
/// audio kept, delayed by the slate duration.
#[test]
fn test_solo_slate_keeps_own_audio() {
    let dir = output_dir();
    let jpg = test_fixture("slate_320x180.jpg");

    // Build a 2s video WITH an embedded soundtrack (tone muxed in).
    let video_path = format!("{}/solo_slate_video.mov", dir);
    let build_args: Vec<String> = [
        "-y", "-loop", "1", "-framerate", "25", "-t", "2", "-i", jpg.as_str(),
        "-i", test_fixture("test_tone.wav").as_str(),
        "-c:v", "libx264", "-pix_fmt", "yuv420p", "-c:a", "pcm_s16le",
        "-shortest", video_path.as_str(),
    ].iter().map(|s| s.to_string()).collect();
    ffmpeg::run_ffmpeg(&build_args).expect("failed to build solo test video");

    let output = temp_output("solo_slated.mov");
    cleanup(&output);
    let spec = ffmpeg::SlateSpec { image_path: jpg.clone(), duration_secs: 3.0, black_secs: 0.0, matte_path: None };
    let args = ffmpeg::build_solo_slate_command(&video_path, &output, &spec, Some(25.0), true);
    ffmpeg::run_ffmpeg(&args).expect("solo slate render failed");

    let out = inspector::inspect_file(&output).expect("inspect solo slated output");
    assert!(
        (out.duration_secs - 5.0).abs() < 0.2,
        "expected ~5s (3s slate + 2s video), got {:.2}s",
        out.duration_secs
    );
    // The soundtrack must survive the slate (probe reports audio fields).
    assert!(out.channel_count.is_some(), "output lost its audio track");
    assert_eq!(out.slate_secs, Some(3.0), "solo slated output should carry the slate tag");

    cleanup(&output);
    cleanup(&video_path);
}

/// Stereo QC + split/join on real files: the test tone is dual-mono (identical
/// channels), so the check must say so; splitting it yields _L/_R monos, and
/// joining them back gives a 2-channel file again.
#[test]
fn test_stereo_check_and_split_join_roundtrip() {
    use app_lib::services::channels;
    let dir = output_dir();
    let src = format!("{}/chan_src.wav", dir);
    std::fs::copy(test_fixture("test_tone.wav"), &src).unwrap();

    // 1. Stereo check: identical L/R → dual_mono, correlation ≈ 1.
    let check = channels::check_stereo(&src, 2).expect("stereo check");
    assert_eq!(check.verdict, "dual_mono", "got {:?}", check);
    assert!(check.correlation.unwrap() > 0.98);

    // 2. Split → two mono files named by channel.
    let outs = channels::split_channels(&src, Some("stereo"), 2).expect("split");
    assert_eq!(outs.len(), 2);
    assert!(outs[0].ends_with("chan_src_L.wav") && outs[1].ends_with("chan_src_R.wav"), "{outs:?}");
    for o in &outs {
        let f = inspector::inspect_file(o).expect("inspect stem");
        assert_eq!(f.channel_count, Some(1), "stem should be mono");
    }
    // A mono stem reports as mono, not judged.
    assert_eq!(channels::check_stereo(&outs[0], 1).unwrap().verdict, "mono");

    // 3. Join them back → stereo again.
    let joined = format!("{}/chan_src_stereo.wav", dir);
    channels::join_channels(&outs, "stereo", &joined).expect("join");
    let j = inspector::inspect_file(&joined).expect("inspect joined");
    assert_eq!(j.channel_count, Some(2));
    assert!((j.duration_secs - 2.0).abs() < 0.1);

    cleanup(&src);
    for o in &outs { cleanup(o); }
    cleanup(&joined);
}

#[test]
fn test_audio_only_conversion_44k1_16bit_and_flac() {
    // WAV 48k → WAV 44.1k 16-bit, then → FLAC 24-bit: rate follows, file plays.
    let pair = make_audio_pair("test_tone.wav", "conv_44.1k_16bit.wav", false, -23.0, -1.0);
    let settings = ExportSettings {
        audio_container: AudioContainer::Wav,
        sample_rate: Some(44100),
        bit_depth: Some(16),
        ..Default::default()
    };
    let r = processor::process_pair(&pair, &settings, |_| {});
    assert!(r.success, "conversion failed: {:?}", r.error);
    let out = r.output_path.clone().unwrap();
    let f = inspector::inspect_file(&out).expect("inspect converted");
    assert_eq!(f.sample_rate, Some(44100.0));
    assert_eq!(f.channel_count, Some(2));
    assert!((f.duration_secs - 2.0).abs() < 0.05);
    assert!(f.codec_info.as_deref().unwrap_or("").contains("s16"), "codec {:?}", f.codec_info);
    cleanup(&out);

    let pair = make_audio_pair("test_tone.wav", "conv_24bit.flac", false, -23.0, -1.0);
    let settings = ExportSettings {
        audio_container: AudioContainer::Flac,
        bit_depth: Some(24),
        ..Default::default()
    };
    let r = processor::process_pair(&pair, &settings, |_| {});
    assert!(r.success, "flac failed: {:?}", r.error);
    let out = r.output_path.clone().unwrap();
    let f = inspector::inspect_file(&out).expect("inspect flac");
    assert_eq!(f.codec_info.as_deref(), Some("flac"));
    assert_eq!(f.sample_rate, Some(48000.0));
    cleanup(&out);
    cleanup(&pair.audio.path);
}

#[test]
fn test_fold_fade_and_trim() {
    use app_lib::services::processing;
    let dir = output_dir();
    let src = format!("{}/proc_src.wav", dir);
    std::fs::copy(test_fixture("test_tone.wav"), &src).unwrap();

    // Fold to mono: the dual-mono tone comes out mono at the same level.
    let mono = processing::process_audio("fold_mono", &src, Some("stereo"), 2, 2.0, None).expect("fold mono");
    assert!(mono.ends_with("proc_src_mono.wav"));
    let f = inspector::inspect_file(&mono).unwrap();
    assert_eq!(f.channel_count, Some(1));
    let before = loudness::measure(&src).unwrap().true_peak_dbtp;
    let after = loudness::measure(&mono).unwrap().true_peak_dbtp;
    assert!((before - after).abs() < 0.3, "mono fold changed level: {before} → {after}");

    // Fade: same length, quieter overall (the ends are faded).
    let faded = processing::process_audio("fade", &src, Some("stereo"), 2, 2.0, Some(0.5)).expect("fade");
    let f = inspector::inspect_file(&faded).unwrap();
    assert!((f.duration_secs - 2.0).abs() < 0.05);
    let l_before = loudness::measure(&src).unwrap().integrated_lufs;
    let l_after = loudness::measure(&faded).unwrap().integrated_lufs;
    assert!(l_after < l_before - 0.5, "fade should lower integrated loudness: {l_before} → {l_after}");

    // Trim: pad the tone with a second of silence each end, then trim it off.
    let padded = format!("{}/proc_padded.wav", dir);
    ffmpeg::run_ffmpeg(&[
        "-y".into(), "-i".into(), src.clone(),
        "-af".into(), "adelay=1000:all=1,apad=pad_dur=1".into(),
        "-c:a".into(), "pcm_s24le".into(), padded.clone(),
    ]).expect("pad");
    assert!((inspector::inspect_file(&padded).unwrap().duration_secs - 4.0).abs() < 0.05);
    let trimmed = processing::process_audio("trim", &padded, Some("stereo"), 2, 4.0, None).expect("trim");
    let f = inspector::inspect_file(&trimmed).unwrap();
    assert!((f.duration_secs - 2.0).abs() < 0.1, "trimmed length {}", f.duration_secs);

    // Fold to stereo needs more than two channels.
    assert!(processing::process_audio("fold_stereo", &src, Some("stereo"), 2, 2.0, None).is_err());

    for p in [&src, &mono, &faded, &padded, &trimmed] { cleanup(p); }
}

#[test]
fn test_probe_reports_bit_depth_and_waveform_peaks() {
    use app_lib::services::waveform;
    let f = inspector::inspect_file(&test_fixture("test_tone.wav")).unwrap();
    assert_eq!(f.sample_rate, Some(48000.0));
    assert!(f.bit_depth.is_some(), "a PCM WAV should report its bit depth");
    let peaks = waveform::compute_peaks(&test_fixture("test_tone.wav"), 50).expect("peaks");
    assert_eq!(peaks.len(), 50);
    // A steady -14 dBFS tone: every bucket ≈ 0.2, none silent, none over.
    assert!(peaks.iter().all(|p| *p > 0.1 && *p <= 1.0), "{peaks:?}");
}

#[test]
fn test_chain_shape_fold_fade_then_split_keeps_stem() {
    use app_lib::services::processing::{self, ShapeOp};
    let work = processing::chain_workdir().unwrap();
    let src = format!("{}/chain_src.wav", output_dir());
    std::fs::copy(test_fixture("test_tone.wav"), &src).unwrap();
    // Fade then split: the stems come from the FADED file, named by channel,
    // and the intermediate keeps the source stem ("chain_src.wav").
    // Fold-to-stereo on a stereo file is skipped quietly, not an error.
    let ops = vec![
        ShapeOp { kind: "fold_stereo".into(), param: None },
        ShapeOp { kind: "fade".into(), param: Some(0.25) },
        ShapeOp { kind: "split".into(), param: None },
    ];
    let r = processing::shape_file(&src, &ops, &work).expect("shape");
    assert_eq!(r.skipped, vec!["fold_stereo".to_string()]);
    assert_eq!(r.applied, vec!["fade".to_string(), "split".to_string()]);
    let outs = r.files;
    assert_eq!(outs.len(), 2, "{outs:?}");
    assert!(outs[0].ends_with("split/chain_src_L.wav"), "{outs:?}");
    assert!(Path::new(&format!("{}/step2/chain_src.wav", work)).exists(), "fade is op 2, so its intermediate is step2");
    let l = loudness::measure(&outs[0]).unwrap().integrated_lufs;
    let orig = loudness::measure(&src).unwrap().integrated_lufs;
    assert!(l < orig - 0.3, "stem should carry the fade: {orig} → {l}");
    processing::remove_workdir(&work).unwrap();
    assert!(!Path::new(&work).exists());
    cleanup(&src);
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

    // Verify normalized to target LUFS — tight tolerance, measured by the same
    // ebur128 meter QC (and Pro Tools) use. The old ±2.0 tolerance let a
    // consistent 0.2 LU loudnorm targeting error pass unnoticed.
    let m = loudness::measure(&output).expect("Failed to measure output");
    assert!(
        (m.integrated_lufs - (-23.0)).abs() <= 0.15,
        "Expected -23.0 LUFS ±0.15, got {:.2} LUFS (delta: {:.2} dB)",
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
            frame_rate: None,
            width: None,
            height: None,
            slate_secs: None,
            channel_layout: None,
            bit_depth: None,
            bit_rate: None,
            thumbnail_data: None,        },
        output_filename: src_name.into(), // namer regenerates a name identical to the source
        normalization_enabled: true,
        normalization_settings: NormalizationSettings { target_lufs: 0.0, true_peak_limit: -1.0 },
        timecode_offset_secs: 0.0,
        match_confidence: 1.0,
        silence_compliance: false,
        silence_ms: 240.0,
        fade_ms: 5.0,
        clock_enabled: false,
        length_fix: LengthFix::default(),
        slate_enabled: false,
        slate_duration_secs: 5.0,
        slate_text: String::new(),
        slate_image: None,
        slate_black_secs: 0.0,
            slate_overlay: false,
            slate_matte: None,    };
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

#[test]
fn test_run_ffmpeg_with_progress_reports_and_outputs() {
    // Generate a short clip, transcode it to ProRes through the progress-aware
    // runner, and confirm it reports progress and produces a valid output.
    let dir = output_dir();
    let src = format!("{}/prog_src.mp4", dir);
    let out = format!("{}/prog_src_ProRes_Proxy.mov", dir);
    cleanup(&src);
    cleanup(&out);

    let ff = ffmpeg::find_ffmpeg();
    let made = std::process::Command::new(&ff)
        .args([
            "-y", "-f", "lavfi", "-i", "testsrc=duration=3:size=320x240:rate=15",
            "-c:v", "mpeg4", &src,
        ])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    assert!(made && Path::new(&src).exists(), "could not generate a test video");

    let args = ffmpeg::build_prores_command(&src, &out, 0); // 0 = ProRes Proxy (fastest)
    let calls = std::cell::RefCell::new(Vec::<f64>::new());
    let result = ffmpeg::run_ffmpeg_with_progress(&args, 3.0, |p| calls.borrow_mut().push(p));

    assert!(result.is_ok(), "run_ffmpeg_with_progress failed: {:?}", result);
    assert!(Path::new(&out).exists(), "ProRes output was not created");

    let calls = calls.into_inner();
    assert!(!calls.is_empty(), "expected at least one progress callback");
    assert!(calls.iter().all(|&p| (0.0..=1.0).contains(&p)), "progress out of range: {:?}", calls);
    assert_eq!(*calls.last().unwrap(), 1.0, "final progress should be 1.0 (complete)");

    cleanup(&src);
    cleanup(&out);
}
