import { useEffect } from "react";
import { create } from "zustand";
import { listen } from "@tauri-apps/api/event";
import { ipc } from "./ipc";
import type { RelFile } from "./relationModel";

// the model (types, built-in relations, resolving) is relationModel.ts; this file adds the per-project store
export * from "./relationModel";

// ---------- the store: one file per project, read on first use and again when the CLI changes it ----------

type RelState = { byRoot: Record<string, RelFile>; set: (root: string, f: RelFile) => void };
const useRelStore = create<RelState>((set) => ({ byRoot: {}, set: (root, f) => set((s) => ({ byRoot: { ...s.byRoot, [root]: f } })) }));
const loading = new Set<string>();
const EMPTY: RelFile = { relations: [], builtin: {} };

function load(root: string) {
  ipc
    .loadRelations(root)
    .then((f) => useRelStore.getState().set(root, { relations: f.relations ?? [], builtin: f.builtin ?? {} }))
    .catch(() => useRelStore.getState().set(root, EMPTY));
}
// `maestro relation define` and a drawing that declares relations change the file under us (listened from first use)
let listening = false;
const listenOnce = () => {
  if (listening) return;
  listening = true;
  void listen<string>("maestro://relations", (e) => loading.has(e.payload) && load(e.payload));
};

/** The project's relations file (empty until read), read once per project. */
export function useRelFile(root: string): RelFile {
  const f = useRelStore((s) => s.byRoot[root]);
  useEffect(() => {
    listenOnce();
    if (!loading.has(root)) {
      loading.add(root);
      load(root);
    }
  }, [root]);
  return f ?? EMPTY;
}

/** Writes the project's relations (checked by the backend: a refused change throws and nothing changes here). */
export async function saveRelations(root: string, f: RelFile) {
  await ipc.saveRelations(root, f);
  useRelStore.getState().set(root, f);
}

/** A new relation's id (the backend's are `r_<ms>_<n>` too). */
export const newRelationId = (f: RelFile) => {
  const ms = Date.now();
  for (let n = 0; ; n++) if (!f.relations.some((r) => r.id === `r_${ms}_${n}`)) return `r_${ms}_${n}`;
};
