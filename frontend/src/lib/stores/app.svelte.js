import { invoke } from '@tauri-apps/api/core';
import { renderSlateImage } from '../slate.js';
import { nameForRule } from '../naming.js';

// Small persisted preferences (localStorage in the webview). Guarded — a
// missing or blocked store must never break the app.
function loadPref(key, fallback) {
  try {
    const raw = localStorage.getItem(`pref:${key}`);
    return raw == null ? fallback : JSON.parse(raw);
  } catch { return fallback; }
}
function savePref(key, value) {
  try { localStorage.setItem(`pref:${key}`, JSON.stringify(value)); } catch { /* optional */ }
}

// Flags the frontend owns on a pair. The Rust namer round-trip drops unknown
// fields, so these are merged back after every generate_names call.
const FRONTEND_FLAGS = ['nameCustomized', 'lengthFixChosen', 'audioStart', 'startChosen'];
function keepFrontendFlags(fresh, previous) {
  const byId = new Map(previous.map(p => [p.id, p]));
  return fresh.map(p => {
    const old = byId.get(p.id);
    if (!old) return p;
    const flags = {};
    for (const k of FRONTEND_FLAGS) if (old[k] !== undefined) flags[k] = old[k];
    return { ...p, ...flags };
  });
}

// Reactive state using Svelte 5 runes
let files = $state([]);
let matchedPairs = $state([]);
let ffmpegStatus = $state({ available: false, version: '' });
let isScanning = $state(false);
let isProcessing = $state(false);
let processingResults = $state([]);
let progressMap = $state({});
let errors = $state([]);

// Blocking "audio is longer than video" prompt. When set, App shows a modal for
// this one pair and export waits until the user picks cut / fade / freeze.
let lengthPrompt = $state(null); // { pairId, filename, overBy, fps } | null
let _lengthResolve = null;

// How much longer (seconds) the audio must run before we ask about it.
const LENGTH_MISMATCH_TOLERANCE = 0.5;

// ── Slate ───────────────────────────────────────────────────────────────────
// A user-written text card prepended to the video for a chosen duration,
// silent underneath, audio delayed to match. One batch text with per-file
// tweaks. The card is drawn to a canvas at the video's exact frame size and
// sent to the backend as a base64 JPEG at export.
let slateEditor = $state(null); // { scope: 'batch' | 'solo' | <pairId>, text, duration, video? } | null
let batchSlate = $state({ text: '', duration: 5 });
// Solo-video slating (video dropped without audio): render state per video path.
let soloSlateStatus = $state({}); // { [videoPath]: { state: 'working'|'done'|'error', pct, output? } }

function openSlateEditor(scope) {
  if (scope === 'batch') {
    slateEditor = { scope, text: batchSlate.text, duration: batchSlate.duration };
    return;
  }
  // A solo video (no pair) — the object form carries the MediaFile itself.
  if (typeof scope === 'object' && scope?.path) {
    slateEditor = {
      scope: 'solo',
      video: scope,
      text: batchSlate.text,
      duration: batchSlate.duration,
    };
    return;
  }
  const p = matchedPairs.find(p => p.id === scope);
  if (!p) return;
  slateEditor = {
    scope,
    // A pair with no text of its own starts from the batch slate.
    text: p.slateText || batchSlate.text,
    duration: p.slateEnabled ? p.slateDurationSecs : batchSlate.duration,
  };
}

function closeSlateEditor() {
  slateEditor = null;
}

// Solo slate: render the card and run the slate job immediately — there's no
// pair or export step to defer to. Keeps the video's own soundtrack.
async function applySoloSlate(video, text, dur) {
  const path = video.path;
  soloSlateStatus = { ...soloSlateStatus, [path]: { state: 'working', pct: 0 } };
  try {
    const image = renderSlateImage(text, video.width, video.height);
    const output = await invoke('slate_video', {
      videoPath: path,
      videoDurationSecs: video.durationSecs,
      slateImage: image,
      slateDurationSecs: dur,
      frameRate: video.frameRate ?? null,
      // The probe reports audio-stream fields for videos with a soundtrack.
      hasAudio: video.channelCount != null || video.sampleRate != null,
    });
    soloSlateStatus = { ...soloSlateStatus, [path]: { state: 'done', pct: 100, output } };
    playCompletionSound();
  } catch (e) {
    errors = [...errors, `Slate failed: ${e}`];
    soloSlateStatus = { ...soloSlateStatus, [path]: { state: 'error', pct: 0 } };
  }
}

