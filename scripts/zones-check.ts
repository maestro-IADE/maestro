// Areas: growing for their contents, fitting, folding (src/zones.ts; docs/features/canvas/zones.md). Run: `npm test`.
import { deepStrictEqual, equal, ok } from "node:assert/strict";
import {
  addPhase,
  boxOf,
  commonPhased,
  contentBox,
  currentOf,
  dropInto,
  fitZone,
  foldZone,
  foldedInto,
  goPhase,
  growZones,
  heldKey,
  hiddenByPhase,
  hostAt,
  inside,
  isStack,
  layStacks,
  lineEnds,
  lineHiddenByPhase,
  openSize,
  phaseLabel,
  removePhase,
  renamePhase,
  rowsOf,
  seatIn,
  setPhaseMode,
  settleZones,
  setWhen,
  stackOnto,
  stackedInto,
  tidyArea,
  toggleWhen,
  withOpenSize,
  wrapZone,
  type Metrics,
  type Phase,
  type ZNode,
} from "../src/zones.ts";

const m: Metrics = { grid: 24, pad: 24, top: 48, min: { width: 192, height: 120 }, chip: { width: 240, height: 48 }, fallback: { width: 240, height: 72 } };
const zone = (id: string, x: number, y: number, width: number, height: number, parentId?: string): ZNode => ({ id, type: "zone", parentId, position: { x, y }, width, height, data: { title: id } });
const block = (id: string, parentId: string | undefined, x: number, y: number, width = 240, height = 72): ZNode => ({ id, type: "struct_block", parentId, position: { x, y }, width, height, data: { title: id } });
const get = (ns: ZNode[], id: string) => ns.find((n) => n.id === id)!;
/** Where a node is on the canvas: its position plus every area's around it. */
const abs = (ns: ZNode[], id: string) => {
  let x = 0, y = 0;
  for (let n: ZNode | undefined = get(ns, id); n; n = n.parentId ? get(ns, n.parentId) : undefined) {
    x += n.position.x;
    y += n.position.y;
  }
  return { x, y };
};

// ---- growZones
// everything inside: nothing changes, and the very same array comes back (no needless render)
const calm = [zone("z", 0, 0, 432, 288), block("b", "z", 24, 48)];
equal(growZones(calm, m), calm);
deepStrictEqual(contentBox(calm, "z", m), { x0: 24, y0: 48, x1: 264, y1: 120 });
equal(contentBox(calm, "b", m), null);

// sticking out right and bottom: those sides move out to it plus the padding, on whole cells; the area stays put
const rb = growZones([zone("z", 0, 0, 432, 288), block("b", "z", 300, 250)], m);
deepStrictEqual([get(rb, "z").position, get(rb, "z").width, get(rb, "z").height], [{ x: 0, y: 0 }, 576, 360]);
deepStrictEqual(get(rb, "b").position, { x: 300, y: 250 });

// sticking out left and top: the area's corner moves, and nothing inside moves on the canvas
const before = [zone("z", 100, 100, 432, 288), block("out", "z", -24, -24), block("in", "z", 100, 100)];
const lt = growZones(before, m);
deepStrictEqual([get(lt, "z").position, get(lt, "z").width, get(lt, "z").height], [{ x: 52, y: 28 }, 480, 360]);
deepStrictEqual(get(lt, "out").position, { x: 24, y: 48 }, "padding on the left, room for the name on top");
for (const id of ["out", "in"]) deepStrictEqual(abs(lt, id), abs(before, id), `${id} stays where it was`);

// an area that grew pushes the one around it (children listed before their parents work too)
const nested = growZones([block("b", "i", 200, 48), zone("i", 24, 48, 240, 120, "o"), zone("o", 0, 0, 480, 360)], m);
equal(get(nested, "i").width, 480);
equal(get(nested, "o").width, 528);
equal(get(nested, "o").height, 360);

// a block with no size yet counts as the fallback size
const unsized = growZones([zone("z", 0, 0, 192, 120), { ...block("q", "z", 24, 48), width: undefined, height: undefined }], m);
deepStrictEqual([get(unsized, "z").width, get(unsized, "z").height], [288, 120], "touching the bottom edge is still inside");
// ... and as its measured size once it has one
const measured = growZones([zone("z", 0, 0, 192, 120), { ...block("q", "z", 24, 48), width: undefined, height: undefined, measured: { width: 312, height: 200 } }], m);
deepStrictEqual([get(measured, "z").width, get(measured, "z").height], [360, 288]);

