// Where the canvas looks after its agent changed it (src/canvasFocus.ts; docs/features/canvas/canvas-controls.md
// "화면이 따라가기"). Run: `npm test`.
import { deepStrictEqual, equal, ok } from "node:assert/strict";
import { boundsOf, changed, focusPlan, inView, planFor, shows, structure, toward, viewFor, worthMoving, type FEdge, type Sheet } from "../src/canvasFocus.ts";
import { holdMs, stepsOf } from "../src/canvasGuide.ts";
import { fitText, miniOf } from "../src/canvasMini.ts";
import { brings, chatView, namesOf, visualOf, type Gate, type Queued } from "../src/visualSteps.ts";
import type { ZNode } from "../src/zones.ts";

const zone = (id: string, x: number, y: number, width: number, height: number, parentId?: string): ZNode => ({ id, type: "zone", parentId, position: { x, y }, width, height, data: { title: id } });
const block = (id: string, parentId: string | undefined, x: number, y: number): ZNode => ({ id, type: "struct_block", parentId, position: { x, y }, width: 240, height: 72, data: { title: id } });
const line = (id: string, source: string, target: string, kind = "arrow"): FEdge => ({ id, source, target, data: { kind } });
const size = (n: ZNode) => ({ w: n.width ?? 240, h: n.height ?? 72 });

// two areas joined by an arrow between their blocks, an area on its own, and a document with a reference line
const base: Sheet = {
  nodes: [zone("A", 0, 0, 600, 300), block("a1", "A", 24, 48), zone("B", 700, 0, 600, 300), block("b1", "B", 24, 48), zone("C", 0, 1000, 400, 300), block("c1", "C", 24, 48), block("doc", undefined, 2000, 0)],
  edges: [line("e1", "a1", "b1"), line("r1", "doc", "a1", "reference")],
};
const withNodes = (...more: ZNode[]): Sheet => ({ nodes: [...base.nodes, ...more], edges: base.edges });

// ---- what a change brought
deepStrictEqual(changed(base, base), { fresh: [], touched: [] });
deepStrictEqual(changed(base, withNodes(block("a2", "A", 288, 48))), { fresh: ["a2"], touched: [] });
// a block whose notes changed (its data is another object), a new line between things that were there
const edited: Sheet = { nodes: base.nodes.map((n) => (n.id === "a1" ? { ...n, data: { ...n.data, body: "새 메모" } } : n)), edges: base.edges };
deepStrictEqual(changed(base, edited), { fresh: [], touched: ["a1"] });
const joined: Sheet = { nodes: base.nodes, edges: [...base.edges, line("e2", "c1", "doc")] };
deepStrictEqual(changed(base, joined), { fresh: [], touched: ["c1", "doc"] });
// something erased: nothing to look at
const erased: Sheet = { nodes: base.nodes.filter((n) => n.id !== "c1"), edges: base.edges };
deepStrictEqual(changed(base, erased), { fresh: [], touched: [] });
equal(focusPlan(base, erased, size), null);
equal(focusPlan(base, base, size), null);

// ---- the structure things belong to: the outermost areas they are in, and what arrows join to those
deepStrictEqual([...structure(base, ["a1"])].sort(), ["A", "B"]);
deepStrictEqual([...structure(base, ["c1"])], ["C"]);
// a line that isn't an arrow (a document an agent refers to) joins nothing
deepStrictEqual([...structure(base, ["doc"])], ["doc"]);
// arrows are followed however far
const chain: Sheet = { nodes: base.nodes, edges: [...base.edges, line("e3", "b1", "c1")] };
deepStrictEqual([...structure(chain, ["a1"])].sort(), ["A", "B", "C"]);
deepStrictEqual([...structure(base, ["nope"])], []);

// ---- boxes on the canvas: places summed up through the areas
deepStrictEqual(boundsOf(base.nodes, ["a1"], size), { x: 24, y: 48, w: 240, h: 72 });
deepStrictEqual(boundsOf(base.nodes, ["A", "B"], size), { x: 0, y: 0, w: 1300, h: 300 });
equal(boundsOf(base.nodes, [], size), null);

