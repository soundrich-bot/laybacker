<script>
  // A blocking question from a running chain: fix / skip / stop, with an
  // "apply to all remaining files" tick so a big batch asks once.
  let { prompt, onChoose } = $props();
  let applyAll = $state(false);

  function handleKeydown(e) {
    if (e.key === 'Escape') onChoose('stop', false);
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="overlay" role="alertdialog" aria-modal="true" aria-labelledby="chain-prompt-title">
  <div class="box" class:warn={prompt.kind === 'unfixable'}>
    <div class="eyebrow">{prompt.kind === 'unfixable' ? 'CAN’T BE FIXED' : 'CHAIN PAUSED'} · FILE {prompt.index} OF {prompt.total}</div>
    <div class="title" id="chain-prompt-title">{prompt.title}</div>
    <div class="file" title={prompt.filename}>{prompt.filename}</div>
    {#if prompt.lines?.length}
      <ul class="lines">
        {#each prompt.lines as line}<li>{line}</li>{/each}
      </ul>
    {/if}
    {#if prompt.body}<p class="body">{prompt.body}</p>{/if}

    <label class="apply-all">
      <input type="checkbox" bind:checked={applyAll} />
      <span>Do the same for every remaining file in this run</span>
    </label>

    <div class="actions">
      <button class="btn stop" onclick={() => onChoose('stop', false)} title="Stop the run here. Files already finished are kept.">STOP RUN</button>
      <span class="spacer"></span>
      {#each prompt.choices as c}
        <button class="btn {c.kind}" onclick={() => onChoose(c.id, applyAll)}>{c.label}</button>
      {/each}
    </div>
  </div>
</div>

<style>
  .overlay { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.6); display: flex; align-items: center; justify-content: center; z-index: 1100; }
  .box {
    width: min(520px, calc(100vw - 48px));
    background: var(--bg-panel);
    border: 1px solid var(--neon-cyan);
    border-radius: var(--radius-md);
    padding: var(--gap-lg);
    box-shadow: 0 12px 40px rgba(0, 0, 0, 0.5);
  }
  .box.warn { border-color: var(--neon-orange); }
  .eyebrow { font-family: var(--font-display); font-size: 9px; letter-spacing: 0.15em; color: var(--text-muted); margin-bottom: 6px; }
  .title { font-family: var(--font-display); font-size: 14px; letter-spacing: 0.08em; color: var(--neon-cyan); margin-bottom: 4px; }
  .box.warn .title { color: var(--neon-orange); }
  .file { font-family: var(--font-mono); font-size: 12px; color: var(--text-primary); margin-bottom: var(--gap-md); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .lines { margin: 0 0 var(--gap-md) 18px; padding: 0; font-family: var(--font-mono); font-size: 12px; line-height: 1.6; color: var(--text-secondary); }
  .body { font-family: var(--font-mono); font-size: 12px; line-height: 1.6; color: var(--text-secondary); margin: 0 0 var(--gap-md); }
  .apply-all { display: flex; align-items: center; gap: 8px; font-family: var(--font-mono); font-size: 11px; color: var(--text-muted); margin-bottom: var(--gap-lg); cursor: pointer; }
  .apply-all input { accent-color: var(--neon-cyan); }
  .actions { display: flex; align-items: center; gap: var(--gap-sm); }
  .spacer { flex: 1; }
  .btn {
    font-family: var(--font-display);
    font-size: 11px;
    letter-spacing: 0.08em;
    padding: 7px 14px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--border-color);
    background: var(--cap-face);
    color: var(--text-secondary);
    cursor: pointer;
    box-shadow: var(--cap-shadow);
    transition: all 0.15s;
  }
  .btn:hover { box-shadow: var(--cap-shadow-hover); color: var(--text-primary); }
  .btn:active { transform: translateY(1px); box-shadow: var(--cap-shadow-pressed); }
  .btn.primary { background: var(--neon-cyan); border-color: var(--neon-cyan); color: var(--bg-dark); }
  .btn.primary:hover { filter: brightness(1.1); color: var(--bg-dark); }
  .box.warn .btn.primary { background: var(--neon-orange); border-color: var(--neon-orange); }
  .btn.stop { color: var(--neon-pink); border-color: rgba(255, 46, 99, 0.4); }
  .btn.stop:hover { color: var(--neon-pink); border-color: var(--neon-pink); }
</style>
