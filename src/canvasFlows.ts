// 흐름 (docs/features/canvas/canvas-flows.md): what an agent showed on a canvas with its lines, kept so it can be
// looked at again. A flow is the steps shown one after another (each a line and what it showed); the canvas saves
// them with its file. Pure, so scripts/focus-check.ts runs it in Node.

/** One step: the line said, what it showed (node ids, to go back there) and by what names (for an agent reading it). */
/** `at`: the step or state it put areas at (area id → step/state id), when it moved one (`maestro step`/`state`). */
export type FlowStep = { say: string; ids: string[]; names: string[]; at?: Record<string, string> };
/** `at`: when it began, `end`: when its last step was shown (ms). `summary`: a line about it, when someone gave one. */
/**
 * `named`: its title was given (the agent's `--play`, or the user renamed it), so nothing names it again. `titled`:
 * how many steps it had when the assistant model last named it from its lines (named again once it has more).
 */
export type Flow = {
  id: string;
  title: string;
  summary?: string;
  at: number;
  end: number;
  steps: FlowStep[];
  source?: string[];
  named?: true;
  titled?: number;
  /** its steps went the way the arrows between what they show go, when that was first seen (`syncFlows`): then a
   *  change of the arrows' order changes its order too */
  follows?: boolean;
};

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
  const one: FlowStep = { say, ids: [...step.ids], names: [...step.names], ...(step.at && Object.keys(step.at).length ? { at: { ...step.at } } : {}) };
  const name = play?.trim();
  // (`source`: what the play explains, `--source`; added to what it already had)
  const withSource = (f: Flow): Flow => (source.length ? { ...f, source: [...new Set([...(f.source ?? []), ...source])] } : f);
  const last = flows[flows.length - 1];
  if (!name && last && now - last.end < FLOW_GAP) return [...flows.slice(0, -1), withSource({ ...last, end: now, steps: [...last.steps, one] })];
  return [...flows, withSource({ id, title: name ? titleOf(name) : titleOf(say), at: now, end: now, steps: [one], ...(name ? { named: true as const } : {}) })].slice(-FLOWS_MAX);
}

type SN = { id: string; parentId?: string; data: Record<string, unknown> };
type SE = { source: string; target: string; data?: { kind?: string } };
const nameOf = (n: SN | undefined) => (typeof n?.data.title === "string" ? n.data.title.trim() : "");
const same = (a: readonly number[]) => a.every((v, i) => v === i);

/**
 * The one order the arrows give a flow's steps (their indices), or null when they don't give exactly one. A step
 * owns what it shows and what sits inside that; step a comes before b when an arrow or relation line goes from
 * something of a's to something of b's. One order only when every step is on one way through them all (a unique
 * topological order, which is a path through every step).
 */
export function arrowOrder(targets: readonly (readonly string[])[], nodes: readonly SN[], edges: readonly SE[]): number[] | null {
  if (targets.length < 2 || targets.some((t) => !t.length)) return null;
  const kids = new Map<string, string[]>();
  for (const n of nodes) if (n.parentId) kids.set(n.parentId, [...(kids.get(n.parentId) ?? []), n.id]);
  const own = targets.map((t) => {
    const s = new Set<string>();
    const walk = (id: string) => {
      if (s.has(id)) return;
      s.add(id);
      for (const c of kids.get(id) ?? []) walk(c);
    };
    t.forEach(walk);
    return s;
  });
  const owners = (id: string) => own.flatMap((s, i) => (s.has(id) ? [i] : []));
  const next = targets.map(() => new Set<number>());
  for (const e of edges) {
    const k = e.data?.kind;
    if (k === "orchestrates" || k === "command_pipe" || k === "reference") continue;
    for (const a of owners(e.source)) for (const b of owners(e.target)) if (a !== b) next[a].add(b);
  }
  const indeg = targets.map((_, i) => next.reduce((c, s) => c + (s.has(i) ? 1 : 0), 0));
  const order: number[] = [];
  let ready = indeg.flatMap((d, i) => (d === 0 ? [i] : []));
  while (ready.length === 1) {
    const i = ready[0];
    order.push(i);
    ready = [];
    for (const b of next[i]) if (--indeg[b] === 0) ready.push(b);
  }
  return order.length === targets.length ? order : null;
}

/**
 * The flows kept up with the canvas as it is now: a step shows what is there by its current name (one that was
 * drawn anew under the same name is found by it), what is gone is left out (a step left with nothing to show goes,
 * and a flow left with no steps), and a flow that went the way its arrows go follows them when they change order.
 * The same array back when nothing changed.
 */
export function syncFlows(flows: readonly Flow[], nodes: readonly SN[], edges: readonly SE[]): Flow[] {
  const byId = new Map(nodes.map((n) => [n.id, n]));
  let changed = false;
  const out = flows.flatMap((f): Flow[] => {
    let steps = f.steps.flatMap((st): FlowStep[] => {
      if (!st.ids.length) return [st]; // (a line on its own, or one whose names were never found)
      const kept = st.ids.filter((id) => byId.has(id));
      const keptNames = new Set(kept.map((id) => nameOf(byId.get(id))));
      const found = kept.length < st.ids.length
        ? st.names.filter((nm) => nm && !keptNames.has(nm)).flatMap((nm) => {
            const m = nodes.find((n) => nameOf(n) === nm && !kept.includes(n.id));
            return m ? [m.id] : [];
          })
        : [];
      const ids = [...new Set([...kept, ...found])];
      if (!ids.length) return [];
      const at = st.at ? Object.fromEntries(Object.entries(st.at).filter(([z]) => byId.has(z))) : undefined;
      return [{ say: st.say, ids, names: ids.map((id) => nameOf(byId.get(id))).filter(Boolean), ...(at && Object.keys(at).length ? { at } : {}) }];
    });
    if (!steps.length) {
      changed = true;
      return [];
    }
    const now = arrowOrder(steps.map((s) => s.ids), nodes, edges);
    let follows = f.follows;
    if (follows === undefined && now) follows = same(now);
    else if (follows && now && !same(now)) steps = now.map((i) => steps[i]);
    const next: Flow = { ...f, steps, ...(follows === undefined ? {} : { follows }) };
    if (JSON.stringify(next) === JSON.stringify(f)) return [f];
    changed = true;
    return [next];
  });
  return changed ? out : (flows as Flow[]);
}

