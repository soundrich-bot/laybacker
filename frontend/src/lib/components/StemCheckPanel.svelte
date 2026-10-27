<script>
  import { convertFileSrc, invoke } from '@tauri-apps/api/core';
  import { stemCandidate } from '../stems.js';

  // The stem check, kept simple: the mix and its stems stacked on one time
  // axis, and a click on any row to hear that file. A visual check by eye
  // and by ear — no summing, no verdicts.
  let {
    files = [],          // the audio files on the page (MediaFile)
    mixPath = '',        // the file the prompt (or the user) called the mix; drawn first
  } = $props();

  // Rows: the mix first, then the rest in drop order.
  let rows = $derived.by(() => {
    const mix = files.find(f => f.path === mixPath) ?? stemCandidate(files)?.mix ?? null;
    const rest = files.filter(f => f !== mix);
    return [...(mix ? [{ file: mix, label: 'MIX', isMix: true }] : []), ...rest.map(f => ({ file: f, label: f.filenameNoExt || f.filename, isMix: false }))];
  });
  let peaks = $state({}); // path -> mono peaks
  $effect(() => {
    const want = rows.map(r => r.file.path).filter(p => !(p in peaks));
    if (!want.length) return;
    let cancelled = false;
    for (const path of want) {
      invoke('waveform_lanes', { audioPath: path, buckets: 1600, channels: 1 })
        .then(l => { if (!cancelled) peaks = { ...peaks, [path]: l?.[0] ?? [] }; })
        .catch(() => { if (!cancelled) peaks = { ...peaks, [path]: [] }; });
    }
    return () => { cancelled = true; };
  });
  let longest = $derived(rows.reduce((m, r) => Math.max(m, r.file.durationSecs || 0), 0));

  // ── Player: one file at a time, the playhead shared by every row ──
  let currentPath = $state('');
  let mediaEl = $state(null);
  let canvas = $state(null);
  let isPlaying = $state(false);
  let currentTime = $state(0);
  let loaded = $state(false);
  let pendingSeek = null;
  let pendingPlay = false;
  $effect(() => {
    // Follow the files: start on the mix, drop a file that has gone.
    if (!rows.length) { currentPath = ''; return; }
    if (!rows.some(r => r.file.path === currentPath)) currentPath = rows[0].file.path;
  });
  let current = $derived(rows.find(r => r.file.path === currentPath) ?? null);
  let assetUrl = $derived.by(() => {
    if (!current) return '';
    try { return convertFileSrc(current.file.path); } catch { return ''; }
  });

  function cssVar(name, fallback) {
    const v = getComputedStyle(document.documentElement).getPropertyValue(name).trim();
    return v || fallback;
  }
  // The waveforms change rarely and the playhead moves sixty times a second,
  // so the bars are painted once into two offscreen layers — every row in
  // the quiet style, and the lit style for the played part — and each frame
  // only copies them and draws the playhead. Smooth, and nearly free.
  let layerDim = null, layerLit = null, layerKey = '';
  let playheadColour = '#08f7fe';
  function paintLayers(w, h, dpr) {
    const lit = cssVar('--neon-cyan', '#08f7fe');
    const dim = cssVar('--text-muted', '#8888aa');
    const make = () => { const c = document.createElement('canvas'); c.width = w * dpr; c.height = h * dpr; return c; };
    layerDim = make(); layerLit = make();
    playheadColour = lit;
    const n = Math.max(1, rows.length);
    const rowH = h / n;
    const d = longest || 1;
    for (const [layer, isLit] of [[layerDim, false], [layerLit, true]]) {
      const ctx = layer.getContext('2d');
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      rows.forEach((row, li) => {
        const top = li * rowH, mid = top + rowH / 2;
        const selected = row.file.path === currentPath;
        if (selected && !isLit) { ctx.fillStyle = lit; ctx.globalAlpha = 0.07; ctx.fillRect(0, top, w, rowH); ctx.globalAlpha = 1; }
        const pk = peaks[row.file.path] ?? [];
        const m = pk.length;
        const own = row.file.durationSecs || d;
        if (m) {
          ctx.fillStyle = isLit ? lit : dim;
          ctx.globalAlpha = isLit ? 1 : selected ? 0.7 : 0.45;
          for (let x = 0; x < w; x++) {
            const t = (x / w) * d;
            if (t >= own) break;
            const pv = pk[Math.min(m - 1, Math.floor((t / own) * m))];
            const bar = Math.max(1, pv * (rowH - 8));
            ctx.fillRect(x, mid - bar / 2, 1, bar);
          }
          ctx.globalAlpha = 1;
        }
        ctx.fillStyle = dim; ctx.globalAlpha = 0.25; ctx.fillRect(0, mid, w, 1); ctx.globalAlpha = 1;
        if (li > 0) { ctx.fillStyle = dim; ctx.globalAlpha = 0.35; ctx.fillRect(0, top, w, 1); ctx.globalAlpha = 1; }
        ctx.fillStyle = selected ? lit : dim;
        ctx.globalAlpha = 0.95;
        ctx.font = '700 10px Inter, sans-serif';
        ctx.fillText(row.label, 6, top + 12);
        ctx.globalAlpha = 1;
      });
    }
  }
  function draw() {
    if (!canvas) return;
    const dpr = window.devicePixelRatio || 1;
    const w = canvas.clientWidth, h = canvas.clientHeight;
    if (!w || !h) return;
    if (canvas.width !== w * dpr || canvas.height !== h * dpr) { canvas.width = w * dpr; canvas.height = h * dpr; }
    const key = [w, h, dpr, currentPath, rows.map(r => r.file.path).join('|'), Object.keys(peaks).length, document.documentElement.className].join('#');
    if (key !== layerKey || !layerDim) { paintLayers(w, h, dpr); layerKey = key; }
    const ctx = canvas.getContext('2d');
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.clearRect(0, 0, canvas.width, canvas.height);
    ctx.drawImage(layerDim, 0, 0);
    const n = Math.max(1, rows.length);
    const li = rows.findIndex(r => r.file.path === currentPath);
    const playedX = Math.round((currentTime / (longest || 1)) * w);
    if (li >= 0 && playedX > 0) {
      const rowH = (h / n) * dpr;
      ctx.drawImage(layerLit, 0, li * rowH, playedX * dpr, rowH, 0, li * rowH, playedX * dpr, rowH);
    }
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.fillStyle = playheadColour;
    ctx.fillRect(playedX, 0, 2, h);
  }
  $effect(() => { void rows; void peaks; void currentTime; void currentPath; draw(); });
  $effect(() => {
    if (!canvas) return;
    const ro = new ResizeObserver(() => draw());
    ro.observe(canvas);
    return () => ro.disconnect();
  });
  let raf = 0;
  function tick() {
    if (mediaEl && isPlaying) { currentTime = mediaEl.currentTime; raf = requestAnimationFrame(tick); }
  }
  $effect(() => {
    if (isPlaying) raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  });

  function seekTo(t) {
    if (!mediaEl) return;
    if (!loaded) { pendingSeek = t; return; }
    const d = current?.file.durationSecs || mediaEl.duration || 0;
    mediaEl.currentTime = Math.max(0, Math.min(d > 0 ? d - 0.05 : t, t));
    currentTime = mediaEl.currentTime;
  }
  // Click a row: switch to that file, keeping the playhead where it is (and
  // keep playing if it was). Click along the row to move the playhead.
  function clickWave(e) {
    const rect = e.currentTarget.getBoundingClientRect();
    const li = Math.floor(((e.clientY - rect.top) / rect.height) * rows.length);
    const row = rows[Math.max(0, Math.min(rows.length - 1, li))];
    const t = ((e.clientX - rect.left) / rect.width) * (longest || 0);
    if (row && row.file.path !== currentPath) {
      pendingPlay = isPlaying;
      loaded = false;
      currentPath = row.file.path;
      pendingSeek = t;
      currentTime = t;
    } else {
      seekTo(t);
    }
  }
  function onLoaded() {
    loaded = true;
    if (pendingSeek != null) { const t = pendingSeek; pendingSeek = null; seekTo(t); }
    if (pendingPlay) { pendingPlay = false; mediaEl.play(); }
  }
  function togglePlay() {
    if (!mediaEl || !loaded) return;
    if (isPlaying) mediaEl.pause(); else mediaEl.play();
  }
  function fmtTime(t) {
    if (!isFinite(t)) return '0:00.0';
    const m = Math.floor(t / 60), s = t - m * 60;
    return `${m}:${s.toFixed(1).padStart(4, '0')}`;
  }
