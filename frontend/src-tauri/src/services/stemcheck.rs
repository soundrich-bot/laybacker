//! Stem check: do the stems add up to the mix?
//!
//! A null test done by the computer. Each stem is lined up against the mix
//! (cross-correlation finds the offset and polarity), the stems are summed and
//! gain-matched to the mix, and what is left over — the residual — is measured
//! in half-second windows and written out as a file you can listen to.
//!
//! The report is a guide, not proof: a mix that went through a limiter or EQ
//! the stems didn't will never cancel completely. Anything flagged is for review.

use std::io::{BufWriter, Read, Seek, SeekFrom, Write};
use std::process::{Child, ChildStdout, Stdio};

use rustfft::num_complex::Complex;
use rustfft::FftPlanner;
use serde::Serialize;

use crate::services::ffmpeg;

/// Alignment works on a mono downmix at this rate: enough to place a stem
/// within a fraction of a millisecond, cheap enough to search ±10 s.
const ALIGN_RATE: u32 = 8000;
const MAX_LAG_SECS: f64 = 10.0;
const SEG_SECS: f64 = 10.0;
const REFINE_SECS: f64 = 5.0;
const REFINE_MARGIN_SECS: f64 = 0.006;
pub const WINDOW_SECS: f64 = 0.5;
/// A window of mix quieter than this is silence: nothing to compare against.
const SILENT_DB: f64 = -60.0;
/// Residual this far below the mix (or quieter) is a match.
const MATCH_DB: f64 = -40.0;
/// Up to here the stems are close — the sort of difference mastering leaves.
const CLOSE_DB: f64 = -20.0;
/// Likely-stem attribution needs at least this much correlation.
const ATTRIBUTE_CORR: f64 = 0.5;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StemAlign {
    pub name: String,
    pub path: String,
    /// Positive: the content sits later in the stem than in the mix (extra head).
    pub lag_ms: f64,
    /// Speed difference found along the file, in percent (+ = the stem runs
    /// long / slow against the mix). 0 when the two keep step.
    pub speed_pct: f64,
    pub polarity_flipped: bool,
    /// False when no confident alignment was found and the stem was used as is.
    pub aligned: bool,
    /// The level the mix contains this stem at, fitted jointly with the other
    /// stems: 0 dB = unity, None = not found in the mix at all.
    pub presence_db: Option<f64>,
    /// How much the mix resembles this stem where the stem is loudest, 0…1
    /// (the normalised correlation at the best offset and polarity). A stem
    /// that was simply summed gives 0.8 or more; one the mix bus has processed
    /// gives less; one that isn't in the mix at all gives next to nothing.
    pub resemblance: f64,
    /// Per window: the level (dB) at which the mix holds this stem, where the
    /// stem is active; None where the stem is quiet. Flat = steady; a dip is
    /// the stem fading or missing from the mix.
    pub track: Vec<Option<f64>>,
    pub note: Option<String>,
}

/// A stretch where a stem is in the stem file but not (or far less) in the mix.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MissingRegion {
    pub stem: String,
    pub start: f64,
    pub end: f64,
    /// How far below the stem's usual level in the mix it fell here.
    pub drop_db: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Region {
    pub start: f64,
    pub end: f64,
    /// Residual level relative to the mix over the region, in dB (higher = worse).
    pub residual_db: f64,
    /// The stem the residual most resembles, if any resembles it enough.
    pub likely: Option<String>,
    /// "stems_only": it is in the stems but not (or less) in the mix.
    /// "mix_only": the mix has it, the stems don't (or have less of it).
    pub direction: Option<String>,
    pub correlation: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StemReport {
    pub sample_rate: u32,
    pub channels: u32,
    pub duration_secs: f64,
    /// The typical (median) fitted stem gain — how the stems sit against the mix overall.
    pub gain_db: f64,
    /// Spread (10th–90th percentile) of the per-window gain. Big = the level
    /// relationship moves about, as it does when the mix is limited.
    pub gain_spread_db: f64,
    pub overall_residual_db: f64,
    /// Windows with a residual above this were flagged.
    pub flag_db: f64,
    /// "match" | "close" | "mismatch" | "processed"
    /// ("processed": every stem is in the mix, but the mix has been treated
    /// after the stems — limiting, EQ, phase rotation — so nothing cancels.)
    pub verdict: String,
    /// How much louder (RMS) the mix is than the fitted stem sum, and how
    /// much lower its peak: the fingerprint of limiting on the mix bus.
    pub mix_louder_db: f64,
    pub mix_peak_lower_db: f64,
    pub stems: Vec<StemAlign>,
    pub regions: Vec<Region>,
    /// Stretches where a stem is present in its file but drops out of the mix.
    pub missing: Vec<MissingRegion>,
    /// Residual level per window (dB relative to the mix); None where the mix is silent.
    pub windows: Vec<Option<f64>>,
    pub window_secs: f64,
    pub residual_path: String,
    pub notes: Vec<String>,
}

// ── decoding ────────────────────────────────────────────────────────────────

struct Decode<'a> {
    path: &'a str,
    filter: Option<String>,
    ss: Option<f64>,
    t: Option<f64>,
}

fn spawn(d: &Decode, rate: u32, channels: u32) -> Result<Child, String> {
    let mut cmd = ffmpeg::silent_command(&ffmpeg::find_ffmpeg());
    cmd.args(["-v", "error"]);
    if let Some(ss) = d.ss {
        cmd.args(["-ss", &format!("{:.6}", ss.max(0.0))]);
    }
    if let Some(t) = d.t {
        cmd.args(["-t", &format!("{:.6}", t.max(0.0))]);
    }
    cmd.args(["-i", d.path, "-vn"]);
    if let Some(f) = &d.filter {
        cmd.args(["-af", f]);
    }
    cmd.args([
        "-ac", &channels.to_string(),
        "-ar", &rate.to_string(),
        "-f", "f32le", "-",
    ])
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());
    cmd.spawn().map_err(|e| format!("Failed to run ffmpeg: {}", e))
}

fn finish(mut child: Child, what: &str) -> Result<(), String> {
    let status = child.wait().map_err(|e| format!("ffmpeg failed: {}", e))?;
    if !status.success() {
        let mut err = String::new();
        if let Some(mut e) = child.stderr.take() {
            let _ = e.read_to_string(&mut err);
        }
        return Err(format!("Could not decode {}: {}", what, err.trim()));
    }
    Ok(())
}

fn bytes_to_f32(bytes: &[u8]) -> Vec<f32> {
    bytes.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect()
}

/// Decode a whole (short) stretch into memory.
fn decode_all(d: &Decode, rate: u32, channels: u32) -> Result<Vec<f32>, String> {
    let mut child = spawn(d, rate, channels)?;
    let mut out = child.stdout.take().ok_or("no ffmpeg output")?;
    let mut bytes = Vec::new();
    out.read_to_end(&mut bytes).map_err(|e| format!("Read failed: {}", e))?;
    finish(child, d.path)?;
    let usable = bytes.len() - bytes.len() % 4;
    Ok(bytes_to_f32(&bytes[..usable]))
}