// ---- fitZone
const loose = [zone("z", 100, 100, 960, 720), block("a", "z", 240, 240), block("b", "z", 600, 480)];
const fit = fitZone(loose, "z", m, { width: 432, height: 288 });
deepStrictEqual([get(fit, "z").position, get(fit, "z").width, get(fit, "z").height], [{ x: 316, y: 292 }, 648, 384]);
deepStrictEqual([get(fit, "a").position, get(fit, "b").position], [{ x: 24, y: 48 }, { x: 384, y: 288 }]);
for (const id of ["a", "b"]) deepStrictEqual(abs(fit, id), abs(loose, id), `${id} stays where it was`);
// fitted twice is the same as once
deepStrictEqual(fitZone(fit, "z", m, { width: 432, height: 288 }), fit);
// never under the smallest area
const tiny = fitZone([zone("z", 0, 0, 960, 720), block("a", "z", 24, 48, 48, 24)], "z", m, { width: 432, height: 288 });
deepStrictEqual([get(tiny, "z").position, get(tiny, "z").width, get(tiny, "z").height], [{ x: 0, y: 0 }, 192, 120]);
// an area that holds nothing goes back to the size a new one has
const empty = fitZone([zone("z", 48, 48, 960, 720)], "z", m, { width: 432, height: 288 });
deepStrictEqual([get(empty, "z").position, get(empty, "z").width, get(empty, "z").height], [{ x: 48, y: 48 }, 432, 288]);
// a block near the area's left edge: fitting keeps the padding, so the corner may move out, and the area around it follows
const cramped = [zone("o", 0, 0, 480, 360), zone("i", 0, 48, 432, 288, "o"), block("a", "i", 0, 48)];
const edge = fitZone(cramped, "i", m, { width: 432, height: 288 });
deepStrictEqual([get(edge, "o").position, get(edge, "o").width], [{ x: -48, y: 0 }, 528], "the outer area grew left for the inner one");
deepStrictEqual(get(edge, "i").position, { x: 24, y: 48 });
deepStrictEqual(get(edge, "a").position, { x: 24, y: 48 });
deepStrictEqual(abs(edge, "a"), abs(cramped, "a"));

// ---- foldZone, foldedInto, lineEnds
const open: ZNode[] = [
  zone("o", 0, 0, 720, 480),
  { ...block("a", "o", 24, 48), selected: true },
  zone("i", 300, 48, 300, 200, "o"),
  { ...block("deep", "i", 24, 48), selected: true },
  { ...block("free", undefined, 900, 0), selected: true },
];
deepStrictEqual([...inside(open, "o")].sort(), ["a", "deep", "i"]);
deepStrictEqual([...inside(open, "i")], ["deep"]);
equal(foldedInto(open).size, 0);

const shut = foldZone(open, "o", true, m);
deepStrictEqual([get(shut, "o").width, get(shut, "o").height, get(shut, "o").position], [240, 48, { x: 0, y: 0 }], "the chip, its corner where the area's was");
deepStrictEqual(get(shut, "o").data, { title: "o", folded: true, full: { width: 720, height: 480 } });
deepStrictEqual([get(shut, "a").selected, get(shut, "deep").selected, get(shut, "free").selected], [false, false, true], "nothing inside stays picked");
deepStrictEqual(get(shut, "a").position, { x: 24, y: 48 }, "what's inside keeps its place for when the area opens");
equal(foldZone(shut, "o", true, m), shut, "folding a folded area changes nothing");
deepStrictEqual(openSize(get(shut, "o"), m), { width: 720, height: 480 });
deepStrictEqual(openSize(get(open, "o"), m), { width: 720, height: 480 });
deepStrictEqual(withOpenSize(get(shut, "o"), { width: 800, height: 500 }).data.full, { width: 800, height: 500 });
equal(withOpenSize(get(shut, "o"), { width: 800, height: 500 }).width, 240, "a folded area stays a chip");
equal(withOpenSize(get(open, "o"), { width: 800, height: 500 }).width, 800);

const into = foldedInto(shut);
deepStrictEqual([...into].sort(), [["a", "o"], ["deep", "o"], ["i", "o"]]);
deepStrictEqual(lineEnds(into, "a", "free"), { source: "o", target: "free", moved: true }, "a line to something inside goes to the chip");
equal(lineEnds(into, "a", "deep"), null, "a line inside a folded area isn't shown");
equal(lineEnds(into, "o", "a"), null);
deepStrictEqual(lineEnds(into, "free", "elsewhere"), { source: "free", target: "elsewhere", moved: false });
// a folded area inside a folded area: everything stands in for the outer one
const both = foldZone(foldZone(open, "i", true, m), "o", true, m);
equal(foldedInto(both).get("deep"), "o");
// only the inner one folded: its contents stand in for it, and it still shows
const inner = foldZone(open, "i", true, m);
deepStrictEqual([...foldedInto(inner)], [["deep", "i"]]);

// a folded area is left alone while it is folded ...
const drawnInto = [...shut, block("late", "o", 600, 440)];
equal(growZones(drawnInto, m), drawnInto);
// ... and opens at the size it had, grown for what was put in meanwhile
const reopened = foldZone(drawnInto, "o", false, m);
deepStrictEqual(get(reopened, "o").data, { title: "o" });
deepStrictEqual([get(reopened, "o").width, get(reopened, "o").height], [864, 552]);
const back = foldZone(shut, "o", false, m);
deepStrictEqual([get(back, "o").width, get(back, "o").height, get(back, "o").data], [720, 480, { title: "o" }]);
// an inner area opening pushes the area around it
const tight = foldZone([zone("o", 0, 0, 288, 144), { ...zone("i", 24, 48, 240, 48, "o"), data: { title: "i", folded: true, full: { width: 480, height: 240 } } }], "i", false, m);
deepStrictEqual([get(tight, "o").width, get(tight, "o").height], [528, 312]);
ok(!get(tight, "i").data.folded);

