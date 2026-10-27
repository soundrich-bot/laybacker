//! Integration tests for Laybacker's audio processing pipeline.
//! These tests require ffmpeg to be installed and use real audio files.
//!
//! Test fixture: test_tone.wav is a 2-second 1kHz stereo sine at ~-14 LUFS, -14 dBTP.

use std::path::Path;

use app_lib::models::*;
use app_lib::services::{ffmpeg, inspector, loudness, namer, processor, stemcheck};

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
        id: format!("integration-test-{}", uuid::Uuid::new_v4()),
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
    let spec = ffmpeg::SlateSpec { image_path: jpg.clone(), duration_secs: 3.0, black_secs: 0.0, matte_path: None, prebaked: false };
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
    let lanes = waveform::compute_lanes(&test_fixture("test_tone.wav"), 50, 2).expect("lanes");
    assert_eq!(lanes.len(), 2);
    assert!(lanes.iter().all(|l| l.len() == 50 && l.iter().all(|p| *p > 0.1)));
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

/// A 6 s H.264 test video (25 fps, keyframes every 2 s) with a moving box and
/// the tone as its soundtrack — the shape of a real camera / NLE export.
fn build_h264_source(path: &str) {
    let jpg = test_fixture("slate_320x180.jpg");
    let tone = test_fixture("test_tone.wav");
    let args: Vec<String> = [
        "-y", "-loop", "1", "-framerate", "25", "-t", "6", "-i", jpg.as_str(),
        "-stream_loop", "3", "-i", tone.as_str(), "-shortest",
        "-vf", "fade=t=out:st=0:d=0.04:c=0x808080,drawbox=x=t*40:y=60:w=30:h=30:c=white:t=fill",
        "-c:v", "libx264", "-profile:v", "high", "-level", "3.1", "-g", "50", "-pix_fmt", "yuv420p",
        "-c:a", "aac", path,
    ].iter().map(|s| s.to_string()).collect();
    ffmpeg::run_ffmpeg(&args).expect("build h264 source");
}

fn synthetic_text_jpeg(dir: &str, name: &str) -> String {
    let raw_path = format!("{}/{}.raw", dir, name);
    let jpg = format!("{}/{}.jpg", dir, name);
    let mut raw = vec![0u8; 320 * 180];
    for y in 70..110 { for x in 110..210 { raw[y * 320 + x] = 255; } }
    std::fs::write(&raw_path, &raw).unwrap();
    let enc: Vec<String> = [
        "-y", "-f", "rawvideo", "-pix_fmt", "gray", "-s", "320x180", "-i", raw_path.as_str(),
        "-frames:v", "1", "-q:v", "2", jpg.as_str(),
    ].iter().map(|s| s.to_string()).collect();
    ffmpeg::run_ffmpeg(&enc).expect("encode synthetic text jpeg");
    cleanup(&raw_path);
    jpg
}