// Live encode progress from the backend (slate-progress events).
function updateSoloSlateProgress(payload) {
  const cur = soloSlateStatus[payload?.videoPath];
  if (!cur || cur.state !== 'working') return;
  soloSlateStatus = {
    ...soloSlateStatus,
    [payload.videoPath]: { ...cur, pct: Math.min(100, Math.round((payload.progress ?? 0) * 100)) },
  };
}

// Apply the editor: batch scope stamps every video pair, a pair scope just one,
// solo scope renders straight away.
function applySlate(text, duration) {
  if (!slateEditor) return;
  const scope = slateEditor.scope;
  const dur = Math.max(0.5, duration || 5);
  if (scope === 'solo') {
    const video = slateEditor.video;
    batchSlate = { text, duration: dur }; // remember for the next slate
    slateEditor = null;
    applySoloSlate(video, text, dur);
    return;
  }
  if (scope === 'batch') {
    batchSlate = { text, duration: dur };
    matchedPairs = matchedPairs.map(p =>
      p.video ? { ...p, slateEnabled: true, slateText: text, slateDurationSecs: dur } : p
    );
  } else {
    matchedPairs = matchedPairs.map(p =>
      p.id === scope ? { ...p, slateEnabled: true, slateText: text, slateDurationSecs: dur } : p
    );
  }
  slateEditor = null;
}

// Remove the slate — from every video pair (batch scope) or one pair.
function removeSlate() {
  if (!slateEditor) return;
  const scope = slateEditor.scope;
  matchedPairs = matchedPairs.map(p =>
    (scope === 'batch' ? !!p.video : p.id === scope)
      ? { ...p, slateEnabled: false, slateImage: null }
      : p
  );
  slateEditor = null;
}


// ── Batch QC ────────────────────────────────────────────────────────────────
// One spec for the whole batch: a loudness value (which is ALSO every pair's
// NORM target, so QC and the export agree) plus an optional 6-frame silence
// check. Results are transient — they describe the source files, not the export.
// The batch value IS the loudness target — the one number QC checks against and
// NORM corrects to. Loudness is the headline metric; the true-peak ceiling
// (-1 dBTP) is a background check, deemphasised in the UI and the naming.
let qcTargetLufs = $state(loadPref('qcTargetLufs', -23));
// True-peak value for the whole batch. Its role depends on qcMode:
//  - 'lufs' mode: a ceiling — NORM levels to LUFS but never pushes peak above it.
//  - 'peak' mode: THE target — NORM boosts/cuts each file so its peak lands here,
//    LUFS ignored entirely. (This maps to the engine's full-scale path, which it
//    triggers when target_lufs >= 0, so peak mode carries target_lufs = 0.)
let qcTruePeak = $state(loadPref('qcTruePeak', -1.0));
let qcMode = $state(loadPref('qcMode', 'lufs')); // 'lufs' | 'peak'
let qcCheckSilence = $state(false);

// The batch normalization spec every pair carries, derived from the current mode
// and targets. Peak mode uses target_lufs = 0 to select the engine's peak path.
function batchNormSettings(existing = {}) {
  return qcMode === 'peak'
    ? { ...existing, targetLufs: 0, truePeakLimit: qcTruePeak }
    : { ...existing, targetLufs: qcTargetLufs, truePeakLimit: qcTruePeak };
}
let qcResults = $state({}); // { [pairId]: { pass, lufsPass, peakPass, silencePass, measuredLufs, measuredTP, headHasAudio, tailHasAudio, error? } }
let qcRunning = $state(false);
let qcProgress = $state({ done: 0, total: 0 });

// Changing the batch loudness value retargets every pair's NORM and voids any
// existing results (they were measured against a different spec).
function setQcTargetLufs(value) {
  qcTargetLufs = value;
  savePref('qcTargetLufs', value);
  matchedPairs = matchedPairs.map(p => ({
    ...p,
    normalizationSettings: batchNormSettings(p.normalizationSettings),
  }));
  qcResults = {};
  clockChecks = {}; // clock verdicts were judged against the old target
  regenerateNames(); // the spec in the filename follows the target
}

// Changing the batch true-peak value retargets every pair and voids results
// (the peak check and the NORM cap/target both depend on it).
function setQcTruePeak(value) {
  qcTruePeak = value;
  savePref('qcTruePeak', value);
  matchedPairs = matchedPairs.map(p => ({
    ...p,
    normalizationSettings: batchNormSettings(p.normalizationSettings),
  }));
  qcResults = {};
  clockChecks = {};
  regenerateNames();
}

// Switch the whole batch between loudness (LUFS) and true-peak (dBTP) targeting.
function setQcMode(mode) {
  qcMode = mode;
  savePref('qcMode', mode);
  matchedPairs = matchedPairs.map(p => ({
    ...p,
    normalizationSettings: batchNormSettings(p.normalizationSettings),
  }));
  qcResults = {};
  clockChecks = {};
  regenerateNames();
}