/// Decode a whole file at `rate`/`channels` through the standard chain
/// (interleaved f32). For diagnostics and tests; memory grows with the file.
pub fn decode_whole(path: &str, rate: u32, channels: u32) -> Result<Vec<f32>, String> {
    decode_all(&Decode { path, filter: Some(base_filter(rate, channels)), ss: None, t: None }, rate, channels)
}

/// Fill `buf` from a pipe, looping until it is full or the stream ends.
/// Returns how many bytes were read; the rest is zeroed.
fn fill(reader: &mut ChildStdout, buf: &mut [u8]) -> Result<usize, String> {
    let mut got = 0;
    while got < buf.len() {
        let n = reader.read(&mut buf[got..]).map_err(|e| format!("Read failed: {}", e))?;
        if n == 0 {
            break;
        }
        got += n;
    }
    for b in &mut buf[got..] {
        *b = 0;
    }
    Ok(got)
}

/// Stream the mix and every stem in lock-step, one window at a time.
/// The callback gets the mix window and one equally long slice per stem
/// (interleaved frames). Stops when the mix ends; short stems read as silence.
fn stream_windows<F>(
    mix: &Decode,
    stems: &[Decode],
    rate: u32,
    channels: u32,
    mut on_window: F,
) -> Result<usize, String>
where
    F: FnMut(&[f32], &[Vec<f32>]),
{
    let frames = (rate as f64 * WINDOW_SECS).round().max(1.0) as usize;
    let bytes_per_window = frames * channels as usize * 4;
    let mut mix_child = spawn(mix, rate, channels)?;
    let mut mix_out = mix_child.stdout.take().ok_or("no ffmpeg output")?;
    let mut stem_children = Vec::new();
    let mut stem_outs = Vec::new();
    for s in stems {
        let mut c = spawn(s, rate, channels)?;
        stem_outs.push(c.stdout.take().ok_or("no ffmpeg output")?);
        stem_children.push(c);
    }
    let mut mix_buf = vec![0u8; bytes_per_window];
    let mut stem_buf = vec![0u8; bytes_per_window];
    let mut stem_samples: Vec<Vec<f32>> = vec![Vec::new(); stems.len()];
    let mut total_frames = 0usize;
    loop {
        let got = fill(&mut mix_out, &mut mix_buf)?;
        if got == 0 {
            break;
        }
        let got_frames = got / (channels as usize * 4);
        for (i, out) in stem_outs.iter_mut().enumerate() {
            fill(out, &mut stem_buf)?;
            stem_samples[i] = bytes_to_f32(&stem_buf);
        }
        let mix_samples = bytes_to_f32(&mix_buf);
        on_window(&mix_samples[..got_frames * channels as usize], &stem_samples);
        total_frames += got_frames;
        if got < bytes_per_window {
            break;
        }
    }
    // Drain stems that run longer than the mix so ffmpeg exits cleanly.
    for out in stem_outs.iter_mut() {
        let mut sink = Vec::new();
        let _ = out.read_to_end(&mut sink);
    }
    finish(mix_child, mix.path)?;
    for (c, s) in stem_children.into_iter().zip(stems) {
        finish(c, s.path)?;
    }
    Ok(total_frames)
}

// ── alignment ───────────────────────────────────────────────────────────────

fn db(ratio: f64) -> f64 {
    if ratio <= 0.0 { f64::NEG_INFINITY } else { 10.0 * ratio.log10() }
}

fn rms_db(x: &[f32]) -> f64 {
    if x.is_empty() {
        return f64::NEG_INFINITY;
    }
    let e: f64 = x.iter().map(|&v| (v as f64) * (v as f64)).sum();
    db(e / x.len() as f64)
}

/// Normalised cross-correlation of `stem` against every position in `mix`
/// (mix must be at least as long). Returns (best offset k, signed peak,
/// peakiness = |peak| / rms of the correlation).
pub fn correlate(stem: &[f32], mix: &[f32]) -> Option<(usize, f64, f64)> {
    let ns = stem.len();
    let nm = mix.len();
    if ns == 0 || nm < ns {
        return None;
    }
    let n = (nm + ns).next_power_of_two();
    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(n);
    let ifft = planner.plan_fft_inverse(n);
    let mut s: Vec<Complex<f32>> = stem.iter().map(|&v| Complex::new(v, 0.0)).collect();
    s.resize(n, Complex::new(0.0, 0.0));
    let mut m: Vec<Complex<f32>> = mix.iter().map(|&v| Complex::new(v, 0.0)).collect();
    m.resize(n, Complex::new(0.0, 0.0));
    fft.process(&mut s);
    fft.process(&mut m);
    for (a, b) in m.iter_mut().zip(&s) {
        *a *= b.conj();
    }
    ifft.process(&mut m);
    let scale = 1.0 / n as f64;
    let stem_norm = stem.iter().map(|&v| (v as f64) * (v as f64)).sum::<f64>().sqrt();
    if stem_norm == 0.0 {
        return None;
    }
    // Sliding norm of the mix under the stem, from a prefix sum of squares.
    let mut prefix = vec![0f64; nm + 1];
    for (i, &v) in mix.iter().enumerate() {
        prefix[i + 1] = prefix[i] + (v as f64) * (v as f64);
    }
    let valid = nm - ns + 1;
    let mut best_k = 0usize;
    let mut best = 0f64;
    let mut sumsq = 0f64;
    for k in 0..valid {
        let mix_norm = (prefix[k + ns] - prefix[k]).sqrt();
        if mix_norm == 0.0 {
            continue;
        }
        let c = m[k].re as f64 * scale / (stem_norm * mix_norm);
        sumsq += c * c;
        if c.abs() > best.abs() {
            best = c;
            best_k = k;
        }
    }
    let rms = (sumsq / valid as f64).sqrt();
    let peakiness = if rms > 0.0 { best.abs() / rms } else { 0.0 };
    Some((best_k, best, peakiness))
}

/// Where does `stem` sit in `mix`? GCC-PHAT: the cross-spectrum is whitened
/// before the inverse transform, so music and speech (which correlate with
/// themselves for tens of milliseconds) still give one sharp peak instead of a
/// broad hump. Returns (offset k, peakiness = peak / rms of the function).
pub fn correlate_phat(stem: &[f32], mix: &[f32]) -> Option<(usize, f64)> {
    let ns = stem.len();
    let nm = mix.len();
    if ns == 0 || nm < ns {
        return None;
    }
    let n = (nm + ns).next_power_of_two();
    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(n);
    let ifft = planner.plan_fft_inverse(n);
    let mut s: Vec<Complex<f32>> = stem.iter().map(|&v| Complex::new(v, 0.0)).collect();
    s.resize(n, Complex::new(0.0, 0.0));
    let mut m: Vec<Complex<f32>> = mix.iter().map(|&v| Complex::new(v, 0.0)).collect();
    m.resize(n, Complex::new(0.0, 0.0));
    fft.process(&mut s);
    fft.process(&mut m);
    for (a, b) in m.iter_mut().zip(&s) {
        let x = *a * b.conj();
        let mag = x.norm();
        *a = if mag > 1e-12 { x / mag } else { Complex::new(0.0, 0.0) };
    }
    ifft.process(&mut m);
    let valid = nm - ns + 1;
    let mut best_k = 0usize;
    let mut best = f64::NEG_INFINITY;
    let mut sumsq = 0f64;
    for (k, v) in m.iter().enumerate().take(valid) {
        let c = v.re as f64;
        sumsq += c * c;
        if c.abs() > best {
            best = c.abs();
            best_k = k;
        }
    }
    let rms = (sumsq / valid as f64).sqrt();
    Some((best_k, if rms > 0.0 { best / rms } else { 0.0 }))
}

