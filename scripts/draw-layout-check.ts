// Drawing layouts (src/drawLayout.ts; docs/features/canvas/structure-drawing.md). Run: `npm test`.
import { deepStrictEqual, equal, ok } from "node:assert/strict";
import { arrange, freeSpot, layers, repack, wrapWidth } from "../src/drawLayout.ts";

// a chain with a loop back (every canvas4 playbook has a "되돌아감" arrow): the loop doesn't push anything along
deepStrictEqual(layers(["a", "b", "c", "d"], [["a", "b"], ["b", "c"], ["c", "d"], ["d", "b"]]), [["a"], ["b"], ["c"], ["d"]]);
// longest path: d waits for the longer branch
deepStrictEqual(layers(["a", "b", "c", "d"], [["a", "b"], ["b", "c"], ["a", "d"], ["c", "d"]]), [["a"], ["b"], ["c"], ["d"]]);
// fan-out and fan-in, unknown names and self arrows ignored, every id placed once
const fan = layers(["s", "x", "y", "t", "lone"], [["s", "x"], ["s", "y"], ["x", "t"], ["y", "t"], ["x", "x"], ["s", "nowhere"]]);
deepStrictEqual(fan, [["s", "lone"], ["x", "y"], ["t"]]);
// within a step, items follow their predecessors' order (crossings avoided)
deepStrictEqual(layers(["a", "b", "q", "p"], [["a", "p"], ["b", "q"]]), [["a", "b"], ["p", "q"]]);
// a loop with no way in still lands somewhere
equal(layers(["a", "b"], [["a", "b"], ["b", "a"]]).flat().length, 2);

const box = { w: 100, h: 40 };
// grid: rows of at most three, as before
const g = arrange(["1", "2", "3", "4"], [box, box, box, box], [], "grid", 10, 50);
deepStrictEqual(g.at, [{ x: 0, y: 0 }, { x: 110, y: 0 }, { x: 0, y: 50 }, { x: 110, y: 50 }]);
deepStrictEqual([g.w, g.h], [210, 90]);
// row: one line
const r = arrange(["1", "2", "3"], [box, { w: 50, h: 80 }, box], [], "row", 10, 50);
deepStrictEqual(r.at.map((p) => p.x), [0, 110, 170]);
deepStrictEqual([r.w, r.h], [270, 80]);

// flow: steps left → right, each step centred top to bottom
const f = arrange(["s", "x", "y", "t"], [box, box, box, box], [["s", "x"], ["s", "y"], ["x", "t"], ["y", "t"]], "flow", 10, 50);
deepStrictEqual(f.at, [{ x: 0, y: 25 }, { x: 150, y: 0 }, { x: 150, y: 50 }, { x: 300, y: 25 }]);
deepStrictEqual([f.w, f.h], [400, 90]);
// tree: steps top → bottom
const t = arrange(["s", "x", "y"], [box, box, box], [["s", "x"], ["s", "y"]], "tree", 10, 50);
deepStrictEqual(t.at, [{ x: 55, y: 0 }, { x: 0, y: 90 }, { x: 110, y: 90 }]);
deepStrictEqual([t.w, t.h], [210, 130]);

// nothing overlaps in any layout
for (const layout of ["grid", "flow", "tree", "row"] as const) {
  const ids = ["a", "b", "c", "d", "e"];
  const sizes = ids.map((_, n) => ({ w: 80 + n * 10, h: 30 + n * 5 }));
  const a = arrange(ids, sizes, [["a", "b"], ["a", "c"], ["c", "d"], ["d", "a"]], layout, 8, 40);
  for (let i = 0; i < ids.length; i++)
    for (let j = i + 1; j < ids.length; j++) {
      const p = a.at[i], q = a.at[j];
      const apart = p.x + sizes[i].w <= q.x || q.x + sizes[j].w <= p.x || p.y + sizes[i].h <= q.y || q.y + sizes[j].h <= p.y;
      ok(apart, `${layout}: ${ids[i]} and ${ids[j]} overlap`);
      ok(p.x + sizes[i].w <= a.w + 0.001 && p.y + sizes[i].h <= a.h + 0.001, `${layout}: ${ids[i]} fits the space`);
    }
}
deepStrictEqual(arrange([], [], [], "flow", 10, 50), { at: [], w: 0, h: 0 });