// ---- dropInto: what is dragged goes where the pointer lets go of it; nothing else changes who holds what
const can = (n: ZNode) => n.type === "struct_block" || n.type === "zone";
// an area made (or moved, or stretched) over a block doesn't take it: nobody dragged the block
const under = [zone("z", 48, 48, 432, 288), block("b", undefined, 148, 148)];
equal(dropInto(under, [], { x: 200, y: 200 }, m, can), under, "nothing was dragged");
equal(dropInto(under, ["z"], { x: 60, y: 60 }, m, can), under, "the area was dragged over the block: the block stays out");
// dragged in: the pointer is in the area when it lets go
const adopted = dropInto(under, ["b"], { x: 200, y: 200 }, m, can);
deepStrictEqual([get(adopted, "b").parentId, get(adopted, "b").position], ["z", { x: 100, y: 100 }]);
deepStrictEqual(abs(adopted, "b"), abs(under, "b"), "it doesn't move on the canvas");
equal(dropInto(adopted, ["b"], { x: 200, y: 200 }, m, can), adopted, "moved about inside: the same array comes back");
// the pointer counts, not where the block lies: a block mostly on the area, let go with the pointer outside it,
// stays out; one held by a corner that is inside goes in though most of it sticks out
const grazing = [zone("z", 0, 0, 432, 288), block("b", undefined, 250, 100)];
equal(dropInto(grazing, ["b"], { x: 480, y: 120 }, m, can), grazing);
const corner = [zone("z", 0, 0, 432, 288), block("b", undefined, 400, 250)];
equal(get(dropInto(corner, ["b"], { x: 410, y: 260 }, m, can), "b").parentId, "z");
// a block of an area sticking far out of it still belongs to it while the pointer lets go inside; let go outside,
// it comes out, where it is
const inZ = [zone("z", 100, 100, 432, 288), block("b", "z", 500, 0)];
equal(dropInto(inZ, ["b"], { x: 300, y: 200 }, m, can), inZ);
const out = dropInto(inZ, ["b"], { x: 700, y: 120 }, m, can);
deepStrictEqual([get(out, "b").parentId, get(out, "b").position], [undefined, { x: 600, y: 100 }]);
// the innermost area under the pointer takes it; parents end up first
const pile = [block("b", undefined, 100, 150), zone("i", 48, 96, 432, 288, "o"), zone("o", 0, 0, 960, 720)];
equal(hostAt(pile, ["b"], { x: 200, y: 200 }, m)?.id, "i");
equal(hostAt(pile, ["b"], { x: 700, y: 600 }, m)?.id, "o");
equal(hostAt(pile, ["b"], { x: 2000, y: 0 }, m), undefined);
const sorted = dropInto(pile, ["b"], { x: 200, y: 200 }, m, can);
deepStrictEqual(sorted.map((n) => [n.id, n.parentId]), [["o", undefined], ["i", "o"], ["b", "i"]]);
deepStrictEqual(abs(sorted, "b"), abs(pile, "b"), "b stays where it was");
// an area is dragged like a block: let go in another area it goes into it, and what it holds comes along as it is
const two = [zone("o", 0, 0, 960, 720), zone("i", 48, 96, 432, 288), block("kid", "i", 24, 48)];
const draggedIn = dropInto(two, ["i", "kid"], { x: 100, y: 110 }, m, can);
deepStrictEqual([get(draggedIn, "i").parentId, get(draggedIn, "i").position, get(draggedIn, "kid").parentId, get(draggedIn, "kid").position], ["o", { x: 48, y: 96 }, "i", { x: 24, y: 48 }]);
// never into itself or into something inside itself (the pointer is in the dragged area and in the one inside it)
const nest = [zone("o", 0, 0, 480, 480), zone("i", 120, 120, 240, 240, "o")];
equal(dropInto(nest, ["o"], { x: 200, y: 200 }, m, can), nest);
// several dragged at once all go where the pointer is
const many = dropInto([zone("z", 0, 0, 432, 288), block("a", undefined, 500, 0), block("c", undefined, 500, 100)], ["a", "c"], { x: 100, y: 100 }, m, can);
deepStrictEqual([get(many, "a").parentId, get(many, "c").parentId], ["z", "z"]);
// what can't sit in an area is left alone (a prompt block dragged over an area)
const card: ZNode = { id: "card", type: "prompt_input_primitive", position: { x: 100, y: 100 }, width: 240, height: 72, data: {} };
const withCard = [zone("z", 0, 0, 432, 288), card];
equal(dropInto(withCard, ["card"], { x: 150, y: 120 }, m, can), withCard);
// a folded area takes nothing in (it would vanish on the spot); once it opens it does
const chip = foldZone([zone("z", 0, 0, 720, 480), block("kept", "z", 400, 300)], "z", true, m);
const onChip = [...chip, block("b", undefined, 0, 0, 240, 48)];
equal(hostAt(onChip, ["b"], { x: 20, y: 20 }, m), undefined);
equal(dropInto(onChip, ["b"], { x: 20, y: 20 }, m, can), onChip);
const opened = foldZone(onChip, "z", false, m);
equal(get(opened, "b").parentId, undefined, "opening doesn't take in what lies on it");
equal(get(dropInto(opened, ["b"], { x: 20, y: 20 }, m, can), "b").parentId, "z");
// let go inside near the edge: in, then the area grows around it; done twice is done once
const settle = (ns: ZNode[]) => growZones(dropInto(ns, ["b"], { x: 310, y: 260 }, m, can), m);
const edgeIn = settle([zone("z", 0, 0, 432, 288), block("b", undefined, 300, 250)]);
deepStrictEqual([get(edgeIn, "b").parentId, get(edgeIn, "z").width, get(edgeIn, "z").height], ["z", 576, 360], "in by the pointer, then the area grows around it");
equal(settle(edgeIn), edgeIn);