/// Fast slate: an H.264 source takes the splice path. The card version is
/// 100 + 150 frames with the programme copied; the overlay keeps exactly the
/// source's 150 frames, with the text on at 0.5 s and gone by 4 s.
#[test]
fn test_fast_slate_splices_h264_and_verifies() {
    use app_lib::services::fastslate;
    let dir = output_dir();
    let src = format!("{}/fast_src.mov", dir);
    build_h264_source(&src);
    let info = fastslate::probe(&src).expect("probe");
    assert_eq!(info.codec, "h264");
    assert_eq!(info.packets, 150);
    assert!(info.keyframes.len() >= 3, "{:?}", info.keyframes);
    assert!(fastslate::eligible(&info).is_ok());

    // Card: 3 s + 1 s black = 100 frames in front of 150.
    let jpg = test_fixture("slate_320x180.jpg");
    let card = ffmpeg::SlateSpec { image_path: jpg.clone(), duration_secs: 3.0, black_secs: 1.0, matte_path: None, prebaked: false };
    let work = format!("{}/fast_work_card", dir);
    let spliced = fastslate::build_spliced_video(&src, &card, &info, &work).expect("fast card");
    let f = inspector::inspect_file(&spliced).unwrap();
    assert!((f.duration_secs - 10.0).abs() < 0.1, "card splice should be 10 s, got {}", f.duration_secs);
    assert_eq!(f.codec_info.as_deref(), Some("h264"));
    let _ = std::fs::remove_dir_all(&work);

    // Overlay: 1 s of text → cut at the 2 s keyframe → same 150 frames out.
    let text = synthetic_text_jpeg(&dir, "fast_text");
    let ov = ffmpeg::SlateSpec { image_path: text.clone(), duration_secs: 1.0, black_secs: 0.0, matte_path: Some(text.clone()), prebaked: false };
    assert_eq!(fastslate::cut_point(&info.keyframes, 1.0), Some(2.0));
    let work = format!("{}/fast_work_ov", dir);
    let spliced = fastslate::build_spliced_video(&src, &ov, &info, &work).expect("fast overlay");
    let f = inspector::inspect_file(&spliced).unwrap();
    assert!((f.duration_secs - 6.0).abs() < 0.1, "overlay splice must keep the runtime, got {}", f.duration_secs);
    let during = grab_gray_frame(&spliced, 0.5, &format!("{}/fast_d.raw", dir));
    let after = grab_gray_frame(&spliced, 4.0, &format!("{}/fast_a.raw", dir));
    assert!(during[90 * 320 + 160] > 215, "text should show at 0.5 s: {}", during[90 * 320 + 160]);
    assert!(after[90 * 320 + 160] < 160, "text should be gone at 4 s (copied tail): {}", after[90 * 320 + 160]);
    let _ = std::fs::remove_dir_all(&work);

    // End to end through the layback path: the audio is delayed by the card
    // and the preroll tag is written, exactly as on the slow path.
    use base64::Engine as _;
    let b64 = format!("data:image/jpeg;base64,{}", base64::engine::general_purpose::STANDARD.encode(std::fs::read(&jpg).unwrap()));
    let mut pair = make_audio_pair("test_tone.wav", "fast_slated.mov", false, 0.0, -1.0);
    pair.video = Some(inspector::inspect_file(&src).unwrap());
    pair.slate_enabled = true;
    pair.slate_duration_secs = 3.0;
    pair.slate_black_secs = 1.0;
    pair.slate_image = Some(b64);
    let output = temp_output("fast_slated.mov");
    let r = processor::process_pair(&pair, &ExportSettings::default(), |_| {});
    assert!(r.success, "{:?}", r.error);
    let out = inspector::inspect_file(&output).unwrap();
    // 4 s preroll + 2 s of tone audio (-shortest): the slated picture is there and tagged.
    assert!(out.duration_secs > 5.5, "got {}", out.duration_secs);
    assert_eq!(out.slate_secs, Some(4.0));
    assert_eq!(out.codec_info.as_deref(), Some("h264"));

    cleanup(&output);
    cleanup(&text);
    cleanup(&src);
    cleanup(&pair.audio.path);
}

/// A source the fast path can't take (ProRes) still slates — via the full path.
#[test]
fn test_fast_slate_falls_back_for_non_h264() {
    use app_lib::services::fastslate;
    let dir = output_dir();
    let src = format!("{}/fast_prores.mov", dir);
    let jpg = test_fixture("slate_320x180.jpg");
    let args: Vec<String> = [
        "-y", "-loop", "1", "-framerate", "25", "-t", "2", "-i", jpg.as_str(),
        "-c:v", "prores_ks", "-profile:v", "1", src.as_str(),
    ].iter().map(|s| s.to_string()).collect();
    ffmpeg::run_ffmpeg(&args).expect("build prores source");
    let info = fastslate::probe(&src).expect("probe");
    assert!(fastslate::eligible(&info).unwrap_err().contains("not H.264"));
    let card = ffmpeg::SlateSpec { image_path: jpg.clone(), duration_secs: 1.0, black_secs: 0.0, matte_path: None, prebaked: false };
    assert!(processor::try_fast_slate(&src, &card, "test-prores").is_err());

    use base64::Engine as _;
    let b64 = format!("data:image/jpeg;base64,{}", base64::engine::general_purpose::STANDARD.encode(std::fs::read(&jpg).unwrap()));
    let mut pair = make_audio_pair("test_tone.wav", "fallback_slated.mov", false, 0.0, -1.0);
    pair.video = Some(inspector::inspect_file(&src).unwrap());
    pair.slate_enabled = true;
    pair.slate_duration_secs = 1.0;
    pair.slate_image = Some(b64);
    let output = temp_output("fallback_slated.mov");
    let r = processor::process_pair(&pair, &ExportSettings::default(), |_| {});
    assert!(r.success, "{:?}", r.error);
    assert!((inspector::inspect_file(&output).unwrap().duration_secs - 3.0).abs() < 0.2);
    cleanup(&output); cleanup(&src); cleanup(&pair.audio.path);
}

