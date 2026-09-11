//! Channel tools for the Audio Only page: the stereo/phase QC check, and
//! splitting a multichannel file into mono stems / joining mono stems into one.

use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::services::ffmpeg;

// ── Stereo / phase QC ───────────────────────────────────────────────────────

/// What a 2-channel file really is. `verdict`:
/// - "mono"       — a 1-channel file (nothing to check)
/// - "stereo"     — genuine stereo; `correlation` is the mono-compatibility figure
/// - "dual_mono"  — L and R identical (warn: delivered as stereo, but isn't)
/// - "one_sided"  — one channel silent or far below the other (fail)
/// - "anti_phase" — channels largely out of polarity: cancels in mono (fail)
/// - "multi"      — more than 2 channels (reported, not judged)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StereoCheck {
    pub channels: u32,
    pub verdict: String,
    pub correlation: Option<f64>,
    pub left_db: Option<f64>,
    pub right_db: Option<f64>,
}

/// Per-channel RMS levels (dB) from astats output, in channel order.
pub fn parse_astats_rms(stderr: &str) -> Vec<f64> {
    let mut out = Vec::new();
    let mut in_channel = false;
    for line in stderr.lines() {
        if line.contains("Channel:") {
            in_channel = true;
        } else if in_channel && line.contains("RMS level dB:") {
            // "-inf" parses as a float in Rust — clamp silence to a finite floor.
            if let Some(v) = line.rsplit(':').next().and_then(|s| s.trim().parse::<f64>().ok()) {
                out.push(if v.is_finite() { v } else { -120.0 });
            }
        } else if line.contains("Overall") {
            in_channel = false;
        }
    }
    out
}

/// Mean phase correlation (-1…1) from aphasemeter+ametadata output lines.
pub fn parse_phase_mean(stdout: &str) -> Option<f64> {
    let mut sum = 0.0;
    let mut n = 0usize;
    for line in stdout.lines() {
        if let Some(v) = line.strip_prefix("lavfi.aphasemeter.phase=") {
            if let Ok(x) = v.trim().parse::<f64>() {
                sum += x;
                n += 1;
            }
        }
    }
    if n == 0 { None } else { Some(sum / n as f64) }
}

/// Judge a 2-channel file from its channel levels and phase correlation.
pub fn judge_stereo(left_db: f64, right_db: f64, correlation: f64) -> &'static str {
    const SILENT_DB: f64 = -70.0;
    const IMBALANCE_DB: f64 = 40.0;
    let l_silent = left_db <= SILENT_DB;
    let r_silent = right_db <= SILENT_DB;
    if (l_silent && !r_silent) || (r_silent && !l_silent) || (left_db - right_db).abs() >= IMBALANCE_DB {
        return "one_sided";
    }
    if correlation <= -0.5 {
        return "anti_phase";
    }
    if correlation >= 0.98 && (left_db - right_db).abs() < 0.5 {
        return "dual_mono";
    }
    "stereo"
}

/// Run the stereo check on a file with `channels` channels.
pub fn check_stereo(path: &str, channels: u32) -> Result<StereoCheck, String> {
    if channels <= 1 {
        return Ok(StereoCheck { channels, verdict: "mono".into(), correlation: None, left_db: None, right_db: None });
    }
    if channels > 2 {
        return Ok(StereoCheck { channels, verdict: "multi".into(), correlation: None, left_db: None, right_db: None });
    }

    // 1. Per-channel RMS level.
    let stats = ffmpeg::silent_command(&ffmpeg::find_ffmpeg())
        .args(["-hide_banner", "-nostats", "-i", path,
               "-af", "astats=measure_perchannel=RMS_level:measure_overall=none",
               "-f", "null", "-"])
        .output()
        .map_err(|e| format!("Stereo check failed to run ffmpeg: {}", e))?;
    let rms = parse_astats_rms(&String::from_utf8_lossy(&stats.stderr));
    let (left_db, right_db) = match rms.as_slice() {
        [l, r, ..] => (*l, *r),
        _ => return Err("Stereo check: could not read channel levels".to_string()),
    };

    // 2. Phase correlation, averaged over the file.
    let phase = ffmpeg::silent_command(&ffmpeg::find_ffmpeg())
        .args(["-hide_banner", "-nostats", "-i", path,
               "-af", "aphasemeter=video=0,ametadata=mode=print:key=lavfi.aphasemeter.phase:file=-",
               "-f", "null", "-"])
        .output()
        .map_err(|e| format!("Stereo check failed to run ffmpeg: {}", e))?;
    let correlation = parse_phase_mean(&String::from_utf8_lossy(&phase.stdout))
        .ok_or_else(|| "Stereo check: could not read phase".to_string())?;

    Ok(StereoCheck {
        channels,
        verdict: judge_stereo(left_db, right_db, correlation).into(),
        correlation: Some(correlation),
        left_db: Some(left_db),
        right_db: Some(right_db),
    })
}

