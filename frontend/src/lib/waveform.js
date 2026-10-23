// Shared waveform drawing: one lane per channel, stacked, with the played
// part lit, optional fault markers, and a playhead. `lanes` is an array of
// arrays of peaks in 0…1 (from the `waveform_lanes` command).

function cssVar(name, fallback) {
  const v = getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  return v || fallback;
}

export function drawWaveform(canvas, lanes, { currentTime = 0, duration = 0, markers = [] } = {}) {
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
  const playedX = duration > 0 ? (currentTime / duration) * w : 0;
  const n = Math.max(1, lanes?.length || 0);
  const laneH = h / n;

  for (let li = 0; li < n; li++) {
    const peaks = lanes?.[li] ?? [];
    const top = li * laneH, mid = top + laneH / 2;
    if (peaks.length) {
      const m = peaks.length;
      for (let x = 0; x < w; x++) {
        const p = peaks[Math.min(m - 1, Math.floor((x / w) * m))];
        const bar = Math.max(1, p * (laneH - 4));
        ctx.fillStyle = x <= playedX ? lit : dim;
        ctx.globalAlpha = x <= playedX ? 1 : 0.5;
        ctx.fillRect(x, mid - bar / 2, 1, bar);
      }
      ctx.globalAlpha = 1;
    }
    ctx.fillStyle = dim; ctx.globalAlpha = 0.25; ctx.fillRect(0, mid, w, 1); ctx.globalAlpha = 1;
    if (li > 0) { ctx.fillStyle = dim; ctx.globalAlpha = 0.35; ctx.fillRect(0, top, w, 1); ctx.globalAlpha = 1; }
  }
  // Channel letters for a stereo lane pair, so you know which is which.
  if (n === 2) {
    ctx.fillStyle = dim; ctx.globalAlpha = 0.8;
    ctx.font = '700 10px Inter, sans-serif';
    ctx.fillText('L', 4, 11); ctx.fillText('R', 4, laneH + 11);
    ctx.globalAlpha = 1;
  }
  if (duration > 0) {
    for (const mk of markers) {
      const x0 = (mk.time / duration) * w;
      const x1 = mk.end != null ? (mk.end / duration) * w : x0;
      ctx.fillStyle = orange;
      if (x1 - x0 >= 2) { ctx.globalAlpha = 0.35; ctx.fillRect(x0, 0, x1 - x0, h); }
      else { ctx.globalAlpha = 0.9; ctx.fillRect(Math.round(x0) - 1, 0, 2, h); }
      ctx.globalAlpha = 1;
      ctx.beginPath(); ctx.moveTo(x0 - 5, 0); ctx.lineTo(x0 + 5, 0); ctx.lineTo(x0, 7); ctx.closePath(); ctx.fill();
    }
    ctx.fillStyle = lit;
    ctx.fillRect(Math.round(playedX), 0, 2, h);
  }
}
