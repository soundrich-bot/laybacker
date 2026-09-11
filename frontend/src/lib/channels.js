// Channel helpers for the Audio Only page (frontend side of services/channels.rs).

// Mono stems are matched into these sets by filename suffix ("Mix_L.wav",
// "Mix_R.wav" → stereo). Larger sets are tried first so a full 5.1 isn't
// mistaken for a stereo pair plus strays.
export const CHANNEL_SETS = [
  { layout: '7.1', names: ['L', 'R', 'C', 'LFE', 'Lsr', 'Rsr', 'Lss', 'Rss'] },
  { layout: '5.1', names: ['L', 'R', 'C', 'LFE', 'Ls', 'Rs'] },
  { layout: 'quad', names: ['L', 'R', 'Ls', 'Rs'] },
  { layout: 'stereo', names: ['L', 'R'] },
];

const ALL_SUFFIXES = new Set(CHANNEL_SETS.flatMap(s => s.names));

// Split "Mix_Ls" into { stem: "Mix", name: "Ls" } when the suffix is a known
// channel name (case-sensitive: "Ls" is a channel, "ls" is not).
export function parseStemSuffix(filenameNoExt) {
  const i = filenameNoExt.lastIndexOf('_');
  if (i <= 0) return null;
  const name = filenameNoExt.slice(i + 1);
  if (!ALL_SUFFIXES.has(name)) return null;
  return { stem: filenameNoExt.slice(0, i), name };
}

// Group mono files into joinable sets. `files`: MediaFile-like objects with
// path, filenameNoExt, channelCount. Returns
// [{ stem, layout, names, inputs: [paths in channel order], output }].
export function findJoinGroups(files) {
  const byStem = new Map();
  for (const f of files) {
    if ((f.channelCount ?? 1) !== 1) continue;
    const parsed = parseStemSuffix(f.filenameNoExt);
    if (!parsed) continue;
    const dir = f.path.slice(0, f.path.lastIndexOf('/'));
    const key = `${dir}/${parsed.stem}`;
    if (!byStem.has(key)) byStem.set(key, { stem: parsed.stem, dir, members: new Map() });
    byStem.get(key).members.set(parsed.name, f.path);
  }
  const groups = [];
  for (const g of byStem.values()) {
    const set = CHANNEL_SETS.find(s => s.names.every(n => g.members.has(n)));
    if (!set) continue;
    groups.push({
      stem: g.stem,
      layout: set.layout,
      names: set.names,
      inputs: set.names.map(n => g.members.get(n)),
      output: `${g.dir}/${g.stem}_${set.layout}.wav`,
    });
  }
  return groups;
}