// ── Split / join ────────────────────────────────────────────────────────────

/// Human channel names for a layout, in ffmpeg's channel order. Unknown
/// layouts fall back to ch1…chN so nothing is ever mislabelled.
pub fn channel_names(layout: &str, count: u32) -> Vec<String> {
    let known: Option<&[&str]> = match layout {
        "mono" => Some(&["M"]),
        "stereo" => Some(&["L", "R"]),
        "2.1" => Some(&["L", "R", "LFE"]),
        "3.0" => Some(&["L", "R", "C"]),
        "quad" => Some(&["L", "R", "Ls", "Rs"]),
        "5.0" | "5.0(side)" => Some(&["L", "R", "C", "Ls", "Rs"]),
        "5.1" | "5.1(side)" => Some(&["L", "R", "C", "LFE", "Ls", "Rs"]),
        "6.1" => Some(&["L", "R", "C", "LFE", "Cs", "Ls", "Rs"]),
        "7.1" => Some(&["L", "R", "C", "LFE", "Lsr", "Rsr", "Lss", "Rss"]),
        _ => None,
    };
    match known {
        Some(names) if names.len() == count as usize => names.iter().map(|s| s.to_string()).collect(),
        _ => (1..=count).map(|i| format!("ch{}", i)).collect(),
    }
}

/// ffmpeg's own layout name for a channel count when the probe gave none.
pub fn layout_for_count(count: u32) -> &'static str {
    match count {
        1 => "mono",
        2 => "stereo",
        3 => "2.1",
        4 => "quad",
        5 => "5.0",
        6 => "5.1",
        7 => "6.1",
        8 => "7.1",
        _ => "stereo",
    }
}

/// One ffmpeg run: channelsplit → one mono output per channel.
pub fn build_split_command(path: &str, layout: &str, outputs: &[String]) -> Vec<String> {
    let n = outputs.len();
    let labels: Vec<String> = (0..n).map(|i| format!("[c{}]", i)).collect();
    let mut args = vec![
        "-y".to_string(),
        "-i".to_string(), path.to_string(),
        "-filter_complex".to_string(),
        format!("[0:a]channelsplit=channel_layout={}{}", layout, labels.join("")),
    ];
    for (label, out) in labels.iter().zip(outputs) {
        args.extend([
            "-map".to_string(), label.clone(),
            "-c:a".to_string(), "pcm_s24le".to_string(),
            out.clone(),
        ]);
    }
    args
}

/// Split a multichannel file into mono files beside it: "<stem>_L.wav" …
/// Returns the output paths in channel order.
pub fn split_channels(path: &str, layout: Option<&str>, channels: u32) -> Result<Vec<String>, String> {
    if channels < 2 {
        return Err("That file is already mono".to_string());
    }
    let layout = layout.filter(|l| !l.is_empty()).unwrap_or_else(|| layout_for_count(channels)).to_string();
    let names = channel_names(&layout, channels);
    let p = Path::new(path);
    let dir = p.parent().map(|d| d.to_string_lossy().to_string()).unwrap_or_else(|| ".".to_string());
    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("audio");
    let outputs: Vec<String> = names.iter().map(|n| format!("{}/{}_{}.wav", dir, stem, n)).collect();
    let args = build_split_command(path, &layout, &outputs);
    ffmpeg::run_ffmpeg(&args)?;
    Ok(outputs)
}

