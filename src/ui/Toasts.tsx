import { useEffect, useRef, useState } from "react";
import { CircleAlert, X } from "../icons";
import { useStore } from "../store";
import type { Toast } from "./toastList";

// Error toasts (docs/features/ui/menus-and-dialogs.md "토스트"): top centre, just under the top bar, where nothing
// stays (the canvas composer is at the bottom centre, the minimap bottom right). Each closes itself after a while,
// not while the pointer is on it. Mounted once per window (App, WorkspaceWindow); `setError` feeds it.

const SHOW_MS = 8000;

function ToastItem({ t }: { t: Toast }) {
  const drop = useStore((s) => s.dropToast);
  const [paused, setPaused] = useState(false);
  // time left before it closes; a repeat of the same text (`at` moves) starts it over
  const left = useRef(SHOW_MS);
  useEffect(() => void (left.current = SHOW_MS), [t.at]);
  useEffect(() => {
    if (paused) return;
    const start = Date.now();
    const timer = setTimeout(() => drop(t.id), left.current);
    return () => {
      clearTimeout(timer);
      left.current = Math.max(0, left.current - (Date.now() - start));
    };
  }, [paused, t.at]);

  return (
    <div className="toast" role="alert" onMouseEnter={() => setPaused(true)} onMouseLeave={() => setPaused(false)}>
      <CircleAlert className="toast-icon" />
      <div className="toast-text">
        {t.text}
        {t.n > 1 && <span className="toast-count"> ×{t.n}</span>}
      </div>
      <button className="toast-x" title="닫기" aria-label="닫기" onClick={() => drop(t.id)}>
        <X size={14} />
      </button>
    </div>
  );
}

export function Toasts() {
  const toasts = useStore((s) => s.toasts);
  if (!toasts.length) return null;
  return (
    <div className="toasts" aria-live="assertive">
      {toasts.map((t) => (
        <ToastItem key={t.id} t={t} />
      ))}
    </div>
  );
}
