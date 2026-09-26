// Multifunction Chain: one ordered set of steps applied to every audio file.
// Pure helpers live here (shape, naming, unfixable checks, CSV); the run loop
// is in the store.

export const DEFAULT_CHAIN = {
  // Shape (runs first — QC measures the file as it will be delivered)
  fold: 'none',        // none | stereo | mono
  trim: false,         // trim head/tail silence
  fade: 0,             // seconds per end, 0 = off
  split: false,        // fan out into mono stems
  // Fixes, against the QC spec (LUFS / PEAK, targets from the QC panel)
  sixFr: 'prompt',     // off | prompt | always
  loudness: 'prompt',  // off | prompt | always
  unfixable: 'prompt', // prompt | pass | skip
  // Delivery
  clock: false,
  convert: { container: 'original', sampleRate: null, bitDepth: null, aacBitrate: 320000 },
  rename: { rule: 'smart', pattern: '{name}_{date}' },
};

export const FADE_CHOICES = [
  { value: 0,    label: 'OFF' },
  { value: 0.01, label: '10 ms' },
  { value: 0.02, label: '20 ms' },
  { value: 0.05, label: '50 ms' },
  { value: 0.1,  label: '100 ms' },
  { value: 0.5,  label: '0.5 s' },
  { value: 1,    label: '1 s' },
  { value: 2,    label: '2 s' },
];

// A fix only ever runs on a file that FAILS its check; a pass is left alone.
export const FIX_MODES = [
  { value: 'off',    label: 'CHECK ONLY' },
  { value: 'prompt', label: 'PROMPT TO FIX' },
  { value: 'always', label: 'FIX WITHOUT ASKING' },
];

export const UNFIXABLE_MODES = [
  { value: 'prompt', label: 'WARN & ASK' },
  { value: 'pass',   label: 'PASS ANYWAY' },
  { value: 'skip',   label: 'SKIP THE FILE' },
];

export const RENAME_RULES = [
  { value: 'smart',  label: 'NAME + SPEC TAGS', desc: 'The original name with the tags for what the chain did, e.g. Mix_-23LUFS_6Fr_Clocked' },
  { value: 'audio',  label: 'ORIGINAL NAME',    desc: 'The original name, no tags' },
  { value: 'date',   label: 'NAME + DATE',      desc: 'The original name with today’s date and time' },
  { value: 'bump',   label: 'BUMP VERSION',     desc: 'Mix_v3 → Mix_v4 (adds _v2 when there is no version)' },
  { value: 'custom', label: 'CUSTOM PATTERN',   desc: 'Build the name from tokens' },
];

export const NAME_TOKENS = [
  { token: '{name}',  desc: 'original filename' },
  { token: '{date}',  desc: 'date & time in your chosen format' },
  { token: '{spec}',  desc: 'every tag the chain applied' },
  { token: '{lufs}',  desc: 'loudness target, e.g. -23LUFS' },
  { token: '{dbtp}',  desc: 'true-peak target, e.g. -1dBTP' },
  { token: '{rate}',  desc: 'sample rate, e.g. 48k' },
  { token: '{depth}', desc: 'bit depth, e.g. 24bit' },
  { token: '{ch}',    desc: 'mono / stereo / 5.1' },
  { token: '{n}',     desc: 'file number in the run' },
];

// Fill in anything missing from an older saved chain.
export function normaliseChain(saved) {
  const c = { ...DEFAULT_CHAIN, ...(saved || {}) };
  c.convert = { ...DEFAULT_CHAIN.convert, ...(saved?.convert || {}) };
  c.rename = { ...DEFAULT_CHAIN.rename, ...(saved?.rename || {}) };
  return c;
}

// The shaping steps, in the order the backend runs them.
export function shapeOps(chain) {
  const ops = [];
  if (chain.fold === 'stereo') ops.push({ kind: 'fold_stereo', param: null });
  if (chain.fold === 'mono') ops.push({ kind: 'fold_mono', param: null });
  if (chain.trim) ops.push({ kind: 'trim', param: -60 });
  if (chain.fade > 0) ops.push({ kind: 'fade', param: chain.fade });
  if (chain.split) ops.push({ kind: 'split', param: null });
  return ops;
}

// Does the chain do anything at all?
export function chainIsEmpty(chain) {
  return shapeOps(chain).length === 0
    && chain.sixFr === 'off' && chain.loudness === 'off'
    && !chain.clock && !convertIsSet(chain.convert)
    && chain.rename.rule === 'audio';
}

export function convertIsSet(convert) {
  return (convert.container && convert.container !== 'original') || !!convert.sampleRate || !!convert.bitDepth;
}

const CONTAINER_EXT = { wav: 'wav', aiff: 'aif', flac: 'flac', alac: 'm4a', aac: 'm4a' };
const LOSSY_EXTS = ['mp3', 'aac', 'ogg', 'oga', 'opus', 'wma', 'ac3', 'eac3'];