// ---- steps and states: an area says when what it holds shows
const P = (id: string, name = ""): Phase => ({ id, name });
const phased = (z: ZNode, phases: Phase[], at?: string, mode: "step" | "state" = "step"): ZNode => ({ ...z, data: { ...z.data, mode, phases, at } });
const only = (n: ZNode, ...when: string[]): ZNode => ({ ...n, data: { ...n.data, when } });
const whenOf = (ns: ZNode[], id: string) => get(ns, id).data.when;

const flow = [
  phased(zone("z", 0, 0, 720, 480), [P("p1"), P("p2", "검토"), P("p3")], "p2"),
  only(block("one", "z", 24, 48), "p1"),
  only(block("two", "z", 24, 48), "p2", "p3"),
  block("always", "z", 300, 48),
  block("free", undefined, 900, 0),
];
equal(currentOf(get(flow, "z")), "p2");
deepStrictEqual([...hiddenByPhase(flow)], ["one"], "only what doesn't exist in the step showing; a block with no `when` is in all of them");
deepStrictEqual([phaseLabel(get(flow, "z"), "p1"), phaseLabel(get(flow, "z"), "p2"), phaseLabel(get(flow, "z"), "nope")], ["Step 1", "Step 2 · 검토", ""]);
const states = phased(zone("s", 0, 0, 432, 288), [P("s1", "정상"), P("s2")], "s1", "state");
deepStrictEqual([phaseLabel(states, "s1"), phaseLabel(states, "s2")], ["정상", "State 2"]);
// `at` naming none of them: the first one shows
equal(currentOf(phased(zone("z", 0, 0, 432, 288), [P("a"), P("b")], "gone")), "a");
// a plain area hides nothing, whatever its blocks say; an id the area no longer has doesn't count
equal(hiddenByPhase([zone("z", 0, 0, 432, 288), only(block("b", "z", 24, 48), "p9")]).size, 0);
equal(hiddenByPhase([phased(zone("z", 0, 0, 432, 288), [P("p1"), P("p2")], "p1"), only(block("b", "z", 24, 48), "gone")]).size, 0);
// an area hidden by a step hides what it holds
const nestedIn = [...flow, only(zone("in", 300, 150, 300, 200, "z"), "p1"), block("deep", "in", 24, 48)];
deepStrictEqual([...hiddenByPhase(nestedIn)].sort(), ["deep", "in", "one"]);

// going on, back and straight to one; it stops at the ends (and then nothing changes)
const third = goPhase(flow, "z", "next");
deepStrictEqual([currentOf(get(third, "z")), [...hiddenByPhase(third)]], ["p3", ["one"]]);
equal(goPhase(third, "z", "next"), third);
const firstStep = goPhase(flow, "z", "p1");
deepStrictEqual([currentOf(get(firstStep, "z")), [...hiddenByPhase(firstStep)]], ["p1", ["two"]]);
equal(goPhase(firstStep, "z", "prev"), firstStep);
equal(goPhase(flow, "z", "nope"), flow);
equal(goPhase(flow, "free", "next"), flow, "not an area");

// an area gets steps (it starts with one), switches to states (its list stays), goes plain again (everything shows)
const plainZone = [zone("z", 0, 0, 432, 288), only(block("b", "z", 24, 48), "x")];
const turnedOn = setPhaseMode(plainZone, "z", "step", P("a"));
deepStrictEqual(get(turnedOn, "z").data, { title: "z", mode: "step", phases: [P("a")], at: "a" });
deepStrictEqual(get(setPhaseMode(turnedOn, "z", "state", P("unused")), "z").data, { title: "z", mode: "state", phases: [P("a")], at: "a" });
const turnedOff = setPhaseMode(flow, "z", undefined, P("x"));
deepStrictEqual([get(turnedOff, "z").data, whenOf(turnedOff, "one"), "when" in get(turnedOff, "two").data], [{ title: "z" }, undefined, false]);
equal(get(turnedOff, "free"), get(flow, "free"));
equal(setPhaseMode(plainZone, "z", undefined, P("x")), plainZone);

// a new step is gone to; a plain area takes none
const fourth = addPhase(flow, "z", P("p4"));
deepStrictEqual([(get(fourth, "z").data.phases as Phase[]).length, currentOf(get(fourth, "z"))], [4, "p4"]);
equal(addPhase(plainZone, "z", P("p")), plainZone);
equal((get(renamePhase(flow, "z", "p1", "시작"), "z").data.phases as Phase[])[0].name, "시작");
equal(renamePhase(flow, "z", "nope", "x"), flow);
// a step taken away: the area goes to the one in its place, and what was only in it stays, in all that are left
const cut = removePhase(flow, "z", "p2");
deepStrictEqual([(get(cut, "z").data.phases as Phase[]).map((p) => p.id), currentOf(get(cut, "z")), whenOf(cut, "one"), whenOf(cut, "two")], [["p1", "p3"], "p3", ["p1"], ["p3"]]);
const cutAgain = removePhase(cut, "z", "p3");
deepStrictEqual([currentOf(get(cutAgain, "z")), "when" in get(cutAgain, "two").data, hiddenByPhase(cutAgain).size], ["p1", false, 0]);
equal(removePhase(cutAgain, "z", "p1"), cutAgain, "never the last one");

