<script>
  import { onMount } from 'svelte';
  import { listen } from '@tauri-apps/api/event';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { getAppState } from './lib/stores/app.svelte.js';

  import Header from './lib/components/Header.svelte';
  import DropZone from './lib/components/DropZone.svelte';
  import MatchedPairsList from './lib/components/MatchedPairsList.svelte';
  import SettingsPanel from './lib/components/SettingsPanel.svelte';
  import ProcessButton from './lib/components/ProcessButton.svelte';
  import ErrorBar from './lib/components/ErrorBar.svelte';
  import UpdateBanner from './lib/components/UpdateBanner.svelte';
  import LengthFixModal from './lib/components/LengthFixModal.svelte';
  import SlateEditor from './lib/components/SlateEditor.svelte';
  import AudioStartModal from './lib/components/AudioStartModal.svelte';

  const app = getAppState();
  let isDraggingOver = $state(false);
  let tameMode = $state(localStorage.getItem('tameMode') === 'true');
  let timestampFormat = $state(localStorage.getItem('timestampFormat') || 'YYYYMMDD_HHmm');
  let proresProfile = $state(localStorage.getItem('proresProfile') || 'lt');

  // Apply saved tame mode on load
  if (tameMode) document.documentElement.classList.add('tame');

  function toggleTame() {
    tameMode = !tameMode;
    localStorage.setItem('tameMode', tameMode);
    if (tameMode) {
      document.documentElement.classList.add('tame');
    } else {
      document.documentElement.classList.remove('tame');
    }
  }

  onMount(async () => {
    // Check for FFmpeg on startup
    await app.checkFfmpeg();

    // Listen for processing progress events from Rust backend
    await listen('processing-progress', (event) => {
      app.updateProgress(event.payload);
    });

    // Solo-video slate encode progress (video dropped without audio)
    await listen('slate-progress', (event) => {
      app.updateSoloSlateProgress(event.payload);
    });

    // Listen for native Tauri drag-drop events (gives us full file paths)
    const currentWindow = getCurrentWindow();

    await currentWindow.onDragDropEvent((event) => {
      if (event.payload.type === 'over') {
        isDraggingOver = true;
      } else if (event.payload.type === 'drop') {
        isDraggingOver = false;
        const paths = event.payload.paths;
        if (paths && paths.length > 0) {
          app.scanFiles(paths);
        }
      } else if (event.payload.type === 'leave') {
        isDraggingOver = false;
      }
    });
  });

  function handleFilesDropped(paths) {
    app.scanFiles(paths);
  }

  function handleSettingsChange(newSettings) {
    app.exportSettings = newSettings;
    // The container extension follows the format, but the NAME is the user's —
    // they may have edited it, so only the extension is swapped, never the name.
    app.updateOutputExtensions();
  }
</script>

<div class="app-container">
  <div class="noise-overlay"></div>

  <Header ffmpegStatus={app.ffmpegStatus} />

  <UpdateBanner />

  <ErrorBar errors={app.errors} onDismiss={app.dismissError} />

  <DropZone onFilesDropped={handleFilesDropped} isScanning={app.isScanning} {isDraggingOver} />

  <MatchedPairsList
    pairs={app.matchedPairs}
    progressMap={app.progressMap}
    results={app.processingResults}
    videos={app.getVideos()}
    videoCount={app.getVideos().length}
    audioCount={app.getAudios().length}
    onUpdateNormalization={app.updatePairNormalization}
    onUpdateCompliance={app.updatePairCompliance}
    onUpdateClock={app.updatePairClock}
    onUpdateFilename={app.updatePairFilename}
    onRemove={app.removePair}
    onReveal={app.revealInFinder}
    onCreateProres={(videoPath, durationSecs) => app.createProres(videoPath, durationSecs, proresProfile)}
    onToggleAllNorm={app.toggleAllNorm}
    {timestampFormat}
    qcTargetLufs={app.qcTargetLufs}
    qcTruePeak={app.qcTruePeak}
    qcMode={app.qcMode}
    qcCheckSilence={app.qcCheckSilence}
    qcResults={app.qcResults}
    qcRunning={app.qcRunning}
    qcProgress={app.qcProgress}
    onQcTargetChange={app.setQcTargetLufs}
    onQcTruePeakChange={app.setQcTruePeak}
    onQcModeChange={app.setQcMode}
    onQcSilenceChange={app.setQcCheckSilence}
    onRunQc={app.runBatchQc}
    onNormalizeAll={app.normalizeAllNow}
    onClockAll={app.clockAllNow}
    onSixFrAll={app.sixFrAllNow}
    onOpenSlate={app.openSlateEditor}
    soloSlateStatus={app.soloSlateStatus}
    isProcessing={app.isProcessing}
    clockChecks={app.clockChecks}
    clockRunning={app.clockRunning}
    clockProgress={app.clockProgress}
    onRunClockCheck={app.runClockCheck}
  />

  <SettingsPanel
    settings={app.exportSettings}
    onSettingsChange={handleSettingsChange}
    {tameMode}
    onToggleTame={toggleTame}
    {timestampFormat}
    onTimestampFormatChange={(fmt) => { timestampFormat = fmt; localStorage.setItem('timestampFormat', fmt); }}
    {proresProfile}
    onProresProfileChange={(p) => { proresProfile = p; localStorage.setItem('proresProfile', p); }}
    audioOnly={app.matchedPairs.length > 0 && app.matchedPairs.every(p => !p.video)}
  />

  <ProcessButton
    pairCount={app.matchedPairs.length}
    fileCount={app.files.length}
    isProcessing={app.isProcessing}
    onProcess={app.runMainAction}
    onCancel={app.cancelProcessing}
    onClear={app.clearAll}
    audioOnly={app.matchedPairs.length > 0 && app.matchedPairs.every(p => !p.video)}
    clockOnly={app.matchedPairs.length > 0
      && app.matchedPairs.every(p => !p.video)
      && app.matchedPairs.some(p => p.clockEnabled)
      && !app.matchedPairs.some(p => p.normalizationEnabled)}
  />
</div>

{#if app.lengthPrompt}
  <LengthFixModal
    prompt={app.lengthPrompt}
    onChoose={app.resolveLengthFix}
    onCancel={app.cancelLengthFix}
  />
{/if}

{#if app.startPrompt}
  <AudioStartModal
    prompt={app.startPrompt}
    onChoose={app.resolveAudioStart}
    onCancel={app.cancelAudioStart}
  />
{/if}

{#if app.slateEditor}
  <SlateEditor
    editor={app.slateEditor}
    isBatch={app.slateEditor.scope === 'batch'}
    isSolo={app.slateEditor.scope === 'solo'}
    videoCount={app.matchedPairs.filter(p => p.video).length}
    onApply={app.applySlate}
    onRemove={(app.slateEditor.scope === 'batch'
      ? app.matchedPairs.some(p => p.video && p.slateEnabled)
      : app.matchedPairs.find(p => p.id === app.slateEditor.scope)?.slateEnabled)
      ? app.removeSlate : null}
    onCancel={app.closeSlateEditor}
  />
{/if}

<style>
  .app-container {
    height: 100%;
    display: flex;
    flex-direction: column;
    position: relative;
  }
</style>
