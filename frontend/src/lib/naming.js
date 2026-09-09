// Output-name helpers shared by the store, the per-row NAME FROM menu and the
// batch NAME ALL BY rule.

export function splitName(filename) {
  const dot = filename.lastIndexOf('.');
  return dot > 0
    ? { stem: filename.slice(0, dot), ext: filename.slice(dot + 1) }
    : { stem: filename, ext: '' };
}

// Bump the LAST version marker in a stem — "Mix_v3" → "Mix_v4", "Cut_V03" →
// "Cut_V04" (zero padding kept), "Spot-v12" → "Spot-v13". No marker: append _v2.
export function bumpVersion(stem) {
  const re = /([_\-\s]?)([vV])(\d+)(?!.*[vV]\d)/;
  const m = stem.match(re);
  if (!m) return `${stem}_v2`;
  const digits = m[3];
  const next = String(parseInt(digits, 10) + 1).padStart(digits.length, '0');
  return stem.replace(re, `${m[1]}${m[2]}${next}`);
}

// Resolve a naming rule for a pair. Returns the new filename, or null when the
// rule doesn't apply (e.g. "video" on an audio-only pair) or means "let the
// smart namer decide" ('smart').
export function nameForRule(pair, rule, ext) {
  switch (rule) {
    case 'audio':
      return `${pair.audio.filenameNoExt}.${ext}`;
    case 'video':
      return pair.video ? `${pair.video.filenameNoExt}.${ext}` : null;
    case 'bump': {
      const { stem } = splitName(pair.outputFilename || '');
      return stem ? `${bumpVersion(stem)}.${ext}` : null;
    }
    default:
      return null;
  }
}
