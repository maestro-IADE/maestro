// The pictures the chat shows of what an agent puts up for its canvas (docs/features/agents/chat-visuals.md): made
// from the canvas itself when it is open, and when it isn't, from the queued things (drawn on an empty sheet) or,
// for a thing the agent only points at, from the canvas's saved file.
import { GRID } from "./nodes/AgentNode";
import { BLOCK_SIZE, STACK_GAP, ZONE_METRICS } from "./nodes/StructNodes";
import { QUESTION_WIDTH } from "./nodes/QuestionBlock";
import { applyAsk, applyAskSet, applyDrawing, applyShow, type Drawing } from "./drawing";
import { foldedInto, growZones, hiddenByPhase, layStacks } from "./zones";
import { miniOf, type Mini } from "./canvasMini";
import { ipc } from "./ipc";
import type { Queued } from "./visualSteps";
import type { CanvasEdge, CanvasNode } from "./types";

export type Sheet = { nodes: CanvasNode[]; edges: CanvasEdge[] };

/** (a block not measured yet: about what it will be) */
const sizeOf = (n: CanvasNode) => {
  const guess = n.type === "question_block" ? { width: QUESTION_WIDTH, height: 10 * GRID } : BLOCK_SIZE;
  return { w: n.width ?? n.measured?.width ?? guess.width, h: n.height ?? n.measured?.height ?? guess.height };
};

/** What queued things bring, put onto a sheet (what they only do, a pointer, a tidy, an erase, is left out). */
export function applyBrought(items: readonly Queued[], owner: string, sheet: Sheet): Sheet {
  let { nodes: ns, edges: es } = sheet;
  for (const d of items) {
    if (d.ask) ns = applyAsk(d.ask, owner, ns);
    else if (d.ask_set) ns = applyAskSet(d.ask_set, owner, ns);
    else if (d.show) ns = applyShow(d.show, ns);
    else if (Array.isArray(d.areas) || Array.isArray(d.blocks) || Array.isArray(d.arrows)) ({ nodes: ns, edges: es } = applyDrawing(d as Drawing, ns, es));
  }
  return { nodes: ns, edges: es };
}

/** The blocks, areas, questions and view blocks of these names (what `maestro focus` can point at). */
export const named = (ns: readonly CanvasNode[], names: readonly string[]) =>
  ns.filter((n) => (n.type === "struct_block" || n.type === "zone" || n.type === "question_block" || n.type === "view_block") && names.includes(((n.data as { title?: string }).title ?? "").trim())).map((n) => n.id);

/** The picture of some nodes of a sheet with everything in them; null when none of them is there or shows. */
export function pictureOf(sheet: Sheet, ids: readonly string[]): Mini | null {
  if (!ids.length) return null;
  const nodes = growZones(layStacks(sheet.nodes, ZONE_METRICS, STACK_GAP), ZONE_METRICS);
  const only = new Set(ids);
  for (let grew = true; grew; ) {
    grew = false;
    for (const n of nodes) {
      if (!n.parentId || !only.has(n.parentId) || only.has(n.id)) continue;
      only.add(n.id);
      grew = true;
    }
  }
  const hidden = new Set([...foldedInto(nodes).keys(), ...hiddenByPhase(nodes)]);
  const mini = miniOf(nodes, sheet.edges, sizeOf, hidden, 12, only);
  return mini.boxes.length ? mini : null;
}

/** The picture of what queued things bring, drawn on an empty sheet (no canvas is open to say how it really falls). */
export function pictureOfItems(items: readonly Queued[], owner: string): Mini | null {
  try {
    const sheet = applyBrought(items, owner, { nodes: [], edges: [] });
    return pictureOf(sheet, sheet.nodes.map((n) => n.id));
  } catch {
    return null;
  }
}

/**
 * The picture of what a step points at (`maestro focus`) on a canvas that isn't open: the canvas as it was saved
 * (`root`, `key`: which file), with what waits before the step put on top, since the thing may have been drawn a
 * moment ago. Null when nothing of that name is there.
 */
export async function pictureOfPointed(root: string, key: string, owner: string, before: readonly Queued[], names: readonly string[]): Promise<Mini | null> {
  let sheet: Sheet = { nodes: [], edges: [] };
  try {
    const saved = await ipc.loadCanvas(root, key);
    const state = saved ? (JSON.parse(saved) as Partial<Sheet>) : null;
    if (state && Array.isArray(state.nodes) && Array.isArray(state.edges)) sheet = { nodes: state.nodes, edges: state.edges };
    sheet = applyBrought(before, owner, sheet);
    return pictureOf(sheet, named(sheet.nodes, names));
  } catch {
    return null;
  }
}
