// Areas (zones) on the canvas: how big one has to be for what it holds, fitting one to its contents, folding one to
// a chip, and what a folded one hides (docs/features/canvas/zones.md). Pure (no React, no canvas imports), so
// scripts/zones-check.ts runs it in Node.

/** The part of a canvas node these rules read and write (CanvasNode fits it). */
export type ZNode = {
  id: string;
  type?: string;
  parentId?: string;
  position: { x: number; y: number };
  width?: number;
  height?: number;
  measured?: { width?: number; height?: number };
  selected?: boolean;
  data: Record<string, unknown>;
};
export type Size = { width: number; height: number };
/** What a zone keeps for folding: `folded`, and `full`, the size it opens back to. */
export type FoldData = { folded?: boolean; full?: Size };

/**
 * The numbers the rules work with (the canvas gives them, built on its grid): `pad` kept around the contents on the
 * left, right and bottom, `top` above them (the area's name sits there), `min` the smallest open area, `chip` a
 * folded one, `fallback` the size of a block that has none yet (not measured).
 */
export type Metrics = { grid: number; pad: number; top: number; min: Size; chip: Size; fallback: Size };

const isZone = (n: ZNode) => n.type === "zone";
export const isFolded = (n: ZNode) => isZone(n) && !!(n.data as FoldData).folded;

/** A node's size as the canvas lays it out: a folded area counts as its chip, an open one as it is drawn. */
export const sizeOf = (n: ZNode, m: Metrics): Size => ({
  width: n.width ?? n.measured?.width ?? m.fallback.width,
  height: n.height ?? n.measured?.height ?? m.fallback.height,
});
/** An area's size when open: a folded one keeps it in `data.full`. */
export const openSize = (n: ZNode, m: Metrics): Size => (isFolded(n) ? ((n.data as FoldData).full ?? m.min) : sizeOf(n, m));
/** The area with a new open size: on the node, or in `data.full` while it is folded. */
export const withOpenSize = <N extends ZNode>(n: N, size: Size): N => (isFolded(n) ? { ...n, data: { ...n.data, full: size } } : { ...n, ...size });

const up = (v: number, grid: number) => Math.ceil(v / grid - 1e-6) * grid;
const down = (v: number, grid: number) => Math.floor(v / grid + 1e-6) * grid;

/** How deep a node sits (0: top level), for working from the innermost area out. */
function depths(ns: readonly ZNode[]) {
  const byId = new Map(ns.map((n) => [n.id, n]));
  const depth = (n: ZNode) => {
    let d = 0;
    for (let p = n.parentId; p && d < 50; p = byId.get(p)?.parentId) d++;
    return d;
  };
  return depth;
}

/**
 * The box around what an area holds, in the area's own coordinates; null when it holds nothing that shows. In an
 * area with steps or states only what exists in the one showing counts: the area is as big as what is seen in it,
 * and is fitted again when it goes to another step (`settleZones`).
 */
export function contentBox(ns: readonly ZNode[], id: string, m: Metrics) {
  const z = ns.find((n) => n.id === id);
  let box: { x0: number; y0: number; x1: number; y1: number } | null = null;
  for (const c of ns) {
    if (c.parentId !== id || !showsIn(z, (c.data as WhenData).when)) continue;
    const s = sizeOf(c, m);
    const b = { x0: c.position.x, y0: c.position.y, x1: c.position.x + s.width, y1: c.position.y + s.height };
    box = box ? { x0: Math.min(box.x0, b.x0), y0: Math.min(box.y0, b.y0), x1: Math.max(box.x1, b.x1), y1: Math.max(box.y1, b.y1) } : b;
  }
  return box;
}

/**
 * Every open area made big enough for what it holds (an area never cuts its contents): where something sticks out,
 * that side moves out to it plus the padding. Nothing inside moves on screen: when the left or top side moves, the
 * area's position goes with it and its contents' own positions make up for it. Areas only grow here (shrinking is
 * `fitZone`, on request). Innermost first, so an area that grew can push the one around it. A folded area is left
 * alone until it opens (`foldZone`). Returns the same array when nothing changed.
 */
export function growZones<N extends ZNode>(ns: readonly N[], m: Metrics): N[] {
  const depth = depths(ns);
  const zones = ns.filter((n) => isZone(n) && !isFolded(n)).sort((a, b) => depth(b) - depth(a));
  let next = ns as N[];
  for (const z0 of zones) {
    const z = next.find((n) => n.id === z0.id)!;
    const box = contentBox(next, z.id, m);
    if (!box) continue;
    const s = sizeOf(z, m);
    // how far the left and top sides move out (whole cells), then the right and bottom ones
    const dx = box.x0 < 0 ? up(m.pad - box.x0, m.grid) : 0;
    const dy = box.y0 < 0 ? up(m.top - box.y0, m.grid) : 0;
    const width = box.x1 + dx > s.width + dx ? up(box.x1 + dx + m.pad, m.grid) : s.width + dx;
    const height = box.y1 + dy > s.height + dy ? up(box.y1 + dy + m.pad, m.grid) : s.height + dy;
    if (!dx && !dy && width === s.width && height === s.height) continue;
    next = next.map((n) => {
      if (n.id === z.id) return { ...n, position: { x: n.position.x - dx, y: n.position.y - dy }, width, height };
      if (n.parentId === z.id && (dx || dy)) return { ...n, position: { x: n.position.x + dx, y: n.position.y + dy } };
      return n;
    });
  }
  return next;
}

