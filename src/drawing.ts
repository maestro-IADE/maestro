import { GRID } from "./nodes/AgentNode";
import { ARROW } from "./Canvas";
import { BLOCK_SIZE, ZONE_SIZE, ZONE_Z } from "./nodes/StructNodes";
import type { CanvasEdge, CanvasNode } from "./types";

/** What an agent draws with `maestro draw` (checked by the backend: every name it points to exists in it). */
export type Drawing = {
  areas: { title: string; in?: string }[];
  blocks: { title: string; body?: string; area?: string }[];
  /** `relation`: the relation id the backend put for the arrow's `as` (declared there if it was new) */
  arrows: { from: string; to: string; label?: string; as?: string; relation?: string; body?: string }[];
};

const PAD = GRID, TOP = 2 * GRID, GAP = GRID, ROOT_GAP = 2 * GRID;
type Item = { kind: "area"; title: string; parent?: string } | { kind: "block"; title: string; body?: string; parent?: string };
type Size = { w: number; h: number };

/**
 * The canvas after a drawing: blocks and areas already there (same name) are updated in place, new ones are laid
 * out (each area as rows of what it holds, the top-level ones in a row right of what's on the canvas), and
 * arrows are added or relabelled.
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
    ...d.areas.filter((a) => !byTitle.has(a.title.trim())).map((a): Item => ({ kind: "area", title: a.title.trim(), parent: a.in?.trim() || undefined })),
    ...d.blocks.filter((b) => !byTitle.has(b.title.trim())).map((b): Item => ({ kind: "block", title: b.title.trim(), body: b.body, parent: b.area?.trim() || undefined })),
  ];
  const kids = (t: string) => items.filter((i) => i.parent === t);

  // an area's size: its contents in rows of at most three, under its name
  const rowsOf = (list: Item[]) => {
    const per = Math.min(3, Math.max(1, Math.ceil(Math.sqrt(list.length))));
    return Array.from({ length: Math.ceil(list.length / per) }, (_, r) => list.slice(r * per, r * per + per));
  };
  const measure = (i: Item, depth = 0): Size => {
    if (i.kind === "block" || depth > 20) return { w: BLOCK_SIZE.width, h: BLOCK_SIZE.height };
    const inner = kids(i.title);
    if (!inner.length) return { w: ZONE_SIZE.width, h: ZONE_SIZE.height };
    let w = 0, h = TOP;
    for (const row of rowsOf(inner)) {
      const s = row.map((k) => measure(k, depth + 1));
      w = Math.max(w, s.reduce((a, x) => a + x.w, 0) + GAP * (row.length - 1));
      h += Math.max(...s.map((x) => x.h)) + GAP;
    }
    return { w: w + 2 * PAD, h: h - GAP + PAD };
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
    let cy = TOP;
    for (const row of rowsOf(kids(i.title))) {
      let cx = PAD;
      const ss = row.map((k) => measure(k));
      row.forEach((k, j) => {
        place(k, cx, cy, id, depth + 1);
        cx += ss[j].w + GAP;
      });
      cy += Math.max(...ss.map((x) => x.h)) + GAP;
    }
  };

  // new things inside an area that's already on the canvas: stacked under what it holds, the area grows
  for (const i of items.filter((i) => i.parent && byTitle.has(i.parent))) {
    const host = nodes.find((n) => n.id === byTitle.get(i.parent!)!.id)!;
    const s = measure(i);
    const h = host.height ?? ZONE_SIZE.height;
    place(i, PAD, h - PAD + GAP, host.id);
    nodes = nodes.map((n) => (n.id === host.id ? { ...n, height: h + s.h + GAP, width: Math.max(n.width ?? ZONE_SIZE.width, s.w + 2 * PAD) } : n));
  }
  // the rest of the top level: in a row to the right of everything on the canvas
  const top = ns.filter((n) => !n.parentId);
  let x = top.length ? Math.max(...top.map((n) => n.position.x + (n.width ?? 0))) + 4 * GRID : 0;
  const y = top.length ? Math.min(...top.map((n) => n.position.y)) : 0;
  for (const i of items.filter((i) => !i.parent)) {
    place(i, x, y);
    x += measure(i).w + ROOT_GAP;
  }
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
