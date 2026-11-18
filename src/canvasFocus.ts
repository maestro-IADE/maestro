// Where the canvas looks after its agent changed it (docs/features/canvas/canvas-controls.md "화면이 따라가기"): at
// what is new, then at the whole structure the new things belong to. Pure (no React, no canvas imports), so
// scripts/focus-check.ts runs it in Node.
import type { ZNode } from "./zones";

export type FEdge = { id: string; source: string; target: string; data?: Record<string, unknown> };
export type Sheet<N extends ZNode = ZNode> = { nodes: readonly N[]; edges: readonly FEdge[] };
export type Rect = { x: number; y: number; w: number; h: number };
export type View = { x: number; y: number; zoom: number };
/** A node's size on the canvas (the canvas knows what a block that isn't measured yet will be about). */
export type SizeOf<N extends ZNode = ZNode> = (n: N) => { w: number; h: number };

/**
 * What a change brought: `fresh`, the nodes that weren't there before; `touched`, the nodes that were and whose
 * contents changed (a block's notes, an area's step), with the ends of lines that are new or changed.
 */
export function changed(before: Sheet, after: Sheet): { fresh: string[]; touched: string[] } {
  const was = new Map(before.nodes.map((n) => [n.id, n]));
  const fresh = after.nodes.filter((n) => !was.has(n.id)).map((n) => n.id);
  const touched = new Set(after.nodes.filter((n) => was.has(n.id) && was.get(n.id)!.data !== n.data).map((n) => n.id));
  const lines = new Map(before.edges.map((e) => [e.id, e]));
  for (const e of after.edges) {
    if (lines.get(e.id)?.data === e.data && lines.has(e.id)) continue;
    for (const end of [e.source, e.target]) if (was.has(end)) touched.add(end);
  }
  return { fresh, touched: [...touched] };
}

/** The outermost area a node is in (the node itself at the top level). */
const topOf = (byId: ReadonlyMap<string, ZNode>, id: string) => {
  let n = byId.get(id);
  for (let k = 0; n?.parentId && byId.has(n.parentId) && k < 50; k++) n = byId.get(n.parentId);
  return n?.id ?? id;
};

/**
 * The structure some nodes belong to, as the top-level nodes it is made of: the outermost areas they are in, and
 * everything joined to those by arrows, however far (an arrow between blocks inside two areas joins the areas).
 * Lines that aren't arrows (an agent's references, a prompt's pipe) don't make a structure.
 */
export function structure(sheet: Sheet, ids: readonly string[]): Set<string> {
  const byId = new Map(sheet.nodes.map((n) => [n.id, n]));
  const tops = new Set(ids.filter((id) => byId.has(id)).map((id) => topOf(byId, id)));
  const arrows = sheet.edges.filter((e) => e.data?.kind === "arrow").map((e) => [topOf(byId, e.source), topOf(byId, e.target)] as const);
  for (let grew = true, k = 0; grew && k < 1000; k++) {
    grew = false;
    for (const [a, b] of arrows) {
      if (tops.has(a) === tops.has(b)) continue;
      tops.add(a);
      tops.add(b);
      grew = true;
    }
  }
  return tops;
}

/** The box around some nodes on the canvas (their places summed up through the areas they are in); null for none. */
export function boundsOf<N extends ZNode>(nodes: readonly N[], ids: Iterable<string>, size: SizeOf<N>): Rect | null {
  const byId = new Map(nodes.map((n) => [n.id, n]));
  let x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity;
  for (const id of ids) {
    const n = byId.get(id);
    if (!n) continue;
    let x = 0, y = 0;
    for (let c: N | undefined = n, k = 0; c && k < 50; c = c.parentId ? byId.get(c.parentId) : undefined, k++) {
      x += c.position.x;
      y += c.position.y;
    }
    const s = size(n);
    x0 = Math.min(x0, x);
    y0 = Math.min(y0, y);
    x1 = Math.max(x1, x + s.w);
    y1 = Math.max(y1, y + s.h);
  }
  return x0 === Infinity ? null : { x: x0, y: y0, w: x1 - x0, h: y1 - y0 };
}

/**
 * The view that shows a box whole, in the middle of a pane: as large as fits with `padding` (a share of the pane on
 * each side) kept free, between `minZoom` and `maxZoom` (so one small block isn't blown up, and the canvas isn't
 * shrunk past what the canvas allows).
 */
export function viewFor(r: Rect, pane: { w: number; h: number }, o: { padding: number; minZoom: number; maxZoom: number }): View {
  const room = 1 - 2 * o.padding;
  const zoom = Math.min(o.maxZoom, Math.max(o.minZoom, Math.min((pane.w * room) / Math.max(1, r.w), (pane.h * room) / Math.max(1, r.h))));
  return { x: pane.w / 2 - (r.x + r.w / 2) * zoom, y: pane.h / 2 - (r.y + r.h / 2) * zoom, zoom };
}

