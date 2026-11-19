// 흐름 (docs/features/canvas/canvas-flows.md): what an agent showed on a canvas with its lines, kept so it can be
// looked at again. A flow is the steps shown one after another (each a line and what it showed); the canvas saves
// them with its file. Pure, so scripts/focus-check.ts runs it in Node.

/** One step: the line said, what it showed (node ids, to go back there) and by what names (for an agent reading it). */
export type FlowStep = { say: string; ids: string[]; names: string[] };
/** `at`: when it began, `end`: when its last step was shown (ms). `summary`: a line about it, when someone gave one. */
export type Flow = { id: string; title: string; summary?: string; at: number; end: number; steps: FlowStep[] };

/** Steps shown within this long of each other are one flow; a step after a longer quiet starts a new one. */
export const FLOW_GAP = 3 * 60_000;
/** The canvas keeps this many flows (the oldest go). */
export const FLOWS_MAX = 20;
const TITLE_MAX = 40;

const titleOf = (say: string) => (say.length > TITLE_MAX ? `${say.slice(0, TITLE_MAX - 1).trimEnd()}…` : say);

/**
 * A step just shown, added to the flows: to the newest one when that was still going (`FLOW_GAP`), else as the
 * first step of a new flow named by its line. A step with no line is not a step of a flow.
 */
export function record(flows: readonly Flow[], step: FlowStep, now: number, id: string): Flow[] {
  const say = step.say.trim();
  if (!say) return flows as Flow[];
  const one = { say, ids: [...step.ids], names: [...step.names] };
  const last = flows[flows.length - 1];
  if (last && now - last.end < FLOW_GAP) return [...flows.slice(0, -1), { ...last, end: now, steps: [...last.steps, one] }];
  return [...flows, { id, title: titleOf(say), at: now, end: now, steps: [one] }].slice(-FLOWS_MAX);
}

/** A flow given another name (an empty one: named by its first line again). */
export const retitle = (flows: readonly Flow[], id: string, title: string): Flow[] =>
  flows.map((f) => (f.id === id ? { ...f, title: title.trim().slice(0, 60) || titleOf(f.steps[0]?.say ?? "") } : f));

export const without = (flows: readonly Flow[], id: string): Flow[] => flows.filter((f) => f.id !== id);

/** What was read from a canvas file as its flows, with anything that isn't one left out. */
export function flowsFrom(saved: unknown): Flow[] {
  if (!Array.isArray(saved)) return [];
  const text = (v: unknown) => (typeof v === "string" ? v : "");
  const list = (v: unknown) => (Array.isArray(v) ? v.filter((x): x is string => typeof x === "string") : []);
  return saved
    .flatMap((f): Flow[] => {
      const steps = Array.isArray(f?.steps) ? (f.steps as any[]).filter((s) => text(s?.say).trim()).map((s) => ({ say: text(s.say), ids: list(s.ids), names: list(s.names) })) : [];
      if (!text(f?.id) || !steps.length) return [];
      const at = Number(f.at) || 0;
      return [{ id: f.id, title: text(f.title) || titleOf(steps[0].say), ...(text(f.summary) ? { summary: f.summary } : {}), at, end: Number(f.end) || at, steps }];
    })
    .slice(-FLOWS_MAX);
}
