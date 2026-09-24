<script>
  import { convertFileSrc, invoke } from '@tauri-apps/api/core';

  // The QC player: always on screen under the QC table. Pick a file, see its
  // waveform with every fault marked on it, and play from any of them.
  // `cue` = { pairId, time, nonce } — set by the table's time buttons.
  let { pairs = [], qcResults = {}, cue = null } = $props();

  let currentId = $state(null);
  let mediaEl = $state(null);
  let canvas = $state(null);
  let peaks = $state(null);
  let isPlaying = $state(false);
  let currentTime = $state(0);
  let duration = $state(0);
  let loaded = $state(false);
  let pendingSeek = null;

  let current = $derived(pairs.find(p => p.id === currentId) ?? null);
  let result = $derived(current ? (qcResults[current.id] ?? null) : null);
  // (Guarded: outside Tauri there is no file bridge, and the page must still render.)
  let assetUrl = $derived.by(() => {
    if (!current) return '';
    try { return convertFileSrc(current.audio.path); } catch { return ''; }
  });

  const CH = ['L', 'R', 'C', 'LFE', 'Ls', 'Rs', 'Lss', 'Rss'];
  function chLabel(k, channels) {
    if (!channels || channels < 2) return '';
    if (k.allChannels) return channels === 2 ? 'L+R' : 'ALL';
    return CH[k.channel] ?? `ch${k.channel + 1}`;
  }

  // Faults on the current file, as markers: { time, kind, label, channel }.
  let markers = $derived.by(() => {
    if (!result || result.error) return [];
    const m = [];
    const c = result.clicks;
    if (c && !c.error) {
      if (c.headPop) m.push({ time: 0, kind: 'click', label: 'Pop at the head' });
      for (const k of c.clicks) m.push({ time: k.time, kind: 'click', channel: chLabel(k, c.channels), label: `Click · ${k.prominenceDb.toFixed(0)} dB above the audio around it` });
      if (c.tailPop && current) m.push({ time: current.audio.durationSecs, kind: 'click', label: 'Pop at the tail' });
    }
    const cl = result.clipping;
    if (cl && !cl.error) for (const e of cl.events) m.push({ time: e.time, kind: 'clip', channel: chLabel(e, current?.audio.channelCount), label: `Clipping · ${e.samples} samples at ${e.levelDb.toFixed(1)} dBFS` });
    const d = result.dropouts;
    if (d && !d.error) for (const g of d.gaps) m.push({ time: g.start, end: g.start + g.duration, kind: 'gap', label: `Dropout · ${(g.duration * 1000).toFixed(0)} ms of silence` });
    if (result.silenceChecked && !result.silencePass && current) {
      if (result.headHasAudio) m.push({ time: 0, end: 0.24, kind: 'sixfr', label: 'Sound in the first 6 frames' });
      if (result.tailHasAudio) m.push({ time: current.audio.durationSecs - 0.24, end: current.audio.durationSecs, kind: 'sixfr', label: 'Sound in the last 6 frames' });
    }
    return m.sort((a, b) => a.time - b.time);
  });

  // Pick the first failing file once results land, unless the user already chose.
  $effect(() => {
    const ids = pairs.map(p => p.id);
    if (currentId && !ids.includes(currentId)) currentId = null;
    if (!currentId && pairs.length) {
      const firstFail = pairs.find(p => qcResults[p.id] && !qcResults[p.id].pass);
      currentId = (firstFail ?? pairs[0]).id;
    }
  });

  // A cue from the table: switch file if needed, then seek and play.
  $effect(() => {
    if (!cue) return;
    const { pairId, time } = cue;
    pendingSeek = Math.max(0, time - 0.75);
    if (pairId !== currentId) {
      currentId = pairId;
    } else if (mediaEl && loaded) {
      seekTo(pendingSeek);
      pendingSeek = null;
      mediaEl.play();
    }
  });

  // Load peaks whenever the file changes.
  $effect(() => {
    const path = current?.audio.path;
    peaks = null; loaded = false; isPlaying = false; currentTime = 0; duration = 0;
    if (!path) return;
    let alive = true;
    invoke('waveform_peaks', { audioPath: path, buckets: 800 })
      .then(p => { if (alive) peaks = p; })
      .catch(() => { if (alive) peaks = []; });
    return () => { alive = false; };
  });

  function cssVar(name, fallback) {
    const v = getComputedStyle(document.documentElement).getPropertyValue(name).trim();
    return v || fallback;
  }

  function draw() {
    if (!canvas) return;
    const dpr = window.devicePixelRatio || 1;
    const w = canvas.clientWidth, h = canvas.clientHeight;
    if (!w || !h) return;
    if (canvas.width !== w * dpr || canvas.height !== h * dpr) { canvas.width = w * dpr; canvas.height = h * dpr; }
    const ctx = canvas.getContext('2d');
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, w, h);
    const lit = cssVar('--neon-cyan', '#08f7fe');
    const dim = cssVar('--text-muted', '#8888aa');
    const orange = cssVar('--neon-orange', '#ff9500');
    const dur = duration || current?.audio.durationSecs || 0;
    const mid = h / 2;
    const playedX = dur > 0 ? (currentTime / dur) * w : 0;
    if (peaks && peaks.length) {
      const n = peaks.length;
      for (let x = 0; x < w; x++) {
        const p = peaks[Math.min(n - 1, Math.floor((x / w) * n))];
        const bar = Math.max(1, p * (h - 6));
        ctx.fillStyle = x <= playedX ? lit : dim;
        ctx.globalAlpha = x <= playedX ? 1 : 0.5;
        ctx.fillRect(x, mid - bar / 2, 1, bar);
      }
      ctx.globalAlpha = 1;
    }
    ctx.fillStyle = dim; ctx.globalAlpha = 0.25; ctx.fillRect(0, mid, w, 1); ctx.globalAlpha = 1;
    // Fault markers: a translucent band for a span, a full-height line for a point.
    if (dur > 0) {
      for (const m of markers) {
        const x0 = (m.time / dur) * w;
        const x1 = m.end != null ? (m.end / dur) * w : x0;
        ctx.fillStyle = orange;
        if (x1 - x0 >= 2) {
          ctx.globalAlpha = 0.35; ctx.fillRect(x0, 0, x1 - x0, h);
        } else {
          ctx.globalAlpha = 0.9; ctx.fillRect(Math.round(x0) - 1, 0, 2, h);
        }
        ctx.globalAlpha = 1;
        // A small flag at the top so single-sample marks are findable.
        ctx.beginPath(); ctx.moveTo(x0 - 5, 0); ctx.lineTo(x0 + 5, 0); ctx.lineTo(x0, 7); ctx.closePath(); ctx.fill();
      }
      ctx.fillStyle = lit;
      ctx.fillRect(Math.round(playedX), 0, 2, h);
    }
  }
  $effect(() => { peaks; currentTime; duration; markers; canvas; draw(); });
  $effect(() => {
    if (!canvas) return;
    const ro = new ResizeObserver(() => draw());
    ro.observe(canvas);
    return () => ro.disconnect();
  });

  // The audio element only reports time a few times a second; while playing,
  // read it every frame so the playhead glides.
  let raf = 0;
  function tick() {
    if (mediaEl && isPlaying) {
      currentTime = mediaEl.currentTime;
      raf = requestAnimationFrame(tick);
    }
  }
  $effect(() => {
    if (isPlaying) raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  });

  function seekTo(t) {
    if (!mediaEl) return;
    const d = duration || current?.audio.durationSecs || 0;
    mediaEl.currentTime = Math.max(0, Math.min(d > 0 ? d - 0.05 : t, t));
    currentTime = mediaEl.currentTime;
  }
  function seekWave(e) {
    const rect = e.currentTarget.getBoundingClientRect();
    const d = duration || current?.audio.durationSecs || 0;
    seekTo(((e.clientX - rect.left) / rect.width) * d);
  }
  function togglePlay() {
    if (!mediaEl || !loaded) return;
    if (isPlaying) mediaEl.pause(); else mediaEl.play();
  }
  function onLoaded() {
    duration = mediaEl.duration;
    loaded = true;
    if (pendingSeek != null) {
      seekTo(pendingSeek);
      pendingSeek = null;
      mediaEl.play();
    }
  }
  function jump(dir) {
    // Next / previous fault from the playhead.
    const t = currentTime;
    const list = dir > 0 ? markers.filter(m => m.time > t + 0.05) : markers.filter(m => m.time < t - 0.05).reverse();
    if (list.length) { seekTo(Math.max(0, list[0].time - 0.75)); mediaEl?.play(); }
  }
  function fmt(t) {
    if (!isFinite(t)) return '0:00.0';
    const m = Math.floor(t / 60), s = t - m * 60;
    return `${m}:${s.toFixed(1).padStart(4, '0')}`;
  }
  function verdict(p) {
    const r = qcResults[p.id];
    return !r ? '' : r.error ? '?' : r.pass ? 'PASS' : 'FAIL';
  }
