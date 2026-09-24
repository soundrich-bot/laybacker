//! File Processing for the Audio Only page: fold-downs, fades and silence
//! trims. Each writes a new 24-bit WAV beside the source (originals are never
//! touched) and returns its path.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::services::channels::{layout_for_count, split_channels_to};
use crate::services::{ffmpeg, inspector};

/// ffmpeg's channel names for a layout, in its channel order. Unknown layouts
/// fall back to the standard layout for the channel count.
pub fn layout_channels(layout: &str, count: u32) -> Vec<&'static str> {
    let known: Option<&[&str]> = match layout {
        "mono" => Some(&["FC"]),
        "stereo" => Some(&["FL", "FR"]),
        "2.1" => Some(&["FL", "FR", "LFE"]),
        "3.0" => Some(&["FL", "FR", "FC"]),
        "3.0(back)" => Some(&["FL", "FR", "BC"]),
        "3.1" => Some(&["FL", "FR", "FC", "LFE"]),
        "4.0" => Some(&["FL", "FR", "FC", "BC"]),
        "quad" => Some(&["FL", "FR", "BL", "BR"]),
        "quad(side)" => Some(&["FL", "FR", "SL", "SR"]),
        "4.1" => Some(&["FL", "FR", "FC", "LFE", "BC"]),
        "5.0" => Some(&["FL", "FR", "FC", "BL", "BR"]),
        "5.0(side)" => Some(&["FL", "FR", "FC", "SL", "SR"]),
        "5.1" => Some(&["FL", "FR", "FC", "LFE", "BL", "BR"]),
        "5.1(side)" => Some(&["FL", "FR", "FC", "LFE", "SL", "SR"]),
        "6.0" => Some(&["FL", "FR", "FC", "BC", "SL", "SR"]),
        "6.0(front)" => Some(&["FL", "FR", "FLC", "FRC", "SL", "SR"]),
        "hexagonal" => Some(&["FL", "FR", "FC", "BL", "BR", "BC"]),
        "6.1" => Some(&["FL", "FR", "FC", "LFE", "BC", "SL", "SR"]),
        "6.1(back)" => Some(&["FL", "FR", "FC", "LFE", "BL", "BR", "BC"]),
        "6.1(front)" => Some(&["FL", "FR", "LFE", "FLC", "FRC", "SL", "SR"]),
        "7.0" => Some(&["FL", "FR", "FC", "BL", "BR", "SL", "SR"]),
        "7.0(front)" => Some(&["FL", "FR", "FC", "FLC", "FRC", "SL", "SR"]),
        "7.1" => Some(&["FL", "FR", "FC", "LFE", "BL", "BR", "SL", "SR"]),
        "7.1(wide)" => Some(&["FL", "FR", "FC", "LFE", "BL", "BR", "FLC", "FRC"]),
        "7.1(wide-side)" => Some(&["FL", "FR", "FC", "LFE", "FLC", "FRC", "SL", "SR"]),
        "octagonal" => Some(&["FL", "FR", "FC", "BL", "BR", "BC", "SL", "SR"]),
        _ => None,
    };
    match known {
        Some(ch) if ch.len() == count as usize => ch.to_vec(),
        _ => {
            let fallback = layout_for_count(count);
            if fallback == layout {
                Vec::new()
            } else {
                layout_channels(fallback, count)
            }
        }
    }
}

/// Contribution of one source channel to a Lo/Ro stereo fold-down:
/// (left gain, right gain). ITU-style: centre and surrounds at −3 dB, LFE dropped.
fn stereo_weights(ch: &str) -> (f64, f64) {
    match ch {
        "FL" => (1.0, 0.0),
        "FR" => (0.0, 1.0),
        "FC" => (0.707, 0.707),
        "BC" => (0.5, 0.5),
        "BL" | "SL" | "FLC" => (0.707, 0.0),
        "BR" | "SR" | "FRC" => (0.0, 0.707),
        _ => (0.0, 0.0), // LFE and anything exotic
    }
}

fn pan_terms(terms: &[(f64, &str)]) -> String {
    terms
        .iter()
        .map(|(g, ch)| if (*g - 1.0).abs() < 1e-9 { ch.to_string() } else { format!("{}*{}", g, ch) })
        .collect::<Vec<_>>()
        .join("+")
}

