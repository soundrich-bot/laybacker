// Stem check: spotting stems by name, and guessing which file is the mix.
//
// Stems are labelled stems; mixes can be called anything. So the rule is:
// if any dropped audio file gives itself away as a stem, or as one part of a
// soundtrack (dialogue, music, effects…), offer a stem check. The mix is the
// file that doesn't look like a stem — or whichever one the user picks.

const STEM_WORDS = new Set([
  'stem', 'stems',
  'dx', 'dial', 'dialog', 'dialogue', 'dia', 'vo', 'voice', 'narr', 'narration',
  'mx', 'music', 'mus', 'score', 'song',
  'fx', 'sfx', 'efx', 'effects', 'pfx', 'spot', 'spots',
  'atmos', 'atm', 'amb', 'ambience', 'ambiance', 'bg', 'bgs', 'foley', 'prod',
  'mne', 'm&e',
]);

const MIX_WORDS = new Set([
  'mix', 'fullmix', 'mixdown', 'final', 'master', 'printmaster', 'pm', 'comp', 'composite', 'ltrt', 'lort', 'stereo_mix',
]);

function tokens(file) {
  const name = (file.filenameNoExt ?? file.filename ?? '').toLowerCase().replace(/m&e/g, 'mne');
  return name.split(/[^a-z0-9]+/).filter(Boolean);
}

/** The telltale word that marks this file as a stem, or null. */
export function stemWord(file) {
  return tokens(file).find(t => STEM_WORDS.has(t)) ?? null;
}

/** The telltale word that marks this file as a mix, or null. */
export function mixWord(file) {
  return tokens(file).find(t => MIX_WORDS.has(t)) ?? null;
}

/**
 * Should we offer a stem check for these files? Returns the guess —
 * `{ stems, mix }` with `mix` possibly null when it isn't obvious — or null
 * when nothing looks like a stem (or there's only one file).
 */
export function stemCandidate(files) {
  const audios = files.filter(f => f.mediaType === 'audio');
  if (audios.length < 2) return null;
  const stems = audios.filter(f => stemWord(f) && !mixWord(f));
  if (stems.length === 0) return null;
  const named = audios.filter(f => mixWord(f));
  const others = audios.filter(f => !stems.includes(f));
  const mix = named.length === 1 ? named[0] : others.length === 1 ? others[0] : null;
  return { stems, mix };
}
