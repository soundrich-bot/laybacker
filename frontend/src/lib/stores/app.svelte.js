import { invoke } from '@tauri-apps/api/core';

// Reactive state using Svelte 5 runes
let files = $state([]);
let matchedPairs = $state([]);
let ffmpegStatus = $state({ available: false, version: '' });
let isScanning = $state(false);
let isProcessing = $state(false);
let processingResults = $state([]);
let progressMap = $state({});
let errors = $state([]);

// ── Batch QC ────────────────────────────────────────────────────────────────
// One spec for the whole batch: a loudness value (which is ALSO every pair's
// NORM target, so QC and the export agree) plus an optional 6-frame silence
// check. Results are transient — they describe the source files, not the export.
// The batch value IS the loudness target — the one number QC checks against and
// NORM corrects to. Loudness is the headline metric; the true-peak ceiling
// (-1 dBTP) is a background check, deemphasised in the UI and the naming.
let qcTargetLufs = $state(-23);
let qcCheckSilence = $state(true);
let qcResults = $state({}); // { [pairId]: { pass, lufsPass, peakPass, silencePass, measuredLufs, measuredTP, headHasAudio, tailHasAudio, error? } }
let qcRunning = $state(false);
let qcProgress = $state({ done: 0, total: 0 });

// Changing the batch loudness value retargets every pair's NORM and voids any
// existing results (they were measured against a different spec).
function setQcTargetLufs(value) {
  qcTargetLufs = value;
  matchedPairs = matchedPairs.map(p => ({
    ...p,
    normalizationSettings: { ...p.normalizationSettings, targetLufs: value },
  }));
  qcResults = {};
  clockChecks = {}; // clock verdicts were judged against the old target
  regenerateNames(); // the spec in the filename follows the target
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
      normalizationSettings: { ...p.normalizationSettings, targetLufs: qcTargetLufs },
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
      const lufsPass = Math.abs(measuredLufs - qcTargetLufs) <= 1.0;
      const peakPass = measuredTP <= peakLimit + 0.05; // ceiling, not a target
      const silencePass = qcCheckSilence ? (!headHasAudio && !tailHasAudio) : true;
      results[p.id] = {
        pass: lufsPass && peakPass && silencePass,
        lufsPass, peakPass, silencePass,
        measuredLufs, measuredTP, headHasAudio, tailHasAudio, peakLimit,
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
function getOutputExtension() {
  return exportSettings.audioFormat === 'original' ? 'mov' : 'mp4';
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
        normalizationSettings: { targetLufs: qcTargetLufs, truePeakLimit: -1.0 },
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

    // The batch loudness value is THE target — every pair carries it, including
    // files dropped later, so QC and the export can never disagree.
    pairs = pairs.map(p => ({
      ...p,
      normalizationSettings: { ...p.normalizationSettings, targetLufs: qcTargetLufs },
    }));

    matchedPairs = pairs;
  } catch (e) {
    errors = [...errors, `Matching failed: ${e}`];
  }
}

async function regenerateNames() {
  if (matchedPairs.length === 0) return;
  try {
    const pairs = await invoke('generate_names', {
      pairs: matchedPairs,
      removeDuplicates: namingSettings.removeDuplicates,
      outputExtension: getOutputExtension(),
    });
    matchedPairs = pairs;
  } catch (e) {
    errors = [...errors, `Naming failed: ${e}`];
  }
}

async function cancelProcessing() {
  await invoke('cancel_processing');
}

async function processAll() {
  if (matchedPairs.length === 0) return;
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

function updatePairFilename(pairId, filename) {
  matchedPairs = matchedPairs.map(p => {
    if (p.id === pairId) {
      return { ...p, outputFilename: filename };
    }
    return p;
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
      ? { ...p.normalizationSettings, targetLufs: qcTargetLufs }
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
    set exportSettings(v) { exportSettings = v; },
    get namingSettings() { return namingSettings; },
    set namingSettings(v) { namingSettings = v; },
    getOutputExtension,
    getVideos,
    getAudios,
    checkFfmpeg,
    scanFiles,
    autoMatch,
    regenerateNames,
    processAll,
    cancelProcessing,
    updateProgress,
    updatePairNormalization,
    updatePairCompliance,
    updatePairClock,
    get qcTargetLufs() { return qcTargetLufs; },
    get qcCheckSilence() { return qcCheckSilence; },
    get qcResults() { return qcResults; },
    get qcRunning() { return qcRunning; },
    get qcProgress() { return qcProgress; },
    setQcTargetLufs,
    setQcCheckSilence,
    runBatchQc,
    normalizeAllNow,
    clockAllNow,
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