/**
 * 내용에 맞추기: the area drawn tight around what it holds (padding on the sides and bottom, room for its name on
 * top), never under the smallest area; nothing inside moves on screen. An area that holds nothing goes back to
 * `empty`. Then the areas around it are checked as in `growZones`.
 */
export function fitZone<N extends ZNode>(ns: readonly N[], id: string, m: Metrics, empty: Size): N[] {
  const z = ns.find((n) => n.id === id);
  if (!z || !isZone(z) || isFolded(z)) return ns as N[];
  const box = contentBox(ns, id, m);
  if (!box) return growZones(ns.map((n) => (n.id === id ? { ...n, ...empty } : n)), m);
  // the new left/top side, in the area's old coordinates (whole cells, so everything stays on the grid)
  const x = down(box.x0 - m.pad, m.grid), y = down(box.y0 - m.top, m.grid);
  const width = Math.max(m.min.width, up(box.x1 + m.pad - x, m.grid));
  const height = Math.max(m.min.height, up(box.y1 + m.pad - y, m.grid));
  // (it fits already: the same array comes back)
  const s = sizeOf(z, m);
  if (!x && !y && width === s.width && height === s.height) return growZones(ns, m);
  return growZones(
    ns.map((n) => {
      if (n.id === id) return { ...n, position: { x: n.position.x + x, y: n.position.y + y }, width, height };
      if (n.parentId === id) return { ...n, position: { x: n.position.x - x, y: n.position.y - y } };
      return n;
    }),
    m,
  );
}

/**
 * What an area shows right now, as a string that changes when that changes: which nodes, and their boxes relative
 * to each other. Moving the area, or pulling at its own sides, doesn't change it (nor does fitting the area). Null
 * while one of them has no size yet (not measured); "" when it shows nothing.
 */
export function heldKey(ns: readonly ZNode[], id: string, m: Metrics): string | null {
  const z = ns.find((n) => n.id === id);
  const kids = ns.filter((c) => c.parentId === id && showsIn(z, (c.data as WhenData).when));
  if (!kids.every(sized)) return null;
  const box = contentBox(ns, id, m);
  if (!box) return "";
  return kids
    .map((c) => {
      const s = sizeOf(c, m);
      return `${c.id}:${c.position.x - box.x0},${c.position.y - box.y0},${s.width},${s.height}`;
    })
    .sort()
    .join("|");
}

/**
 * What an area was last seen holding (`settleZones`): `shape`, its `heldKey`; `x`/`y`, where the box around its
 * contents started on the canvas (0, 0 when it showed nothing); `zx`/`zy`, where the area itself was.
 */
export type Held = { shape: string; x: number; y: number; zx: number; zy: number };
const heldOf = (ns: readonly ZNode[], z: ZNode, m: Metrics): Held | null => {
  const shape = heldKey(ns, z.id, m);
  if (shape === null) return null;
  const o = absBox(new Map(ns.map((n) => [n.id, n])), z, m), box = contentBox(ns, z.id, m);
  return { shape, x: box ? o.x + box.x0 : 0, y: box ? o.y + box.y0 : 0, zx: o.x, zy: o.y };
};
/**
 * Whether what an area holds changed: other nodes or other boxes among them, or all of it moved on the canvas
 * without the area having moved the same way. The area dragged along with what it holds is no change, and neither
 * is a side of the area pulled by hand (what it holds stays where it is on the canvas).
 */
const heldChanged = (now: Held, was: Held) => {
  const off = (a: number, b: number) => Math.abs(a - b) > 0.5;
  return now.shape !== was.shape || ((off(now.x, was.x) || off(now.y, was.y)) && (off(now.x - was.x, now.zx - was.zx) || off(now.y - was.y, now.zy - was.zy)));
};

/**
 * The areas brought in line with what they hold, once things have come to rest (zones.md "크기"). An area whose
 * contents changed since it was last settled is fitted to them again, as 내용에 맞추기 does: something came in or
 * went out, moved or changed size inside it, or the area went to another step (`seen`: what each area held then).
 * An area whose contents are as they were keeps the size it has (one stretched by hand stays stretched until
 * something in it changes) and only grows if it must (`growZones`). An area seen for the first time is only
 * remembered, and so is one whose contents aren't all measured yet. A stack lays itself out (`layStacks`); a
 * folded or hidden area waits until it shows; an area showing nothing keeps its size. Innermost first: an area
 * fitted to its contents changes what the area around it holds. Returns the nodes (the same array when nothing
 * changed) and what to pass as `seen` next time.
 */
export function settleZones<N extends ZNode>(ns: readonly N[], m: Metrics, seen: ReadonlyMap<string, Held>): { nodes: N[]; seen: Map<string, Held> } {
  const depth = depths(ns);
  const hidden = new Set([...foldedInto(ns).keys(), ...hiddenByPhase(ns)]);
  const zones = ns.filter((n) => isZone(n)).sort((a, b) => depth(b) - depth(a));
  const waits = (z: ZNode) => isFolded(z) || hidden.has(z.id);
  let next = ns as N[];
  for (const z0 of zones) {
    if (waits(z0) || isStack(z0)) continue;
    const was = seen.get(z0.id), now = was && heldOf(next, next.find((n) => n.id === z0.id)!, m);
    if (was && now && now.shape && heldChanged(now, was)) next = fitZone(next, z0.id, m, m.min);
  }
  next = growZones(next, m);
  const out = new Map<string, Held>();
  for (const z0 of zones) {
    // (not showing, or not measured: what was known stays, to be compared when it shows)
    const keep = (waits(z0) ? null : heldOf(next, next.find((n) => n.id === z0.id)!, m)) ?? seen.get(z0.id);
    if (keep) out.set(z0.id, keep);
  }
  return { nodes: next, seen: out };
}