/// Click detection on real files: a tone with one injected single-sample
/// spike is found at the right time; the plain tone reports none.
#[test]
fn test_click_detection_on_real_files() {
    use app_lib::services::clicks;
    let dir = output_dir();
    let tone = test_fixture("test_tone.wav");
    let clean = clicks::detect_clicks(&tone, 48000, 2, "normal").expect("detect clean");
    assert_eq!(clean.count, 0, "{:?}", clean.clicks);

    // Inject a ~1-sample spike at 1.0 s on both channels with aeval.
    let spiked = format!("{}/click_spiked.wav", dir);
    let args: Vec<String> = [
        "-y", "-i", tone.as_str(),
        "-af", "aeval=val(0)+if(between(t\\,1.0\\,1.00002)\\,0.8\\,0)|val(1)+if(between(t\\,1.0\\,1.00002)\\,0.8\\,0)",
        "-c:a", "pcm_s24le", spiked.as_str(),
    ].iter().map(|s| s.to_string()).collect();
    ffmpeg::run_ffmpeg(&args).expect("inject click");
    let r = clicks::detect_clicks(&spiked, 48000, 2, "normal").expect("detect spiked");
    assert!(r.count >= 1 && r.count <= 4, "{:?}", r);
    assert!(r.clicks.iter().any(|c| (c.time - 1.0).abs() < 0.002), "{:?}", r.clicks);
    assert!(r.clicks.iter().all(|c| c.width_ms <= 0.5));
    // Sensitivity ordering holds on the real file too.
    let low = clicks::detect_clicks(&spiked, 48000, 2, "low").expect("low");
    let high = clicks::detect_clicks(&spiked, 48000, 2, "high").expect("high");
    assert!(low.count <= r.count && r.count <= high.count.max(r.count));
    cleanup(&spiked);
}

/// Clipping and dropout scan on real files.
#[test]
fn test_clipping_and_dropout_scan_on_real_files() {
    use app_lib::services::audioscan;
    let dir = output_dir();
    let tone = test_fixture("test_tone.wav");
    let clean = audioscan::scan(&tone, 48000, 2).expect("scan clean");
    assert_eq!(clean.clipping.count, 0, "{:?}", clean.clipping.events);
    assert_eq!(clean.dropouts.count, 0);

    // Driven 10× (the tone sits at −14 dBFS) and hard-clipped by aeval.
    let clipped = format!("{}/scan_clipped.wav", dir);
    let args: Vec<String> = ["-y", "-i", tone.as_str(),
        "-af", "aeval=min(max(val(0)*10\\,-1)\\,1)|min(max(val(1)*10\\,-1)\\,1)",
        "-c:a", "pcm_s24le", clipped.as_str()].iter().map(|s| s.to_string()).collect();
    ffmpeg::run_ffmpeg(&args).expect("clip");
    let r = audioscan::scan(&clipped, 48000, 2).expect("scan clipped");
    // Continuous clipping merges into one event per channel; the sample
    // count is what says how bad it is.
    assert!(r.clipping.count >= 1 && r.clipping.count <= 2, "{:?}", r.clipping.count);
    assert!(r.clipping.clipped_samples > 20_000, "{}", r.clipping.clipped_samples);
    assert!(r.clipping.events[0].kind == "hard");
    assert!(r.clipping.events[0].level_db > -0.1);
    assert_eq!(r.dropouts.count, 0);

    // A 120 ms hole at 0.8 s.
    let holed = format!("{}/scan_holed.wav", dir);
    let args: Vec<String> = ["-y", "-i", tone.as_str(),
        "-af", "aeval=if(between(t\\,0.8\\,0.92)\\,0\\,val(0))|if(between(t\\,0.8\\,0.92)\\,0\\,val(1))",
        "-c:a", "pcm_s24le", holed.as_str()].iter().map(|s| s.to_string()).collect();
    ffmpeg::run_ffmpeg(&args).expect("hole");
    let r = audioscan::scan(&holed, 48000, 2).expect("scan holed");
    assert_eq!(r.dropouts.count, 1, "{:?}", r.dropouts.gaps);
    assert!((r.dropouts.gaps[0].start - 0.8).abs() < 0.005, "{:?}", r.dropouts.gaps);
    assert!((r.dropouts.gaps[0].duration - 0.12).abs() < 0.005);
    assert_eq!(r.clipping.count, 0);
    cleanup(&clipped);
    cleanup(&holed);
}

