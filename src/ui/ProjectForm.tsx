import { useEffect, useState, useSyncExternalStore } from "react";
import { createPortal } from "react-dom";
import { open } from "@tauri-apps/plugin-dialog";
import { homeDir } from "@tauri-apps/api/path";
import { ipc } from "../ipc";
import { isImeComposing } from "../ime";
import { ChevronDown, FolderPlus, Link } from "../icons";
import { defaultParent, expandHome, folderNameOk, joinPath, repoNameFromUrl, shortPath } from "../projectForm";
import { useBodyKeys } from "./useBodyKeys";

/**
 * 프로젝트 추가's two forms (docs/features/ui/add-project.md): 저장소 클론 (URL) and 새 프로젝트 (이름). Like Orca's,
 * each asks for one thing; where it goes is one folded 위치 row (`~/maestro/projects/<name>` until another place is
 * used), opened to change the place (and a clone's folder name). `await openProjectForm(kind)` is the folder made
 * (cloned, or made and committed by the backend), or null when cancelled; the caller adds it as a project. The form
 * runs the work itself and shows its error in place. One at a time, drawn by `<ProjectFormHost />` (mounted once per
 * window, in WindowLayers).
 */
export type ProjectFormKind = "clone" | "new";

type Request = { id: number; kind: ProjectFormKind; resolve: (dir: string | null) => void };

let current: Request | null = null;
let seq = 0;
const subs = new Set<() => void>();
const emit = () => subs.forEach((f) => f());
const subscribe = (f: () => void) => {
  subs.add(f);
  return () => void subs.delete(f);
};

export function openProjectForm(kind: ProjectFormKind): Promise<string | null> {
  return new Promise((resolve) => {
    current?.resolve(null); // a second one replaces the first
    current = { id: ++seq, kind, resolve };
    emit();
  });
}

function settle(id: number, dir: string | null) {
  if (current?.id !== id) return;
  const req = current;
  current = null;
  emit();
  req.resolve(dir);
}

export function ProjectFormHost() {
  const req = useSyncExternalStore(subscribe, () => current);
  return req ? createPortal(<ProjectForm key={req.id} kind={req.kind} onDone={(dir) => settle(req.id, dir)} />, document.body) : null;
}

// the 위치 last used (both forms share it); without one, `~/maestro/projects` (`defaultParent`), made when first used
const PARENT_KEY = "maestro.projectParent";

function savedParent(): string {
  try {
    return localStorage.getItem(PARENT_KEY) ?? "";
  } catch {
    return "";
  }
}

