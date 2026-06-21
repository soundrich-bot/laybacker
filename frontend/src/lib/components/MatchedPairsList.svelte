<script>
  import MatchedPairRow from './MatchedPairRow.svelte';
  import ProResButton from './ProResButton.svelte';

  let {
    pairs = [],
    progressMap = {},
    results = [],
    videos = [],
    videoCount = 0,
    audioCount = 0,
    onUpdateNormalization,
    onUpdateCompliance,
    onUpdateFilename,
    onRemove,
    onReveal,
    onCreateProres,
    onToggleAllNorm,
    timestampFormat = 'YYYYMMDD_HHmm',
  } = $props();

  let allNormEnabled = $derived(pairs.length > 0 && pairs.every(p => p.normalizationEnabled));
  let someNormEnabled = $derived(pairs.some(p => p.normalizationEnabled));

  function getResult(pairId) {
    return results.find(r => r.pairId === pairId) ?? null;
  }
</script>

<div class="pairs-section">
  {#if pairs.length === 0}
    <div class="empty-state" class:waiting={videoCount > 0 || audioCount > 0}>
      {#if videoCount > 0 && audioCount === 0}
        <div class="waiting-icon">
          <svg width="32" height="32" viewBox="0 0 32 32" fill="none">
            <rect x="2" y="6" width="28" height="20" rx="3" stroke="currentColor" stroke-width="1.5"/>
            <path d="M13 12L19 16L13 20V12Z" fill="currentColor"/>
          </svg>
          <span class="checkmark">✓</span>
        </div>
        <p class="waiting-text">{videoCount} VIDEO{videoCount !== 1 ? 'S' : ''} LOADED</p>
        <p class="waiting-hint">Drop audio to lay back — or make a ProRes working file for Pro Tools</p>
        <div class="video-prores-list">
          {#each videos as v (v.path)}
            <div class="video-prores-item">
              <div class="vp-thumb">
                {#if v.thumbnailData}
                  <img src={v.thumbnailData} alt="" />
                {:else}
                  <svg width="16" height="12" viewBox="0 0 16 12" fill="none">
                    <rect x="1" y="1" width="14" height="10" rx="1.5" stroke="currentColor" stroke-width="1.2"/>
                    <path d="M6 4L10 6L6 8V4Z" fill="currentColor"/>
                  </svg>
                {/if}
              </div>
              <span class="vp-name" title={v.filename}>{v.filename}</span>
              <ProResButton videoPath={v.path} durationSecs={v.durationSecs} {onCreateProres} {onReveal} />
            </div>
          {/each}
        </div>
      {:else if audioCount > 0 && videoCount === 0}
        <div class="waiting-icon">
          <svg width="32" height="32" viewBox="0 0 32 32" fill="none">
            <path d="M16 4C16 4 18 4 18 7V25C18 28 16 28 16 28C16 28 14 28 14 25V7C14 4 16 4 16 4Z" stroke="currentColor" stroke-width="1.5"/>
            <path d="M10 14V20C10 23.3 12.7 26 16 26C19.3 26 22 23.3 22 20V14" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
            <path d="M16 26V30" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
          </svg>
          <span class="checkmark">✓</span>
        </div>
        <p class="waiting-text">{audioCount} AUDIO FILE{audioCount !== 1 ? 'S' : ''} LOADED</p>
        <p class="waiting-hint">Now drop a video file to pair up</p>
      {:else}
        <p class="empty-text">READY TO GO</p>
        <p class="empty-hint">Drop some files above to get started</p>
      {/if}
    </div>
  {:else}
    <div class="pairs-header">
      {#if pairs.some(p => !p.video)}
        <span class="pairs-count">{pairs.length} AUDIO FILE{pairs.length !== 1 ? 'S' : ''} READY</span>
      {:else}
        <span class="pairs-count">{pairs.length} LAYBACK{pairs.length !== 1 ? 'S' : ''} READY</span>
      {/if}
      <div class="column-labels">
        {#if pairs.every(p => p.video)}
          <span class="col-label col-video">VIDEO</span>
        {/if}
        <span class="col-label col-audio">AUDIO</span>
        <button
          class="col-norm-btn"
          class:active={allNormEnabled}
          class:partial={someNormEnabled && !allNormEnabled}
          onclick={onToggleAllNorm}
          title={allNormEnabled ? "Disable normalization on all" : "Enable normalization on all"}
        >
          NORM ALL
        </button>
      </div>
    </div>
    <div class="pairs-list">
      {#each pairs as pair (pair.id)}
        <MatchedPairRow
          {pair}
          progress={progressMap[pair.id] ?? null}
          result={getResult(pair.id)}
          {onUpdateNormalization}
          {onUpdateCompliance}
          {onUpdateFilename}
          {onRemove}
          {onReveal}
          {onCreateProres}
          {timestampFormat}
        />
      {/each}
    </div>
  {/if}
</div>

<style>
  .pairs-section {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    padding: 0 var(--gap-lg);
  }

  .pairs-header {
    padding: var(--gap-sm) 0;
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    gap: var(--gap-xs);
  }

  .pairs-count {
    font-family: var(--font-display);
    font-size: 12px;
    letter-spacing: 0.15em;
    color: var(--neon-cyan);
  }

  .column-labels {
    display: flex;
    align-items: center;
    padding: 0 var(--gap-md);
    gap: var(--gap-sm);
  }

  .col-label {
    font-family: var(--font-display);
    font-size: 10px;
    letter-spacing: 0.15em;
    color: var(--text-muted);
    opacity: 0.6;
  }

  .col-video {
    flex: 1;
    padding-left: 70px; /* thumbnail width + gap */
  }

  .col-audio {
    flex: 1;
    padding-left: 30px; /* arrow gap */
  }

  .col-norm-btn {
    font-family: var(--font-display);
    font-size: 10px;
    letter-spacing: 0.1em;
    color: var(--text-muted);
    background: none;
    border: 1px solid var(--border-color);
    border-radius: var(--radius-sm);
    padding: 2px 8px;
    cursor: pointer;
    transition: all 0.15s;
    flex-shrink: 0;
    opacity: 0.6;
  }

  .col-norm-btn:hover {
    opacity: 1;
    border-color: var(--neon-yellow);
    color: var(--neon-yellow);
  }

  .col-norm-btn.partial {
    opacity: 0.8;
    color: var(--neon-yellow);
    border-color: rgba(237, 255, 33, 0.3);
  }

  .col-norm-btn.active {
    opacity: 1;
    color: var(--bg-dark);
    background: var(--neon-yellow);
    border-color: var(--neon-yellow);
    box-shadow: 0 0 6px rgba(237, 255, 33, 0.2);
  }

  .pairs-list {
    flex: 1;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: var(--gap-sm);
    padding-bottom: var(--gap-md);
  }

  .empty-state {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    opacity: 0.5;
  }

  .empty-state.waiting {
    opacity: 1;
  }

  .empty-text {
    font-family: var(--font-display);
    font-size: 15px;
    letter-spacing: 0.15em;
    color: var(--text-muted);
    margin-bottom: var(--gap-xs);
  }

  .empty-hint {
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--text-muted);
  }

  .waiting-icon {
    position: relative;
    color: var(--neon-cyan);
    margin-bottom: var(--gap-md);
    animation: pulse-glow 2s ease-in-out infinite;
  }

  .checkmark {
    position: absolute;
    bottom: -2px;
    right: -6px;
    font-size: 15px;
    color: var(--neon-green);
    text-shadow: 0 0 6px rgba(57, 255, 20, 0.5);
  }

  :global(:root.tame) .checkmark {
    text-shadow: none;
  }

  .waiting-text {
    font-family: var(--font-display);
    font-size: 19px;
    letter-spacing: 0.15em;
    color: var(--neon-cyan);
    margin-bottom: var(--gap-xs);
    text-shadow: 0 0 12px rgba(8, 247, 254, 0.3);
  }

  :global(:root.tame) .waiting-text {
    text-shadow: none;
  }

  .waiting-hint {
    font-family: var(--font-mono);
    font-size: 13px;
    color: var(--text-secondary);
    letter-spacing: 0.03em;
  }

  .video-prores-list {
    display: flex;
    flex-direction: column;
    gap: var(--gap-sm);
    margin-top: var(--gap-lg);
    width: 100%;
    max-width: 460px;
  }

  .video-prores-item {
    display: flex;
    align-items: center;
    gap: var(--gap-sm);
    padding: 6px 10px;
    background: var(--bg-panel);
    border: 1px solid var(--border-color);
    border-radius: var(--radius-sm);
  }

  .vp-thumb {
    flex-shrink: 0;
    width: 40px;
    height: 28px;
    border-radius: 3px;
    overflow: hidden;
    background: var(--bg-dark);
    border: 1px solid var(--border-color);
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--text-muted);
  }

  .vp-thumb img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .vp-name {
    flex: 1;
    min-width: 0;
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--text-secondary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    text-align: left;
  }

  @keyframes pulse-glow {
    0%, 100% { opacity: 0.8; filter: drop-shadow(0 0 4px rgba(8, 247, 254, 0.2)); }
    50% { opacity: 1; filter: drop-shadow(0 0 10px rgba(8, 247, 254, 0.4)); }
  }
</style>
