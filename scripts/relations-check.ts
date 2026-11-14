// Relations, the model (src/relationModel.ts; docs/features/canvas/relations.md). Run: `npm test`.
import { equal, deepStrictEqual, ok } from "node:assert/strict";
import { allOf, choicesFor, lookalike, relationOfLine, resolve, type RelFile } from "../src/relationModel.ts";

const file: RelFile = {
  relations: [
    { id: "r1", name: "원인", pattern: "solid", color: "red", description: "A가 B를 일으켜요" },
    { id: "r2", name: "가정", extends: "r1", pattern: "dotted" },
    { id: "r3", name: "설계 참조", extends: "reference", color: "amber" },
    { id: "r4", name: "세부 설계 참조", extends: "r3" },
  ],
  builtin: { orchestrates: "blue" },
};
const list = allOf(file);

// built-in first, with a picked colour
deepStrictEqual(list.slice(0, 4).map((r) => r.id), ["plain", "orchestrates", "command_pipe", "reference"]);
equal(resolve(list, "orchestrates")?.color, "blue", "a built-in relation takes its picked colour");
equal(resolve(list, "reference")?.pattern, "dashed", "참조 is dashed");

// inheritance: empty fields come from the parent, own ones win
const guess = resolve(list, "r2")!;
equal(guess.pattern, "dotted", "own pattern");
equal(guess.color, "red", "colour from the parent");
equal(guess.head, "one", "default head");
equal(guess.behavior, null, "no behaviour without a built-in root");
const deep = resolve(list, "r4")!;
equal(deep.behavior, "reference", "behaviour from the built-in root, two steps up");
equal(deep.color, "amber");
equal(deep.pattern, "dashed", "pattern from the built-in root");

// a line takes its own relation only when it fits; otherwise its default
const arrow = (relation?: string) => ({ data: { kind: "arrow" as const, relation } });
const ref = (relation?: string) => ({ data: { kind: "reference" as const, relation } });
equal(relationOfLine(list, arrow()).id, "plain", "a drawn line is 기본 관계");
equal(relationOfLine(list, arrow("r1")).id, "r1");
equal(relationOfLine(list, arrow("r3")).id, "plain", "a reference relation doesn't fit a drawn line");
equal(relationOfLine(list, arrow("gone")).id, "plain", "a deleted relation falls back");
equal(relationOfLine(list, ref()).id, "reference");
equal(relationOfLine(list, ref("r4")).id, "r4", "a reference line takes a relation that builds on 참조");
equal(relationOfLine(list, ref("r1")).id, "reference", "but not one without that behaviour");
equal(relationOfLine([], ref()).id, "reference", "without a list: the built-in");

// what a line can switch to
deepStrictEqual(choicesFor(list, arrow()).map((r) => r.id), ["plain", "r1", "r2"]);
deepStrictEqual(choicesFor(list, ref()).map((r) => r.id), ["reference", "r3", "r4"]);

// declaring a twin is worth a word
ok(lookalike(list, { id: "new", name: "새것", pattern: "solid", color: "red" })?.id === "r1", "same look as 원인");
equal(lookalike(list, { id: "new", name: "새것", pattern: "dashed", color: "teal" }), null);

// a loop in the file never hangs
const loop = allOf({ relations: [{ id: "a", name: "a", extends: "b" }, { id: "b", name: "b", extends: "a" }], builtin: {} });
ok(resolve(loop, "a"), "resolves despite a loop");

console.log("ok relations");