// ---- what to look at
// a block drawn into an area that is part of a structure: the block first, then the whole structure
deepStrictEqual(focusPlan(base, withNodes(block("a2", "A", 288, 48)), size), { first: { x: 288, y: 48, w: 240, h: 72 }, then: { x: 0, y: 0, w: 1300, h: 300 }, marks: ["a2"], fresh: true });
// into an area on its own: the block, then that area
deepStrictEqual(focusPlan(base, withNodes(block("c2", "C", 24, 144)), size), { first: { x: 24, y: 1144, w: 240, h: 72 }, then: { x: 0, y: 1000, w: 400, h: 300 }, marks: ["c2"], fresh: true });
// a new drawing of its own (an area with its blocks): the drawing, and nothing after it: not the whole canvas
const fresh: Sheet = { nodes: [...base.nodes, zone("N", 3000, 0, 600, 300), block("n1", "N", 24, 48), block("n2", "N", 336, 48)], edges: [...base.edges, line("e9", "n1", "n2")] };
// (what is pointed out is the new area, not each block in it)
deepStrictEqual(focusPlan(base, fresh, size), { first: { x: 3000, y: 0, w: 600, h: 300 }, then: null, marks: ["N"], fresh: true });
// a question (or a view block) alone at the top level: just that
deepStrictEqual(focusPlan(base, withNodes({ ...block("q", undefined, 1500, 600), type: "question_block" }), size), { first: { x: 1500, y: 600, w: 240, h: 72 }, then: null, marks: ["q"], fresh: true });
// nothing new, but something changed: that, then its structure
deepStrictEqual(focusPlan(base, edited, size), { first: { x: 24, y: 48, w: 240, h: 72 }, then: { x: 0, y: 0, w: 1300, h: 300 }, marks: ["a1"], fresh: false });
// new things win over changed ones
const both: Sheet = { nodes: [...edited.nodes, block("c2", "C", 24, 144)], edges: edited.edges };
deepStrictEqual(focusPlan(base, both, size)?.first, { x: 24, y: 1144, w: 240, h: 72 });
// what doesn't show (drawn into a folded area) is looked at through what stands for it
const folded = withNodes(block("c2", "C", 24, 144));
deepStrictEqual(focusPlan(base, folded, size, new Set(["c1", "c2"])), { first: { x: 0, y: 1000, w: 400, h: 300 }, then: null, marks: ["C"], fresh: true });

// ---- the view for a box
const pane = { w: 1000, h: 800 }, opts = { padding: 0.15, minZoom: 0.15, maxZoom: 1 };
// a small block isn't blown up: 100%, in the middle
deepStrictEqual(viewFor({ x: 0, y: 0, w: 240, h: 72 }, pane, opts), { x: 380, y: 364, zoom: 1 });
// something that fits at 50%: as large as fits with the padding kept free
deepStrictEqual(viewFor({ x: 0, y: 0, w: 1400, h: 400 }, pane, opts), { x: 150, y: 300, zoom: 0.5 });
// something huge: no further out than the canvas goes
deepStrictEqual(viewFor({ x: 0, y: 0, w: 7000, h: 2800 }, pane, opts), { x: -25, y: 190, zoom: 0.15 });

// ---- whether it is in view, and whether moving is worth it
const at = { x: 0, y: 0, zoom: 1 };
ok(shows(at, pane, { x: 100, y: 100, w: 200, h: 100 }));
ok(!shows(at, pane, { x: 900, y: 100, w: 200, h: 100 }));
ok(shows({ x: 0, y: 0, zoom: 0.5 }, pane, { x: 900, y: 100, w: 200, h: 100 }));
const small = { x: 100, y: 100, w: 240, h: 72 };
// in view at a size like the one the move would give: nothing moves
ok(!worthMoving(at, viewFor(small, pane, opts), pane, small));
ok(!worthMoving({ x: 0, y: 0, zoom: 0.8 }, viewFor(small, pane, opts), pane, small));
// in view but far out: it comes in; out of view: it goes there
ok(worthMoving({ x: 0, y: 0, zoom: 0.3 }, viewFor(small, pane, opts), pane, small));
ok(worthMoving({ x: -5000, y: 0, zoom: 1 }, viewFor(small, pane, opts), pane, small));

// ---- how much of a box is in view (for telling what the user hasn't seen yet), and which way it lies
equal(inView(at, pane, { x: 100, y: 100, w: 200, h: 100 }), 1);
equal(inView(at, pane, { x: 2000, y: 100, w: 200, h: 100 }), 0);
equal(inView(at, pane, { x: 900, y: 100, w: 200, h: 100 }), 0.5);
// something bigger than the view that fills it counts as seen
equal(inView(at, pane, { x: -1000, y: -1000, w: 5000, h: 5000 }), 1);
equal(toward(at, pane, { x: 3000, y: 350, w: 100, h: 100 }), 0);
equal(toward(at, pane, { x: 450, y: 3000, w: 100, h: 100 }), 90);
equal(toward(at, pane, { x: -3000, y: 350, w: 100, h: 100 }), 180);
equal(toward(at, pane, { x: 450, y: -3000, w: 100, h: 100 }), 270);

