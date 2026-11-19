// 흐름 (docs/features/canvas/canvas-flows.md): what an agent showed on a canvas with its lines, kept so it can be
// looked at again. A flow is the steps shown one after another (each a line and what it showed); the canvas saves
// them with its file. Pure, so scripts/focus-check.ts runs it in Node.

/** One step: the line said, what it showed (node ids, to go back there) and by what names (for an agent reading it). */
export type FlowStep = { say: string; ids: string[]; names: string[] };
/** `at`: when it began, `end`: when its last step was shown (ms). `summary`: a line about it, when someone gave one. */
/**
 * `named`: its title was given (the agent's `--play`, or the user renamed it), so nothing names it again. `titled`:
 * how many steps it had when the assistant model last named it from its lines (named again once it has more).
 */
export type Flow = { id: string; title: string; summary?: string; at: number; end: number; steps: FlowStep[]; source?: string[]; named?: true; titled?: number };

/** Steps shown within this long of each other are one flow; a step after a longer quiet starts a new one. */
export const FLOW_GAP = 3 * 60_000;
/** The canvas keeps this many flows (the oldest go). */
export const FLOWS_MAX = 20;
const TITLE_MAX = 40;

// (until it has a better one: its first line, without the number an agent puts in front, "1. ")
const titleOf = (line: string) => {
  const say = line.replace(/^\s*\d+\s*[.)]\s*/, "").trim() || line.trim();
  return say.length > TITLE_MAX ? `${say.slice(0, TITLE_MAX - 1).trimEnd()}…` : say;
};

/** The flows the assistant model should name now: not given a name, and grown since it last named them. */
export const untitled = (flows: readonly Flow[]): Flow[] => flows.filter((f) => !f.named && f.titled !== f.steps.length);

/** A flow named from its lines by the assistant model, with how many steps it had then. */
export const autoTitled = (flows: readonly Flow[], id: string, title: string, steps: number): Flow[] =>
  flows.map((f) => (f.id === id && !f.named ? { ...f, title: title.trim().slice(0, TITLE_MAX) || f.title, titled: steps } : f));

/**
 * A step just shown, added to the flows: to the newest one when that was still going (`FLOW_GAP`), else as the
 * first step of a new flow named by its line. `play`: the agent named a play starting here (`--play`): a new flow
 * by that name, whatever came just before. A step with no line is not a step of a flow.
 */
export function record(flows: readonly Flow[], step: FlowStep, now: number, id: string, play?: string, source: readonly string[] = []): Flow[] {
  const say = step.say.trim();
  if (!say) return flows as Flow[];
  const one = { say, ids: [...step.ids], names: [...step.names] };
  const name = play?.trim();
  // (`source`: what the play explains, `--source`; added to what it already had)
  const withSource = (f: Flow): Flow => (source.length ? { ...f, source: [...new Set([...(f.source ?? []), ...source])] } : f);
  const last = flows[flows.length - 1];
  if (!name && last && now - last.end < FLOW_GAP) return [...flows.slice(0, -1), withSource({ ...last, end: now, steps: [...last.steps, one] })];
  return [...flows, withSource({ id, title: name ? titleOf(name) : titleOf(say), at: now, end: now, steps: [one], ...(name ? { named: true as const } : {}) })].slice(-FLOWS_MAX);
}

/** A flow given another name (an empty one: named by its first line again). */
export const retitle = (flows: readonly Flow[], id: string, title: string): Flow[] =>
  flows.map((f) => {
    if (f.id !== id) return f;
    const t = title.trim().slice(0, 60);
    // (emptied: back to its first line, and to being named from its lines again)
    if (!t) {
      const { named: _n, titled: _t, ...rest } = f;
      return { ...rest, title: titleOf(f.steps[0]?.say ?? "") };
    }
    return { ...f, title: t, named: true as const };
  });

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
      const source = list(f.source);
      return [{ id: f.id, title: text(f.title) || titleOf(steps[0].say), ...(text(f.summary) ? { summary: f.summary } : {}), at, end: Number(f.end) || at, steps, ...(source.length ? { source } : {}), ...(f.named === true ? { named: true as const } : {}), ...(typeof f.titled === "number" ? { titled: f.titled } : {}) }];
    })
    .slice(-FLOWS_MAX);
}