// pressing a step in a block's list: from "all" it is only that one; then added or dropped; none or all = all
const three = [P("p1"), P("p2"), P("p3")];
deepStrictEqual(toggleWhen(undefined, three, "p2"), ["p2"]);
deepStrictEqual(toggleWhen(["p2"], three, "p3"), ["p2", "p3"]);
deepStrictEqual(toggleWhen(["p3", "p1"], three, "p2"), undefined, "all three again");
deepStrictEqual(toggleWhen(["p2"], three, "p2"), undefined, "none left");
deepStrictEqual(toggleWhen(["p3", "p1"], three, "p3"), ["p1"]);
deepStrictEqual(toggleWhen(["gone"], three, "p1"), ["p1"]);
deepStrictEqual(whenOf(setWhen(flow, ["always"], ["p1"]), "always"), ["p1"]);
equal("when" in get(setWhen(flow, ["one"], undefined), "one").data, false);

// a block dragged into an area with steps exists in the one showing; dragged out, it forgets it
const dropped = dropInto([phased(zone("z", 0, 0, 720, 480), [P("p1"), P("p2")], "p2"), block("new", undefined, 100, 100)], ["new"], { x: 150, y: 120 }, m, can);
deepStrictEqual([get(dropped, "new").parentId, whenOf(dropped, "new")], ["z", ["p2"]]);
const left = dropInto(dropped, ["new"], { x: 2000, y: 0 }, m, can);
deepStrictEqual([get(left, "new").parentId, "when" in get(left, "new").data], [undefined, false]);
// an area a step hides takes nothing in: the one around it does
const hiddenHost = dropInto([phased(zone("z", 0, 0, 960, 720), [P("p1"), P("p2")], "p2"), only(zone("in", 24, 48, 432, 288, "z"), "p1"), block("b", undefined, 100, 150)], ["b"], { x: 150, y: 170 }, m, can);
deepStrictEqual([get(hiddenHost, "b").parentId, whenOf(hiddenHost, "b")], ["z", ["p2"]]);
// an area is as big as what shows in it: a block of another step sticking out doesn't make it grow; going to that
// step does
const stepArea = [phased(zone("z", 0, 0, 432, 288), [P("p1"), P("p2")], "p2"), only(block("past", "z", 400, 48), "p1"), only(block("now", "z", 24, 48), "p2")];
equal(growZones(stepArea, m), stepArea);
deepStrictEqual(contentBox(stepArea, "z", m), { x0: 24, y0: 48, x1: 264, y1: 120 });
equal(get(growZones(goPhase(stepArea, "z", "p1"), m), "z").width, 672);

// ---- settleZones: an area is fitted again when what it holds changes, and only then
{
  const zoneOf = (ns: ZNode[], id = "z") => [get(ns, id).position, get(ns, id).width, get(ns, id).height];
  // seen for the first time: remembered as it is (a roomy area stays roomy)
  const roomy = [zone("z", 0, 0, 720, 480), block("a", "z", 24, 48), block("b", "z", 288, 48)];
  const first = settleZones(roomy, m, new Map());
  equal(first.nodes, roomy);
  equal(first.seen.get("z")?.shape, heldKey(roomy, "z", m));
  // nothing in it changed: nothing happens, however often
  equal(settleZones(roomy, m, first.seen).nodes, roomy);
  // the area dragged somewhere else (what it holds goes along), or stretched by hand: it stays as it is
  const stretched = roomy.map((n) => (n.id === "z" ? { ...n, position: { x: 500, y: 300 }, width: 960 } : n));
  equal(settleZones(stretched, m, first.seen).nodes, stretched);
  // its left side pulled out by hand (React Flow keeps what it holds in place on the canvas): the same
  const pulled = roomy.map((n) => (n.id === "z" ? { ...n, position: { x: -48, y: 0 }, width: 768 } : { ...n, position: { x: n.position.x + 48, y: n.position.y } }));
  equal(settleZones(pulled, m, first.seen).nodes, pulled);
  // a block moved inside it: fitted to its contents again (a cell around them, two on top)
  const refit = settleZones(roomy.map((n) => (n.id === "b" ? { ...n, position: { x: 288, y: 144 } } : n)), m, first.seen);
  deepStrictEqual(zoneOf(refit.nodes), [{ x: 0, y: 0 }, 552, 240]);
  equal(settleZones(refit.nodes, m, refit.seen).nodes, refit.nodes, "settled twice is settled once");
  // everything in it moved together (their places among themselves are the same): fitted again too
  const shifted = settleZones(roomy.map((n) => (n.id === "z" ? n : { ...n, position: { x: n.position.x + 48, y: n.position.y + 48 } })), m, first.seen);
  deepStrictEqual(zoneOf(shifted.nodes), [{ x: 48, y: 48 }, 552, 144]);
  // something went out: fitted around what is left. The last one out: the area keeps its size
  const oneLeft = settleZones(refit.nodes.filter((n) => n.id !== "b"), m, refit.seen);
  deepStrictEqual(zoneOf(oneLeft.nodes), [{ x: 0, y: 0 }, 288, 144]);
  const emptied = settleZones(oneLeft.nodes.filter((n) => n.id !== "a"), m, oneLeft.seen);
  deepStrictEqual([get(emptied.nodes, "z").width, emptied.seen.get("z")?.shape], [288, ""]);
  // something comes into an empty area: fitted around it, where it is on the canvas
  const cameIn = settleZones([...emptied.nodes, block("c", "z", 120, 120)], m, emptied.seen);
  deepStrictEqual([...zoneOf(cameIn.nodes), abs(cameIn.nodes, "c")], [{ x: 96, y: 72 }, 288, 144, { x: 120, y: 120 }]);
  // a block that changes size (a question answered, a block resized): fitted again
  const grown = settleZones(cameIn.nodes.map((n) => (n.id === "c" ? { ...n, height: 240 } : n)), m, cameIn.seen);
  equal(get(grown.nodes, "z").height, 312);
  // not measured yet: nothing is fitted or remembered until it is
  const unmeasured = [zone("z", 0, 0, 720, 480), { ...block("q", "z", 24, 48), width: undefined, height: undefined }];
  const waiting = settleZones(unmeasured, m, new Map());
  deepStrictEqual([waiting.nodes === unmeasured, waiting.seen.has("z"), heldKey(unmeasured, "z", m)], [true, false, null]);
  // the area goes to another step: fitted to what shows there (a question set shows one question at a time)
  const set = [phased(zone("z", 0, 0, 360, 240), [P("p1"), P("p2")], "p1"), only(block("q1", "z", 24, 48, 312, 168), "p1"), only(block("q2", "z", 24, 48, 312, 360), "p2")];
  const s1 = settleZones(set, m, new Map());
  equal(s1.nodes, set);
  const s2 = settleZones(goPhase(s1.nodes, "z", "next"), m, s1.seen);
  equal(get(s2.nodes, "z").height, 432);
  const s3 = settleZones(goPhase(s2.nodes, "z", "prev"), m, s2.seen);
  equal(get(s3.nodes, "z").height, 240);
  // innermost first: the inner area is fitted, then the one around it (what it holds changed size); nothing moves
  const outer = [zone("o", 0, 0, 960, 720), zone("i", 24, 48, 720, 480, "o"), block("a", "i", 24, 48)];
  const k0 = settleZones(outer, m, new Map());
  const both = settleZones(k0.nodes.map((n) => (n.id === "a" ? { ...n, position: { x: 48, y: 48 } } : n)), m, k0.seen);
  deepStrictEqual([get(both.nodes, "i").width, get(both.nodes, "i").height, get(both.nodes, "o").width, get(both.nodes, "o").height, abs(both.nodes, "a")], [288, 144, 336, 216, { x: 72, y: 96 }]);
  // a folded area waits: what it was seen holding stays, and it is fitted when it opens with other contents
  const shut = foldZone(roomy, "z", true, m);
  const while_ = settleZones([...shut, block("c", "z", 24, 240)], m, first.seen);
  deepStrictEqual([get(while_.nodes, "z").width, while_.seen.get("z")?.shape], [240, first.seen.get("z")?.shape]);
  const reopened = settleZones(foldZone(while_.nodes, "z", false, m), m, while_.seen);
  deepStrictEqual(zoneOf(reopened.nodes), [{ x: 0, y: 0 }, 552, 336]);
  // a stack is left to `layStacks`
  const st = [{ ...zone("s", 0, 0, 720, 480), data: { title: "s", stack: true } }, block("a", "s", 24, 48)];
  const st1 = settleZones(st, m, new Map());
  equal(settleZones(st1.nodes.map((n) => (n.id === "a" ? { ...n, position: { x: 300, y: 48 } } : n)), m, st1.seen).nodes.find((n) => n.id === "s")!.width, 720);
}

