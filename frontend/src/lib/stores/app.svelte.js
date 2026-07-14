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
        normalizationEnabled: true,
        normalizationSettings: { targetLufs: 0.0, truePeakLimit: -1.0 },
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
  matchedPairs = matchedPairs.map(p =>
    p.id === pairId ? { ...p, clockEnabled: enabled } : p
  );
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
  matchedPairs = matchedPairs.map(p => ({
    ...p,
    normalizationEnabled: !allEnabled,
  }));
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
    updatePairFilename,
    removePair,
    toggleAllNorm,
    clearAll,
    dismissError,
    revealInFinder,
    createProres,
  };
}