// ── Batch passes that act immediately ───────────────────────────────────────
// The QC bar owns batch operations, and they RUN when clicked: NORMALISE ALL
// renders levelled files, CLOCK ALL checks then renders clocked files. After a
// pass, the list reloads with the rendered outputs (plus any files the pass
// skipped) and QC re-runs automatically so the NEW levels are analysed and
// displayed — no flags left pending on the big green button.

/// Render `subset` now, then reload the list with the outputs (+ `keep` paths
/// that weren't part of the pass) and re-analyse everything.
async function runPassAndReload(subset, keepPaths) {
  isProcessing = true;
  processingResults = [];
  progressMap = {};
  try {
    const results = await invoke('process_pairs', { pairs: subset, settings: exportSettings });
    const failed = results.filter(r => !r.success);
    for (const f of failed) {
      errors = [...errors, `Processing failed: ${f.error ?? 'unknown error'}`];
    }
    playCompletionSound();
    const outputs = results.filter(r => r.success && r.outputPath).map(r => r.outputPath);
    if (outputs.length > 0) {
      const reload = [...outputs, ...keepPaths];
      files = [];
      matchedPairs = [];
      processingResults = [];
      progressMap = {};
      qcResults = {};
      clockChecks = {};
      const scanned = await invoke('scan_files', { paths: reload });
      files = scanned;
      await autoMatch();
      await runBatchQc(); // analyse and display the new levels
    }
  } catch (e) {
    errors = [...errors, `Processing failed: ${e}`];
  } finally {
    isProcessing = false;
  }
}

/// Batch loudness normalisation: level every audio file to the batch target now.
async function normalizeAllNow() {
  if (isProcessing || qcRunning || clockRunning) return;
  const targets = matchedPairs.filter(p => !p.video);
  if (targets.length === 0) return;
  matchedPairs = matchedPairs.map(p =>
    p.video ? p : {
      ...p,
      normalizationEnabled: true,
      clockEnabled: false,
      normalizationSettings: batchNormSettings(p.normalizationSettings),
    }
  );
  await regenerateNames(); // output names carry the spec before rendering
  const subset = matchedPairs.filter(p => !p.video);
  await runPassAndReload(subset, []);
}

/// CLOCK ALL: check every audio file (level + head/tail silence), then render
/// the clocked files for the ones that pass. Failures stay in the list with
/// their reason, unrendered.
async function clockAllNow() {
  if (isProcessing || qcRunning || clockRunning) return;
  await runBatchClock(); // sets clockChecks and clockEnabled on the passes
  const toClock = matchedPairs.filter(p => !p.video && p.clockEnabled);
  if (toClock.length === 0) return;
  const keep = matchedPairs
    .filter(p => !p.video && !p.clockEnabled)
    .map(p => p.audio.path);
  await runPassAndReload(toClock, keep);
}

/// 6 Fr ALL: apply the 6-frame broadcast mute to every audio file now, then
/// reload with the muted outputs. Guarded behind an "Are you sure?" in the UI
/// because the mute is destructive at the head and tail.
async function sixFrAllNow() {
  if (isProcessing || qcRunning || clockRunning) return;
  const targets = matchedPairs.filter(p => !p.video);
  if (targets.length === 0) return;
  matchedPairs = matchedPairs.map(p =>
    p.video ? p : {
      ...p,
      silenceCompliance: true,
      clockEnabled: false,
    }
  );
  await regenerateNames(); // output names carry the _6Fr marker before rendering
  const subset = matchedPairs.filter(p => !p.video);
  await runPassAndReload(subset, []);
}

function setQcCheckSilence(value) {
  qcCheckSilence = value;
  qcResults = {};
}

