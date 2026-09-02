<script>
  let { prompt, onChoose, onCancel } = $props();

  // A Laybacker-slated picture carries its slate length in the file — then
  // "end" is exact, so it's the default. Otherwise "start" is today's behaviour.
  let tagged = $derived(prompt?.slateSecs != null);
  let defaultChoice = $derived(tagged ? 'end' : 'start');

  const options = $derived([
    {
      id: 'start',
      title: 'START',
      desc: 'The sound starts on the first frame of picture. The last few seconds of picture will be silent.',
    },
    {
      id: 'end',
      title: 'END',
      desc: tagged
        ? `Line the sound up with the end of the picture. This file carries a ${prompt.slateSecs.toFixed(1)}s slate that Laybacker rendered, so the sound lands exactly on the first frame of programme.`
        : 'Line the sound up with the end of the picture — right for a file that has a slate or head at the front and the programme audio is full length.',
    },
  ]);
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="overlay" onclick={onCancel}>
  <div class="box" onclick={(e) => e.stopPropagation()} role="alertdialog" aria-modal="true">
    <div class="title">AUDIO IS SHORTER THAN VIDEO</div>
    <p class="body">
      <strong>{prompt.filename}</strong> is <strong>{prompt.shortBy.toFixed(1)}s shorter</strong> than its video.
      Where does the sound belong?
    </p>

    <div class="options">
      {#each options as opt (opt.id)}
        <button
          class="opt"
          class:default={opt.id === defaultChoice}
          onclick={() => onChoose(opt.id)}
        >
          <span class="opt-title">
            {opt.title}
            {#if opt.id === defaultChoice}<span class="opt-tag">default</span>{/if}
          </span>
          <span class="opt-desc">{opt.desc}</span>
        </button>
      {/each}
    </div>

    <div class="actions">
      <button class="cancel" onclick={onCancel}>CANCEL EXPORT</button>
    </div>
  </div>
</div>

<svelte:window onkeydown={(e) => {
  if (e.key === 'Escape') onCancel();
  else if (e.key === 'Enter') { e.preventDefault(); onChoose(defaultChoice); }
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
    width: min(500px, calc(100vw - 48px));
    background: var(--bg-panel);
    border: 1px solid var(--neon-orange);
    border-radius: var(--radius-md);
    padding: var(--gap-lg);
    box-shadow: 0 12px 40px rgba(0, 0, 0, 0.5);
  }

  .title {
    font-family: var(--font-display);
    font-size: 14px;
    letter-spacing: 0.08em;
    color: var(--neon-orange);
    margin-bottom: var(--gap-md);
  }

  .body {
    font-family: var(--font-mono);
    font-size: 12.5px;
    line-height: 1.6;
    color: var(--text-secondary);
    margin-bottom: var(--gap-lg);
    word-break: break-word;
  }
  .body strong { color: var(--text-primary); }

  .options {
    display: flex;
    flex-direction: column;
    gap: var(--gap-sm);
    margin-bottom: var(--gap-lg);
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
  .opt.default { border-color: rgba(8, 247, 254, 0.5); }

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

  .actions { display: flex; justify-content: flex-end; }
  .cancel {
    font-family: var(--font-mono);
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.05em;
    color: var(--text-muted);
    background: transparent;
    border: 1px solid var(--border-color);
    border-radius: var(--radius-sm);
    padding: 7px 14px;
    cursor: pointer;
    transition: color 0.15s, border-color 0.15s;
  }
  .cancel:hover { color: var(--text-primary); border-color: var(--text-muted); }
</style>
