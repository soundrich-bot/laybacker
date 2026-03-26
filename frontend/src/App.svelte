<script>
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { getAppState } from './lib/stores/app.svelte.js';

  import Header from './lib/components/Header.svelte';
  import DropZone from './lib/components/DropZone.svelte';
  import MatchedPairsList from './lib/components/MatchedPairsList.svelte';
  import SettingsPanel from './lib/components/SettingsPanel.svelte';
  import ProcessButton from './lib/components/ProcessButton.svelte';
  import ErrorBar from './lib/components/ErrorBar.svelte';

  const app = getAppState();
  let isDraggingOver = $state(false);
  let tameMode = $state(localStorage.getItem('tameMode') === 'true');
  let timestampFormat = $state(localStorage.getItem('timestampFormat') || 'YYYYMMDD_HHmm');

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
    console.log('[mount] starting...');

    // Resolve bundled completion sound
    try {
      const soundPath = await invoke('get_resource_path', { resource: 'resources/LaybackComplete.wav' });
      app.completionSoundPath = soundPath;
      console.log('[mount] sound path resolved');
    } catch { /* sound is optional */ }

    // Check for FFmpeg on startup
    await app.checkFfmpeg();
    console.log('[mount] ffmpeg checked');

    // Listen for processing progress events from Rust backend
    await listen('processing-progress', (event) => {
      app.updateProgress(event.payload);
    });
    console.log('[mount] progress listener set');

    // Listen for native Tauri drag-drop events (gives us full file paths)
    const currentWindow = getCurrentWindow();
    console.log('[mount] got window, setting up drag-drop...');

    await currentWindow.onDragDropEvent((event) => {
      console.log('[drop] event:', event.payload.type);
      if (event.payload.type === 'over') {
        isDraggingOver = true;
      } else if (event.payload.type === 'drop') {
        isDraggingOver = false;
        const paths = event.payload.paths;
        console.log('[drop] paths:', paths);
        if (paths && paths.length > 0) {
          app.scanFiles(paths);
        }
      } else if (event.payload.type === 'leave') {
        isDraggingOver = false;
      }
    });
    console.log('[mount] drag-drop listener set — ready!');
  });

  function handleFilesDropped(paths) {
    app.scanFiles(paths);
  }

  function handleSettingsChange(newSettings) {
    app.exportSettings = newSettings;
    // Regenerate names when output format changes
    app.regenerateNames();
  }
</script>

<div class="app-container">
  <div class="noise-overlay"></div>

  <Header ffmpegStatus={app.ffmpegStatus} />

  <ErrorBar errors={app.errors} onDismiss={app.dismissError} />

  <DropZone onFilesDropped={handleFilesDropped} isScanning={app.isScanning} {isDraggingOver} />

  <MatchedPairsList
    pairs={app.matchedPairs}
    progressMap={app.progressMap}
    results={app.processingResults}
    videoCount={app.getVideos().length}
    audioCount={app.getAudios().length}
    onUpdateNormalization={app.updatePairNormalization}
    onUpdateFilename={app.updatePairFilename}
    onRemove={app.removePair}
    onReveal={app.revealInFinder}
    onToggleAllNorm={app.toggleAllNorm}
    {timestampFormat}
  />

  <SettingsPanel
    settings={app.exportSettings}
    onSettingsChange={handleSettingsChange}
    {tameMode}
    onToggleTame={toggleTame}
    {timestampFormat}
    onTimestampFormatChange={(fmt) => { timestampFormat = fmt; localStorage.setItem('timestampFormat', fmt); }}
  />

  <ProcessButton
    pairCount={app.matchedPairs.length}
    isProcessing={app.isProcessing}
    onProcess={app.processAll}
    onCancel={app.cancelProcessing}
    onClear={app.clearAll}
  />
</div>

<style>
  .app-container {
    height: 100%;
    display: flex;
    flex-direction: column;
    position: relative;
  }
</style>
