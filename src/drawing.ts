import { GRID } from "./nodes/AgentNode";
import { ARROW } from "./Canvas";
import { BLOCK_SIZE, ZONE_METRICS, ZONE_SIZE, ZONE_Z } from "./nodes/StructNodes";
import { commonPhased, goPhase, openSize, phaseLabel, phasesOf, removePhase, rowsOf, tidyArea, withOpenSize, type PhaseData } from "./zones";
import type { RelColor } from "./relationModel";
import { QUESTION_WIDTH } from "./nodes/QuestionBlock";
import { VIEW_SIZE } from "./nodes/ViewBlock";
import type { CanvasEdge, CanvasNode } from "./types";
import { commonParent, flowOfLayout, sidesFor } from "./lineSides";
import { arrange, freeSpot, repack, wrapWidth, type Arranged, type Layout, type Rect } from "./drawLayout";

/** What an agent draws with `maestro draw` (checked by the backend: every name it points to exists in it). */
export type Drawing = {
  /** how the top-level areas and blocks are placed (drawLayout.ts); none: in a row, as before */
  layout?: Layout;
  /** `layout`: how the area places what it holds; none: grid */
  /** `color`: the border colour, one of the line colours (the backend refuses any other) */
  /**
   * `steps` (a count, or names) or `states` (names): the area has steps or states (zones.md), never both. `step`
   * (numbers from 1) / `state` (names) on what it holds, an inner area or an arrow: it exists only in those (the
   * backend checked them against the area).
   */
  /** `stack`: the area is a stack (zones.md "묶음"): what it holds sits in one column, in the order given */
  areas: ({ title: string; in?: string; layout?: Layout; color?: RelColor; steps?: number | string[]; states?: string[]; stack?: boolean } & In)[];
  blocks: ({ title: string; body?: string; area?: string; color?: RelColor } & In)[];
  /** `relation`: the relation id the backend put for the arrow's `as` (declared there if it was new) */
  arrows: ({ from: string; to: string; label?: string; as?: string; relation?: string; body?: string } & In)[];
};
type In = { step?: number | number[]; state?: string | string[] };
type Drawn = { mode: "step" | "state"; names: string[] };

const PAD = GRID, TOP = 2 * GRID, GAP = GRID, ROOT_GAP = 2 * GRID, RANK_GAP = 3 * GRID;
// Placement (structure-drawing.md "배치 규칙"): nothing an agent puts on the canvas runs on sideways. Inside an area
// a row is about four blocks wide at most, on the canvas itself about two screens; past that things go on underneath.
// New things keep `APART` from what's there.
const AREA_WRAP = 50 * GRID, PAGE_WRAP = 96 * GRID, APART = 4 * GRID;
/** What's on the canvas at the top level, as boxes: a folded area at the size it opens to, a block not measured yet at a guess. */
const taken = (ns: CanvasNode[]): Rect[] =>
  ns.filter((n) => !n.parentId).map((n) => {
    const guess = n.type === "question_block" ? { width: QUESTION_WIDTH, height: 10 * GRID } : BLOCK_SIZE;
    const s = n.type === "zone" ? openSize(n, ZONE_METRICS) : { width: n.width ?? n.measured?.width ?? guess.width, height: n.height ?? n.measured?.height ?? guess.height };
    return { x: n.position.x, y: n.position.y, w: s.width, h: s.height };
  });
/**
 * Where something new of this size goes on the canvas: the first free place from the top and from the left
 * (drawLayout.ts `freeSpot`). The canvas is filled no wider than `PAGE_WRAP`, or than 1.6 times its height once
 * it is taller than that gives (so a full canvas stays roughly screen-shaped instead of becoming one long strip
 * either way).
 */