// ---- seatIn: what comes into an area and lands on something there is put beside it, on the side it was let go
{
  // `held`: what the area shows already; `n` came in at `drop` (in the area), the pointer let go at `at` (the area is at 0, 0)
  const seat = (held: ZNode[], drop: [number, number], at: [number, number], z: ZNode = zone("z", 0, 0, 576, 360)) =>
    get(seatIn([z, ...held, block("n", "z", drop[0], drop[1])], ["n"], "z", { x: at[0], y: at[1] }, m, 24), "n").position;
  const one = [block("a", "z", 24, 48)];
  // on free room: it stays where it was let go
  deepStrictEqual(seat(one, [24, 144], [100, 170]), { x: 24, y: 144 });
  deepStrictEqual(seat([], [24, 48], [100, 80]), { x: 24, y: 48 });
  // on the block there: right, left, under, above it (centred on it), by where the pointer was from its middle
  deepStrictEqual(seat(one, [150, 60], [250, 84]), { x: 288, y: 48 });
  deepStrictEqual(seat(one, [0, 60], [40, 84]), { x: -240, y: 48 });
  deepStrictEqual(seat(one, [30, 90], [144, 110]), { x: 24, y: 144 });
  deepStrictEqual(seat(one, [30, 20], [144, 60]), { x: 24, y: -48 });
  // a corner of something wide: above or under it, at that end
  const wide = [block("a", "z", 24, 48), block("b", "z", 288, 48)];
  deepStrictEqual(seat(wide, [300, 60], [500, 115]), { x: 288, y: 144 });
  deepStrictEqual(seat(wide, [0, 20], [50, 55]), { x: 24, y: -48 });
  // a corner of something tall: beside it, at that end
  const tallOnes = [block("a", "z", 24, 48), block("b", "z", 24, 144), block("c", "z", 24, 240)];
  deepStrictEqual(seat(tallOnes, [100, 260], [250, 300]), { x: 288, y: 240 });
  deepStrictEqual(seat(tallOnes, [0, 30], [40, 60]), { x: -240, y: 48 });
  // the very middle of a wide one: under it, centred (on the grid)
  deepStrictEqual(seat(wide, [150, 60], [276, 84]), { x: 168, y: 144 });
  // what another step holds isn't there now: nothing to land on
  const stepZone = phased(zone("z", 0, 0, 576, 360), [P("p1"), P("p2")], "p2");
  deepStrictEqual(seat([only(block("a", "z", 24, 48), "p1")], [30, 60], [100, 80], stepZone), { x: 30, y: 60 });
  // several come in as one piece: they keep their places among themselves
  const two = seatIn([zone("z", 0, 0, 576, 360), block("a", "z", 24, 48), block("n1", "z", 100, 60), block("n2", "z", 100, 156)], ["n1", "n2"], "z", { x: 250, y: 84 }, m, 24);
  deepStrictEqual([get(two, "n1").position, get(two, "n2").position], [{ x: 288, y: 0 }, { x: 288, y: 96 }]);
  // a stack places its own blocks; an area that isn't there changes nothing
  const stk = [{ ...zone("z", 0, 0, 288, 144), data: { title: "z", stack: true } }, block("a", "z", 24, 48), block("n", "z", 30, 60)];
  equal(seatIn(stk, ["n"], "z", { x: 250, y: 84 }, m, 24), stk);
  equal(seatIn(stk, ["n"], "nope", { x: 250, y: 84 }, m, 24), stk);
}