// ---- nothing runs on sideways: past the width given, a layout goes on underneath
const five = ["1", "2", "3", "4", "5"], boxes = five.map(() => box);
// row: breaks into the next row where the next item would pass the width
const wr = arrange(five, boxes, [], "row", 10, 50, 250);
deepStrictEqual(wr.at, [{ x: 0, y: 0 }, { x: 110, y: 0 }, { x: 0, y: 50 }, { x: 110, y: 50 }, { x: 0, y: 100 }]);
deepStrictEqual([wr.w, wr.h], [210, 140]);
// one item wider than the width still gets a row of its own; a width of 0 is one item per row (a column)
deepStrictEqual(arrange(["a", "b"], [{ w: 400, h: 40 }, box], [], "row", 10, 50, 250).at, [{ x: 0, y: 0 }, { x: 0, y: 50 }]);
deepStrictEqual(arrange(["a", "b", "c"], [box, box, box], [], "row", 10, 50, 0).at.map((p) => p.x), [0, 0, 0]);
// grid: three to a row at most, fewer when three don't fit
deepStrictEqual(arrange(five, boxes, [], "grid", 10, 50, 250).at.map((p) => p.y), [0, 0, 50, 50, 100]);
deepStrictEqual(arrange(five, boxes, [], "grid", 10, 50, 1000).at.map((p) => p.y), [0, 0, 0, 50, 50]);
// flow: a chain of five, three steps to a band (100 + 50 + 100 + 50 + 100 = 400): the flow turns down under its
// last step and comes back right → left, so each arrow stays between neighbours
const chain = [["1", "2"], ["2", "3"], ["3", "4"], ["4", "5"]] as const;
const wf = arrange(five, boxes, chain, "flow", 10, 50, 400);
deepStrictEqual(wf.at, [{ x: 0, y: 0 }, { x: 150, y: 0 }, { x: 300, y: 0 }, { x: 300, y: 90 }, { x: 150, y: 90 }]);
deepStrictEqual([wf.w, wf.h], [400, 130]);
// a third band goes on left → right again, under the end of the second
const seven = ["1", "2", "3", "4", "5", "6", "7"];
const wf3 = arrange(seven, seven.map(() => box), [...chain, ["5", "6"], ["6", "7"]], "flow", 10, 50, 250);
deepStrictEqual(wf3.at.map((p) => [p.x, p.y]), [[0, 0], [150, 0], [150, 90], [0, 90], [0, 180], [150, 180], [150, 270]]);
// a band coming back that is longer than the one above it: nothing ends up left of 0
const lopsided = arrange(["a", "b", "c"], [{ w: 300, h: 40 }, box, box], [["a", "b"], ["b", "c"]], "flow", 10, 50, 320);
ok(lopsided.at.every((p) => p.x >= 0) && lopsided.w <= 320);
// tree: a step too wide for the width goes on in rows under its first, each centred
const kids = ["r", "a", "b", "c", "d", "e"];
const wt = arrange(kids, kids.map(() => box), kids.slice(1).map((k) => ["r", k] as const), "tree", 10, 50, 330);
deepStrictEqual(wt.at, [{ x: 110, y: 0 }, { x: 0, y: 90 }, { x: 110, y: 90 }, { x: 220, y: 90 }, { x: 55, y: 140 }, { x: 165, y: 140 }]);
deepStrictEqual([wt.w, wt.h], [320, 180]);
// wrapped or not, nothing overlaps and everything fits the space
for (const layout of ["grid", "flow", "tree", "row"] as const) {
  const ids = ["a", "b", "c", "d", "e", "f", "g", "h"];
  const sizes = ids.map((_, n) => ({ w: 80 + (n % 3) * 30, h: 30 + (n % 4) * 10 }));
  const links = ids.slice(1).map((id, n) => [ids[layout === "tree" ? 0 : n], id] as const);
  const a = arrange(ids, sizes, links, layout, 8, 40, 300);
  for (let i = 0; i < ids.length; i++) {
    ok(a.at[i].x >= -0.001 && a.at[i].x + sizes[i].w <= a.w + 0.001 && a.at[i].y + sizes[i].h <= a.h + 0.001, `${layout} wrapped: ${ids[i]} fits the space`);
    for (let j = i + 1; j < ids.length; j++) {
      const p = a.at[i], q = a.at[j];
      ok(p.x + sizes[i].w <= q.x || q.x + sizes[j].w <= p.x || p.y + sizes[i].h <= q.y || q.y + sizes[j].h <= p.y, `${layout} wrapped: ${ids[i]} and ${ids[j]} overlap`);
    }
  }
  ok(a.w <= 300 + 140, `${layout} wrapped: about as wide as asked (${a.w})`);
}
// the width a container wraps at: its base, or enough for its two widest items side by side
equal(wrapWidth(1200, [box, box, box], 72), 1200);
equal(wrapWidth(1200, [{ w: 900, h: 1 }, { w: 800, h: 1 }, box], 72), 1772);
equal(wrapWidth(1200, [{ w: 2000, h: 1 }], 72), 2000);
equal(wrapWidth(1200, [], 72), 1200);

