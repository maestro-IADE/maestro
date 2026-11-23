import { useEffect, useState, useSyncExternalStore } from "react";
import { createPortal } from "react-dom";
import { open } from "@tauri-apps/plugin-dialog";
import { homeDir } from "@tauri-apps/api/path";
import { ipc } from "../ipc";
import { isImeComposing } from "../ime";
import { useStore } from "../store";
import { FolderPlus, Link } from "../icons";
import { folderNameOk, parentOf, repoNameFromUrl } from "../projectForm";
import { useBodyKeys } from "./useBodyKeys";

/**
 * 프로젝트 추가's two forms (docs/features/ui/add-project.md): URL로 복제 (URL, 위치, 폴더 이름) and 새 프로젝트
 * (이름, 위치). `await openProjectForm(kind)` is the folder made (cloned, or made and committed by the backend), or
 * null when cancelled; the caller adds it as a project. The form runs the work itself and shows its error in place.
 * One at a time, drawn by `<ProjectFormHost />` (mounted once per window, in WindowLayers).
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

// the 위치 last used (both forms share it); without one, the folder the last listed project sits in, else home
const PARENT_KEY = "maestro.projectParent";

function startParent(): string {
  try {
    const saved = localStorage.getItem(PARENT_KEY);
    if (saved) return saved;
  } catch {}
  const repos = useStore.getState().repos;
  return repos.length ? parentOf(repos[repos.length - 1].root) : "";
}

function ProjectForm({ kind, onDone }: { kind: ProjectFormKind; onDone: (dir: string | null) => void }) {
  const clone = kind === "clone";
  const [url, setUrl] = useState("");
  const [parent, setParent] = useState(startParent);
  // the folder name follows the URL until the user types their own
  const [typedName, setTypedName] = useState<string | null>(null);
  const name = typedName ?? (clone ? repoNameFromUrl(url) : "");
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState("");

  // no 위치 yet (no project listed, none used): the home folder, unless the user got there first
  useEffect(() => {
    if (parent) return;
    let live = true;
    homeDir()
      .then((h) => live && setParent((p) => p || h))
      .catch(() => {});
    return () => {
      live = false;
    };
  }, []);

  const nameBad = !!name.trim() && !folderNameOk(name);
  // under the name field itself (first in 새 프로젝트, last in 복제), not after 위치
  const nameHint = nameBad && <span className="field-hint" data-err>폴더 이름으로 쓸 수 없어요</span>;
  const can = !busy && !!parent.trim() && !!name.trim() && !nameBad && (!clone || !!url.trim());

  const browse = async () => {
    const dir = await open({ directory: true, title: "위치 선택", defaultPath: parent.trim() || undefined }).catch(() => null);
    if (typeof dir === "string") setParent(dir);
  };

  const submit = async () => {
    if (!can) return;
    setBusy(true);
    setErr("");
    try {
      const dir = clone ? await ipc.cloneRepo(url.trim(), parent.trim(), name.trim()) : await ipc.createProject(parent.trim(), name.trim());
      try {
        localStorage.setItem(PARENT_KEY, parent.trim());
      } catch {}
      onDone(dir);
    } catch (e) {
      setErr(String(e));
      setBusy(false);
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

  const title = clone ? "URL로 복제" : "새 프로젝트";
  return (
    <div className="modal-backdrop" onMouseDown={(e) => e.target === e.currentTarget && !busy && onDone(null)}>
      <form className="modal form-dialog project-form" onSubmit={(e) => (e.preventDefault(), void submit())} onKeyDown={onKey} aria-label={title}>
        <h3 className="modal-title">
          {clone ? <Link size={16} /> : <FolderPlus size={16} />}
          <span className="ellipsis">{title}</span>
        </h3>
        {clone && (
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
        )}
        {!clone && (
          <label className="field">
            <span className="field-label">이름</span>
            <input autoFocus value={name} disabled={busy} spellCheck={false} autoComplete="off" aria-invalid={nameBad || undefined} onChange={(e) => setTypedName(e.target.value)} />
            {nameHint}
          </label>
        )}
        <div className="field">
          <span className="field-label" id="project-form-parent">위치</span>
          <span className="path-pick">
            <input value={parent} disabled={busy} spellCheck={false} autoComplete="off" aria-labelledby="project-form-parent" onChange={(e) => setParent(e.target.value)} />
            <button type="button" disabled={busy} onClick={() => void browse()}>찾기</button>
          </span>
        </div>
        {clone && (
          <label className="field">
            <span className="field-label">폴더 이름</span>
            <input value={name} disabled={busy} spellCheck={false} autoComplete="off" aria-invalid={nameBad || undefined} onChange={(e) => setTypedName(e.target.value)} />
            {nameHint}
          </label>
        )}
        {err && <div className="dialog-error" role="alert"><span className="wrap">{err}</span></div>}
        <div className="dialog-foot">
          <span className="spacer" />
          <button type="button" disabled={busy} onClick={() => onDone(null)}>취소</button>
          <button type="submit" className="primary" disabled={!can}>
            {clone ? (busy ? "복제 중…" : "복제") : busy ? "만드는 중…" : "만들기"}
            {!busy && <kbd className="keycap">Ctrl ↵</kbd>}
          </button>
        </div>
      </form>
    </div>
  );
}
