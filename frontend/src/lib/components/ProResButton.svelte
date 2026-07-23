<script>
  import { onMount } from 'svelte';
  import { listen } from '@tauri-apps/api/event';

  // A self-contained ProRes working-file button. Used on matched-pair rows AND
  // on lone videos (dropped without audio yet) so a ProRes can always be made.
  let { videoPath, durationSecs = 0, onCreateProres, onReveal } = $props();

  let proresState = $state('idle'); // idle | working | done | error
  let proresPath = $state('');
  let pct = $state(0);              // 0–100, live encode progress

  onMount(() => {
    let unlisten;
    listen('prores-progress', (e) => {
      if (e.payload?.videoPath === videoPath) {
        pct = Math.min(100, Math.round((e.payload.progress ?? 0) * 100));
      }
    }).then((fn) => { unlisten = fn; });
    return () => { if (unlisten) unlisten(); };
  });

  async function runProres() {
    if (proresState === 'done' && proresPath) { onReveal(proresPath); return; }
    if (proresState === 'working') return;
    pct = 0;
    proresState = 'working';
    try {
      proresPath = await onCreateProres(videoPath, durationSecs);
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
  {#if proresState === 'working'}
    <span class="prores-fill" style="width:{pct}%"></span>
    <span class="prores-label">ProRes… {pct}%</span>
  {:else if proresState === 'done'}
    ✓ ProRes
  {:else if proresState === 'error'}
    ProRes ✕
  {:else}
    ProRes
  {/if}
</button>

<style>
  .prores-btn {
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
  .prores-btn:hover {
    opacity: 1;
    color: var(--neon-cyan);
    border-color: rgba(8, 247, 254, 0.4);
    box-shadow: var(--cap-shadow-hover);
  }
  .prores-btn:active {
    transform: translateY(1px);
    box-shadow: var(--cap-shadow-pressed);
  }
  .prores-btn.working { color: var(--text-secondary); opacity: 1; cursor: default; border-color: rgba(8, 247, 254, 0.4); }
  .prores-btn.done { color: var(--neon-green); border-color: rgba(57, 255, 20, 0.4); opacity: 1; }
  .prores-btn.error { color: var(--neon-pink); border-color: rgba(255, 46, 99, 0.4); opacity: 1; }

  .prores-fill {
    position: absolute;
    left: 0;
    top: 0;
    bottom: 0;
    background: rgba(8, 247, 254, 0.18);
    transition: width 0.2s linear;
    z-index: 0;
  }
  .prores-label { position: relative; z-index: 1; }
</style>