/** The middle of a node on the canvas: its position plus every area's around it, then half its size. */
export function centreOf(ns: readonly ZNode[], id: string, m: Metrics): { x: number; y: number } | null {
  const byId = new Map(ns.map((n) => [n.id, n]));
  const n = byId.get(id);
  if (!n) return null;
  let x = 0, y = 0;
  for (let c: ZNode | undefined = n, k = 0; c && k < 50; c = c.parentId ? byId.get(c.parentId) : undefined, k++) {
    x += c.position.x;
    y += c.position.y;
  }
  const s = sizeOf(n, m);
  return { x: x + s.width / 2, y: y + s.height / 2 };
}

/** A node's box on the canvas: its position plus every area's around it. */
function absBox(byId: ReadonlyMap<string, ZNode>, n: ZNode, m: Metrics) {
  let x = 0, y = 0;
  for (let c: ZNode | undefined = n, k = 0; c && k < 50; c = c.parentId ? byId.get(c.parentId) : undefined, k++) {
    x += c.position.x;
    y += c.position.y;
  }
  const s = sizeOf(n, m);
  return { x, y, w: s.width, h: s.height };
}

/**
 * The area a drop at `at` (a point on the canvas: where the pointer is) goes into: the innermost open area the point
 * is in, or none. Never one of the dragged nodes (`ids`) or an area inside one (nothing goes into itself), a folded
 * one (what went in would vanish on the spot) or one that isn't showing (inside a folded area, or not in its area's
 * step or state right now). Where areas overlap, the smallest counts as the innermost.
 */
export function hostAt<N extends ZNode>(ns: readonly N[], ids: readonly string[], at: { x: number; y: number }, m: Metrics): N | undefined {
  const byId = new Map<string, N>(ns.map((n) => [n.id, n]));
  const hidden = new Set([...foldedInto(ns).keys(), ...hiddenByPhase(ns)]);
  const dragged = (z: N) => {
    for (let p: string | undefined = z.id, k = 0; p && k < 50; p = byId.get(p)?.parentId, k++) if (ids.includes(p)) return true;
    return false;
  };
  let host: N | undefined, hostArea = Infinity;
  for (const z of ns) {
    if (!isZone(z) || isFolded(z) || hidden.has(z.id) || dragged(z)) continue;
    const b = absBox(byId, z, m);
    if (at.x > b.x && at.x < b.x + b.w && at.y > b.y && at.y < b.y + b.h && b.w * b.h < hostArea) {
      host = z;
      hostArea = b.w * b.h;
    }
  }
  return host;
}

/**
 * Where what was dragged belongs, decided by where the pointer let go of it and by nothing else (zones.md "소속"):
 * each dragged node (`ids`) that can sit in an area (`nestable`) goes into the area the pointer is in (`hostAt`), or
 * to the top level when the pointer is in none. A node dragged along inside a dragged area stays in that area. So a
 * thing comes into an area only by being dragged (or dropped) in, and leaves only by being dragged out: an area
 * made, moved or stretched over a block doesn't take it, and a block lying partly outside its area still belongs to
 * it. Positions are rewritten for the new parent, so nothing moves on the canvas. Into an area with steps or states
 * a node exists in the one showing; out of one, it forgets that. Parents come before their children in the result
 * (React Flow draws them in that order). Returns the same array when nothing changed.
 */
export function dropInto<N extends ZNode>(ns: readonly N[], ids: readonly string[], at: { x: number; y: number }, m: Metrics, nestable: (n: N) => boolean): N[] {
  const byId = new Map<string, N>(ns.map((n) => [n.id, n]));
  const host = hostAt(ns, ids, at, m);
  const along = (n: N) => {
    for (let p = n.parentId, k = 0; p && k < 50; p = byId.get(p)?.parentId, k++) if (ids.includes(p)) return true;
    return false;
  };
  const moved = new Set(ns.filter((n) => ids.includes(n.id) && nestable(n) && !along(n) && n.parentId !== host?.id).map((n) => n.id));
  if (!moved.size) return ns as N[];
  const base = host ? absBox(byId, host, m) : { x: 0, y: 0 };
  const showing = currentOf(host);
  return parentsFirst(
    ns.map((n) => {
      if (!moved.has(n.id)) return n;
      const b = absBox(byId, n, m);
      const { when: _w, ...rest } = n.data as WhenData & Record<string, unknown>;
      return { ...n, parentId: host?.id, position: { x: b.x - base.x, y: b.y - base.y }, data: showing ? { ...rest, when: [showing] } : rest };
    }),
  );
}

/**
 * A place for what just came into an area (zones.md "들어올 때 자리 잡기"). An area is as big as what it holds, so
 * there is rarely free room where a block is let go. When what came in (`ids`, now held by the area `hostId`) lies
 * on something the area shows, it is moved clear of all of it, to the side the pointer (`at`, on the canvas) was
 * on, seen from the middle of what was there, in one of eight directions. Right or left: beside it, centred. Up or
 * down: above or under it, centred. A corner: at that corner, along the long side of what was there (above or
 * under something wide, beside something tall), so the area stays compact. `gap` apart, on the grid. Let go on
 * free room, it stays where it is. What came in moves as one piece. A stack places its own blocks (`layStacks`):
 * nothing is done there.
 */
