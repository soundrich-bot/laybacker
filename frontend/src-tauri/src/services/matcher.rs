use crate::models::*;
use strsim::normalized_levenshtein;
use uuid::Uuid;

const DURATION_TOLERANCE: f64 = 2.0; // seconds — audio may be slightly longer/shorter

/// Match audio files to video files by duration, then by name similarity
pub fn match_files(files: &[MediaFile]) -> Vec<MatchedPair> {
    let videos: Vec<&MediaFile> = files.iter().filter(|f| f.media_type == MediaType::Video).collect();
    let audios: Vec<&MediaFile> = files.iter().filter(|f| f.media_type == MediaType::Audio).collect();

    if videos.is_empty() || audios.is_empty() {
        return Vec::new();
    }

    // One audio laid back onto every video (1 audio -> N videos): the user dropped
    // a single mix and several pictures, so pair that audio with each video.
    if audios.len() == 1 {
        let audio = audios[0];
        return videos
            .iter()
            .map(|video| {
                let duration_delta = (video.duration_secs - audio.duration_secs).abs();
                let confidence = if duration_delta <= DURATION_TOLERANCE {
                    1.0
                } else {
                    (1.0 - (duration_delta / 10.0)).max(0.0)
                };
                MatchedPair {
                    id: Uuid::new_v4().to_string(),
                    video: Some((*video).clone()),
                    audio: audio.clone(),
                    output_filename: String::new(), // Will be set by namer
                    normalization_enabled: false,
                    normalization_settings: NormalizationSettings::default(),
                    timecode_offset_secs: 0.0,
                    match_confidence: confidence,
                    silence_compliance: false,
                    silence_ms: 240.0,
                    fade_ms: 5.0,
                }
            })
            .collect();
    }

    // Build candidate pairs with scores
    let mut candidates: Vec<(usize, usize, f64, f64, f64)> = Vec::new(); // (video_idx, audio_idx, duration_delta, name_sim, combined)

    for (vi, video) in videos.iter().enumerate() {
        for (ai, audio) in audios.iter().enumerate() {
            let duration_delta = (video.duration_secs - audio.duration_secs).abs();
            let duration_matches = duration_delta <= DURATION_TOLERANCE;

            let name_similarity = normalized_levenshtein(
                &video.filename_no_ext.to_lowercase(),
                &audio.filename_no_ext.to_lowercase(),
            );

            // Score: duration match weighted heavily, name similarity secondary
            let duration_score = if duration_matches {
                1.0
            } else {
                (1.0 - (duration_delta / 10.0)).max(0.0)
            };
            let combined = (duration_score * 0.7) + (name_similarity * 0.3);

            candidates.push((vi, ai, duration_delta, name_similarity, combined));
        }
    }

    // Sort by combined score descending
    candidates.sort_by(|a, b| b.4.partial_cmp(&a.4).unwrap_or(std::cmp::Ordering::Equal));

    // Greedy assignment: allow video reuse (1 video -> N audio), but each audio only once
    let mut assigned_audios: std::collections::HashSet<usize> = std::collections::HashSet::new();
    let mut pairs = Vec::new();

    for (vi, ai, _duration_delta, _name_sim, combined) in &candidates {
        if assigned_audios.contains(ai) {
            continue;
        }

        // Always pair — users drop files they intend to combine
        // Ranking by score ensures best matches come first

        assigned_audios.insert(*ai);

        let video = videos[*vi].clone();
        let audio = audios[*ai].clone();

        pairs.push(MatchedPair {
            id: Uuid::new_v4().to_string(),
            video: Some(video),
            audio,
            output_filename: String::new(), // Will be set by namer
            normalization_enabled: false,
            normalization_settings: NormalizationSettings::default(),
            timecode_offset_secs: 0.0,
            match_confidence: *combined,
            silence_compliance: false,
            silence_ms: 240.0,
            fade_ms: 5.0,
        });
    }

    pairs
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_video(name: &str, duration: f64) -> MediaFile {
        MediaFile {
            id: MediaFile::new_id(),
            path: format!("/test/{}.mov", name),
            filename: format!("{}.mov", name),
            filename_no_ext: name.to_string(),
            extension: "mov".to_string(),
            media_type: MediaType::Video,
            duration_secs: duration,
            codec_info: None,
            sample_rate: None,
            channel_count: None,
            thumbnail_data: None,
        }
    }

    fn make_audio(name: &str, duration: f64) -> MediaFile {
        MediaFile {
            id: MediaFile::new_id(),
            path: format!("/test/{}.wav", name),
            filename: format!("{}.wav", name),
            filename_no_ext: name.to_string(),
            extension: "wav".to_string(),
            media_type: MediaType::Audio,
            duration_secs: duration,
            codec_info: None,
            sample_rate: None,
            channel_count: None,
            thumbnail_data: None,
        }
    }

    #[test]
    fn test_match_by_duration() {
        let files = vec![
            make_video("commercial_30s", 30.0),
            make_video("commercial_15s", 15.0),
            make_audio("audio_mix_a", 30.0),
            make_audio("audio_mix_b", 15.0),
        ];

        let pairs = match_files(&files);
        assert_eq!(pairs.len(), 2);
    }

    #[test]
    fn test_one_audio_multiple_videos() {
        // A single mix dropped with several pictures should pair onto every video.
        let files = vec![
            make_audio("final_mix", 30.0),
            make_video("clip_a", 30.0),
            make_video("clip_b", 15.0),
            make_video("clip_c", 30.0),
        ];

        let pairs = match_files(&files);
        assert_eq!(pairs.len(), 3, "one audio should pair with all three videos");
        // Every pair uses the same audio.
        assert!(pairs.iter().all(|p| p.audio.filename_no_ext == "final_mix"));
        // All three videos are represented, each once.
        let mut vids: Vec<_> = pairs
            .iter()
            .map(|p| p.video.as_ref().unwrap().filename_no_ext.clone())
            .collect();
        vids.sort();
        assert_eq!(vids, vec!["clip_a", "clip_b", "clip_c"]);
    }

    #[test]
    fn test_one_video_multiple_audio() {
        let files = vec![
            make_video("commercial_30s", 30.0),
            make_audio("mix_coming_soon", 30.0),
            make_audio("mix_available_now", 30.0),
        ];

        let pairs = match_files(&files);
        assert_eq!(pairs.len(), 2);
        // Both pairs should reference the same video
        assert_eq!(pairs[0].video.as_ref().unwrap().filename_no_ext, pairs[1].video.as_ref().unwrap().filename_no_ext);
    }
}
