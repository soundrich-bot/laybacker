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
  // 'prepend' puts a card in front of the picture; 'overlay' lays the text
  // over the first seconds of the picture (runtime and sound unchanged).
  mode: 'prepend',
  // Lines the user has dragged: { [lineIndex]: { x, y } } as fractions of the
  // frame (the line's centre). A line with no entry sits in the auto layout.
  linePos: {},
};

function anchorFractions(anchor) {
  const ax = anchor.endsWith('l') ? 0 : anchor.endsWith('r') ? 1 : 0.5;
  const ay = anchor.startsWith('t') ? 0 : anchor.startsWith('b') ? 1 : 0.5;
  return { ax, ay };
}

// The non-empty lines of slate text, in order — the index is what `linePos` keys on.
export function slateLines(text) {
  return (text || '').split('\n').map(l => l.trim()).filter(l => l.length > 0);
}

// Work out where every line goes on a w×h frame: font size (shrunk to fit),
// and each line's centre — the dragged position if it has one, else the auto
// layout (a centred block at top / middle / bottom). Shared by drawing and by
// the editor's hit-testing, so what you grab is what is drawn.
export function layoutSlate(ctx, w, h, text, style = {}) {
  const font = SLATE_FONTS[style.font] ?? SLATE_FONTS.helvetica;
  const sizeFrac = (SLATE_SIZES[style.size] ?? SLATE_SIZES.m).frac;
  const lines = slateLines(text);
  if (lines.length === 0) return { lines: [], size: 0, font };

  // Start at the chosen size, shrink until the widest line fits.
  let size = Math.floor(h * sizeFrac);
  const maxWidth = w * 0.86;
  while (size > 4) {
    ctx.font = `${font.weight} ${size}px ${font.family}`;
    const widest = Math.max(...lines.map(l => ctx.measureText(l).width));
    const total = lines.length * size * 1.4;
    if (widest <= maxWidth && total <= h * 0.8) break;
    size -= 2;
  }
  ctx.font = `${font.weight} ${size}px ${font.family}`;
  const lineHeight = size * 1.4;
  const block = (lines.length - 1) * lineHeight;
  const pos = style.textPos ?? 'middle';
  const centreY = pos === 'top' ? h * 0.10 + size / 2 + block / 2
    : pos === 'bottom' ? h * 0.90 - size / 2 - block / 2
    : h / 2;
  const startY = centreY - block / 2;
  const linePos = style.linePos ?? {};
  return {
    size, font,
    lines: lines.map((t, i) => {
      const custom = linePos[i];
      const x = custom ? custom.x * w : w / 2;
      const y = custom ? custom.y * h : startY + i * lineHeight;
      return { text: t, index: i, x, y, width: ctx.measureText(t).width, height: size, moved: !!custom };
    }),
  };
}

// `style`: { font, size, bgImage, fit, scale, anchor, textPos, linePos } —
// bgImage is a loaded HTMLImageElement or null.
// `opts.layer`:
//   'full'  (default) the prepended card: black / image background + text
//   'text'  overlay colour layer: white text on black
//   'matte' overlay alpha: white text (plus a soft halo that becomes a dark
//           edge over the picture, for legibility) on black
// `opts.backdrop`: an image drawn behind everything — the editor's preview of
// the video's own first frame in overlay mode. Never used for an export.
// `opts.highlight`: a line index to outline (the one being dragged / hovered).
export function drawSlate(canvas, text, style = {}, opts = {}) {
  const w = canvas.width;
  const h = canvas.height;
  const ctx = canvas.getContext('2d');
  const layer = opts.layer ?? 'full';

  ctx.shadowColor = 'transparent';
  ctx.fillStyle = '#000000';
  ctx.fillRect(0, 0, w, h);

  const backdrop = opts.backdrop;
  if (backdrop && backdrop.naturalWidth > 0) {
    ctx.drawImage(backdrop, 0, 0, w, h);
  }

  // The card's own image — prepend mode only.
  const img = layer === 'full' && !backdrop ? style.bgImage : null;
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

  const layout = layoutSlate(ctx, w, h, text, style);
  if (layout.lines.length === 0) return layout;

  ctx.textAlign = 'center';
  ctx.textBaseline = 'middle';
  ctx.font = `${layout.font.weight} ${layout.size}px ${layout.font.family}`;

  const overPicture = !!img || !!backdrop;
  if (layer === 'matte') {
    // Halo first: a blurred, part-opaque spread around the glyphs. The colour
    // layer is black there, so over the picture it reads as a dark edge.
    ctx.fillStyle = 'rgba(255, 255, 255, 0.55)';
    ctx.shadowColor = 'rgba(255, 255, 255, 0.55)';
    ctx.shadowBlur = Math.max(4, h * 0.014);
    layout.lines.forEach(l => ctx.fillText(l.text, l.x, l.y));
    ctx.shadowColor = 'transparent';
    ctx.shadowBlur = 0;
  } else if (overPicture) {
    // Keep text legible over any picture.
    ctx.shadowColor = 'rgba(0, 0, 0, 0.85)';
    ctx.shadowBlur = Math.max(4, h * 0.012);
    ctx.shadowOffsetY = layer === 'full' && !backdrop ? Math.max(1, h * 0.003) : 0;
  }
  ctx.fillStyle = '#ffffff';
  layout.lines.forEach(l => ctx.fillText(l.text, l.x, l.y));
  ctx.shadowColor = 'transparent';
  ctx.shadowBlur = 0;
  ctx.shadowOffsetY = 0;

  // Editor affordance: outline the grabbed / hovered line.
  if (opts.highlight != null) {
    const l = layout.lines[opts.highlight];
    if (l) {
      const padX = l.height * 0.35, padY = l.height * 0.2;
      ctx.strokeStyle = '#08f7fe';
      ctx.lineWidth = Math.max(1, h * 0.004);
      ctx.setLineDash([h * 0.02, h * 0.012]);
      ctx.strokeRect(l.x - l.width / 2 - padX, l.y - l.height / 2 - padY, l.width + 2 * padX, l.height + 2 * padY);
      ctx.setLineDash([]);
    }
  }
  return layout;
}

// Which line (index) is under a point in canvas pixels, or null.
export function hitSlateLine(layout, px, py) {
  if (!layout?.lines) return null;
  // Last drawn wins, so a line dragged on top of another is the one you grab.
  for (let i = layout.lines.length - 1; i >= 0; i--) {
    const l = layout.lines[i];
    const padX = l.height * 0.35, padY = l.height * 0.25;
    if (Math.abs(px - l.x) <= l.width / 2 + padX && Math.abs(py - l.y) <= l.height / 2 + padY) return l.index;
  }
  return null;
}

function renderLayer(text, width, height, style, layer) {
  const canvas = document.createElement('canvas');
  canvas.width = width || 1920;
  canvas.height = height || 1080;
  drawSlate(canvas, text, style, { layer });
  return canvas.toDataURL('image/jpeg', 0.95);
}

// Render the slate at the video's exact frame size and return a JPEG data URL —
// JPEG because the bundled ffmpeg decodes mjpeg but not PNG.
export function renderSlateImage(text, width, height, style = {}) {
  return renderLayer(text, width, height, style, style.mode === 'overlay' ? 'text' : 'full');
}

// Overlay mode's second image: the text's greyscale matte (JPEG has no alpha,
// so ffmpeg merges this in as the alpha channel). null in prepend mode.
export function renderSlateMatte(text, width, height, style = {}) {
  if (style.mode !== 'overlay') return null;
  return renderLayer(text, width, height, style, 'matte');
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