export function seatIn<N extends ZNode>(ns: readonly N[], ids: readonly string[], hostId: string, at: { x: number; y: number }, m: Metrics, gap: number): N[] {
  const byId = new Map<string, N>(ns.map((n) => [n.id, n]));
  const host = byId.get(hostId);
  if (!host || !isZone(host) || isStack(host) || isFolded(host)) return ns as N[];
  type B = { x0: number; y0: number; x1: number; y1: number };
  const box = (n: ZNode): B => {
    const s = sizeOf(n, m);
    return { x0: n.position.x, y0: n.position.y, x1: n.position.x + s.width, y1: n.position.y + s.height };
  };
  const around = (bs: B[]) => bs.reduce((a, b) => ({ x0: Math.min(a.x0, b.x0), y0: Math.min(a.y0, b.y0), x1: Math.max(a.x1, b.x1), y1: Math.max(a.y1, b.y1) }));
  const came = ns.filter((n) => ids.includes(n.id) && n.parentId === hostId).map(box);
  const there = ns.filter((n) => n.parentId === hostId && !ids.includes(n.id) && showsIn(host, (n.data as WhenData).when)).map(box);
  if (!came.length || !there.length || !came.some((a) => there.some((b) => a.x0 < b.x1 && b.x0 < a.x1 && a.y0 < b.y1 && b.y0 < a.y1))) return ns as N[];
  const g = around(came), c = around(there), o = absBox(byId, host, m);
  const w = g.x1 - g.x0, h = g.y1 - g.y0, cx = (c.x0 + c.x1) / 2, cy = (c.y0 + c.y1) / 2;
  // the pointer seen from the middle of what was there, in halves of its width and height (its corners lie on the
  // diagonals), as one of eight directions of 45° each: 0 right, then clockwise on screen (2 down, 4 left, 6 up).
  // The very middle counts as down
  const dx = (at.x - o.x - cx) / Math.max(1, (c.x1 - c.x0) / 2), dy = (at.y - o.y - cy) / Math.max(1, (c.y1 - c.y0) / 2);
  const dir = !dx && !dy ? 2 : (Math.round(Math.atan2(dy, dx) / (Math.PI / 4)) + 8) % 8;
  const right = dir === 7 || dir <= 1, left = dir >= 3 && dir <= 5, below = dir >= 1 && dir <= 3, above = dir >= 5;
  const corner = dir % 2 === 1;
  const snap = (v: number) => Math.round(v / m.grid) * m.grid;
  let x: number, y: number;
  if (corner ? c.x1 - c.x0 <= c.y1 - c.y0 : right || left) {
    x = right ? up(c.x1 + gap, m.grid) : down(c.x0 - w - gap, m.grid);
    y = snap(!corner ? cy - h / 2 : above ? c.y0 : c.y1 - h);
  } else {
    y = below ? up(c.y1 + gap, m.grid) : down(c.y0 - h - gap, m.grid);
    x = snap(!corner ? cx - w / 2 : left ? c.x0 : c.x1 - w);
  }
  const sx = x - g.x0, sy = y - g.y0;
  return ns.map((n) => (ids.includes(n.id) && n.parentId === hostId ? { ...n, position: { x: n.position.x + sx, y: n.position.y + sy } } : n));
}

/** A node's box on the canvas (its position plus every area's around it); null when it isn't there. */
export function boxOf(ns: readonly ZNode[], id: string, m: Metrics): { x: number; y: number; w: number; h: number } | null {
  const byId = new Map(ns.map((n) => [n.id, n]));
  const n = byId.get(id);
  return n ? absBox(byId, n, m) : null;
}

/**
 * A new area drawn around nodes where they are (the agents' area, made once around the cards): those of `ids` that
 * are at the top level go into it and stay where they are on the canvas; the area comes first in the list (a parent
 * before its children). `zone` is the area without its place and size. Nothing to wrap: the list as it was.
 */
export function wrapZone<N extends ZNode>(ns: readonly N[], ids: readonly string[], zone: N, m: Metrics): N[] {
  const held = ns.filter((n) => ids.includes(n.id) && !n.parentId);
  if (!held.length) return ns as N[];
  let x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity;
  for (const n of held) {
    const s = sizeOf(n, m);
    x0 = Math.min(x0, n.position.x);
    y0 = Math.min(y0, n.position.y);
    x1 = Math.max(x1, n.position.x + s.width);
    y1 = Math.max(y1, n.position.y + s.height);
  }
  const x = down(x0 - m.pad, m.grid), y = down(y0 - m.top, m.grid);
  const made = { ...zone, position: { x, y }, width: Math.max(m.min.width, up(x1 + m.pad - x, m.grid)), height: Math.max(m.min.height, up(y1 + m.pad - y, m.grid)) };
  return [made, ...ns.map((n) => (held.includes(n) ? { ...n, parentId: zone.id, position: { x: n.position.x - x, y: n.position.y - y } } : n))];
}

/** Everything inside an area, however deep. */
export function inside(ns: readonly ZNode[], id: string): Set<string> {
  const out = new Set<string>();
  // parents may come after their children in the list: go round until nothing new turns up
  for (let grew = true, k = 0; grew && k < 50; k++) {
    grew = false;
    for (const n of ns) {
      if (n.parentId && !out.has(n.id) && (n.parentId === id || out.has(n.parentId))) {
        out.add(n.id);
        grew = true;
      }
    }
  }
  return out;
}

