// Which sides a line leaves and enters by (src/lineSides.ts; docs/features/canvas/canvas-controls.md "연결점 규약").
// Run: `npm test`.
import { deepStrictEqual, equal } from "node:assert/strict";
import { commonParent, flowOfLayout, sidesFor } from "../src/lineSides.ts";

const box = (x: number, y: number, w = 240, h = 72) => ({ x, y, w, h });

// ---- no flow: the sides that face each other, by which way the two are further apart
deepStrictEqual(sidesFor(box(0, 0), box(0, 300)), ["bottom", "top"]);
deepStrictEqual(sidesFor(box(0, 300), box(0, 0)), ["top", "bottom"]);
deepStrictEqual(sidesFor(box(0, 0), box(600, 100)), ["right", "left"]);
deepStrictEqual(sidesFor(box(600, 0), box(0, 100)), ["left", "right"]);
deepStrictEqual(sidesFor(undefined, box(0, 0)), ["bottom", "top"]);

// ---- a tree: every child is joined from its parent's bottom to its own top, however far out to the side it is
const parent = box(600, 0);
for (const child of [box(0, 144), box(600, 144), box(1500, 144), box(3000, 144)]) deepStrictEqual(sidesFor(parent, child, "down"), ["bottom", "top"]);
// (without the tree's way the far ones would be joined at the parent's flank)
deepStrictEqual(sidesFor(parent, box(3000, 144)), ["right", "left"]);
// a line going back up: out of the top, in at the bottom
deepStrictEqual(sidesFor(box(0, 144), parent, "down"), ["top", "bottom"]);
// two on the same row (siblings joined by an arrow): sideways
deepStrictEqual(sidesFor(box(0, 144), box(300, 144), "down"), ["right", "left"]);
deepStrictEqual(sidesFor(box(300, 144), box(0, 150), "down"), ["left", "right"]);

// ---- a flow: out of the right, in at the left, even when the next step sits much lower
deepStrictEqual(sidesFor(box(0, 0), box(312, 0), "across"), ["right", "left"]);
deepStrictEqual(sidesFor(box(0, 0), box(312, 900), "across"), ["right", "left"]);
// a flow that turned back runs right → left: the other way round
deepStrictEqual(sidesFor(box(624, 144), box(312, 144), "across"), ["left", "right"]);
// where it turns down, the next step is right under the last one: up and down
deepStrictEqual(sidesFor(box(624, 0), box(624, 144), "across"), ["bottom", "top"]);
deepStrictEqual(sidesFor(box(624, 144), box(624, 0), "across"), ["top", "bottom"]);

// ---- boxes that aren't measured are points: nothing is "the same row", the flow's way holds
deepStrictEqual(sidesFor(box(0, 0, 0, 0), box(500, 0, 0, 0), "down"), ["bottom", "top"]);
deepStrictEqual(sidesFor(box(0, 0, 0, 0), box(0, 500, 0, 0), "across"), ["right", "left"]);

// ---- which layouts have a way
equal(flowOfLayout("tree"), "down");
equal(flowOfLayout("flow"), "across");
equal(flowOfLayout("grid"), undefined);
equal(flowOfLayout(undefined), undefined);

// ---- the area both ends are in
const nodes = [{ id: "o" }, { id: "i", parentId: "o" }, { id: "a", parentId: "i" }, { id: "b", parentId: "i" }, { id: "c", parentId: "o" }, { id: "free" }];
equal(commonParent(nodes, "a", "b"), "i");
equal(commonParent(nodes, "a", "c"), "o");
equal(commonParent(nodes, "a", "free"), undefined);
// an area and something inside it: the area around the area (not the area itself)
equal(commonParent(nodes, "i", "a"), "o");
equal(commonParent(nodes, "o", "a"), undefined);

console.log("ok sides");