function ProjectForm({ kind, onDone }: { kind: ProjectFormKind; onDone: (dir: string | null) => void }) {
  const clone = kind === "clone";
  const [url, setUrl] = useState("");
  const [home, setHome] = useState("");
  const [parent, setParent] = useState(savedParent);
  // the folder name follows the URL until the user types their own
  const [typedName, setTypedName] = useState<string | null>(null);
  const name = typedName ?? (clone ? repoNameFromUrl(url) : "");
  const [placeOpen, setPlaceOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState("");

  // the home folder: `~` in the 위치 row, and the default 위치 when none was used yet (unless the user got there first)
  useEffect(() => {
    let live = true;
    homeDir()
      .then((h) => {
        if (!live) return;
        setHome(h);
        setParent((p) => p || defaultParent(h));
      })
      .catch(() => {});
    return () => {
      live = false;
    };
  }, []);

  const nameBad = !!name.trim() && !folderNameOk(name);
  // under the name field itself (first in 새 프로젝트, in the opened 위치 row in 클론)
  const nameHint = nameBad && <span className="field-hint" data-err>폴더 이름으로 쓸 수 없어요</span>;
  const can = !busy && !!parent.trim() && !!name.trim() && !nameBad && (!clone || !!url.trim());
  // the place as the backend gets it: `~` (as the row writes it) back to the home folder
  const place = expandHome(parent.trim(), home);
  const target = joinPath(place, name);

  // a clone whose URL names no usable folder: the row opens so 폴더 이름 can be filled in. Once typing pauses (a URL
  // typed by hand names none partway through), and then it stays open: a name typed there never folds it away
  useEffect(() => {
    if (!clone || placeOpen || !url.trim() || name.trim()) return;
    const t = setTimeout(() => setPlaceOpen(true), 600);
    return () => clearTimeout(t);
  }, [clone, placeOpen, url, name]);

  const browse = async () => {
    const dir = await open({ directory: true, title: "위치 선택", defaultPath: place || undefined }).catch(() => null);
    if (typeof dir === "string") setParent(dir);
  };

  const submit = async () => {
    if (!can) {
      // Ctrl+Enter on a clone that still needs its folder name: show where it goes
      if (clone && !!url.trim() && (!name.trim() || nameBad)) setPlaceOpen(true);
      return;
    }
    setBusy(true);
    setErr("");
    try {
      const dir = clone ? await ipc.cloneRepo(url.trim(), place, name.trim()) : await ipc.createProject(place, name.trim());
      try {
        localStorage.setItem(PARENT_KEY, place);
      } catch {}
      onDone(dir);
    } catch (e) {
      setErr(String(e));
      setBusy(false);
      // the folder is taken: 폴더 이름 and 위치, which fix it, are in the row
      if (String(e).includes("이미 있는 폴더")) setPlaceOpen(true);
    }
  };

  // the form dialogs' keys: Ctrl+Enter makes it, a plain Enter in a field doesn't, Esc cancels unless it's running
  const onKey = (e: React.KeyboardEvent | KeyboardEvent) => {
    if (isImeComposing(e)) return;
    if (e.key === "Escape" && !busy) {
      e.stopPropagation();
      return onDone(null);
    }
    if (e.key !== "Enter") return;
    if (e.ctrlKey || e.metaKey) {
      e.preventDefault();
      void submit();
    } else if (e.target instanceof HTMLInputElement) e.preventDefault();
  };
  // focus fell to the body (the clicked button got disabled while busy): Esc and Ctrl+Enter still work
  useBodyKeys(onKey);

  const title = clone ? "저장소 클론" : "새 프로젝트";
  return (
    <div className="modal-backdrop" onMouseDown={(e) => e.target === e.currentTarget && !busy && onDone(null)}>
      <form className="modal form-dialog project-form" onSubmit={(e) => (e.preventDefault(), void submit())} onKeyDown={onKey} aria-label={title}>
        <h3 className="modal-title">
          {clone ? <Link size={16} /> : <FolderPlus size={16} />}
          <span className="ellipsis">{title}</span>
        </h3>
        {clone ? (
          <label className="field">
            <span className="field-label">URL</span>
            <input
              autoFocus
              value={url}
              disabled={busy}
              spellCheck={false}
              autoComplete="off"
              placeholder="https://github.com/user/repo.git"
              onChange={(e) => setUrl(e.target.value)}
            />
          </label>
        ) : (
          <label className="field">
            <span className="field-label">이름</span>
            <input
              autoFocus
              value={name}
              disabled={busy}
              spellCheck={false}
              autoComplete="off"
              placeholder="my-project"
              aria-invalid={nameBad || undefined}
              onChange={(e) => setTypedName(e.target.value)}
            />
            {nameHint}
          </label>
        )}
        <div className="place" data-open={placeOpen || undefined}>
          <button type="button" className="place-head" aria-expanded={placeOpen} disabled={busy} onClick={() => setPlaceOpen(!placeOpen)}>
            <span className="field-label">위치</span>
            <span className="place-path ellipsis" title={target}>
              {shortPath(target, home)}
            </span>
            <ChevronDown size={14} />
          </button>
          {placeOpen && (
            <div className="place-body">
              <span className="path-pick">
                <input value={parent} disabled={busy} spellCheck={false} autoComplete="off" aria-label="위치" onChange={(e) => setParent(e.target.value)} />
                <button type="button" disabled={busy} onClick={() => void browse()}>찾기</button>
              </span>
              {clone && (
                <label className="field">
                  <span className="field-label">폴더 이름</span>
                  <input value={name} disabled={busy} spellCheck={false} autoComplete="off" aria-invalid={nameBad || undefined} onChange={(e) => setTypedName(e.target.value)} />
                  {nameHint}
                </label>
              )}
            </div>
          )}
        </div>
        {err && <div className="dialog-error" role="alert"><span className="wrap">{err}</span></div>}
        <div className="dialog-foot">
          <span className="spacer" />
          <button type="button" disabled={busy} onClick={() => onDone(null)}>취소</button>
          <button type="submit" className="primary" disabled={!can}>
            {clone ? (busy ? "클론하는 중…" : "클론") : busy ? "만드는 중…" : "만들기"}
            {!busy && <kbd className="keycap">Ctrl ↵</kbd>}
          </button>
        </div>
      </form>
    </div>
  );
}