/**
 * Folding an area to a chip and back. Folded: it takes the chip's size (its top-left corner stays), remembers the
 * size it had (`data.full`), and nothing inside stays picked (what can't be seen can't be acted on). Opened: it gets
 * that size back, then grows as in `growZones` (blocks may have been drawn into it meanwhile, and the area around it
 * has to hold the opened one).
 */
export function foldZone<N extends ZNode>(ns: readonly N[], id: string, folded: boolean, m: Metrics): N[] {
  const z = ns.find((n) => n.id === id);
  if (!z || !isZone(z) || isFolded(z) === folded) return ns as N[];
  if (folded) {
    const kids = inside(ns, id);
    return ns.map((n) => {
      if (n.id === id) return { ...n, ...m.chip, data: { ...n.data, folded: true, full: sizeOf(n, m) } };
      return kids.has(n.id) && n.selected ? { ...n, selected: false } : n;
    });
  }
  const { folded: _f, full, ...data } = z.data as FoldData & Record<string, unknown>;
  return growZones(
    ns.map((n) => (n.id === id ? { ...n, ...(full ?? m.min), data } : n)),
    m,
  );
}

/**
 * What a folded area stands in for: each node inside a folded area, mapped to that area (the outermost folded one
 * when they nest). Nodes that show aren't in the map.
 */
export function foldedInto(ns: readonly ZNode[]): Map<string, string> {
  const byId = new Map(ns.map((n) => [n.id, n]));
  const out = new Map<string, string>();
  for (const n of ns) {
    let host: string | undefined;
    for (let p = n.parentId, k = 0; p && k < 50; p = byId.get(p)?.parentId, k++) {
      const a = byId.get(p);
      if (a && isFolded(a)) host = p;
    }
    if (host) out.set(n.id, host);
  }
  return out;
}

/**
 * A line while areas are folded: `null` when it isn't shown (both ends in the same folded area, or one end is that
 * area itself), otherwise the two nodes it is drawn between (an end inside a folded area moves to the area's chip)
 * and whether either end moved.
 */
export function lineEnds(into: ReadonlyMap<string, string>, source: string, target: string): { source: string; target: string; moved: boolean } | null {
  const s = into.get(source) ?? source, t = into.get(target) ?? target;
  if (s === t) return null;
  return { source: s, target: t, moved: s !== source || t !== target };
}

// ---- steps and states (zones.md "단계와 상태"): an area can say when what it holds shows

/** One step or state of an area: a stable id (what blocks refer to) and the name shown. */
export type Phase = { id: string; name: string };
/**
 * What an area keeps when it has steps or states. `mode`: a timeline of steps gone through in order, or a set of
 * states picked by name (never both; an area in an area gives both). `phases`: them, in order. `at`: the id of the
 * one showing now, a plain value anything may set (the area's own control, a block's button, an agent's command).
 */
export type PhaseData = { mode?: "step" | "state"; phases?: Phase[]; at?: string };
/** What a block in such an area keeps: the steps or states it exists in (`when`: phase ids); none = all of them. */
export type WhenData = { when?: string[] };

/** An area's steps or states; none for a plain area (or anything that isn't an area). */
export const phasesOf = (n: ZNode | undefined): Phase[] => {
  const d = (n?.data ?? {}) as PhaseData;
  return n && isZone(n) && d.mode ? (d.phases ?? []) : [];
};
/** The step or state an area shows now: `at`, or the first when `at` names none of them. */
export const currentOf = (n: ZNode | undefined): string | undefined => {
  const ps = phasesOf(n);
  const at = (n?.data as PhaseData | undefined)?.at;
  return (ps.find((p) => p.id === at) ?? ps[0])?.id;
};
/** How a step or state is called: "2단계 · 이름" (its place counts), or the state's name. */
export const phaseLabel = (z: ZNode | undefined, id: string): string => {
  const ps = phasesOf(z);
  const i = ps.findIndex((p) => p.id === id);
  if (i < 0) return "";
  const name = ps[i].name.trim();
  return (z!.data as PhaseData).mode === "step" ? `Step ${i + 1}${name ? ` · ${name}` : ""}` : name || `State ${i + 1}`;
};

/** Whether something limited to `when` shows in area `z` now. Ids the area no longer has don't count; none left = all. */
const showsIn = (z: ZNode | undefined, when: unknown): boolean => {
  const ps = phasesOf(z);
  if (!ps.length || !Array.isArray(when)) return true;
  const mine = when.filter((id) => ps.some((p) => p.id === id));
  return !mine.length || mine.includes(currentOf(z));
};

/**
 * What the areas' steps and states hide right now: a block that doesn't exist in the step or state its area shows,
 * and everything inside such a block (an area hidden this way hides what it holds).
 */
export function hiddenByPhase(ns: readonly ZNode[]): Set<string> {
  const byId = new Map(ns.map((n) => [n.id, n]));
  const out = new Set<string>();
  for (const n of ns) {
    for (let c: ZNode | undefined = n, k = 0; c?.parentId && k < 50; c = byId.get(c.parentId), k++) {
      if (!showsIn(byId.get(c.parentId), (c.data as WhenData).when)) {
        out.add(n.id);
        break;
      }
    }
  }
  return out;
}

