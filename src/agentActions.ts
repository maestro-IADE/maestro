import { useStore, type Worktree } from "./store";
import type { AgentView } from "./types";

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

/**
 * The agent opened on `view` as its own for now (`setViewOverride`: the app's 에이전트 보기 stays; the app's own view
 * ends the exception), in a lasting tab. The terminal shows full screen: an agent's canvas has no terminal of itself.
 * Unlike the header's view switch this moves no conversation: a terminal mounting while the chat holds it offers
 * "터미널로 넘기기" (a terminal question, the sidebar row's menu).
 */
export function showAgentAs(path: string, view: AgentView) {
  const s = useStore.getState();
  s.setViewOverride(path, view);
  s.select(path, true);
  const group = useStore.getState().focus;
  if (view === "cli" && group) s.setTabLayout(group, path, "full");
}

/** The agent's tab, then that tab into a window of its own. */
export function openInWindow(w: Worktree) {
  const s = useStore.getState();
  s.select(w.path, true);
  const from = useStore.getState().focus;
  if (from) s.popOutTab({ kind: "wt", path: w.path }, from).catch(s.setError);
}