const newPlace = (ns: CanvasNode[], box: { w: number; h: number }) => {
  const there = taken(ns);
  const tall = there.length ? Math.max(...there.map((r) => r.y + r.h)) - Math.min(...there.map((r) => r.y)) : 0;
  return freeSpot(there, box, APART, Math.max(PAGE_WRAP, Math.round(1.6 * tall)), GRID);
};
type Item = { kind: "area"; title: string; parent?: string; layout?: Layout; color?: RelColor; phases?: Drawn; stack?: boolean } | { kind: "block"; title: string; body?: string; parent?: string; color?: RelColor };
const many = <T,>(v: T | T[] | undefined): T[] => (v == null ? [] : Array.isArray(v) ? v : [v]);
/** The steps or states a drawing gives an area. */
const drawnPhases = (a: Drawing["areas"][number]): Drawn | undefined =>
  a.steps != null ? { mode: "step", names: typeof a.steps === "number" ? Array.from({ length: a.steps }, () => "") : a.steps } : a.states?.length ? { mode: "state", names: a.states } : undefined;
/** The phase ids of `zone` a drawing's `step` / `state` names (none when the area has no such steps or states). */
const whenIn = (zone: CanvasNode | undefined, x: In): string[] | undefined => {
  const ps = phasesOf(zone);
  if (!ps.length) return undefined;
  const step = (zone!.data as PhaseData).mode === "step";
  const ids = step ? many(x.step).map((n) => ps[n - 1]?.id) : many(x.state).map((s) => ps.find((p) => phaseLabel(zone, p.id) === s.trim())?.id);
  const got = ids.filter((id): id is string => !!id);
  return got.length ? got : undefined;
};
type Size = { w: number; h: number };

/**
 * The canvas after a drawing: blocks and areas already there (same name) are updated in place, new ones are laid
 * out by the drawing's intent (each area's `layout` for what it holds, the top `layout` for the top level, in the
 * first free place on the canvas), and arrows are added or relabelled.
 */
