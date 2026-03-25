<script>
  let { onFilesDropped, isScanning = false, isDraggingOver = false } = $props();
</script>

<div
  class="drop-zone"
  class:dragging={isDraggingOver}
  class:scanning={isScanning}
  role="region"
  aria-label="File drop zone"
  title="Drop any combination of video and audio files — Laybacker will sort them by duration and pair them automatically"
>
  <div class="drop-content">
    {#if isScanning}
      <div class="spinner"></div>
      <p class="drop-text">SCANNING FILES...</p>
    {:else if isDraggingOver}
      <div class="drop-icon active">
        <svg width="48" height="48" viewBox="0 0 48 48" fill="none">
          <rect x="4" y="8" width="40" height="32" rx="4" stroke="currentColor" stroke-width="2.5"/>
          <path d="M24 16V32M16 24H32" stroke="currentColor" stroke-width="3" stroke-linecap="round"/>
        </svg>
      </div>
      <p class="drop-text">LET GO!</p>
    {:else}
      <div class="drop-icon">
        <svg width="48" height="48" viewBox="0 0 48 48" fill="none">
          <rect x="4" y="8" width="40" height="32" rx="4" stroke="currentColor" stroke-width="2" stroke-dasharray="4 4"/>
          <path d="M24 18V30M18 24H30" stroke="currentColor" stroke-width="2.5" stroke-linecap="round"/>
        </svg>
      </div>
      <p class="drop-text">DROP VIDEO & AUDIO FILES</p>
      <p class="drop-hint">
        Drop one at a time or all at once — we'll sort and pair them for you
      </p>
      <p class="drop-formats">
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
    min-height: 140px;
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
    transform: scale(1.01);
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
  }

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
    font-size: 16px;
    letter-spacing: 0.15em;
    color: var(--text-secondary);
    margin-bottom: var(--gap-xs);
  }

  .dragging .drop-text {
    color: var(--neon-cyan);
    text-shadow: 0 0 10px rgba(8, 247, 254, 0.5);
  }

  .drop-hint {
    font-family: var(--font-body);
    font-size: 12px;
    color: var(--text-secondary);
    margin-bottom: var(--gap-xs);
  }

  .drop-formats {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--text-muted);
    letter-spacing: 0.05em;
  }

  .spinner {
    width: 32px;
    height: 32px;
    border: 3px solid var(--border-accent);
    border-top-color: var(--neon-yellow);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
    margin: 0 auto var(--gap-sm);
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }
</style>