// ---- nodes that are named (a step's, or `maestro focus`), and what leads among them
deepStrictEqual(planFor(base, ["c1"], size), { first: { x: 24, y: 1048, w: 240, h: 72 }, then: { x: 0, y: 1000, w: 400, h: 300 }, marks: ["c1"] });
equal(planFor(base, ["nope"], size), null);
equal(planFor(base, [], size), null);
// a question among what came is looked at first; the rest of it comes with the wider look
const asked = withNodes(block("c2", "C", 24, 144), { ...block("q", undefined, 1500, 1000), type: "question_block" });
deepStrictEqual(planFor(asked, ["c2", "q"], size, new Set(), (n) => n.type === "question_block"), { first: { x: 1500, y: 1000, w: 240, h: 72 }, then: { x: 0, y: 1000, w: 1740, h: 300 }, marks: ["c2", "q"] });
// only questions: nothing to put them before
deepStrictEqual(planFor(asked, ["q"], size, new Set(), (n) => n.type === "question_block")?.first, { x: 1500, y: 1000, w: 240, h: 72 });

// ---- the queue cut into steps: a thing with a line starts one, what follows without a line belongs to it
deepStrictEqual(stepsOf([]), []);
deepStrictEqual(stepsOf([{ a: 1 }, { a: 2 }]), [{ say: undefined, items: [{ a: 1 }, { a: 2 }] }]);
deepStrictEqual(
  stepsOf([{ a: 1 }, { a: 2, say: " 첫 화면 " }, { a: 3 }, { a: 4, say: "둘째" }, { a: 5, say: "  " }]),
  [{ say: undefined, items: [{ a: 1 }] }, { say: "첫 화면", items: [{ a: 2, say: " 첫 화면 " }, { a: 3 }] }, { say: "둘째", items: [{ a: 4, say: "둘째" }, { a: 5, say: "  " }] }],
);
// a step keeps the canvas for its line's time; things nothing was said about don't hold anything up
equal(holdMs({ say: "설명" }, 4), 4000);
equal(holdMs({}, 4), 0);

// ---- a small picture of a piece of the canvas (for the chat): boxes where they are, lines between their edges
{
  const pic = miniOf(base.nodes.filter((n) => ["A", "a1", "B", "b1"].includes(n.id)), base.edges, size);
  // moved so the whole starts 12 from the corner, areas first
  deepStrictEqual(pic.boxes.map((b) => [b.id, b.kind, b.x, b.y, b.depth]), [["A", "area", 12, 12, 0], ["B", "area", 712, 12, 0], ["a1", "block", 36, 60, 1], ["b1", "block", 736, 60, 1]]);
  deepStrictEqual([pic.w, pic.h], [1324, 324]);
  // the arrow a1 → b1 runs between the facing edges of the two blocks; the line to a document that isn't in the picture is left out
  deepStrictEqual(pic.lines, [{ x1: 276, y1: 96, x2: 736, y2: 96 }]);
  // what doesn't show isn't drawn, and nothing at all gives an empty picture
  deepStrictEqual(miniOf(base.nodes.filter((n) => n.id === "A" || n.id === "a1"), [], size, new Set(["a1"])).boxes.map((b) => b.id), ["A"]);
  deepStrictEqual(miniOf([], [], size), { w: 0, h: 0, boxes: [], lines: [] });
  // a picture of some of them only: still placed by the area they sit in (b1 is right of a1 by the gap between A and B)
  deepStrictEqual(miniOf(base.nodes, base.edges, size, new Set(), 12, new Set(["a1", "b1"])).boxes.map((b) => [b.id, b.x, b.y]), [["a1", 12, 12], ["b1", 712, 12]]);
  // a view block is named by its file
  equal(miniOf([{ id: "v", type: "view_block", position: { x: 0, y: 0 }, data: { path: "C:/w/out/chart.png" } }], [], size).boxes[0].title, "chart.png");
  // names cut to the width: Latin letters narrow, Korean ones wide
  equal(fitText("API", 100, 12), "API");
  equal(fitText("a long block name", 60, 12), "a long …");
  equal(fitText("배포 전에 확인할 점검표", 60, 12), "배포 전…");
  equal(fitText("x", 2, 12), "");
}

