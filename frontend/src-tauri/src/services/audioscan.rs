//! QC scan for two faults you can't hear until it's too late: clipping and
//! dropouts. One streaming decode (float PCM through ffmpeg), two detectors.
//!
//! Clipping: a run of samples pinned at full scale, or a flat top — several
//! consecutive samples with exactly the same high value, the fingerprint of
//! audio that was clipped upstream and turned down afterwards.
//!
//! Dropouts: a run of digital silence inside the programme (every channel
//! below −70 dBFS for at least 50 ms). Silence that touches the head or tail
//! is not a dropout — that's the 6 Fr check's business.

use std::io::Read;
use std::process::Stdio;

use serde::{Deserialize, Serialize};

use crate::services::ffmpeg;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ClipEvent {
    pub time: f64,
    pub channel: u32,
    /// Consecutive clipped samples.
    pub samples: u32,
    /// Level of the run, dBFS.
    pub level_db: f64,
    /// "hard" (at full scale) or "flat" (a flat top below full scale).
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ClipReport {
    pub events: Vec<ClipEvent>,
    pub count: u32,
    pub clipped_samples: u64,
    pub longest_run: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Gap {
    pub start: f64,
    pub duration: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct DropoutReport {
    pub gaps: Vec<Gap>,
    pub count: u32,
    pub total_secs: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ScanReport {
    pub clipping: ClipReport,
    pub dropouts: DropoutReport,
    pub sample_rate: u32,
    pub channels: u32,
    pub duration_secs: f64,
}

pub const MAX_LISTED: usize = 200;

/// Hard clip: this close to full scale…
const HARD_LEVEL: f32 = 0.999;
/// …for at least this many consecutive samples.
const HARD_RUN: u32 = 3;
/// Flat top: this many consecutive samples with exactly the same value…
const FLAT_RUN: u32 = 4;
/// …at or above this level (−12 dBFS). Quiet flats are just silence or DC.
const FLAT_LEVEL: f32 = 0.25;
/// Two clip runs closer than this are one event.
const MERGE_MS: f64 = 20.0;

/// Dropout: every channel below this (−70 dBFS)…
const DROP_LEVEL: f32 = 0.000_316;
/// …for at least this long.
const DROP_MIN_MS: f64 = 50.0;

fn db(v: f64) -> f64 {
    20.0 * v.max(1e-9).log10()
}

struct ClipState {
    /// Current run: (start sample, length, peak level, kind), if any.
    run: Option<(u64, u32, f32, &'static str)>,
    prev: f32,
    same_count: u32, // consecutive samples equal to `prev`
    last_event_end: u64,
}

pub struct Scanner {
    rate: u32,
    channels: u32,
    n: u64, // frames seen
    clips: Vec<ClipState>,
    merge_samples: u64,
    // Dropouts (all channels together)
    silent_since: Option<u64>,
    drop_min: u64,
    heard_sound: bool,
    pub clipping: ClipReport,
    pub dropouts: DropoutReport,
    // A gap still open at the end of the file is tail silence, not a dropout.
    pending_gap: Option<(u64, u64)>,
}

impl Scanner {
    pub fn new(rate: u32, channels: u32) -> Self {
        let rate = rate.max(8000);
        let ch = channels.max(1);
        Self {
            rate,
            channels: ch,
            n: 0,
            clips: (0..ch).map(|_| ClipState { run: None, prev: 0.0, same_count: 0, last_event_end: 0 }).collect(),
            merge_samples: (rate as f64 * MERGE_MS / 1000.0) as u64,
            silent_since: None,
            drop_min: (rate as f64 * DROP_MIN_MS / 1000.0) as u64,
            heard_sound: false,
            clipping: ClipReport::default(),
            dropouts: DropoutReport::default(),
            pending_gap: None,
        }
    }

    pub fn feed(&mut self, samples: &[f32]) {
        let ch = self.channels as usize;
        for frame in samples.chunks_exact(ch) {
            self.n += 1;
            let mut all_silent = true;
            for (c, &v) in frame.iter().enumerate() {
                if v.abs() >= DROP_LEVEL {
                    all_silent = false;
                }
                self.clip_step(c, v);
            }
            self.dropout_step(all_silent);
        }
    }

    fn clip_step(&mut self, c: usize, v: f32) {
        let a = v.abs();
        let st = &mut self.clips[c];
        // Flat-top bookkeeping: exact repeats of the previous sample.
        if v == st.prev {
            st.same_count += 1;
        } else {
            st.same_count = 1;
        }
        st.prev = v;
        let hard = a >= HARD_LEVEL;
        let flat = st.same_count >= FLAT_RUN && a >= FLAT_LEVEL;
        let clipped = hard || flat;
        match (&mut st.run, clipped) {
            (None, true) => {
                // A flat top is detected on its 4th sample — count from its first.
                let start = if flat && !hard { self.n.saturating_sub(st.same_count as u64 - 1) } else { self.n };
                let len = if flat && !hard { st.same_count } else { 1 };
                st.run = Some((start, len, a, if hard { "hard" } else { "flat" }));
            }
            (Some(run), true) => {
                run.1 += 1;
                run.2 = run.2.max(a);
                if hard { run.3 = "hard"; }
            }
            (Some(_), false) => {
                let (start, len, peak, kind) = st.run.take().unwrap();
                self.close_clip(c, start, len, peak, kind);
            }
            (None, false) => {}
        }
    }

    fn close_clip(&mut self, c: usize, start: u64, len: u32, peak: f32, kind: &'static str) {
        // A hard run must be long enough to mean anything; a flat run already is.
        if kind == "hard" && len < HARD_RUN {
            return;
        }
        let st = &mut self.clips[c];
        self.clipping.clipped_samples += len as u64;
        self.clipping.longest_run = self.clipping.longest_run.max(len);
        let end = start + len as u64;
        // Merge with a run that just ended on this channel.
        if st.last_event_end > 0 && start.saturating_sub(st.last_event_end) <= self.merge_samples {
            st.last_event_end = end;
            if let Some(ev) = self.clipping.events.iter_mut().rev().find(|e| e.channel == c as u32) {
                ev.samples += len;
                ev.level_db = ev.level_db.max(db(peak as f64));
                if kind == "hard" { ev.kind = "hard".into(); }
            }
            return;
        }
        st.last_event_end = end;
        self.clipping.count += 1;
        if self.clipping.events.len() < MAX_LISTED {
            self.clipping.events.push(ClipEvent {
                time: start.saturating_sub(1) as f64 / self.rate as f64,
                channel: c as u32,
                samples: len,
                level_db: db(peak as f64),
                kind: kind.to_string(),
            });
        }
    }

    fn dropout_step(&mut self, all_silent: bool) {
        if all_silent {
            if self.silent_since.is_none() {
                self.silent_since = Some(self.n);
            }
        } else {
            if let Some(start) = self.silent_since.take() {
                let len = self.n - start;
                // Silence from the very start is a head, not a gap.
                if self.heard_sound && len >= self.drop_min {
                    self.push_gap(start, len);
                }
            }
            self.heard_sound = true;
        }
    }

    fn push_gap(&mut self, start: u64, len: u64) {
        self.dropouts.count += 1;
        self.dropouts.total_secs += len as f64 / self.rate as f64;
        if self.dropouts.gaps.len() < MAX_LISTED {
            self.dropouts.gaps.push(Gap {
                start: start.saturating_sub(1) as f64 / self.rate as f64,
                duration: len as f64 / self.rate as f64,
            });
        }
    }

    pub fn finish(mut self) -> ScanReport {
        for c in 0..self.channels as usize {
            if let Some((start, len, peak, kind)) = self.clips[c].run.take() {
                self.close_clip(c, start, len, peak, kind);
            }
        }
        // Silence still running at the end is the tail — not a dropout.
        self.silent_since = None;
        self.pending_gap = None;
        ScanReport {
            clipping: self.clipping,
            dropouts: self.dropouts,
            sample_rate: self.rate,
            channels: self.channels,
            duration_secs: self.n as f64 / self.rate as f64,
        }
    }
}

/// Scan a file. Streams the decode so a long file never sits in memory whole.
pub fn scan(path: &str, sample_rate: u32, channels: u32) -> Result<ScanReport, String> {
    let rate = if sample_rate == 0 { 48000 } else { sample_rate };
    let ch = channels.max(1);
    let mut child = ffmpeg::silent_command(&ffmpeg::find_ffmpeg())
        .args(["-v", "error", "-i", path, "-vn", "-ac", &ch.to_string(), "-ar", &rate.to_string(), "-f", "f32le", "-"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to run ffmpeg: {}", e))?;
    let mut out = child.stdout.take().ok_or("no ffmpeg output")?;
    let mut sc = Scanner::new(rate, ch);
    let mut buf = vec![0u8; 1 << 20];
    let mut carry: Vec<u8> = Vec::new();
    let frame_bytes = 4 * ch as usize;
    loop {
        let n = out.read(&mut buf).map_err(|e| format!("Read failed: {}", e))?;
        if n == 0 {
            break;
        }
        carry.extend_from_slice(&buf[..n]);
        let usable = carry.len() - carry.len() % frame_bytes;
        let samples: Vec<f32> = carry[..usable].chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect();
        sc.feed(&samples);
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
    Ok(sc.finish())
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
    fn test_clean_tone_reports_nothing() {
        let mut s = Scanner::new(48000, 1);
        s.feed(&tone(48000, 2.0, 440.0, 0.7));
        let r = s.finish();
        assert_eq!(r.clipping.count, 0, "{:?}", r.clipping.events);
        assert_eq!(r.dropouts.count, 0);
        assert!((r.duration_secs - 2.0).abs() < 1e-6);
    }

    #[test]
    fn test_hard_clipping_is_found_and_merged() {
        // A tone driven 4× and clamped: every peak clips for many samples.
        let s: Vec<f32> = tone(48000, 1.0, 100.0, 4.0).iter().map(|v| v.clamp(-1.0, 1.0)).collect();
        let mut sc = Scanner::new(48000, 1);
        sc.feed(&s);
        let r = sc.finish();
        // 100 Hz → 200 half-cycles, each a clipped run; peaks 5 ms apart, so
        // nothing merges (merge window is 20 ms of GAP, and gaps are ~2.6 ms)…
        assert!(r.clipping.count >= 1, "{:?}", r.clipping.count);
        assert!(r.clipping.clipped_samples > 10_000, "{}", r.clipping.clipped_samples);
        assert!(r.clipping.events[0].kind == "hard");
        assert!(r.clipping.events[0].level_db > -0.1);
    }

    #[test]
    fn test_flat_top_below_full_scale_is_clipping() {
        // Clipped at source, then turned down 6 dB: flat tops at 0.5.
        let s: Vec<f32> = tone(48000, 0.5, 100.0, 4.0).iter().map(|v| v.clamp(-1.0, 1.0) * 0.5).collect();
        let mut sc = Scanner::new(48000, 1);
        sc.feed(&s);
        let r = sc.finish();
        assert!(r.clipping.count >= 1);
        assert_eq!(r.clipping.events[0].kind, "flat");
        assert!((r.clipping.events[0].level_db - (-6.02)).abs() < 0.2, "{}", r.clipping.events[0].level_db);
    }

    #[test]
    fn test_short_touch_of_full_scale_is_not_clipping() {
        // One or two samples at full scale is a legitimate peak.
        let mut s = tone(48000, 1.0, 440.0, 0.5);
        s[1000] = 1.0;
        s[2000] = 1.0; s[2001] = 1.0;
        let mut sc = Scanner::new(48000, 1);
        sc.feed(&s);
        assert_eq!(sc.finish().clipping.count, 0);
    }

    #[test]
    fn test_dropout_inside_programme_but_not_head_or_tail() {
        let rate = 48000;
        let mut s = tone(rate, 3.0, 440.0, 0.5);
        // Head silence 0.3 s, a 100 ms hole at 1.0 s, tail silence 0.4 s.
        let head = (rate as f64 * 0.3) as usize;
        s[..head].fill(0.0);
        s[rate as usize..rate as usize + 4800].fill(0.0);
        let n = s.len();
        s[n - (rate as f64 * 0.4) as usize..].fill(0.0);
        let mut sc = Scanner::new(rate, 1);
        sc.feed(&s);
        let r = sc.finish();
        assert_eq!(r.dropouts.count, 1, "{:?}", r.dropouts.gaps);
        assert!((r.dropouts.gaps[0].start - 1.0).abs() < 0.001);
        assert!((r.dropouts.gaps[0].duration - 0.1).abs() < 0.001);
        // A 20 ms hole is below the floor.
        let mut s2 = tone(rate, 1.0, 440.0, 0.5);
        s2[24000..24960].fill(0.0);
        let mut sc = Scanner::new(rate, 1);
        sc.feed(&s2);
        assert_eq!(sc.finish().dropouts.count, 0);
    }

    #[test]
    fn test_dropout_needs_every_channel_silent() {
        let rate = 48000;
        let mut s = vec![0f32; rate as usize * 2];
        // Stereo: left has a hole at 0.5 s, right keeps going.
        let l = tone(rate, 1.0, 440.0, 0.5);
        let r = tone(rate, 1.0, 330.0, 0.5);
        for i in 0..rate as usize {
            s[2 * i] = if (24000..28800).contains(&i) { 0.0 } else { l[i] };
            s[2 * i + 1] = r[i];
        }
        let mut sc = Scanner::new(rate, 2);
        sc.feed(&s);
        assert_eq!(sc.finish().dropouts.count, 0);
    }
}