async function runBatchQc() {
  if (qcRunning || matchedPairs.length === 0) return;
  qcRunning = true;
  qcResults = {};
  qcProgress = { done: 0, total: matchedPairs.length };
  const results = {};
  // Sequential: each measurement spawns ffmpeg, so don't thrash the CPU on a
  // big batch — and it gives an honest progress count.
  for (const p of matchedPairs) {
    try {
      const [measuredLufs, measuredTP] = await invoke('measure_loudness', { audioPath: p.audio.path });
      let headHasAudio = false;
      let tailHasAudio = false;
      if (qcCheckSilence) {
        [headHasAudio, tailHasAudio] = await invoke('check_silence', {
          audioPath: p.audio.path,
          durationSecs: p.audio.durationSecs,
          silenceMs: p.silenceMs ?? 240.0,
        });
      }
      const peakLimit = p.normalizationSettings?.truePeakLimit ?? -1.0;
      let lufsPass, peakPass;
      if (qcMode === 'peak') {
        // True-peak targeting: LUFS is irrelevant; the peak must land on target
        // (a file under target is "wrong" here — NORM will boost it up).
        lufsPass = true;
        peakPass = Math.abs(measuredTP - qcTruePeak) <= 0.1;
      } else {
        lufsPass = Math.abs(measuredLufs - qcTargetLufs) <= 1.0;
        peakPass = measuredTP <= peakLimit + 0.05; // ceiling, not a target
      }
      const silencePass = qcCheckSilence ? (!headHasAudio && !tailHasAudio) : true;
      results[p.id] = {
        pass: lufsPass && peakPass && silencePass,
        lufsPass, peakPass, silencePass, silenceChecked: qcCheckSilence,
        measuredLufs, measuredTP, headHasAudio, tailHasAudio, peakLimit, mode: qcMode,
      };
    } catch (e) {
      results[p.id] = { error: String(e) };
    }
    qcProgress = { done: qcProgress.done + 1, total: matchedPairs.length };
    qcResults = { ...results };
  }
  qcResults = results;
  qcRunning = false;
}

// Completion chime, played through the webview (WebKit) rather than a native
// player (afplay). A native CoreAudio player triggers a one-time macOS
// microphone permission prompt after each update; webview audio does not.
let completionAudio = null;
function playCompletionSound() {
  try {
    if (!completionAudio) completionAudio = new Audio('/LaybackComplete.wav');
    completionAudio.currentTime = 0;
    completionAudio.play().catch(() => { /* autoplay/optional — ignore */ });
  } catch { /* sound is optional */ }
}

let exportSettings = $state({
  videoCodec: 'original',
  audioFormat: 'original',
  aacBitrate: 320000,
  outputDirectory: null,
  useAudioFileLocation: true,
});

let namingSettings = $state({
  removeDuplicates: true,
});

// Derived
// Remembered between launches, like the date format already is — a peak-mode
// user shouldn't re-set -1 dBTP and AAC every session.
exportSettings = { ...exportSettings, ...loadPref('exportSettings', {}) };

function getOutputExtension() {
  return exportSettings.audioFormat === 'original' ? 'mov' : 'mp4';
}

// Format toggled (Original/H.264, Original/AAC): the container extension must
// follow, but the NAME is the user's — they may have edited it already, so
// don't regenerate it, just swap the extension on video outputs.
function updateOutputExtensions() {
  const ext = getOutputExtension();
  matchedPairs = matchedPairs.map(p => {
    if (!p.video || !p.outputFilename) return p;
    const dot = p.outputFilename.lastIndexOf('.');
    const stem = dot > 0 ? p.outputFilename.slice(0, dot) : p.outputFilename;
    return { ...p, outputFilename: `${stem}.${ext}` };
  });
}

function getVideos() {
  return files.filter(f => f.mediaType === 'video');
}

function getAudios() {
  return files.filter(f => f.mediaType === 'audio');
}

// Actions
async function checkFfmpeg() {
  try {
    const version = await invoke('check_ffmpeg');
    ffmpegStatus = { available: true, version };
  } catch (e) {
    ffmpegStatus = { available: false, version: '' };
    errors = [...errors, e];
  }
}

async function scanFiles(paths) {
  // If processing has completed, clear everything for a fresh start
  if (processingResults.length > 0) {
    files = [];
    matchedPairs = [];
    processingResults = [];
    progressMap = {};
    qcResults = {};
    clockChecks = {};
  }

  isScanning = true;
  errors = [];
  try {
    const scanned = await invoke('scan_files', { paths });

    // Merge new files with existing, deduplicating by full path
    const existingPaths = new Set(files.map(f => f.path));
    const newFiles = scanned.filter(f => !existingPaths.has(f.path));
    files = [...files, ...newFiles];

    await autoMatch();
  } catch (e) {
    console.error('[store] SCAN ERROR:', e);
    errors = [...errors, `Scan failed: ${e}`];
  } finally {
    isScanning = false;
  }
}

