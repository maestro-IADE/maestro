// How what an agent puts on its canvas comes in (docs/features/canvas/canvas-controls.md "그림 넘기기"): in steps,
// each under the line the agent says with it. Pure, so scripts/focus-check.ts runs it in Node.

/** Something taken from the agent's queue; `say` is the line said with it. */
export type Said = { say?: unknown };
/** A step: the things that come in together under one line (no line for things nothing was said about). */
export type Step<T> = { say?: string; items: T[] };

/**
 * The queue cut into steps: a thing with a line starts a step, and the things after it that have none belong to
 * it; things before the first line are a step of their own, without a line.
 */
export function stepsOf<T extends Said>(items: readonly T[]): Step<T>[] {
  const out: Step<T>[] = [];
  for (const it of items) {
    const say = typeof it.say === "string" && it.say.trim() ? it.say.trim() : undefined;
    if (say || !out.length) out.push({ say, items: [it] });
    else out[out.length - 1].items.push(it);
  }
  return out;
}

/**
 * How long a step keeps the canvas before the next comes in on its own: its line's time when it has one, none
 * otherwise (things nothing was said about come in at once, as they always did).
 */
export const holdMs = (step: { say?: string }, seconds: number) => (step.say ? Math.max(1, seconds) * 1000 : 0);