/// Normalised correlation of `stem` against `mix` at one offset (sign = polarity).
fn ncc_at(stem: &[f32], mix: &[f32], k: usize) -> f64 {
    let n = stem.len().min(mix.len().saturating_sub(k));
    if n == 0 {
        return 0.0;
    }
    let mut xy = 0f64;
    let mut xx = 0f64;
    let mut yy = 0f64;
    for i in 0..n {
        let a = stem[i] as f64;
        let b = mix[i + k] as f64;
        xy += a * b;
        xx += a * a;
        yy += b * b;
    }
    if xx > 0.0 && yy > 0.0 { xy / (xx * yy).sqrt() } else { 0.0 }
}

struct Alignment {
    /// Offset at the start of the file (stem time − mix time).
    lag_secs: f64,
    /// |ncc| at full rate at the best offset and polarity (0 when unaligned).
    resemblance: f64,
    /// d(lag)/d(position): the stem runs long by this fraction.
    drift: f64,
    flipped: bool,
    aligned: bool,
    note: Option<String>,
}

/// The filter that plays a stem faster by `drift` (a pull-up/pull-down fix).
fn speed_filter(rate: u32, drift: f64) -> String {
    let fast = (rate as f64 * (1.0 + drift)).round() as u32;
    format!("asetrate={},aresample={}", fast, rate)
}

