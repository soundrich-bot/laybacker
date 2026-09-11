<script>
  import { onMount } from 'svelte';
  import { listen } from '@tauri-apps/api/event';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { open as openDialog } from '@tauri-apps/plugin-dialog';

  // Save to…: pick one folder for every output. Off by default (outputs land
  // beside the audio); remembered with the other export settings.
  async function chooseOutputDir() {
    const dir = await openDialog({ directory: true, multiple: false, title: 'Save outputs to…' });
    if (!dir) return;
    handleSettingsChange({ ...app.exportSettings, outputDirectory: dir, useAudioFileLocation: false });
  }
  function clearOutputDir() {
    handleSettingsChange({ ...app.exportSettings, outputDirectory: null, useAudioFileLocation: true });
  }

  // Corner grip: a visible cue that the window resizes, and it works — dragging
  // it hands off to the OS resize (the window's own edges still work too).
  function startResize(e) {
    if (e.button !== 0) return;
    e.preventDefault();
    getCurrentWindow().startResizeDragging('SouthEast').catch(() => { /* edge resize still available */ });
  }
  import { getAppState } from './lib/stores/app.svelte.js';

  import Header from './lib/components/Header.svelte';
  import DropZone from './lib/components/DropZone.svelte';
  import MatchedPairsList from './lib/components/MatchedPairsList.svelte';
  import AudioPage from './lib/components/AudioPage.svelte';
  import AudioModePrompt from './lib/components/AudioModePrompt.svelte';
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

  <DropZone onFilesDropped={handleFilesDropped} isScanning={app.isScanning} {isDraggingOver} compact={app.appMode === 'audio'} />

  {#if app.appMode === 'audio'}
  <AudioPage
    pairs={app.matchedPairs}
    progressMap={app.progressMap}
    results={app.processingResults}
    onUpdateNormalization={app.updatePairNormalization}
    onUpdateCompliance={app.updatePairCompliance}
    onUpdateClock={app.updatePairClock}
    onUpdateFilename={app.updatePairFilename}
    onRemove={app.removePair}
    onReveal={app.revealInFinder}
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
    onApplyNameRule={app.applyNameRule}
    nameRule={app.nameRule}
    onNameRuleChange={app.setNameRule}
    isProcessing={app.isProcessing}
    clockChecks={app.clockChecks}
    clockRunning={app.clockRunning}
    clockProgress={app.clockProgress}
    onRunClockCheck={app.runClockCheck}
    onBackToLayback={() => app.setAppMode('layback')}
    onSplitAll={app.splitAllNow}
    onJoinAll={app.joinMonosNow}
    joinableGroups={app.joinableGroups}
    onConvertAll={app.convertAllNow}
    conversionSet={app.conversionSet}
    conversionLabel={app.conversionLabel}
    onProcessAll={app.processAudioAllNow}
  />
  {:else}
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
    onApplyNameRule={app.applyNameRule}
    nameRule={app.nameRule}
    onNameRuleChange={app.setNameRule}
    soloSlateStatus={app.soloSlateStatus}
    isProcessing={app.isProcessing}
    clockChecks={app.clockChecks}
    clockRunning={app.clockRunning}
    clockProgress={app.clockProgress}
    onRunClockCheck={app.runClockCheck}
  />
  {/if}

  <SettingsPanel
    settings={app.exportSettings}
    onSettingsChange={handleSettingsChange}
    {tameMode}
    onToggleTame={toggleTame}
    {timestampFormat}
    onTimestampFormatChange={(fmt) => { timestampFormat = fmt; localStorage.setItem('timestampFormat', fmt); }}
    onChooseOutputDir={chooseOutputDir}
    onClearOutputDir={clearOutputDir}
    defaultNameRule={app.defaultNameRule}
    onDefaultNameRuleChange={app.setDefaultNameRule}
    {proresProfile}
    onProresProfileChange={(p) => { proresProfile = p; localStorage.setItem('proresProfile', p); }}
    audioOnly={app.matchedPairs.length > 0 && app.matchedPairs.every(p => !p.video)}
    audioPage={app.appMode === 'audio'}
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

{#if app.audioModePrompt}
  <AudioModePrompt count={app.getAudios().length} onChoose={app.chooseAudioMode} />
{/if}

{#if app.startPrompt}
  <AudioStartModal
    prompt={app.startPrompt}
    onChoose={app.resolveAudioStart}
    onCancel={app.cancelAudioStart}
  />
{/if}

<!-- Resize grip: the classic three diagonal lines, bottom-right -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="resize-grip" onmousedown={startResize} title="Drag to resize the window" aria-hidden="true">
  <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
    <path d="M13 1L1 13M13 6L6 13M13 11L11 13" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"/>
  </svg>
</div>

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
    assets={app.slateAssets}
    onPickImage={app.pickSlateImage}
    onCancel={app.closeSlateEditor}
  />
{/if}

<style>
  .resize-grip {
    position: fixed;
    right: 4px;
    bottom: 4px;
    z-index: 900;
    color: var(--text-muted);
    opacity: 0.55;
    cursor: nwse-resize;
    display: flex;
    padding: 3px;
    transition: opacity 0.15s, color 0.15s;
    user-select: none;
    -webkit-user-select: none;
  }
  .resize-grip:hover {
    opacity: 1;
    color: var(--neon-cyan);
  }

  .app-container {
    height: 100%;
    display: flex;
    flex-direction: column;
    position: relative;
  }
</style>
