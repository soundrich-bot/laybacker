use crate::models::NormalizationSettings;
use crate::services::ffmpeg;

/// Result of loudness measurement
#[derive(Debug, Clone)]
pub struct LoudnessMeasurement {
    pub integrated_lufs: f64,
    pub true_peak_dbtp: f64,
}

/// Measure loudness of an audio file using ffmpeg's loudnorm filter
/// This is more reliable than a custom implementation and leverages
/// FFmpeg's ITU-R BS.1770-4 compliant measurement
pub fn measure(audio_path: &str) -> Result<LoudnessMeasurement, String> {
    let (lufs, true_peak) = ffmpeg::measure_loudness_ffmpeg(audio_path)?;

    Ok(LoudnessMeasurement {
        integrated_lufs: lufs,
        true_peak_dbtp: true_peak,
    })
}

/// Calculate the gain in dB needed to reach the target loudness,
/// while respecting the true peak limit.
///
/// Two modes:
/// - **Broadcast/Streaming** (target_lufs < 0): normalize to LUFS target, capped by true peak limit
/// - **Full Scale Review** (target_lufs >= 0): maximize loudness up to true peak limit only
pub fn calculate_gain(
    measurement: &LoudnessMeasurement,
    settings: &NormalizationSettings,
) -> f64 {
    if measurement.integrated_lufs.is_infinite() || measurement.integrated_lufs.is_nan() {
        return 0.0;
    }

    // Maximum gain before true peak limit is exceeded.
    // Compensate for ebur128 reading true peak ~0.1dB lower than industry meters
    // (YouLean, Nugen) by adding 0.1dB to the measured peak.
    let max_gain = settings.true_peak_limit - (measurement.true_peak_dbtp + 0.1);

    if settings.target_lufs >= 0.0 {
        // Full Scale Review mode: push as loud as possible up to true peak limit
        let gain = max_gain;
        return (gain * 100.0).round() / 100.0;
    }

    // Broadcast/Streaming mode: target a specific LUFS level
    let desired_gain = settings.target_lufs - measurement.integrated_lufs;

    // Use the smaller of the two (don't exceed true peak)
    let gain = desired_gain.min(max_gain);

    // Round to 2 decimal places
    (gain * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gain_calculation_normal() {
        let measurement = LoudnessMeasurement {
            integrated_lufs: -20.0,
            true_peak_dbtp: -3.0,
        };
        let settings = NormalizationSettings {
            target_lufs: -23.0,
            true_peak_limit: -1.0,
        };

        let gain = calculate_gain(&measurement, &settings);
        assert_eq!(gain, -3.0); // Need to reduce by 3 dB
    }

    #[test]
    fn test_gain_calculation_true_peak_limited() {
        let measurement = LoudnessMeasurement {
            integrated_lufs: -30.0,
            true_peak_dbtp: -2.0,
        };
        let settings = NormalizationSettings {
            target_lufs: -23.0,
            true_peak_limit: -1.0,
        };

        let gain = calculate_gain(&measurement, &settings);
        // Want +7 dB but true peak allows: -1.0 - (-2.0 + 0.1) = 0.9 dB
        assert_eq!(gain, 0.9);
    }

    #[test]
    fn test_gain_calculation_full_scale_mode() {
        let measurement = LoudnessMeasurement {
            integrated_lufs: -14.5,
            true_peak_dbtp: -1.96,
        };
        let settings = NormalizationSettings {
            target_lufs: 0.0,       // Full scale mode
            true_peak_limit: -1.0,
        };

        let gain = calculate_gain(&measurement, &settings);
        // max_gain = -1.0 - (-1.96 + 0.1) = -1.0 + 1.86 = 0.86
        assert_eq!(gain, 0.86);
    }

    #[test]
    fn test_gain_calculation_full_scale_quiet_source() {
        let measurement = LoudnessMeasurement {
            integrated_lufs: -30.0,
            true_peak_dbtp: -12.0,
        };
        let settings = NormalizationSettings {
            target_lufs: 0.0,       // Full scale mode
            true_peak_limit: -1.0,
        };

        let gain = calculate_gain(&measurement, &settings);
        // max_gain = -1.0 - (-12.0 + 0.1) = -1.0 + 11.9 = 10.9
        assert_eq!(gain, 10.9);
    }

    #[test]
    fn test_gain_calculation_infinite_lufs() {
        let measurement = LoudnessMeasurement {
            integrated_lufs: f64::NEG_INFINITY,
            true_peak_dbtp: -100.0,
        };
        let settings = NormalizationSettings::default();

        let gain = calculate_gain(&measurement, &settings);
        assert_eq!(gain, 0.0); // Don't adjust silence
    }

    #[test]
    fn test_gain_calculation_nan_lufs() {
        let measurement = LoudnessMeasurement {
            integrated_lufs: f64::NAN,
            true_peak_dbtp: -5.0,
        };
        let settings = NormalizationSettings { target_lufs: -23.0, true_peak_limit: -1.0 };
        assert_eq!(calculate_gain(&measurement, &settings), 0.0);
    }

    #[test]
    fn test_gain_includes_true_peak_calibration() {
        // ebur128 reads true peak ~0.1dB lower than industry meters.
        // With TP at -1.0 dBTP (measured) and limit at -1.0 dBTP,
        // naive calc would say 0 gain, but with +0.1 calibration it should be -0.1.
        let measurement = LoudnessMeasurement {
            integrated_lufs: -23.0,
            true_peak_dbtp: -1.0,
        };
        let settings = NormalizationSettings { target_lufs: -23.0, true_peak_limit: -1.0 };
        let gain = calculate_gain(&measurement, &settings);
        // max_gain = -1.0 - (-1.0 + 0.1) = -0.1
        // desired_gain = -23.0 - (-23.0) = 0.0
        // gain = min(0.0, -0.1) = -0.1
        assert_eq!(gain, -0.1);
    }

    #[test]
    fn test_full_scale_with_various_peaks() {
        // File with -6dBTP peak, limit at -1dBTP
        let measurement = LoudnessMeasurement {
            integrated_lufs: -20.0,
            true_peak_dbtp: -6.0,
        };
        let settings = NormalizationSettings { target_lufs: 0.0, true_peak_limit: -1.0 };
        let gain = calculate_gain(&measurement, &settings);
        // max_gain = -1.0 - (-6.0 + 0.1) = -1.0 + 5.9 = 4.9
        assert_eq!(gain, 4.9);
    }

    #[test]
    fn test_lufs_mode_no_headroom() {
        // File already louder than target, and peak is near limit
        let measurement = LoudnessMeasurement {
            integrated_lufs: -20.0,
            true_peak_dbtp: -0.5,
        };
        let settings = NormalizationSettings { target_lufs: -23.0, true_peak_limit: -1.0 };
        let gain = calculate_gain(&measurement, &settings);
        // desired = -23 - (-20) = -3.0 (reduce)
        // max = -1.0 - (-0.5 + 0.1) = -1.0 + 0.4 = -0.6
        // gain = min(-3.0, -0.6) = -3.0 (desired is smaller, i.e. more reduction)
        assert_eq!(gain, -3.0);
    }
}
