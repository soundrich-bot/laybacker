<script>
  import { invoke } from '@tauri-apps/api/core';
  import { drawSlate, hitSlateLine, loadImage, slateLines, SLATE_FONTS, SLATE_SIZES, SLATE_ANCHORS } from '../slate.js';

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

  // PREPEND puts a card in front of the picture. OVERLAY lays the text over
  // the picture's first seconds — the runtime and the sound don't move.
  let mode = $state(editor.mode === 'overlay' ? 'overlay' : 'prepend');
  let overlay = $derived(mode === 'overlay');

  // Lines the user has dragged: { [lineIndex]: { x, y } } as frame fractions.
  let linePos = $state({ ...(editor.linePos ?? {}) });
  let movedCount = $derived(Object.keys(linePos).length);
  let lineCount = $derived(slateLines(text).length);
  // A line that no longer exists (text deleted) takes its position with it.
  $effect(() => {
    const stale = Object.keys(linePos).filter(k => Number(k) >= lineCount);
    if (stale.length) {
      const next = { ...linePos };
      for (const k of stale) delete next[k];
      linePos = next;
    }
  });

  // Overlay preview backdrop: a real frame of the video, scrubbable across
  // the seconds the text will cover, so it can be placed clear of whatever
  // the picture already carries (its own slate, a clock, burnt-in timecode).
  let backdropImg = $state(null);
  let frameAt = $state(0.5);
  let frameLoading = $state(false);
  let frameMax = $derived(Math.max(0.5, Math.min(parseFloat(duration) || 5, (editor.frameDuration || 5) - 0.1)));
  let frameTimer = null;
  let frameSeq = 0;
  $effect(() => {
    const path = editor.framePath;
    const secs = Math.min(frameAt, frameMax);
    if (!overlay || !path) return;
    clearTimeout(frameTimer);
    frameTimer = setTimeout(async () => {
      const seq = ++frameSeq;
      frameLoading = true;
      try {
        const url = await invoke('video_frame', { videoPath: path, secs, width: 960 });
        const img = await loadImage(url);
        if (seq === frameSeq) backdropImg = img;
      } catch { /* keep the last good frame */ }
      finally { if (seq === frameSeq) frameLoading = false; }
    }, 120);
    return () => clearTimeout(frameTimer);
  });

  let bgAsset = $derived(bgId ? (assets[bgId] ?? null) : null);

  // Dragging
  let layout = null;          // last drawn layout, for hit-testing
  let dragging = $state(null); // line index being dragged
  let hover = $state(null);
  let grabOffset = { x: 0, y: 0 };
  let snapX = $state(false);

  // Live preview: redraw whenever the text, style or a drag changes.
  $effect(() => {
    if (!previewCanvas) return;
    layout = drawSlate(previewCanvas, text,
      { font, size, bgImage: overlay ? null : (bgAsset?.img ?? null), fit, scale, anchor, textPos, linePos },
      { backdrop: overlay ? backdropImg : null, highlight: dragging ?? hover });
    if (snapX && dragging != null) {
      const ctx = previewCanvas.getContext('2d');
      ctx.fillStyle = 'rgba(8, 247, 254, 0.7)';
      ctx.fillRect(previewCanvas.width / 2, 0, 1, previewCanvas.height);
    }
  });

  function canvasPoint(e) {
    const r = previewCanvas.getBoundingClientRect();
    return {
      x: (e.clientX - r.left) * (previewCanvas.width / r.width),
      y: (e.clientY - r.top) * (previewCanvas.height / r.height),
    };
  }
  function onPointerDown(e) {
    const p = canvasPoint(e);
    const i = hitSlateLine(layout, p.x, p.y);
    if (i == null) return;
    const l = layout.lines[i];
    grabOffset = { x: p.x - l.x, y: p.y - l.y };
    dragging = i;
    previewCanvas.setPointerCapture(e.pointerId);
    e.preventDefault();
  }
  function onPointerMove(e) {
    const p = canvasPoint(e);
    if (dragging == null) {
      hover = hitSlateLine(layout, p.x, p.y);
      return;
    }
    let x = (p.x - grabOffset.x) / previewCanvas.width;
    let y = (p.y - grabOffset.y) / previewCanvas.height;
    // Snap to the centre line — most slate text wants to be centred.
    snapX = Math.abs(x - 0.5) < 0.015;
    if (snapX) x = 0.5;
    // Keep the whole line inside the frame, not just its centre.
    const l = layout?.lines?.[dragging];
    const halfW = l ? (l.width / 2) / previewCanvas.width : 0.02;
    const halfH = l ? (l.height / 2) / previewCanvas.height : 0.03;
    x = halfW >= 0.5 ? 0.5 : Math.min(1 - halfW - 0.01, Math.max(halfW + 0.01, x));
    y = Math.min(1 - halfH - 0.01, Math.max(halfH + 0.01, y));
    linePos = { ...linePos, [dragging]: { x, y } };
  }
  function onPointerUp(e) {
    if (dragging != null) previewCanvas.releasePointerCapture?.(e.pointerId);
    dragging = null;
    snapX = false;
  }
  function resetPositions() { linePos = {}; }

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
      font, size, bgId: overlay ? null : bgId, fit, scale: parseFloat(scale) || 0.7, anchor, textPos,
      black: overlay ? 0 : Math.max(0, parseFloat(black) || 0),
      mode, linePos: { ...linePos },
    });
  }

  // A card can be image-only; an overlay is text-only, so it needs some.
  let canApply = $derived(text.trim().length > 0 || (!overlay && !!bgId));
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="overlay" onclick={onCancel}>
  <div class="box" onclick={(e) => e.stopPropagation()} role="dialog" aria-modal="true">
    <div class="title">{isBatch ? 'SLATE — ALL VIDEOS' : isSolo ? `SLATE — ${editor.video?.filename ?? 'THIS VIDEO'}` : 'SLATE — THIS FILE'}</div>

    <div class="mode-row" role="group" aria-label="Slate type">
      <button class="mode-btn" class:active={!overlay} onclick={() => mode = 'prepend'}
        title="A card in front of the picture. The file gets longer by the slate's length.">
        <span class="mode-name">CARD BEFORE PICTURE</span>
        <span class="mode-desc">Adds to the front · runtime grows</span>
      </button>
      <button class="mode-btn" class:active={overlay} onclick={() => mode = 'overlay'}
        title="Text over the first seconds of the picture — for videos that already carry a slate. Runtime and sound stay exactly as they are.">
        <span class="mode-name">TEXT OVER PICTURE</span>
        <span class="mode-desc">Over the opening seconds · runtime unchanged</span>
      </button>
    </div>

    <canvas
      bind:this={previewCanvas}
      class="preview"
      class:grab={hover != null && dragging == null}
      class:grabbing={dragging != null}
      width="960"
      height="540"
      aria-label="Slate preview — drag a line of text to move it"
      onpointerdown={onPointerDown}
      onpointermove={onPointerMove}
      onpointerup={onPointerUp}
      onpointercancel={onPointerUp}
      onpointerleave={() => { if (dragging == null) hover = null; }}
    ></canvas>
    {#if overlay && editor.framePath}
      <div class="scrub">
        <span class="scrub-label">PREVIEW FRAME</span>
        <input class="scrub-range" type="range" min="0" max={frameMax} step="0.1" bind:value={frameAt}
          title="Scrub through the seconds the text will cover, to check it clears what's already on the picture" />
        <span class="scrub-readout">{Math.min(frameAt, frameMax).toFixed(1)}s{frameLoading ? ' …' : ''}</span>
      </div>
    {/if}
    <div class="drag-hint">
      <span>{lineCount > 0 ? 'Drag any line of text to place it — it snaps to centre.' : 'Type some text, then drag each line into place.'}{overlay && !backdropImg ? ' (No preview frame for this video — positions still apply.)' : ''}</span>
      {#if movedCount > 0}
        <button class="link-btn" onclick={resetPositions} title="Put every line back in the automatic layout">RESET POSITIONS</button>
      {/if}
    </div>

    <textarea
      class="slate-text"
      rows="3"
      placeholder={overlay ? 'Type the text to lay over the picture — one line per row, drag each into place' : bgId ? 'Text over the image (optional) — drag each line into place' : 'Type the slate text — one line per row, drag each into place'}
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
      {#if !overlay}
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
      {#if overlay}
      <div class="control">
        <label class="control-label" for="slate-show">SHOW</label>
        <div class="seg duration-row">
          <span class="duration-part">
            <input id="slate-show" class="duration-input" type="number" min="0.5" step="0.5" bind:value={duration}
              title="How long the text stays over the picture, from the first frame" />
            <span class="duration-unit">seconds from the first frame, with a short fade out</span>
          </span>
        </div>
      </div>
      {:else}
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
      {/if}
    </div>

    <p class="note">
      {#if overlay}
        The text is laid over the first {parseFloat(duration) || 5} seconds of the picture. Nothing is
        added to the front, so <strong>the runtime stays the same and the sound isn't moved</strong> —
        versions still line up for A/B. On an H.264 file only the opening seconds are re-encoded and
        the rest of the picture is copied, so it's quick; other formats re-encode in full (a progress
        bar will show).{#if isSolo} A new file "…_Slated.mov" is rendered next to the original.{/if}
      {:else if isSolo}
        The slate is silent — the video's own soundtrack is kept and starts with
        the first frame of programme. A new file "…_Slated.mov" is rendered next
        to the original. On an H.264 file only the card is encoded and the
        picture is copied, so it's quick; other formats re-encode in full (the
        button shows progress).
      {:else}
        The slate is silent — the soundtrack starts with the first frame of programme.
        On an H.264 file only the card is encoded and the picture is copied, so it's
        quick; other formats re-encode in full (a progress bar will show).
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
    margin-bottom: 4px;
    touch-action: none;
    user-select: none;
    -webkit-user-select: none;
  }
  .preview.grab { cursor: grab; }
  .preview.grabbing { cursor: grabbing; border-color: var(--neon-cyan); }

  .scrub { display: flex; align-items: center; gap: 10px; margin-bottom: 4px; }
  .scrub-label { font-family: var(--font-display); font-size: 9px; letter-spacing: 0.12em; color: var(--text-muted); flex-shrink: 0; }
  .scrub-range { flex: 1; accent-color: var(--neon-cyan); cursor: pointer; }
  .scrub-readout { font-family: var(--font-mono); font-size: 11px; font-weight: 700; color: var(--text-secondary); min-width: 48px; text-align: right; }

  .drag-hint {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    font-family: var(--font-mono);
    font-size: 10.5px;
    color: var(--text-muted);
    margin-bottom: var(--gap-md);
    min-height: 18px;
  }
  .link-btn {
    font-family: var(--font-display);
    font-size: 9px;
    letter-spacing: 0.1em;
    color: var(--neon-cyan);
    background: none;
    border: none;
    padding: 0;
    cursor: pointer;
    flex-shrink: 0;
  }
  .link-btn:hover { text-decoration: underline; }

  /* PREPEND / OVERLAY switch */
  .mode-row { display: grid; grid-template-columns: 1fr 1fr; gap: 6px; margin-bottom: var(--gap-md); }
  .mode-btn {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 2px;
    text-align: left;
    padding: 7px 10px;
    background: var(--cap-face);
    border: 1px solid var(--border-color);
    border-radius: var(--radius-sm);
    cursor: pointer;
    transition: all 0.15s;
    box-shadow: var(--cap-shadow);
  }
  .mode-name { font-family: var(--font-display); font-size: 10px; letter-spacing: 0.1em; color: var(--text-muted); }
  .mode-desc { font-family: var(--font-mono); font-size: 10px; color: var(--text-muted); opacity: 0.8; }
  .mode-btn:hover:not(.active) { border-color: rgba(8, 247, 254, 0.5); }
  .mode-btn.active { border-color: var(--neon-cyan); background: rgba(8, 247, 254, 0.08); }
  .mode-btn.active .mode-name { color: var(--neon-cyan); }
  .note strong { color: var(--text-secondary); }

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
