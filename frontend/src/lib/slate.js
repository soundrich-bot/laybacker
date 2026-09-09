// Slate card renderer: a background (black, or the user's own image filling the
// frame), white centred text in the chosen font and size, auto-shrunk so it
// always fits. Draws onto the given canvas at its current width/height. Shared
// by the live preview (small canvas) and the export render (full frame size).

// Named fonts. Each has a fallback stack so a face missing on one platform
// degrades to its nearest cousin rather than to the browser default.
export const SLATE_FONTS = {
  helvetica:  { label: 'Helvetica Neue',      family: '"Helvetica Neue", Helvetica, Arial, sans-serif', weight: 600 },
  arial:      { label: 'Arial',               family: 'Arial, Helvetica, sans-serif', weight: 700 },
  futura:     { label: 'Futura',              family: 'Futura, "Century Gothic", "Avenir Next", sans-serif', weight: 600 },
  gillsans:   { label: 'Gill Sans',           family: '"Gill Sans", "Gill Sans MT", Calibri, sans-serif', weight: 600 },
  avenir:     { label: 'Avenir Next',         family: '"Avenir Next", Avenir, "Segoe UI", sans-serif', weight: 600 },
  trebuchet:  { label: 'Trebuchet MS',        family: '"Trebuchet MS", "Segoe UI", sans-serif', weight: 700 },
  verdana:    { label: 'Verdana',             family: 'Verdana, Geneva, sans-serif', weight: 700 },
  impact:     { label: 'Impact',              family: 'Impact, "Arial Black", sans-serif', weight: 400 },
  georgia:    { label: 'Georgia',             family: 'Georgia, "Times New Roman", serif', weight: 600 },
  times:      { label: 'Times New Roman',     family: '"Times New Roman", Times, serif', weight: 700 },
  baskerville:{ label: 'Baskerville',         family: 'Baskerville, "Libre Baskerville", Georgia, serif', weight: 600 },
  palatino:   { label: 'Palatino',            family: 'Palatino, "Palatino Linotype", "Book Antiqua", serif', weight: 600 },
  typewriter: { label: 'American Typewriter', family: '"American Typewriter", "Courier New", serif', weight: 600 },
  menlo:      { label: 'Menlo',               family: 'Menlo, Consolas, "Courier New", monospace', weight: 700 },
  courier:    { label: 'Courier New',         family: '"Courier New", Courier, monospace', weight: 700 },
};

// Starting text height as a fraction of the frame height. The fit loop below
// may shrink it, never grow it.
export const SLATE_SIZES = {
  s:  { label: 'S',  frac: 0.06 },
  m:  { label: 'M',  frac: 0.085 },
  l:  { label: 'L',  frac: 0.11 },
  xl: { label: 'XL', frac: 0.14 },
};

// Image layout: 'fit' shows the whole picture (scaled to `scale` of the frame,
// letterboxed on black — right for a logo); 'fill' covers the frame and crops.
// `anchor` is a 3×3 position key; `textPos` keeps the words clear of the logo.
export const SLATE_ANCHORS = ['tl', 't', 'tr', 'l', 'c', 'r', 'bl', 'b', 'br'];
export const DEFAULT_SLATE_STYLE = {
  font: 'helvetica', size: 'm', bgId: null,
  fit: 'fit', scale: 0.7, anchor: 'c', textPos: 'middle',
  black: 0, // seconds of black after the card, before programme
};

function anchorFractions(anchor) {
  const ax = anchor.endsWith('l') ? 0 : anchor.endsWith('r') ? 1 : 0.5;
  const ay = anchor.startsWith('t') ? 0 : anchor.startsWith('b') ? 1 : 0.5;
  return { ax, ay };
}