/// `pan` expression folding a multichannel layout to Lo/Ro stereo. Uses the
/// real ITU coefficients (no renormalising, so levels are honest) and is
/// followed by a limiter in the chain so a hot centre can't clip the sum.
pub fn fold_stereo_expr(layout: &str, count: u32) -> Result<String, String> {
    let chans = layout_channels(layout, count);
    if chans.len() < 3 {
        return Err(format!("{} is already stereo or mono", layout));
    }
    let mut left = Vec::new();
    let mut right = Vec::new();
    for ch in chans {
        let (l, r) = stereo_weights(ch);
        if l > 0.0 { left.push((l, ch)); }
        if r > 0.0 { right.push((r, ch)); }
    }
    if left.is_empty() || right.is_empty() {
        return Err(format!("Don't know how to fold a {} file to stereo", layout));
    }
    Ok(format!("pan=stereo|FL={}|FR={}", pan_terms(&left), pan_terms(&right)))
}

/// `pan` expression folding any layout to mono. Gains are renormalised (the
/// `<` form) so the sum can't clip: a dual-mono stereo file comes out at the
/// same level it went in at.
pub fn fold_mono_expr(layout: &str, count: u32) -> Result<String, String> {
    let chans = layout_channels(layout, count);
    if chans.len() < 2 {
        return Err(format!("{} is already mono", layout));
    }
    let mut terms = Vec::new();
    for ch in chans {
        let (l, r) = stereo_weights(ch);
        let g = l.max(r);
        if g > 0.0 { terms.push((g, ch)); }
    }
    Ok(format!("pan=mono|FC<{}", pan_terms(&terms)))
}

/// Fade in and out over `secs` at each end of a file `duration` long.
pub fn fade_expr(duration: f64, secs: f64) -> String {
    let d = secs.max(0.001).min(duration / 2.0);
    let start = (duration - d).max(0.0);
    format!("afade=t=in:st=0:d={},afade=t=out:st={}:d={}", d, start, d)
}

/// Trim silence below `threshold_db` from the head and tail, keeping 10 ms of
/// it either side. The tail is done by reversing the file (`areverse`) so a
/// pause mid-programme can never be mistaken for the end.
pub fn trim_expr(threshold_db: f64) -> String {
    let sr = format!("silenceremove=start_periods=1:start_threshold={}dB:start_silence=0.01", threshold_db);
    format!("{},areverse,{},areverse", sr, sr)
}

/// One ffmpeg run: source → `-af` chain → 24-bit WAV.
pub fn build_process_command(path: &str, output: &str, filter: &str) -> Vec<String> {
    vec![
        "-y".to_string(),
        "-i".to_string(), path.to_string(),
        "-vn".to_string(),
        "-af".to_string(), filter.to_string(),
        "-c:a".to_string(), "pcm_s24le".to_string(),
        output.to_string(),
    ]
}

/// Output path beside the source: "<stem>_<suffix>.wav".
pub fn output_beside(path: &str, suffix: &str) -> String {
    let p = Path::new(path);
    let dir = p.parent().map(|d| d.to_string_lossy().to_string()).unwrap_or_else(|| ".".to_string());
    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("audio");
    format!("{}/{}_{}.wav", dir, stem, suffix)
}

/// Run one processing op on a file. `op` is one of "fold_stereo",
/// "fold_mono", "fade" (param = seconds per end) or "trim" (param =
/// threshold dB, default −60). Returns the new file's path.
pub fn process_audio(
    op: &str,
    path: &str,
    layout: Option<&str>,
    channels: u32,
    duration_secs: f64,
    param: Option<f64>,
) -> Result<String, String> {
    process_audio_to(op, path, layout, channels, duration_secs, param, None)
}

/// As `process_audio`, but with an explicit output path when given (the
/// chain writes its intermediates into a work directory, keeping the stem).
#[allow(clippy::too_many_arguments)]
pub fn process_audio_to(
    op: &str,
    path: &str,
    layout: Option<&str>,
    channels: u32,
    duration_secs: f64,
    param: Option<f64>,
    output: Option<&str>,
) -> Result<String, String> {
    let layout = layout
        .filter(|l| !l.is_empty())
        .unwrap_or_else(|| layout_for_count(channels))
        .to_string();
    let (filter, suffix) = match op {
        "fold_stereo" => (
            format!("{},alimiter=limit=0.98:level=false", fold_stereo_expr(&layout, channels)?),
            "stereo",
        ),
        "fold_mono" => (fold_mono_expr(&layout, channels)?, "mono"),
        "fade" => (fade_expr(duration_secs, param.unwrap_or(0.02)), "faded"),
        "trim" => (trim_expr(param.unwrap_or(-60.0)), "trimmed"),
        other => return Err(format!("Unknown processing op: {}", other)),
    };
    let output = output.map(|o| o.to_string()).unwrap_or_else(|| output_beside(path, suffix));
    ffmpeg::run_ffmpeg(&build_process_command(path, &output, &filter))?;
    Ok(output)
}