// ---- what the chat shows of what an agent puts up for its canvas (chat-visuals.md)
{
  const draw = (seq: number, title: string, say?: string): Queued => ({ seq, say, areas: [{ title }], blocks: [{ title: `${title} 블록`, area: title }] });
  // a step as the chat shows it: its number, whether it brings something, what it points at, what it is in a few words
  const v1 = visualOf("q4", { say: "설계 흐름이에요", items: [draw(4, "설계", "설계 흐름이에요"), { seq: 5, focus: { title: " 설계 " } }] });
  deepStrictEqual([v1.id, v1.brings, v1.points, v1.seqs, v1.what], ["q4", true, ["설계"], [4, 5], "설계 흐름이에요"]);
  // without a line it is named by what it brings (three names, then how many more)
  equal(visualOf("q1", { items: [draw(1, "설계")] }).what, "'설계', '설계 블록'");
  equal(namesOf([draw(1, "가"), draw(2, "나")]), "'가', '가 블록', '나' 외 1개");
  equal(namesOf([{ ask: { id: "q", title: "어느 DB?", choices: [], multi: false, other: false } }, { show: { id: "v", path: "C:/w/out/chart.png", kind: "image" } }, { ask_set: { id: "s", questions: [] } }]), "'어느 DB?', 'chart.png', '질문 0개'");
  // a line on its own, a tidy, a pointer, an erase bring nothing
  deepStrictEqual([{ say: "말" }, { tidy: {} }, { focus: { title: "x" } }, { erase: { all: true } }, { phase: { area: "a", to: 2 } }].map((d) => brings(d as Queued)), [false, false, false, false, false]);
  ok(brings({ arrows: [] }) && brings({ show: { id: "v", path: "a.png", kind: "image" } }));

  const A = "C:\\w\\api";
  const none: Gate = { open: {}, waiting: {}, queued: {}, passes: {}, closed: {} };
  const k = "c:/w/api";
  const s1 = visualOf("s1", { say: "하나", items: [draw(1, "가", "하나")] }), s2 = visualOf("s2", { say: "둘", items: [draw(2, "나", "둘")] });
  const q1 = visualOf("q1", { say: "하나", items: [draw(1, "가", "하나")] }), q2 = visualOf("q2", { say: "말만", items: [{ seq: 2, say: "말만" }] }), q3 = visualOf("q3", { say: "셋", items: [draw(3, "다", "셋")] });
  // nothing waits: nothing shown, whatever the mode
  deepStrictEqual(chatView(none, A, true), { v: null, mode: "ask", more: 0, posted: 0, open: false });
  deepStrictEqual(chatView(none, A, false), { v: null, mode: "record", more: 0, posted: 0, open: false });
  // the canvas open, by hand: the next step it holds back, and how many wait behind it
  const open: Gate = { ...none, open: { [k]: true }, waiting: { [k]: [s1, s2] } };
  deepStrictEqual(chatView(open, A, true), { v: s1, mode: "ask", more: 1, posted: 0, open: true });
  // on its own with the canvas open: the canvas shows it, the chat nothing
  equal(chatView(open, A, false).v, null);
  // the canvas closed, by hand: the first step not let in yet; those let in are counted as put up
  const closed: Gate = { ...none, queued: { [k]: [q1, q2, q3] } };
  deepStrictEqual(chatView(closed, A, true), { v: q1, mode: "ask", more: 2, posted: 0, open: false });
  deepStrictEqual(chatView({ ...closed, passes: { [k]: 2 } }, A, true), { v: q3, mode: "ask", more: 0, posted: 2, open: false });
  deepStrictEqual(chatView({ ...closed, passes: { [k]: 9 } }, A, true), { v: null, mode: "ask", more: 0, posted: 3, open: false });
  // the canvas closed, on its own: the newest thing with a picture (a line on its own has none), as what goes up
  deepStrictEqual(chatView(closed, A, false), { v: q3, mode: "record", more: 1, posted: 0, open: false });
  // closed in the chat without a decision: not shown again, but a thing that comes later is
  equal(chatView({ ...closed, closed: { [k]: 3 } }, A, false).v, null);
  deepStrictEqual(chatView({ ...closed, closed: { [k]: 1 } }, A, false), { v: q3, mode: "record", more: 0, posted: 0, open: false });
  // another agent's chat shows none of it
  equal(chatView(closed, "C:/w/web", true).v, null);
}

console.log("ok focus");