/// Line one stem up with the mix. `pre` is a speed correction already known
/// (from a first pass): the stem is decoded with it applied, so this pass
/// measures what is left — a second pass pins a drift down precisely once the
/// first has taken the bulk of it out.
fn align_stem(mix_path: &str, stem_path: &str, mix_dur: f64, stem_dur_raw: f64, rate: u32, pre: f64) -> Result<Alignment, String> {
    let none = |note: &str| Alignment { lag_secs: 0.0, resemblance: 0.0, drift: 0.0, flipped: false, aligned: false, note: Some(note.to_string()) };
    let stem_dur = stem_dur_raw / (1.0 + pre);
    // Seeks are in the file's own time; the corrected stem runs faster.
    let seek = |pos: f64| pos * (1.0 + pre);
    // The speed filter must be built for the rate the chain is running at.
    let stem_filter = |r: u32| if pre != 0.0 { format!("{},{}", base_filter(r, 1), speed_filter(r, pre)) } else { base_filter(r, 1) };
    // With a speed filter in the chain, `-t` cuts against rescaled timestamps
    // and comes up short; cutting inside the chain is exact either way.
    let stem_segment = |pos: f64, len: f64, r: u32| -> Decode<'_> {
        Decode {
            path: stem_path,
            filter: Some(format!("{},atrim=end_sample={}", stem_filter(r), (len * r as f64).round() as u64)),
            ss: Some(seek(pos)),
            t: None,
        }
    };
    let span = mix_dur.min(stem_dur);
    if span <= 0.0 {
        return Ok(none("Empty file — compared as is."));
    }
    // Several positions along the file: the offset should agree everywhere.
    // If it climbs steadily, the two run at different speeds (a pull-up or
    // pull-down), and that is corrected too.
    let fractions: &[f64] = if span < 2.0 * SEG_SECS { &[0.0, 0.33, 0.66] } else { &[0.05, 0.2, 0.35, 0.5, 0.65, 0.8, 0.95] };
    let seg = SEG_SECS.min(stem_dur).min((span / 4.0).max(0.5));
    let mono8 = Some(base_filter(ALIGN_RATE, 1));
    // (position, lag, confidence, flipped)
    let mut found: Vec<(f64, f64, f64, bool)> = Vec::new();
    // A long segment sees more of a quiet stem; a short one survives a speed
    // difference (over 7 s, a 0.1 % drift smears the match by 7 ms). Try the
    // long one first and fall back to the short one.
    let short = 1.5f64.min(seg);
    let lengths: Vec<f64> = if (seg - short).abs() < 0.01 { vec![seg] } else { vec![seg, short] };
    let mut positions: Vec<f64> = fractions.iter().map(|&frac| (frac * span).min((stem_dur - seg).max(0.0))).collect();
    positions.dedup_by(|a, b| (*a - *b).abs() < 0.01);
    for pos in positions {
        for &seg_len in &lengths {
            let stem_seg = decode_all(&stem_segment(pos, seg_len, ALIGN_RATE), ALIGN_RATE, 1)?;
            if stem_seg.len() < ALIGN_RATE as usize / 2 || rms_db(&stem_seg) < SILENT_DB {
                if std::env::var("STEM_DEBUG").is_ok() { eprintln!("ALIGN skip stem seg pos {:.2} seg {:.2}: len {} rms {:.1}", pos, seg_len, stem_seg.len(), rms_db(&stem_seg)); }
                continue;
            }
            let start = (pos - MAX_LAG_SECS).max(0.0);
            let len = (seg_len + 2.0 * MAX_LAG_SECS).min((mix_dur - start).max(0.0));
            let mix_seg = decode_all(&Decode { path: mix_path, filter: mono8.clone(), ss: Some(start), t: Some(len) }, ALIGN_RATE, 1)?;
            if mix_seg.len() < stem_seg.len() {
                if std::env::var("STEM_DEBUG").is_ok() { eprintln!("ALIGN skip mix seg pos {:.2} seg {:.2}: mix {} < stem {}", pos, seg_len, mix_seg.len(), stem_seg.len()); }
                continue;
            }
            let Some((k, peakiness)) = correlate_phat(&stem_seg, &mix_seg) else { continue };
            let ncc = ncc_at(&stem_seg, &mix_seg, k);
            let room = (mix_seg.len() - stem_seg.len()) as f64 / ALIGN_RATE as f64;
            // A sharp PHAT peak that the plain correlation agrees with (same
            // offset, some actual resemblance). With little room to slide,
            // peakiness means nothing, so lean on the resemblance instead.
            // Unrelated material gives |ncc| around 0.01, so a plain
            // correlation of 0.15 with a reasonably sharp peak is a real lock.
            let confident = if room >= 1.0 {
                (peakiness >= 8.0 && ncc.abs() >= 0.02) || (peakiness >= 5.0 && ncc.abs() >= 0.15)
            } else {
                ncc.abs() >= 0.3
            };
            if std::env::var("STEM_DEBUG").is_ok() { eprintln!("ALIGN {} pos {:.2} seg {:.2} stemlen {} mixlen {} k {} lag {:.4} peak {:.1} ncc {:.3} room {:.1}", stem_path.rsplit('/').next().unwrap_or(""), pos, seg_len, stem_seg.len(), mix_seg.len(), k, (pos - start) - k as f64 / ALIGN_RATE as f64, peakiness, ncc, room); }
            if confident {
                let lag = (pos - start) - k as f64 / ALIGN_RATE as f64;
                found.push((pos, lag, peakiness * ncc.abs(), ncc < 0.0));
                break;
            }
        }
    }
    if found.is_empty() {
        return Ok(none("Couldn't find where this lines up with the mix (too quiet, too different, or offset by more than 10 s) — compared as is."));
    }
    // Polarity is decided at full rate in the refinement below, where both
    // signs are tried: the band-limited coarse pass can disagree with the
    // full band when the mix bus has shifted phase.

    // Fit lag = a + b·pos, dropping anything more than 3 ms off the line
    // (a mis-locked segment), then decide whether the slope is real.
    let mut pts: Vec<(f64, f64, f64)> = found.iter().map(|f| (f.0, f.1, f.2)).collect();
    let fit = |pts: &[(f64, f64, f64)]| -> (f64, f64) {
        if pts.len() < 2 {
            return (pts.first().map(|p| p.1).unwrap_or(0.0), 0.0);
        }
        let w: f64 = pts.iter().map(|p| p.2).sum();
        let mx = pts.iter().map(|p| p.0 * p.2).sum::<f64>() / w;
        let my = pts.iter().map(|p| p.1 * p.2).sum::<f64>() / w;
        let sxx: f64 = pts.iter().map(|p| p.2 * (p.0 - mx) * (p.0 - mx)).sum();
        let sxy: f64 = pts.iter().map(|p| p.2 * (p.0 - mx) * (p.1 - my)).sum();
        let b = if sxx > 0.0 { sxy / sxx } else { 0.0 };
        (my - b * mx, b)
    };
    let mut note = None;
    let (mut a, mut b) = fit(&pts);
    for _ in 0..2 {
        let keep: Vec<(f64, f64, f64)> = pts.iter().copied().filter(|p| (p.1 - (a + b * p.0)).abs() <= 0.003).collect();
        if keep.len() >= 2 && keep.len() < pts.len() {
            pts = keep;
            let f = fit(&pts);
            a = f.0;
            b = f.1;
        } else if keep.len() < 2 {
            // No line fits: trust the strongest single match.
            let best = found.iter().max_by(|x, y| x.2.partial_cmp(&y.2).unwrap_or(std::cmp::Ordering::Equal)).unwrap();
            pts = vec![(best.0, best.1, best.2)];
            a = best.1;
            b = 0.0;
            note = Some("The offset differs along the file (an edit in one but not the other?) — aligned at the strongest match.".into());
            break;
        } else {
            break;
        }
    }
    // Refine at the mix's own rate: a short stretch, searched ±2 ms around
    // the coarse offset. Done at both ends of the file, the two refined
    // offsets give the speed difference far more precisely than the coarse
    // line can.
    // Returns (position the result refers to, refined lag there, signed ncc).
    // Under drift the window is kept short (0.1 ms of smear at most) and the
    // result is referred to the window's centre, so the smear doesn't bias it.
    let refine = |pos: f64, lag_at: &dyn Fn(f64) -> f64, drift_now: f64| -> Result<(f64, f64, f64), String> {
        let want = if drift_now != 0.0 { (0.0001 / drift_now.abs()).clamp(0.1, REFINE_SECS) } else { REFINE_SECS };
        let seg_f = want.min(stem_dur - pos).max(0.1);
        let centre = pos + seg_f / 2.0;
        let coarse_lag = lag_at(centre);
        let mono = Some(base_filter(rate, 1));
        let stem_fine = decode_all(&stem_segment(pos, seg_f, rate), rate, 1)?;
        let mix_start = pos - coarse_lag - REFINE_MARGIN_SECS;
        let shift = if mix_start < 0.0 { -mix_start } else { 0.0 };
        let mix_fine = decode_all(
            &Decode { path: mix_path, filter: mono, ss: Some(mix_start + shift), t: Some(seg_f + 2.0 * REFINE_MARGIN_SECS) },
            rate,
            1,
        )?;
        let margin = (REFINE_MARGIN_SECS * rate as f64).round() as usize;
        if mix_fine.len() < stem_fine.len() || stem_fine.is_empty() {
            return Ok((centre, coarse_lag, 0.0));
        }
        let n = stem_fine.len().min(mix_fine.len().saturating_sub(2 * margin));
        // Either polarity: the strongest |correlation| wins.
        let mut best_c = 0f64;
        let mut best_k = margin;
        let max_k = mix_fine.len() - n;
        for k in 0..=max_k.min(2 * margin) {
            let mut c = 0f64;
            for i in 0..n {
                c += stem_fine[i] as f64 * mix_fine[i + k] as f64;
            }
            if c.abs() > best_c.abs() {
                best_c = c;
                best_k = k;
            }
        }
        let ncc = ncc_at(&stem_fine[..n], &mix_fine, best_k);
        Ok((centre, pos - (mix_start + shift) - best_k as f64 / rate as f64, ncc))
    };

    let span_fit = pts.iter().map(|p| p.0).fold(0.0f64, f64::max) - pts.iter().map(|p| p.0).fold(f64::MAX, f64::min);
    let coarse_drift = if pts.len() >= 3 && span_fit > 5.0 && b.abs() >= 2e-5 { b } else { 0.0 };
    let line = |t: f64| a + b * t;
    // The strongest coarse match is where the offset, polarity and resemblance are read.
    let best = pts.iter().max_by(|x, y| x.2.partial_cmp(&y.2).unwrap_or(std::cmp::Ordering::Equal)).unwrap();
    let (p_best, lag_best, ncc_best) = refine(best.0, &line, coarse_drift)?;
    let flipped = ncc_best < 0.0;
    let resemblance = ncc_best.abs();
    // Speed: only when the coarse line already says so, and the refined
    // offsets at both ends agree with it (same sign, within a factor of two).
    // One mis-locked refinement must not invent a pull-down.
    let mut drift = 0.0;
    let mut lag0 = lag_best - coarse_drift * p_best;
    if coarse_drift != 0.0 {
        let first = pts.iter().min_by(|x, y| x.0.partial_cmp(&y.0).unwrap()).unwrap();
        let last = pts.iter().max_by(|x, y| x.0.partial_cmp(&y.0).unwrap()).unwrap();
        let (p_first, lag_first, _) = refine(first.0, &line, coarse_drift)?;
        let (p_last, lag_last, _) = refine(last.0, &line, coarse_drift)?;
        let slope = (lag_last - lag_first) / (p_last - p_first);
        let agrees = slope.signum() == coarse_drift.signum() && (0.5..=2.0).contains(&(slope / coarse_drift));
        if slope.abs() >= 2e-5 && agrees {
            drift = slope;
            lag0 = lag_first - slope * p_first;
        } else {
            lag0 = lag_best;
        }
    }
    if std::env::var("STEM_DEBUG").is_ok() {
        eprintln!("ALIGN-RESULT pre {:.6} pts {:?} coarse a {:.5} b {:.7} coarse_drift {:.7} best {:.5}@{:.2} ncc {:+.3} -> lag0 {:.5} drift {:.7}", pre, pts.iter().map(|p| (p.0, (p.1 * 1000.0).round() / 1000.0)).collect::<Vec<_>>(), a, b, coarse_drift, lag_best, p_best, ncc_best, lag0, drift);
    }
    Ok(Alignment { lag_secs: lag0, resemblance, drift, flipped, aligned: true, note })
}