const patchZone = <N extends ZNode>(ns: readonly N[], id: string, change: (d: PhaseData & Record<string, unknown>, z: N) => Record<string, unknown> | null): N[] => {
  let hit = false;
  const next = ns.map((n) => {
    if (n.id !== id || !isZone(n)) return n;
    const data = change(n.data as PhaseData & Record<string, unknown>, n);
    if (!data) return n;
    hit = true;
    return { ...n, data };
  });
  return hit ? next : (ns as N[]);
};

/**
 * Gives an area steps or states, switches between the two (its list stays), or takes them away (`mode` undefined:
 * the area is plain again and everything in it shows). An area new to this starts with one, `first`.
 */
export function setPhaseMode<N extends ZNode>(ns: readonly N[], id: string, mode: "step" | "state" | undefined, first: Phase): N[] {
  if (mode) {
    return patchZone(ns, id, (d) => {
      const phases = d.phases?.length ? d.phases : [first];
      return { ...d, mode, phases, at: (phases.find((p) => p.id === d.at) ?? phases[0]).id };
    });
  }
  if (!(ns.find((n) => n.id === id)?.data as PhaseData | undefined)?.mode) return ns as N[];
  return ns.map((n) => {
    if (n.id === id && isZone(n)) {
      const { mode: _m, phases: _p, at: _a, ...data } = n.data as PhaseData & Record<string, unknown>;
      return { ...n, data };
    }
    if (n.parentId === id && (n.data as WhenData).when) {
      const { when: _w, ...data } = n.data as WhenData & Record<string, unknown>;
      return { ...n, data };
    }
    return n;
  });
}

/** A new step or state at the end of an area's list; the area goes to it (it is what is being made). */
export function addPhase<N extends ZNode>(ns: readonly N[], id: string, phase: Phase): N[] {
  return patchZone(ns, id, (d) => (d.mode ? { ...d, phases: [...(d.phases ?? []), phase], at: phase.id } : null));
}

/** A step's or state's name. */
export function renamePhase<N extends ZNode>(ns: readonly N[], id: string, phaseId: string, name: string): N[] {
  return patchZone(ns, id, (d) => (d.phases?.some((p) => p.id === phaseId) ? { ...d, phases: d.phases.map((p) => (p.id === phaseId ? { ...p, name } : p)) } : null));
}

/**
 * Takes a step or state away (never the last one). What existed only in it stays, in every one that is left (nothing
 * vanishes with a step); the area goes to the one that took its place.
 */
export function removePhase<N extends ZNode>(ns: readonly N[], id: string, phaseId: string): N[] {
  const z = ns.find((n) => n.id === id);
  const ps = phasesOf(z);
  const i = ps.findIndex((p) => p.id === phaseId);
  if (i < 0 || ps.length < 2) return ns as N[];
  const left = ps.filter((p) => p.id !== phaseId);
  return ns.map((n) => {
    if (n.id === id) {
      const d = n.data as PhaseData & Record<string, unknown>;
      return { ...n, data: { ...d, phases: left, at: d.at === phaseId || !left.some((p) => p.id === d.at) ? left[Math.min(i, left.length - 1)].id : d.at } };
    }
    const when = (n.data as WhenData).when;
    if (n.parentId !== id || !when?.includes(phaseId)) return n;
    const { when: _w, ...rest } = n.data as WhenData & Record<string, unknown>;
    const kept = when.filter((w) => w !== phaseId);
    return { ...n, data: kept.length ? { ...rest, when: kept } : rest };
  });
}

/** Moves an area to a step or state: by id, or one on ("next") or back ("prev") in its list (it stops at the ends). */
export function goPhase<N extends ZNode>(ns: readonly N[], id: string, to: string): N[] {
  return patchZone(ns, id, (d, z) => {
    const ps = phasesOf(z);
    const i = ps.findIndex((p) => p.id === currentOf(z));
    const next = to === "next" ? ps[Math.min(i + 1, ps.length - 1)] : to === "prev" ? ps[Math.max(i - 1, 0)] : ps.find((p) => p.id === to);
    return next && next.id !== currentOf(z) ? { ...d, at: next.id } : null;
  });
}

/**
 * The steps or states a block exists in after `phaseId` is pressed in its list. From "all of them": only that one.
 * Otherwise that one is added or dropped; none left, or all of them again, is "all of them" (undefined).
 */
export function toggleWhen(when: string[] | undefined, all: readonly Phase[], phaseId: string): string[] | undefined {
  const mine = (when ?? []).filter((id) => all.some((p) => p.id === id));
  if (!mine.length) return [phaseId];
  const next = mine.includes(phaseId) ? mine.filter((id) => id !== phaseId) : [...mine, phaseId];
  return !next.length || next.length === all.length ? undefined : all.filter((p) => next.includes(p.id)).map((p) => p.id);
}

/** Says in which steps or states these blocks exist (`when` undefined: all of them). */
export function setWhen<N extends ZNode>(ns: readonly N[], ids: readonly string[], when: string[] | undefined): N[] {
  return ns.map((n) => {
    if (!ids.includes(n.id)) return n;
    const { when: _w, ...rest } = n.data as WhenData & Record<string, unknown>;
    return { ...n, data: when?.length ? { ...rest, when } : rest };
  });
}

