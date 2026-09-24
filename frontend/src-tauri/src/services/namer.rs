use crate::models::*;
use strsim::normalized_levenshtein;
use std::collections::HashMap;

const SIMILARITY_THRESHOLD: f64 = 0.6;

/// Generate output filename for a matched pair
pub fn generate_name(
    video: &MediaFile,
    audio: &MediaFile,
    remove_duplicates: bool,
    output_ext: &str,
) -> String {
    let video_name = &video.filename_no_ext;
    let audio_name = &audio.filename_no_ext;

    let similarity = normalized_levenshtein(
        &video_name.to_lowercase(),
        &audio_name.to_lowercase(),
    );

    let base_name = if similarity > SIMILARITY_THRESHOLD && remove_duplicates {
        // Names are very similar — use video name + unique part from audio
        let unique_part = remove_duplicate_info(video_name, audio_name);
        if unique_part.is_empty() {
            format!("{}_with audio", video_name)
        } else {
            format!("{}_with audio_{}", video_name, unique_part)
        }
    } else if remove_duplicates {
        let unique_part = remove_duplicate_info(video_name, audio_name);
        if unique_part.is_empty() {
            format!("{}_with audio", video_name)
        } else {
            format!("{}_{}", video_name, unique_part)
        }
    } else {
        format!("{}_{}", video_name, audio_name)
    };

    format!("{}.{}", base_name, output_ext)
}

/// Generate names for all pairs, ensuring uniqueness
/// Strip a loudness-spec suffix this app appended previously, so re-processing
/// an output doesn't stack them (e.g. "MyMix_-23LUFS_-1dBTP" -> "MyMix").
/// Also strips the legacy "_normalised_..." marker written by older versions.
fn strip_spec_suffix(name: &str) -> &str {
    // Legacy marker: drop everything from "_normalised_" onwards.
    let mut s = match name.find("_normalised_") {
        Some(idx) => &name[..idx],
        None => name,
    };
    // Trailing conversion tags, in reverse order of how they were added:
    // "_16bit" / "_24bit" / "_32f", then "_44.1k" / "_48k" / "_96k".
    if let Some(i) = s.rfind('_') {
        let tail = &s[i + 1..];
        let is_depth = tail == "32f"
            || tail.strip_suffix("bit").map(|n| n.parse::<u32>().is_ok()).unwrap_or(false);
        if is_depth {
            s = &s[..i];
        }
    }
    if let Some(i) = s.rfind('_') {
        let tail = &s[i + 1..];
        if tail.strip_suffix('k').map(|n| n.parse::<f64>().is_ok()).unwrap_or(false) {
            s = &s[..i];
        }
    }
    // Trailing "_Clocked"
    if let Some(rest) = s.strip_suffix("_Clocked") {
        s = rest;
    }
    // Trailing "_6Fr"
    if let Some(rest) = s.strip_suffix("_6Fr") {
        s = rest;
    }
    // Trailing "_<number>dBTP"
    if let Some(i) = s.rfind('_') {
        if let Some(num) = s[i + 1..].strip_suffix("dBTP") {
            if num.parse::<f64>().is_ok() {
                s = &s[..i];
            }
        }
    }
    // Trailing "_<number>LUFS"
    if let Some(i) = s.rfind('_') {
        if let Some(num) = s[i + 1..].strip_suffix("LUFS") {
            if num.parse::<f64>().is_ok() {
                s = &s[..i];
            }
        }
    }
    s
}

/// Source extensions that can't hold PCM: a re-encode of one of these has to
/// land in a WAV instead (the bundled ffmpeg has no MP3 encoder either).
pub fn is_lossy_ext(ext: &str) -> bool {
    matches!(ext.to_lowercase().as_str(), "mp3" | "aac" | "ogg" | "oga" | "opus" | "wma" | "ac3" | "eac3")
}

