<script>
  // Shown when audio files are dropped on their own: work on them here, or
  // hold them until a video arrives to lay them back onto.
  let { count = 0, onChoose } = $props();
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="overlay">
  <div class="box" role="dialog" aria-modal="true">
    <div class="title">{count} AUDIO FILE{count === 1 ? '' : 'S'} — NO VIDEO</div>
    <p class="body">What would you like to do with {count === 1 ? 'it' : 'them'}?</p>

    <div class="options">
      <button class="opt" onclick={() => onChoose('audio')}>
        <span class="opt-title">AUDIO ONLY <span class="opt-tag">default</span></span>
        <span class="opt-desc">Open the Audio Only page: QC, file deliverables and file processing for these files.</span>
      </button>
      <button class="opt" onclick={() => onChoose('layback')}>
        <span class="opt-title">WAITING FOR VIDEO</span>
        <span class="opt-desc">Hold them here — I'll drop the picture next and lay the sound back onto it.</span>
      </button>
    </div>
  </div>
</div>

<svelte:window onkeydown={(e) => {
  if (e.key === 'Enter') { e.preventDefault(); onChoose('audio'); }
  else if (e.key === 'Escape') { e.preventDefault(); onChoose('layback'); }
}} />

<style>
  .overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.6);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 200;
  }

  .box {
    width: min(480px, calc(100vw - 48px));
    background: var(--bg-panel);
    border: 1px solid var(--neon-cyan);
    border-radius: var(--radius-md);
    padding: var(--gap-lg);
    box-shadow: 0 12px 40px rgba(0, 0, 0, 0.5);
  }

  .title {
    font-family: var(--font-display);
    font-size: 14px;
    letter-spacing: 0.08em;
    color: var(--neon-cyan);
    margin-bottom: var(--gap-sm);
  }

  .body {
    font-family: var(--font-mono);
    font-size: 12.5px;
    color: var(--text-secondary);
    margin-bottom: var(--gap-lg);
  }

  .options {
    display: flex;
    flex-direction: column;
    gap: var(--gap-sm);
  }

  .opt {
    display: flex;
    flex-direction: column;
    gap: 4px;
    text-align: left;
    padding: 12px 14px;
    background: var(--cap-face);
    border: 1px solid var(--border-color);
    border-radius: var(--radius-sm);
    cursor: pointer;
    transition: border-color 0.15s, box-shadow 0.15s;
    box-shadow: var(--cap-shadow);
  }
  .opt:hover { border-color: var(--neon-cyan); box-shadow: var(--cap-shadow-hover); }
  .opt:active { transform: translateY(1px); box-shadow: var(--cap-shadow-pressed); }
  .opt:first-child { border-color: rgba(8, 247, 254, 0.5); }

  .opt-title {
    font-family: var(--font-display);
    font-size: 13px;
    letter-spacing: 0.05em;
    color: var(--text-primary);
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .opt-tag {
    font-family: var(--font-mono);
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--bg-dark);
    background: var(--neon-cyan);
    padding: 1px 6px;
    border-radius: 999px;
  }
  .opt-desc {
    font-family: var(--font-mono);
    font-size: 11.5px;
    line-height: 1.5;
    color: var(--text-secondary);
  }
</style>
