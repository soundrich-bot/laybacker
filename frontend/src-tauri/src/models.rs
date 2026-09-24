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
    /// Video frame rate (fps), when known — used to size a "12-frame" fade and
    /// to talk about frames in the UI. None for audio, or if ffprobe can't tell.
    #[serde(default)]
    pub frame_rate: Option<f64>,
    /// Video frame size in pixels, when known — the slate image is rendered at
    /// exactly this size so no scaling happens on the concat.
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
    /// A slate Laybacker itself rendered onto this file, read back from the
    /// container tag it wrote (`laybacker_slate_secs`). Lets a re-dropped
    /// slated picture know where its programme audio belongs.
    #[serde(default)]
    pub slate_secs: Option<f64>,
    /// ffmpeg's channel layout name ("stereo", "5.1", "5.1(side)"…) when the
    /// probe reports one — used to give split stems real channel names.
    #[serde(default)]
    pub channel_layout: Option<String>,
    /// Bit depth of PCM / lossless audio ("16", "24", "32f" for float), when
    /// the probe reports one. None for lossy codecs — see `bit_rate`.
    #[serde(default)]
    pub bit_depth: Option<String>,
    /// Stream bit rate in bits per second, when known (what a lossy file is
    /// described by).
    #[serde(default)]
    pub bit_rate: Option<u64>,
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
    /// What to do when the audio is longer than the video. Chosen per file in a
    /// blocking prompt at export time; defaults to a plain cut (today's behaviour).
    #[serde(default)]
    pub length_fix: LengthFix,
    /// Slate: user-written text rendered as a card at the head of the video for
    /// `slate_duration_secs`, silent underneath, audio delayed to match. The
    /// frontend renders the text to a PNG (base64) at the video's exact frame
    /// size; the backend never draws text (the bundled ffmpeg has no freetype).
    #[serde(default)]
    pub slate_enabled: bool,
    #[serde(default = "default_slate_duration")]
    pub slate_duration_secs: f64,
    #[serde(default)]
    pub slate_text: String,
    /// Base64-encoded JPEG of the rendered slate, set by the frontend at export.
    #[serde(default)]
    pub slate_image: Option<String>,
    /// Black (silent) run between the slate card and the first frame of
    /// programme — e.g. a 5s preroll of 4s slate + 1s black.
    #[serde(default)]
    pub slate_black_secs: f64,
    /// OVERLAY mode: the text is laid over the first `slate_duration_secs` of
    /// the picture instead of being prepended (runtime and audio unchanged).
    #[serde(default)]
    pub slate_overlay: bool,
    /// Base64 JPEG greyscale matte for `slate_image` — overlay mode only.
    #[serde(default)]
    pub slate_matte: Option<String>,
}

fn default_slate_duration() -> f64 {
    5.0
}

/// How to reconcile an audio file that runs longer than its video.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum LengthFix {
    /// Trim the audio hard at the end of the video (ffmpeg `-shortest`).
    #[default]
    Cut,
    /// Keep the video length, but fade the audio out over the final 12 frames.
    Fade,
    /// Hold the last video frame until the video is as long as the audio, so no
    /// sound is lost. Requires re-encoding the video.
    Freeze,
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
    /// Audio-only outputs (the Audio Only page's FORMAT / SAMPLE RATE / BIT
    /// DEPTH). Original = keep the source's. Ignored on video laybacks.
    #[serde(default)]
    pub audio_container: AudioContainer,
    #[serde(default)]
    pub sample_rate: Option<u32>,
    /// 16, 24, or 32 (32 = 32-bit float — WAV/AIFF only).
    #[serde(default)]
    pub bit_depth: Option<u32>,
}

impl Default for ExportSettings {
    fn default() -> Self {
        Self {
            video_codec: VideoCodecOption::Original,
            audio_format: AudioFormatOption::Original,
            aac_bitrate: 320000,
            output_directory: None,
            use_audio_file_location: true,
            audio_container: AudioContainer::Original,
            sample_rate: None,
            bit_depth: None,
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

    /// The audio-only output spec: container, sample rate and bit depth.
    pub fn audio_output_spec(&self) -> AudioOutputSpec {
        AudioOutputSpec {
            container: self.audio_container.clone(),
            sample_rate: self.sample_rate,
            bit_depth: self.bit_depth,
        }
    }
}

/// Container for audio-only outputs. Original keeps the source's own.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum AudioContainer {
    #[default]
    Original,
    Wav,
    Aiff,
    Flac,
    Alac,
    Aac,
}

impl AudioContainer {
    /// File extension the container writes, or None for "keep the source's".
    pub fn extension(&self) -> Option<&'static str> {
        match self {
            AudioContainer::Original => None,
            AudioContainer::Wav => Some("wav"),
            AudioContainer::Aiff => Some("aif"),
            AudioContainer::Flac => Some("flac"),
            AudioContainer::Alac => Some("m4a"),
            AudioContainer::Aac => Some("m4a"),
        }
    }

    /// Lossy containers can't hold PCM, so a re-encode has to go elsewhere.
    pub fn is_lossy(&self) -> bool {
        matches!(self, AudioContainer::Aac)
    }
}

/// What an audio-only render should come out as. `None` = keep the source's.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct AudioOutputSpec {
    #[serde(default)]
    pub container: AudioContainer,
    #[serde(default)]
    pub sample_rate: Option<u32>,
    #[serde(default)]
    pub bit_depth: Option<u32>,
}

impl AudioOutputSpec {
    /// True when the spec asks for a change to sample rate or bit depth.
    pub fn converts(&self) -> bool {
        self.sample_rate.is_some() || self.bit_depth.is_some()
    }

    /// True when anything about the output differs from "as the source is".
    pub fn is_set(&self) -> bool {
        self.container != AudioContainer::Original || self.converts()
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