// ---- areas with steps or states (zones.ts; kept to plain data here so this file has no imports)
type PN = { id: string; type?: string; parentId?: string; data: Record<string, unknown> };
const phasesIn = (z: PN | undefined): string[] =>
  z?.type === "zone" && z.data.mode && Array.isArray(z.data.phases) ? (z.data.phases as { id?: unknown }[]).flatMap((p) => (typeof p?.id === "string" ? [p.id] : [])) : [];
const currentIn = (z: PN | undefined): string | undefined => {
  const ps = phasesIn(z);
  return ps.includes(z?.data.at as string) ? (z!.data.at as string) : ps[0];
};

/**
 * What a step or state of an area brings into sight: what in it shows then and not in the step before (steps), or
 * shows in only some states (states). The area itself when nothing does.
 */
export function revealedBy(nodes: readonly PN[], zoneId: string, phase: string): string[] {
  const z = nodes.find((n) => n.id === zoneId);
  const ps = phasesIn(z), i = ps.indexOf(phase);
  if (!z || i < 0) return [zoneId];
  const prev = z.data.mode === "step" && i > 0 ? ps[i - 1] : undefined;
  const out = nodes.filter((n) => {
    const w = n.parentId === zoneId && Array.isArray(n.data.when) ? (n.data.when as string[]) : null;
    return !!w && w.includes(phase) && (z.data.mode === "step" ? !prev || !w.includes(prev) : w.length < ps.length);
  });
  return out.length ? out.map((n) => n.id) : [zoneId];
}

/**
 * A step that moved areas' steps or states (`before` → `after` the move): where it put each (`at`) and what that
 * brought into sight (`ids`), to keep with the play. Null when it moved none.
 */
export function phaseStep(before: readonly PN[], after: readonly PN[]): { at: Record<string, string>; ids: string[] } | null {
  const was = new Map(before.map((n) => [n.id, currentIn(n)]));
  const at: Record<string, string> = {};
  for (const z of after) {
    const now = currentIn(z);
    if (now && phasesIn(z).length && was.get(z.id) !== now) at[z.id] = now;
  }
  const zs = Object.keys(at);
  return zs.length ? { at, ids: [...new Set(zs.flatMap((z) => revealedBy(after, z, at[z])))] } : null;
}

/** A step as a play goes again: what it points at, and the step or state each area is put at first. */
export type PlayStep = { say: string; ids: string[]; at?: Record<string, string> };

/**
 * A flow's steps as it goes again. A step kept with `at` puts its areas there. A run of steps kept before that (each
 * only an area with steps, one after another) is read as the agent going through that area's last steps in order,
 * each showing what its step brings.
 */
export function playSteps(f: Flow, nodes: readonly PN[]): PlayStep[] {
  const byId = new Map(nodes.map((n) => [n.id, n]));
  const lone = (st: FlowStep) => {
    const z = st.ids.length === 1 && !st.at ? byId.get(st.ids[0]) : undefined;
    return z && z.data.mode === "step" && phasesIn(z).length ? z.id : null;
  };
  const out: PlayStep[] = [];
  for (let i = 0; i < f.steps.length; ) {
    const z = lone(f.steps[i]);
    if (!z) {
      const st = f.steps[i++];
      out.push({ say: st.say, ids: st.ids, ...(st.at ? { at: st.at } : {}) });
      continue;
    }
    let j = i;
    while (j < f.steps.length && lone(f.steps[j]) === z) j++;
    const ps = phasesIn(byId.get(z));
    for (let k = 0; k < j - i; k++) {
      const p = ps[Math.max(0, ps.length - (j - i) + k)];
      out.push({ say: f.steps[i + k].say, ids: revealedBy(nodes, z, p), at: { [z]: p } });
    }
    i = j;
  }
  return out;
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
      const atOf = (v: unknown) => {
        const e = v && typeof v === "object" ? Object.entries(v as Record<string, unknown>).filter((x): x is [string, string] => typeof x[1] === "string") : [];
        return e.length ? { at: Object.fromEntries(e) } : {};
      };
      const steps = Array.isArray(f?.steps) ? (f.steps as any[]).filter((s) => text(s?.say).trim()).map((s) => ({ say: text(s.say), ids: list(s.ids), names: list(s.names), ...atOf(s.at) })) : [];
      if (!text(f?.id) || !steps.length) return [];
      const at = Number(f.at) || 0;
      const source = list(f.source);
      return [{ id: f.id, title: text(f.title) || titleOf(steps[0].say), ...(text(f.summary) ? { summary: f.summary } : {}), at, end: Number(f.end) || at, steps, ...(source.length ? { source } : {}), ...(f.named === true ? { named: true as const } : {}), ...(typeof f.titled === "number" ? { titled: f.titled } : {}), ...(typeof f.follows === "boolean" ? { follows: f.follows } : {}) }];
    })
    .slice(-FLOWS_MAX);
}
