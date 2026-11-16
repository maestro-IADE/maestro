// Drawing layouts (src/drawLayout.ts; docs/features/canvas/structure-drawing.md). Run: `npm test`.
import { deepStrictEqual, equal, ok } from "node:assert/strict";
import { arrange, layers } from "../src/drawLayout.ts";

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

console.log("ok draw layout");
