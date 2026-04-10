<script>
  let { errors = [], onDismiss } = $props();
</script>

{#if errors.length > 0}
  <div class="error-bar">
    {#each errors as error, i}
      <div class="error-item">
        <span class="error-icon">!</span>
        <span class="error-text">{error}</span>
        <button class="error-dismiss" onclick={() => onDismiss(i)}>&times;</button>
      </div>
    {/each}
  </div>
{/if}

<style>
  .error-bar {
    padding: var(--gap-sm) var(--gap-lg);
    display: flex;
    flex-direction: column;
    gap: var(--gap-xs);
    flex-shrink: 0;
  }

  .error-item {
    display: flex;
    align-items: center;
    gap: var(--gap-sm);
    padding: var(--gap-sm) var(--gap-md);
    background: rgba(255, 46, 99, 0.1);
    border: 1px solid rgba(255, 46, 99, 0.2);
    border-radius: var(--radius-sm);
  }

  :global(:root.tame) .error-item {
    background: rgba(224, 122, 95, 0.1);
    border-color: rgba(224, 122, 95, 0.25);
  }

  .error-icon {
    font-family: var(--font-display);
    font-size: 13px;
    color: var(--neon-pink);
    width: 20px;
    height: 20px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 50%;
    border: 2px solid var(--neon-pink);
    flex-shrink: 0;
  }

  .error-text {
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--neon-pink);
    flex: 1;
  }

  .error-dismiss {
    background: none;
    border: none;
    color: var(--neon-pink);
    font-size: 17px;
    cursor: pointer;
    padding: 2px 6px;
    opacity: 0.6;
    line-height: 1;
  }

  .error-dismiss:hover {
    opacity: 1;
  }
</style>
