<script>
  import { stemWord } from '../stems.js';
  // "These look like stems" — offered when dropped audio gives itself away.
  // The user confirms which file is the mix (guessed when it's obvious).
  let { prompt, onRun, onDismiss } = $props();
  let mixPath = $state(prompt?.mixPath ?? '');
  let stems = $derived(prompt.files.filter(f => f.path !== mixPath));
  let tells = $derived([...new Set(prompt.files.map(stemWord).filter(Boolean))].map(w => w.toUpperCase()));
</script>

<svelte:window onkeydown={(e) => {
  if (e.key === 'Escape') onDismiss();
  else if (e.key === 'Enter' && mixPath) { e.preventDefault(); onRun(mixPath); }
}} />

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="overlay" onclick={onDismiss}>
  <div class="box" onclick={(e) => e.stopPropagation()} role="alertdialog" aria-modal="true" aria-labelledby="stem-prompt-title">
    <div class="eyebrow">{tells.length ? tells.join(' · ') : 'STEMS'}</div>
    <div class="title" id="stem-prompt-title">THESE LOOK LIKE STEMS</div>
    <p class="body">
      See the mix and its stems stacked on one time axis, and click between them to listen. Tell Laybacker which file is the mix.
    </p>

    <label class="field">
      <span class="field-label">THE MIX</span>
      <select class="mix-select" bind:value={mixPath}>
        <option value="">— not sure —</option>
        {#each prompt.files as f (f.path)}
          <option value={f.path}>{f.filename}</option>
        {/each}
      </select>
    </label>
    <div class="stems">
      <span class="field-label">THE STEMS</span>
      <span class="stem-list">{stems.map(f => f.filename).join(' · ')}</span>
    </div>

    <div class="actions">
      <button class="btn later" onclick={onDismiss}>NOT NOW</button>
      <span class="spacer"></span>
      <button class="btn go" onclick={() => onRun(mixPath)}>VIEW STEMS</button>
    </div>
  </div>
</div>

<style>
  .overlay { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.6); display: flex; align-items: center; justify-content: center; z-index: 1100; }
  .box {
    width: min(560px, calc(100vw - 48px));
    background: var(--bg-panel);
    border: 1px solid var(--neon-cyan);
    border-radius: var(--radius-md);
    padding: var(--gap-lg);
    box-shadow: 0 12px 40px rgba(0, 0, 0, 0.5);
    display: flex; flex-direction: column; gap: 10px;
  }
  .eyebrow { font-family: var(--font-display); font-weight: 700; font-size: 9px; letter-spacing: 0.04em; color: var(--text-muted); }
  .title { font-family: var(--font-display); font-weight: 700; font-size: 14px; letter-spacing: 0.04em; color: var(--neon-cyan); }
  .body { font-family: var(--font-text); font-weight: 500; font-size: 13px; line-height: 1.5; color: var(--text-secondary); margin: 0; }
  .field, .stems { display: flex; align-items: baseline; gap: 10px; flex-wrap: wrap; }
  .field-label { font-family: var(--font-display); font-weight: 700; font-size: 11px; letter-spacing: 0.04em; color: var(--text-muted); min-width: 84px; }
  .mix-select { flex: 1; min-width: 240px; font-size: 13px; font-weight: 700; color: var(--text-primary); background: var(--bg-dark); border: 1px solid var(--border-color); border-radius: var(--radius-sm); padding: 5px 8px; }
  .stem-list { font-family: var(--font-text); font-weight: 500; font-size: 12.5px; color: var(--text-primary); flex: 1; min-width: 240px; line-height: 1.5; }
  .actions { display: flex; align-items: center; gap: 10px; margin-top: 4px; }
  .spacer { flex: 1; }
  .btn {
    font-family: var(--font-display); font-weight: 700; font-size: 11px; letter-spacing: 0.04em;
    border-radius: var(--radius-sm); padding: 8px 14px; cursor: pointer; transition: all 0.15s; box-shadow: var(--cap-shadow);
  }
  .btn.later { color: var(--text-secondary); background: var(--cap-face); border: 1px solid var(--border-color); }
  .btn.later:hover { color: var(--neon-cyan); border-color: rgba(8, 247, 254, 0.5); }
  .btn.go { color: var(--bg-dark); background: var(--neon-cyan); border: 1px solid var(--neon-cyan); }
  .btn.go:hover:not(:disabled) { filter: brightness(1.1); }
  .btn:disabled { opacity: 0.4; cursor: not-allowed; }
</style>
