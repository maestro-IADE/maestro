import { useEffect, useRef, useState } from "react";
import { ipc } from "./ipc";
import { ChevronRight, Gauge, RefreshIcon } from "./icons";
import type { UsageReport, UsageSource, UsageWindow } from "./types";

/** how often the foot's summary is read again while the app is open */
const EVERY_MS = 5 * 60_000;

/**
 * 사용량 (docs/features/ui/usage.md): the gauge at the left end of the sidebar's foot, across from the settings
 * gear, with the fullest window's % beside it. A press opens a popover over the foot: one short line per CLI (its
 * 5시간 and 7일 bars), a press on a CLI unfolds its every window with when it resets. Read on start, every 5 minutes
 * and each time the popover opens (usage.rs); ↻ reads again. Esc or a press outside closes it.
 */
export function UsageButton() {
  const [open, setOpen] = useState(false);
  const [report, setReport] = useState<UsageReport | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [unfolded, setUnfolded] = useState<string | null>(null);
  const box = useRef<HTMLDivElement>(null);

  const load = () => {
    setLoading(true);
    ipc.usageReport()
      .then((r) => {
        setReport(r);
        setError(null);
      })
      .catch((e) => setError(String(e)))
      .finally(() => setLoading(false));
  };

  useEffect(() => {
    load();
    const t = setInterval(load, EVERY_MS);
    return () => clearInterval(t);
  }, []);

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

  const sources = report ? ([["Claude", report.claude], ["Codex", report.codex]] as const) : [];
  const top = Math.max(-1, ...sources.flatMap(([, s]) => s.windows.map((w) => w.percent)));
  const topPct = top < 0 ? null : clamp(top);

  return (
    <div className="sb-usage" ref={box}>
      <button
        className="sb-usage-btn"
        data-level={topPct === null ? undefined : level(topPct)}
        title={topPct === null ? "사용량" : `사용량 · 가장 많이 찬 한도 ${topPct}%`}
        aria-label="사용량"
        aria-expanded={open}
        onClick={() => setOpen(!open)}
      >
        <Gauge />
        {topPct !== null && <span>{topPct}%</span>}
      </button>
      {open && (
        <div className="usage-pop" role="dialog" aria-label="사용량">
          <div className="usage-head">
            <span>사용량</span>
            <button className="icon" title="다시 불러오기" aria-label="다시 불러오기" data-spin={loading || undefined} disabled={loading} onClick={load}>
              <RefreshIcon />
            </button>
          </div>
          {error && <p className="usage-note">{error}</p>}
          {!report && !error && <p className="usage-note">불러오는 중…</p>}
          {sources.map(([name, src]) => (
            <Provider key={name} name={name} src={src} unfolded={unfolded === name} onToggle={() => setUnfolded(unfolded === name ? null : name)} />
          ))}
        </div>
      )}
    </div>
  );
}

/** One CLI: a head (name, plan, ›) and its 5시간·7일 bars side by side; unfolded, every window in full. */
function Provider({ name, src, unfolded, onToggle }: { name: string; src: UsageSource; unfolded: boolean; onToggle: () => void }) {
  const has = src.windows.length > 0;
  const short = src.windows.filter((w) => w.label === "5시간" || w.label === "7일");
  return (
    <section className="usage-sec" data-open={unfolded || undefined}>
      <button className="usage-name" disabled={!has} aria-expanded={has ? unfolded : undefined} onClick={onToggle}>
        <span>
          {name}
          {src.plan && <span className="usage-plan">{src.plan}</span>}
        </span>
        {has && <ChevronRight size={14} className="usage-caret" />}
      </button>
      {!has && <p className="usage-note">{src.note}</p>}
      {has && !unfolded && (
        <div className="usage-mini">
          {(short.length ? short : src.windows.slice(0, 2)).map((w) => (
            <div key={w.label} className="usage-mini-cell" data-level={level(clamp(w.percent))}>
              <div className="usage-line">
                <span>{w.label}</span>
                <span className="usage-pct">{clamp(w.percent)}%</span>
              </div>
              <Bar pct={clamp(w.percent)} />
            </div>
          ))}
        </div>
      )}
      {has && unfolded && (
        <div className="usage-rows">
          {src.windows.map((w) => <Row key={w.label} w={w} />)}
          {src.as_of && <p className="usage-note">마지막 Codex 대화({ago(src.as_of)}) 때 값이에요</p>}
        </div>
      )}
    </section>
  );
}

function Row({ w }: { w: UsageWindow }) {
  const pct = clamp(w.percent);
  const at = w.resets_unix != null ? w.resets_unix * 1000 : w.resets_at ? Date.parse(w.resets_at) : NaN;
  return (
    <div className="usage-row" data-level={level(pct)}>
      <div className="usage-line">
        <span>{w.label}</span>
        <span className="usage-pct">{pct}%</span>
      </div>
      <Bar pct={pct} />
      {Number.isFinite(at) && <div className="usage-reset">{resetIn(at)}</div>}
    </div>
  );
}

const Bar = ({ pct }: { pct: number }) => (
  <div className="usage-bar"><span style={{ transform: `scaleX(${pct / 100})` }} /></div>
);

const clamp = (p: number) => Math.max(0, Math.min(100, Math.round(p)));
const level = (pct: number) => (pct >= 90 ? "high" : pct >= 70 ? "mid" : undefined);

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