// ---- freeSpot: something new goes into the first free place, from the top and from the left
deepStrictEqual(freeSpot([], { w: 300, h: 200 }, 96, 2000), { x: 0, y: 0 });
// room to the right, within the width: beside what's there, on the top row
const one = [{ x: 0, y: 0, w: 600, h: 400 }];
deepStrictEqual(freeSpot(one, { w: 300, h: 200 }, 96, 2000), { x: 696, y: 0 });
// no room within the width: under it, at the left edge (not further to the right)
deepStrictEqual(freeSpot(one, { w: 300, h: 200 }, 96, 900), { x: 0, y: 496 });
// a canvas that already runs far to the right: the new thing goes underneath, near the left
const strip = [{ x: 0, y: 0, w: 900, h: 300 }, { x: 996, y: 0, w: 900, h: 500 }, { x: 1992, y: 0, w: 900, h: 300 }];
deepStrictEqual(freeSpot(strip, { w: 400, h: 200 }, 96, 2304), { x: 0, y: 396 });
// a gap under a short thing beside a tall one is used before a new row is started under everything
const tall = [{ x: 0, y: 0, w: 500, h: 1000 }, { x: 596, y: 0, w: 500, h: 300 }];
deepStrictEqual(freeSpot(tall, { w: 400, h: 200 }, 96, 1100), { x: 596, y: 396 });
// wider than the width: a row of its own at the left edge
deepStrictEqual(freeSpot(one, { w: 3000, h: 200 }, 96, 2000), { x: 0, y: 496 });
// places are on the grid, and the left edge is where the canvas starts (not 0)
deepStrictEqual(freeSpot([{ x: -240, y: 48, w: 500, h: 290 }], { w: 300, h: 200 }, 96, 700, 24), { x: -240, y: 456 });
// what it finds never overlaps what's there (or comes closer than the gap)
{
  const there = [{ x: 0, y: 0, w: 300, h: 200 }, { x: 396, y: 0, w: 200, h: 500 }, { x: 0, y: 296, w: 250, h: 150 }, { x: 700, y: 100, w: 400, h: 100 }];
  for (const b of [{ w: 100, h: 100 }, { w: 500, h: 300 }, { w: 1500, h: 80 }]) {
    const p = freeSpot(there, b, 96, 1200, 24);
    ok(there.every((r) => p.x >= r.x + r.w + 96 || p.x + b.w + 96 <= r.x || p.y >= r.y + r.h + 96 || p.y + b.h + 96 <= r.y), `a free place for ${b.w}×${b.h}`);
  }
}

// ---- repack: things packed again in the order given, around what stays (배치 정리)
// a far corner and uneven gaps become one tidy row from the top-left corner of everything
const loose = [{ x: 0, y: 30, w: 300, h: 200 }, { x: 900, y: 0, w: 300, h: 100 }, { x: 100, y: 800, w: 300, h: 100 }];
deepStrictEqual(repack(loose, [0, 1, 2], [], 96, 2000), [{ x: 0, y: 0 }, { x: 396, y: 0 }, { x: 792, y: 0 }]);
// no wider than asked: the third goes under the short one
deepStrictEqual(repack(loose, [0, 1, 2], [], 96, 800), [{ x: 0, y: 0 }, { x: 396, y: 0 }, { x: 396, y: 196 }]);
// the order given is the order placed
deepStrictEqual(repack(loose, [2, 0, 1], [], 96, 2000), [{ x: 396, y: 0 }, { x: 792, y: 0 }, { x: 0, y: 0 }]);
// what stays where it is is kept clear of
deepStrictEqual(repack([{ x: 2000, y: 2000, w: 300, h: 100 }], [0], [{ x: 0, y: 0, w: 500, h: 500 }], 96, 2000), [{ x: 596, y: 0 }]);
deepStrictEqual(repack([], [], [], 96, 2000), []);
// a free place read from a corner that is given
deepStrictEqual(freeSpot([], { w: 10, h: 10 }, 96, 2000, 1, { x: 240, y: 48 }), { x: 240, y: 48 });

console.log("ok draw layout");
