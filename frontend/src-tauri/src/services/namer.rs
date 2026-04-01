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
pub fn generate_names(pairs: &mut [MatchedPair], remove_duplicates: bool, output_ext: &str) {
    // First pass: generate names
    for pair in pairs.iter_mut() {
        if let Some(ref video) = pair.video {
            pair.output_filename = generate_name(video, &pair.audio, remove_duplicates, output_ext);
        } else {
            // Audio-only: keep original extension
            pair.output_filename = format!("{}_normalized.{}", pair.audio.filename_no_ext, pair.audio.extension);
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
    let video_segments: Vec<&str> = video_name.split(|c: char| c == '_' || c == '-' || c == ' ')
        .filter(|s| !s.is_empty())
        .collect();
    let audio_segments: Vec<&str> = audio_name.split(|c: char| c == '_' || c == '-' || c == ' ')
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
}