/** The innermost area with steps or states that holds both nodes (a line between them can follow it); none: undefined. */
export function commonPhased(ns: readonly ZNode[], a: string, b: string): string | undefined {
  const byId = new Map(ns.map((n) => [n.id, n]));
  const above = (id: string) => {
    const out: string[] = [];
    for (let p = byId.get(id)?.parentId, k = 0; p && k < 50; p = byId.get(p)?.parentId, k++) out.push(p);
    return out;
  };
  const over = new Set(above(b));
  return above(a).find((id) => over.has(id) && phasesOf(byId.get(id)).length > 0);
}

/**
 * Whether a line is hidden by steps and states: an end of it is (`hidden`: `hiddenByPhase`), or the line itself is
 * limited (`when`, read against area `whenZone`) to steps or states other than the one showing.
 */
export function lineHiddenByPhase(ns: readonly ZNode[], hidden: ReadonlySet<string>, line: { source: string; target: string; when?: string[]; whenZone?: string }): boolean {
  if (hidden.has(line.source) || hidden.has(line.target)) return true;
  return !!line.whenZone && !showsIn(ns.find((n) => n.id === line.whenZone), line.when);
}

// ---- stacks (zones.md "묶음"): an area that keeps what it holds in one column and is exactly as big as that takes

/** What a stack keeps on its area: `stack`. */
export type StackData = { stack?: boolean };
export const isStack = (n: ZNode | undefined) => !!n && isZone(n) && !!(n.data as StackData).stack;

const sized = (n: ZNode) => (n.width ?? n.measured?.width) != null && (n.height ?? n.measured?.height) != null;
/** The list with parents before their children (React Flow draws them in that order); the order is otherwise kept. */
function parentsFirst<N extends ZNode>(ns: readonly N[]): N[] {
  const depth = depths(ns);
  return ns
    .map((n, i) => ({ n, i, d: depth(n) }))
    .sort((a, b) => a.d - b.d || a.i - b.i)
    .map((x) => x.n);
}

/**
 * Every open stack laid out: what it holds in one column, in the order their middles are from the top (so a block
 * dropped between two others lands between them), `gap` apart, and the stack exactly as big as that takes: it shrinks
 * too, unlike a plain area. A stack that holds nothing is taken away (it was only ever its contents). A stack holding
 * a block with no size yet waits for it. Innermost first, so a stack in a stack is counted at its new size. Returns
 * the same array when nothing changed.
 */
export function layStacks<N extends ZNode>(ns: readonly N[], m: Metrics, gap: number): N[] {
  const depth = depths(ns);
  const stacks = ns.filter((n) => isStack(n) && !isFolded(n)).sort((a, b) => depth(b) - depth(a));
  let next = ns as N[];
  for (const z0 of stacks) {
    const z = next.find((n) => n.id === z0.id)!;
    const kids = next.filter((n) => n.parentId === z.id);
    if (!kids.length) {
      next = next.filter((n) => n.id !== z.id);
      continue;
    }
    if (!kids.every(sized)) continue;
    const order = kids.map((k, i) => ({ k, i, mid: k.position.y + sizeOf(k, m).height / 2 })).sort((a, b) => a.mid - b.mid || a.i - b.i);
    const at = new Map<string, number>();
    let y = m.top, wide = 0;
    for (const { k } of order) {
      const s = sizeOf(k, m);
      at.set(k.id, y);
      y += s.height + gap;
      wide = Math.max(wide, s.width);
    }
    const width = Math.max(m.min.width, up(wide + 2 * m.pad, m.grid)), height = Math.max(m.min.height, up(y - gap + m.pad, m.grid));
    const s = sizeOf(z, m);
    if (s.width === width && s.height === height && kids.every((k) => k.position.x === m.pad && k.position.y === at.get(k.id))) continue;
    next = next.map((n) => {
      if (n.id === z.id) return { ...n, width, height };
      return at.has(n.id) ? { ...n, position: { x: m.pad, y: at.get(n.id)! } } : n;
    });
  }
  return next;
}

/**
 * A stack made by dropping blocks on a block: `zone` (the stack, without its place and size) takes the target's
 * place where the target was (in the same area, in the same steps or states), the target is its first block and the
 * dropped ones (`ids`) come under it, in the order they were from the top. The target stays where it was on the
 * canvas. Nothing dropped, or no such target: the list as it was.
 */
export function stackOnto<N extends ZNode>(ns: readonly N[], ids: readonly string[], targetId: string, zone: N, m: Metrics, gap: number): N[] {
  const byId = new Map<string, ZNode>(ns.map((n) => [n.id, n]));
  const t = byId.get(targetId);
  const moved = ns.filter((n) => ids.includes(n.id) && n.id !== targetId).sort((a, b) => absBox(byId, a, m).y - absBox(byId, b, m).y);
  if (!t || !moved.length) return ns as N[];
  const at = new Map<string, number>();
  let y = m.top, wide = 0;
  for (const k of [t, ...moved]) {
    const s = sizeOf(k, m);
    at.set(k.id, y);
    y += s.height + gap;
    wide = Math.max(wide, s.width);
  }
  const when = (t.data as WhenData).when;
  const made = {
    ...zone,
    parentId: t.parentId,
    position: { x: t.position.x - m.pad, y: t.position.y - m.top },
    width: Math.max(m.min.width, up(wide + 2 * m.pad, m.grid)),
    height: Math.max(m.min.height, up(y - gap + m.pad, m.grid)),
    data: when ? { ...zone.data, when } : zone.data,
  };
  const out = ns.map((n) => {
    if (!at.has(n.id)) return n;
    // (which step it shows in is the stack's to say now)
    const { when: _w, ...data } = n.data as WhenData & Record<string, unknown>;
    return { ...n, parentId: zone.id, position: { x: m.pad, y: at.get(n.id)! }, data };
  });
  return parentsFirst([made, ...out]);
}

