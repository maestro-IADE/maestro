import { useEffect, useRef, useState, useSyncExternalStore } from "react";
import { createPortal } from "react-dom";

/**
 * 가져오기 (docs/features/ui/side-panels.md "그룹"): the repos found right inside a folder, each with a checkbox, and
 * how to take the picked ones in: each on its own (별도로) or as one group (그룹으로). `await pickImport({...})` is
 * the choice, or null when cancelled. One at a time, drawn by `<ImportHost />` (mounted once per window, beside the
 * confirmation dialog's host).
 */

/** A repo found in the folder: "empty" ones (no commit yet) can't be picked, `listed` ones are on the list already. */
export type ImportRepo = { root: string; name: string; state: "ready" | "empty"; listed: boolean };
export type ImportAs = "group" | "separate";
export type ImportOptions = {
  /** the folder's name, for the title */
  name: string;
  repos: ImportRepo[];
  /** a folder just picked offers both; a group's 새 저장소 찾기 only "group" */
  ways: ImportAs[];
};
export type ImportResult = { as: ImportAs; picks: string[] };

type Request = ImportOptions & { id: number; resolve: (r: ImportResult | null) => void };

let current: Request | null = null;
let seq = 0;
const subs = new Set<() => void>();
const emit = () => subs.forEach((f) => f());
const subscribe = (f: () => void) => {
  subs.add(f);
  return () => void subs.delete(f);
};

export function pickImport(o: ImportOptions): Promise<ImportResult | null> {
  return new Promise((resolve) => {
    current?.resolve(null); // a second one replaces the first
    current = { ...o, id: ++seq, resolve };
    emit();
  });
}

function settle(id: number, r: ImportResult | null) {
  if (current?.id !== id) return;
  const req = current;
  current = null;
  emit();
  req.resolve(r);
}

export function ImportHost() {
  const req = useSyncExternalStore(subscribe, () => current);
  return req ? createPortal(<ImportDialog key={req.id} req={req} onDone={(r) => settle(req.id, r)} />, document.body) : null;
}

const pickable = (r: ImportRepo) => r.state === "ready" && !r.listed;

function ImportDialog({ req, onDone }: { req: Request; onDone: (r: ImportResult | null) => void }) {
  const box = useRef<HTMLDivElement>(null);
  const allBox = useRef<HTMLInputElement>(null);
  const done = useRef(onDone);
  done.current = onDone;
  const can = req.repos.filter(pickable).map((r) => r.root);
  const [picked, setPicked] = useState<string[]>(can);
  const both = req.ways.length > 1;
  // a group may hold only projects already listed (they move into it); taking in nothing on its own does nothing
  const groupOk = picked.length > 0 || (both && req.repos.some((r) => r.listed && r.state === "ready"));
  const allOn = can.length > 0 && picked.length === can.length;

  useEffect(() => {
    if (allBox.current) allBox.current.indeterminate = picked.length > 0 && !allOn;
  }, [picked, allOn]);

  useEffect(() => {
    const before = document.activeElement as HTMLElement | null;
    const focusables = () => [...(box.current?.querySelectorAll<HTMLElement>("input:not(:disabled), button:not(:disabled)") ?? [])];
    (box.current?.querySelector<HTMLElement>("button.primary:not(:disabled)") ?? focusables()[0])?.focus();
    const inside = (t: EventTarget | null) => t instanceof Node && !!box.current?.contains(t);
    // keys go nowhere else while it's open (xterm, the canvas, window shortcuts); Tab stays inside
    const onKey = (e: KeyboardEvent) => {
      if (e.isComposing) return;
      e.stopPropagation();
      if (e.key === "Escape") {
        e.preventDefault();
        done.current(null);
      } else if (e.key === "Tab") {
        const f = focusables();
        const i = f.indexOf(document.activeElement as HTMLElement);
        e.preventDefault();
        f[(i + (e.shiftKey ? f.length - 1 : 1)) % f.length]?.focus();
      }
    };
    const onFocus = (e: FocusEvent) => !inside(e.target) && focusables()[0]?.focus();
    window.addEventListener("keydown", onKey, true);
    document.addEventListener("focusin", onFocus, true);
    return () => {
      window.removeEventListener("keydown", onKey, true);
      document.removeEventListener("focusin", onFocus, true);
      if (before?.isConnected) before.focus({ preventScroll: true });
    };
  }, []);

  const flip = (root: string, on: boolean) => setPicked((p) => (on ? [...p, root] : p.filter((x) => x !== root)));
  const finish = (as: ImportAs) => onDone({ as, picks: picked });

  return (
    <div className="modal-backdrop confirm-backdrop" onMouseDown={(e) => e.target === e.currentTarget && onDone(null)}>
      <div ref={box} className="modal import" role="dialog" aria-modal="true" aria-labelledby={`import-${req.id}`}>
        <h3 id={`import-${req.id}`}>{both ? `'${req.name}'에서 가져오기` : `'${req.name}' 새 저장소`}</h3>
        <div className="import-list" role="group" aria-label="저장소">
          {can.length > 1 && (
            <label className="import-row import-all">
              <input ref={allBox} type="checkbox" checked={allOn} onChange={(e) => setPicked(e.target.checked ? can : [])} />
              <span className="import-name">모두</span>
              <span className="import-meta">{picked.length}/{can.length}</span>
            </label>
          )}
          {req.repos.map((r) => {
            const off = !pickable(r);
            return (
              <label key={r.root} className="import-row" data-off={off || undefined} title={r.root}>
                <input type="checkbox" disabled={off} checked={r.listed || picked.includes(r.root)} onChange={(e) => flip(r.root, e.target.checked)} />
                <span className="import-name ellipsis">{r.name}</span>
                {r.listed ? <span className="import-meta">추가됨</span> : r.state === "empty" ? <span className="import-meta">커밋 없음</span> : null}
              </label>
            );
          })}
        </div>
        <div className="modal-actions">
          <button type="button" onClick={() => onDone(null)}>취소</button>
          {both && (
            <button type="button" disabled={!picked.length} onClick={() => finish("separate")}>별도로 가져오기</button>
          )}
          <button type="button" className="primary" disabled={!groupOk} onClick={() => finish("group")}>{both ? "그룹으로 가져오기" : "가져오기"}</button>
        </div>
      </div>
    </div>
  );
}
