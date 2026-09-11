<script>
  import { convertFileSrc, invoke } from '@tauri-apps/api/core';
  import { onMount } from 'svelte';

  let {
    filePath = '',
    filename = '',
    mediaType = 'video', // 'video' or 'audio'
    onClose,
  } = $props();

  let mediaEl = $state(null);
  let isPlaying = $state(false);
  let currentTime = $state(0);
  let duration = $state(0);
  let loaded = $state(false);
  let error = $state(null);

  let assetUrl = $derived(filePath ? convertFileSrc(filePath) : '');
  let progressPct = $derived(duration > 0 ? (currentTime / duration) * 100 : 0);

  // Waveform overview: peaks come from the backend (ffmpeg decode), drawn on
  // a canvas with the played part lit. Video previews get a slim strip of
  // their own soundtrack under the picture.
  let peaks = $state(null);
  let waveError = $state(false);
  let canvas = $state(null);
  const BUCKETS = 600;

  onMount(async () => {
    try {
      peaks = await invoke('waveform_peaks', { audioPath: filePath, buckets: BUCKETS });
    } catch {
      waveError = true;
    }
  });

  function cssVar(name, fallback) {
    const v = getComputedStyle(document.documentElement).getPropertyValue(name).trim();
    return v || fallback;
  }

  function drawWave() {
    if (!canvas || !peaks || peaks.length === 0) return;
    const dpr = window.devicePixelRatio || 1;
    const w = canvas.clientWidth, h = canvas.clientHeight;
    if (w === 0 || h === 0) return;
    if (canvas.width !== w * dpr || canvas.height !== h * dpr) {
      canvas.width = w * dpr; canvas.height = h * dpr;
    }
    const ctx = canvas.getContext('2d');
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, w, h);
    const lit = cssVar('--neon-cyan', '#08f7fe');
    const dim = cssVar('--text-muted', '#8888aa');
    const mid = h / 2;
    const n = peaks.length;
    const playedX = duration > 0 ? (currentTime / duration) * w : 0;
    // Bars: one per pixel column, from the nearest bucket.
    for (let x = 0; x < w; x++) {
      const p = peaks[Math.min(n - 1, Math.floor((x / w) * n))];
      const bar = Math.max(1, p * (h - 4));
      ctx.fillStyle = x <= playedX ? lit : dim;
      ctx.globalAlpha = x <= playedX ? 1 : 0.55;
      ctx.fillRect(x, mid - bar / 2, 1, bar);
    }
    ctx.globalAlpha = 1;
    // Centre line and playhead
    ctx.fillStyle = dim; ctx.globalAlpha = 0.25; ctx.fillRect(0, mid, w, 1); ctx.globalAlpha = 1;
    if (duration > 0) {
      ctx.fillStyle = lit;
      ctx.fillRect(Math.round(playedX), 0, 1.5, h);
    }
  }

  // Redraw whenever the peaks arrive, the playhead moves, or the modal resizes.
  $effect(() => {
    peaks; currentTime; duration; canvas;
    drawWave();
  });
  $effect(() => {
    if (!canvas) return;
    const ro = new ResizeObserver(() => drawWave());
    ro.observe(canvas);
    return () => ro.disconnect();
  });

  function seekWave(e) {
    if (!mediaEl || !duration) return;
    const rect = e.currentTarget.getBoundingClientRect();
    const pct = (e.clientX - rect.left) / rect.width;
    mediaEl.currentTime = Math.max(0, Math.min(duration, pct * duration));
  }

  function formatTime(secs) {
    const m = Math.floor(secs / 60);
    const s = Math.floor(secs % 60);
    return `${m}:${s.toString().padStart(2, '0')}`;
  }

  function togglePlay() {
    if (!mediaEl) return;
    if (isPlaying) {
      mediaEl.pause();
    } else {
      mediaEl.play();
    }
  }

  function handleTimeUpdate() {
    if (mediaEl) {
      currentTime = mediaEl.currentTime;
    }
  }

  function handleLoaded() {
    if (mediaEl) {
      duration = mediaEl.duration;
      loaded = true;
      mediaEl.play();
    }
  }

  function handlePlay() { isPlaying = true; }
  function handlePause() { isPlaying = false; }
  function handleEnded() { isPlaying = false; }
  function handleError() { error = 'Could not load media file'; }

  function seek(e) {
    if (!mediaEl || !duration) return;
    const rect = e.currentTarget.getBoundingClientRect();
    const pct = (e.clientX - rect.left) / rect.width;
    mediaEl.currentTime = pct * duration;
  }

  function handleKeydown(e) {
    if (e.key === 'Escape') onClose();
    if (e.key === ' ') { e.preventDefault(); togglePlay(); }
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<!-- svelte-ignore a11y_click_events_have_key_events -->
<div class="modal-overlay" onclick={onClose} onkeydown={handleKeydown}>
  <div class="modal-content" onclick={(e) => e.stopPropagation()}>
    <!-- Header -->
    <div class="modal-header">
      <span class="modal-type">{mediaType === 'video' ? 'VIDEO' : 'AUDIO'} PREVIEW</span>
      <span class="modal-filename" title={filename}>{filename}</span>
      <button class="close-btn" onclick={onClose}>&times;</button>
    </div>

    <!-- Media -->
    <div class="media-area" class:audio-only={mediaType === 'audio'}>
      {#if error}
        <div class="error-msg">{error}</div>
      {:else if mediaType === 'video'}
        <video
          bind:this={mediaEl}
          src={assetUrl}
          ontimeupdate={handleTimeUpdate}
          onloadedmetadata={handleLoaded}
          onplay={handlePlay}
          onpause={handlePause}
          onended={handleEnded}
          onerror={handleError}
          class="video-player"
          preload="auto"
        ></video>
      {:else}
        {#if peaks && peaks.length > 0}
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <div class="wave-wrap audio" onclick={seekWave} title="Click to seek">
            <canvas class="wave" bind:this={canvas}></canvas>
          </div>
        {:else}
          <div class="audio-visual" class:loading={!waveError}>
            <svg width="64" height="64" viewBox="0 0 64 64" fill="none">
              <circle cx="32" cy="32" r="28" stroke="currentColor" stroke-width="1.5" opacity="0.3"/>
              <path d="M32 12C32 12 36 12 36 18V46C36 52 32 52 32 52C32 52 28 52 28 46V18C28 12 32 12 32 12Z" stroke="currentColor" stroke-width="1.5"/>
              <path d="M20 28V40C20 46.6 25.4 52 32 52C38.6 52 44 46.6 44 40V28" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
              <path d="M32 52V60" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
              <path d="M26 60H38" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
            </svg>
            {#if isPlaying}
              <div class="audio-pulse"></div>
            {/if}
            {#if !waveError}<span class="wave-loading">DRAWING WAVEFORM…</span>{/if}
          </div>
        {/if}
        <audio
          bind:this={mediaEl}
          src={assetUrl}
          ontimeupdate={handleTimeUpdate}
          onloadedmetadata={handleLoaded}
          onplay={handlePlay}
          onpause={handlePause}
          onended={handleEnded}
          onerror={handleError}
          preload="auto"
        ></audio>
      {/if}
    </div>

    {#if mediaType === 'video' && peaks && peaks.length > 0}
      <!-- The video's own soundtrack, as a slim strip under the picture -->
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <div class="wave-wrap strip" onclick={seekWave} title="Click to seek">
        <canvas class="wave" bind:this={canvas}></canvas>
      </div>
    {/if}

    <!-- Transport -->
    <div class="transport">
      <button class="transport-btn" onclick={togglePlay} disabled={!loaded}>
        {#if isPlaying}
          <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
            <rect x="3" y="2" width="4" height="12" rx="1" fill="currentColor"/>
            <rect x="9" y="2" width="4" height="12" rx="1" fill="currentColor"/>
          </svg>
        {:else}
          <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
            <path d="M4 2L14 8L4 14V2Z" fill="currentColor"/>
          </svg>
        {/if}
      </button>

      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="progress-track" onclick={seek}>
        <div class="progress-fill" style="width: {progressPct}%"></div>
        <div class="progress-head" style="left: {progressPct}%"></div>
      </div>

      <span class="time-display">
        {formatTime(currentTime)} / {formatTime(duration)}
      </span>
    </div>
  </div>
</div>

<style>
  .modal-overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.85);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
    backdrop-filter: blur(4px);
  }

  .modal-content {
    background: var(--bg-raised);
    border: 1px solid var(--border-accent);
    border-radius: var(--radius-lg);
    overflow: hidden;
    display: flex;
    flex-direction: column;
    max-width: 720px;
    width: 90%;
    box-shadow: 0 16px 64px rgba(0, 0, 0, 0.6);
  }

  .modal-header {
    display: flex;
    align-items: center;
    gap: var(--gap-sm);
    padding: 10px 16px;
    border-bottom: 1px solid var(--border-color);
  }

  .modal-type {
    font-family: var(--font-display);
    font-size: 10px;
    letter-spacing: 0.12em;
    color: var(--neon-cyan);
    flex-shrink: 0;
  }

  .modal-filename {
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--text-secondary);
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .close-btn {
    background: none;
    border: 1px solid var(--border-color);
    color: var(--text-muted);
    font-size: 21px;
    line-height: 1;
    cursor: pointer;
    padding: 2px 8px;
    border-radius: var(--radius-sm);
    transition: all 0.15s;
    flex-shrink: 0;
  }

  .close-btn:hover {
    color: var(--neon-pink);
    border-color: rgba(255, 46, 99, 0.3);
    background: rgba(255, 46, 99, 0.1);
  }

  :global(:root.tame) .modal-overlay {
    background: rgba(0, 0, 0, 0.5);
  }

  :global(:root.tame) .modal-content {
    box-shadow: 0 16px 64px rgba(0, 0, 0, 0.15);
  }

  :global(:root.tame) .close-btn:hover {
    border-color: rgba(224, 122, 95, 0.3);
    background: rgba(224, 122, 95, 0.1);
  }

  .media-area {
    background: #000;
    display: flex;
    align-items: center;
    justify-content: center;
    min-height: 200px;
  }

  .media-area.audio-only {
    min-height: 160px;
    background: var(--bg-dark);
  }

  .video-player {
    width: 100%;
    max-height: 400px;
    display: block;
  }

  .audio-visual {
    position: relative;
    color: var(--neon-cyan);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 32px;
  }

  .audio-visual.loading { flex-direction: column; gap: 10px; }
  .wave-loading {
    font-family: var(--font-display);
    font-size: 9px;
    letter-spacing: 0.15em;
    color: var(--text-muted);
  }

  /* Waveform overview */
  .wave-wrap {
    width: 100%;
    cursor: pointer;
    background: var(--bg-dark);
  }
  .wave-wrap.audio { height: 160px; padding: 12px 16px; box-sizing: border-box; }
  .wave-wrap.strip { height: 56px; padding: 6px 16px; box-sizing: border-box; border-top: 1px solid var(--border-color); }
  .wave { display: block; width: 100%; height: 100%; }

  .audio-pulse {
    position: absolute;
    inset: 0;
    border-radius: 50%;
    border: 2px solid var(--neon-cyan);
    opacity: 0.3;
    animation: audio-ring 1.5s ease-out infinite;
  }

  @keyframes audio-ring {
    0% { transform: scale(0.8); opacity: 0.4; }
    100% { transform: scale(1.4); opacity: 0; }
  }

  .error-msg {
    font-family: var(--font-mono);
    font-size: 13px;
    color: var(--neon-pink);
    padding: 32px;
  }

  /* Transport */
  .transport {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px 16px;
    border-top: 1px solid var(--border-color);
  }

  .transport-btn {
    background: none;
    border: 1px solid var(--border-accent);
    color: var(--text-primary);
    cursor: pointer;
    padding: 6px 8px;
    border-radius: var(--radius-sm);
    display: flex;
    align-items: center;
    justify-content: center;
    transition: all 0.15s;
    flex-shrink: 0;
  }

  .transport-btn:hover:not(:disabled) {
    border-color: var(--neon-cyan);
    color: var(--neon-cyan);
    box-shadow: 0 0 8px rgba(8, 247, 254, 0.15);
  }

  .transport-btn:disabled {
    opacity: 0.3;
    cursor: default;
  }

  .progress-track {
    flex: 1;
    height: 6px;
    background: var(--bg-dark);
    border-radius: 3px;
    cursor: pointer;
    position: relative;
    overflow: visible;
  }

  .progress-fill {
    height: 100%;
    background: var(--neon-cyan);
    border-radius: 3px;
    transition: width 0.1s linear;
  }

  .progress-head {
    position: absolute;
    top: 50%;
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: var(--neon-cyan);
    transform: translate(-50%, -50%);
    box-shadow: 0 0 6px rgba(8, 247, 254, 0.4);
    opacity: 0;
    transition: opacity 0.15s;
  }

  .progress-track:hover .progress-head {
    opacity: 1;
  }

  .time-display {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--text-muted);
    flex-shrink: 0;
    min-width: 80px;
    text-align: right;
    letter-spacing: 0.03em;
  }
</style>