/// Every decode opens with the same explicit conversion to the working rate,
/// layout and float format. Left to ffmpeg's automatic conversion, a stereo
/// downmix comes out 3 dB different depending on which filters follow it, and
/// a null test cannot live with that.
pub fn base_filter(rate: u32, channels: u32) -> String {
    format!("aresample=osr={}:ochl={}:osf=flt", rate, crate::services::channels::layout_for_count(channels.max(1)))
}

/// The ffmpeg filter that puts a stem in time (and polarity) with the mix.
pub fn align_filter(lag_secs: f64, drift: f64, flipped: bool, rate: u32, channels: u32) -> String {
    let samples = (lag_secs * rate as f64).round() as i64;
    let mut parts = vec![base_filter(rate, channels)];
    if drift != 0.0 {
        // The stem runs long by `drift`: play it faster by the same fraction
        // (relabel the rate, then resample back), which also fixes its pitch.
        parts.push(speed_filter(rate, drift));
    }
    if samples > 0 {
        parts.push(format!("atrim=start_sample={}", samples));
    } else if samples < 0 {
        parts.push(format!("adelay={}S:all=1", -samples));
    }
    if flipped {
        parts.push("volume=-1".to_string());
    }
    parts.join(",")
}

// ── residual file ───────────────────────────────────────────────────────────

pub struct WavWriter {
    out: BufWriter<std::fs::File>,
    frames: u64,
    channels: u16,
    rate: u32,
}

impl WavWriter {
    pub fn create(path: &str, rate: u32, channels: u32) -> Result<Self, String> {
        let f = std::fs::File::create(path).map_err(|e| format!("Could not write residual: {}", e))?;
        let mut w = WavWriter { out: BufWriter::new(f), frames: 0, channels: channels as u16, rate };
        w.header(0)?;
        Ok(w)
    }
    fn header(&mut self, data_bytes: u32) -> Result<(), String> {
        let ch = self.channels as u32;
        let block = ch * 3;
        let mut h = Vec::with_capacity(44);
        h.extend_from_slice(b"RIFF");
        h.extend_from_slice(&(36 + data_bytes).to_le_bytes());
        h.extend_from_slice(b"WAVEfmt ");
        h.extend_from_slice(&16u32.to_le_bytes());
        h.extend_from_slice(&1u16.to_le_bytes()); // PCM
        h.extend_from_slice(&self.channels.to_le_bytes());
        h.extend_from_slice(&self.rate.to_le_bytes());
        h.extend_from_slice(&(self.rate * block).to_le_bytes());
        h.extend_from_slice(&(block as u16).to_le_bytes());
        h.extend_from_slice(&24u16.to_le_bytes());
        h.extend_from_slice(b"data");
        h.extend_from_slice(&data_bytes.to_le_bytes());
        self.out.write_all(&h).map_err(|e| format!("Could not write residual: {}", e))
    }
    pub fn write(&mut self, samples: &[f32]) -> Result<(), String> {
        let mut bytes = Vec::with_capacity(samples.len() * 3);
        for &v in samples {
            let s = (v.clamp(-1.0, 1.0) as f64 * 8_388_607.0).round() as i32;
            let b = s.to_le_bytes();
            bytes.extend_from_slice(&b[..3]);
        }
        self.out.write_all(&bytes).map_err(|e| format!("Could not write residual: {}", e))?;
        self.frames += (samples.len() / self.channels as usize) as u64;
        Ok(())
    }
    pub fn close(mut self) -> Result<(), String> {
        let data_bytes = (self.frames * self.channels as u64 * 3).min(u32::MAX as u64) as u32;
        self.out.flush().map_err(|e| format!("Could not write residual: {}", e))?;
        self.out.seek(SeekFrom::Start(0)).map_err(|e| format!("Could not write residual: {}", e))?;
        self.header(data_bytes)?;
        self.out.flush().map_err(|e| format!("Could not write residual: {}", e))
    }
}

// ── the check ───────────────────────────────────────────────────────────────

