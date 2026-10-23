<script>
  import { convertFileSrc, invoke } from '@tauri-apps/api/core';
  import SoloSlateButton from './SoloSlateButton.svelte';
  import { drawWaveform } from '../waveform.js';

  // A video dropped on its own: a small resizable viewer, the soundtrack's
  // waveform, what the audio measures (information, not a verdict), and the
  // things you can make from it — a working file, the audio on its own, a slate.
  let {
    video, info = null, slateStatus = null, extractStatus = null,
    workingFormat = 'lt', onWorkingFormatChange,
    workingStatus = null, onMakeWorkingFile, onExtractAudio, onOpenSlate, onReveal,
  } = $props();

  const FORMATS = [
    { value: 'proxy', label: 'ProRes 422 Proxy' },
    { value: 'lt',    label: 'ProRes 422 LT' },
    { value: '422',   label: 'ProRes 422' },
    { value: 'hq',    label: 'ProRes 422 HQ' },
    { value: 'h264',  label: 'H.264 (MP4)' },
  ];
  let fmtLabel = $derived(FORMATS.find(f => f.value === workingFormat)?.label ?? 'ProRes 422 LT');

  let src = $derived.by(() => { try { return convertFileSrc(video.path); } catch { return ''; } });
  let videoEl = $state(null);
  let canvas = $state(null);
  let peaks = $state(null);
  let currentTime = $state(0);
  let duration = $derived(video.durationSecs || 0);
  let hasAudio = $derived(video.channelCount != null || video.sampleRate != null);

  $effect(() => {
    const path = video.path;
    peaks = null;
    if (!hasAudio) { peaks = []; return; }
    let alive = true;
    invoke('waveform_lanes', { audioPath: path, buckets: 600, channels: video.channelCount ?? 2 }).then(p => { if (alive) peaks = p; }).catch(() => { if (alive) peaks = []; });
    return () => { alive = false; };
  });

  // Playhead follows the viewer; clicking the waveform seeks it.
  let raf = 0;
  function tick() { if (videoEl && !videoEl.paused) { currentTime = videoEl.currentTime; raf = requestAnimationFrame(tick); } }
  function onPlay() { raf = requestAnimationFrame(tick); }
  function onPause() { cancelAnimationFrame(raf); if (videoEl) currentTime = videoEl.currentTime; }
  function seekWave(e) {
    if (!videoEl || !duration) return;
    const r = e.currentTarget.getBoundingClientRect();
    videoEl.currentTime = ((e.clientX - r.left) / r.width) * duration;
    currentTime = videoEl.currentTime;
  }
  function draw() { drawWaveform(canvas, peaks, { currentTime, duration }); }
  $effect(() => { peaks; currentTime; canvas; draw(); });
  $effect(() => { if (!canvas) return; const ro = new ResizeObserver(draw); ro.observe(canvas); return () => ro.disconnect(); });

  // Audio information, written as facts.
  const CH = ['L', 'R', 'C', 'LFE', 'Ls', 'Rs', 'Lss', 'Rss'];
  function fmt(n, d = 1) { return typeof n === 'number' && Number.isFinite(n) ? n.toFixed(d) : '—'; }
  function fmtT(t) { const m = Math.floor(t / 60), s = t - m * 60; return `${m}:${s.toFixed(1).padStart(4, '0')}`; }
  let spec = $derived.by(() => {
    const bits = [];
    if (video.sampleRate) bits.push(`${video.sampleRate % 1000 === 0 ? video.sampleRate / 1000 : (video.sampleRate / 1000).toFixed(1)}k`);
    if (video.bitDepth) bits.push(video.bitDepth.endsWith('f') ? `${video.bitDepth.slice(0, -1)}-bit float` : `${video.bitDepth}-bit`);
    else if (video.bitRate) bits.push(`${Math.round(video.bitRate / 1000)} kbps`);
    if (video.channelCount) bits.push(video.channelCount === 1 ? 'mono' : video.channelCount === 2 ? 'stereo' : `${video.channelCount}ch`);
    return bits.join(' · ');
  });
  let videoSpec = $derived.by(() => {
    const codec = { h264: 'H.264', hevc: 'HEVC', prores: 'ProRes', dnxhd: 'DNxHD', mpeg2video: 'MPEG-2', mjpeg: 'MJPEG' }[video.codecInfo] ?? (video.codecInfo ? video.codecInfo.toUpperCase() : '');
    const bits = [];
    if (codec) bits.push(codec);
    if (video.width && video.height) bits.push(`${video.width}×${video.height}`);
    if (video.frameRate) bits.push(`${Number.isInteger(video.frameRate) ? video.frameRate : video.frameRate.toFixed(2)} fps`);
    return bits.join(' · ');
  });
  function stereoWord(st) {
    if (!st) return '';
    return { stereo: 'stereo', dual_mono: 'dual mono (L = R)', anti_phase: 'anti-phase', one_sided: 'one-sided', mono: 'mono', multi: `${st.channels} channels` }[st.verdict] ?? st.verdict;
  }
  function seekTo(t) { if (videoEl) { videoEl.currentTime = Math.max(0, t - 0.75); currentTime = videoEl.currentTime; videoEl.play(); } }