</script>

<div class="stem-check">
  {#if rows.length < 2}
    <p class="hint">Drop the mix and its stems together to see them side by side.</p>
  {:else}
    <p class="hint">Every file on one time axis. Click a row to hear that file; click along it to move the playhead. The mix is drawn first.</p>
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <div class="wave-wrap" style="height: {Math.max(120, rows.length * 44)}px" onclick={clickWave} title="Click a row to hear it">
      <canvas class="wave" bind:this={canvas}></canvas>
    </div>
    <div class="row">
      <button class="play" onclick={togglePlay} disabled={!loaded} title="Play / pause (space)">
        {#if isPlaying}
          <svg width="18" height="18" viewBox="0 0 16 16" fill="none"><rect x="3" y="2" width="4" height="12" rx="1" fill="currentColor"/><rect x="9" y="2" width="4" height="12" rx="1" fill="currentColor"/></svg>
        {:else}
          <svg width="18" height="18" viewBox="0 0 16 16" fill="none"><path d="M4 2L14 8L4 14V2Z" fill="currentColor"/></svg>
        {/if}
      </button>
      <span class="time">{fmtTime(currentTime)} / {fmtTime(current?.file.durationSecs || 0)}</span>
      <span class="now" title={current?.file.path}>{current ? (current.isMix ? 'MIX · ' : '') + current.file.filename : ''}</span>
    </div>
    {#if current}
      <audio bind:this={mediaEl} src={assetUrl} preload="auto"
        onloadedmetadata={onLoaded}
        ontimeupdate={() => { if (!isPlaying) currentTime = mediaEl.currentTime; }}
        onplay={() => isPlaying = true} onpause={() => isPlaying = false} onended={() => isPlaying = false}></audio>
    {/if}
  {/if}
</div>

<svelte:window onkeydown={(e) => {
  if (e.key === ' ' && e.target === document.body) { e.preventDefault(); togglePlay(); }
}} />

<style>
  .stem-check { display: flex; flex-direction: column; gap: 10px; }
  .hint { margin: 0; font-family: var(--font-text); font-weight: 500; font-size: 12.5px; line-height: 1.5; color: var(--text-secondary); }
  .wave-wrap { position: relative; cursor: pointer; background: var(--bg-dark); border-radius: 3px; border: 1px solid var(--border-color); }
  .wave { display: block; width: 100%; height: 100%; }
  .row { display: flex; align-items: center; gap: 12px; flex-wrap: wrap; }
  .play { width: 40px; height: 40px; border-radius: 50%; display: flex; align-items: center; justify-content: center; color: var(--bg-dark); background: var(--neon-cyan); border: 1px solid var(--neon-cyan); cursor: pointer; box-shadow: var(--cap-shadow); flex-shrink: 0; }
  .play:disabled { opacity: 0.35; cursor: default; }
  .time { font-family: var(--font-mono); font-size: 14px; font-weight: 700; color: var(--text-primary); min-width: 128px; }
  .now { font-family: var(--font-text); font-weight: 500; font-size: 13px; color: var(--text-primary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
</style>
