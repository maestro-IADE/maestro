// The app's keyboard shortcuts: one table, one guard (docs/features/ui/shortcuts.md). Pure: no React, no store, so
// scripts/chrome-check.ts can test it. The window listener that acts on a match is `useShortcuts` (ui/useShortcuts.ts).
//
// Keys are matched by `code` where a letter or digit is involved, so they work while the Korean IME is on (its `key`
// for J is "ㅓ"). Ctrl+B is left alone on purpose: the terminal's readline uses it.

/** The parts of a KeyboardEvent a shortcut looks at. */
export type KeyLike = { key: string; code: string; ctrlKey: boolean; altKey: boolean; shiftKey: boolean; metaKey: boolean };

export type ShortcutId = "settings" | "back" | "forward" | "nextAttention" | "prevAgent" | "nextAgent" | "tabN" | "rename" | "quickSwitch";

export type Shortcut = {
  id: ShortcutId;
  /** what it does, as a tooltip names it */
  label: string;
  /** the keys, as a tooltip shows them */
  keys: string;
  match: (e: KeyLike) => boolean;
  /**
   * Where it still works: "anywhere" also inside text fields and Monaco (only a terminal keeps the key);
   * "outside-fields" not in a text field, a terminal or the editor; "sidebar" only with the sidebar list focused.
   */
  scope: "anywhere" | "outside-fields" | "sidebar";
};

// Ctrl on Windows and Linux, ⌘ on macOS: either one, alone (no Alt)
const ctrl = (e: KeyLike) => (e.ctrlKey || e.metaKey) && !e.altKey;
const plain = (e: KeyLike) => !e.ctrlKey && !e.metaKey && !e.altKey && !e.shiftKey;

export const SHORTCUTS: readonly Shortcut[] = [
  { id: "settings", label: "설정", keys: "Ctrl+,", scope: "anywhere", match: (e) => ctrl(e) && !e.shiftKey && (e.code === "Comma" || e.key === ",") },
  { id: "back", label: "뒤로", keys: "Alt+←", scope: "outside-fields", match: (e) => e.altKey && !e.ctrlKey && !e.metaKey && !e.shiftKey && e.key === "ArrowLeft" },
  { id: "forward", label: "앞으로", keys: "Alt+→", scope: "outside-fields", match: (e) => e.altKey && !e.ctrlKey && !e.metaKey && !e.shiftKey && e.key === "ArrowRight" },
  { id: "nextAttention", label: "다음 확인할 일", keys: "Ctrl+J", scope: "outside-fields", match: (e) => ctrl(e) && !e.shiftKey && e.code === "KeyJ" },
  { id: "prevAgent", label: "이전 에이전트", keys: "Ctrl+Shift+↑", scope: "outside-fields", match: (e) => ctrl(e) && e.shiftKey && e.key === "ArrowUp" },
  { id: "nextAgent", label: "다음 에이전트", keys: "Ctrl+Shift+↓", scope: "outside-fields", match: (e) => ctrl(e) && e.shiftKey && e.key === "ArrowDown" },
  { id: "tabN", label: "N번째 탭", keys: "Ctrl+1…9", scope: "outside-fields", match: (e) => ctrl(e) && !e.shiftKey && /^Digit[1-9]$/.test(e.code) },
  { id: "rename", label: "에이전트 설정", keys: "F2", scope: "sidebar", match: (e) => plain(e) && e.key === "F2" },
  { id: "quickSwitch", label: "빠른 이동", keys: "Ctrl+K", scope: "outside-fields", match: (e) => ctrl(e) && !e.shiftKey && e.code === "KeyK" },
];

const byId = new Map(SHORTCUTS.map((s) => [s.id, s]));

/** "이름 (키)": the tooltip and aria-label of a button that has a shortcut. */
export const tip = (id: ShortcutId) => {
  const s = byId.get(id)!;
  return `${s.label} (${s.keys})`;
};

/** A shortcut's keys as a tooltip shows them ("Ctrl+J"), for tooltips worded around them. */
export const keysOf = (id: ShortcutId) => byId.get(id)!.keys;

/** Ctrl+1…9: which tab (1 to 9, counted from the project's own tab), or 0 for any other keys. */
export const tabNumber = (e: KeyLike) => {
  const m = /^Digit([1-9])$/.exec(e.code);
  return m ? Number(m[1]) : 0;
};

/** The shortcut these keys press, if any. */
export const matchShortcut = (e: KeyLike): Shortcut | undefined => SHORTCUTS.find((s) => s.match(e));

/** The part of a DOM element the guard asks about (an Element in the app, a plain object in tests). */
export type TargetLike = { closest: (sel: string) => unknown } | null;

const TERMINAL = ".xterm";
const EDITOR = ".monaco-editor";
const FIELD = "input, textarea, select, [contenteditable]:not([contenteditable='false'])";
const SIDEBAR_LIST = ".sb-body";

/**
 * Whether `s` may act on a key pressed with focus in `target`. A terminal always keeps its keys (it is a shell);
 * the editor and text fields keep theirs unless the shortcut works anywhere. While a dialog is open only the
 * shortcuts that work anywhere act, so nothing moves behind it.
 */
export function guardAllows(s: Shortcut, target: TargetLike, dialogOpen: boolean): boolean {
  const inside = (sel: string) => !!target?.closest(sel);
  if (inside(TERMINAL)) return false;
  if (s.scope === "anywhere") return true;
  if (dialogOpen) return false;
  if (inside(EDITOR) || inside(FIELD)) return false;
  if (s.scope === "sidebar") return inside(SIDEBAR_LIST);
  return true;
}