</script>

<div class="solo-card">
  <div class="top">
    <div class="viewer" title="Drag the corner to resize">
      <video bind:this={videoEl} {src} controls playsinline preload="metadata"
        onplay={onPlay} onpause={onPause} onended={onPause} onseeked={() => currentTime = videoEl.currentTime}></video>
    </div>
    <div class="side">
      <div class="name">{video.filename}</div>
      <div class="meta">{videoSpec}{videoSpec && spec ? ' · ' : ''}{spec}{duration ? ` · ${fmtT(duration)}` : ''}</div>

      <div class="actions">
        <label class="wf">
          <span class="wf-label">WORKING FILE</span>
          <select class="sel" value={workingFormat} onchange={(e) => onWorkingFormatChange?.(e.target.value)} title="Remembered for next time">
            {#each FORMATS as f}<option value={f.value}>{f.label}</option>{/each}
          </select>
        </label>
        <button class="cap make" class:done={workingStatus?.state === 'done'} class:working={workingStatus?.state === 'working'}
          onclick={() => workingStatus?.state === 'done' ? onReveal(workingStatus.output) : workingStatus?.state === 'working' ? null : onMakeWorkingFile(video)}
          title={workingStatus?.state === 'done' ? 'Working file ready — click to show in Finder' : `Make a ${fmtLabel} working file beside the video`}>
          {#if workingStatus?.state === 'working'}
            <span class="fill" style="width:{workingStatus.pct ?? 0}%"></span><span class="lbl">{workingStatus.pct ?? 0}%</span>
          {:else if workingStatus?.state === 'done'}✓ {fmtLabel.startsWith('H.264') ? 'H.264' : 'PRORES'} MADE
          {:else if workingStatus?.state === 'error'}MAKE ✕ — TRY AGAIN
          {:else}MAKE {fmtLabel.startsWith('H.264') ? 'H.264' : 'PRORES'}{/if}
        </button>
        {#if hasAudio}
          <button class="cap" class:done={extractStatus?.state === 'done'} class:working={extractStatus?.state === 'working'}
            onclick={() => extractStatus?.state === 'done' ? onReveal(extractStatus.output) : onExtractAudio(video)}
            title={extractStatus?.state === 'done' ? 'Audio file ready — click to show in Finder' : 'Extract the soundtrack as a 24-bit WAV beside the video'}>
            {extractStatus?.state === 'working' ? 'EXTRACTING…' : extractStatus?.state === 'done' ? '✓ AUDIO EXTRACTED' : extractStatus?.state === 'error' ? 'EXTRACT ✕' : 'EXTRACT AUDIO'}
          </button>
        {/if}
        {#if onOpenSlate}
          <SoloSlateButton {video} status={slateStatus} {onOpenSlate} {onReveal} />
        {/if}
      </div>
    </div>
  </div>

  {#if hasAudio}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <div class="wave-wrap" onclick={seekWave} title="The video's soundtrack — click to seek">
      <canvas class="wave" bind:this={canvas}></canvas>
      {#if peaks === null}<span class="wave-note">DRAWING WAVEFORM…</span>{/if}
    </div>

    <div class="info">
      <span class="info-label">AUDIO IN THIS FILE</span>
      {#if !info || info.loading}
        <span class="info-chip muted">measuring…</span>
      {:else if info.error}
        <span class="info-chip muted" title={info.error}>could not measure</span>
      {:else}
        <span class="reading"><b>{fmt(info.lufs)}</b><small>LUFS</small></span>
        <span class="reading"><b>{fmt(info.tp)}</b><small>dBTP</small></span>
        {#if info.stereo}<span class="info-chip" title={info.stereo.correlation != null ? `phase correlation ${info.stereo.correlation.toFixed(2)}` : ''}>{stereoWord(info.stereo)}{info.stereo.correlation != null ? ` · ${info.stereo.correlation >= 0 ? '+' : ''}${info.stereo.correlation.toFixed(2)}` : ''}</span>{/if}
        {#if info.head != null}<span class="info-chip">{!info.head && !info.tail ? 'head & tail silent' : info.head && info.tail ? 'sound at head & tail' : info.head ? 'sound at head' : 'sound at tail'}</span>{/if}
        <span class="info-chip">clicks: {info.clicks ? (info.clicks.count || 'none') : '—'}{info.clicks?.headPop ? ' · head pop' : ''}{info.clicks?.tailPop ? ' · tail pop' : ''}</span>
        <span class="info-chip">clipping: {info.clipping ? (info.clipping.count ? `${info.clipping.count} run${info.clipping.count === 1 ? '' : 's'}` : 'none') : '—'}</span>
        <span class="info-chip">dropouts: {info.dropouts ? (info.dropouts.count || 'none') : '—'}</span>
        {#if info.clicks?.clicks?.length}
          <span class="info-chip list">at {#each info.clicks.clicks.slice(0, 6) as c, i}{i ? ', ' : ''}<button class="t" onclick={() => seekTo(c.time)}>{fmtT(c.time)}{info.clicks.channels > 1 ? ` ${c.allChannels ? 'L+R' : (CH[c.channel] ?? '')}` : ''}</button>{/each}{info.clicks.count > 6 ? '…' : ''}</span>
        {/if}
      {/if}
    </div>
  {:else}
    <div class="info"><span class="info-label">AUDIO IN THIS FILE</span><span class="info-chip muted">no audio track</span></div>
  {/if}
</div>

<style>
  .solo-card {
    width: 100%;
    background: var(--bg-panel);
    border: 1px solid var(--border-color);
    border-radius: var(--radius-md);
    padding: 10px 12px;
    display: flex; flex-direction: column; gap: 8px;
    text-align: left;
  }
  .top { display: flex; gap: 14px; align-items: flex-start; flex-wrap: wrap; }
  /* Small, and resizable from the corner (the OS draws the grip). */
  .viewer {
    width: 288px; height: 162px; min-width: 200px; min-height: 112px; max-width: 100%;
    resize: both; overflow: hidden;
    background: #000; border: 1px solid var(--border-color); border-radius: var(--radius-sm);
    flex-shrink: 0;
  }
  .viewer video { width: 100%; height: 100%; display: block; object-fit: contain; background: #000; }
  .side { flex: 1 1 280px; min-width: 0; display: flex; flex-direction: column; gap: 8px; }
  .name { font-family: var(--font-text); font-weight: 600; font-size: 14px; color: var(--text-primary); overflow-wrap: anywhere; }
  .meta { font-family: var(--font-mono); font-size: 11.5px; color: var(--text-secondary); }
  .actions { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; margin-top: 4px; }
  .wf { display: inline-flex; align-items: center; gap: 6px; }
  .wf-label { font-family: var(--font-display); font-weight: 700; font-size: 10px; letter-spacing: 0.04em; color: var(--text-secondary); }
  .sel { font-size: 12px; font-weight: 700; padding: 4px 24px 4px 10px; color: var(--text-primary); }
  .cap {
    font-family: var(--font-display); font-weight: 700; font-size: 11px; letter-spacing: 0.04em; color: var(--text-secondary);
    background: var(--cap-face); border: 1px solid var(--border-color); border-radius: var(--radius-sm);
    padding: 5px 10px; cursor: pointer; box-shadow: var(--cap-shadow); transition: all 0.15s;
  }
  .cap:hover { color: var(--neon-cyan); border-color: rgba(8, 247, 254, 0.5); }
  .cap.done { color: var(--neon-green); border-color: rgba(57, 255, 20, 0.4); }
  .cap.working { opacity: 0.85; cursor: wait; }
  .cap.make { position: relative; overflow: hidden; min-width: 112px; }
  .cap.make .fill { position: absolute; left: 0; top: 0; bottom: 0; background: rgba(8, 247, 254, 0.18); transition: width 0.2s; }
  .cap.make .lbl { position: relative; }

  .wave-wrap { position: relative; height: 72px; cursor: pointer; background: var(--bg-dark); border: 1px solid var(--border-color); border-radius: 3px; }
  .wave { display: block; width: 100%; height: 100%; }
  .wave-note { position: absolute; inset: 0; display: flex; align-items: center; justify-content: center; font-family: var(--font-display); font-weight: 700; font-size: 10px; letter-spacing: 0.08em; color: var(--text-secondary); pointer-events: none; }

  /* One line, always — it scrolls sideways rather than wrapping. */
  .info { display: flex; align-items: center; gap: 5px; flex-wrap: nowrap; overflow-x: auto; white-space: nowrap; padding-bottom: 2px; scrollbar-width: thin; }
  .info > * { flex-shrink: 0; }
  .info-label { font-family: var(--font-display); font-weight: 700; font-size: 10px; letter-spacing: 0.04em; color: var(--text-secondary); margin-right: 4px; }
  .info-chip {
    font-family: var(--font-mono); font-size: 11px; color: var(--text-primary);
    background: var(--bg-dark); border: 1px solid var(--border-color); border-radius: 3px; padding: 2px 6px;
  }
  .info-chip b { font-weight: 700; color: var(--neon-cyan); }
  .reading { display: inline-flex; align-items: baseline; gap: 3px; margin-right: 4px; }
  .reading b { font-family: var(--font-mono); font-size: 18px; font-weight: 700; color: var(--neon-cyan); }
  .reading small { font-family: var(--font-display); font-weight: 700; font-size: 10px; letter-spacing: 0.04em; color: var(--text-secondary); }
  .info-chip.muted { color: var(--text-secondary); }
  .info-chip .t { font: inherit; color: var(--neon-cyan); background: none; border: none; padding: 0; cursor: pointer; }
  .info-chip .t:hover { text-decoration: underline; }
</style>