// Extension the chain's output gets.
export function outputExt(sourceExt, convert, willEncode) {
  const chosen = CONTAINER_EXT[convert.container];
  if (chosen) return chosen;
  if (willEncode && LOSSY_EXTS.includes((sourceExt || '').toLowerCase())) return 'wav';
  return sourceExt || 'wav';
}

// Date stamp in the app's chosen format.
export function stampFor(format, now = new Date()) {
  const pad = (n) => String(n).padStart(2, '0');
  const Y = now.getFullYear(), M = pad(now.getMonth() + 1), D = pad(now.getDate());
  const h = pad(now.getHours()), m = pad(now.getMinutes());
  switch (format) {
    case 'YYYY-MM-DD_HH-mm': return `${Y}-${M}-${D}_${h}-${m}`;
    case 'DD-MM-YYYY_HH-mm': return `${D}-${M}-${Y}_${h}-${m}`;
    case 'MMDDYYYY_HHmm':    return `${M}${D}${Y}_${h}${m}`;
    default:                 return `${Y}${M}${D}_${h}${m}`;
  }
}

// Mirror of the Rust namer's strip: take old spec / conversion tags off a
// stem so re-running a chain on its own output doesn't stack them.
export function stripSpecSuffix(stem) {
  let s = stem;
  const i = s.indexOf('_normalised_');
  if (i >= 0) s = s.slice(0, i);
  s = s.replace(/_(\d+bit|32f)$/, '');
  s = s.replace(/_\d+(\.\d+)?k$/, '');
  s = s.replace(/_Clocked$/, '');
  s = s.replace(/_6Fr$/, '');
  s = s.replace(/_-?\d+(\.\d+)?dBTP$/, '');
  s = s.replace(/_-?\d+(\.\d+)?LUFS$/, '');
  return s;
}

// The tags for what the chain applied, in processing order.
export function specTags({ normalise, sixFr, clock, spec, convert }) {
  const parts = [];
  if (normalise) parts.push(spec.mode === 'peak' ? `${spec.truePeak}dBTP` : `${spec.targetLufs}LUFS`);
  if (sixFr) parts.push('6Fr');
  if (clock) parts.push('Clocked');
  if (convert.sampleRate) parts.push(`${convert.sampleRate % 1000 === 0 ? convert.sampleRate / 1000 : convert.sampleRate / 1000}k`);
  if (convert.bitDepth) parts.push(convert.bitDepth === 32 ? '32f' : `${convert.bitDepth}bit`);
  return parts.length ? `_${parts.join('_')}` : '';
}

export function channelWord(count) {
  if (!count) return '';
  if (count === 1) return 'mono';
  if (count === 2) return 'stereo';
  if (count === 6) return '5.1';
  if (count === 8) return '7.1';
  return `${count}ch`;
}

// Bump the last version marker (same rule as the NAME menu).
export function bumpVersion(stem) {
  const re = /([_\-\s]?)([vV])(\d+)(?!.*[vV]\d)/;
  const m = stem.match(re);
  if (!m) return `${stem}_v2`;
  const next = String(parseInt(m[3], 10) + 1).padStart(m[3].length, '0');
  return stem.replace(re, `${m[1]}${m[2]}${next}`);
}

