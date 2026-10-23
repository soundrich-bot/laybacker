// Spelling follows the date format: the American date order (MMDDYYYY) gets
// American spelling everywhere; every other format gets English. Source
// strings are written in English and passed through `sp()` where shown.
let locale = $state('uk');

export function setSpellingFromFormat(format) {
  locale = format === 'MMDDYYYY_HHmm' ? 'us' : 'uk';
}
export function spelling() { return locale; }

// English → American, case-preserving, for the words the app actually uses.
const PAIRS = [
  [/normalis(e|ed|es|ing|ation)/g, 'normaliz$1'],
  [/Normalis(e|ed|es|ing|ation)/g, 'Normaliz$1'],
  [/NORMALIS(E|ED|ES|ING|ATION)/g, 'NORMALIZ$1'],
  [/programme/g, 'program'], [/Programme/g, 'Program'],
  [/colour/g, 'color'], [/Colour/g, 'Color'],
];
export function sp(text) {
  if (locale !== 'us' || !text) return text;
  let t = text;
  for (const [re, to] of PAIRS) t = t.replace(re, to);
  return t;
}

// Re-spell every text node under `root` (used for the static help panel,
// which is too much markup to route through sp() piece by piece). Originals
// are remembered so switching back to English restores them exactly.
const originals = new WeakMap();
export function respell(root) {
  if (!root) return;
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  let n;
  while ((n = walker.nextNode())) {
    if (!originals.has(n)) originals.set(n, n.nodeValue);
    const orig = originals.get(n);
    const next = sp(orig);
    if (n.nodeValue !== next) n.nodeValue = next;
  }
}
