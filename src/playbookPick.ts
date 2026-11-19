// The playbook an agent is told to follow with its next messages (docs/features/orchestration/playbook-pick.md): picked
// in the canvas's 플레이북 panel, by name in the message box (`/` lists them, like a skill), or by a button that starts
// one (플레이 만들기). None picked: the agent chooses for itself. Kept per agent for the session.
import { create } from "zustand";
import { ipc, type DocFile } from "./ipc";

const keyOf = (path: string) => path.replace(/\\/g, "/").replace(/\/+$/, "").toLowerCase();

export const usePicked = create<{ by: Record<string, DocFile | null> }>(() => ({ by: {} }));
export const pickedBook = (s: { by: Record<string, DocFile | null> }, path: string) => s.by[keyOf(path)] ?? null;
export const pickBook = (path: string, book: DocFile | null) => usePicked.setState((s) => ({ by: { ...s.by, [keyOf(path)]: book } }));

/** The line that goes ahead of a message when a playbook is picked. */
export const bookLine = (b: DocFile) => `[플레이북: ${b.title}] ${b.path} 를 읽고 이 절차를 따라.`;

/** The playbook that makes a play (seed `make-play.md`), by its file name. */
export const MAKE_PLAY = "make-play.md";

let kept: { at: number; list: Promise<DocFile[]> } | null = null;
/** Every playbook, read at most once every half minute. */
export function playbooks(): Promise<DocFile[]> {
  if (!kept || Date.now() - kept.at > 30_000) {
    const list = ipc.playbookList().then((fs) => fs.flatMap((f) => f.files));
    kept = { at: Date.now(), list };
    list.catch(() => (kept = null));
  }
  return kept.list;
}