// `style`: { font, size, bgImage, fit, scale, anchor, textPos } — bgImage is a
// loaded HTMLImageElement or null.
export function drawSlate(canvas, text, style = {}) {
  const w = canvas.width;
  const h = canvas.height;
  const ctx = canvas.getContext('2d');
  const font = SLATE_FONTS[style.font] ?? SLATE_FONTS.helvetica;
  const sizeFrac = (SLATE_SIZES[style.size] ?? SLATE_SIZES.m).frac;

  // Background: black, then the user's image if any.
  ctx.fillStyle = '#000000';
  ctx.fillRect(0, 0, w, h);
  const img = style.bgImage;
  if (img && img.naturalWidth > 0 && img.naturalHeight > 0) {
    const fit = style.fit ?? 'fit';
    const { ax, ay } = anchorFractions(style.anchor ?? 'c');
    if (fit === 'fill') {
      const s = Math.max(w / img.naturalWidth, h / img.naturalHeight);
      const dw = img.naturalWidth * s, dh = img.naturalHeight * s;
      // Anchor chooses which part survives the crop.
      ctx.drawImage(img, (w - dw) * ax, (h - dh) * ay, dw, dh);
    } else {
      const frac = Math.min(1, Math.max(0.05, style.scale ?? 0.7));
      const s = Math.min(w / img.naturalWidth, h / img.naturalHeight) * frac;
      const dw = img.naturalWidth * s, dh = img.naturalHeight * s;
      const pad = Math.round(w * 0.04);
      ctx.drawImage(img, pad + (w - 2 * pad - dw) * ax, pad + (h - 2 * pad - dh) * ay, dw, dh);
    }
  }

  const drawn = (text || '').split('\n').map(l => l.trim()).filter(l => l.length > 0);
  if (drawn.length === 0) return;

  ctx.fillStyle = '#ffffff';
  ctx.textAlign = 'center';
  ctx.textBaseline = 'middle';
  if (img) {
    // Keep text legible over any picture.
    ctx.shadowColor = 'rgba(0, 0, 0, 0.85)';
    ctx.shadowBlur = Math.max(4, h * 0.012);
    ctx.shadowOffsetY = Math.max(1, h * 0.003);
  }

  // Start at the chosen size, shrink until the widest line fits.
  let size = Math.floor(h * sizeFrac);
  const maxWidth = w * 0.86;
  while (size > 4) {
    ctx.font = `${font.weight} ${size}px ${font.family}`;
    const widest = Math.max(...drawn.map(l => ctx.measureText(l).width));
    const total = drawn.length * size * 1.4;
    if (widest <= maxWidth && total <= h * 0.8) break;
    size -= 2;
  }
  const lineHeight = size * 1.4;
  const block = (drawn.length - 1) * lineHeight;
  // Text placement: keep the words clear of a logo when asked.
  const pos = style.textPos ?? 'middle';
  const centreY = pos === 'top' ? h * 0.10 + size / 2 + block / 2
    : pos === 'bottom' ? h * 0.90 - size / 2 - block / 2
    : h / 2;
  const startY = centreY - block / 2;
  drawn.forEach((line, i) => ctx.fillText(line, w / 2, startY + i * lineHeight));
  ctx.shadowColor = 'transparent';
}

// Render the slate at the video's exact frame size and return a JPEG data URL —
// JPEG because the bundled ffmpeg decodes mjpeg but not PNG.
export function renderSlateImage(text, width, height, style = {}) {
  const canvas = document.createElement('canvas');
  canvas.width = width || 1920;
  canvas.height = height || 1080;
  drawSlate(canvas, text, style);
  return canvas.toDataURL('image/jpeg', 0.95);
}

// Turn a data URL into a loaded <img> the renderer can draw (data: URLs never
// taint the canvas, unlike asset:// file URLs would).
export function loadImage(dataUrl) {
  return new Promise((resolve, reject) => {
    const img = new Image();
    img.onload = () => resolve(img);
    img.onerror = () => reject(new Error('Could not read that image'));
    img.src = dataUrl;
  });
}
