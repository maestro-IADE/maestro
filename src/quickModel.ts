// 빠른 이동 (Ctrl+K, docs/features/ui/quick-switch.md): which items match what was typed, best first. Pure: no React,
// no store, so scripts/quick-check.ts can test it. The palette that builds the items and draws them is QuickSwitch.tsx.

export type QuickKind = "agent" | "project" | "action";

/** One thing the palette can go to or do. `label` is what the row shows; `keywords` are also searched, below it. */
export type QuickItem = { id: string; kind: QuickKind; label: string; keywords?: readonly string[] };

/** Lower case, runs of white space as one space, no space at either end. */
export const normQuery = (s: string) => s.toLowerCase().replace(/\s+/g, " ").trim();

// what may come before a word: a space or the punctuation of names, branches and paths
const SEP = /[\s\-_/.·:#()[\]]/;

/** How `term` sits in `text` (both normalized): 4 the whole of it, 3 at its start, 2 at a word's start, 1 inside, 0 not at all. */
export function matchTier(text: string, term: string): number {
  if (!term) return 0;
  if (text === term) return 4;
  if (text.startsWith(term)) return 3;
  let best = 0;
  for (let i = text.indexOf(term, 1); i > 0; i = text.indexOf(term, i + 1)) {
    if (SEP.test(text[i - 1])) return 2;
    best = 1;
  }
  return best;
}

/**
 * How well `item` matches the query, 0 for not at all. The whole query found in the label wins (110..140, by how it
 * sits there). Otherwise every word of the query has to be found, in the label (5..8) or a keyword (1..4); the score
 * is their average, so it stays under any whole-query match.
 */
export function quickScore(item: QuickItem, q: string): number {
  const query = normQuery(q);
  if (!query) return 0;
  const label = normQuery(item.label);
  const phrase = matchTier(label, query);
  if (phrase) return 100 + phrase * 10;
  const keys = (item.keywords ?? []).map(normQuery);
  const words = query.split(" ");
  let total = 0;
  for (const w of words) {
    const inLabel = matchTier(label, w);
    const s = inLabel ? inLabel + 4 : Math.max(0, ...keys.map((k) => matchTier(k, w)));
    if (!s) return 0;
    total += s;
  }
  return total / words.length;
}

/**
 * The items that match `q`, best first: a match at the start beats one at a word's start, which beats one inside a
 * word; equal scores keep the items' own order (agents in sidebar order, then projects, then actions). An empty
 * query keeps every item in that order. At most `limit`.
 */
export function rankQuick<T extends QuickItem>(items: readonly T[], q: string, limit = Infinity): T[] {
  if (!normQuery(q)) return items.slice(0, limit);
  return items
    .map((it, i) => ({ it, i, s: quickScore(it, q) }))
    .filter((x) => x.s > 0)
    .sort((a, b) => b.s - a.s || a.i - b.i)
    .slice(0, limit)
    .map((x) => x.it);
}

/** The highlighted row after ↑ (-1) or ↓ (1) among `n`: round from either end; -1 when there are none. */
export function moveHot(i: number, n: number, dir: -1 | 1): number {
  if (n <= 0) return -1;
  if (i < 0 || i >= n) return dir > 0 ? 0 : n - 1;
  return (i + dir + n) % n;
}
