import { GRID } from "./nodes/AgentNode";
import { ARROW } from "./Canvas";
import { BLOCK_SIZE, ZONE_SIZE, ZONE_Z } from "./nodes/StructNodes";
import type { CanvasEdge, CanvasNode } from "./types";
import { arrange, type Arranged, type Layout } from "./drawLayout";

/** What an agent draws with `maestro draw` (checked by the backend: every name it points to exists in it). */
export type Drawing = {
  /** how the top-level areas and blocks are placed (drawLayout.ts); none: in a row, as before */
  layout?: Layout;
  /** `layout`: how the area places what it holds; none: grid */
  areas: { title: string; in?: string; layout?: Layout }[];
  blocks: { title: string; body?: string; area?: string }[];
  /** `relation`: the relation id the backend put for the arrow's `as` (declared there if it was new) */
  arrows: { from: string; to: string; label?: string; as?: string; relation?: string; body?: string }[];
};

const PAD = GRID, TOP = 2 * GRID, GAP = GRID, ROOT_GAP = 2 * GRID, RANK_GAP = 3 * GRID;
type Item = { kind: "area"; title: string; parent?: string; layout?: Layout } | { kind: "block"; title: string; body?: string; parent?: string };
type Size = { w: number; h: number };

/**
 * The canvas after a drawing: blocks and areas already there (same name) are updated in place, new ones are laid
 * out by the drawing's intent (each area's `layout` for what it holds, the top `layout` for the top level, right of
 * what's on the canvas), and arrows are added or relabelled.
 */