async function autoMatch() {
  try {
    // Remember user customizations keyed by video+audio path combo
    const customizations = {};
    for (const p of matchedPairs) {
      const key = `${p.video?.path || 'none'}|${p.audio.path}`;
      customizations[key] = {
        outputFilename: p.outputFilename,
        nameCustomized: p.nameCustomized,
        normalizationEnabled: p.normalizationEnabled,
        normalizationSettings: p.normalizationSettings,
      };
    }

    const videos = files.filter(f => f.mediaType === 'video');
    const audios = files.filter(f => f.mediaType === 'audio');

    let pairs;
    if (videos.length === 0 && audios.length > 0) {
      // Audio-only mode: create entries for each audio file
      pairs = audios.map(audio => ({
        id: crypto.randomUUID(),
        video: null,
        audio,
        outputFilename: '',
        // QC-first workflow: files arrive untouched. QC (or NORM ALL / FIX)
        // decides what gets levelled — to the one batch target.
        normalizationEnabled: false,
        normalizationSettings: batchNormSettings(),
        timecodeOffsetSecs: 0,
        matchConfidence: 1.0,
        silenceCompliance: false,
        silenceMs: 240.0,
        fadeMs: 5.0,
        clockEnabled: false,
      }));
    } else {
      pairs = await invoke('match_files', { files });
    }

    pairs = await invoke('generate_names', {
      pairs,
      removeDuplicates: namingSettings.removeDuplicates,
      outputExtension: getOutputExtension(),
    });

    // Restore user customizations for pairs that still exist
    pairs = pairs.map(p => {
      const key = `${p.video?.path || 'none'}|${p.audio.path}`;
      const custom = customizations[key];
      if (custom) {
        return { ...p, ...custom };
      }
      return p;
    });

    // The batch spec (mode + targets) rides on every pair, including files
    // dropped later, so QC and the export can never disagree.
    pairs = pairs.map(p => ({
      ...p,
      normalizationSettings: batchNormSettings(p.normalizationSettings),
    }));

    // New drops follow the batch naming rule (user-renamed pairs are kept).
    matchedPairs = applyBatchRule(pairs);
  } catch (e) {
    errors = [...errors, `Matching failed: ${e}`];
  }
}

async function regenerateNames() {
  if (matchedPairs.length === 0) return;
  const before = matchedPairs;
  try {
    const fresh = await invoke('generate_names', {
      pairs: matchedPairs,
      removeDuplicates: namingSettings.removeDuplicates,
      outputExtension: getOutputExtension(),
    });
    // The namer round-trip drops frontend-only flags — put them back, then
    // keep every user-edited name (stem as typed, extension from the namer)
    // and apply the batch rule to the rest.
    const byId = new Map(before.map(p => [p.id, p]));
    let pairs = keepFrontendFlags(fresh, before).map(p => {
      const old = byId.get(p.id);
      if (!old?.nameCustomized) return p;
      const mine = old.outputFilename;
      const dot = mine.lastIndexOf('.');
      const stem = dot > 0 ? mine.slice(0, dot) : mine;
      const freshDot = p.outputFilename.lastIndexOf('.');
      const ext = freshDot > 0 ? p.outputFilename.slice(freshDot + 1) : extFor(p);
      return { ...p, outputFilename: `${stem}.${ext}` };
    });
    matchedPairs = applyBatchRule(pairs);
  } catch (e) {
    errors = [...errors, `Naming failed: ${e}`];
  }
}

async function cancelProcessing() {
  await invoke('cancel_processing');
}

// Ask the user how to handle one over-length pair. Resolves to the chosen fix
// ('cut' | 'fade' | 'freeze'), or null if they cancel the whole export.
function askLengthFix(pair) {
  const overBy = pair.audio.durationSecs - pair.video.durationSecs;
  lengthPrompt = {
    pairId: pair.id,
    filename: pair.audio.filename,
    overBy,
    fps: pair.video.frameRate ?? null,
  };
  return new Promise((resolve) => { _lengthResolve = resolve; });
}

// Called by the modal: record the choice on the pair and let export continue.
// `lengthFixChosen` marks that the USER picked — distinct from the backend's
// default `lengthFix: "cut"`, which every pair carries from the matcher.
function resolveLengthFix(choice) {
  const id = lengthPrompt?.pairId;
  if (id) {
    matchedPairs = matchedPairs.map(p =>
      p.id === id ? { ...p, lengthFix: choice, lengthFixChosen: true } : p
    );
  }
  lengthPrompt = null;
  const r = _lengthResolve; _lengthResolve = null;
  if (r) r(choice);
}

// Called by the modal's Cancel: abort the export entirely.
function cancelLengthFix() {
  lengthPrompt = null;
  const r = _lengthResolve; _lengthResolve = null;
  if (r) r(null);
}

// ── Audio shorter than video: where does the sound belong? ──────────────────
// A slated picture is longer than its programme audio by exactly the slate
// length, so "line the sound up with the end" is correct by construction — no
// picture analysis needed. If Laybacker made the slate, the file carries a tag
// with the exact length and the prompt pre-selects END.
let startPrompt = $state(null); // { pairId, filename, shortBy, slateSecs } | null
let _startResolve = null;