// lines: hidden with an end, or by their own steps (read against the area both ends are in)
deepStrictEqual([commonPhased(nestedIn, "one", "two"), commonPhased(nestedIn, "deep", "always"), commonPhased(nestedIn, "one", "free")], ["z", "z", undefined]);
const gone = hiddenByPhase(flow);
equal(lineHiddenByPhase(flow, gone, { source: "one", target: "always" }), true);
equal(lineHiddenByPhase(flow, gone, { source: "two", target: "always" }), false);
equal(lineHiddenByPhase(flow, gone, { source: "two", target: "always", when: ["p1"], whenZone: "z" }), true);
equal(lineHiddenByPhase(flow, gone, { source: "two", target: "always", when: ["p2"], whenZone: "z" }), false);
equal(lineHiddenByPhase(flow, gone, { source: "two", target: "always", when: ["p1"], whenZone: "nowhere" }), false, "its area is gone: it shows");

// ---- wrapZone: an area made around nodes where they are (the agents' area)
const cards = [block("lead", undefined, 240, 0, 672, 504), block("w1", undefined, 0, 600, 336, 264), block("w2", undefined, 384, 600, 336, 264), block("note", undefined, 2000, 0), block("inZone", "other", 24, 48), zone("other", 3000, 0, 432, 288)];
const team = wrapZone(cards, ["lead", "w1", "w2", "inZone"], zone("team", 0, 0, 0, 0), m);
equal(team[0].id, "team", "the area comes first: a parent before its children");
deepStrictEqual([get(team, "team").position, get(team, "team").width, get(team, "team").height], [{ x: -24, y: -48 }, 960, 936]);
deepStrictEqual([get(team, "lead").parentId, get(team, "w1").parentId, get(team, "note").parentId, get(team, "inZone").parentId], ["team", "team", undefined, "other"], "only what was named and at the top level");
for (const id of ["lead", "w1", "w2"]) deepStrictEqual(abs(team, id), abs(cards, id), `${id} stays where it was`);
equal(growZones(team, m), team, "it holds them as it is");
equal(wrapZone(cards, ["nope"], zone("team", 0, 0, 0, 0), m), cards);
deepStrictEqual(boxOf(team, "w2", m), { x: 384, y: 600, w: 336, h: 264 });
equal(boxOf(team, "nope", m), null);

