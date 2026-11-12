import { useEffect, useRef, useSyncExternalStore } from "react";
import { createPortal } from "react-dom";

/**
 * The app's one confirmation dialog, in place of the browser's built-in confirm box: `await ask({...})` is true when the user
 * picks the confirm button. Requests queue up and show one at a time in the window's `<ConfirmHost />` (mounted once
 * in App and once in WorkspaceWindow).
 */
export type AskOptions = {
  title: string;
  /** a paragraph under the title; line breaks are kept */
  body?: string;
  /** names to look at before confirming (files, paths), in a small monospace box */
  list?: string[];
  /** the confirm button: the action's name ("폐기", "목록에서 제거") */
  confirm: string;
  /** destructive: a red confirm button, focus starts on 취소 and Enter doesn't confirm */
  danger?: boolean;
};

type Request = AskOptions & { id: number; resolve: (ok: boolean) => void };

let queue: Request[] = [];
let seq = 0;
const subs = new Set<() => void>();
const emit = () => subs.forEach((f) => f());
const subscribe = (f: () => void) => {
  subs.add(f);
  return () => void subs.delete(f);
};

export function ask(o: AskOptions): Promise<boolean> {
  return new Promise((resolve) => {
    queue = [...queue, { ...o, id: ++seq, resolve }];
    emit();
  });
}

/** Closing a file with unsaved edits (its tab, the code view's close button). */
export const askDiscardEdits = (rel: string) =>
  ask({ title: "저장하지 않은 변경이 있어요", body: "닫으면 이 파일의 편집이 사라져요", list: [rel], confirm: "저장하지 않고 닫기", danger: true });

function settle(id: number, ok: boolean) {
  const r = queue.find((q) => q.id === id);
  if (!r) return;
  queue = queue.filter((q) => q.id !== id);
  emit();
  r.resolve(ok);
}

/** Shows the oldest open `ask()`. One per window. */
export function ConfirmHost() {
  const q = useSyncExternalStore(subscribe, () => queue);
  const cur = q[0];
  return cur ? createPortal(<ConfirmDialog key={cur.id} req={cur} onDone={(ok) => settle(cur.id, ok)} />, document.body) : null;
}

const LIST_MAX = 8;

function ConfirmDialog({ req, onDone }: { req: Request; onDone: (ok: boolean) => void }) {
  const box = useRef<HTMLDivElement>(null);
  const cancelBtn = useRef<HTMLButtonElement>(null);
  const okBtn = useRef<HTMLButtonElement>(null);
  const done = useRef(onDone);
  done.current = onDone;

  useEffect(() => {
    // focus back where it was (a terminal, the editor) once the dialog is gone
    const before = document.activeElement as HTMLElement | null;
    (req.danger ? cancelBtn : okBtn).current?.focus();
    const inside = (t: EventTarget | null) => t instanceof Node && !!box.current?.contains(t);
    // keys go nowhere else while it's open (xterm, the canvas, window shortcuts): captured on the window first
    const onKey = (e: KeyboardEvent) => {
      if (e.isComposing) return;
      e.stopPropagation();
      if (e.key === "Escape") {
        e.preventDefault();
        done.current(false);
      } else if (e.key === "Enter") {
        // on a focused button Enter presses that button; elsewhere it confirms, unless that's destructive
        if (inside(e.target) && (e.target as HTMLElement).tagName === "BUTTON") return;
        e.preventDefault();
        if (!req.danger) done.current(true);
      } else if (e.key === "Tab") {
        // focus stays among the dialog's buttons
        const btns = [cancelBtn.current, okBtn.current].filter((b): b is HTMLButtonElement => !!b);
        const i = btns.indexOf(document.activeElement as HTMLButtonElement);
        e.preventDefault();
        btns[(i + (e.shiftKey ? btns.length - 1 : 1)) % btns.length]?.focus();
      }
    };
    // something outside taking the focus (a terminal focusing itself) hands it back
    const onFocus = (e: FocusEvent) => {
      if (!inside(e.target)) (req.danger ? cancelBtn : okBtn).current?.focus();
    };
    window.addEventListener("keydown", onKey, true);
    document.addEventListener("focusin", onFocus, true);
    return () => {
      window.removeEventListener("keydown", onKey, true);
      document.removeEventListener("focusin", onFocus, true);
      if (before?.isConnected) before.focus({ preventScroll: true });
    };
  }, []);

  const list = req.list ?? [];
  return (
    <div className="modal-backdrop confirm-backdrop" onMouseDown={(e) => e.target === e.currentTarget && onDone(false)}>
      <div ref={box} className="modal confirm" role="alertdialog" aria-modal="true" aria-labelledby={`confirm-${req.id}`}>
        <h3 id={`confirm-${req.id}`}>{req.title}</h3>
        {req.body && <p className="confirm-body">{req.body}</p>}
        {list.length > 0 && (
          <ul className="discard-list">
            {list.slice(0, LIST_MAX).map((x) => <li key={x} className="ellipsis" title={x}>{x}</li>)}
            {list.length > LIST_MAX && <li className="muted">… 외 {list.length - LIST_MAX}개</li>}
          </ul>
        )}
        <div className="modal-actions">
          <button ref={cancelBtn} type="button" onClick={() => onDone(false)}>취소</button>
          <button ref={okBtn} type="button" className={req.danger ? "danger-fill" : "primary"} onClick={() => onDone(true)}>{req.confirm}</button>
        </div>
      </div>
    </div>
  );
}