</script>

<div class="player" class:has-file={!!current}>
  <div class="row top">
    <span class="label">PLAYER</span>
    <select class="file-select" value={currentId ?? ''} onchange={(e) => currentId = e.target.value}>
      {#each pairs as p (p.id)}
        <option value={p.id}>{verdict(p) ? `${verdict(p)} · ` : ''}{p.audio.filename}</option>
      {/each}
    </select>
    {#if result && !result.error}
      <span class="verdict" class:pass={result.pass} class:fail={!result.pass}>{result.pass ? 'PASS' : 'FAIL'}</span>
    {/if}
    <span class="spacer"></span>
    {#if markers.length}
      <span class="fault-count">{markers.length} fault{markers.length === 1 ? '' : 's'} marked</span>
      <button class="cap" onclick={() => jump(-1)} title="Previous fault">◀ FAULT</button>
      <button class="cap" onclick={() => jump(1)} title="Next fault">FAULT ▶</button>
    {/if}
  </div>

  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div class="wave-wrap" onclick={seekWave} title="Click to seek — orange marks are faults">
    <canvas class="wave" bind:this={canvas}></canvas>
    {#if current && peaks === null}<span class="wave-note">DRAWING WAVEFORM…</span>{/if}
    {#if !current}<span class="wave-note">RUN QC, then pick a file or click a time in the table</span>{/if}
  </div>

  <div class="row transport">
    <button class="play" onclick={togglePlay} disabled={!loaded} title="Play / pause (space)">
      {#if isPlaying}
        <svg width="18" height="18" viewBox="0 0 16 16" fill="none"><rect x="3" y="2" width="4" height="12" rx="1" fill="currentColor"/><rect x="9" y="2" width="4" height="12" rx="1" fill="currentColor"/></svg>
      {:else}
        <svg width="18" height="18" viewBox="0 0 16 16" fill="none"><path d="M4 2L14 8L4 14V2Z" fill="currentColor"/></svg>
      {/if}
    </button>
    <span class="time">{fmt(currentTime)} / {fmt(duration || current?.audio.durationSecs || 0)}</span>
    {#if markers.length}
      <div class="marker-chips">
        {#each markers.slice(0, 12) as m}
          <button class="chip" onclick={() => { seekTo(Math.max(0, m.time - 0.75)); mediaEl?.play(); }} title={m.label}>
            {m.kind === 'click' ? 'CLICK' : m.kind === 'clip' ? 'CLIP' : m.kind === 'gap' ? 'GAP' : '6 Fr'} {fmt(m.time)}{m.channel ? ` ${m.channel}` : ''}
          </button>
        {/each}
        {#if markers.length > 12}<span class="more">+{markers.length - 12} more on the waveform</span>{/if}
      </div>
    {/if}
  </div>

  {#if current}
    <audio bind:this={mediaEl} src={assetUrl} preload="auto"
      onloadedmetadata={onLoaded}
      ontimeupdate={() => { if (!isPlaying) currentTime = mediaEl.currentTime; }}
      onplay={() => isPlaying = true} onpause={() => isPlaying = false} onended={() => isPlaying = false}></audio>
  {/if}
</div>

<svelte:window onkeydown={(e) => {
  if (e.key === ' ' && e.target === document.body) { e.preventDefault(); togglePlay(); }
}} />

<style>
  .player {
    margin-top: 10px;
    border: 1px solid var(--border-color);
    border-radius: var(--radius-sm);
    background: var(--bg-dark);
    padding: 10px 12px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .row { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; }
  .label { font-family: var(--font-display); font-size: 12.5px; letter-spacing: 0.12em; color: var(--neon-cyan); }
  .file-select { font-size: 13px; font-weight: 700; max-width: 520px; color: var(--text-primary); }
  .verdict {
    font-family: var(--font-display); font-size: 11px; letter-spacing: 0.12em;
    padding: 2px 8px; border-radius: 3px; border: 1px solid currentColor;
  }
  .verdict.pass { color: var(--neon-green); }
  .verdict.fail { color: var(--neon-orange); }
  .spacer { flex: 1; }
  .fault-count { font-family: var(--font-mono); font-size: 12.5px; color: var(--neon-orange); font-weight: 700; }
  .cap {
    font-family: var(--font-display); font-size: 11.5px; letter-spacing: 0.1em; color: var(--text-secondary);
    background: var(--cap-face); border: 1px solid var(--border-color); border-radius: var(--radius-sm);
    padding: 5px 10px; cursor: pointer; box-shadow: var(--cap-shadow);
  }
  .cap:hover { color: var(--neon-cyan); border-color: rgba(8, 247, 254, 0.5); }

  .wave-wrap { position: relative; height: 96px; cursor: pointer; background: var(--bg-panel); border-radius: 3px; border: 1px solid var(--border-color); }
  .wave { display: block; width: 100%; height: 100%; }
  .wave-note {
    position: absolute; inset: 0; display: flex; align-items: center; justify-content: center;
    font-family: var(--font-display); font-size: 11.5px; letter-spacing: 0.12em; color: var(--text-secondary); pointer-events: none;
  }

  .play {
    width: 40px; height: 40px; border-radius: 50%;
    display: flex; align-items: center; justify-content: center;
    color: var(--bg-dark); background: var(--neon-cyan); border: 1px solid var(--neon-cyan);
    cursor: pointer; box-shadow: var(--cap-shadow); flex-shrink: 0;
  }
  .play:disabled { opacity: 0.35; cursor: default; }
  .time { font-family: var(--font-mono); font-size: 14px; font-weight: 700; color: var(--text-primary); min-width: 128px; }
  .marker-chips { display: flex; flex-wrap: wrap; gap: 5px; align-items: center; }
  .chip {
    font-family: var(--font-mono); font-size: 12px; font-weight: 700; color: var(--neon-orange);
    background: rgba(255, 149, 0, 0.08); border: 1px solid rgba(255, 149, 0, 0.4); border-radius: 3px;
    padding: 2px 8px; cursor: pointer;
  }
  .chip:hover { background: rgba(255, 149, 0, 0.2); }
  .more { font-family: var(--font-mono); font-size: 12px; color: var(--text-secondary); }
</style>
