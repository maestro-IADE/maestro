// The toast stack as data (ui/Toasts.tsx draws it, store.ts keeps it). Pure, so scripts/chrome-check.ts tests it.

export type Toast = {
  id: number;
  text: string;
  /** how many times this same text came in; shown as "×n" from 2 */
  n: number;
  /** when it last came in: a repeat restarts its timer */
  at: number;
};

/** At most this many on screen; a newer one closes the oldest. */
export const MAX_TOASTS = 3;

/**
 * The stack after `text` comes in. The same text already showing becomes one toast with its count up (and its
 * timer restarted); otherwise a new toast goes at the end and the oldest beyond `MAX_TOASTS` close.
 */
export function withToast(list: Toast[], text: string, id: number, at: number): Toast[] {
  const same = list.find((t) => t.text === text);
  if (same) return list.map((t) => (t === same ? { ...t, n: t.n + 1, at } : t));
  return [...list, { id, text, n: 1, at }].slice(-MAX_TOASTS);
}
