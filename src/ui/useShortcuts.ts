import { useEffect } from "react";
import { isImeComposing } from "../ime";
import { guardAllows, matchShortcut, type ShortcutId } from "../shortcuts";
import { useStore } from "../store";

// What each shortcut does in this window. The table and the guard are in shortcuts.ts; the ones not listed here
// are defined there for later steps (docs/prd/ui-structure.md, stage 4 to 8) and do nothing yet.
const ACTIONS: Partial<Record<ShortcutId, () => void>> = {
  settings: () => {
    const s = useStore.getState();
    if (!s.settings) s.openSettings();
  },
  back: () => useStore.getState().goNav(-1),
  forward: () => useStore.getState().goNav(1),
};

/**
 * The window's keyboard shortcuts: one keydown listener on `window` (bubbling, so a terminal or editor that takes
 * the key first stops it there). Nothing while the IME is composing. Mounted once per window (App, WorkspaceWindow).
 */
export function useShortcuts() {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.defaultPrevented || isImeComposing(e)) return;
      const s = matchShortcut(e);
      const act = s && ACTIONS[s.id];
      if (!s || !act) return;
      const target = e.target instanceof Element ? e.target : null;
      // a dialog open (its backdrop is the page's modal layer): nothing behind it moves
      const dialogOpen = !!document.querySelector(".modal-backdrop");
      if (!guardAllows(s, target, dialogOpen)) return;
      e.preventDefault();
      act();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);
}