pub struct StemFile {
    pub path: String,
    pub name: String,
    pub duration_secs: f64,
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = ((sorted.len() - 1) as f64 * p).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

/// Run the whole check. `progress(stage, fraction)` is called as it goes.
pub fn stem_check(
    mix: &StemFile,
    stems: &[StemFile],
    sample_rate: u32,
    channels: u32,
    residual_path: &str,
    progress: &dyn Fn(&str, f64),
) -> Result<StemReport, String> {
    if stems.is_empty() {
        return Err("Pick at least one stem to compare against the mix.".into());
    }
    let rate = if sample_rate == 0 { 48000 } else { sample_rate };
    let ch = channels.max(1);
    let mut notes = Vec::new();

    // 1. Line each stem up with the mix.
    let mut aligns = Vec::new();
    for (i, s) in stems.iter().enumerate() {
        progress("align", i as f64 / stems.len() as f64);
        let mut a = align_stem(&mix.path, &s.path, mix.duration_secs, s.duration_secs, rate, 0.0)?;
        if a.aligned && a.drift != 0.0 {
            // Take the bulk of the speed difference out and measure again:
            // the second pass sees un-smeared matches and pins it down.
            let d1 = a.drift;
            let second = align_stem(&mix.path, &s.path, mix.duration_secs, s.duration_secs, rate, d1)?;
            if second.aligned {
                let drift = (1.0 + d1) * (1.0 + second.drift) - 1.0;
                a = Alignment { lag_secs: second.lag_secs, resemblance: second.resemblance, drift, flipped: second.flipped, aligned: true, note: second.note.or(a.note) };
            }
            let d = a.drift;
            let speed_note = format!(
                "Runs {:.3} % {} than the mix (a pull-up or pull-down?) — speed-corrected for the comparison.",
                d.abs() * 100.0,
                if d > 0.0 { "slower" } else { "faster" }
            );
            a.note = Some(match a.note.take() { Some(n) => format!("{} {}", speed_note, n), None => speed_note });
        }
        let diff = s.duration_secs - mix.duration_secs;
        if diff.abs() > 0.5 {
            notes.push(format!(
                "{} is {:.1} s {} than the mix; compared over the mix's length.",
                s.name,
                diff.abs(),
                if diff > 0.0 { "longer" } else { "shorter" }
            ));
        }
        aligns.push(a);
    }
    let stem_decodes: Vec<Decode> = stems
        .iter()
        .zip(&aligns)
        .map(|(s, a)| Decode { path: &s.path, filter: Some(align_filter(a.lag_secs, a.drift, a.flipped, rate, ch)), ss: None, t: None })
        .collect();
    let mix_decode = Decode { path: &mix.path, filter: Some(base_filter(rate, ch)), ss: None, t: None };

    // 2. The level the mix holds each stem at, fitted jointly (least squares
    //    on the whole file). Stems don't always arrive at the level they sit
    //    in the mix — a music stem printed at unity but mixed 8 dB down is
    //    still the right stem — so each gets its own gain, and the report
    //    says what it is. Equal-level stems fit at unity.
    progress("gain", 0.0);
    let n = stems.len();
    let total_frames_est = (mix.duration_secs * rate as f64).max(1.0);
    let mut sts = vec![vec![0f64; n]; n]; // Σ stem_i·stem_j
    let mut stm = vec![0f64; n]; // Σ stem_i·mix
    let mut done = 0f64;
    stream_windows(&mix_decode, &stem_decodes, rate, ch, |m, s| {
        for (i, &mv) in m.iter().enumerate() {
            let mut v = [0f64; 16];
            for (j, st) in s.iter().enumerate().take(16) {
                v[j] = st.get(i).copied().unwrap_or(0.0) as f64;
            }
            for j in 0..n.min(16) {
                stm[j] += v[j] * mv as f64;
                for k in j..n.min(16) {
                    sts[j][k] += v[j] * v[k];
                }
            }
        }
        done += (m.len() / ch as usize) as f64;
        progress("gain", (done / total_frames_est).min(1.0));
    })?;
    if n > 16 {
        return Err("Up to 16 stems at a time, please.".into());
    }
    // Mirror the upper triangle.
    #[allow(clippy::needless_range_loop)]
    for j in 0..n {
        for k in 0..j {
            let v = sts[k][j];
            sts[j][k] = v;
        }
    }
    if (0..n).all(|j| sts[j][j] <= 0.0) {
        return Err("The stems are silent — nothing to compare.".into());
    }
    let mut gains = solve_gains(&sts, &stm);
    let mut absent: Vec<usize> = Vec::new();
    for (j, g) in gains.iter_mut().enumerate() {
        if sts[j][j] <= 0.0 || *g <= 0.001 {
            // No positive share of the mix: the mix doesn't contain this stem.
            *g = 0.0;
            absent.push(j);
        } else if !(0.1..=10.0).contains(g) {
            notes.push(format!(
                "{} fits the mix at {:+.1} dB — an unusual amount; held to ±20 dB.",
                stems[j].name,
                20.0 * g.log10()
            ));
            *g = g.clamp(0.1, 10.0);
        }
    }
    let mut present_db: Vec<f64> = gains.iter().filter(|g| **g > 0.0).map(|g| 20.0 * g.log10()).collect();
    present_db.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let gain_db = if present_db.is_empty() { 0.0 } else { percentile(&present_db, 0.5) };
    let gain = 10f64.powf(gain_db / 20.0);
    // Stems the mix holds at a very different level from the rest are worth a
    // look even when the residual nulls: a level mistake, or the wrong stem.
    let mut level_flags = 0usize;
    for (j, g) in gains.iter().enumerate() {
        if *g > 0.0 {
            let d = 20.0 * g.log10() - gain_db;
            if d.abs() > 6.0 {
                level_flags += 1;
                notes.push(format!(
                    "{} sits {:.1} dB {} in the mix than the other stems — check its level.",
                    stems[j].name,
                    d.abs(),
                    if d < 0.0 { "lower" } else { "higher" }
                ));
            }
        }
    }
    for &j in &absent {
        notes.push(format!("{} doesn't appear in the mix at all.", stems[j].name));
    }

    // 3. Residual pass at full quality: measure per window, write the file.
    progress("compare", 0.0);
    let mut wav = WavWriter::create(residual_path, rate, ch)?;
    let mut windows: Vec<Option<f64>> = Vec::new();
    // Per window, for attribution: Σres·stem_i, Σstem_i².
    let mut win_corr: Vec<Vec<(f64, f64)>> = Vec::new();
    let mut win_res: Vec<(f64, f64)> = Vec::new(); // (E_res, E_mix)
    let mut win_gains: Vec<f64> = Vec::new();
    let mut tot_mix = 0f64;
    let mut tot_res = 0f64;
    let mut stem_xy = vec![0f64; n];
    let mut stem_xx = vec![0f64; n];
    // Per stem per window: the level the mix holds it at (where it is active).
    let mut tracks: Vec<Vec<Option<f64>>> = vec![Vec::new(); n];
    let mut mix_peak = 0f64;
    let mut sum_peak = 0f64;
    let mut tot_sum = 0f64;
    let frames_est = (mix.duration_secs * rate as f64).max(1.0);
    let mut frames_done = 0f64;
    let mut res_buf: Vec<f32> = Vec::new();
    let mut wav_err: Option<String> = None;
    stream_windows(&mix_decode, &stem_decodes, rate, ch, |m, s| {
        let len = m.len();
        res_buf.clear();
        res_buf.reserve(len);
        let mut e_mix = 0f64;
        let mut e_res = 0f64;
        let mut e_sum = 0f64;
        let mut xy_sum = 0f64;
        let mut corr = vec![(0f64, 0f64); n];
        let mut w_xy = vec![0f64; n];
        let mut w_xx = vec![0f64; n];
        for (i, &mix_v) in m.iter().enumerate() {
            let mv = mix_v as f64;
            // The fitted sum: each stem at the level the mix holds it.
            let mut sum = 0f64;
            for (j, st) in s.iter().enumerate() {
                let v = st.get(i).copied().unwrap_or(0.0) as f64;
                sum += gains[j] * v;
                stem_xy[j] += mv * v;
                stem_xx[j] += v * v;
                w_xy[j] += mv * v;
                w_xx[j] += v * v;
            }
            let res = mv - sum;
            res_buf.push(res as f32);
            e_mix += mv * mv;
            e_res += res * res;
            e_sum += sum * sum;
            xy_sum += mv * sum;
            mix_peak = mix_peak.max(mv.abs());
            sum_peak = sum_peak.max(sum.abs());
            for (j, st) in s.iter().enumerate() {
                let v = st.get(i).copied().unwrap_or(0.0) as f64;
                corr[j].0 += res * v;
                corr[j].1 += v * v;
            }
        }
        let frames = len / ch as usize;
        let mix_level = if frames > 0 { db(e_mix / len.max(1) as f64) } else { f64::NEG_INFINITY };
        if mix_level > SILENT_DB {
            windows.push(Some(db(e_res / e_mix)));
            tot_mix += e_mix;
            tot_res += e_res;
            if e_sum > 0.0 && mix_level > -40.0 {
                let g_w = xy_sum / e_sum;
                if g_w > 0.0 {
                    win_gains.push(20.0 * g_w.log10());
                }
            }
        } else {
            windows.push(None);
        }
        win_res.push((e_res, e_mix));
        win_corr.push(corr);
        tot_sum += e_sum;
        for j in 0..n {
            // Active = the stem has signal here; the level is its LS gain
            // against the mix (floored at −60 dB when the mix has none of it).
            let active = len > 0 && db(w_xx[j] / len as f64) > -50.0;
            tracks[j].push(if active {
                let g = w_xy[j] / w_xx[j];
                Some(if g > 1e-3 { 20.0 * g.log10() } else { -60.0 })
            } else {
                None
            });
        }
        if wav_err.is_none() {
            if let Err(e) = wav.write(&res_buf) {
                wav_err = Some(e);
            }
        }
        frames_done += frames as f64;
        progress("compare", (frames_done / frames_est).min(1.0));
    })?;
    if let Some(e) = wav_err {
        return Err(e);
    }
    wav.close()?;
    let duration_secs = windows.len() as f64 * WINDOW_SECS;
    if tot_mix <= 0.0 {
        return Err("The mix is silent — nothing to compare.".into());
    }
    let overall = db(tot_res / tot_mix);
    let mix_louder_db = if tot_sum > 0.0 { db(tot_mix / tot_sum) } else { 0.0 };
    let mix_peak_lower_db = if mix_peak > 0.0 && sum_peak > 0.0 { 20.0 * (sum_peak / mix_peak).log10() } else { 0.0 };

    // Per stem: where it sits, and at what level the mix holds it.
    let _ = (&stem_xy, &stem_xx); // kept for the window stats; the level comes from the joint fit
    let mut stem_reports = Vec::new();
    for (i, (s, a)) in stems.iter().zip(&aligns).enumerate() {
        let presence = if gains[i] > 0.0 { Some(20.0 * gains[i].log10()) } else { None };
        stem_reports.push(StemAlign {
            name: s.name.clone(),
            path: s.path.clone(),
            lag_ms: a.lag_secs * 1000.0,
            speed_pct: a.drift * 100.0,
            polarity_flipped: a.flipped,
            aligned: a.aligned,
            presence_db: presence,
            resemblance: a.resemblance,
            track: tracks[i].clone(),
            note: a.note.clone(),
        });
    }

    // Stretches where a stem is in its file but the mix has lost it: its
    // level track falls 12 dB or more below its own usual level. This holds
    // up on a processed mix, where the residual doesn't.
    let mut missing: Vec<MissingRegion> = Vec::new();
    for (j, s) in stems.iter().enumerate() {
        let mut vals: Vec<f64> = tracks[j].iter().flatten().copied().collect();
        if vals.len() < 4 {
            continue;
        }
        vals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let usual = percentile(&vals, 0.5);
        let mut i = 0;
        while i < tracks[j].len() {
            let low = |w: usize| tracks[j][w].is_some_and(|v| v < usual - 12.0) && windows[w].is_some();
            if low(i) {
                let start = i;
                let mut end = i + 1;
                while end < tracks[j].len() && low(end) {
                    end += 1;
                }
                let drop = (start..end).filter_map(|w| tracks[j][w]).map(|v| usual - v).fold(0.0f64, f64::max);
                missing.push(MissingRegion { stem: s.name.clone(), start: start as f64 * WINDOW_SECS, end: end as f64 * WINDOW_SECS, drop_db: drop });
                i = end;
            } else {
                i += 1;
            }
        }
    }
    missing.sort_by(|a, b| a.start.partial_cmp(&b.start).unwrap_or(std::cmp::Ordering::Equal));
    missing.truncate(60);

    // Gain spread: does the level relationship wander (limiting, compression)?
    win_gains.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let gain_spread_db = if win_gains.len() >= 4 { percentile(&win_gains, 0.9) - percentile(&win_gains, 0.1) } else { 0.0 };
    if gain_spread_db > 3.0 {
        notes.push(format!(
            "The level relationship between the stems and the mix moves about by {:.1} dB — typical of limiting or compression on the mix. Flagged stretches may be that rather than a missing element.",
            gain_spread_db
        ));
    }
    if gain_db.abs() > 1.0 {
        notes.push(format!(
            "The stems sit {:.1} dB {} in the mix than at unity; the comparison allows for that.",
            gain_db.abs(),
            if gain_db < 0.0 { "lower" } else { "higher" }
        ));
    }
    let _ = gain;

    // Flag windows clearly worse than the file as a whole, merge into regions,
    // and say which stem each region most resembles.
    let flag_db = CLOSE_DB.max(overall + 6.0);
    let mut regions: Vec<Region> = Vec::new();
    let mut i = 0;
    while i < windows.len() {
        if windows[i].is_some_and(|v| v > flag_db) {
            let start = i;
            let mut end = i + 1;
            while end < windows.len() && windows[end].is_some_and(|v| v > flag_db) {
                end += 1;
            }
            let mut e_res = 0f64;
            let mut e_mix = 0f64;
            let mut c = vec![(0f64, 0f64); n];
            for w in start..end {
                e_res += win_res[w].0;
                e_mix += win_res[w].1;
                for j in 0..n {
                    c[j].0 += win_corr[w][j].0;
                    c[j].1 += win_corr[w][j].1;
                }
            }
            let mut best: Option<(usize, f64)> = None;
            for (j, (xy, xx)) in c.iter().enumerate() {
                if *xx > 0.0 && e_res > 0.0 {
                    let r = xy / (xx * e_res).sqrt();
                    if best.map_or(true, |b| r.abs() > b.1.abs()) {
                        best = Some((j, r));
                    }
                }
            }
            let (likely, direction, correlation) = match best {
                Some((j, r)) if r.abs() >= ATTRIBUTE_CORR => (
                    Some(stems[j].name.clone()),
                    Some(if r < 0.0 { "stems_only" } else { "mix_only" }.to_string()),
                    r,
                ),
                Some((_, r)) => (None, None, r),
                None => (None, None, 0.0),
            };
            regions.push(Region {
                start: start as f64 * WINDOW_SECS,
                end: end as f64 * WINDOW_SECS,
                residual_db: db(e_res / e_mix.max(1e-12)),
                likely,
                direction,
                correlation,
            });
            i = end;
        } else {
            i += 1;
        }
    }
    regions.sort_by(|a, b| b.residual_db.partial_cmp(&a.residual_db).unwrap_or(std::cmp::Ordering::Equal));
    regions.truncate(60);
    regions.sort_by(|a, b| a.start.partial_cmp(&b.start).unwrap_or(std::cmp::Ordering::Equal));

    let mut verdict = if overall <= MATCH_DB { "match" } else if overall <= CLOSE_DB { "close" } else { "mismatch" };
    if verdict == "match" && (!absent.is_empty() || level_flags > 0) {
        verdict = "close";
    }
    // Every stem is recognisably in the mix, yet nothing cancels: the mix
    // was treated after the stems. Say so, and don't pretend the residual
    // means anything — the per-stem tracks carry the useful information.
    let present: Vec<&Alignment> = aligns.iter().filter(|a| a.aligned).collect();
    let all_recognised = !present.is_empty() && present.len() == aligns.len() && present.iter().all(|a| a.resemblance >= 0.2);
    // "Processed" means the failure is everywhere, not one hole: the typical
    // window is itself far from cancelling.
    let mut win_vals: Vec<f64> = windows.iter().flatten().copied().collect();
    win_vals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let typical_window_db = if win_vals.is_empty() { overall } else { percentile(&win_vals, 0.5) };
    if verdict == "mismatch" && all_recognised && typical_window_db > -15.0 {
        verdict = "processed";
        regions.clear();
        let mut why = Vec::new();
        if mix_louder_db > 1.0 {
            why.push(format!("it is {:.1} dB louder than the stems add up to", mix_louder_db));
        }
        if mix_peak_lower_db > 1.0 {
            why.push(format!("its peaks sit {:.1} dB lower than the stems' sum (limiting)", mix_peak_lower_db));
        }
        let inverted: Vec<&str> = stems.iter().zip(&aligns).filter(|(_, a)| a.flipped).map(|(s, _)| s.name.as_str()).collect();
        if !inverted.is_empty() {
            why.push(format!(
                "{} come{} back phase-shifted or inverted, which a multi-band crossover will do",
                inverted.join(" and "),
                if inverted.len() == 1 { "s" } else { "" }
            ));
        }
        let offsets: Vec<String> = stems.iter().zip(&aligns).filter(|(_, a)| a.lag_secs.abs() >= 0.0003).map(|(s, a)| format!("{} {:.1} ms", s.name, a.lag_secs * 1000.0)).collect();
        if offsets.len() >= 2 {
            why.push(format!("the stems sit at different offsets ({})", offsets.join(", ")));
        }
        notes.insert(0, format!(
            "Every stem is recognisably in the mix, but the mix is not a plain sum of them{}{}. Mix-bus limiting, EQ or phase rotation does this; so does printing the stems through a different chain. A null test can't pass here, so the residual is not the measure — the per-stem levels and the stretches flagged below are.",
            if why.is_empty() { "" } else { ": " },
            why.join("; ")
        ));
    }
    progress("done", 1.0);
    Ok(StemReport {
        sample_rate: rate,
        channels: ch,
        duration_secs,
        gain_db,
        gain_spread_db,
        overall_residual_db: overall,
        flag_db,
        verdict: verdict.to_string(),
        mix_louder_db,
        mix_peak_lower_db,
        stems: stem_reports,
        regions,
        missing,
        windows,
        window_secs: WINDOW_SECS,
        residual_path: residual_path.to_string(),
        notes,
    })
}

/// Solve (SᵀS) g = Sᵀm for the stem gains: Gaussian elimination with a touch
/// of ridge so two near-identical stems don't blow up.
pub fn solve_gains(sts: &[Vec<f64>], stm: &[f64]) -> Vec<f64> {
    let n = stm.len();
    let trace: f64 = (0..n).map(|i| sts[i][i]).sum();
    let ridge = trace * 1e-9 / n.max(1) as f64;
    let mut a: Vec<Vec<f64>> = (0..n).map(|i| {
        let mut row = sts[i].clone();
        row[i] += ridge;
        row.push(stm[i]);
        row
    }).collect();
    for col in 0..n {
        let piv = (col..n).max_by(|&x, &y| a[x][col].abs().partial_cmp(&a[y][col].abs()).unwrap_or(std::cmp::Ordering::Equal)).unwrap();
        a.swap(col, piv);
        let p = a[col][col];
        if p.abs() < 1e-18 {
            continue;
        }
        for r in 0..n {
            if r != col {
                let f = a[r][col] / p;
                if f != 0.0 {
                    let pivot_row = a[col].clone();
                    for (c, pv) in pivot_row.iter().enumerate().skip(col) {
                        a[r][c] -= f * pv;
                    }
                }
            }
        }
    }
    (0..n).map(|i| if a[i][i].abs() < 1e-18 { 0.0 } else { a[i][n] / a[i][i] }).collect()
}

/// A fresh folder for the residual file (removed with `processing::remove_workdir`).
pub fn workdir() -> Result<String, String> {
    let dir = std::env::temp_dir().join(format!("laybacker-stemcheck-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).map_err(|e| format!("Could not create work folder: {}", e))?;
    Ok(dir.to_string_lossy().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn noise(n: usize, seed: u64) -> Vec<f32> {
        let mut x = seed;
        (0..n)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                ((x % 20001) as f32 / 10000.0 - 1.0) * 0.3
            })
            .collect()
    }

    #[test]
    fn correlate_finds_offset_and_polarity() {
        let mix = noise(8000, 7);
        let stem: Vec<f32> = mix[1234..1234 + 2000].to_vec();
        let (k, peak, peakiness) = correlate(&stem, &mix).unwrap();
        assert_eq!(k, 1234);
        assert!(peak > 0.9, "peak {}", peak);
        assert!(peakiness > 10.0, "peakiness {}", peakiness);
        let flipped: Vec<f32> = stem.iter().map(|v| -v).collect();
        let (k2, peak2, _) = correlate(&flipped, &mix).unwrap();
        assert_eq!(k2, 1234);
        assert!(peak2 < -0.9);
    }

    #[test]
    fn phat_finds_offset_in_autocorrelated_signal() {
        // A low-passed (smoothed) signal correlates with itself for a while;
        // PHAT still puts one sharp peak at the right place.
        let raw = noise(16000, 5);
        let mut mix = vec![0f32; raw.len()];
        let mut acc = 0f32;
        for (i, &v) in raw.iter().enumerate() {
            acc = 0.97 * acc + 0.03 * v;
            mix[i] = acc;
        }
        let stem: Vec<f32> = mix[3000..3000 + 4000].to_vec();
        let (k, peakiness) = correlate_phat(&stem, &mix).unwrap();
        assert_eq!(k, 3000);
        assert!(peakiness > 8.0, "peakiness {}", peakiness);
        assert!(ncc_at(&stem, &mix, k) > 0.99);
    }

    #[test]
    fn solve_gains_recovers_levels() {
        let a = noise(4000, 1);
        let b = noise(4000, 2);
        let mix: Vec<f64> = a.iter().zip(&b).map(|(x, y)| 0.5 * *x as f64 + 2.0 * *y as f64).collect();
        let s = [a, b];
        let mut sts = vec![vec![0f64; 2]; 2];
        let mut stm = vec![0f64; 2];
        for i in 0..4000 {
            for j in 0..2 {
                stm[j] += s[j][i] as f64 * mix[i];
                for k in 0..2 {
                    sts[j][k] += s[j][i] as f64 * s[k][i] as f64;
                }
            }
        }
        let g = solve_gains(&sts, &stm);
        assert!((g[0] - 0.5).abs() < 1e-6 && (g[1] - 2.0).abs() < 1e-6, "{:?}", g);
    }

    #[test]
    fn correlate_of_unrelated_noise_is_flat() {
        let mix = noise(8000, 1);
        let stem = noise(2000, 2);
        let (_, peak, peakiness) = correlate(&stem, &mix).unwrap();
        assert!(peak.abs() < 0.2, "peak {}", peak);
        assert!(peakiness < 6.0, "peakiness {}", peakiness);
    }

    #[test]
    fn align_filter_shapes() {
        let base = "aresample=osr=48000:ochl=stereo:osf=flt";
        assert_eq!(align_filter(0.0, 0.0, false, 48000, 2), base);
        assert_eq!(align_filter(0.1, 0.0, false, 48000, 2), format!("{},atrim=start_sample=4800", base));
        assert_eq!(align_filter(-0.01, 0.0, true, 48000, 2), format!("{},adelay=480S:all=1,volume=-1", base));
        assert_eq!(align_filter(0.0, 0.001, false, 48000, 2), format!("{},asetrate=48048,aresample=48000", base));
        assert_eq!(base_filter(8000, 1), "aresample=osr=8000:ochl=mono:osf=flt");
    }

    #[test]
    fn wav_writer_writes_a_valid_header() {
        let dir = std::env::temp_dir().join(format!("stemcheck-wav-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("r.wav");
        let mut w = WavWriter::create(p.to_str().unwrap(), 48000, 2).unwrap();
        w.write(&[0.5, -0.5, 0.0, 1.0]).unwrap();
        w.close().unwrap();
        let bytes = std::fs::read(&p).unwrap();
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(u32::from_le_bytes([bytes[40], bytes[41], bytes[42], bytes[43]]), 12);
        assert_eq!(bytes.len(), 44 + 12);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
