//! Waveform overview for the preview player: decode a file to low-rate mono
//! PCM through ffmpeg and reduce it to one peak per bucket (0…1).

use std::process::Stdio;

use crate::services::ffmpeg;

/// Sample rate we decode at for the overview. Enough for the shape of the
/// sound; keeps a feature-length file to a few megabytes.
const OVERVIEW_RATE: u32 = 4000;

/// Reduce interleaved s16le mono samples to `buckets` peak values in 0…1.
pub fn peaks_from_s16le(bytes: &[u8], buckets: usize) -> Vec<f32> {
    let n = bytes.len() / 2;
    if n == 0 || buckets == 0 {
        return Vec::new();
    }
    let mut out = vec![0f32; buckets];
    for (i, chunk) in bytes.chunks_exact(2).enumerate() {
        let v = i16::from_le_bytes([chunk[0], chunk[1]]).unsigned_abs() as f32 / 32768.0;
        let b = i * buckets / n;
        if v > out[b] {
            out[b] = v;
        }
    }
    out
}

/// Peaks for a file: `buckets` values in 0…1, left to right.
pub fn compute_peaks(path: &str, buckets: usize) -> Result<Vec<f32>, String> {
    let output = ffmpeg::silent_command(&ffmpeg::find_ffmpeg())
        .args([
            "-v", "error",
            "-i", path,
            "-vn",
            "-ac", "1",
            "-ar", &OVERVIEW_RATE.to_string(),
            "-f", "s16le",
            "-",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("Failed to run ffmpeg: {}", e))?;
    if !output.status.success() {
        return Err(format!("Could not decode audio: {}", String::from_utf8_lossy(&output.stderr).trim()));
    }
    Ok(peaks_from_s16le(&output.stdout, buckets))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s16(samples: &[i16]) -> Vec<u8> {
        samples.iter().flat_map(|s| s.to_le_bytes()).collect()
    }

    #[test]
    fn test_peaks_take_the_loudest_sample_per_bucket() {
        let bytes = s16(&[0, 100, -32768, 0, 5, -6, 7, 0]);
        let p = peaks_from_s16le(&bytes, 2);
        assert_eq!(p.len(), 2);
        assert!((p[0] - 1.0).abs() < 1e-6); // -32768 → full scale
        assert!((p[1] - 7.0 / 32768.0).abs() < 1e-6);
    }

    #[test]
    fn test_peaks_empty_and_zero_buckets() {
        assert!(peaks_from_s16le(&[], 10).is_empty());
        assert!(peaks_from_s16le(&s16(&[1, 2]), 0).is_empty());
    }
}
