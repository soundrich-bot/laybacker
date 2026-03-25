<script>
  let { pairCount = 0, isProcessing = false, onProcess, onClear } = $props();
</script>

<div class="process-bar">
  <div class="process-bar-left">
    {#if pairCount > 0}
      <button class="clear-btn" onclick={onClear} disabled={isProcessing}>
        CLEAR ALL
      </button>
    {/if}
  </div>

  <button
    class="process-btn"
    class:processing={isProcessing}
    disabled={pairCount === 0 || isProcessing}
    onclick={onProcess}
  >
    {#if isProcessing}
      <span class="btn-spinner"></span>
      PROCESSING...
    {:else}
      LAYBACK {pairCount > 0 ? `(${pairCount})` : ''}
    {/if}
  </button>

  <div class="process-bar-right"></div>
</div>

<style>
  .process-bar {
    padding: var(--gap-md) var(--gap-lg);
    border-top: 1px solid var(--border-color);
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-shrink: 0;
    background: linear-gradient(0deg, var(--bg-raised) 0%, var(--bg-dark) 100%);
  }

  .process-bar-left,
  .process-bar-right {
    flex: 1;
  }

  .clear-btn {
    font-family: var(--font-mono);
    font-size: 10px;
    letter-spacing: 0.1em;
    color: var(--text-muted);
    background: none;
    border: 1px solid var(--border-color);
    border-radius: var(--radius-sm);
    padding: 6px 14px;
    cursor: pointer;
    transition: all 0.15s;
  }

  .clear-btn:hover:not(:disabled) {
    color: var(--neon-pink);
    border-color: var(--neon-pink);
  }

  .clear-btn:disabled {
    opacity: 0.3;
    cursor: not-allowed;
  }

  .process-btn {
    font-family: var(--font-display);
    font-size: 16px;
    letter-spacing: 0.2em;
    color: var(--bg-dark);
    background: var(--neon-green);
    border: none;
    border-radius: var(--radius-md);
    padding: 12px 48px;
    cursor: pointer;
    transition: all 0.2s;
    display: flex;
    align-items: center;
    gap: var(--gap-sm);
    box-shadow:
      0 0 20px rgba(57, 255, 20, 0.3),
      0 0 60px rgba(57, 255, 20, 0.1);
  }

  .process-btn:hover:not(:disabled) {
    transform: scale(1.03);
    box-shadow:
      0 0 30px rgba(57, 255, 20, 0.4),
      0 0 80px rgba(57, 255, 20, 0.2);
  }

  .process-btn:active:not(:disabled) {
    transform: scale(0.98);
  }

  .process-btn:disabled {
    opacity: 0.3;
    cursor: not-allowed;
    box-shadow: none;
  }

  .process-btn.processing {
    background: var(--neon-yellow);
    box-shadow:
      0 0 20px rgba(237, 255, 33, 0.3),
      0 0 60px rgba(237, 255, 33, 0.1);
  }

  .btn-spinner {
    width: 14px;
    height: 14px;
    border: 2px solid rgba(0, 0, 0, 0.2);
    border-top-color: var(--bg-dark);
    border-radius: 50%;
    animation: spin 0.6s linear infinite;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  :global(:root.tame) .process-btn {
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.1);
    color: #fff;
  }

  :global(:root.tame) .process-btn:hover:not(:disabled) {
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.15);
  }
</style>