/// Extract audio from a video: a WAV beside it, same length, same channels.
#[test]
fn test_extract_audio_from_video() {
    let dir = output_dir();
    let src = format!("{}/extract_src.mov", dir);
    build_h264_source(&src);
    let out = format!("{}/extract_src_audio.wav", dir);
    ffmpeg::run_ffmpeg(&ffmpeg::build_extract_audio_command(&src, &out)).expect("extract");
    let f = inspector::inspect_file(&out).unwrap();
    assert_eq!(f.codec_info.as_deref(), Some("pcm_s24le"));
    assert!((f.duration_secs - 6.0).abs() < 0.1, "{}", f.duration_secs);
    assert_eq!(f.channel_count, Some(2));
    cleanup(&out); cleanup(&src);
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

// ── Stem check ──────────────────────────────────────────────────────────────
// Synthetic stems: three independent noise signals (10 s, 48 kHz stereo) and a
// mix built from them in various deliberately wrong ways.

fn stem_noise(frames: usize, seed: u64, amp: f32) -> Vec<f32> {
    let mut x = seed;
    (0..frames * 2)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            ((x % 20001) as f32 / 10000.0 - 1.0) * amp
        })
        .collect()
}

fn write_wav(path: &str, samples: &[f32]) {
    let mut w = stemcheck::WavWriter::create(path, 48000, 2).unwrap();
    w.write(samples).unwrap();
    w.close().unwrap();
}

fn stem_file(path: &str, name: &str, secs: f64) -> stemcheck::StemFile {
    stemcheck::StemFile { path: path.to_string(), name: name.to_string(), duration_secs: secs }
}

struct StemFixture {
    dir: String,
    stems: Vec<Vec<f32>>,
    paths: Vec<String>,
}

fn stem_fixture(tag: &str) -> StemFixture {
    let dir = format!("{}/stems-{}", output_dir(), tag);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let frames = 48000 * 10;
    let stems = vec![stem_noise(frames, 11, 0.25), stem_noise(frames, 23, 0.15), stem_noise(frames, 37, 0.1)];
    let names = ["DX_stem.wav", "MX_stem.wav", "FX_stem.wav"];
    let mut paths = Vec::new();
    for (s, n) in stems.iter().zip(names) {
        let p = format!("{}/{}", dir, n);
        write_wav(&p, s);
        paths.push(p);
    }
    StemFixture { dir, stems, paths }
}

fn run_check(fx: &StemFixture, mix: &[f32], tag: &str) -> stemcheck::StemReport {
    let mix_path = format!("{}/{}_mix.wav", fx.dir, tag);
    write_wav(&mix_path, mix);
    let residual = format!("{}/{}_residual.wav", fx.dir, tag);
    let stems: Vec<_> = fx.paths.iter().zip(["DX", "MX", "FX"]).map(|(p, n)| stem_file(p, n, 10.0)).collect();
    stemcheck::stem_check(&stem_file(&mix_path, "MIX", 10.0), &stems, 48000, 2, &residual, &|_, _| {}).unwrap()
}

#[test]
fn test_stem_check_perfect_stems_match() {
    let fx = stem_fixture("perfect");
    let mix: Vec<f32> = (0..fx.stems[0].len()).map(|i| fx.stems.iter().map(|s| s[i]).sum()).collect();
    let r = run_check(&fx, &mix, "perfect");
    assert_eq!(r.verdict, "match", "residual {:.1} dB", r.overall_residual_db);
    assert!(r.overall_residual_db < -50.0, "residual {:.1}", r.overall_residual_db);
    assert!(r.gain_db.abs() < 0.1, "gain {}", r.gain_db);
    for s in &r.stems {
        assert!(s.aligned, "{} not aligned", s.name);
        assert!(s.lag_ms.abs() < 0.05, "{} lag {}", s.name, s.lag_ms);
        assert!(!s.polarity_flipped);
        assert!(s.presence_db.unwrap().abs() < 0.5, "{} presence {:?}", s.name, s.presence_db);
    }
    assert!(r.regions.is_empty(), "regions {:?}", r.regions);
    assert!(Path::new(&r.residual_path).exists());
    let _ = std::fs::remove_dir_all(&fx.dir);
}

