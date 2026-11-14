import { useEffect } from "react";
import { create } from "zustand";
import { listen } from "@tauri-apps/api/event";
import { ipc } from "./ipc";
import type { RelColor, RelFile, Relation } from "./relationModel";

// the model (types, built-in relations, resolving) is relationModel.ts; this file adds the per-project store
export * from "./relationModel";

// ---------- the store: one file per project, read on first use and again when the CLI changes it ----------

type RelState = { byRoot: Record<string, RelFile>; set: (root: string, f: RelFile) => void };
const useRelStore = create<RelState>((set) => ({ byRoot: {}, set: (root, f) => set((s) => ({ byRoot: { ...s.byRoot, [root]: f } })) }));
const loading = new Set<string>();
const EMPTY: RelFile = { relations: [], builtin: {} };
const asFile = (f: Partial<RelFile> | null | undefined): RelFile => ({ relations: f?.relations ?? [], builtin: f?.builtin ?? {} });
// each read is numbered per project: an older read that lands after a newer one (or after a change) is dropped
const reads = new Map<string, number>();

function load(root: string) {
  const n = (reads.get(root) ?? 0) + 1;
  reads.set(root, n);
  ipc
    .loadRelations(root)
    .then((f) => reads.get(root) === n && useRelStore.getState().set(root, asFile(f)))
    .catch(() => {
      if (reads.get(root) !== n) return;
      useRelStore.getState().set(root, EMPTY);
      loading.delete(root); // read again the next time a canvas of this project opens
    });
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

/**
 * Changes to the project's relations, each made by the backend on the file as it is now (so an agent's declaration
 * in between is kept) and checked there: a refused change throws and nothing changes.
 */
export const relationOps = {
  upsert: (root: string, relation: Relation) => change(root, "upsert", { relation }),
  remove: (root: string, id: string) => change(root, "remove", { id }),
  color: (root: string, id: string, color?: RelColor) => change(root, "color", { id, color: color ?? null }),
};
async function change(root: string, op: "upsert" | "remove" | "color", args: Parameters<typeof ipc.changeRelations>[2]) {
  const f = asFile(await ipc.changeRelations(root, op, args));
  reads.set(root, (reads.get(root) ?? 0) + 1); // a read still on its way is older than this
  useRelStore.getState().set(root, f);
}

/** A new relation's id (the backend's are `r_<ms>_<n>` too). */
export const newRelationId = (f: RelFile) => {
  const ms = Date.now();
  for (let n = 0; ; n++) if (!f.relations.some((r) => r.id === `r_${ms}_${n}`)) return `r_${ms}_${n}`;
};