function askAudioStart(pair) {
  startPrompt = {
    pairId: pair.id,
    filename: pair.audio.filename,
    shortBy: pair.video.durationSecs - pair.audio.durationSecs,
    slateSecs: pair.video.slateSecs ?? null,
  };
  return new Promise((resolve) => { _startResolve = resolve; });
}

// choice: 'start' (sound from the first frame) | 'end' (sound ends with picture)
function resolveAudioStart(choice) {
  const id = startPrompt?.pairId;
  if (id) {
    matchedPairs = matchedPairs.map(p => {
      if (p.id !== id) return p;
      const offset = choice === 'end'
        ? Math.max(0, p.video.durationSecs - p.audio.durationSecs)
        : 0;
      return { ...p, timecodeOffsetSecs: offset, audioStart: choice, startChosen: true };
    });
  }
  startPrompt = null;
  const r = _startResolve; _startResolve = null;
  if (r) r(choice);
}

function cancelAudioStart() {
  startPrompt = null;
  const r = _startResolve; _startResolve = null;
  if (r) r(null);
}

async function processAll() {
  if (matchedPairs.length === 0) return;

  // Pre-flight: for every pair whose audio runs past its video, ask how to
  // reconcile it — one blocking prompt at a time — before any rendering starts.
  const overLong = matchedPairs.filter(p =>
    p.video &&
    !p.lengthFixChosen &&
    (p.audio.durationSecs - p.video.durationSecs) > LENGTH_MISMATCH_TOLERANCE
  );
  for (const p of overLong) {
    const choice = await askLengthFix(p);
    if (choice === null) return; // user cancelled — export nothing
  }

  // …and for every pair whose audio is SHORTER than its video (a slated or
  // headed picture), ask where the sound belongs.
  const overShort = matchedPairs.filter(p =>
    p.video &&
    !p.startChosen &&
    (p.video.durationSecs - p.audio.durationSecs) > LENGTH_MISMATCH_TOLERANCE
  );
  for (const p of overShort) {
    const choice = await askAudioStart(p);
    if (choice === null) return;
  }

  // Render each slate card at its video's exact frame size (the backend can't
  // draw text — the bundled ffmpeg has no freetype).
  matchedPairs = matchedPairs.map(p =>
    p.video && p.slateEnabled
      ? { ...p, slateImage: renderSlateImage(p.slateText, p.video.width, p.video.height) }
      : p
  );

  isProcessing = true;
  processingResults = [];
  progressMap = {};

  try {
    const results = await invoke('process_pairs', {
      pairs: matchedPairs,
      settings: exportSettings,
    });
    processingResults = results;

    // Play completion chime (through the webview — see playCompletionSound).
    playCompletionSound();
  } catch (e) {
    errors = [...errors, `Processing failed: ${e}`];
  } finally {
    isProcessing = false;
  }
}

// The main action button does the job it's LABELLED for. For an audio-only
// batch that means normalising to the QC spec (or clocking) — not a plain
// passthrough, which would leave the files untouched. Video laybacks lay back.
async function runMainAction() {
  if (matchedPairs.length === 0) return;
  const audioOnly = matchedPairs.every(p => !p.video);
  if (!audioOnly) return processAll();

  const clockOnly = matchedPairs.some(p => p.clockEnabled)
    && !matchedPairs.some(p => p.normalizationEnabled);
  return clockOnly ? clockAllNow() : normalizeAllNow();
}

function updateProgress(progress) {
  progressMap = { ...progressMap, [progress.pairId]: progress };
}

function updatePairNormalization(pairId, enabled, settings) {
  matchedPairs = matchedPairs.map(p => {
    if (p.id === pairId) {
      return { ...p, normalizationEnabled: enabled, normalizationSettings: settings };
    }
    return p;
  });
  regenerateNames();
}

function updatePairCompliance(pairId, enabled) {
  matchedPairs = matchedPairs.map(p => {
    if (p.id === pairId) {
      return { ...p, silenceCompliance: enabled };
    }
    return p;
  });
}

function updatePairClock(pairId, enabled) {
  // Clock composes with NORM: the handles are silence and don't change the
  // programme level, so a file can be levelled AND clocked in one export.
  matchedPairs = matchedPairs.map(p =>
    p.id === pairId ? { ...p, clockEnabled: enabled } : p
  );
  regenerateNames();
}

// ── Clock ───────────────────────────────────────────────────────────────────
// Clock is its own pass, independent of QC. The check lives here (rather than in
// the row) so the per-file button and CLOCK ALL share one path and report the
// same per-file results.
let clockChecks = $state({}); // { [pairId]: { silencePass, loudnessPass, ... } | { error } }
let clockRunning = $state(false);
let clockProgress = $state({ done: 0, total: 0 });

