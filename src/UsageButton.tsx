import { useEffect, useRef, useState } from "react";
import { ipc } from "./ipc";
import { Gauge, RefreshIcon } from "./icons";
import type { UsageReport, UsageSource, UsageWindow } from "./types";

/**
 * 사용량 (docs/features/ui/usage.md): the gauge at the left end of the sidebar's foot, across from the settings
 * gear. A press opens a popover over the foot with the Claude and Codex rate-limit windows, read fresh each time
 * it opens (usage.rs); ↻ reads them again. Esc or a press outside closes it.
 */
export function UsageButton() {
  const [open, setOpen] = useState(false);
  const [report, setReport] = useState<UsageReport | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const box = useRef<HTMLDivElement>(null);

  const load = () => {
    setLoading(true);
    setError(null);
    ipc.usageReport()
      .then(setReport)
      .catch((e) => setError(String(e)))
      .finally(() => setLoading(false));
  };

  useEffect(() => {
    if (!open) return;
    load();
    const away = (e: MouseEvent) => !box.current?.contains(e.target as Node) && setOpen(false);
    const esc = (e: KeyboardEvent) => e.key === "Escape" && setOpen(false);
    window.addEventListener("mousedown", away);
    window.addEventListener("keydown", esc);
    return () => {
      window.removeEventListener("mousedown", away);
      window.removeEventListener("keydown", esc);
    };
  }, [open]);

  return (
    <div className="sb-usage" ref={box}>
      <button className="icon sb-gear" title="사용량" aria-label="사용량" aria-expanded={open} onClick={() => setOpen(!open)}>
        <Gauge />
      </button>
      {open && (
        <div className="usage-pop" role="dialog" aria-label="사용량">
          <div className="usage-head">
            <span>사용량</span>
            <button className="icon" title="다시 불러오기" aria-label="다시 불러오기" disabled={loading} onClick={load}>
              <RefreshIcon />
            </button>
          </div>
          {error && <p className="usage-note">{error}</p>}
          {!report && !error && <p className="usage-note">불러오는 중…</p>}
          {report && (
            <>
              <Section name="Claude" src={report.claude} />
              <Section name="Codex" src={report.codex} />
            </>
          )}
        </div>
      )}
    </div>
  );
}

function Section({ name, src }: { name: string; src: UsageSource }) {
  return (
    <section className="usage-sec">
      <div className="usage-name">
        {name}
        {src.as_of && <span className="usage-asof">{ago(src.as_of)} 기준</span>}
      </div>
      {src.note && <p className="usage-note">{src.note}</p>}
      {src.windows.map((w) => <Row key={w.label} w={w} />)}
    </section>
  );
}

function Row({ w }: { w: UsageWindow }) {
  const pct = Math.max(0, Math.min(100, Math.round(w.percent)));
  const at = w.resets_unix != null ? w.resets_unix * 1000 : w.resets_at ? Date.parse(w.resets_at) : NaN;
  return (
    <div className="usage-row" data-level={pct >= 90 ? "high" : pct >= 70 ? "mid" : undefined}>
      <div className="usage-line">
        <span>{w.label}</span>
        <span className="usage-pct">{pct}%</span>
      </div>
      <div className="usage-bar"><span style={{ transform: `scaleX(${pct / 100})` }} /></div>
      {Number.isFinite(at) && <div className="usage-reset">{resetIn(at)}</div>}
    </div>
  );
}

/** "2시간 10분 후 초기화", "3일 4시간 후 초기화" */
function resetIn(at: number) {
  const min = Math.round((at - Date.now()) / 60000);
  if (min <= 0) return "곧 초기화";
  const d = Math.floor(min / 1440);
  const h = Math.floor((min % 1440) / 60);
  const m = min % 60;
  const text = d ? `${d}일${h ? ` ${h}시간` : ""}` : h ? `${h}시간${m ? ` ${m}분` : ""}` : `${m}분`;
  return `${text} 후 초기화`;
}

/** "5분 전", "3시간 전", "2일 전" */
function ago(iso: string) {
  const min = Math.max(0, Math.round((Date.now() - Date.parse(iso)) / 60000));
  if (min < 1) return "방금";
  if (min < 60) return `${min}분 전`;
  if (min < 1440) return `${Math.floor(min / 60)}시간 전`;
  return `${Math.floor(min / 1440)}일 전`;
}
