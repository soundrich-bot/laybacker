use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum MediaType {
    Video,
    Audio,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaFile {
    pub id: String,
    pub path: String,
    pub filename: String,
    pub filename_no_ext: String,
    pub extension: String,
    pub media_type: MediaType,
    pub duration_secs: f64,
    pub codec_info: Option<String>,
    pub sample_rate: Option<f64>,
    pub channel_count: Option<u32>,
    pub thumbnail_data: Option<String>,
}

impl MediaFile {
    pub fn new_id() -> String {
        Uuid::new_v4().to_string()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchedPair {
    pub id: String,
    pub video: Option<MediaFile>,
    pub audio: MediaFile,
    pub output_filename: String,
    pub normalization_enabled: bool,
    pub normalization_settings: NormalizationSettings,
    pub timecode_offset_secs: f64,
    pub match_confidence: f64,
    pub silence_compliance: bool,
    pub silence_ms: f64,
    pub fade_ms: f64,
    /// "Clock" delivery format (audio-only): prepend 10s / append 5s of silence
    /// on export. Gated in the UI behind a levels + head/tail-silence check.
    #[serde(default)]
    pub clock_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NormalizationSettings {
    pub target_lufs: f64,
    pub true_peak_limit: f64,
}

impl Default for NormalizationSettings {
    fn default() -> Self {
        Self {
            target_lufs: 0.0,
            true_peak_limit: -1.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportSettings {
    pub video_codec: VideoCodecOption,
    pub audio_format: AudioFormatOption,
    pub aac_bitrate: u32,
    pub output_directory: Option<String>,
    pub use_audio_file_location: bool,
}

impl Default for ExportSettings {
    fn default() -> Self {
        Self {
            video_codec: VideoCodecOption::Original,
            audio_format: AudioFormatOption::Original,
            aac_bitrate: 320000,
            output_directory: None,
            use_audio_file_location: true,
        }
    }
}

impl ExportSettings {
    pub fn output_extension(&self) -> &str {
        match self.audio_format {
            AudioFormatOption::Original => "mov",
            AudioFormatOption::Aac => "mp4",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum VideoCodecOption {
    Original,
    H264,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum AudioFormatOption {
    Original,
    Aac,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessingProgress {
    pub pair_id: String,
    pub state: String,
    pub progress: f64,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessingResult {
    pub pair_id: String,
    pub success: bool,
    pub output_path: Option<String>,
    pub error: Option<String>,
    pub measured_lufs: Option<f64>,
    pub measured_true_peak: Option<f64>,
}

/// Supported file extensions
pub const VIDEO_EXTENSIONS: &[&str] = &["mp4", "mov", "m4v", "mxf"];
pub const AUDIO_EXTENSIONS: &[&str] = &["wav", "aif", "aiff", "bwf", "m4a", "aac", "mp3", "flac"];

pub fn is_video_extension(ext: &str) -> bool {
    VIDEO_EXTENSIONS.contains(&ext.to_lowercase().as_str())
}

pub fn is_audio_extension(ext: &str) -> bool {
    AUDIO_EXTENSIONS.contains(&ext.to_lowercase().as_str())
}

pub fn is_supported_extension(ext: &str) -> bool {
    is_video_extension(ext) || is_audio_extension(ext)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extension_helpers() {
        assert!(is_video_extension("mp4"));
        assert!(is_video_extension("MOV")); // case-insensitive
        assert!(!is_video_extension("wav"));
        assert!(is_audio_extension("wav"));
        assert!(is_audio_extension("AIFF"));
        assert!(!is_audio_extension("mp4"));
        assert!(is_supported_extension("mov"));
        assert!(is_supported_extension("flac"));
        assert!(!is_supported_extension("txt"));
    }

    #[test]
    fn test_output_extension() {
        let mut s = ExportSettings::default();
        assert_eq!(s.output_extension(), "mov"); // Original
        s.audio_format = AudioFormatOption::Aac;
        assert_eq!(s.output_extension(), "mp4");
    }

    #[test]
    fn test_defaults_and_new_id() {
        let n = NormalizationSettings::default();
        assert_eq!(n.target_lufs, 0.0);
        assert_eq!(n.true_peak_limit, -1.0);

        let e = ExportSettings::default();
        assert!(e.use_audio_file_location);
        assert_eq!(e.aac_bitrate, 320000);
        assert_eq!(e.video_codec, VideoCodecOption::Original);

        let a = MediaFile::new_id();
        let b = MediaFile::new_id();
        assert_ne!(a, b, "ids should be unique");
        assert_eq!(a.len(), 36, "uuid v4 string is 36 chars");
    }
}