export function applyDrawing(d: Drawing, ns: CanvasNode[], es: CanvasEdge[]): { nodes: CanvasNode[]; edges: CanvasEdge[] } {
  const stamp = Date.now();
  let seq = 0;
  const newId = (p: string) => `${p}_${stamp}_${seq++}`;
  const drawn = (n: CanvasNode) => n.type === "struct_block" || n.type === "zone";
  const title = (n: CanvasNode) => ((n.data as { title?: string }).title ?? "").trim();
  const byTitle = new Map<string, CanvasNode>();
  for (const n of ns) if (drawn(n) && !byTitle.has(title(n))) byTitle.set(title(n), n);

  const phase = (name: string) => ({ id: newId("ph"), name });
  // steps or states for an area that's already there: a plain one gets them; one that has them keeps its own and
  // only gets the ones it lacks (more steps at the end, states by a name it doesn't have); the other kind is left
  const withPhases = (data: PhaseData & Record<string, unknown>, dp: Drawn) => {
    if (!data.mode) {
      const phases = dp.names.map(phase);
      return { ...data, mode: dp.mode, phases, at: phases[0].id };
    }
    if (data.mode !== dp.mode) return data;
    const have = data.phases ?? [];
    const more = dp.mode === "step" ? dp.names.slice(have.length) : dp.names.filter((n) => !have.some((p) => p.name.trim() === n.trim()));
    return more.length ? { ...data, phases: [...have, ...more.map(phase)] } : data;
  };
  // what's there already: a block gets its new notes, a block or area the border colour it is given, an area its
  // steps or states
  let nodes = ns.map((n) => {
    const b = n.type === "struct_block" ? d.blocks.find((x) => x.title.trim() === title(n)) : n.type === "zone" ? d.areas.find((x) => x.title.trim() === title(n)) : undefined;
    if (!b) return n;
    const body = n.type === "struct_block" ? (b as Drawing["blocks"][number]).body : undefined;
    const dp = n.type === "zone" ? drawnPhases(b as Drawing["areas"][number]) : undefined;
    const stack = n.type === "zone" && !!(b as Drawing["areas"][number]).stack;
    const layout = n.type === "zone" ? (b as Drawing["areas"][number]).layout : undefined;
    if (body == null && !b.color && !dp && !stack && !layout) return n;
    const data = { ...n.data, ...(body != null && { body }), ...(b.color && { color: b.color }), ...(stack && { stack: true }), ...(layout && { layout }) };
    return { ...n, data: dp ? withPhases(data as PhaseData & Record<string, unknown>, dp) : data } as CanvasNode;
  });

  const items: Item[] = [
    ...d.areas.filter((a) => !byTitle.has(a.title.trim())).map((a): Item => ({ kind: "area", title: a.title.trim(), parent: a.in?.trim() || undefined, layout: a.layout, color: a.color, phases: drawnPhases(a), stack: a.stack || undefined })),
    ...d.blocks.filter((b) => !byTitle.has(b.title.trim())).map((b): Item => ({ kind: "block", title: b.title.trim(), body: b.body, parent: b.area?.trim() || undefined, color: b.color })),
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
    const sizes = list.map((i) => measure(i, depth + 1));
    // (the width its rows, flow or tree wrap at: wider when it holds big areas, so two of them still sit side by side)
    const wrap = wrapWidth(container === undefined ? PAGE_WRAP : AREA_WRAP, sizes, RANK_GAP);
    const p = arrange(list.map((_, n) => String(n)), sizes, links, layout, container === undefined ? ROOT_GAP : GAP, RANK_GAP, wrap);
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
        ? ({ id, type: "zone", position: { x, y }, width: s.w, height: s.h, zIndex: ZONE_Z, parentId, data: i.phases ? withPhases({ title: i.title, color: i.color, layout: i.layout }, i.phases) : { title: i.title, color: i.color, layout: i.layout, ...(i.stack && { stack: true }) } } as CanvasNode)
        : ({ id, type: "struct_block", position: { x, y }, width: s.w, height: s.h, parentId, data: { title: i.title, body: i.body, color: i.color } } as CanvasNode),
    );
    if (i.kind !== "area" || depth > 20) return;
    const p = plan(i.title);
    kids(i.title).forEach((k, j) => place(k, PAD + p.at[j].x, TOP + p.at[j].y, id, depth + 1));
  };

  // new things inside an area that's already on the canvas: under what it holds, in rows of at most three (as a
  // new area's grid), and the area grows around them. A stack takes them one under another (zones.ts lays it out).
  // A folded area (zones.md) stays folded: it grows at the size it opens back to, and its chip counts the new blocks
  const added = new Map<string, Item[]>();
  for (const i of items) if (i.parent && byTitle.has(i.parent)) added.set(i.parent, [...(added.get(i.parent) ?? []), i]);
  for (const [parent, list] of added) {
    const host = nodes.find((n) => n.id === byTitle.get(parent)!.id)!;
    const open = openSize(host, ZONE_METRICS);
    const sizes = list.map((i) => measure(i));
    const stack = !!(host.data as { stack?: boolean }).stack;
    const p = arrange(list.map((_, n) => String(n)), sizes, [], stack ? "row" : "grid", GAP, RANK_GAP, stack ? 0 : Math.max(open.width - 2 * PAD, wrapWidth(AREA_WRAP, sizes, GAP)));
    list.forEach((i, j) => place(i, PAD + p.at[j].x, open.height - PAD + GAP + p.at[j].y, host.id));
    nodes = nodes.map((n) => (n.id === host.id ? withOpenSize(n, { height: open.height + p.h + GAP, width: Math.max(open.width, p.w + 2 * PAD) }) : n));
  }
  // the rest of the top level, laid out as the top layout says: into the first free place on the canvas, from the
  // top and from the left (a folded area counted at its open size, so it can open without landing on the drawing)
  const tops = kids(undefined);
  if (tops.length) {
    const p = plan(undefined);
    const o = newPlace(ns, p);
    tops.forEach((i, j) => place(i, o.x + p.at[j].x, o.y + p.at[j].y));
  }
  nodes = [...nodes, ...created];

  // the steps or states a block or inner area exists in: read against the area it is in on the canvas (the one just
  // drawn, or the one it was in already), now that every area has its steps or states
  const placed = new Map(nodes.map((n) => [n.id, n]));
  const whens = new Map<string, string[]>();
  for (const x of [...d.areas, ...d.blocks]) {
    if (x.step == null && x.state == null) continue;
    const n = placed.get(ids.get(x.title.trim()) ?? "");
    const when = n?.parentId ? whenIn(placed.get(n.parentId), x) : undefined;
    if (n && when) whens.set(n.id, when);
  }
  if (whens.size) nodes = nodes.map((n) => (whens.has(n.id) ? ({ ...n, data: { ...n.data, when: whens.get(n.id) } } as CanvasNode) : n));

  // An arrow keeps to the way of the layout that holds both its ends (lineSides.ts): in a tree every child is joined
  // from its parent's bottom to its own top, however far out to the side it sits; in a flow from the right side to
  // the left. With no such layout (a grid, a row) it takes the sides facing each other. Places are summed up through
  // the areas
  const byId = new Map(nodes.map((n) => [n.id, n]));
  const boxOf = (id: string) => {
    let x = 0, y = 0;
    for (let n = byId.get(id), k = 0; n && k < 50; n = n.parentId ? byId.get(n.parentId) : undefined, k++) {
      x += n.position.x;
      y += n.position.y;
    }
    const n = byId.get(id);
    return { x, y, w: n?.width ?? 0, h: n?.height ?? 0 };
  };
  const drawnNow = new Set(created.map((c) => c.id));
  const topOf = (id: string) => {
    let n = byId.get(id);
    for (let k = 0; n?.parentId && k < 50; k++) n = byId.get(n.parentId);
    return n?.id ?? id;
  };
  // (the layout that holds two ends: the area both are in; at the top level, this drawing's own when both were drawn by it)
  const layoutFor = (source: string, target: string) => {
    const holder = commonParent(nodes, source, target);
    if (holder) return (byId.get(holder)?.data as { layout?: string } | undefined)?.layout;
    return drawnNow.has(topOf(source)) && drawnNow.has(topOf(target)) ? d.layout : undefined;
  };
  let edges = es;
  for (const a of d.arrows) {
    const source = ids.get(a.from.trim()), target = ids.get(a.to.trim());
    if (!source || !target || source === target) continue;
    const label = a.label?.trim() || undefined;
    const body = a.body?.trim() || undefined;
    const relation = a.relation && a.relation !== "plain" ? a.relation : undefined;
    // an arrow limited to some steps or states: read against the area both its ends are in
    const zid = a.step != null || a.state != null ? commonPhased(nodes, source, target) : undefined;
    const when = zid ? whenIn(nodes.find((n) => n.id === zid), a) : undefined;
    const limit = when ? { when, whenZone: zid } : {};
    const old = edges.find((e) => e.data?.kind === "arrow" && e.source === source && e.target === target);
    if (old) {
      edges = edges.map((e) => (e === old ? { ...e, data: { ...e.data!, label, ...(body != null && { body }), ...(a.relation != null && { relation }), ...limit } } : e));
      continue;
    }
    const flow = flowOfLayout(layoutFor(source, target));
    const [sourceHandle, targetHandle] = sidesFor(boxOf(source), boxOf(target), flow);
    edges = [...edges, { id: newId("arrow"), type: "arrow", source, target, sourceHandle, targetHandle, markerEnd: ARROW, data: { kind: "arrow", label, body, relation, ...(flow && { flow }), ...limit } }];
  }
  return { nodes, edges };
}

