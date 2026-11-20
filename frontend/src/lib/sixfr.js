// 6-frame results in words. The pass line is digital silence, so a fail is
// always shown with the level that was found: −26 dB of programme and −73 dB
// of dither both fail, and the user needs to see which one they have.

export function sixFrLevel(db) {
  return typeof db === 'number' && isFinite(db) ? `${db.toFixed(1).replace('-', '−')} dB` : '';
}

/** "HEAD −73.4 dB · TAIL −26.5 dB" — only the ends that failed. */
export function sixFrFail({ headHasAudio, tailHasAudio, headPeak, tailPeak }, { caps = true } = {}) {
  const bits = [];
  if (headHasAudio) bits.push(`${caps ? 'HEAD' : 'head'} ${sixFrLevel(headPeak)}`.trim());
  if (tailHasAudio) bits.push(`${caps ? 'TAIL' : 'tail'} ${sixFrLevel(tailPeak)}`.trim());
  return bits.join(' · ');
}

export const SIXFR_RULE = 'The pass line is digital silence: every sample in the first and last 6 frames exactly zero. The level shown is the loudest sample found there.';
