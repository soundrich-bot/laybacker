//! Fast slates: re-encode only what changes. A prepended card is encoded on
//! its own and the whole programme is copied; an overlay re-encodes the
//! picture only up to the first keyframe past the text, and copies the rest.
//! The pieces are joined as MPEG-TS segments (which carry their own
//! timestamps and parameter sets) and remuxed to a video-only .mov that the
//! normal slate commands then treat as the source, copying the video.
//!
//! Only H.264 sources with a constant frame rate qualify, and every result is
//! verified — exact packet count, clean decode — before it is used. Anything
//! that fails falls back to the full re-encode, so the fast path can only
//! ever save time, never change the outcome.

use std::path::Path;

use crate::services::ffmpeg::{self, SlateSpec};

/// What the fast path needs to know about the source picture.
#[derive(Debug, Clone, PartialEq)]
pub struct SourceVideo {
    pub codec: String,
    pub profile: String,
    pub level: i64,
    pub pix_fmt: String,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub sar: String,
    /// Keyframe times, seconds, ascending.
    pub keyframes: Vec<f64>,
    pub packets: u64,
}

/// x264's name for an ffprobe profile, or None when the fast path can't
/// match it (10-bit, 4:2:2 and the like).
pub fn x264_profile(probe_profile: &str) -> Option<&'static str> {
    match probe_profile.trim() {
        "High" => Some("high"),
        "Main" => Some("main"),
        "Baseline" | "Constrained Baseline" => Some("baseline"),
        _ => None,
    }
}

/// Can this source take the fast path at all?
pub fn eligible(v: &SourceVideo) -> Result<(), String> {
    if v.codec != "h264" {
        return Err(format!("source is {}, not H.264", v.codec));
    }
    if x264_profile(&v.profile).is_none() {
        return Err(format!("H.264 profile {} can't be matched", v.profile));
    }
    if v.fps <= 0.0 || v.width == 0 || v.height == 0 || v.pix_fmt.is_empty() {
        return Err("incomplete stream details".to_string());
    }
    if !matches!(v.pix_fmt.as_str(), "yuv420p" | "yuvj420p") {
        return Err(format!("pixel format {} can't be matched", v.pix_fmt));
    }
    if v.packets == 0 || v.keyframes.is_empty() {
        return Err("no frames".to_string());
    }
    Ok(())
}

/// The first keyframe at or after `secs` — where an overlay's re-encoded
/// head ends and the copied tail begins.
pub fn cut_point(keyframes: &[f64], secs: f64) -> Option<f64> {
    keyframes.iter().copied().find(|k| *k >= secs - 0.0005)
}

