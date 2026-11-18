// Where `maestro draw` puts what it draws, from the drawing's intent (`layout`), never from coordinates an agent
// picked: docs/features/canvas/structure-drawing.md. Pure, so scripts/draw-layout-check.ts runs it in Node.

/**
 * grid: in rows of at most three (an area's default) · flow: steps along the arrows, left → right · tree: steps
 * along the arrows, top → bottom · row: side by side, top-aligned (the top level's default). None of them runs on
 * sideways: past the width given to `arrange` they go on underneath.
 */
export type Layout = "grid" | "flow" | "tree" | "row";
export const LAYOUTS: readonly Layout[] = ["grid", "flow", "tree"];

export type Box = { w: number; h: number };
export type Arranged = { at: { x: number; y: number }[]; w: number; h: number };

/**
 * The steps of a layered layout. Arrows that close a loop (found by a depth-first walk in the given order) are
 * left out; each item goes one step past its furthest predecessor; within a step, items follow the average
 * place of their predecessors (then the given order). Every id is in exactly one step.
 */
export function layers(ids: readonly string[], edges: readonly (readonly [string, string])[]): string[][] {
  const known = new Set(ids);
  const out = new Map<string, string[]>(ids.map((i) => [i, []]));
  for (const [a, b] of edges) if (known.has(a) && known.has(b) && a !== b) out.get(a)!.push(b);

  // loops: an arrow back to an item still being walked
  const state = new Map<string, 1 | 2>();
  const back = new Set<string>();
  const walk = (v: string) => {
    state.set(v, 1);
    for (const w of out.get(v)!) {
      if (state.get(w) === 1) back.add(`${v}\u0000${w}`);
      else if (!state.has(w)) walk(w);
    }
    state.set(v, 2);
  };
  for (const i of ids) if (!state.has(i)) walk(i);

  const preds = new Map<string, string[]>(ids.map((i) => [i, []]));
  for (const [a, list] of out) for (const b of list) if (!back.has(`${a}\u0000${b}`)) preds.get(b)!.push(a);

  // longest path from the sources (the graph without loops is acyclic)
  const rank = new Map<string, number>();
  const rankOf = (v: string, depth = 0): number => {
    const r = rank.get(v);
    if (r !== undefined) return r;
    const p = preds.get(v)!;
    const mine = depth > ids.length || !p.length ? 0 : Math.max(...p.map((u) => rankOf(u, depth + 1))) + 1;
    rank.set(v, mine);
    return mine;
  };
  ids.forEach((i) => rankOf(i));

  const steps: string[][] = [];
  for (const i of ids) (steps[rank.get(i)!] ??= []).push(i);
  const place = new Map<string, number>();
  return steps.filter(Boolean).map((step) => {
    const key = (v: string) => {
      const p = preds.get(v)!.filter((u) => place.has(u));
      return p.length ? p.reduce((s, u) => s + place.get(u)!, 0) / p.length : Number.POSITIVE_INFINITY;
    };
    const order = step.map((v, n) => ({ v, n, k: key(v) })).sort((a, b) => a.k - b.k || a.n - b.n).map((x) => x.v);
    order.forEach((v, n) => place.set(v, n));
    return order;
  });
}

/**
 * Where each item goes (its top-left, relative to the space they share) and the size of that space. `gap`
 * separates items in a row or a step, `rankGap` separates the steps of flow and tree (room for arrows and labels).
 *
 * `wrap`: the width nothing runs past (structure-drawing.md "배치 규칙"); what doesn't fit goes on underneath, so a
 * drawing grows down rather than sideways. A row (and a grid's row) breaks into the next one; a flow turns down and
 * comes back the other way (its next step sits right under the last one, so the arrow between them stays short); a
 * tree's step that is too wide goes on in further rows under its first. One item wider than `wrap` still gets a
 * row of its own. Without `wrap` nothing breaks.
 */