/** A line while blocks sit in stacks: each block in a stack (however deep), mapped to the outermost stack it is in. */
export function stackedInto(ns: readonly ZNode[]): Map<string, string> {
  const byId = new Map(ns.map((n) => [n.id, n]));
  const out = new Map<string, string>();
  for (const n of ns) {
    let host: string | undefined;
    for (let p = n.parentId, k = 0; p && k < 50; p = byId.get(p)?.parentId, k++) if (isStack(byId.get(p))) host = p;
    if (host) out.set(n.id, host);
  }
  return out;
}

// ---- tidying (zones.md "배치 정리"): what an area holds put straight, once, when asked

/**
 * Boxes read as rows, the way a page is read: sorted from the top, a box joins the row before it when it overlaps
 * that row's first box by half its own height or of that box's (whichever is less); each row is then sorted from
 * the left. Returns the rows as indices into `boxes`.
 */
export function rowsOf(boxes: readonly { x: number; y: number; w: number; h: number }[]): number[][] {
  const order = boxes.map((_, i) => i).sort((a, b) => boxes[a].y - boxes[b].y || boxes[a].x - boxes[b].x);
  const rows: number[][] = [];
  for (const i of order) {
    const row = rows[rows.length - 1], head = row ? boxes[row[0]] : undefined, b = boxes[i];
    const shared = head ? Math.min(head.y + head.h, b.y + b.h) - Math.max(head.y, b.y) : 0;
    if (row && head && shared >= Math.min(head.h, b.h) / 2) row.push(i);
    else rows.push([i]);
  }
  return rows.map((r) => r.sort((a, b) => boxes[a].x - boxes[b].x));
}

/**
 * What an area holds put straight, like a row layout that wraps (zones.md "배치 정리"): what shows in it is read as
 * rows (`rowsOf`), and every row is set on one line: tops aligned, `gap` apart across and down, from the area's
 * padding. Rows keep to the left, or stay centred on each other when that is how they were (a tree drawn top down).
 * What can be given a size (`sizable`: structure blocks) is made as wide as the widest of them, and as tall as the
 * tallest in its row. Then the area is fitted. Nothing is done to a stack (it lays itself out), an area with steps
 * or states (what it holds may share a place on purpose) or a folded one. Returns the same array when the area
 * isn't one of those to tidy.
 */
export function tidyArea<N extends ZNode>(ns: readonly N[], id: string, m: Metrics, gap: number, sizable: (n: N) => boolean): N[] {
  const z = ns.find((n) => n.id === id);
  if (!z || !isZone(z) || isStack(z) || isFolded(z) || phasesOf(z).length) return ns as N[];
  const kids = ns.filter((n) => n.parentId === id);
  if (!kids.length) return ns as N[];
  const boxes = kids.map((k) => ({ x: k.position.x, y: k.position.y, w: sizeOf(k, m).width, h: sizeOf(k, m).height }));
  const rows = rowsOf(boxes);
  // the sizes they get: every sizable one as wide as the widest, and as tall as the tallest of its row
  const wide = Math.max(0, ...kids.map((k, i) => (sizable(k) ? boxes[i].w : 0)));
  const size = new Map<number, { w: number; h: number }>();
  for (const row of rows) {
    const tall = Math.max(0, ...row.map((i) => (sizable(kids[i]) ? boxes[i].h : 0)));
    for (const i of row) size.set(i, sizable(kids[i]) ? { w: wide, h: tall } : { w: boxes[i].w, h: boxes[i].h });
  }
  // centred rows stay centred: when their middles lined up better than their left sides
  const span = (vs: number[]) => Math.max(...vs) - Math.min(...vs);
  const lefts = rows.map((r) => Math.min(...r.map((i) => boxes[i].x)));
  const mids = rows.map((r, k) => (lefts[k] + Math.max(...r.map((i) => boxes[i].x + boxes[i].w))) / 2);
  const centred = rows.length > 1 && span(mids) + m.grid / 2 < span(lefts);
  const widthOf = (r: number[]) => r.reduce((t, i) => t + size.get(i)!.w, 0) + gap * (r.length - 1);
  const widest = Math.max(...rows.map(widthOf));
  const at = new Map<string, { x: number; y: number; w: number; h: number }>();
  let y = m.top;
  for (const row of rows) {
    let x = m.pad + (centred ? Math.round((widest - widthOf(row)) / 2 / m.grid) * m.grid : 0);
    for (const i of row) {
      const s = size.get(i)!;
      at.set(kids[i].id, { x, y, ...s });
      x += s.w + gap;
    }
    y += Math.max(...row.map((i) => size.get(i)!.h)) + gap;
  }
  const placed = ns.map((n) => {
    const p = at.get(n.id);
    if (!p) return n;
    const s = sizeOf(n, m);
    if (n.position.x === p.x && n.position.y === p.y && s.width === p.w && s.height === p.h) return n;
    return sizable(n) ? { ...n, position: { x: p.x, y: p.y }, width: p.w, height: p.h } : { ...n, position: { x: p.x, y: p.y } };
  });
  return fitZone(placed, id, m, m.min);
}