/** `maestro step` / `maestro state`: an area (by name) goes to a step (a number from 1, "next", "prev") or a state (its name). */
export type PhaseMove = { area: string; to: number | string };

/** The canvas after such a move; unchanged when the area or the step isn't there (the backend checked, the canvas may have changed since). */
export function applyPhaseMove(p: PhaseMove, ns: CanvasNode[]): CanvasNode[] {
  const zone = ns.find((n) => n.type === "zone" && (n.data.title ?? "").trim() === p.area.trim() && phasesOf(n).length > 0);
  if (!zone) return ns;
  const ps = phasesOf(zone);
  const to = p.to === "next" || p.to === "prev" ? p.to : typeof p.to === "number" ? ps[p.to - 1]?.id : ps.find((x) => phaseLabel(zone, x.id) === String(p.to).trim())?.id;
  return to ? goPhase(ns, zone.id, to) : ns;
}

/** A question an agent asks with `maestro ask` (checked by the backend). */
export type Ask = { id: string; title: string; body?: string; choices: string[]; multi: boolean; other: boolean };

/**
 * The canvas with question `q` on it, asked by the agent `owner`: in the first free place on the canvas (where a
 * new drawing would go). Asked twice (the same id), it is there once.
 */
export function applyAsk(q: Ask, owner: string, ns: CanvasNode[]): CanvasNode[] {
  if (ns.some((n) => n.type === "question_block" && n.data.qid === q.id)) return ns;
  const data = { qid: q.id, owner, title: q.title, body: q.body, choices: q.choices.length ? q.choices : undefined, multi: q.multi || undefined, other: q.other || undefined };
  return [...ns, { id: `question_${q.id}`, type: "question_block", position: newPlace(ns, askBox(q)), data } as CanvasNode];
}
/** About how big a question block will be (it is measured once drawn): enough to find it a free place. */
const askBox = (q: Ask) => ({ w: QUESTION_WIDTH, h: (6 + (q.body ? 2 : 0) + 2 * q.choices.length + (q.other || !q.choices.length ? 4 : 0)) * GRID });

