// A small picture of a piece of a canvas (docs/features/agents/chat-visuals.md): where its areas and blocks are and
// what joins them, so it can be shown somewhere else (a card in the chat). Pure, so scripts/focus-check.ts runs it in
// Node.
import type { ZNode } from "./zones";

export type MiniKind = "area" | "block" | "question" | "view" | "doc" | "other";
/**
 * One thing in the picture: its box (in the picture's own coordinates), its name, and how deep it sits in areas.
 * `view`: a view block's file and what it is, so the picture can show it (the image, the page).
 */
export type MiniBox = { id: string; kind: MiniKind; x: number; y: number; w: number; h: number; title: string; color?: string; depth: number; view?: { path: string; kind: "image" | "html" } };
export type MiniLine = { x1: number; y1: number; x2: number; y2: number };
/** The picture: its size, its boxes (areas first, the outer ones before the ones in them) and its lines. */
export type Mini = { w: number; h: number; boxes: MiniBox[]; lines: MiniLine[] };

const KINDS: Record<string, MiniKind> = { zone: "area", struct_block: "block", question_block: "question", view_block: "view", markdown_block: "doc" };
/** What a view block's file is, by its name (as nodes/ViewBlock.tsx `viewKind`), for a block that doesn't say. */
const viewKind = (path: string): "image" | "html" | null => (/\.(png|jpe?g|gif|webp|svg)$/i.test(path) ? "image" : /\.html?$/i.test(path) ? "html" : null);

/** Where the line from the middle of box `a` towards point `to` leaves the box. */
function edgeOf(a: { x: number; y: number; w: number; h: number }, to: { x: number; y: number }) {
  const cx = a.x + a.w / 2, cy = a.y + a.h / 2, dx = to.x - cx, dy = to.y - cy;
  if (!dx && !dy) return { x: cx, y: cy };
  const t = Math.min(dx ? a.w / 2 / Math.abs(dx) : Infinity, dy ? a.h / 2 / Math.abs(dy) : Infinity);
  return { x: cx + dx * t, y: cy + dy * t };
}

/**
 * The picture of some nodes and the lines between them: every node that shows (`hidden`: those that don't, in a
 * folded area or not in the step their area shows), placed as on the canvas and moved so the whole starts `pad`
 * from the corner. A line is drawn between the edges of the two boxes it joins, and left out when an end isn't in
 * the picture. `size`: a node's size (a block not measured yet: about what it will be). `only`: the picture is of
 * these nodes alone (they are still placed by the areas they sit in, which needn't be among them).
 */
export function miniOf<N extends ZNode>(
  nodes: readonly N[],
  edges: readonly { source: string; target: string }[],
  size: (n: N) => { w: number; h: number },
  hidden: ReadonlySet<string> = new Set(),
  pad = 12,
  only?: ReadonlySet<string>,
): Mini {
  const byId = new Map(nodes.map((n) => [n.id, n]));
  const placed = nodes
    .filter((n) => !hidden.has(n.id) && (!only || only.has(n.id)))
    .map((n) => {
      let x = 0, y = 0, depth = 0;
      for (let c: N | undefined = n, k = 0; c && k < 50; c = c.parentId ? byId.get(c.parentId) : undefined, k++) {
        x += c.position.x;
        y += c.position.y;
        if (k) depth++;
      }
      const s = size(n), d = n.data as { title?: unknown; path?: unknown; color?: unknown; kind?: unknown };
      const shows = n.type === "view_block" && typeof d.path === "string" ? (d.kind === "image" || d.kind === "html" ? d.kind : viewKind(d.path)) : null;
      const name = typeof d.title === "string" && d.title.trim() ? d.title.trim() : typeof d.path === "string" ? (d.path.split(/[\\/]/).pop() ?? "") : "";
      return { id: n.id, kind: KINDS[n.type ?? ""] ?? ("other" as MiniKind), x, y, w: s.w, h: s.h, title: name, color: typeof d.color === "string" ? d.color : undefined, depth, ...(shows && { view: { path: d.path as string, kind: shows } }) };
    });
  if (!placed.length) return { w: 0, h: 0, boxes: [], lines: [] };
  const x0 = Math.min(...placed.map((b) => b.x)) - pad, y0 = Math.min(...placed.map((b) => b.y)) - pad;
  const boxes = placed
    .map((b) => ({ ...b, x: b.x - x0, y: b.y - y0 }))
    // areas under what they hold, outer ones first; the rest in the order given
    .map((b, i) => ({ b, i }))
    .sort((p, q) => Number(q.b.kind === "area") - Number(p.b.kind === "area") || (p.b.kind === "area" ? p.b.depth - q.b.depth : 0) || p.i - q.i)
    .map((p) => p.b);
  const at = new Map(boxes.map((b) => [b.id, b]));
  const lines = edges.flatMap((e) => {
    const a = at.get(e.source), b = at.get(e.target);
    if (!a || !b || a === b) return [];
    const p = edgeOf(a, { x: b.x + b.w / 2, y: b.y + b.h / 2 }), q = edgeOf(b, { x: a.x + a.w / 2, y: a.y + a.h / 2 });
    return [{ x1: p.x, y1: p.y, x2: q.x, y2: q.y }];
  });
  return { w: Math.max(...boxes.map((b) => b.x + b.w)) + pad, h: Math.max(...boxes.map((b) => b.y + b.h)) + pad, boxes, lines };
}

/**
 * A name cut to fit a width in the picture, with … when it doesn't: Latin letters count as a bit over half the font
 * size wide, Korean and other wide letters as the whole of it.
 */
export function fitText(text: string, width: number, fontSize: number): string {
  let used = 0;
  const out: string[] = [];
  for (const ch of text) {
    const w = (ch.codePointAt(0)! > 0x2e7f ? 1 : 0.58) * fontSize;
    if (used + w > width) return out.length ? out.slice(0, Math.max(1, out.length - 1)).join("") + "…" : "";
    used += w;
    out.push(ch);
  }
  return text;
}
