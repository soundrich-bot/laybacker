<script>
  // A self-contained ProRes working-file button. Used on matched-pair rows AND
  // on lone videos (dropped without audio yet) so a ProRes can always be made.
  let { videoPath, onCreateProres, onReveal } = $props();

  let proresState = $state('idle'); // idle | working | done | error
  let proresPath = $state('');

  async function runProres() {
    if (proresState === 'done' && proresPath) { onReveal(proresPath); return; }
    if (proresState === 'working') return;
    proresState = 'working';
    try {
      proresPath = await onCreateProres(videoPath);
      proresState = 'done';
    } catch {
      proresState = 'error';
    }
  }
</script>

<button
  class="prores-btn"
  class:working={proresState === 'working'}
  class:done={proresState === 'done'}
  class:error={proresState === 'error'}
  onclick={runProres}
  title={proresState === 'done'
    ? 'ProRes working file ready — click to show in Finder'
    : proresState === 'working'
      ? 'Creating ProRes…'
      : 'Make a ProRes working file for Pro Tools'}
>
  {#if proresState === 'working'}ProRes…{:else if proresState === 'done'}✓ ProRes{:else if proresState === 'error'}ProRes ✕{:else}ProRes{/if}
</button>

<style>
  .prores-btn {
    flex-shrink: 0;
    font-family: var(--font-mono);
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.04em;
    color: var(--text-muted);
    background: none;
    border: 1px solid var(--border-color);
    border-radius: var(--radius-sm);
    padding: 4px 8px;
    cursor: pointer;
    transition: all 0.15s;
    white-space: nowrap;
    opacity: 0.6;
  }
  .prores-btn:hover { opacity: 1; color: var(--neon-cyan); border-color: rgba(8, 247, 254, 0.4); }
  .prores-btn.working { color: var(--text-secondary); opacity: 0.85; cursor: default; }
  .prores-btn.done { color: var(--neon-green); border-color: rgba(57, 255, 20, 0.4); opacity: 1; }
  .prores-btn.error { color: var(--neon-pink); border-color: rgba(255, 46, 99, 0.4); opacity: 1; }
</style>
