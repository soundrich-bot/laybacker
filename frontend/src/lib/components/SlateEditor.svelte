<script>
  import { drawSlate } from '../slate.js';

  let { editor, isBatch, isSolo = false, videoCount = 0, onApply, onRemove, onCancel } = $props();

  let text = $state(editor.text ?? '');
  let duration = $state(editor.duration ?? 5);
  let previewCanvas = $state(null);

  // Live preview: redraw whenever the text changes.
  $effect(() => {
    if (previewCanvas) drawSlate(previewCanvas, text);
  });

  function apply() {
    onApply(text, parseFloat(duration) || 5);
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="overlay" onclick={onCancel}>
  <div class="box" onclick={(e) => e.stopPropagation()} role="dialog" aria-modal="true">
    <div class="title">{isBatch ? 'SLATE — ALL VIDEOS' : isSolo ? `SLATE — ${editor.video?.filename ?? 'THIS VIDEO'}` : 'SLATE — THIS FILE'}</div>

    <canvas
      bind:this={previewCanvas}
      class="preview"
      width="480"
      height="270"
      aria-label="Slate preview"
    ></canvas>

    <textarea
      class="slate-text"
      rows="4"
      placeholder="Type the slate text — each line is centred on the card"
      bind:value={text}
    ></textarea>

    <div class="duration-row">
      <label class="duration-label" for="slate-duration">SLATE DURATION</label>
      <input
        id="slate-duration"
        class="duration-input"
        type="number"
        min="0.5"
        step="0.5"
        bind:value={duration}
      />
      <span class="duration-unit">seconds</span>
    </div>

    <p class="note">
      {#if isSolo}
        The slate is silent — the video's own soundtrack is kept and starts with
        the first frame of programme. A new file "…_Slated.mov" is rendered next
        to the original (re-encoded, so it takes a moment — the button shows
        progress).
      {:else}
        The slate is silent — the soundtrack starts with the first frame of programme.
        Adding a slate re-encodes the video, so the export takes longer (a progress
        bar will show).
      {/if}
    </p>

    <div class="actions">
      {#if onRemove}
        <button class="remove" onclick={onRemove}>REMOVE SLATE</button>
      {/if}
      <span class="spacer"></span>
      <button class="cancel" onclick={onCancel}>CANCEL</button>
      <button class="apply" onclick={apply} disabled={!text.trim()}>
        {isBatch ? `APPLY TO ALL ${videoCount} VIDEO${videoCount === 1 ? '' : 'S'}`
          : isSolo ? 'RENDER SLATED FILE' : 'APPLY'}
      </button>
    </div>
  </div>
</div>

<svelte:window onkeydown={(e) => { if (e.key === 'Escape') onCancel(); }} />

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
    width: min(540px, calc(100vw - 48px));
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
    margin-bottom: var(--gap-md);
  }

  .preview {
    display: block;
    width: 100%;
    aspect-ratio: 16 / 9;
    border: 1px solid var(--border-color);
    border-radius: var(--radius-sm);
    background: #000;
    margin-bottom: var(--gap-md);
  }

  .slate-text {
    width: 100%;
    resize: vertical;
    font-family: var(--font-mono);
    font-size: 13px;
    line-height: 1.5;
    color: var(--text-primary);
    background: var(--bg-dark);
    border: 1px solid var(--border-color);
    border-radius: var(--radius-sm);
    padding: 10px 12px;
    margin-bottom: var(--gap-md);
  }
  .slate-text:focus { outline: none; border-color: var(--neon-cyan); }

  .duration-row {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: var(--gap-md);
  }
  .duration-label {
    font-family: var(--font-display);
    font-size: 10px;
    letter-spacing: 0.12em;
    color: var(--text-muted);
  }
  .duration-input {
    width: 70px;
    font-family: var(--font-mono);
    font-size: 12px;
    font-weight: 700;
    color: var(--text-primary);
    background: var(--bg-dark);
    border: 1px solid var(--border-color);
    border-radius: var(--radius-sm);
    padding: 4px 8px;
  }
  .duration-input:focus { outline: none; border-color: var(--neon-cyan); }
  .duration-unit {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--text-muted);
  }

  .note {
    font-family: var(--font-mono);
    font-size: 11px;
    line-height: 1.55;
    color: var(--text-muted);
    margin-bottom: var(--gap-lg);
  }

  .actions {
    display: flex;
    align-items: center;
    gap: var(--gap-sm);
  }
  .spacer { flex: 1; }

  .cancel, .remove {
    font-family: var(--font-mono);
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.05em;
    color: var(--text-muted);
    background: var(--cap-face);
    border: 1px solid var(--border-color);
    border-radius: var(--radius-sm);
    padding: 7px 14px;
    cursor: pointer;
    transition: all 0.15s;
    box-shadow: var(--cap-shadow);
  }
  .cancel:hover { color: var(--text-primary); box-shadow: var(--cap-shadow-hover); }
  .remove { color: var(--neon-orange); border-color: rgba(255, 149, 0, 0.4); }
  .remove:hover { border-color: var(--neon-orange); box-shadow: var(--cap-shadow-hover); }
  .cancel:active, .remove:active { transform: translateY(1px); box-shadow: var(--cap-shadow-pressed); }

  .apply {
    font-family: var(--font-display);
    font-size: 11px;
    letter-spacing: 0.08em;
    color: var(--bg-dark);
    background: var(--neon-cyan);
    border: 1px solid var(--neon-cyan);
    border-radius: var(--radius-sm);
    padding: 7px 14px;
    cursor: pointer;
    transition: all 0.15s;
    box-shadow: var(--cap-shadow);
  }
  .apply:hover:not(:disabled) { filter: brightness(1.1); box-shadow: var(--cap-shadow-hover); }
  .apply:active:not(:disabled) { transform: translateY(1px); box-shadow: var(--cap-shadow-pressed); }
  .apply:disabled { opacity: 0.4; cursor: not-allowed; }
</style>
