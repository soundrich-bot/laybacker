//! Digital click detection for QC. A hard click is a sample-to-sample jump
//! far larger than the audio around it and over almost at once — an edit
//! pop, a dropout, a DC step. The detector streams the file through ffmpeg as
//! float PCM and watches, per channel, the first difference of the signal
//! against the biggest difference seen in the last ~20 ms (a decaying peak —
//! a real click towers over the steepest slope nearby, where a tone fading in
//! only ever just exceeds it). A hit
//! that lasts longer than half a millisecond is a transient, not a click, and
//! is ignored. Head and tail pops (a non-zero first / last sample) are
//! reported on their own — the 6 Fr mute already cures them.

use std::io::Read;
use std::process::Stdio;

use serde::{Deserialize, Serialize};

use crate::services::ffmpeg;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Click {
    /// Seconds from the start of the file.
    pub time: f64,
    /// 0-based channel.
    pub channel: u32,
    /// How far the jump stood above the local level, in dB.
    pub prominence_db: f64,
    /// Width of the excursion in milliseconds.
    pub width_ms: f64,
    /// The same click hit every channel at once (an edit pop, not a
    /// one-channel fault) — reported once, on channel 0.
    #[serde(default)]
    pub all_channels: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ClickReport {
    pub clicks: Vec<Click>,
    /// Total found (the list is capped, so this can be larger).
    pub count: u32,
    pub head_pop: bool,
    pub tail_pop: bool,
    pub sensitivity: String,
    pub sample_rate: u32,
    pub channels: u32,
}

/// The most clicks the report carries — the QC panel lists them; a file with
/// thousands is broken in a different way and the count says so.
pub const MAX_LISTED: usize = 200;

/// A click is over in less than this — anything longer is a real transient.
const MAX_CLICK_MS: f64 = 0.5;
/// Jumps below this level are never clicks, whatever the ratio (keeps silence
/// and dither from tripping the detector).
const FLOOR_DBFS: f64 = -48.0;
/// A first / last sample above this is a pop at the file edge.
const EDGE_POP_DBFS: f64 = -40.0;

/// Threshold ratio between a jump and the recent peak jump: how many times
/// bigger a difference has to be to count. Lower = more sensitive.
pub fn ratio_for(sensitivity: &str) -> f64 {
    match sensitivity {
        "high" => 4.0,
        "low" => 12.0,
        _ => 7.0, // normal
    }
}

fn db(v: f64) -> f64 {
    20.0 * v.max(1e-9).log10()
}

/// Per-channel detector state, fed one sample at a time.
struct ChannelState {
    prev: f32,
    /// Decaying peak of |diff| (~20 ms), trained only on ordinary samples.
    env: f64,
    /// Current excursion: start sample index, samples so far, peak ratio.
    run: Option<(u64, u32, f64)>,
    /// Samples seen.
    n: u64,
    last_reported: u64,
}

impl ChannelState {
    fn new() -> Self {
        Self { prev: 0.0, env: 0.0, run: None, n: 0, last_reported: 0 }
    }
}

/// Streaming detector shared by the file path and the tests.
pub struct Detector {
    rate: u32,
    channels: u32,
    ratio: f64,
    /// Per-sample decay of the peak reference.
    decay: f64,
    max_run: u32,
    min_gap: u64,
    states: Vec<ChannelState>,
    floor: f64,
    pub clicks: Vec<Click>,
    pub count: u32,
    pub first_sample: Option<f32>,
    pub last_sample: f32,
}

impl Detector {
    pub fn new(rate: u32, channels: u32, sensitivity: &str) -> Self {
        let rate = rate.max(8000);
        Self {
            rate,
            channels: channels.max(1),
            ratio: ratio_for(sensitivity),
            // The peak reference falls to 1/e in ~20 ms.
            decay: (-1.0 / (rate as f64 * 0.02)).exp(),
            max_run: ((MAX_CLICK_MS / 1000.0) * rate as f64).ceil() as u32,
            // Don't report the same click twice within 5 ms.
            min_gap: (rate as f64 * 0.005) as u64,
            states: (0..channels.max(1)).map(|_| ChannelState::new()).collect(),
            floor: 10f64.powf(FLOOR_DBFS / 20.0),
            clicks: Vec::new(),
            count: 0,
            first_sample: None,
            last_sample: 0.0,
        }
    }

    /// Feed interleaved samples.
    pub fn feed(&mut self, samples: &[f32]) {
        let ch = self.channels as usize;
        for frame in samples.chunks_exact(ch) {
            if self.first_sample.is_none() {
                self.first_sample = Some(frame.iter().fold(0f32, |m, v| if v.abs() > m.abs() { *v } else { m }));
            }
            self.last_sample = frame.iter().fold(0f32, |m, v| if v.abs() > m.abs() { *v } else { m });
            for (c, &v) in frame.iter().enumerate() {
                self.step(c, v);
            }
        }
    }

    fn step(&mut self, c: usize, v: f32) {
        let ratio = self.ratio;
        let decay = self.decay;
        let max_run = self.max_run;
        let min_gap = self.min_gap;
        let floor = self.floor;
        let rate = self.rate as f64;
        let st = &mut self.states[c];
        let diff = (v - st.prev).abs() as f64;
        st.prev = v;
        st.n += 1;
        // Is this jump an outlier against the steepest slope seen lately?
        // (The reference has a floor of its own so silence can't make a
        // whisper look like a click.)
        let reference = st.env.max(floor / ratio);
        let outlier = diff > floor && diff > ratio * reference;
        match (&mut st.run, outlier) {
            (None, true) => {
                st.run = Some((st.n, 1, diff / reference));
            }
            (Some(run), true) => {
                run.1 += 1;
                run.2 = run.2.max(diff / reference);
            }
            (Some(run), false) => {
                let (start, len, peak) = *run;
                st.run = None;
                if len <= max_run && start.saturating_sub(st.last_reported) >= min_gap {
                    st.last_reported = start;
                    self.count += 1;
                    if self.clicks.len() < MAX_LISTED {
                        self.clicks.push(Click {
                            time: (start.saturating_sub(1)) as f64 / rate,
                            channel: c as u32,
                            prominence_db: db(peak),
                            width_ms: len as f64 / rate * 1000.0,
                            all_channels: false,
                        });
                    }
                }
            }
            (None, false) => {}
        }
        // Only ordinary jumps train the reference — an outlier must not raise
        // the bar for the samples right after it.
        st.env *= decay;
        if !outlier && diff > st.env {
            st.env = diff;
        }
    }

    pub fn finish(mut self, sensitivity: &str) -> ClickReport {
        // Close any run still open at the end of the file.
        for c in 0..self.channels as usize {
            let st = &mut self.states[c];
            if let Some((start, len, peak)) = st.run.take() {
                if len <= self.max_run {
                    self.count += 1;
                    if self.clicks.len() < MAX_LISTED {
                        self.clicks.push(Click {
                            time: start.saturating_sub(1) as f64 / self.rate as f64,
                            channel: c as u32,
                            prominence_db: db(peak),
                            width_ms: len as f64 / self.rate as f64 * 1000.0,
                            all_channels: false,
                        });
                    }
                }
            }
        }
        let edge = 10f64.powf(EDGE_POP_DBFS / 20.0) as f32;
        self.clicks.sort_by(|a, b| a.time.partial_cmp(&b.time).unwrap_or(std::cmp::Ordering::Equal));
        // One click that hits every channel within a millisecond is one
        // event, not one per channel. Keep the loudest, mark it, drop the rest.
        if self.channels > 1 {
            let window = 0.001;
            let mut merged: Vec<Click> = Vec::with_capacity(self.clicks.len());
            let mut i = 0;
            while i < self.clicks.len() {
                let mut j = i + 1;
                let mut seen: Vec<u32> = vec![self.clicks[i].channel];
                let mut best = self.clicks[i].clone();
                while j < self.clicks.len() && self.clicks[j].time - self.clicks[i].time <= window {
                    if !seen.contains(&self.clicks[j].channel) {
                        seen.push(self.clicks[j].channel);
                    }
                    if self.clicks[j].prominence_db > best.prominence_db {
                        best = self.clicks[j].clone();
                    }
                    j += 1;
                }
                if seen.len() as u32 >= self.channels {
                    best.channel = 0;
                    best.all_channels = true;
                    self.count = self.count.saturating_sub((j - i - 1) as u32);
                    merged.push(best);
                } else {
                    merged.extend(self.clicks[i..j].iter().cloned());
                }
                i = j;
            }
            self.clicks = merged;
        }
        ClickReport {
            head_pop: self.first_sample.map(|s| s.abs() > edge).unwrap_or(false),
            tail_pop: self.last_sample.abs() > edge,
            count: self.count,
            clicks: self.clicks,
            sensitivity: sensitivity.to_string(),
            sample_rate: self.rate,
            channels: self.channels,
        }
    }
}

/// Scan a file for clicks. Streams the decode so a long file never sits in
/// memory whole.
pub fn detect_clicks(path: &str, sample_rate: u32, channels: u32, sensitivity: &str) -> Result<ClickReport, String> {
    let rate = if sample_rate == 0 { 48000 } else { sample_rate };
    let ch = channels.max(1);
    let mut child = ffmpeg::silent_command(&ffmpeg::find_ffmpeg())
        .args([
            "-v", "error",
            "-i", path,
            "-vn",
            "-ac", &ch.to_string(),
            "-ar", &rate.to_string(),
            "-f", "f32le",
            "-",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to run ffmpeg: {}", e))?;
    let mut out = child.stdout.take().ok_or("no ffmpeg output")?;
    let mut det = Detector::new(rate, ch, sensitivity);
    let mut buf = vec![0u8; 1 << 20];
    let mut carry: Vec<u8> = Vec::new();
    loop {
        let n = out.read(&mut buf).map_err(|e| format!("Read failed: {}", e))?;
        if n == 0 {
            break;
        }
        carry.extend_from_slice(&buf[..n]);
        // Whole frames only (4 bytes × channels).
        let frame_bytes = 4 * ch as usize;
        let usable = carry.len() - carry.len() % frame_bytes;
        let samples: Vec<f32> = carry[..usable]
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .collect();
        det.feed(&samples);
        carry.drain(..usable);
    }
    let status = child.wait().map_err(|e| format!("ffmpeg failed: {}", e))?;
    if !status.success() {
        let mut err = String::new();
        if let Some(mut e) = child.stderr.take() {
            let _ = e.read_to_string(&mut err);
        }
        return Err(format!("Could not decode audio: {}", err.trim()));
    }
    Ok(det.finish(sensitivity))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(rate: u32, secs: f64, freq: f64, amp: f32) -> Vec<f32> {
        (0..(rate as f64 * secs) as usize)
            .map(|i| amp * (2.0 * std::f64::consts::PI * freq * i as f64 / rate as f64).sin() as f32)
            .collect()
    }

    #[test]
    fn test_clean_tone_has_no_clicks() {
        // Faded in and out, so the ends are clean (a tone cut mid-cycle IS a pop).
        let mut s = tone(48000, 2.0, 1000.0, 0.5);
        let n = s.len();
        for i in 0..480 {
            let g = i as f32 / 480.0;
            s[i] *= g;
            s[n - 1 - i] *= g;
        }
        let mut d = Detector::new(48000, 1, "normal");
        d.feed(&s);
        let r = d.finish("normal");
        assert_eq!(r.count, 0, "{:?}", r.clicks);
        assert!(!r.head_pop && !r.tail_pop);
    }

    #[test]
    fn test_tone_cut_mid_cycle_is_a_tail_pop() {
        let mut d = Detector::new(48000, 1, "normal");
        d.feed(&tone(48000, 2.0, 1000.0, 0.5));
        let r = d.finish("normal");
        assert!(r.tail_pop && !r.head_pop);
    }

    #[test]
    fn test_single_sample_spike_is_found_at_the_right_time() {
        let mut s = tone(48000, 2.0, 200.0, 0.3);
        s[48000] = 0.9; // one-sample spike at 1.000 s
        s[48000 + 24000] += 0.8; // a step at 1.5 s
        let mut d = Detector::new(48000, 1, "normal");
        d.feed(&s);
        let r = d.finish("normal");
        assert!(r.count >= 2, "{:?}", r.clicks);
        assert!(r.clicks.iter().any(|c| (c.time - 1.0).abs() < 0.001), "{:?}", r.clicks);
        assert!(r.clicks.iter().any(|c| (c.time - 1.5).abs() < 0.001), "{:?}", r.clicks);
        assert!(r.clicks.iter().all(|c| c.width_ms <= 0.5));
    }

    #[test]
    fn test_a_real_transient_is_not_a_click() {
        // A 5 ms burst (a hand clap-ish edge) rises over many samples.
        let rate = 48000;
        let mut s = tone(rate, 2.0, 200.0, 0.3);
        for i in 0..240 {
            let t = i as f32 / 240.0;
            s[48000 + i] += 0.6 * (1.0 - t) * ((i as f32) * 0.9).sin();
        }
        let mut d = Detector::new(rate, 1, "normal");
        d.feed(&s);
        let r = d.finish("normal");
        // The burst's onset may be sharp, but nothing sustained inside it counts.
        assert!(r.count <= 1, "{:?}", r.clicks);
    }

    #[test]
    fn test_edge_pops_and_stereo_channel_index() {
        let mut s = vec![0f32; 48000 * 2];
        s[0] = 0.5; // head pop on the left
        s[48000 * 2 - 1] = 0.4; // tail pop on the right
        s[24000 * 2 + 1] = 0.7; // mid-file click on the right (channel 1)
        let mut d = Detector::new(48000, 2, "high");
        d.feed(&s);
        let r = d.finish("high");
        assert!(r.head_pop && r.tail_pop);
        assert!(r.clicks.iter().any(|c| c.channel == 1 && !c.all_channels && (c.time - 0.5).abs() < 0.001), "{:?}", r.clicks);
    }

    #[test]
    fn test_click_on_both_channels_is_one_event() {
        let mut s = vec![0f32; 48000 * 2];
        let l = tone(48000, 1.0, 300.0, 0.3);
        for (i, v) in l.iter().enumerate() { s[2 * i] = *v; s[2 * i + 1] = *v; }
        s[2 * 24000] += 0.8;     // same instant, both channels
        s[2 * 24000 + 1] += 0.8;
        let mut d = Detector::new(48000, 2, "normal");
        d.feed(&s);
        let r = d.finish("normal");
        assert_eq!(r.count, 1, "{:?}", r.clicks);
        assert_eq!(r.clicks.len(), 1);
        assert!(r.clicks[0].all_channels);
    }

    #[test]
    fn test_sensitivity_orders() {
        assert!(ratio_for("high") < ratio_for("normal") && ratio_for("normal") < ratio_for("low"));
    }
}