/// Name tag for a conversion: "_48k", "_44.1k_16bit", "_32f"… Empty when the
/// spec keeps the source's rate and depth.
pub fn conversion_tag(spec: &AudioOutputSpec) -> String {
    let mut tag = String::new();
    if let Some(rate) = spec.sample_rate {
        if rate % 1000 == 0 {
            tag.push_str(&format!("_{}k", rate / 1000));
        } else {
            tag.push_str(&format!("_{}k", rate as f64 / 1000.0));
        }
    }
    match spec.bit_depth {
        Some(32) => tag.push_str("_32f"),
        Some(d) => tag.push_str(&format!("_{}bit", d)),
        None => {}
    }
    tag
}

/// The extension an audio-only output gets: the chosen container's, or the
/// source's own — unless the source is lossy and the file is being re-encoded,
/// in which case it becomes a WAV.
pub fn audio_output_ext(source_ext: &str, spec: Option<&AudioOutputSpec>, will_encode: bool) -> String {
    if let Some(ext) = spec.and_then(|s| s.container.extension()) {
        return ext.to_string();
    }
    if will_encode && is_lossy_ext(source_ext) {
        return "wav".to_string();
    }
    source_ext.to_string()
}

pub fn generate_names(pairs: &mut [MatchedPair], remove_duplicates: bool, output_ext: &str) {
    generate_names_with_audio(pairs, remove_duplicates, output_ext, None);
}

/// As `generate_names`, with the audio-only output spec: the container decides
/// the extension, and a sample-rate / bit-depth change is recorded in the name.
pub fn generate_names_with_audio(
    pairs: &mut [MatchedPair],
    remove_duplicates: bool,
    output_ext: &str,
    audio: Option<&AudioOutputSpec>,
) {
    let conv_tag = audio.map(conversion_tag).unwrap_or_default();
    // First pass: generate names
    for pair in pairs.iter_mut() {
        if let Some(ref video) = pair.video {
            pair.output_filename = generate_name(video, &pair.audio, remove_duplicates, output_ext);
        } else {
            // Audio-only: include the norm spec in the filename if enabled.
            // Strip existing spec markers only when we're re-applying a spec,
            // so a plain passthrough keeps the file's real name — e.g. a
            // peak-normalised "MyMix_-1dBTP.wav" reloaded after the pass still
            // reads _-1dBTP instead of being flattened back to "MyMix".
            let any_spec = pair.normalization_enabled
                || pair.silence_compliance
                || pair.clock_enabled
                || !conv_tag.is_empty();
            let base_name = if any_spec {
                strip_spec_suffix(&pair.audio.filename_no_ext).to_string()
            } else {
                pair.audio.filename_no_ext.clone()
            };
            // NORM and Clock compose; the name records each step that was
            // actually applied. In full-scale mode the peak IS the spec, so the
            // name carries the dBTP target; in loudness mode it carries LUFS.
            // Applied in processing order: normalise, then the 6-frame mute,
            // then the clock handles — the name reflects each step taken.
            let mut name = base_name;
            if pair.normalization_enabled {
                if pair.normalization_settings.target_lufs >= 0.0 {
                    name = format!("{}_{}dBTP", name, pair.normalization_settings.true_peak_limit);
                } else {
                    name = format!("{}_{}LUFS", name, pair.normalization_settings.target_lufs);
                }
            }
            if pair.silence_compliance {
                name = format!("{}_6Fr", name);
            }
            if pair.clock_enabled {
                name = format!("{}_Clocked", name);
            }
            name.push_str(&conv_tag);
            let ext = audio_output_ext(&pair.audio.extension, audio, any_spec);
            pair.output_filename = format!("{}.{}", name, ext);
        }
    }

    // Second pass: check for duplicate output names and disambiguate
    let mut name_counts: HashMap<String, usize> = HashMap::new();
    for pair in pairs.iter() {
        *name_counts.entry(pair.output_filename.clone()).or_insert(0) += 1;
    }

    // For any duplicates, append a number suffix
    let mut name_indices: HashMap<String, usize> = HashMap::new();
    for pair in pairs.iter_mut() {
        if let Some(&count) = name_counts.get(&pair.output_filename) {
            if count > 1 {
                let idx = name_indices.entry(pair.output_filename.clone()).or_insert(0);
                *idx += 1;
                // Split on the actual extension in the filename, not the video output_ext
                if let Some(dot_pos) = pair.output_filename.rfind('.') {
                    let stem = &pair.output_filename[..dot_pos];
                    let ext = &pair.output_filename[dot_pos + 1..];
                    pair.output_filename = format!("{}_{}.{}", stem, idx, ext);
                } else {
                    pair.output_filename = format!("{}_{}", pair.output_filename, idx);
                }
            }
        }
    }
}