/// Probe the source: stream details plus every keyframe time (from packet
/// flags, so nothing is decoded).
pub fn probe(path: &str) -> Result<SourceVideo, String> {
    let out = ffmpeg::silent_command(&ffmpeg::find_ffprobe())
        .args([
            "-v", "error",
            "-select_streams", "v:0",
            "-show_entries",
            "stream=codec_name,profile,level,pix_fmt,width,height,r_frame_rate,avg_frame_rate,sample_aspect_ratio:packet=pts_time,dts_time,flags",
            "-of", "json",
            path,
        ])
        .output()
        .map_err(|e| format!("Failed to run ffprobe: {}", e))?;
    if !out.status.success() {
        return Err(format!("ffprobe failed: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).map_err(|e| format!("ffprobe output: {}", e))?;
    parse_probe(&json)
}

fn parse_rate(s: &str) -> f64 {
    match s.split_once('/') {
        Some((n, d)) => {
            let n: f64 = n.parse().unwrap_or(0.0);
            let d: f64 = d.parse().unwrap_or(1.0);
            if d > 0.0 { n / d } else { 0.0 }
        }
        None => s.parse().unwrap_or(0.0),
    }
}

pub fn parse_probe(json: &serde_json::Value) -> Result<SourceVideo, String> {
    let stream = json["streams"].get(0).ok_or("no video stream")?;
    let r = parse_rate(stream["r_frame_rate"].as_str().unwrap_or("0"));
    let avg = parse_rate(stream["avg_frame_rate"].as_str().unwrap_or("0"));
    // A variable frame rate can't be re-timed reliably — leave it to the full path.
    if avg > 0.0 && (r - avg).abs() > 0.01 {
        return Err(format!("variable frame rate ({:.3} vs {:.3})", r, avg));
    }
    let mut keyframes = Vec::new();
    let mut packets = 0u64;
    if let Some(pk) = json["packets"].as_array() {
        for p in pk {
            packets += 1;
            if p["flags"].as_str().map(|f| f.starts_with('K')).unwrap_or(false) {
                let t = p["pts_time"].as_str().or(p["dts_time"].as_str()).and_then(|s| s.parse::<f64>().ok());
                if let Some(t) = t {
                    keyframes.push(t);
                }
            }
        }
    }
    keyframes.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let sar = stream["sample_aspect_ratio"].as_str().unwrap_or("1:1");
    Ok(SourceVideo {
        codec: stream["codec_name"].as_str().unwrap_or("").to_string(),
        profile: stream["profile"].as_str().unwrap_or("").to_string(),
        level: stream["level"].as_i64().unwrap_or(0),
        pix_fmt: stream["pix_fmt"].as_str().unwrap_or("").to_string(),
        width: stream["width"].as_u64().unwrap_or(0) as u32,
        height: stream["height"].as_u64().unwrap_or(0) as u32,
        fps: r,
        sar: if sar == "0:1" || sar.is_empty() { "1:1".to_string() } else { sar.to_string() },
        keyframes,
        packets,
    })
}

fn x264_args(v: &SourceVideo) -> Vec<String> {
    let mut a: Vec<String> = vec![
        "-c:v".into(), "libx264".into(),
        "-profile:v".into(), x264_profile(&v.profile).unwrap_or("high").into(),
        "-crf".into(), "18".into(),
        "-preset".into(), "medium".into(),
        "-pix_fmt".into(), v.pix_fmt.clone(),
    ];
    if v.level > 0 {
        a.extend(["-level".into(), format!("{:.1}", v.level as f64 / 10.0)]);
    }
    a
}

/// Encode the head segment. Prepend: the card (plus black) at the source's
/// size, rate and codec parameters. Overlay: the source up to `cut`, with the
/// text over it.
pub fn build_head_command(video_path: &str, slate: &SlateSpec, v: &SourceVideo, cut: Option<f64>, out: &str) -> Vec<String> {
    let sar = v.sar.replace(':', "/");
    let mut args: Vec<String> = vec!["-y".into()];
    if let (Some(matte), Some(cut)) = (&slate.matte_path, cut) {
        // Overlay head: decode the source up to the cut, lay the text over it.
        args.extend(["-t".into(), format!("{:.4}", cut), "-i".into(), video_path.into()]);
        args.extend(slate.still_input(&slate.image_path, v.fps));
        args.extend(slate.still_input(matte, v.fps));
        args.extend([
            "-filter_complex".into(),
            slate.overlay_graph_with_format("1:v", "2:v", &format!("setsar={}", sar), &v.pix_fmt),
            "-map".into(), "[v]".into(),
        ]);
    } else {
        // Card head: the still, looped, scaled to the frame, then black if asked.
        args.extend(slate.still_input(&slate.image_path, v.fps));
        args.extend([
            "-vf".into(),
            format!("scale={}:{},setsar={},{}", v.width, v.height, sar, slate.stream_chain_for(&v.pix_fmt)),
        ]);
    }
    args.extend(["-an".into(), "-r".into(), format!("{:.5}", v.fps)]);
    args.extend(x264_args(v));
    args.extend(["-f".into(), "mpegts".into(), out.into()]);
    args
}

/// Copy the tail segment: the whole programme (prepend) or from `cut` on (overlay).
pub fn build_tail_command(video_path: &str, cut: Option<f64>, out: &str) -> Vec<String> {
    let mut args: Vec<String> = vec!["-y".into()];
    if let Some(c) = cut {
        args.extend(["-ss".into(), format!("{:.4}", c)]);
    }
    args.extend([
        "-i".into(), video_path.into(),
        "-map".into(), "0:v:0".into(),
        "-c:v".into(), "copy".into(),
        "-bsf:v".into(), "h264_mp4toannexb".into(),
        "-f".into(), "mpegts".into(),
        out.into(),
    ]);
    args
}

/// Join the segments into a video-only .mov, copying.
pub fn build_join_command(list_path: &str, out: &str) -> Vec<String> {
    vec![
        "-y".into(),
        "-f".into(), "concat".into(), "-safe".into(), "0".into(),
        "-i".into(), list_path.into(),
        "-map".into(), "0:v:0".into(),
        "-c:v".into(), "copy".into(),
        out.into(),
    ]
}

fn count_packets(path: &str) -> Result<u64, String> {
    let out = ffmpeg::silent_command(&ffmpeg::find_ffprobe())
        .args(["-v", "error", "-select_streams", "v:0", "-count_packets", "-show_entries", "stream=nb_read_packets", "-of", "csv=p=0", path])
        .output()
        .map_err(|e| format!("Failed to run ffprobe: {}", e))?;
    // MPEG-TS reports the stream inside its program too — one count is enough.
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .and_then(|l| l.parse::<u64>().ok())
        .ok_or_else(|| "could not count packets".to_string())
}

/// Decode the whole file; any complaint from the decoder fails verification.
fn decodes_cleanly(path: &str) -> Result<(), String> {
    let out = ffmpeg::silent_command(&ffmpeg::find_ffmpeg())
        .args(["-v", "error", "-i", path, "-f", "null", "-"])
        .output()
        .map_err(|e| format!("Failed to run ffmpeg: {}", e))?;
    let err = String::from_utf8_lossy(&out.stderr);
    if !out.status.success() || !err.trim().is_empty() {
        return Err(format!("decode check: {}", err.trim()));
    }
    Ok(())
}

/// Build the pre-slated, video-only .mov in `work_dir`. Returns its path, or
/// an Err explaining why the full re-encode is needed instead.
pub fn build_spliced_video(video_path: &str, slate: &SlateSpec, v: &SourceVideo, work_dir: &str) -> Result<String, String> {
    eligible(v)?;
    let cut = if slate.is_overlay() {
        let c = cut_point(&v.keyframes, slate.duration_secs).ok_or("no keyframe after the overlay — nothing to copy")?;
        // If the cut is at the very end there is nothing to save.
        if c >= *v.keyframes.last().unwrap_or(&0.0) && v.keyframes.len() > 1 && c > slate.duration_secs * 4.0 {
            // Fine — still valid; a long GOP just means more re-encode. Keep going.
        }
        Some(c)
    } else {
        None
    };
    let dir = Path::new(work_dir);
    std::fs::create_dir_all(dir).map_err(|e| format!("work folder: {}", e))?;
    let head = dir.join("head.ts").to_string_lossy().to_string();
    let tail = dir.join("tail.ts").to_string_lossy().to_string();
    let list = dir.join("segments.txt").to_string_lossy().to_string();
    let out = dir.join("spliced.mov").to_string_lossy().to_string();

    ffmpeg::run_ffmpeg(&build_head_command(video_path, slate, v, cut, &head))?;
    ffmpeg::run_ffmpeg(&build_tail_command(video_path, cut, &tail))?;
    std::fs::write(&list, format!("file '{}'\nfile '{}'\n", head.replace('\'', "'\\''"), tail.replace('\'', "'\\''")))
        .map_err(|e| format!("segment list: {}", e))?;
    ffmpeg::run_ffmpeg(&build_join_command(&list, &out))?;

    // Verify: every frame accounted for, and the join decodes without a murmur.
    let expected = count_packets(&head)? + count_packets(&tail)?;
    let got = count_packets(&out)?;
    if got != expected {
        return Err(format!("frame count {} ≠ expected {}", got, expected));
    }
    if let Some(c) = cut {
        // An overlay must give back exactly the source's frame count.
        if got != v.packets {
            return Err(format!("overlay join has {} frames, source has {} (cut at {:.3}s)", got, v.packets, c));
        }
    }
    decodes_cleanly(&out)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn src() -> SourceVideo {
        SourceVideo {
            codec: "h264".into(), profile: "High".into(), level: 40, pix_fmt: "yuv420p".into(),
            width: 1920, height: 1080, fps: 25.0, sar: "1:1".into(),
            keyframes: vec![0.0, 9.96, 19.92], packets: 750,
        }
    }

    #[test]
    fn test_eligibility() {
        assert!(eligible(&src()).is_ok());
        let mut v = src(); v.codec = "prores".into();
        assert!(eligible(&v).unwrap_err().contains("not H.264"));
        let mut v = src(); v.profile = "High 10".into();
        assert!(eligible(&v).unwrap_err().contains("profile"));
        let mut v = src(); v.pix_fmt = "yuv422p".into();
        assert!(eligible(&v).unwrap_err().contains("pixel format"));
    }

    #[test]
    fn test_cut_point_is_first_keyframe_at_or_after() {
        assert_eq!(cut_point(&[0.0, 9.96, 19.92], 5.0), Some(9.96));
        assert_eq!(cut_point(&[0.0, 9.96, 19.92], 9.96), Some(9.96));
        assert_eq!(cut_point(&[0.0, 9.96, 19.92], 25.0), None);
    }

    #[test]
    fn test_parse_probe_reads_keyframes_and_rejects_vfr() {
        let j: serde_json::Value = serde_json::from_str(r#"{
          "streams":[{"codec_name":"h264","profile":"High","level":40,"pix_fmt":"yuv420p","width":320,"height":180,
                      "r_frame_rate":"25/1","avg_frame_rate":"25/1","sample_aspect_ratio":"1:1"}],
          "packets":[{"pts_time":"0.000000","flags":"K__"},{"pts_time":"0.040000","flags":"___"},{"pts_time":"2.000000","flags":"K__"}]}"#).unwrap();
        let v = parse_probe(&j).unwrap();
        assert_eq!(v.keyframes, vec![0.0, 2.0]);
        assert_eq!(v.packets, 3);
        assert_eq!(v.fps, 25.0);
        let j2: serde_json::Value = serde_json::from_str(r#"{"streams":[{"codec_name":"h264","profile":"High","level":40,"pix_fmt":"yuv420p","width":320,"height":180,"r_frame_rate":"30/1","avg_frame_rate":"29/1"}],"packets":[]}"#).unwrap();
        assert!(parse_probe(&j2).unwrap_err().contains("variable frame rate"));
    }

    #[test]
    fn test_head_command_matches_source_parameters() {
        let slate = SlateSpec { image_path: "/tmp/card.jpg".into(), duration_secs: 4.0, black_secs: 1.0, matte_path: None, prebaked: false };
        let args = build_head_command("/v.mov", &slate, &src(), None, "/w/head.ts");
        let j = args.join(" ");
        assert!(j.contains("-profile:v high") && j.contains("-level 4.0") && j.contains("-pix_fmt yuv420p"));
        assert!(j.contains("scale=1920:1080") && j.contains("tpad=stop_mode=add:stop_duration=1.0000"));
        assert!(j.contains("-f mpegts /w/head.ts") && j.contains("-an"));
        assert!(!j.contains("/v.mov"), "a card head never reads the source");

        let ov = SlateSpec { image_path: "/tmp/t.jpg".into(), duration_secs: 3.0, black_secs: 0.0, matte_path: Some("/tmp/m.jpg".into()), prebaked: false };
        let args = build_head_command("/v.mov", &ov, &src(), Some(9.96), "/w/head.ts");
        let j = args.join(" ");
        assert!(j.starts_with("-y -t 9.9600 -i /v.mov"), "{j}");
        assert!(j.contains("alphamerge") && j.contains("overlay=0:0:eof_action=pass"));
        assert!(j.contains("format=yuv420p[v]"));
    }

    #[test]
    fn test_tail_and_join_commands_copy() {
        let t = build_tail_command("/v.mov", Some(9.96), "/w/tail.ts").join(" ");
        assert!(t.starts_with("-y -ss 9.9600 -i /v.mov") && t.contains("-c:v copy") && t.contains("h264_mp4toannexb"));
        let t0 = build_tail_command("/v.mov", None, "/w/tail.ts").join(" ");
        assert!(!t0.contains("-ss"));
        let jn = build_join_command("/w/segments.txt", "/w/spliced.mov").join(" ");
        assert!(jn.contains("-f concat -safe 0 -i /w/segments.txt") && jn.contains("-c:v copy"));
    }
}
