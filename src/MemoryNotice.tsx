import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { MemoryStick } from "./icons";
import { ipc } from "./ipc";
import { useStore } from "./store";
import type { MemoryReport } from "./types";

// Memory warning (docs/features/agents/away-stop.md "메모리 경고"): the backend looks every 10 s (memory.rs) and says
// when it changed ("maestro://memory"). While memory is low and agents are on, a button shows in the top bar; its
// popover opens by itself once each time memory goes low, and offers to turn the resting agents off. Main window only.

const gb = (mb: number) => `${(mb / 1024).toFixed(1)} GB`;

export function MemoryNotice() {
  const [r, setR] = useState<MemoryReport | null>(null);
  const [open, setOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const box = useRef<HTMLDivElement>(null);
  // this low spell has opened the popover already
  const opened = useRef(false);

  useEffect(() => {
    let stop = false;
    ipc.memoryReport().then((x) => !stop && setR(x)).catch(() => {});
    const un = listen<MemoryReport>("maestro://memory", (e) => setR(e.payload));
    return () => {
      stop = true;
      void un.then((f) => f());
    };
  }, []);

  const show = !!r && r.low && r.agentsOn > 0;
  useEffect(() => {
    if (show && !opened.current) {
      opened.current = true;
      setOpen(true);
    }
    if (!show) {
      opened.current = false;
      setOpen(false);
    }
  }, [show]);

  // a press outside the button and its popover, or Esc, closes the popover
  useEffect(() => {
    if (!open) return;
    const away = (e: MouseEvent) => !box.current?.contains(e.target as Node) && setOpen(false);
    const esc = (e: KeyboardEvent) => e.key === "Escape" && setOpen(false);
    window.addEventListener("mousedown", away);
    window.addEventListener("keydown", esc);
    return () => {
      window.removeEventListener("mousedown", away);
      window.removeEventListener("keydown", esc);
    };
  }, [open]);

  if (!show || !r) return null;

  const turnOff = async () => {
    setBusy(true);
    try {
      await ipc.turnOffIdle();
      setOpen(false);
    } catch (e) {
      useStore.getState().setError(e);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="tb-update" ref={box}>
      <button className="tb-text tb-warn" title="메모리 부족" aria-expanded={open} onClick={() => setOpen(!open)}>
        <MemoryStick size={14} />
        메모리
      </button>
      {open && (
        <div className="tb-popover update-pop" role="dialog" aria-label="메모리">
          <div className="update-title">메모리 부족</div>
          <p className="update-text">
            남은 메모리 {gb(r.availableMb)} · 에이전트 {r.agentsOn}개 {gb(r.agentsMb)}
          </p>
          <div className="update-actions">
            <button onClick={() => setOpen(false)}>닫기</button>
            <button className="primary" disabled={busy || r.idle === 0} onClick={turnOff}>
              쉬는 에이전트 슬립{r.idle > 0 ? ` ${r.idle}` : ""}
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