/** Whether a box is whole in view. */
export function shows(v: View, pane: { w: number; h: number }, r: Rect): boolean {
  const x0 = -v.x / v.zoom, y0 = -v.y / v.zoom;
  return r.x >= x0 && r.y >= y0 && r.x + r.w <= x0 + pane.w / v.zoom && r.y + r.h <= y0 + pane.h / v.zoom;
}

/**
 * Whether the view should move to show a box: not when the box is whole in view already at about the size the move
 * would give it (three quarters of that zoom or more). So nothing moves for what the user is looking at anyway, and
 * a view far out still comes in on something new.
 */
export const worthMoving = (now: View, to: View, pane: { w: number; h: number }, r: Rect) => !(shows(now, pane, r) && now.zoom >= to.zoom * 0.75);

/** How much of a box is in view: the area they share, as a share of the smaller of the two (1: whole in view, or filling it). */
export function inView(v: View, pane: { w: number; h: number }, r: Rect): number {
  const x0 = -v.x / v.zoom, y0 = -v.y / v.zoom, w = pane.w / v.zoom, h = pane.h / v.zoom;
  const across = Math.min(r.x + r.w, x0 + w) - Math.max(r.x, x0), down = Math.min(r.y + r.h, y0 + h) - Math.max(r.y, y0);
  if (across <= 0 || down <= 0) return 0;
  return (across * down) / Math.max(1, Math.min(r.w * r.h, w * h));
}

/** Which way a box lies from the middle of the view: degrees clockwise from "to the right" (90 = below, 180 = to the left). */
export function toward(v: View, pane: { w: number; h: number }, r: Rect): number {
  const cx = (pane.w / 2 - v.x) / v.zoom, cy = (pane.h / 2 - v.y) / v.zoom;
  return (Math.round((Math.atan2(r.y + r.h / 2 - cy, r.x + r.w / 2 - cx) * 180) / Math.PI) + 360) % 360;
}

/**
 * What to look at for some nodes (the ones a step brought, changed or names): `first`, the box around them;
 * `then`, the box around the whole structure they belong to, when that is more than `first`. `marks`: the nodes
 * to point out (the outermost of them: a new area, not each block in it). What doesn't show (`hidden`: in a folded
 * area, or not in the step its area shows; zones.ts `foldedInto`, `hiddenByPhase`) is looked at through the nearest
 * thing around it that does. `lead`: what is looked at before the rest when it is among them (a question that
 * waits for an answer: the agent stands still until it has one); the rest then comes with `then`. Null for none.
 */
export function planFor<N extends ZNode>(sheet: Sheet<N>, ids: readonly string[], size: SizeOf<N>, hidden: ReadonlySet<string> = new Set(), lead?: (n: N) => boolean): { first: Rect; then: Rect | null; marks: string[] } | null {
  const byId = new Map(sheet.nodes.map((n) => [n.id, n]));
  const seen = (id: string) => {
    let n = byId.get(id);
    for (let k = 0; n && hidden.has(n.id) && n.parentId && k < 50; k++) n = byId.get(n.parentId);
    return n?.id ?? id;
  };
  const shown = [...new Set(ids.filter((id) => byId.has(id)).map(seen))];
  const all = boundsOf(sheet.nodes, shown, size);
  if (!all) return null;
  const leading = lead ? shown.filter((id) => lead(byId.get(id)!)) : [];
  const first = (leading.length && leading.length < shown.length ? boundsOf(sheet.nodes, leading, size) : null) ?? all;
  const whole = boundsOf(sheet.nodes, [...structure(sheet, shown), ...shown], size);
  const more = !!whole && (whole.w > first.w + 1 || whole.h > first.h + 1);
  const inOther = (id: string) => {
    for (let p = byId.get(id)?.parentId, k = 0; p && k < 50; p = byId.get(p)?.parentId, k++) if (shown.includes(p)) return true;
    return false;
  };
  return { first, then: more ? whole : null, marks: shown.filter((id) => !inOther(id)) };
}

/**
 * What to look at after a change: what is new, or, when nothing is new, what changed (`planFor`); `fresh` says
 * which. Null when the change brought nothing to look at (something erased).
 */
export function focusPlan<N extends ZNode>(before: Sheet<N>, after: Sheet<N>, size: SizeOf<N>, hidden: ReadonlySet<string> = new Set()): { first: Rect; then: Rect | null; marks: string[]; fresh: boolean } | null {
  const { fresh, touched } = changed(before, after);
  const plan = planFor(after, fresh.length ? fresh : touched, size, hidden);
  return plan && { ...plan, fresh: fresh.length > 0 };
}
