// What an agent puts up for its canvas, as the chat shows it (docs/features/agents/chat-visuals.md): the queue cut into
// steps, each with what the card needs, and which of them the chat shows. Pure, so scripts/focus-check.ts runs it in
// Node; the store that holds it is canvasGate.ts.
import type { Step } from "./canvasGuide";
import type { Mini } from "./canvasMini";
import type { Ask, AskSet, Drawing, Erase, PhaseMove, Show } from "./drawing";

/** Something an agent queued for its canvas (control.rs `enqueue`): `seq` is the number the backend gave it. */
export type Queued = Partial<Drawing> & { erase?: Erase; ask?: Ask; phase?: PhaseMove; ask_set?: AskSet; show?: Show; focus?: { title: string }; tidy?: { area?: string | null }; say?: string; play?: string; seq?: number };

/** A step as the chat shows it. */
export type Visual = {
  /** `s<n>`: a step an open canvas holds back; `q<seq>`: one that waits in the backend */
  id: string;
  say?: string;
  /** the queued things themselves */
  items: Queued[];
  /** it brings something onto the canvas, so it can be thrown away (a line on its own, a tidy, a pointer can't) */
  brings: boolean;
  /** the names it points the view at (`maestro focus`) */
  points: string[];
  /** the numbers of its queued things */
  seqs: number[];
  /** what it is in a few words: for the card, and for the note to the agent when it is thrown away */
  what: string;
  /** its picture, when an open canvas made it from what is really there (otherwise the chat draws one from `items`) */
  mini?: Mini | null;
};

export const keyOf = (path: string) => path.replace(/\\/g, "/").replace(/\/+$/, "").toLowerCase();

/** Whether a queued thing puts something on the canvas. */
export const brings = (d: Queued) => !!d.ask || !!d.ask_set || !!d.show || Array.isArray(d.areas) || Array.isArray(d.blocks) || Array.isArray(d.arrows);

/** What a step is, in a few words: the names of what it brings or points at (its line says it better, when it has one). */
export function namesOf(items: readonly Queued[]): string {
  const base = (p: string) => p.split(/[\\/]/).pop() ?? p;
  const names = items.flatMap((d) => [
    ...(d.areas ?? []).map((a) => a.title),
    ...(d.blocks ?? []).map((b) => b.title),
    ...(d.ask ? [d.ask.title] : []),
    ...(d.ask_set ? [d.ask_set.title?.trim() || `질문 ${d.ask_set.questions.length}개`] : []),
    ...(d.show ? [d.show.title?.trim() || base(d.show.path)] : []),
    ...(d.focus ? [d.focus.title] : []),
  ]).map((t) => (t ?? "").trim()).filter(Boolean);
  const first = names.slice(0, 3).map((t) => `'${t}'`).join(", ");
  return names.length > 3 ? `${first} 외 ${names.length - 3}개` : first;
}

/** A step of the queue as the chat shows it. */
export function visualOf(id: string, step: Step<Queued>): Visual {
  const items = step.items;
  return {
    id,
    say: step.say,
    items,
    brings: items.some(brings),
    points: items.flatMap((d) => (d.focus ? [d.focus.title.trim()] : [])),
    seqs: items.flatMap((d) => (typeof d.seq === "number" ? [d.seq] : [])),
    what: step.say ?? namesOf(items),
  };
}

export type Gate = {
  /** the agents whose canvas is on screen */
  open: Record<string, boolean>;
  /** the steps an open canvas holds back (눌러서), next first */
  waiting: Record<string, Visual[]>;
  /** the steps that wait in the backend for a canvas that isn't open, oldest first */
  queued: Record<string, Visual[]>;
  /** 캔버스에 올리기 pressed while the canvas wasn't open: that many of the first steps come in when it opens */
  passes: Record<string, number>;
  /** the chat's card closed without a decision: queued things up to this number aren't shown again */
  closed: Record<string, number>;
  /** the user named a playbook for this agent's turn: what it shows comes in without a press (설정이 눌러서여도) */
  loose: Record<string, boolean>;
};

const NONE: Visual[] = [];
const lastSeq = (v: Visual) => Math.max(0, ...v.seqs);

/**
 * What the chat shows for an agent, from the gate's state. `byHand`: steps wait for the user (설정 → 시각화 허용 →
 * 눌러서). `ask`: the next step, to be let in or thrown away. `record`: on its own nothing waits for a yes, so with
 * the canvas closed the newest thing that has a picture is shown as what goes up when it opens. `more`: how many
 * others wait behind it. `posted`: how many were let in while the canvas is closed (they come in when it opens).
 */
export function chatView(s: Gate, path: string, asked: boolean): { v: Visual | null; mode: "ask" | "record"; more: number; posted: number; open: boolean } {
  const k = keyOf(path), open = !!s.open[k];
  // (a playbook the user named: nothing waits for a yes this turn)
  const byHand = asked && !s.loose[k];
  if (open) {
    const w = byHand ? (s.waiting[k] ?? NONE) : NONE;
    return { v: w[0] ?? null, mode: "ask", more: Math.max(0, w.length - 1), posted: 0, open };
  }
  const q = s.queued[k] ?? NONE, posted = Math.min(s.passes[k] ?? 0, q.length);
  if (byHand) {
    const left = q.slice(posted);
    return { v: left[0] ?? null, mode: "ask", more: Math.max(0, left.length - 1), posted, open };
  }
  const shown = q.filter((v) => (v.brings || v.points.length > 0) && lastSeq(v) > (s.closed[k] ?? 0));
  return { v: shown[shown.length - 1] ?? null, mode: "record", more: Math.max(0, shown.length - 1), posted: 0, open };
}

