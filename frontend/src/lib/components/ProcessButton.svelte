<script>
  let { pairCount = 0, isProcessing = false, onProcess, onClear, onCancel, audioOnly = false } = $props();
</script>

<div class="process-bar">
  <div class="process-bar-left">
    {#if pairCount > 0}
      <button class="clear-btn" onclick={onClear} disabled={isProcessing}>
        CLEAR ALL
      </button>
    {/if}
  </div>

  {#if isProcessing}
    <button class="cancel-btn" onclick={onCancel}>
      CANCEL
    </button>
  {:else}
    <button
      class="process-btn"
      disabled={pairCount === 0}
      onclick={onProcess}
    >
      {audioOnly ? 'NORMALIZE' : 'LAYBACK'} {pairCount > 0 ? `(${pairCount})` : ''}
    </button>
  {/if}

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

  .cancel-btn {
    font-family: var(--font-display);
    font-size: 16px;
    letter-spacing: 0.2em;
    color: #fff;
    background: var(--neon-pink);
    border: none;
    border-radius: var(--radius-md);
    padding: 12px 48px;
    cursor: pointer;
    transition: all 0.2s;
    box-shadow:
      0 0 20px rgba(255, 46, 99, 0.3),
      0 0 60px rgba(255, 46, 99, 0.1);
  }

  .cancel-btn:hover {
    transform: scale(1.03);
    box-shadow:
      0 0 30px rgba(255, 46, 99, 0.4),
      0 0 80px rgba(255, 46, 99, 0.2);
  }

  :global(:root.tame) .process-btn {
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.1);
    color: #fff7f0;
  }

  :global(:root.tame) .process-btn:hover:not(:disabled) {
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.15);
  }

  :global(:root.tame) .cancel-btn {
    color: #fff7f0;
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.1);
  }

  :global(:root.tame) .cancel-btn:hover {
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.15);
  }
</style>