export function applyDrawing(d: Drawing, ns: CanvasNode[], es: CanvasEdge[]): { nodes: CanvasNode[]; edges: CanvasEdge[] } {
  const stamp = Date.now();
  let seq = 0;
  const newId = (p: string) => `${p}_${stamp}_${seq++}`;
  const drawn = (n: CanvasNode) => n.type === "struct_block" || n.type === "zone";
  const title = (n: CanvasNode) => ((n.data as { title?: string }).title ?? "").trim();
  const byTitle = new Map<string, CanvasNode>();
  for (const n of ns) if (drawn(n) && !byTitle.has(title(n))) byTitle.set(title(n), n);

  // what's there already: a block gets its new notes
  let nodes = ns.map((n) => {
    const b = n.type === "struct_block" && d.blocks.find((x) => x.title.trim() === title(n));
    return b && b.body != null ? ({ ...n, data: { ...n.data, body: b.body } } as CanvasNode) : n;
  });

  const items: Item[] = [
    ...d.areas.filter((a) => !byTitle.has(a.title.trim())).map((a): Item => ({ kind: "area", title: a.title.trim(), parent: a.in?.trim() || undefined, layout: a.layout })),
    ...d.blocks.filter((b) => !byTitle.has(b.title.trim())).map((b): Item => ({ kind: "block", title: b.title.trim(), body: b.body, parent: b.area?.trim() || undefined })),
  ];
  const byName = new Map(items.map((i) => [i.title, i]));
  const kids = (t: string | undefined) => items.filter((i) => (t === undefined ? !i.parent : i.parent === t));

  // the arrows between what one container holds: an arrow from or to something inside a held area counts for that area
  const heldBy = (name: string, container: string | undefined) => {
    let i = byName.get(name.trim());
    for (let k = 0; i && k < 50; k++, i = i.parent ? byName.get(i.parent) : undefined) if (i.parent === container) return i.title;
    return undefined;
  };
  const linksIn = (container: string | undefined) =>
    d.arrows.flatMap((a) => {
      const f = heldBy(a.from, container), t = heldBy(a.to, container);
      return f && t && f !== t ? [[f, t] as const] : [];
    });

  // where a container's contents go: computed once per container (sizes need the contents' arrangements first)
  const plans = new Map<string, Arranged>();
  const plan = (container: string | undefined, depth = 0): Arranged => {
    const key = container ?? " top";
    const known = plans.get(key);
    if (known) return known;
    const list = kids(container);
    const layout: Layout = container === undefined ? (d.layout ?? "row") : ((byName.get(container) as { layout?: Layout } | undefined)?.layout ?? "grid");
    // keyed by place, not name: an area and a block of the same name (linted) still get a place each
    const slot = (t: string) => String(list.findIndex((i) => i.title === t));
    const links = linksIn(container).map(([f, t]) => [slot(f), slot(t)] as const);
    const p = arrange(list.map((_, n) => String(n)), list.map((i) => measure(i, depth + 1)), links, layout, container === undefined ? ROOT_GAP : GAP, RANK_GAP);
    plans.set(key, p);
    return p;
  };
  // an area's size: its contents as its layout places them, under its name
  const measure = (i: Item, depth = 0): Size => {
    if (i.kind === "block" || depth > 20) return { w: BLOCK_SIZE.width, h: BLOCK_SIZE.height };
    if (!kids(i.title).length) return { w: ZONE_SIZE.width, h: ZONE_SIZE.height };
    const p = plan(i.title, depth);
    return { w: p.w + 2 * PAD, h: TOP + p.h + PAD };
  };

  const ids = new Map<string, string>([...byTitle].map(([t, n]) => [t, n.id]));
  const created: CanvasNode[] = [];
  const place = (i: Item, x: number, y: number, parentId?: string, depth = 0) => {
    const s = measure(i);
    const id = newId(i.kind === "area" ? "zone" : "block");
    ids.set(i.title, id);
    created.push(
      i.kind === "area"
        ? ({ id, type: "zone", position: { x, y }, width: s.w, height: s.h, zIndex: ZONE_Z, parentId, data: { title: i.title } } as CanvasNode)
        : ({ id, type: "struct_block", position: { x, y }, width: s.w, height: s.h, parentId, data: { title: i.title, body: i.body } } as CanvasNode),
    );
    if (i.kind !== "area" || depth > 20) return;
    const p = plan(i.title);
    kids(i.title).forEach((k, j) => place(k, PAD + p.at[j].x, TOP + p.at[j].y, id, depth + 1));
  };

  // new things inside an area that's already on the canvas: stacked under what it holds, the area grows
  for (const i of items.filter((i) => i.parent && byTitle.has(i.parent))) {
    const host = nodes.find((n) => n.id === byTitle.get(i.parent!)!.id)!;
    const s = measure(i);
    const h = host.height ?? ZONE_SIZE.height;
    place(i, PAD, h - PAD + GAP, host.id);
    nodes = nodes.map((n) => (n.id === host.id ? { ...n, height: h + s.h + GAP, width: Math.max(n.width ?? ZONE_SIZE.width, s.w + 2 * PAD) } : n));
  }
  // the rest of the top level: to the right of everything on the canvas, as the top layout places it
  const top = ns.filter((n) => !n.parentId);
  const x0 = top.length ? Math.max(...top.map((n) => n.position.x + (n.width ?? 0))) + 4 * GRID : 0;
  const y0 = top.length ? Math.min(...top.map((n) => n.position.y)) : 0;
  const tops = kids(undefined);
  const p = plan(undefined);
  tops.forEach((i, j) => place(i, x0 + p.at[j].x, y0 + p.at[j].y));
  nodes = [...nodes, ...created];

  // arrows leave and enter on the sides facing each other (positions summed up through the areas)
  const byId = new Map(nodes.map((n) => [n.id, n]));
  const abs = (id: string) => {
    let px = 0, py = 0;
    for (let n = byId.get(id), k = 0; n && k < 50; n = n.parentId ? byId.get(n.parentId) : undefined, k++) {
      px += n.position.x;
      py += n.position.y;
    }
    const n = byId.get(id);
    return { x: px + (n?.width ?? 0) / 2, y: py + (n?.height ?? 0) / 2 };
  };
  let edges = es;
  for (const a of d.arrows) {
    const source = ids.get(a.from.trim()), target = ids.get(a.to.trim());
    if (!source || !target || source === target) continue;
    const label = a.label?.trim() || undefined;
    const body = a.body?.trim() || undefined;
    const relation = a.relation && a.relation !== "plain" ? a.relation : undefined;
    const old = edges.find((e) => e.data?.kind === "arrow" && e.source === source && e.target === target);
    if (old) {
      edges = edges.map((e) => (e === old ? { ...e, data: { ...e.data!, label, ...(body != null && { body }), ...(a.relation != null && { relation }) } } : e));
      continue;
    }
    const p = abs(source), q = abs(target);
    const dx = q.x - p.x, dy = q.y - p.y;
    const [sourceHandle, targetHandle] = Math.abs(dy) >= Math.abs(dx) ? (dy >= 0 ? ["bottom", "top"] : ["top", "bottom"]) : dx >= 0 ? ["right", "left"] : ["left", "right"];
    edges = [...edges, { id: newId("arrow"), type: "arrow", source, target, sourceHandle, targetHandle, markerEnd: ARROW, data: { kind: "arrow", label, body, relation } }];
  }
  return { nodes, edges };
}

