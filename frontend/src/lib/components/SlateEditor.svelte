<script>
  import { drawSlate, SLATE_FONTS, SLATE_SIZES, SLATE_ANCHORS } from '../slate.js';

  let {
    editor, isBatch, isSolo = false, videoCount = 0,
    assets = {}, onPickImage,
    onApply, onRemove, onCancel,
  } = $props();

  let text = $state(editor.text ?? '');
  let duration = $state(editor.duration ?? 5);
  let font = $state(editor.font ?? 'sans');
  let size = $state(editor.size ?? 'm');
  let bgId = $state(editor.bgId ?? null);
  // Image layout + text placement (see slate.js for what each means).
  let fit = $state(editor.fit ?? 'fit');
  let scale = $state(editor.scale ?? 0.7);
  let anchor = $state(editor.anchor ?? 'c');
  let textPos = $state(editor.textPos ?? 'middle');
  // Black (silent) run after the card. A preroll preset sets card + black
  // together — e.g. 5s = 4s slate + 1s black.
  let black = $state(editor.black ?? 0);
  const prerolls = [
    { label: '5s PREROLL', slate: 4, black: 1, desc: '4 seconds of slate, then 1 second of black' },
    { label: '10s PREROLL', slate: 9, black: 1, desc: '9 seconds of slate, then 1 second of black' },
  ];
  function setPreroll(p) { duration = p.slate; black = p.black; }
  let preroll = $derived((parseFloat(duration) || 0) + (parseFloat(black) || 0));
  const textPositions = [['top', 'TOP'], ['middle', 'MIDDLE'], ['bottom', 'BOTTOM']];
  let picking = $state(false);
  let previewCanvas = $state(null);

  let bgAsset = $derived(bgId ? (assets[bgId] ?? null) : null);

  // Live preview: redraw whenever the text or style changes.
  $effect(() => {
    if (previewCanvas) {
      drawSlate(previewCanvas, text, { font, size, bgImage: bgAsset?.img ?? null, fit, scale, anchor, textPos });
    }
  });

  async function chooseImage() {
    if (picking || !onPickImage) return;
    picking = true;
    try {
      const id = await onPickImage();
      if (id) bgId = id;
    } finally {
      picking = false;
    }
  }

  function apply() {
    onApply(text, parseFloat(duration) || 5, {
      font, size, bgId, fit, scale: parseFloat(scale) || 0.7, anchor, textPos,
      black: Math.max(0, parseFloat(black) || 0),
    });
  }

  // A card can be image-only; otherwise it needs some text.
  let canApply = $derived(text.trim().length > 0 || !!bgId);
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
      rows="3"
      placeholder={bgId ? 'Text over the image (optional) — each line is centred' : 'Type the slate text — each line is centred on the card'}
      bind:value={text}
    ></textarea>

    <div class="controls">
      <div class="control">
        <label class="control-label" for="slate-font">FONT</label>
        <select id="slate-font" class="font-select" bind:value={font}
          style="font-family: {SLATE_FONTS[font]?.family ?? 'inherit'}"
          title="Named fonts — each falls back to its nearest cousin on a machine that lacks it">
          {#each Object.entries(SLATE_FONTS) as [key, f] (key)}
            <option value={key} style="font-family: {f.family}">{f.label}</option>
          {/each}
        </select>
      </div>
      <div class="control">
        <span class="control-label">SIZE</span>
        <div class="seg">
          {#each Object.entries(SLATE_SIZES) as [key, s] (key)}
            <button class="seg-btn" class:active={size === key} onclick={() => size = key}>{s.label}</button>
          {/each}
        </div>
      </div>
      <div class="control">
        <span class="control-label">IMAGE</span>
        <div class="seg">
          <button class="seg-btn" onclick={chooseImage} disabled={picking}
            title="Use your own picture as the card — it fills the frame, text goes over the top">
            {picking ? 'CHOOSING…' : bgAsset ? `${bgAsset.name}` : 'CHOOSE IMAGE…'}
          </button>
          {#if bgId}
            <button class="seg-btn remove-img" onclick={() => bgId = null} title="Back to a plain black card">✕</button>
          {/if}
        </div>
      </div>
      {#if bgId}
        <div class="control">
          <span class="control-label">LAYOUT</span>
          <div class="seg">
            <button class="seg-btn" class:active={fit === 'fit'} onclick={() => fit = 'fit'}
              title="Show the whole image, letterboxed on black — right for a logo">FIT</button>
            <button class="seg-btn" class:active={fit === 'fill'} onclick={() => fit = 'fill'}
              title="Fill the frame edge to edge, cropping if the shape differs — right for a photo">FILL</button>
          </div>
        </div>
        {#if fit === 'fit'}
          <div class="control">
            <label class="control-label" for="slate-scale">SCALE</label>
            <div class="seg scale-row">
              <input id="slate-scale" class="scale-range" type="range" min="0.05" max="1" step="0.01"
                bind:value={scale} title="Image size as a share of the frame, 5–100%" />
              <span class="scale-readout">{Math.round(scale * 100)}%</span>
            </div>
          </div>
        {/if}
        <div class="control">
          <span class="control-label">POSITION</span>
          <div class="anchor-grid" role="group" aria-label="Image position">
            {#each SLATE_ANCHORS as a (a)}
              <button class="anchor-btn" class:active={anchor === a} onclick={() => anchor = a}
                title={fit === 'fill' ? 'Which part of the image to keep' : 'Where the image sits'}
                aria-label={a}></button>
            {/each}
          </div>
        </div>
      {/if}
      <div class="control">
        <span class="control-label">TEXT</span>
        <div class="seg">
          {#each textPositions as [key, label] (key)}
            <button class="seg-btn" class:active={textPos === key} onclick={() => textPos = key}
              title="Where the words sit on the card">{label}</button>
          {/each}
        </div>
      </div>
      <div class="control">
        <span class="control-label">PREROLL</span>
        <div class="seg">
          {#each prerolls as p (p.label)}
            <button class="seg-btn"
              class:active={Math.abs(parseFloat(duration) - p.slate) < 0.01 && Math.abs(parseFloat(black) - p.black) < 0.01}
              onclick={() => setPreroll(p)} title={p.desc}>{p.label}</button>
          {/each}
        </div>
      </div>
      <div class="control">
        <label class="control-label" for="slate-duration">DURATION</label>
        <div class="seg duration-row">
          <span class="duration-part">
            <input id="slate-duration" class="duration-input" type="number" min="0.5" step="0.5" bind:value={duration}
              title="How long the card is shown" />
            <span class="duration-unit">s slate</span>
          </span>
          <span class="duration-plus">+</span>
          <span class="duration-part">
            <input id="slate-black" class="duration-input" type="number" min="0" step="0.5" bind:value={black}
              title="Black (silent) after the card, before the first frame of programme — 0 for none" />
            <span class="duration-unit">s black</span>
          </span>
          <span class="duration-total" title="Total preroll before programme starts">= {preroll.toFixed(1)}s</span>
        </div>
      </div>
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
      <button class="apply" onclick={apply} disabled={!canApply}>
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
    width: min(560px, calc(100vw - 48px));
    max-height: calc(100vh - 48px);
    overflow-y: auto;
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

  .controls {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-bottom: var(--gap-md);
  }
  .control {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .control-label {
    width: 64px;
    flex-shrink: 0;
    font-family: var(--font-display);
    font-size: 10px;
    letter-spacing: 0.12em;
    color: var(--text-muted);
  }
  .seg {
    display: flex;
    align-items: center;
    gap: 4px;
    flex-wrap: wrap;
  }
  .seg-btn {
    font-family: var(--font-display);
    font-size: 10px;
    letter-spacing: 0.1em;
    color: var(--text-muted);
    background: var(--cap-face);
    border: 1px solid var(--border-color);
    border-radius: var(--radius-sm);
    padding: 4px 10px;
    cursor: pointer;
    transition: all 0.15s;
    box-shadow: var(--cap-shadow);
    max-width: 260px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .seg-btn:hover:not(:disabled):not(.active) { color: var(--neon-cyan); border-color: rgba(8, 247, 254, 0.5); }
  .seg-btn:active:not(:disabled) { transform: translateY(1px); box-shadow: var(--cap-shadow-pressed); }
  .seg-btn.active { color: var(--bg-dark); background: var(--neon-cyan); border-color: var(--neon-cyan); }
  .seg-btn:disabled { opacity: 0.5; cursor: wait; }
  .seg-btn.remove-img { color: var(--neon-orange); border-color: rgba(255, 149, 0, 0.45); }

  /* Named-font dropdown, shown in the chosen face */
  .font-select {
    font-size: 13px;
    color: var(--text-primary);
    background: var(--bg-dark);
    border: 1px solid var(--border-color);
    border-radius: var(--radius-sm);
    padding: 4px 8px;
    min-width: 220px;
    cursor: pointer;
  }
  .font-select:focus { outline: none; border-color: var(--neon-cyan); }

  /* 5–100% image scale slider */
  .scale-row { flex-wrap: nowrap; }
  .scale-range {
    width: 200px;
    accent-color: var(--neon-cyan);
    cursor: pointer;
  }
  .scale-readout {
    font-family: var(--font-mono);
    font-size: 11px;
    font-weight: 700;
    color: var(--text-secondary);
    min-width: 38px;
  }

  /* Slate + black = preroll */
  .duration-row { flex-wrap: nowrap; gap: 8px; }
  .duration-part { display: inline-flex; align-items: center; gap: 5px; }
  .duration-plus, .duration-total {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--text-muted);
  }
  .duration-total { font-weight: 700; color: var(--neon-cyan); }

  /* 3×3 image position picker — a tiny frame you click a corner/edge/centre of */
  .anchor-grid {
    display: grid;
    grid-template-columns: repeat(3, 16px);
    grid-template-rows: repeat(3, 12px);
    gap: 2px;
    padding: 3px;
    background: var(--bg-dark);
    border: 1px solid var(--border-color);
    border-radius: var(--radius-sm);
  }
  .anchor-btn {
    width: 16px;
    height: 12px;
    padding: 0;
    background: var(--cap-face);
    border: 1px solid var(--border-color);
    border-radius: 2px;
    cursor: pointer;
    transition: all 0.15s;
  }
  .anchor-btn:hover:not(.active) { border-color: var(--neon-cyan); }
  .anchor-btn.active { background: var(--neon-cyan); border-color: var(--neon-cyan); }

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