// The output stem for one file. `ctx`: { name, date, spec, lufs, dbtp, rate,
// depth, ch, n } — `spec` is the tag string from specTags().
export function renamedStem(rename, ctx) {
  const base = ctx.spec ? stripSpecSuffix(ctx.name) : ctx.name;
  switch (rename.rule) {
    case 'audio': return ctx.name;
    case 'date':  return `${base}${ctx.spec}_${ctx.date}`;
    case 'bump':  return `${bumpVersion(base)}${ctx.spec}`;
    case 'custom': {
      const out = (rename.pattern || '{name}')
        .replace(/\{name\}/g, base)
        .replace(/\{date\}/g, ctx.date)
        .replace(/\{spec\}/g, ctx.spec.replace(/^_/, ''))
        .replace(/\{lufs\}/g, ctx.lufs || '')
        .replace(/\{dbtp\}/g, ctx.dbtp || '')
        .replace(/\{rate\}/g, ctx.rate || '')
        .replace(/\{depth\}/g, ctx.depth || '')
        .replace(/\{ch\}/g, ctx.ch || '')
        .replace(/\{n\}/g, String(ctx.n ?? ''))
        .replace(/[\\/:*?"<>|]/g, '-')  // keep it a legal filename
        .replace(/_{2,}/g, '_')
        .replace(/^_+|_+$/g, '');
      return out || base;
    }
    default: return `${base}${ctx.spec}`; // smart
  }
}

// Issues the chain can't fix. `m`: { lufs, tp, head, tail, stereo, durationSecs,
// channelCount }; `spec`: { mode, targetLufs, truePeak }.
export function unfixableIssues(m, spec, chain, willNormalise) {
  const issues = [];
  const st = m.stereo;
  if (st?.verdict === 'anti_phase') {
    issues.push(`Channels are out of phase (correlation ${st.correlation?.toFixed(2)}) — it will cancel in mono`);
  }
  if (st?.verdict === 'dual_mono') {
    issues.push('Left and right are identical (dual mono) — it can’t be made stereo');
  }
  if (st?.verdict === 'one_sided') {
    issues.push(`One channel is silent or far below the other (L ${st.leftDb?.toFixed(1)} dB, R ${st.rightDb?.toFixed(1)} dB)`);
  }
  if (willNormalise && spec.mode !== 'peak' && isFinite(m.lufs) && isFinite(m.tp)) {
    const gain = spec.targetLufs - m.lufs;
    const predictedTp = m.tp + gain;
    if (predictedTp > spec.truePeak + (spec.tpTol ?? 0.05)) {
      issues.push(`At ${spec.targetLufs} LUFS the true peak would reach ${predictedTp.toFixed(1)} dBTP, over the ${spec.truePeak} dBTP ceiling — it will be held at the ceiling and land ${(predictedTp - spec.truePeak).toFixed(1)} dB under target`);
    }
  }
  if (chain.clock && m.durationSecs < 1.0) {
    issues.push(`The file is only ${m.durationSecs.toFixed(2)} s long — clock handles would be longer than the programme`);
  }
  return issues;
}

// Human summary of a chain for the panel and the report header.
export function describeChain(chain, specLabel) {
  const bits = [];
  if (chain.fold !== 'none') bits.push(`fold to ${chain.fold}`);
  if (chain.trim) bits.push('trim silence');
  if (chain.fade > 0) bits.push(`fade ${chain.fade >= 1 ? chain.fade + ' s' : chain.fade * 1000 + ' ms'}`);
  if (chain.split) bits.push('split to mono');
  bits.push('QC');
  if (chain.sixFr !== 'off') bits.push(`6 Fr (${chain.sixFr})`);
  if (chain.loudness !== 'off') bits.push(`normalise to ${specLabel} (${chain.loudness})`);
  bits.push(`unfixable: ${chain.unfixable}`);
  if (chain.clock) bits.push('clock');
  if (convertIsSet(chain.convert)) {
    const c = chain.convert;
    const conv = [c.container !== 'original' ? c.container.toUpperCase() : null,
      c.sampleRate ? `${c.sampleRate / 1000}k` : null,
      c.bitDepth ? (c.bitDepth === 32 ? '32f' : `${c.bitDepth}-bit`) : null].filter(Boolean).join(' ');
    bits.push(`convert → ${conv}`);
  }
  bits.push(`rename: ${RENAME_RULES.find(r => r.value === chain.rename.rule)?.label.toLowerCase() ?? chain.rename.rule}`);
  return bits.join(' → ');
}

function csvCell(v) {
  const s = v == null ? '' : String(v);
  return /[",\n]/.test(s) ? `"${s.replace(/"/g, '""')}"` : s;
}

// The run report as CSV.
export function csvReport(rows, header = {}) {
  const lines = [];
  if (header.title) lines.push(csvCell(header.title));
  if (header.chain) lines.push(`Chain,${csvCell(header.chain)}`);
  if (header.when) lines.push(`Run,${csvCell(header.when)}`);
  if (lines.length) lines.push('');
  lines.push(['File', 'Status', 'Output', 'LUFS before', 'dBTP before', 'Stereo', '6 Fr before',
    'QC loudness', 'QC 6 Fr', 'QC stereo', 'QC clicks', 'QC clipping', 'QC dropouts',
    'Steps applied', 'LUFS after', 'dBTP after', 'Warnings passed', 'Notes'].join(','));
  const qc = (v) => v == null ? '' : v ? 'PASS' : 'FAIL';
  for (const r of rows) {
    lines.push([
      r.file, r.status, r.output,
      fmtNum(r.before?.lufs), fmtNum(r.before?.tp), r.before?.stereo ?? '',
      r.before?.sixFr ?? '',
      qc(r.qc?.loudness), qc(r.qc?.sixFr), qc(r.qc?.stereo), qc(r.qc?.clicks), qc(r.qc?.clipping), qc(r.qc?.dropouts),
      (r.steps || []).join('; '),
      fmtNum(r.after?.lufs), fmtNum(r.after?.tp),
      (r.warnings || []).join('; '),
      r.notes ?? '',
    ].map(csvCell).join(','));
  }
  return lines.join('\n') + '\n';
}

function fmtNum(n) {
  return typeof n === 'number' && isFinite(n) ? n.toFixed(1) : '';
}
