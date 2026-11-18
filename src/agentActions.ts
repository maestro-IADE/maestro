import { create } from "zustand";
import { ipc } from "./ipc";
import { samePath, useStore, type Worktree } from "./store";
import type { AgentView } from "./types";

// What an agent's menus offer besides opening it (the sidebar row's, the agent header's ⋯): the same actions, one place.

/**
 * The agents whose changed setting (model, reasoning level, permission mode) waits for a restart: they were at work
 * when it was picked, so they restart when the turn ends (the message box does it, `restartIfDue`). By folder.
 */
export const useRestartDue = create<{ due: Record<string, true> }>(() => ({ due: {} }));
const dueKey = (path: string) => path.replace(/\\/g, "/").replace(/\/+$/, "").toLowerCase();
export const restartDue = (s: { due: Record<string, true> }, path: string) => !!s.due[dueKey(path)];
const setDue = (path: string, on: boolean) =>
  useRestartDue.setState((s) => {
    const due = { ...s.due };
    if (on) due[dueKey(path)] = true;
    else delete due[dueKey(path)];
    return { due };
  });

/**
 * A setting picked from the message box's chips (docs/features/agents/chat-view.md): saved at once. A running agent
 * takes it on a restart on the same conversation: now when it rests, else when its turn ends. An agent that is off
 * takes it the next time it starts.
 */
export async function changeAgent(w: Worktree, patch: { model?: string; effort?: string; permissionMode?: string }) {
  const s = useStore.getState();
  try {
    const r = await ipc.updateAgent({ projectRoot: w.root, worktreePath: w.path, ...patch });
    s.upsert(w.root, r.info);
    if (r.applied !== "restart") return;
    const now = useStore.getState().worktrees.find((x) => samePath(x.path, w.path));
    if (now?.status === "IDLE") await ipc.restartAgent(w.root, w.path);
    else setDue(w.path, true);
  } catch (e) {
    s.setError(e);
  }
}

/** The restart a changed setting waits for, once the agent rests (called by the message box as its state changes). */
export async function restartIfDue(w: Worktree) {
  if (w.status !== "IDLE" || !restartDue(useRestartDue.getState(), w.path)) return;
  setDue(w.path, false);
  // (it may have started on something else meanwhile: the setting is saved, the next start takes it)
  await ipc.restartAgent(w.root, w.path).catch(() => {});
}

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