export function arrange(ids: readonly string[], sizes: readonly Box[], edges: readonly (readonly [string, string])[], layout: Layout, gap: number, rankGap: number, wrap = Number.POSITIVE_INFINITY): Arranged {
  const at = ids.map(() => ({ x: 0, y: 0 }));
  if (!ids.length) return { at, w: 0, h: 0 };
  const index = new Map(ids.map((id, n) => [id, n]));

  if (layout === "row" || layout === "grid") {
    const per = layout === "row" ? ids.length : Math.min(3, Math.max(1, Math.ceil(Math.sqrt(ids.length))));
    let x = 0, y = 0, w = 0, tall = 0, inRow = 0;
    ids.forEach((_, n) => {
      // the next row: this one is full, or the item would pass the width (a row always takes one item at least)
      if (inRow && (inRow === per || x + sizes[n].w > wrap)) {
        y += tall + gap;
        x = tall = inRow = 0;
      }
      at[n] = { x, y };
      x += sizes[n].w + gap;
      w = Math.max(w, x - gap);
      tall = Math.max(tall, sizes[n].h);
      inRow++;
    });
    return { at, w, h: y + tall };
  }

  const steps = layers(ids, edges).map((s) => s.map((id) => index.get(id)!));
  if (layout === "flow") {
    // each step is a column: as wide as its widest item, its items one under another
    const cols = steps.map((s) => ({ s, w: Math.max(...s.map((n) => sizes[n].w)), h: s.reduce((t, n) => t + sizes[n].h, 0) + gap * (s.length - 1) }));
    // bands: columns side by side until the next one would pass the width; there the flow turns down and comes back
    const bands: (typeof cols)[] = [];
    let run = 0;
    for (const c of cols) {
      const last = bands[bands.length - 1];
      if (last && run + rankGap + c.w <= wrap) {
        last.push(c);
        run += rankGap + c.w;
      } else {
        bands.push([c]);
        run = c.w;
      }
    }
    // every other band runs right → left, and each band starts under the last column of the one above it (`edge`:
    // that column's right side for a band coming back, its left side for one going on)
    let y = 0, edge = 0, minX = 0, maxX = 0;
    bands.forEach((b, k) => {
      const tall = Math.max(...b.map((c) => c.h));
      const back = k % 2 === 1;
      let x = edge;
      b.forEach((c, j) => {
        const left = back ? x - c.w : x;
        let off = y + (tall - c.h) / 2;
        for (const n of c.s) {
          at[n] = { x: left, y: off };
          off += sizes[n].h + gap;
        }
        minX = Math.min(minX, left);
        maxX = Math.max(maxX, left + c.w);
        if (j === b.length - 1) edge = back ? left : left + c.w;
        x = back ? left - rankGap : left + c.w + rankGap;
      });
      y += tall + rankGap;
    });
    // (a band coming back may be longer than the one above it: everything moves right so nothing is left of 0)
    if (minX < 0) for (const p of at) p.x -= minX;
    return { at, w: maxX - minX, h: y - rankGap };
  }

  // tree: each step is a row, centred; a step wider than the width goes on in further rows under its first
  const rowsOf = (s: number[]) => {
    const rows: number[][] = [];
    let run = 0;
    for (const n of s) {
      const last = rows[rows.length - 1];
      if (last && run + gap + sizes[n].w <= wrap) {
        last.push(n);
        run += gap + sizes[n].w;
      } else {
        rows.push([n]);
        run = sizes[n].w;
      }
    }
    return rows;
  };
  const ranks = steps.map(rowsOf);
  const widthOf = (r: number[]) => r.reduce((t, n) => t + sizes[n].w, 0) + gap * (r.length - 1);
  const wide = Math.max(...ranks.flat().map(widthOf));
  let y = 0;
  for (const rows of ranks) {
    rows.forEach((r, j) => {
      let x = (wide - widthOf(r)) / 2;
      for (const n of r) {
        at[n] = { x, y };
        x += sizes[n].w + gap;
      }
      y += Math.max(...r.map((n) => sizes[n].h)) + (j < rows.length - 1 ? gap : rankGap);
    });
  }
  return { at, w: wide, h: y - rankGap };
}

/**
 * The width a container wraps at: `base`, or wider when what it holds is big, so that its two widest items can
 * always sit side by side (two areas joined by an arrow stay next to each other).
 */
export const wrapWidth = (base: number, sizes: readonly Box[], gap: number) => {
  const widest = [...sizes.map((s) => s.w)].sort((a, b) => b - a);
  return Math.max(base, (widest[0] ?? 0) + (widest[1] != null ? widest[1] + gap : 0));
};

export type Rect = { x: number; y: number; w: number; h: number };

/**
 * Where something new of size `box` goes on a canvas that already holds `taken` (structure-drawing.md "배치
 * 규칙"): the first free place reading the canvas like a page, from the top down and from the left, `gap` away from
 * everything there. A place that would make the canvas wider than `wrap` (counted from its left edge) is passed
 * over, so the canvas fills up and grows down instead of running on to the right; the left edge itself always
 * counts (something wider than `wrap` gets a row of its own). Places are on the `grid`. An empty canvas: 0, 0.
 * `origin`: the corner to read from, instead of the top-left corner of what is there.
 */
export function freeSpot(taken: readonly Rect[], box: Box, gap: number, wrap: number, grid = 1, origin?: { x: number; y: number }): { x: number; y: number } {
  if (!taken.length) return origin ?? { x: 0, y: 0 };
  const up = (v: number) => Math.ceil(v / grid - 1e-6) * grid;
  const left = up(origin ? origin.x : Math.min(...taken.map((r) => r.x))), top = up(origin ? origin.y : Math.min(...taken.map((r) => r.y)));
  const xs = [...new Set([left, ...taken.map((r) => up(r.x + r.w + gap))])].sort((a, b) => a - b);
  const ys = [...new Set([top, ...taken.map((r) => up(r.y + r.h + gap))])].sort((a, b) => a - b);
  const free = (x: number, y: number) => taken.every((r) => x >= r.x + r.w + gap || x + box.w + gap <= r.x || y >= r.y + r.h + gap || y + box.h + gap <= r.y);
  for (const y of ys) for (const x of xs) if ((x === left || x + box.w <= left + wrap) && free(x, y)) return { x, y };
  // (never reached: under everything, at the left edge, is always free)
  return { x: left, y: up(Math.max(...taken.map((r) => r.y + r.h)) + gap) };
}

/**
 * Things on the canvas packed again (structure-drawing.md "배치 정리"): `boxes`, in the `order` given (their
 * reading order: zones.ts `rowsOf`), each put in the first free place (`freeSpot`) among `fixed` (what stays
 * where it is) and those placed before it, reading from the top-left corner of everything. So what was drawn bit
 * by bit ends up in tidy rows with even gaps, no wider than `wrap`. Returns the new places, by index into `boxes`.
 */
export function repack(boxes: readonly Rect[], order: readonly number[], fixed: readonly Rect[], gap: number, wrap: number, grid = 1): { x: number; y: number }[] {
  const all = [...boxes, ...fixed];
  const origin = all.length ? { x: Math.min(...all.map((r) => r.x)), y: Math.min(...all.map((r) => r.y)) } : { x: 0, y: 0 };
  const taken: Rect[] = [...fixed];
  const at = boxes.map((b) => ({ x: b.x, y: b.y }));
  for (const i of order) {
    const p = freeSpot(taken, boxes[i], gap, wrap, grid, origin);
    at[i] = p;
    taken.push({ ...p, w: boxes[i].w, h: boxes[i].h });
  }
  return at;
}