#[test]
fn test_stem_check_finds_offset_polarity_and_gain() {
    let fx = stem_fixture("offset");
    // The MX stem carries 100 ms of extra head relative to the mix; FX is
    // inverted in the stem; and the whole mix sits 6 dB down.
    let n = fx.stems[0].len();
    let head = 4800 * 2; // 100 ms of stereo frames
    let mut mx_file = vec![0f32; head];
    mx_file.extend_from_slice(&fx.stems[1][..n - head]);
    write_wav(&fx.paths[1], &mx_file);
    let fx_flipped: Vec<f32> = fx.stems[2].iter().map(|v| -v).collect();
    write_wav(&fx.paths[2], &fx_flipped);
    let mix: Vec<f32> = (0..n).map(|i| (fx.stems[0][i] + fx.stems[1][i] + fx.stems[2][i]) * 0.5).collect();
    let r = run_check(&fx, &mix, "offset");
    let mx = r.stems.iter().find(|s| s.name == "MX").unwrap();
    assert!(mx.aligned);
    assert!((mx.lag_ms - 100.0).abs() < 0.1, "MX lag {}", mx.lag_ms);
    let fxs = r.stems.iter().find(|s| s.name == "FX").unwrap();
    assert!(fxs.polarity_flipped, "FX polarity not detected");
    assert!(fxs.lag_ms.abs() < 0.1, "FX lag {}", fxs.lag_ms);
    assert!((r.gain_db + 6.02).abs() < 0.2, "gain {}", r.gain_db);
    // Aligned and matched, what's left is the last 100 ms of MX, which the mix
    // has and the shifted stem no longer does: a small tail region, nothing else.
    assert!(r.overall_residual_db < -20.0, "residual {:.1}", r.overall_residual_db);
    assert!(r.regions.iter().all(|g| g.start >= 9.0), "regions {:?}", r.regions);
    let _ = std::fs::remove_dir_all(&fx.dir);
}

#[test]
fn test_stem_check_flags_a_muted_stretch_and_names_the_stem() {
    let fx = stem_fixture("muted");
    // The mix drops the DX stem between 4 s and 6 s.
    let n = fx.stems[0].len();
    let mix: Vec<f32> = (0..n)
        .map(|i| {
            let t = (i / 2) as f64 / 48000.0;
            let dx = if (4.0..6.0).contains(&t) { 0.0 } else { fx.stems[0][i] };
            dx + fx.stems[1][i] + fx.stems[2][i]
        })
        .collect();
    let r = run_check(&fx, &mix, "muted");
    assert_ne!(r.verdict, "match");
    assert!(!r.regions.is_empty(), "no regions flagged");
    let reg = r.regions.iter().max_by(|a, b| (a.end - a.start).partial_cmp(&(b.end - b.start)).unwrap()).unwrap();
    assert!((reg.start - 4.0).abs() <= 0.5 && (reg.end - 6.0).abs() <= 0.5, "region {:?}", reg);
    assert_eq!(reg.likely.as_deref(), Some("DX"), "region {:?}", reg);
    assert_eq!(reg.direction.as_deref(), Some("stems_only"));
    // Outside the hole everything cancels, so the overall gain is still ~unity.
    assert!(r.gain_db.abs() < 1.0, "gain {}", r.gain_db);
    let _ = std::fs::remove_dir_all(&fx.dir);
}

#[test]
fn test_stem_check_reports_a_stem_missing_from_the_mix() {
    let fx = stem_fixture("missing");
    // The mix never had FX in it at all.
    let n = fx.stems[0].len();
    let mix: Vec<f32> = (0..n).map(|i| fx.stems[0][i] + fx.stems[1][i]).collect();
    let r = run_check(&fx, &mix, "missing");
    let fxs = r.stems.iter().find(|s| s.name == "FX").unwrap();
    assert!(fxs.presence_db.is_none() || fxs.presence_db.unwrap() < -20.0, "FX presence {:?}", fxs.presence_db);
    assert!(!fxs.aligned, "an absent stem should not claim alignment");
    assert_ne!(r.verdict, "match");
    let _ = std::fs::remove_dir_all(&fx.dir);
}

/// Smoothed noise: correlates with itself for a while, like music or speech.
fn stem_smooth(frames: usize, seed: u64, amp: f32) -> Vec<f32> {
    let raw = stem_noise(frames, seed, amp * 6.0);
    let mut out = vec![0f32; raw.len()];
    let mut acc = [0f32; 2];
    for (i, &v) in raw.iter().enumerate() {
        let c = i % 2;
        acc[c] = 0.96 * acc[c] + 0.04 * v;
        out[i] = acc[c];
    }
    out
}

