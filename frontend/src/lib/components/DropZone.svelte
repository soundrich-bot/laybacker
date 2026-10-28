<script>
  // `big`: nothing loaded yet, so the zone takes half the window. Either way
  // its height is fixed, so the idle and LET GO states are the same size.
  let { onFilesDropped, isScanning = false, isDraggingOver = false, compact = false, big = false, audioPage = false, ask = '', hint = '', withSelector = false } = $props();
</script>

<div
  class="drop-zone"
  class:dragging={isDraggingOver}
  class:scanning={isScanning}
  class:compact
  class:big
  class:with-selector={big && withSelector}
  role="region"
  aria-label="File drop zone"
  title={compact
    ? 'Drop more audio files here — or a video to go back to the layback page'
    : 'Drop any combination of video and audio files — Laybacker will sort them by duration and pair them automatically'}
>
  <div class="drop-content">
    {#if compact}
      <!-- Audio Only page: a slim strip, the page itself needs the room -->
      <p class="drop-text">
        {#if isScanning}SCANNING…{:else if isDraggingOver}LET GO!{:else}{audioPage ? '+ DROP MORE AUDIO' : '+ DROP MORE VIDEO & AUDIO'}{/if}
      </p>
      {#if !isScanning && !isDraggingOver}
        <span class="drop-formats">{audioPage ? '.wav .aif .aiff .bwf .m4a .aac .mp3 .flac' : '.mp4 .mov .m4v .mxf · .wav .aif .bwf .m4a .mp3 .flac'}</span>
      {/if}
    {:else}
      <!-- One layout for idle, drag-over and scanning: the same four lines,
           only the words change, so nothing leaps when a file crosses the box. -->
      <div class="drop-icon" class:active={isDraggingOver}>
        {#if isScanning}
          <div class="spinner"></div>
        {:else}
          <svg width="48" height="48" viewBox="0 0 48 48" fill="none">
            <rect x="4" y="8" width="40" height="32" rx="4" stroke="currentColor" stroke-width={isDraggingOver ? 2.5 : 2} stroke-dasharray={isDraggingOver ? 'none' : '4 4'}/>
            <path d="M24 17V31M17 24H31" stroke="currentColor" stroke-width="2.5" stroke-linecap="round"/>
          </svg>
        {/if}
      </div>
      <p class="drop-text">{isScanning ? 'SCANNING FILES…' : isDraggingOver ? 'LET GO!' : (ask || 'DROP VIDEO & AUDIO FILES TO START')}</p>
      <p class="drop-hint">
        {isScanning ? 'Reading the files you dropped' : isDraggingOver ? 'Release to add them' : (hint || "Drop one at a time or all at once — we'll sort and pair them for you")}
      </p>
      <p class="drop-formats" class:hold={isDraggingOver || isScanning}>
        .mp4 .mov .m4v .mxf &bull; .wav .aif .aiff .bwf .m4a .aac .mp3 .flac &bull; or a folder
      </p>
    {/if}
  </div>
</div>

<style>
  .drop-zone {
    margin: var(--gap-md) var(--gap-lg);
    padding: var(--gap-xl);
    border: 2px dashed var(--border-accent);
    border-radius: var(--radius-lg);
    background: var(--bg-panel);
    transition: all 0.2s ease;
    flex-shrink: 0;
    height: 150px;
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: default;
  }

  .drop-zone.dragging {
    border-color: var(--neon-cyan);
    border-style: solid;
    background: rgba(8, 247, 254, 0.05);
    box-shadow:
      0 0 20px rgba(8, 247, 254, 0.1),
      inset 0 0 20px rgba(8, 247, 254, 0.05);
  }

  .drop-zone.scanning {
    border-color: var(--neon-yellow);
    border-style: solid;
  }

  .drop-content {
    text-align: center;
  }

  .drop-icon {
    color: var(--text-muted);
    margin-bottom: var(--gap-sm);
    transition: color 0.2s;
    height: 48px;
    display: flex; align-items: center; justify-content: center;
  }
  .drop-formats.hold { visibility: hidden; }

  .drop-icon.active {
    color: var(--neon-cyan);
    animation: pulse 0.6s ease-in-out infinite alternate;
  }

  @keyframes pulse {
    from { transform: scale(1); }
    to { transform: scale(1.1); }
  }

  .drop-text {
    font-family: var(--font-display);
    font-weight: 700;
    font-size: 17px;
    letter-spacing: 0.04em;
    color: var(--text-secondary);
    margin-bottom: var(--gap-xs);
  }

  .dragging .drop-text {
    color: var(--neon-cyan);
    text-shadow: 0 0 10px rgba(8, 247, 254, 0.5);
  }

  .drop-hint {
    font-family: var(--font-body);
    font-size: 13px;
    color: var(--text-secondary);
    margin-bottom: var(--gap-xs);
  }

  .drop-formats {
    font-family: var(--font-text);
    font-weight: 500;
    font-size: 11px;
    color: var(--text-muted);
    letter-spacing: 0;
  }

  .spinner {
    width: 32px;
    height: 32px;
    border: 3px solid var(--border-accent);
    border-top-color: var(--neon-yellow);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
    margin: 0 auto;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  /* Half the window while there's nothing to show beneath it */
  .drop-zone.big { height: 50vh; }
  .drop-zone.big.with-selector { height: 40vh; }

  /* Compact strip for the Audio Only page */
  .drop-zone.compact {
    height: auto;
    min-height: 0;
    padding: 6px var(--gap-md);
    margin: var(--gap-sm) var(--gap-lg) 0;
    border-width: 1px;
  }
  .drop-zone.compact .drop-content {
    display: flex;
    align-items: baseline;
    gap: 12px;
  }
  .drop-zone.compact .drop-text {
    font-size: 12.5px;
    margin: 0;
  }
  .drop-zone.compact .drop-formats {
    font-size: 11.5px;
  }
  .drop-zone.compact.dragging { transform: none; }

  :global(:root.tame) .drop-zone.dragging, :global(:root.porcelain) .drop-zone.dragging {
    background: rgba(90, 138, 122, 0.06);
    box-shadow: none;
  }

  :global(:root.tame) .dragging .drop-text, :global(:root.porcelain) .dragging .drop-text {
    text-shadow: none;
  }
</style>
