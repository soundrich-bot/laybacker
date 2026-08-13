<script>
  // Slate button for a solo video (dropped without audio). Opens the shared
  // slate editor; the store runs the render and reports status/progress here.
  let { video, status = null, onOpenSlate, onReveal } = $props();

  let state = $derived(status?.state ?? 'idle'); // idle | working | done | error
  let pct = $derived(status?.pct ?? 0);

  function click() {
    if (state === 'working') return;
    if (state === 'done' && status?.output) { onReveal(status.output); return; }
    onOpenSlate(video);
  }
</script>

<button
  class="slate-btn"
  class:working={state === 'working'}
  class:done={state === 'done'}
  class:error={state === 'error'}
  onclick={click}
  title={state === 'done'
    ? 'Slated file ready — click to show in Finder'
    : state === 'working'
      ? 'Rendering slate…'
      : state === 'error'
        ? 'Slate failed — click to try again'
        : "Add a text slate to the front of this video (keeps the video's own sound)"}
>
  {#if state === 'working'}
    <span class="slate-fill" style="width:{pct}%"></span>
    <span class="slate-label">Slate… {pct}%</span>
  {:else if state === 'done'}
    ✓ Slated
  {:else if state === 'error'}
    Slate ✕
  {:else}
    Slate
  {/if}
</button>

<style>
  .slate-btn {
    position: relative;
    overflow: hidden;
    flex-shrink: 0;
    font-family: var(--font-mono);
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.04em;
    color: var(--text-muted);
    background: var(--cap-face);
    border: 1px solid var(--border-color);
    border-radius: var(--radius-sm);
    padding: 4px 8px;
    cursor: pointer;
    transition: all 0.15s;
    white-space: nowrap;
    opacity: 0.85;
    box-shadow: var(--cap-shadow);
  }
  .slate-btn:hover {
    opacity: 1;
    color: var(--neon-cyan);
    border-color: rgba(8, 247, 254, 0.4);
    box-shadow: var(--cap-shadow-hover);
  }
  .slate-btn:active {
    transform: translateY(1px);
    box-shadow: var(--cap-shadow-pressed);
  }
  .slate-btn.working {
    cursor: default;
    color: var(--text-secondary);
    border-color: rgba(8, 247, 254, 0.4);
  }
  .slate-btn.done {
    color: var(--neon-green);
    border-color: rgba(57, 255, 20, 0.4);
    opacity: 1;
  }
  .slate-btn.error {
    color: var(--neon-orange);
    border-color: rgba(255, 149, 0, 0.4);
    opacity: 1;
  }
  .slate-fill {
    position: absolute;
    left: 0; top: 0; bottom: 0;
    background: rgba(8, 247, 254, 0.15);
    transition: width 0.2s ease;
  }
  .slate-label { position: relative; }
</style>