#[test]
fn test_stem_check_music_stem_lower_in_mix_with_extra_content() {
    // Richard's case: the music stem is in the mix, but 8 dB down and under
    // applause the stems don't have. Smooth signals, like real material.
    let dir = format!("{}/stems-lowmx", output_dir());
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let frames = 48000 * 12;
    let dx = stem_smooth(frames, 101, 0.25);
    let mx = stem_smooth(frames, 202, 0.25);
    let applause = stem_noise(frames, 303, 0.03);
    let mx_gain = 10f32.powf(-8.0 / 20.0);
    let mix: Vec<f32> = (0..dx.len()).map(|i| dx[i] + mx_gain * mx[i] + applause[i]).collect();
    let paths = [format!("{}/DX_stem.wav", dir), format!("{}/MX_stem.wav", dir), format!("{}/mix.wav", dir)];
    write_wav(&paths[0], &dx);
    write_wav(&paths[1], &mx);
    write_wav(&paths[2], &mix);
    let residual = format!("{}/residual.wav", dir);
    let stems = vec![stem_file(&paths[0], "DX", 12.0), stem_file(&paths[1], "MX", 12.0)];
    let r = stemcheck::stem_check(&stem_file(&paths[2], "MIX", 12.0), &stems, 48000, 2, &residual, &|_, _| {}).unwrap();
    let m = r.stems.iter().find(|s| s.name == "MX").unwrap();
    assert!(m.aligned, "MX should align: {:?}", m);
    assert!(m.lag_ms.abs() < 0.1, "MX lag {}", m.lag_ms);
    assert!((m.presence_db.unwrap() + 8.0).abs() < 0.5, "MX presence {:?}", m.presence_db);
    let d = r.stems.iter().find(|s| s.name == "DX").unwrap();
    assert!(d.presence_db.unwrap().abs() < 0.3, "DX presence {:?}", d.presence_db);
    // What's left is the applause: well below the mix, but the level gap is flagged.
    assert!(r.overall_residual_db < -15.0, "residual {:.1}", r.overall_residual_db);
    assert_ne!(r.verdict, "match");
    assert!(r.notes.iter().any(|n| n.contains("MX") && n.contains("lower")), "notes {:?}", r.notes);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_stem_check_corrects_a_speed_drift() {
    // The MX stem runs 0.1 % long (a 23.976/24 pull-down): without correction
    // it only cancels at one point; with it, the whole file nulls.
    let dir = format!("{}/stems-drift", output_dir());
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let frames = 48000 * 30;
    let dx = stem_smooth(frames, 11, 0.25);
    let mx = stem_smooth(frames, 22, 0.25);
    let mix: Vec<f32> = (0..dx.len()).map(|i| dx[i] + mx[i]).collect();
    // Stretch MX by 1.001 with linear interpolation, per channel.
    let factor = 1.001f64;
    let out_frames = (frames as f64 * factor) as usize;
    let mut mx_long = vec![0f32; out_frames * 2];
    for f in 0..out_frames {
        let src = f as f64 / factor;
        let i0 = src.floor() as usize;
        let t = (src - i0 as f64) as f32;
        let i1 = (i0 + 1).min(frames - 1);
        for c in 0..2 {
            mx_long[f * 2 + c] = mx[i0 * 2 + c] * (1.0 - t) + mx[i1 * 2 + c] * t;
        }
    }
    let paths = [format!("{}/DX_stem.wav", dir), format!("{}/MX_stem.wav", dir), format!("{}/mix.wav", dir)];
    write_wav(&paths[0], &dx);
    write_wav(&paths[1], &mx_long);
    write_wav(&paths[2], &mix);
    let residual = format!("{}/residual.wav", dir);
    let stems = vec![stem_file(&paths[0], "DX", 30.0), stem_file(&paths[1], "MX", 30.03)];
    let r = stemcheck::stem_check(&stem_file(&paths[2], "MIX", 30.0), &stems, 48000, 2, &residual, &|_, _| {}).unwrap();
    let m = r.stems.iter().find(|s| s.name == "MX").unwrap();
    assert!(m.aligned, "{:?}", m);
    assert!((m.speed_pct - 0.1).abs() < 0.02, "MX speed {} ({:?})", m.speed_pct, m.note);
    assert!(r.overall_residual_db < -20.0, "residual {:.1} stems {:?} notes {:?}", r.overall_residual_db, r.stems, r.notes);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Diagnostic, not a test: run the stem check on real files and print the
/// report. `STEM_MIX=/path/mix.wav STEM_STEMS=/a.wav:/b.wav STEM_DEBUG=1
/// cargo test --test integration_test stem_check_on_real_files -- --ignored --nocapture`
#[test]
#[ignore]
fn stem_check_on_real_files() {
    let mix = std::env::var("STEM_MIX").expect("STEM_MIX");
    let stems_env = std::env::var("STEM_STEMS").expect("STEM_STEMS");
    let dur = |p: &str| inspector::inspect_file(p).map(|m| m.duration_secs).unwrap_or(0.0);
    let probe = inspector::inspect_file(&mix).expect("probe mix");
    let stems: Vec<_> = stems_env
        .split(':')
        .filter(|p| !p.is_empty())
        .map(|p| stem_file(p, Path::new(p).file_stem().unwrap().to_str().unwrap(), dur(p)))
        .collect();
    let residual = temp_output("real_residual.wav");
    let r = stemcheck::stem_check(
        &stem_file(&mix, "MIX", probe.duration_secs),
        &stems,
        probe.sample_rate.unwrap_or(48000.0) as u32,
        probe.channel_count.unwrap_or(2),
        &residual,
        &|stage, f| eprintln!("  {} {:.0}%", stage, f * 100.0),
    )
    .unwrap();
    eprintln!("{}", serde_json::to_string_pretty(&r).unwrap());
}

/// Diagnostic: ground truth on real files, no alignment — do the stems sum
/// to the mix as delivered, and at what levels? Same env vars as above.
#[test]
#[ignore]
#[allow(clippy::needless_range_loop)]
fn stem_check_ground_truth() {
    let mix_path = std::env::var("STEM_MIX").expect("STEM_MIX");
    let stems_env = std::env::var("STEM_STEMS").expect("STEM_STEMS");
    let paths: Vec<String> = stems_env.split(':').filter(|p| !p.is_empty()).map(String::from).collect();
    let mix = stemcheck::decode_whole(&mix_path, 48000, 2).unwrap();
    let stems: Vec<Vec<f32>> = paths.iter().map(|p| stemcheck::decode_whole(p, 48000, 2).unwrap()).collect();
    let n = stems.iter().map(|s| s.len()).min().unwrap().min(mix.len());
    let db = |x: f64| if x > 0.0 { 10.0 * x.log10() } else { f64::NEG_INFINITY };
    let em: f64 = mix[..n].iter().map(|v| (*v as f64).powi(2)).sum();
    eprintln!("mix rms {:.1} dBFS, {} frames", db(em / n as f64), n / 2);
    let k = stems.len();
    let mut sts = vec![vec![0f64; k]; k];
    let mut stm = vec![0f64; k];
    for i in 0..n {
        let mv = mix[i] as f64;
        for a in 0..k {
            let va = stems[a][i] as f64;
            stm[a] += va * mv;
            for b in a..k {
                sts[a][b] += va * stems[b][i] as f64;
            }
        }
    }
    for a in 0..k {
        for b in 0..a {
            sts[a][b] = sts[b][a];
        }
    }
    for (a, p) in paths.iter().enumerate() {
        let name = Path::new(p).file_stem().unwrap().to_str().unwrap();
        let es = sts[a][a];
        eprintln!(
            "{}: rms {:.1} dBFS, corr with mix {:+.3}, LS gain alone {:+.3} ({:+.1} dB)",
            name,
            db(es / n as f64),
            if es > 0.0 { stm[a] / (es * em).sqrt() } else { 0.0 },
            if es > 0.0 { stm[a] / es } else { 0.0 },
            if es > 0.0 && stm[a] > 0.0 { 20.0 * (stm[a] / es).log10() } else { f64::NAN }
        );
    }
    let g = stemcheck::solve_gains(&sts, &stm);
    eprintln!("joint gains: {:?}", g.iter().map(|v| format!("{:+.3} ({:+.1} dB)", v, if *v > 0.0 { 20.0 * v.log10() } else { f64::NAN })).collect::<Vec<_>>());
    // Residual of the unity sum at a few whole-frame offsets of the sum.
    for lag in [-24i64, -12, -6, -3, -1, 0, 1, 3, 6, 12, 24] {
        let mut er = 0f64;
        for i in 0..n {
            let j = i as i64 + 2 * lag;
            let sum: f64 = if j >= 0 && (j as usize) < n { stems.iter().map(|s| s[j as usize] as f64).sum() } else { 0.0 };
            er += (mix[i] as f64 - sum).powi(2);
        }
        eprintln!("unity sum shifted {:+3} frames: residual {:6.1} dB rel mix", lag, db(er / em));
    }
    // Joint-gain residual, per second.
    let w = 48000 * 2;
    let mut line = String::new();
    for t in 0..n / w {
        let (mut e_m, mut e_r) = (0f64, 0f64);
        for i in t * w..(t + 1) * w {
            let mv = mix[i] as f64;
            let sum: f64 = (0..k).map(|a| g[a] * stems[a][i] as f64).sum();
            e_m += mv * mv;
            e_r += (mv - sum).powi(2);
        }
        line.push_str(&if e_m / w as f64 > 1e-6 { format!("{:2}:{:5.1} ", t, db(e_r / e_m)) } else { format!("{:2}:  --  ", t) });
    }
    eprintln!("joint-fit residual per second (dB rel mix):\n{}", line);
}

/// Diagnostic: per-stem full-rate lag scan on the loudest 10 s of each stem,
/// then per-second gains at that lag — is the mix bus processed?
#[test]
#[ignore]
#[allow(clippy::needless_range_loop)]
fn stem_check_lag_scan() {
    let mix_path = std::env::var("STEM_MIX").expect("STEM_MIX");
    let stems_env = std::env::var("STEM_STEMS").expect("STEM_STEMS");
    let paths: Vec<String> = stems_env.split(':').filter(|p| !p.is_empty()).map(String::from).collect();
    let mix = stemcheck::decode_whole(&mix_path, 48000, 1).unwrap();
    let db = |x: f64| if x > 0.0 { 10.0 * x.log10() } else { f64::NEG_INFINITY };
    let peak = |v: &[f32]| 20.0 * (v.iter().fold(0f32, |m, x| m.max(x.abs())) as f64).log10();
    let mut sum = vec![0f64; mix.len()];
    for p in &paths {
        let stem = stemcheck::decode_whole(p, 48000, 1).unwrap();
        let name = Path::new(p).file_stem().unwrap().to_str().unwrap().to_string();
        let n = stem.len().min(mix.len());
        for i in 0..n { sum[i] += stem[i] as f64; }
        // loudest 10 s window of the stem
        let w = 48000 * 10;
        let mut best_t = 0; let mut best_e = -1.0;
        let mut t = 0;
        while t + w <= n {
            let e: f64 = stem[t..t + w].iter().map(|v| (*v as f64).powi(2)).sum();
            if e > best_e { best_e = e; best_t = t; }
            t += 48000;
        }
        let seg = &stem[best_t..best_t + w];
        let es: f64 = seg.iter().map(|v| (*v as f64).powi(2)).sum();
        let mut best = (0i64, 0f64);
        let max_lag = 2400i64;
        for lag in -max_lag..=max_lag {
            let mut xy = 0f64; let mut yy = 0f64;
            for (i, sv) in seg.iter().enumerate() {
                let j = best_t as i64 + i as i64 - lag; // stem at i matches mix at i - lag (lag = stem - mix time)
                if j >= 0 && (j as usize) < mix.len() {
                    let mv = mix[j as usize] as f64;
                    xy += *sv as f64 * mv; yy += mv * mv;
                }
            }
            let c = if yy > 0.0 { xy / (es * yy).sqrt() } else { 0.0 };
            if c.abs() > best.1.abs() { best = (lag, c); }
        }
        eprintln!("{}: loudest 10 s at {:.0} s (rms {:.1} dBFS, peak {:.1} dBFS); best lag {:+} frames ({:+.2} ms) ncc {:+.3}",
            name, best_t as f64 / 48000.0, db(es / w as f64), peak(seg), best.0, best.0 as f64 / 48.0, best.1);
        // per-second LS gain at the best lag
        let lag = best.0;
        let mut line = String::new();
        for sec in 0..n / 48000 {
            let (mut xy, mut xx) = (0f64, 0f64);
            for i in sec * 48000..(sec + 1) * 48000 {
                let j = i as i64 - lag;
                if j >= 0 && (j as usize) < mix.len() { xy += stem[i] as f64 * mix[j as usize] as f64; xx += (stem[i] as f64).powi(2); }
            }
            line.push_str(&if xx / 48000.0 > 1e-7 { let g = xy / xx; format!("{:2}:{:>6} ", sec, if g > 0.0 { format!("{:+.1}", 20.0 * g.log10()) } else { "neg".into() }) } else { format!("{:2}:   -- ", sec) });
        }
        eprintln!("  per-second gain (dB) at that lag:\n  {}", line);
    }
    let m: Vec<f32> = mix.clone();
    let s32: Vec<f32> = sum.iter().map(|v| *v as f32).collect();
    eprintln!("mix peak {:.1} dBFS rms {:.1}; unity stem sum peak {:.1} dBFS rms {:.1}",
        peak(&m), db(m.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / m.len() as f64),
        peak(&s32), db(sum.iter().map(|v| v * v).sum::<f64>() / sum.len() as f64));
}