async function evaluateClockFor(pair) {
  const [measuredLufs, measuredTP] = await invoke('measure_loudness', { audioPath: pair.audio.path });
  const [headHasAudio, tailHasAudio] = await invoke('check_silence', {
    audioPath: pair.audio.path,
    durationSecs: pair.audio.durationSecs,
    silenceMs: pair.silenceMs ?? 240.0,
  });
  const silencePass = !headHasAudio && !tailHasAudio;
  // Integrated loudness is the target; true peak is a ceiling, not a target.
  // A file already marked for levelling (QC fix / NORM) WILL be at target on
  // export, so the loudness gate passes by construction — step two of the
  // QC-then-clock workflow only needs to judge the silence.
  const willBeLevelled = pair.normalizationEnabled;
  const { targetLufs, truePeakLimit } = pair.normalizationSettings;
  const hasLufsTarget = targetLufs < 0;
  const lufsPass = willBeLevelled || !hasLufsTarget || Math.abs(measuredLufs - targetLufs) <= 1.0;
  const peakPass = willBeLevelled || measuredTP <= truePeakLimit + 0.05;
  return {
    silencePass, loudnessPass: lufsPass && peakPass, lufsPass, peakPass,
    willBeLevelled, hasLufsTarget, targetLufs, truePeakLimit,
    headHasAudio, tailHasAudio, measuredLufs, measuredTP,
  };
}

/// Check one file and clock it if it passes. Returns true when it passed.
async function runClockCheck(pairId) {
  const pair = matchedPairs.find(p => p.id === pairId);
  if (!pair) return false;
  try {
    const r = await evaluateClockFor(pair);
    clockChecks = { ...clockChecks, [pairId]: r };
    const passed = r.silencePass && r.loudnessPass;
    if (passed) updatePairClock(pairId, true);
    return passed;
  } catch (e) {
    clockChecks = { ...clockChecks, [pairId]: { error: String(e) } };
    return false;
  }
}

/// An independent pass over every audio-only file: check, then clock the passes.
/// Failures are left unclocked with their reason, to override individually.
async function runBatchClock() {
  const targets = matchedPairs.filter(p => !p.video);
  if (clockRunning || targets.length === 0) return;
  clockRunning = true;
  clockProgress = { done: 0, total: targets.length };
  const checks = { ...clockChecks };
  const passed = new Set();
  for (const p of targets) {
    try {
      const r = await evaluateClockFor(p);
      checks[p.id] = r;
      if (r.silencePass && r.loudnessPass) passed.add(p.id);
    } catch (e) {
      checks[p.id] = { error: String(e) };
    }
    clockProgress = { done: clockProgress.done + 1, total: targets.length };
    clockChecks = { ...checks };
  }
  matchedPairs = matchedPairs.map(p => (passed.has(p.id) ? { ...p, clockEnabled: true } : p));
  clockChecks = checks;
  clockRunning = false;
  regenerateNames();
}

// A name the user has edited is theirs: it survives every recompute (NORM,
// Clock, QC changes, format) — only the extension follows the format.
function updatePairFilename(pairId, filename) {
  matchedPairs = matchedPairs.map(p => {
    if (p.id === pairId) {
      return { ...p, outputFilename: filename, nameCustomized: true };
    }
    return p;
  });
}

// ── Naming rules ────────────────────────────────────────────────────────────
// Batch rule (NAME ALL BY): 'smart' (the blended default) | 'audio' | 'video'.
// Persisted. Per-file NAME FROM can also apply 'bump' (v3 → v4) and 'smart'.
let nameRule = $state(loadPref('nameRule', 'smart'));

function extFor(p) {
  return p.video ? getOutputExtension() : p.audio.extension;
}

// Apply the batch rule to pairs the user hasn't renamed by hand.
function applyBatchRule(pairs) {
  if (nameRule === 'smart') return pairs;
  return pairs.map(p => {
    if (p.nameCustomized) return p;
    const name = nameForRule(p, nameRule, extFor(p));
    return name ? { ...p, outputFilename: name } : p;
  });
}

function setNameRule(rule) {
  nameRule = rule;
  savePref('nameRule', rule);
  // The rule is the batch's word: it overrides earlier per-file names.
  matchedPairs = matchedPairs.map(p => ({ ...p, nameCustomized: false }));
  regenerateNames();
}

// Per-file NAME FROM. 'smart' hands the name back to the namer; anything else
// sets it and makes it sticky.
function applyNameRule(pairId, rule) {
  if (rule === 'smart') {
    matchedPairs = matchedPairs.map(p => p.id === pairId ? { ...p, nameCustomized: false } : p);
    regenerateNames();
    return;
  }
  matchedPairs = matchedPairs.map(p => {
    if (p.id !== pairId) return p;
    const name = nameForRule(p, rule, extFor(p));
    return name ? { ...p, outputFilename: name, nameCustomized: true } : p;
  });
}