// ── Multifunction Chain: shaping stage ──────────────────────────────────────

/// One shaping step of the chain. `kind` is a `process_audio` op
/// ("fold_stereo", "fold_mono", "trim", "fade") or "split".
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ShapeOp {
    pub kind: String,
    #[serde(default)]
    pub param: Option<f64>,
}

/// A fresh work directory for one chain run, under the system temp dir.
pub fn chain_workdir() -> Result<String, String> {
    let dir = std::env::temp_dir().join(format!("laybacker-chain-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).map_err(|e| format!("Could not create work folder: {}", e))?;
    Ok(dir.to_string_lossy().to_string())
}

/// Remove a chain work directory. Refuses anything that isn't one of ours.
pub fn remove_workdir(path: &str) -> Result<(), String> {
    let p = PathBuf::from(path);
    let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if !name.starts_with("laybacker-chain-") || !p.starts_with(std::env::temp_dir()) {
        return Err("Not a Laybacker work folder".to_string());
    }
    if p.exists() {
        std::fs::remove_dir_all(&p).map_err(|e| format!("Could not remove work folder: {}", e))?;
    }
    Ok(())
}

/// What the shaping stage did: the file(s) to carry on with, and which steps
/// actually ran — a fold-to-stereo on a stereo file, or a split on a mono
/// file, is skipped quietly rather than failing the run.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ShapeResult {
    pub files: Vec<String>,
    pub applied: Vec<String>,
    pub skipped: Vec<String>,
}

/// Does this op make sense on a file with `channels` channels?
fn op_applies(kind: &str, channels: u32) -> bool {
    match kind {
        "fold_stereo" => channels >= 3,
        "fold_mono" | "split" => channels >= 2,
        _ => true,
    }
}

/// Run the chain's shaping steps on one file, in order, each into its own
/// numbered folder under `work_dir` with the source's stem kept ("Mix.wav"),
/// so the final name never picks up "_faded" or the like. Split, when present,
/// runs last and fans out. With no steps the source itself is returned, untouched.
pub fn shape_file(path: &str, ops: &[ShapeOp], work_dir: &str) -> Result<ShapeResult, String> {
    let mut result = ShapeResult { files: vec![path.to_string()], ..Default::default() };
    if ops.is_empty() {
        return Ok(result);
    }
    let stem = Path::new(path).file_stem().and_then(|s| s.to_str()).unwrap_or("audio").to_string();
    let mut current = path.to_string();
    let mut info = inspector::inspect_file(&current)?;
    let mut split_pending = false;
    for (i, op) in ops.iter().enumerate() {
        if !op_applies(&op.kind, info.channel_count.unwrap_or(1)) {
            result.skipped.push(op.kind.clone());
            continue;
        }
        if op.kind == "split" {
            split_pending = true;
            continue;
        }
        let step_dir = Path::new(work_dir).join(format!("step{}", i + 1));
        std::fs::create_dir_all(&step_dir).map_err(|e| format!("Could not create work folder: {}", e))?;
        let out = step_dir.join(format!("{}.wav", stem)).to_string_lossy().to_string();
        process_audio_to(
            &op.kind,
            &current,
            info.channel_layout.as_deref(),
            info.channel_count.unwrap_or(1),
            info.duration_secs,
            op.param,
            Some(&out),
        )?;
        current = out;
        info = inspector::inspect_file(&current)?;
        result.applied.push(op.kind.clone());
    }
    result.files = vec![current.clone()];
    if split_pending {
        let ch = info.channel_count.unwrap_or(1);
        if ch < 2 {
            result.skipped.push("split".to_string());
            return Ok(result);
        }
        let split_dir = Path::new(work_dir).join("split");
        std::fs::create_dir_all(&split_dir).map_err(|e| format!("Could not create work folder: {}", e))?;
        result.files = split_channels_to(&current, info.channel_layout.as_deref(), ch, Some(&split_dir.to_string_lossy()))?;
        result.applied.push("split".to_string());
    }
    Ok(result)
}

/// Write a text file (the chain's CSV report).
pub fn write_text_file(path: &str, contents: &str) -> Result<(), String> {
    std::fs::write(path, contents).map_err(|e| format!("Could not write {}: {}", path, e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fold_stereo_5_1_uses_itu_coefficients_and_drops_lfe() {
        let e = fold_stereo_expr("5.1", 6).unwrap();
        assert_eq!(e, "pan=stereo|FL=FL+0.707*FC+0.707*BL|FR=FR+0.707*FC+0.707*BR");
        assert!(!e.contains("LFE"));
    }

    #[test]
    fn test_fold_stereo_side_layout_and_7_1() {
        assert_eq!(
            fold_stereo_expr("5.1(side)", 6).unwrap(),
            "pan=stereo|FL=FL+0.707*FC+0.707*SL|FR=FR+0.707*FC+0.707*SR"
        );
        assert_eq!(
            fold_stereo_expr("7.1", 8).unwrap(),
            "pan=stereo|FL=FL+0.707*FC+0.707*BL+0.707*SL|FR=FR+0.707*FC+0.707*BR+0.707*SR"
        );
    }

    #[test]
    fn test_fold_stereo_refuses_stereo_and_unknown_count_falls_back() {
        assert!(fold_stereo_expr("stereo", 2).is_err());
        // Unknown layout name, 6 channels → treated as 5.1
        assert_eq!(fold_stereo_expr("weird", 6).unwrap(), fold_stereo_expr("5.1", 6).unwrap());
    }

    #[test]
    fn test_fold_mono_renormalises() {
        assert_eq!(fold_mono_expr("stereo", 2).unwrap(), "pan=mono|FC<FL+FR");
        assert_eq!(fold_mono_expr("5.1", 6).unwrap(), "pan=mono|FC<FL+FR+0.707*FC+0.707*BL+0.707*BR");
        assert!(fold_mono_expr("mono", 1).is_err());
    }

    #[test]
    fn test_fade_expr_clamps_to_half_duration() {
        assert_eq!(fade_expr(10.0, 0.5), "afade=t=in:st=0:d=0.5,afade=t=out:st=9.5:d=0.5");
        assert_eq!(fade_expr(1.0, 2.0), "afade=t=in:st=0:d=0.5,afade=t=out:st=0.5:d=0.5");
    }

    #[test]
    fn test_trim_expr_reverses_for_the_tail() {
        let e = trim_expr(-60.0);
        assert_eq!(e.matches("silenceremove").count(), 2);
        assert_eq!(e.matches("areverse").count(), 2);
        assert!(e.contains("start_threshold=-60dB"));
    }

    #[test]
    fn test_output_beside_and_command_shape() {
        assert_eq!(output_beside("/a/b/Mix.aif", "faded"), "/a/b/Mix_faded.wav");
        let args = build_process_command("/a/in.wav", "/a/out.wav", "afade=t=in:d=1");
        assert_eq!(args[args.len() - 1], "/a/out.wav");
        assert!(args.contains(&"pcm_s24le".to_string()));
        assert!(args.contains(&"-vn".to_string()));
    }

    #[test]
    fn test_workdir_guard_refuses_foreign_paths() {
        assert!(remove_workdir("/Users/someone/Music").is_err());
        assert!(remove_workdir("/tmp/not-ours").is_err());
        // A real one round-trips.
        let d = chain_workdir().unwrap();
        assert!(Path::new(&d).exists());
        remove_workdir(&d).unwrap();
        assert!(!Path::new(&d).exists());
    }

    #[test]
    fn test_shape_file_with_no_ops_returns_source() {
        let r = shape_file("/x/Mix.wav", &[], "/tmp").unwrap();
        assert_eq!(r.files, vec!["/x/Mix.wav".to_string()]);
        assert!(r.applied.is_empty() && r.skipped.is_empty());
    }

    #[test]
    fn test_op_applies_by_channel_count() {
        assert!(!op_applies("fold_stereo", 2));
        assert!(op_applies("fold_stereo", 6));
        assert!(!op_applies("fold_mono", 1));
        assert!(op_applies("fold_mono", 2));
        assert!(!op_applies("split", 1));
        assert!(op_applies("fade", 1));
        assert!(op_applies("trim", 1));
    }

    #[test]
    fn test_process_audio_rejects_unknown_op() {
        let err = process_audio("wibble", "/x.wav", None, 2, 1.0, None).unwrap_err();
        assert!(err.contains("Unknown processing op"));
    }
}
