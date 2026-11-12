import { useEffect } from "react";
import { isImeComposing } from "../ime";
import { guardAllows, matchShortcut, tabNumber, type ShortcutId } from "../shortcuts";
import { barTabs, openTab, selectAttention, useStore } from "../store";
import { nextItem } from "../attention";
import { stepAgent } from "../Sidebar";

// What each shortcut does in this window. The table and the guard are in shortcuts.ts; the one not listed here
// (quickSwitch) is defined there for a later step (docs/prd/ui-structure.md, O1) and does nothing yet.
const ACTIONS: Partial<Record<ShortcutId, (e: KeyboardEvent) => void>> = {
  // the focused group's Nth tab as its bar shows them, the project's own first; past the last one: nothing
  tabN: (e) => {
    const s = useStore.getState();
    const g = s.focus ? s.groups[s.focus] : undefined;
    const t = g && barTabs(g, s)[tabNumber(e) - 1];
    if (t !== undefined) openTab(t);
  },
  settings: () => {
    const s = useStore.getState();
    if (!s.settings) s.openSettings();
  },
  back: () => useStore.getState().goNav(-1),
  forward: () => useStore.getState().goNav(1),
  // the agent before / after the one on screen, in sidebar order (folded projects skipped)
  prevAgent: () => stepAgent(-1),
  nextAgent: () => stepAgent(1),
  // F2 on a sidebar row (the guard only lets it through in the list): that agent's settings, at its name. A project
  // head or a row without an agent has nothing to rename
  rename: (e) => {
    const row = e.target instanceof Element ? e.target.closest<HTMLElement>("[data-agent-path]") : null;
    const path = row?.dataset.agentPath;
    if (path) useStore.getState().openAgentSettings({ path, focus: "name" });
  },
  // the next 확인할 일 after the agent on screen's (round to the first), opened in a lasting tab
  nextAttention: () => {
    const s = useStore.getState();
    const it = nextItem(selectAttention(s), s.selectedPath);
    if (it) s.select(it.path, true);
  },
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
      act(e);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);
}
