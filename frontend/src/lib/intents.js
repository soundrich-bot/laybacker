// The function selector: every job Laybacker does, as a key on the empty
// screen, grouped by what the job does. Choosing one is optional — it changes
// what the drop zone asks for, and sends the drop straight to the right place
// with no questions asked. Nothing is gated: dragging without choosing works
// exactly as before.

/// Layback stands alone: the eponymous job, the one the app is for.
export const HERO_INTENT = { key: 'layback', title: 'LAYBACK', desc: 'Put new audio onto a picture, one or many',
  ask: 'DROP VIDEO AND AUDIO', hint: 'One video and one audio file, or a whole batch — they are paired by duration and name' };

export const INTENT_GROUPS = [
  {
    label: 'CHECK',
    intents: [
      { key: 'qc', title: 'QC', desc: 'Loudness, peaks, phase, clicks, clipping, dropouts',
        ask: 'DROP THE AUDIO FILES', hint: 'Every check runs on every file as soon as they land' },
      { key: 'stems', title: 'STEM CHECK', desc: 'A master and its stems side by side, each playable',
        ask: 'DROP A MASTER AND ITS STEMS', hint: 'Every file on one time axis; click between them to listen' },
    ],
  },
  {
    label: 'DELIVERY',
    intents: [
      { key: 'normalise', title: 'NORMALISE', desc: 'Bring every file to a broadcast loudness spec',
        ask: 'DROP THE AUDIO FILES', hint: 'Pick the spec, then NORMALISE writes new files to it' },
      { key: 'clock', title: 'CLOCK', desc: 'Add 10 s / 5 s clock handles — silence at the head and tail — on export',
        ask: 'DROP THE AUDIO FILES', hint: 'The clock check runs first (at level, clean at both ends); the handles go on when the file is written. Works on a layback too' },
      { key: 'sixfr', title: '6 Fr', desc: 'Mute six frames at head and tail for delivery',
        ask: 'DROP THE AUDIO FILES', hint: 'Then 6 Fr ALL applies the mute to every file. Works on a layback too' },
      { key: 'convert', title: 'CONVERT', desc: 'File type, sample rate, bit depth',
        ask: 'DROP THE AUDIO FILES', hint: 'Choose the format at the bottom, then CONVERT' },
    ],
  },
  {
    label: 'PICTURE',
    intents: [
      { key: 'slate', title: 'SLATE', desc: 'A text card or overlay on the front of a picture',
        ask: 'DROP THE VIDEO TO SLATE', hint: 'A video on its own, or with its mix to slate and lay back in one go' },
      { key: 'working', title: 'WORKING FILE', desc: 'ProRes or H.264 copies for the edit',
        ask: 'DROP THE VIDEO FILES', hint: 'Pick the format at the bottom, then MAKE does the lot' },
      { key: 'extract', title: 'EXTRACT AUDIO', desc: 'Pull the soundtrack out as a WAV',
        ask: 'DROP THE VIDEO FILES', hint: 'EXTRACT AUDIO on each card writes the WAV beside the video' },
    ],
  },
  {
    label: 'UTILITY',
    intents: [
      { key: 'split', title: 'SPLIT / JOIN', desc: 'Multichannel to mono stems, or stems back together',
        ask: 'DROP THE AUDIO FILES', hint: 'A 5.1 to split, or the mono stems to join' },
      { key: 'shape', title: 'FOLD / FADE / TRIM', desc: 'Fold down, fade the ends, trim the silence',
        ask: 'DROP THE AUDIO FILES', hint: 'Each tool has its own button once the files are in' },
      { key: 'chain', title: 'CHAIN', desc: 'One ordered run of steps on every file, as a preset',
        ask: 'DROP THE AUDIO FILES', hint: 'Set the steps once, run them on everything' },
    ],
  },
];

export const INTENTS = Object.fromEntries([HERO_INTENT, ...INTENT_GROUPS.flatMap(g => g.intents)].map(i => [i.key, i]));

/** Which Audio Only section a job lives in, if it is an audio-page job. */
export const INTENT_FOCUS = {
  qc: 'qc', normalise: 'deliver', clock: 'deliver', sixfr: 'deliver', convert: 'deliver',
  split: 'process', shape: 'process', chain: 'chain', stems: 'stems',
};

export const FOCUS_TITLES = { qc: 'QC', deliver: 'FILE DELIVERABLES', process: 'FILE PROCESSING', chain: 'MULTIFUNCTION CHAIN', stems: 'STEM CHECK' };