/** A question an agent asks with `maestro ask` (checked by the backend). */
export type Ask = { id: string; title: string; body?: string; choices: string[]; multi: boolean; other: boolean };

/**
 * The canvas with question `q` on it, asked by the agent `owner`: to the right of everything at the top level, at the
 * top (where a new drawing would go). Asked twice (the same id), it is there once.
 */
export function applyAsk(q: Ask, owner: string, ns: CanvasNode[]): CanvasNode[] {
  if (ns.some((n) => n.type === "question_block" && n.data.qid === q.id)) return ns;
  const top = ns.filter((n) => !n.parentId);
  const x = top.length ? Math.max(...top.map((n) => n.position.x + (n.width ?? n.measured?.width ?? 0))) + 4 * GRID : 0;
  const y = top.length ? Math.min(...top.map((n) => n.position.y)) : 0;
  const data = { qid: q.id, owner, title: q.title, body: q.body, choices: q.choices.length ? q.choices : undefined, multi: q.multi || undefined, other: q.other || undefined };
  return [...ns, { id: `question_${q.id}`, type: "question_block", position: { x: Math.round(x / GRID) * GRID, y: Math.round(y / GRID) * GRID }, data } as CanvasNode];
}

/** What an agent erases with `maestro erase` (names checked by the backend against the canvas as last saved). */
export type Erase = { all?: boolean; areas?: string[]; blocks?: string[]; arrows?: { from: string; to: string }[] };

/**
 * The canvas after an erase: the named blocks and areas go (or every drawn one with `all`), with every line touching
 * them; the named arrows go (or every drawn arrow with `all`). What an erased area held stays where it is on screen,
 * one level up (as deleting an area by hand). Agents, documents and prompt blocks are never touched.
 */
export function applyErase(x: Erase, ns: CanvasNode[], es: CanvasEdge[]): { nodes: CanvasNode[]; edges: CanvasEdge[] } {
  const title = (n: CanvasNode) => ((n.data as { title?: string }).title ?? "").trim();
  const named = (list: string[] | undefined, type: string) => new Set(ns.filter((n) => n.type === type && (list ?? []).some((t) => t.trim() === title(n))).map((n) => n.id));
  const gone = x.all
    ? new Set(ns.filter((n) => n.type === "struct_block" || n.type === "zone").map((n) => n.id))
    : new Set([...named(x.blocks, "struct_block"), ...named(x.areas, "zone")]);
  const byId = new Map(ns.map((n) => [n.id, n]));
  const abs = (id: string) => {
    let px = 0, py = 0;
    for (let n = byId.get(id), k = 0; n && k < 50; n = n.parentId ? byId.get(n.parentId) : undefined, k++) {
      px += n.position.x;
      py += n.position.y;
    }
    return { x: px, y: py };
  };
  const nodes = ns
    .filter((n) => !gone.has(n.id))
    .map((n) => {
      if (!n.parentId || !gone.has(n.parentId)) return n;
      let p: string | undefined = n.parentId;
      for (let k = 0; p && gone.has(p) && k < 50; k++) p = byId.get(p)?.parentId;
      const a = abs(n.id), base = p ? abs(p) : { x: 0, y: 0 };
      return { ...n, parentId: p, position: { x: a.x - base.x, y: a.y - base.y } } as CanvasNode;
    });
  const titled = (id: string, t: string) => {
    const n = byId.get(id);
    return !!n && (n.type === "struct_block" || n.type === "zone") && title(n) === t.trim();
  };
  const edges = es.filter((e) => {
    if (gone.has(e.source) || gone.has(e.target)) return false;
    if (e.data?.kind !== "arrow") return true;
    if (x.all) return false;
    return !(x.arrows ?? []).some((a) => titled(e.source, a.from) && titled(e.target, a.to));
  });
  return { nodes, edges };
}