/// Find and remove the parts of audio_name that are duplicated from video_name.
/// Works on underscore/hyphen-delimited segments to avoid splitting numbers.
fn remove_duplicate_info(video_name: &str, audio_name: &str) -> String {
    // Split both names into segments by common delimiters
    let video_segments: Vec<&str> = video_name.split(['_', '-', ' '])
        .filter(|s| !s.is_empty())
        .collect();
    let audio_segments: Vec<&str> = audio_name.split(['_', '-', ' '])
        .filter(|s| !s.is_empty())
        .collect();

    // Keep audio segments that don't appear in video segments
    let unique_segments: Vec<&str> = audio_segments
        .iter()
        .filter(|seg| !video_segments.contains(seg))
        .copied()
        .collect();

    let result = unique_segments.join("_");

    // Clean up
    result
        .trim_matches(|c: char| c == '_' || c == '-' || c == ' ')
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_video(name: &str) -> MediaFile {
        MediaFile {
            id: "1".into(),
            path: format!("/test/{}.mov", name),
            filename: format!("{}.mov", name),
            filename_no_ext: name.to_string(),
            extension: "mov".into(),
            media_type: MediaType::Video,
            duration_secs: 55.0,
            codec_info: None,
            sample_rate: None,
            channel_count: None,
            frame_rate: None,
            width: None,
            height: None,
            slate_secs: None,
            channel_layout: None,
            bit_depth: None,
            bit_rate: None,
            thumbnail_data: None,
        }
    }

    fn make_audio(name: &str) -> MediaFile {
        MediaFile {
            id: "2".into(),
            path: format!("/test/{}.wav", name),
            filename: format!("{}.wav", name),
            filename_no_ext: name.to_string(),
            extension: "wav".into(),
            media_type: MediaType::Audio,
            duration_secs: 55.0,
            codec_info: None,
            sample_rate: None,
            channel_count: None,
            frame_rate: None,
            width: None,
            height: None,
            slate_secs: None,
            channel_layout: None,
            bit_depth: None,
            bit_rate: None,
            thumbnail_data: None,
        }
    }

    #[test]
    fn test_similar_names_preserve_unique_part() {
        let video = make_video("ITV_TheNeighbourhood_50_240326_1445");
        let audio = make_audio("ITV_TheNeighbourhood_50_240326_1530");

        let name = generate_name(&video, &audio, true, "mov");
        assert!(name.contains("1530"), "Expected unique part '1530' in name, got: {}", name);
    }

    #[test]
    fn test_identical_names_get_with_audio() {
        let video = make_video("ITV_TheNeighbourhood_50_240326_1445");
        let audio = make_audio("ITV_TheNeighbourhood_50_240326_1445");

        let name = generate_name(&video, &audio, true, "mov");
        assert!(name.contains("with audio"), "Expected 'with audio' suffix, got: {}", name);
    }

    #[test]
    fn test_jam_milwaukee_scenario() {
        let video = make_video("Jam_Milwaukee_15_v3_9x16_0500226");
        let audio = make_audio("Jam_Milwaukee_15_v3_050226");

        let name = generate_name(&video, &audio, true, "mov");
        assert!(name.contains("with audio"), "Expected 'with audio' in name, got: {}", name);
    }

    #[test]
    fn test_itv_unique_parts() {
        let unique = remove_duplicate_info(
            "ITV_TheNeighbourhood_50_240326_1445",
            "ITV_TheNeighbourhood_50_240326_1530",
        );
        assert_eq!(unique, "1530", "Expected '1530', got: '{}'", unique);
    }

    fn make_pair(video_name: Option<&str>, audio_name: &str, norm_enabled: bool, target_lufs: f64, tp_limit: f64) -> MatchedPair {
        MatchedPair {
            id: "test".into(),
            video: video_name.map(make_video),
            audio: make_audio(audio_name),
            output_filename: String::new(),
            normalization_enabled: norm_enabled,
            normalization_settings: NormalizationSettings {
                target_lufs,
                true_peak_limit: tp_limit,
            },
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
            slate_matte: None,
        }
    }

    #[test]
    fn test_audio_only_no_norm() {
        let mut pairs = vec![make_pair(None, "MyMix_Final", false, 0.0, -1.0)];
        generate_names(&mut pairs, true, "wav");
        assert_eq!(pairs[0].output_filename, "MyMix_Final.wav");
    }

    #[test]
    fn test_audio_only_lufs_norm() {
        let mut pairs = vec![make_pair(None, "MyMix_Final", true, -23.0, -1.0)];
        generate_names(&mut pairs, true, "wav");
        assert_eq!(pairs[0].output_filename, "MyMix_Final_-23LUFS.wav");
        assert!(!pairs[0].output_filename.contains("normalised"));
    }

    #[test]
    fn test_audio_only_fullscale_norm() {
        let mut pairs = vec![make_pair(None, "MyMix_Final", true, 0.0, -1.0)];
        generate_names(&mut pairs, true, "wav");
        assert_eq!(pairs[0].output_filename, "MyMix_Final_-1dBTP.wav");
        assert!(!pairs[0].output_filename.contains("normalised"));
    }

    #[test]
    fn test_audio_only_strips_existing_spec_suffix() {
        // Re-processing one of our own outputs must not stack suffixes.
        let mut pairs = vec![make_pair(None, "MyMix_Final_-23LUFS_-1dBTP", true, -16.0, -1.0)];
        generate_names(&mut pairs, true, "wav");
        assert_eq!(pairs[0].output_filename, "MyMix_Final_-16LUFS.wav");
        assert!(!pairs[0].output_filename.contains("-23"));
    }

    #[test]
    fn test_audio_only_strips_legacy_normalised_suffix() {
        // Files named by older versions carried a "_normalised_" marker.
        let mut pairs = vec![make_pair(None, "MyMix_Final_normalised_-23LUFS_-1dBTP", true, -16.0, -1.0)];
        generate_names(&mut pairs, true, "wav");
        assert_eq!(pairs[0].output_filename, "MyMix_Final_-16LUFS.wav");
        assert!(!pairs[0].output_filename.contains("normalised"));
    }

    #[test]
    fn test_audio_only_strips_fullscale_spec_suffix() {
        let mut pairs = vec![make_pair(None, "MyMix_-1dBTP", true, -23.0, -1.0)];
        generate_names(&mut pairs, true, "wav");
        assert_eq!(pairs[0].output_filename, "MyMix_-23LUFS.wav");
    }

    #[test]
    fn test_audio_only_passthrough_keeps_the_files_real_name() {
        // No spec is being applied (plain passthrough) — keep the file's real
        // name, including any spec it already carries. This is what a batch
        // reload shows after a normalise pass, so the deliverable's name is
        // visible in the list rather than flattened back to the bare stem.
        let mut peak = vec![make_pair(None, "MyMix_-1dBTP", false, 0.0, -1.0)];
        generate_names(&mut peak, true, "wav");
        assert_eq!(peak[0].output_filename, "MyMix_-1dBTP.wav");

        let mut lufs = vec![make_pair(None, "MyMix_-23LUFS", false, -23.0, -1.0)];
        generate_names(&mut lufs, true, "wav");
        assert_eq!(lufs[0].output_filename, "MyMix_-23LUFS.wav");
    }

    #[test]
    fn test_audio_only_renormalise_still_strips_and_doesnt_stack() {
        // Re-normalising an existing output DOES strip first, so specs never
        // stack (e.g. peak-renormalising our own dBTP output stays single).
        let mut pairs = vec![make_pair(None, "MyMix_-1dBTP", true, 0.0, -2.0)];
        generate_names(&mut pairs, true, "wav");
        assert_eq!(pairs[0].output_filename, "MyMix_-2dBTP.wav");
    }

    #[test]
    fn test_audio_only_clocked_name() {
        // Clocked files keep their level, so the name records "Clocked" and
        // carries no norm spec.
        let mut pairs = vec![make_pair(None, "MyMix", false, -23.0, -1.0)];
        pairs[0].clock_enabled = true;
        generate_names(&mut pairs, true, "wav");
        assert_eq!(pairs[0].output_filename, "MyMix_Clocked.wav");
    }

    #[test]
    fn test_audio_only_clocked_strips_previous_spec_and_doesnt_stack() {
        // Clocking one of our own normalised outputs: drop the old spec, and
        // re-clocking a clocked file must not stack "_Clocked_Clocked".
        let mut pairs = vec![make_pair(None, "MyMix_-23LUFS_-1dBTP", false, -23.0, -1.0)];
        pairs[0].clock_enabled = true;
        generate_names(&mut pairs, true, "wav");
        assert_eq!(pairs[0].output_filename, "MyMix_Clocked.wav");

        let mut again = vec![make_pair(None, "MyMix_Clocked", false, -23.0, -1.0)];
        again[0].clock_enabled = true;
        generate_names(&mut again, true, "wav");
        assert_eq!(again[0].output_filename, "MyMix_Clocked.wav");
    }

    #[test]
    fn test_audio_only_norm_and_clock_compose() {
        // Levelled AND clocked in one export: the name records both steps.
        let mut pairs = vec![make_pair(None, "MyMix", true, -23.0, -1.0)];
        pairs[0].clock_enabled = true;
        generate_names(&mut pairs, true, "wav");
        assert_eq!(pairs[0].output_filename, "MyMix_-23LUFS_Clocked.wav");

        // And re-dropping that output doesn't stack the suffixes.
        let mut again = vec![make_pair(None, "MyMix_-23LUFS_Clocked", true, -23.0, -1.0)];
        again[0].clock_enabled = true;
        generate_names(&mut again, true, "wav");
        assert_eq!(again[0].output_filename, "MyMix_-23LUFS_Clocked.wav");
    }

    #[test]
    fn test_audio_only_6fr_name() {
        let mut pairs = vec![make_pair(None, "MyMix", false, -23.0, -1.0)];
        pairs[0].silence_compliance = true;
        generate_names(&mut pairs, true, "wav");
        assert_eq!(pairs[0].output_filename, "MyMix_6Fr.wav");
    }

    #[test]
    fn test_audio_only_norm_6fr_clock_compose_and_dont_stack() {
        let mut pairs = vec![make_pair(None, "MyMix", true, -23.0, -1.0)];
        pairs[0].silence_compliance = true;
        pairs[0].clock_enabled = true;
        generate_names(&mut pairs, true, "wav");
        assert_eq!(pairs[0].output_filename, "MyMix_-23LUFS_6Fr_Clocked.wav");

        // Re-dropping that output must not stack the suffixes.
        let mut again = vec![make_pair(None, "MyMix_-23LUFS_6Fr_Clocked", true, -23.0, -1.0)];
        again[0].silence_compliance = true;
        again[0].clock_enabled = true;
        generate_names(&mut again, true, "wav");
        assert_eq!(again[0].output_filename, "MyMix_-23LUFS_6Fr_Clocked.wav");
    }

    #[test]
    fn test_audio_only_keeps_unrelated_name_parts() {
        // Don't chew off legitimate name segments that aren't a spec suffix.
        let mut pairs = vec![make_pair(None, "Spot_v2_Mix", true, -23.0, -1.0)];
        generate_names(&mut pairs, true, "wav");
        assert_eq!(pairs[0].output_filename, "Spot_v2_Mix_-23LUFS.wav");
    }

    #[test]
    fn test_duplicate_filenames_get_disambiguated() {
        let mut pairs = vec![
            make_pair(Some("Commercial_30s"), "Mix_A", false, 0.0, -1.0),
            make_pair(Some("Commercial_30s"), "Mix_B", false, 0.0, -1.0),
        ];
        // With these names, remove_duplicates will produce identical output names
        // since both audio names differ from video. But the disambiguation pass should handle it.
        generate_names(&mut pairs, false, "mov");
        // Both should have unique names
        assert_ne!(pairs[0].output_filename, pairs[1].output_filename);
    }

    #[test]
    fn test_audio_only_preserves_original_extension() {
        let mut pair = make_pair(None, "MyTrack", true, -23.0, -1.0);
        pair.audio.extension = "aiff".to_string();
        let mut pairs = vec![pair];
        generate_names(&mut pairs, true, "mov"); // output_ext shouldn't matter for audio-only
        assert!(pairs[0].output_filename.ends_with(".aiff"));
    }

    fn spec(container: AudioContainer, rate: Option<u32>, depth: Option<u32>) -> AudioOutputSpec {
        AudioOutputSpec { container, sample_rate: rate, bit_depth: depth }
    }

    #[test]
    fn test_conversion_tags() {
        assert_eq!(conversion_tag(&spec(AudioContainer::Original, None, None)), "");
        assert_eq!(conversion_tag(&spec(AudioContainer::Wav, Some(48000), None)), "_48k");
        assert_eq!(conversion_tag(&spec(AudioContainer::Wav, Some(44100), Some(16))), "_44.1k_16bit");
        assert_eq!(conversion_tag(&spec(AudioContainer::Wav, None, Some(32))), "_32f");
        assert_eq!(conversion_tag(&spec(AudioContainer::Flac, Some(96000), Some(24))), "_96k_24bit");
    }

    #[test]
    fn test_audio_only_container_sets_extension_and_conversion_tags_name() {
        let pair = make_pair(None, "MyMix", false, -23.0, -1.0);
        let mut pairs = vec![pair];
        let sp = spec(AudioContainer::Flac, Some(44100), Some(16));
        generate_names_with_audio(&mut pairs, true, "mov", Some(&sp));
        assert_eq!(pairs[0].output_filename, "MyMix_44.1k_16bit.flac");

        // Re-applying with a different spec replaces the old tags, not stacks them.
        pairs[0].audio.filename_no_ext = "MyMix_44.1k_16bit".to_string();
        let sp2 = spec(AudioContainer::Wav, Some(48000), None);
        generate_names_with_audio(&mut pairs, true, "mov", Some(&sp2));
        assert_eq!(pairs[0].output_filename, "MyMix_48k.wav");
    }

    #[test]
    fn test_audio_only_norm_then_conversion_order() {
        let pair = make_pair(None, "MyMix", true, -23.0, -1.0);
        let mut pairs = vec![pair];
        let sp = spec(AudioContainer::Original, Some(48000), Some(24));
        generate_names_with_audio(&mut pairs, true, "mov", Some(&sp));
        assert_eq!(pairs[0].output_filename, "MyMix_-23LUFS_48k_24bit.wav");
        // Stripping takes the conversion tags off before the spec suffixes.
        assert_eq!(strip_spec_suffix("MyMix_-23LUFS_6Fr_48k_24bit"), "MyMix");
        assert_eq!(strip_spec_suffix("MyMix_32f"), "MyMix");
    }

    #[test]
    fn test_audio_only_lossy_source_reencode_becomes_wav() {
        // An mp3 that's just passed through keeps its name and extension…
        let mut pair = make_pair(None, "Podcast", false, -23.0, -1.0);
        pair.audio.extension = "mp3".to_string();
        let mut pairs = vec![pair];
        generate_names_with_audio(&mut pairs, true, "mov", Some(&spec(AudioContainer::Original, None, None)));
        assert_eq!(pairs[0].output_filename, "Podcast.mp3");
        // …but one that's normalised (re-encoded) can't go back into mp3.
        pairs[0].normalization_enabled = true;
        generate_names_with_audio(&mut pairs, true, "mov", Some(&spec(AudioContainer::Original, None, None)));
        assert_eq!(pairs[0].output_filename, "Podcast_-23LUFS.wav");
        // Explicit AAC container → m4a.
        generate_names_with_audio(&mut pairs, true, "mov", Some(&spec(AudioContainer::Aac, None, None)));
        assert_eq!(pairs[0].output_filename, "Podcast_-23LUFS.m4a");
    }
}