/** A file an agent shows with `maestro show` (checked by the backend: it is there, an image or a page). */
export type Show = { id: string; path: string; kind: "image" | "html"; title?: string };

/**
 * The canvas with the file on it as a view block (view-block.md): where a new drawing would go (the first free
 * place on the canvas). A file already shown stays one block: it takes the new title and reads the file again.
 */
export function applyShow(v: Show, ns: CanvasNode[]): CanvasNode[] {
  const there = ns.find((n) => n.type === "view_block" && n.data.path.toLowerCase() === v.path.toLowerCase());
  if (there) return ns.map((n) => (n === there && n.type === "view_block" ? { ...n, data: { ...n.data, kind: v.kind, title: v.title ?? n.data.title, at: Date.now() } } : n));
  return [...ns, { id: v.id, type: "view_block", position: newPlace(ns, { w: VIEW_SIZE.width, h: VIEW_SIZE.height }), ...VIEW_SIZE, data: { path: v.path, kind: v.kind, title: v.title } } as CanvasNode];
}

/** Several questions asked together with `maestro ask --file` (checked by the backend). */
export type AskSet = { id: string; title?: string; questions: Ask[] };

/**
 * The canvas with a question set on it (question-block.md "질문 묶음"): an area with a step per question, each
 * question in its own step at the same place, so the area shows one at a time and its head says "2 / 3". Where a new
 * drawing would go. Asked twice (the same id), it is there once.
 */
