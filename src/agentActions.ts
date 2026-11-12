import { useStore, type Worktree } from "./store";

// What an agent's menus offer besides opening it (the sidebar row's, the agent header's ⋯): the same actions, one place.

/** Puts `text` on the clipboard; a refusal shows as an error toast. */
export const copyText = (text: string) => void navigator.clipboard.writeText(text).catch(useStore.getState().setError);

/** The agent in a new split beside the focused group (its tab there kept, not a preview). */
export function openInSplit(w: Worktree) {
  const s = useStore.getState();
  if (!s.focus) return;
  s.markUnread(w.path, false);
  s.dropTab({ kind: "wt", path: w.path }, w.root, s.focus, s.focus, "right", true);
}

/** The agent's tab, then that tab into a window of its own. */
export function openInWindow(w: Worktree) {
  const s = useStore.getState();
  s.select(w.path, true);
  const from = useStore.getState().focus;
  if (from) s.popOutTab({ kind: "wt", path: w.path }, from).catch(s.setError);
}