function toggleAllNorm() {
  const allEnabled = matchedPairs.every(p => p.normalizationEnabled);
  const enable = !allEnabled;
  // NORM ALL normalises everything to the batch QC threshold — so the target it
  // uses (and the spec in the filename) matches what QC checks against.
  matchedPairs = matchedPairs.map(p => ({
    ...p,
    normalizationEnabled: enable,
    normalizationSettings: enable
      ? batchNormSettings(p.normalizationSettings)
      : p.normalizationSettings,
  }));
  regenerateNames();
}

function removePair(pairId) {
  const pairToRemove = matchedPairs.find(p => p.id === pairId);
  matchedPairs = matchedPairs.filter(p => p.id !== pairId);
  // Also remove the associated files so they don't get re-matched
  if (pairToRemove) {
    const pathsToRemove = new Set();
    if (pairToRemove.video) pathsToRemove.add(pairToRemove.video.path);
    if (pairToRemove.audio) pathsToRemove.add(pairToRemove.audio.path);
    files = files.filter(f => !pathsToRemove.has(f.path));
  }
}

function clearAll() {
  files = [];
  matchedPairs = [];
  processingResults = [];
  progressMap = {};
  errors = [];
  // QC/clock verdicts describe the files that were just cleared.
  qcResults = {};
  clockChecks = {};
  qcProgress = { done: 0, total: 0 };
  clockProgress = { done: 0, total: 0 };
  // The slate belongs to the batch that was just cleared.
  batchSlate = { text: '', duration: 5 };
  slateEditor = null;
  soloSlateStatus = {};
}

function dismissError(index) {
  errors = errors.filter((_, i) => i !== index);
}

async function revealInFinder(path) {
  try {
    await invoke('reveal_in_finder', { path });
  } catch (e) {
    errors = [...errors, `Could not reveal file: ${e}`];
  }
}

// Transcode a video to a ProRes .mov working file (for Pro Tools).
// Returns the output path; throws on failure so the caller can show an error.
async function createProres(videoPath, durationSecs, profile) {
  try {
    return await invoke('create_prores', { videoPath, durationSecs, profile });
  } catch (e) {
    errors = [...errors, `ProRes export failed: ${e}`];
    throw e;
  }
}

export function getAppState() {
  return {
    get files() { return files; },
    get matchedPairs() { return matchedPairs; },
    get ffmpegStatus() { return ffmpegStatus; },
    get isScanning() { return isScanning; },
    get isProcessing() { return isProcessing; },
    get processingResults() { return processingResults; },
    get progressMap() { return progressMap; },
    get errors() { return errors; },
    get exportSettings() { return exportSettings; },
    set exportSettings(v) { exportSettings = v; savePref('exportSettings', v); },
    get namingSettings() { return namingSettings; },
    set namingSettings(v) { namingSettings = v; },
    getOutputExtension,
    updateOutputExtensions,
    getVideos,
    getAudios,
    checkFfmpeg,
    scanFiles,
    autoMatch,
    regenerateNames,
    processAll,
    runMainAction,
    get nameRule() { return nameRule; },
    setNameRule,
    applyNameRule,
    cancelProcessing,
    get lengthPrompt() { return lengthPrompt; },
    resolveLengthFix,
    cancelLengthFix,
    get startPrompt() { return startPrompt; },
    resolveAudioStart,
    cancelAudioStart,
    get slateEditor() { return slateEditor; },
    get soloSlateStatus() { return soloSlateStatus; },
    openSlateEditor,
    closeSlateEditor,
    applySlate,
    removeSlate,
    updateSoloSlateProgress,
    updateProgress,
    updatePairNormalization,
    updatePairCompliance,
    updatePairClock,
    get qcTargetLufs() { return qcTargetLufs; },
    get qcTruePeak() { return qcTruePeak; },
    get qcMode() { return qcMode; },
    get qcCheckSilence() { return qcCheckSilence; },
    get qcResults() { return qcResults; },
    get qcRunning() { return qcRunning; },
    get qcProgress() { return qcProgress; },
    setQcTargetLufs,
    setQcTruePeak,
    setQcMode,
    setQcCheckSilence,
    runBatchQc,
    normalizeAllNow,
    clockAllNow,
    sixFrAllNow,
    get clockChecks() { return clockChecks; },
    get clockRunning() { return clockRunning; },
    get clockProgress() { return clockProgress; },
    runClockCheck,
    updatePairFilename,
    removePair,
    toggleAllNorm,
    clearAll,
    dismissError,
    revealInFinder,
    createProres,
  };
}