export function applyAskSet(set: AskSet, owner: string, ns: CanvasNode[]): CanvasNode[] {
  if (!set.questions.length || ns.some((n) => n.type === "zone" && n.data.qset === set.id)) return ns;
  const phases = set.questions.map((q) => ({ id: `ph_${q.id}`, name: "" }));
  // (a free place big enough for its tallest question, as far as that can be told before they are drawn)
  const size = { w: QUESTION_WIDTH + 2 * PAD, h: TOP + Math.max(...set.questions.map((q) => askBox(q).h)) + PAD };
  const zone = {
    id: `zone_${set.id}`,
    type: "zone",
    position: newPlace(ns, size),
    // as wide as a question, and as low as an area gets: it takes the height of the question showing once that is
    // measured, and is fitted to the next one at every step (zones.ts `settleZones`)
    width: QUESTION_WIDTH + 2 * PAD,
    height: ZONE_METRICS.min.height,
    zIndex: ZONE_Z,
    data: { title: set.title?.trim() || `질문 ${set.questions.length}개`, mode: "step", phases, at: phases[0].id, qset: set.id },
  } as CanvasNode;
  const blocks = set.questions.map((q, i) => {
    const data = { qid: q.id, owner, title: q.title, body: q.body, choices: q.choices.length ? q.choices : undefined, multi: q.multi || undefined, other: q.other || undefined, set: set.id, when: [phases[i].id] };
    return { id: `question_${q.id}`, type: "question_block", parentId: zone.id, position: { x: PAD, y: TOP }, data } as CanvasNode;
  });
  return [...ns, zone, ...blocks];
}

/**
 * What an agent erases with `maestro erase` (names checked by the backend against the canvas as last saved).
 * `questions`: question blocks by their id (`maestro erase --question`: an agent clears a question once it has used
 * the answer); `sets`: question sets by their id (their area goes with their questions).
 */
export type Erase = { all?: boolean; areas?: string[]; blocks?: string[]; arrows?: { from: string; to: string }[]; questions?: string[]; sets?: string[]; /** view blocks, by node id (`maestro erase --view`) */ views?: string[] };

/**
 * The canvas after an erase: the named blocks and areas go, with every line touching them, and so do the named
 * arrows, question blocks and view blocks. `all` takes everything an agent can put on the canvas: every drawn block,
 * area and arrow, every question block (answered or still waiting, sets with their area) and every view block. What
 * an erased area held stays where it is on screen, one level up (as deleting an area by hand). Agents, documents and
 * prompt blocks are never touched.
 */
