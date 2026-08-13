// Slate card renderer: black frame, white centred text, sized to fit.
// Draws onto the given canvas at its current width/height. Shared by the
// live preview (small canvas) and the export render (full frame size).
export function drawSlate(canvas, text) {
  const w = canvas.width;
  const h = canvas.height;
  const ctx = canvas.getContext('2d');
  ctx.fillStyle = '#000000';
  ctx.fillRect(0, 0, w, h);
  const drawn = (text || '').split('\n').map(l => l.trim()).filter(l => l.length > 0);
  if (drawn.length === 0) return;
  ctx.fillStyle = '#ffffff';
  ctx.textAlign = 'center';
  ctx.textBaseline = 'middle';
  // Start at 1/10th of frame height, shrink until the widest line fits.
  let size = Math.floor(h / 10);
  const maxWidth = w * 0.86;
  while (size > 4) {
    ctx.font = `600 ${size}px "Helvetica Neue", Helvetica, Arial, sans-serif`;
    const widest = Math.max(...drawn.map(l => ctx.measureText(l).width));
    const total = drawn.length * size * 1.4;
    if (widest <= maxWidth && total <= h * 0.8) break;
    size -= 2;
  }
  const lineHeight = size * 1.4;
  const startY = h / 2 - ((drawn.length - 1) * lineHeight) / 2;
  drawn.forEach((line, i) => ctx.fillText(line, w / 2, startY + i * lineHeight));
}

// Render the slate at the video's exact frame size and return a JPEG data URL —
// JPEG because the bundled ffmpeg decodes mjpeg but not PNG.
export function renderSlateImage(text, width, height) {
  const canvas = document.createElement('canvas');
  canvas.width = width || 1920;
  canvas.height = height || 1080;
  drawSlate(canvas, text);
  return canvas.toDataURL('image/jpeg', 0.95);
}