// ---- stacks: a column, exactly as big as what it holds
{
const GAP = 8;
const stack = (id: string, x: number, y: number, parentId?: string): ZNode => ({ id, type: "zone", parentId, position: { x, y }, width: 0, height: 0, data: { title: id, stack: true } });
// a block dropped on a block: the stack takes the target's place, the target first, the dropped one under it
const loose = [block("a", undefined, 100, 200), block("b", undefined, 130, 210, 336, 96), block("far", undefined, 900, 0)];
const made = stackOnto(loose, ["b"], "a", stack("s", 0, 0), m, GAP);
equal(made[0].id, "s", "the stack comes before what it holds");
deepStrictEqual([get(made, "s").position, get(made, "s").width, get(made, "s").height], [{ x: 76, y: 152 }, 384, 264], "as wide as the widest, as tall as both");
deepStrictEqual([abs(made, "a"), get(made, "a").parentId, get(made, "b").position, get(made, "far").parentId], [{ x: 100, y: 200 }, "s", { x: 24, y: 128 }, undefined], "the target stays where it was");
ok(isStack(get(made, "s")) && !isStack(get(made, "a")));
equal(layStacks(made, m, GAP), made, "already laid out");
equal(dropInto(made, ["b"], { x: 150, y: 230 }, m, can), made, "let go inside the stack: still in it");
equal(growZones(made, m), made);
equal(stackOnto(loose, ["a"], "a", stack("s", 0, 0), m, GAP), loose, "nothing dropped");
equal(stackOnto(loose, ["b"], "nope", stack("s", 0, 0), m, GAP), loose);
// in an area with steps: the stack is where the target was, in the target's step, and its blocks no longer say when
const inStep = [phased(zone("z", 0, 0, 720, 480), [P("p1"), P("p2")], "p1"), only(block("a", "z", 48, 72), "p1"), block("b", undefined, 60, 80)];
const stepStack = stackOnto(inStep, ["b"], "a", stack("s", 0, 0), m, GAP);
deepStrictEqual([stepStack.map((n) => n.id), get(stepStack, "s").parentId, whenOf(stepStack, "s"), "when" in get(stepStack, "a").data, get(stepStack, "b").parentId], [["z", "s", "a", "b"], "z", ["p1"], false, "s"]);
// a block dropped into a stack lands between the two its middle is between; the stack grows for it
const third = layStacks([...made, block("c", "s", 300, 100, 240, 72)], m, GAP);
deepStrictEqual([get(third, "a").position.y, get(third, "c").position, get(third, "b").position.y, get(third, "s").height], [48, { x: 24, y: 128 }, 208, 336]);
// one taken out: the rest close up and the stack shrinks (a plain area wouldn't)
const out = layStacks(third.map((n) => (n.id === "a" ? { ...n, parentId: undefined, position: { x: 2000, y: 0 } } : n)), m, GAP);
deepStrictEqual([get(out, "c").position.y, get(out, "b").position.y, get(out, "s").height, get(out, "s").width], [48, 128, 264, 384]);
// the last one taken out: the stack goes; a stack that waits for a block's size is left as it is; a folded one too
const empty = layStacks(out.map((n) => (n.parentId === "s" ? { ...n, parentId: undefined } : n)), m, GAP);
equal(empty.some((n) => n.id === "s"), false);
const unsized = [...made, { id: "new", type: "markdown_block", parentId: "s", position: { x: 0, y: 0 }, data: {} } as ZNode];
equal(layStacks(unsized, m, GAP), unsized);
const foldedStack = foldZone(third, "s", true, m);
equal(layStacks(foldedStack, m, GAP), foldedStack);
// a stack in a stack: the inner one is sized first, then the outer one around it
const nested = layStacks([stack("outer", 0, 0), block("top", "outer", 24, 48), stack("inner", 24, 300, "outer"), block("x", "inner", 0, 0), block("y", "inner", 0, 500)], m, GAP);
deepStrictEqual([get(nested, "inner").height, get(nested, "inner").position.y, get(nested, "outer").height, get(nested, "y").position.y], [240, 128, 408, 128]);
// lines: a block in a stack stands for its stack (the outermost one)
deepStrictEqual([...stackedInto(nested)].sort(), [["inner", "outer"], ["top", "outer"], ["x", "outer"], ["y", "outer"]]);
equal(stackedInto(loose).size, 0);
}

// ---- tidying: what an area holds put straight (배치 정리)
{
  const can = (n: ZNode) => n.type === "struct_block";
  const laid = (ns: ZNode[], ...ids: string[]) => ids.map((id) => [get(ns, id).position.x, get(ns, id).position.y, get(ns, id).width, get(ns, id).height]);
  // rows are read from the top, each from the left: a box overlapping a row's first one by half is in that row
  deepStrictEqual(rowsOf([{ x: 300, y: 10, w: 100, h: 50 }, { x: 0, y: 0, w: 100, h: 50 }, { x: 0, y: 100, w: 100, h: 50 }, { x: 200, y: 120, w: 100, h: 50 }]), [[1, 0], [2, 3]]);
  deepStrictEqual(rowsOf([]), []);
  // a ragged area (two rows off their lines, blocks of different sizes): rows on a line, a cell apart, the blocks as
  // wide as the widest and as tall as the tallest of their row, and the area fitted around them
  const ragged = [zone("z", 0, 0, 900, 600), block("a", "z", 30, 60), block("b", "z", 320, 75, 200, 100), block("c", "z", 40, 260), block("d", "z", 500, 280)];
  const neat = tidyArea(ragged, "z", m, 24, can);
  deepStrictEqual(laid(neat, "a", "b", "c", "d"), [[24, 48, 240, 100], [288, 48, 240, 100], [24, 172, 240, 72], [288, 172, 240, 72]]);
  deepStrictEqual([get(neat, "z").width, get(neat, "z").height], [552, 288]);
  deepStrictEqual(tidyArea(neat, "z", m, 24, can), neat, "tidied twice is tidied once");
  // rows that were centred on each other (a tree drawn top down) stay centred
  const tree = tidyArea([zone("z", 0, 0, 900, 600), block("r", "z", 156, 48), block("x", "z", 24, 200), block("y", "z", 288, 200)], "z", m, 24, can);
  deepStrictEqual(laid(tree, "r", "x", "y").map((p) => [p[0], p[1]]), [[168, 48], [24, 144], [288, 144]]);
  // what can't be given a size keeps its own, and doesn't make the blocks beside it taller
  const mixed = tidyArea([zone("z", 0, 0, 900, 600), block("a", "z", 30, 60), { ...block("q", "z", 320, 60, 312, 200), type: "question_block" }], "z", m, 24, can);
  deepStrictEqual(laid(mixed, "a", "q"), [[24, 48, 240, 72], [288, 48, 312, 200]]);
  // left alone: an area with steps (what it holds may share a place), a stack, an area that isn't there
  const stepZ = [phased(zone("z", 0, 0, 900, 600), [P("p1"), P("p2")], "p1"), block("a", "z", 30, 60), block("b", "z", 35, 70)];
  equal(tidyArea(stepZ, "z", m, 24, can), stepZ);
  const stk = [{ ...zone("z", 0, 0, 900, 600), data: { title: "z", stack: true } }, block("a", "z", 30, 60)];
  equal(tidyArea(stk, "z", m, 24, can), stk);
  equal(tidyArea(ragged, "nope", m, 24, can), ragged);
}

console.log("zones-check: ok");