export function applyErase(x: Erase, ns: CanvasNode[], es: CanvasEdge[]): { nodes: CanvasNode[]; edges: CanvasEdge[] } {
  const title = (n: CanvasNode) => ((n.data as { title?: string }).title ?? "").trim();
  const named = (list: string[] | undefined, type: string) => new Set(ns.filter((n) => n.type === type && (list ?? []).some((t) => t.trim() === title(n))).map((n) => n.id));
  const asked = ns.filter((n) => (n.type === "question_block" && (x.questions ?? []).includes(n.data.qid)) || (n.type === "zone" && !!n.data.qset && (x.sets ?? []).includes(n.data.qset))).map((n) => n.id);
  // (the agents' area is the app's, not something drawn: `all` leaves it)
  const gone = x.all
    ? new Set(ns.filter((n) => n.type === "struct_block" || n.type === "question_block" || n.type === "view_block" || (n.type === "zone" && !n.data.team)).map((n) => n.id))
    : new Set([...named(x.blocks, "struct_block"), ...named(x.areas, "zone"), ...asked, ...ns.filter((n) => n.type === "view_block" && (x.views ?? []).includes(n.id)).map((n) => n.id)]);
  const byId = new Map(ns.map((n) => [n.id, n]));
  // a question set is one thing: its area erased by name takes its questions, and the area goes with its last question
  const setOf = (z: CanvasNode) => (z.type === "zone" && z.data.qset ? ns.filter((q) => q.type === "question_block" && q.parentId === z.id && q.data.set === z.data.qset) : []);
  for (const z of ns) {
    const qs = setOf(z);
    if (gone.has(z.id)) for (const q of qs) gone.add(q.id);
    else if (qs.length && qs.every((q) => gone.has(q.id))) gone.add(z.id);
  }
  const abs = (id: string) => {
    let px = 0, py = 0;
    for (let n = byId.get(id), k = 0; n && k < 50; n = n.parentId ? byId.get(n.parentId) : undefined, k++) {
      px += n.position.x;
      py += n.position.y;
    }
    return { x: px, y: py };
  };
  let nodes = ns
    .filter((n) => !gone.has(n.id))
    .map((n) => {
      if (!n.parentId || !gone.has(n.parentId)) return n;
      let p: string | undefined = n.parentId;
      for (let k = 0; p && gone.has(p) && k < 50; k++) p = byId.get(p)?.parentId;
      const a = abs(n.id), base = p ? abs(p) : { x: 0, y: 0 };
      return { ...n, parentId: p, position: { x: a.x - base.x, y: a.y - base.y } } as CanvasNode;
    });
  // one question erased out of a set: its step goes with it, so the set's area doesn't keep an empty one
  for (const z of ns) {
    if (gone.has(z.id)) continue;
    for (const q of setOf(z)) if (gone.has(q.id)) for (const p of (q.data as { when?: string[] }).when ?? []) nodes = removePhase(nodes, z.id, p);
  }
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

/**
 * 배치 정리 (structure-drawing.md): the layout put straight once, for a canvas drawn bit by bit. Every plain area,
 * innermost first, has what it holds set in aligned rows with its structure blocks made one size (zones.ts
 * `tidyArea`); then what was drawn at the top level (areas, blocks, questions, view blocks, documents) is packed
 * again in reading order (drawLayout.ts `repack`). Agents, prompt blocks and the agents' area stay where they are,
 * and so does what stacks, areas with steps or states and folded areas hold. `area`: only that area (by name, as
 * an agent gives it, or by id) and the areas in it; the top level is left alone. Unknown area: nothing changes.
 */
export function tidyCanvas(ns: CanvasNode[], area?: string | { id: string }): CanvasNode[] {
  const byId = new Map(ns.map((n) => [n.id, n]));
  const depth = (n: CanvasNode) => {
    let d = 0;
    for (let p = n.parentId; p && d < 50; p = byId.get(p)?.parentId) d++;
    return d;
  };
  const root = area ? ns.find((n) => n.type === "zone" && !n.data.team && (typeof area === "string" ? (n.data.title ?? "").trim() === area.trim() : n.id === area.id)) : undefined;
  if (area && !root) return ns;
  const within = (n: CanvasNode) => {
    for (let c: CanvasNode | undefined = n, k = 0; c && k < 50; c = c.parentId ? byId.get(c.parentId) : undefined, k++) if (c.id === root?.id) return true;
    return false;
  };
  const zones = ns.filter((n) => n.type === "zone" && !n.data.team && (!root || within(n))).sort((a, b) => depth(b) - depth(a));
  let next = ns;
  for (const z of zones) next = tidyArea(next, z.id, ZONE_METRICS, GAP, (n) => n.type === "struct_block");
  if (root) return next;
  const drawn = (n: CanvasNode) => !n.parentId && (n.type === "struct_block" || n.type === "markdown_block" || n.type === "question_block" || n.type === "view_block" || (n.type === "zone" && !n.data.team));
  const tops = taken(next), top = next.filter((n) => !n.parentId);
  const movers = top.flatMap((n, i) => (drawn(n) ? [{ n, box: tops[i] }] : []));
  if (!movers.length) return next;
  const boxes = movers.map((x) => x.box);
  const at = repack(boxes, rowsOf(boxes).flat(), top.flatMap((n, i) => (drawn(n) ? [] : [tops[i]])), APART, PAGE_WRAP, GRID);
  const placed = new Map(movers.map((x, i) => [x.n.id, at[i]]));
  return next.map((n) => {
    const p = placed.get(n.id);
    return p && (p.x !== n.position.x || p.y !== n.position.y) ? ({ ...n, position: p } as CanvasNode) : n;
  });
}
