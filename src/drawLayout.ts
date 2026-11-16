// Where `maestro draw` puts what it draws, from the drawing's intent (`layout`), never from coordinates an agent
// picked: docs/features/canvas/structure-drawing.md. Pure, so scripts/draw-layout-check.ts runs it in Node.

/**
 * grid: in rows of at most three (an area's default) · flow: steps along the arrows, left → right · tree: steps
 * along the arrows, top → bottom · row: one row, top-aligned (the top level's default)
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
 */
export function arrange(ids: readonly string[], sizes: readonly Box[], edges: readonly (readonly [string, string])[], layout: Layout, gap: number, rankGap: number): Arranged {
  const at = ids.map(() => ({ x: 0, y: 0 }));
  if (!ids.length) return { at, w: 0, h: 0 };
  const index = new Map(ids.map((id, n) => [id, n]));

  if (layout === "row" || layout === "grid") {
    const per = layout === "row" ? ids.length : Math.min(3, Math.max(1, Math.ceil(Math.sqrt(ids.length))));
    let y = 0, w = 0;
    for (let r = 0; r * per < ids.length; r++) {
      const row = ids.slice(r * per, r * per + per).map((_, j) => r * per + j);
      let x = 0;
      for (const n of row) {
        at[n] = { x, y };
        x += sizes[n].w + gap;
      }
      w = Math.max(w, x - gap);
      y += Math.max(...row.map((n) => sizes[n].h)) + gap;
    }
    return { at, w, h: y - gap };
  }

  // flow: steps are columns (left → right); tree: steps are rows (top → bottom). Each step is centred across.
  const across = (b: Box) => (layout === "flow" ? b.h : b.w);
  const along = (b: Box) => (layout === "flow" ? b.w : b.h);
  const steps = layers(ids, edges).map((s) => s.map((id) => index.get(id)!));
  const span = steps.map((s) => s.reduce((t, n) => t + across(sizes[n]), 0) + gap * (s.length - 1));
  const widest = Math.max(...span);
  let pos = 0;
  steps.forEach((s, k) => {
    let off = (widest - span[k]) / 2;
    const deep = Math.max(...s.map((n) => along(sizes[n])));
    for (const n of s) {
      at[n] = layout === "flow" ? { x: pos, y: off } : { x: off, y: pos };
      off += across(sizes[n]) + gap;
    }
    pos += deep + rankGap;
  });
  const depth = pos - rankGap;
  return layout === "flow" ? { at, w: depth, h: widest } : { at, w: widest, h: depth };
}