/// One ffmpeg run: N mono inputs → one file in `layout`, in input order.
pub fn build_join_command(inputs: &[String], layout: &str, output: &str) -> Vec<String> {
    let mut args = vec!["-y".to_string()];
    for i in inputs {
        args.extend(["-i".to_string(), i.clone()]);
    }
    // No explicit map: `join` fills the layout's channels from the inputs in
    // order, which is the order our split wrote them in. (A map would need
    // channel NAMES — FL, FR… — not indexes.)
    args.extend([
        "-filter_complex".to_string(),
        format!("join=inputs={}:channel_layout={}[out]", inputs.len(), layout),
        "-map".to_string(), "[out]".to_string(),
        "-c:a".to_string(), "pcm_s24le".to_string(),
        output.to_string(),
    ]);
    args
}

/// Join mono files (already in channel order) into one multichannel file.
pub fn join_channels(inputs: &[String], layout: &str, output: &str) -> Result<(), String> {
    if inputs.len() < 2 {
        return Err("Need at least two mono files to join".to_string());
    }
    ffmpeg::run_ffmpeg(&build_join_command(inputs, layout, output))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_astats_rms_per_channel() {
        let s = "[x] Channel: 1\n[x] RMS level dB: -17.08\n[x] Channel: 2\n[x] RMS level dB: -inf\n[x] Overall\n[x] RMS level dB: -3.0\n";
        assert_eq!(parse_astats_rms(s), vec![-17.08, -120.0]);
    }

    #[test]
    fn test_parse_phase_mean() {
        let s = "frame:0 pts:0\nlavfi.aphasemeter.phase=1.0\nframe:1\nlavfi.aphasemeter.phase=0.5\n";
        assert_eq!(parse_phase_mean(s), Some(0.75));
        assert_eq!(parse_phase_mean("nothing"), None);
    }

    #[test]
    fn test_judge_stereo_verdicts() {
        assert_eq!(judge_stereo(-17.0, -17.0, 1.0), "dual_mono");
        assert_eq!(judge_stereo(-17.0, -18.0, 0.6), "stereo");
        assert_eq!(judge_stereo(-17.0, -120.0, 0.0), "one_sided");
        assert_eq!(judge_stereo(-17.0, -17.0, -0.9), "anti_phase");
        // A loud/quiet pair that isn't silent is still stereo, not one-sided.
        assert_eq!(judge_stereo(-10.0, -30.0, 0.3), "stereo");
    }

    #[test]
    fn test_channel_names_known_and_fallback() {
        assert_eq!(channel_names("stereo", 2), vec!["L", "R"]);
        assert_eq!(channel_names("5.1", 6), vec!["L", "R", "C", "LFE", "Ls", "Rs"]);
        assert_eq!(channel_names("5.1(side)", 6), vec!["L", "R", "C", "LFE", "Ls", "Rs"]);
        // Layout/count mismatch or unknown layout → numbered, never mislabelled.
        assert_eq!(channel_names("stereo", 3), vec!["ch1", "ch2", "ch3"]);
        assert_eq!(channel_names("hexagonal", 6).len(), 6);
    }

    #[test]
    fn test_split_and_join_commands() {
        let outs = vec!["/a_L.wav".to_string(), "/a_R.wav".to_string()];
        let s = build_split_command("/a.wav", "stereo", &outs);
        let fc = s.iter().position(|a| a == "-filter_complex").unwrap();
        assert_eq!(s[fc + 1], "[0:a]channelsplit=channel_layout=stereo[c0][c1]");
        assert!(s.windows(2).any(|w| w[0] == "-map" && w[1] == "[c1]"));
        assert_eq!(s.last().unwrap(), "/a_R.wav");

        let j = build_join_command(&outs, "stereo", "/a_stereo.wav");
        let fc = j.iter().position(|a| a == "-filter_complex").unwrap();
        assert_eq!(j[fc + 1], "join=inputs=2:channel_layout=stereo[out]");
        assert_eq!(j.last().unwrap(), "/a_stereo.wav");
    }
}
