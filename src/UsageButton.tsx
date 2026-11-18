import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { AgentLogo } from "./agents";
import { ipc } from "./ipc";
import { ChevronRight, Gauge, RefreshIcon } from "./icons";
import type { UsageReport, UsageSource, UsageWindow } from "./types";

/**
 * 사용량 (docs/features/ui/usage.md): a plain gauge at the left end of the sidebar's foot, across from the settings
 * gear. A press opens a 340px popup above it (portalled, so it can be wider than the sidebar): one short line per
 * CLI (its 5시간 and 7일 bars), a press on a CLI unfolds its every window with when it resets. Read each time it
 * opens (usage.rs); ↻ reads again. Esc or a press outside closes it.
 */
/**
 * A usage popup's state, shared by the sidebar's button and an agent's own (`AgentUsage`): open or not, the report
 * (read each time it opens), and where it hangs. `where`: the popup's place from its button's box. Esc, a press
 * outside, the window resized or losing the focus close it.
 */
function useUsagePop(where: (r: DOMRect) => React.CSSProperties) {
  const [open, setOpen] = useState(false);
  const [report, setReport] = useState<UsageReport | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [at, setAt] = useState<React.CSSProperties | null>(null);
  const btn = useRef<HTMLButtonElement>(null);
  const pop = useRef<HTMLDivElement>(null);

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

  const toggle = () => {
    if (open) return setOpen(false);
    setAt(where(btn.current!.getBoundingClientRect()));
    setOpen(true);
  };

  useEffect(() => {
    if (!open) return;
    load();
    const inside = (n: Node) => btn.current?.contains(n) || pop.current?.contains(n);
    const away = (e: MouseEvent) => !inside(e.target as Node) && setOpen(false);
    const esc = (e: KeyboardEvent) => e.key === "Escape" && setOpen(false);
    const shut = () => setOpen(false);
    window.addEventListener("mousedown", away);
    window.addEventListener("keydown", esc);
    window.addEventListener("resize", shut);
    window.addEventListener("blur", shut);
    return () => {
      window.removeEventListener("mousedown", away);
      window.removeEventListener("keydown", esc);
      window.removeEventListener("resize", shut);
      window.removeEventListener("blur", shut);
    };
  }, [open]);
  return { open, report, error, loading, at, btn, pop, load, toggle };
}

export function UsageButton() {
  const { open, report, error, loading, at, btn, pop, load, toggle } = useUsagePop((r) => ({ left: r.left, bottom: window.innerHeight - r.top + 6 }));
  const [unfolded, setUnfolded] = useState<string | null>(null);

  const sources = report ? ([["Claude", "claude-code", report.claude], ["Codex", "codex", report.codex]] as const) : [];

  return (
    <>
      <button ref={btn} className="icon sb-gear" title="사용량" aria-label="사용량" aria-expanded={open} onClick={toggle}>
        <Gauge />
      </button>
      {open && at && createPortal(
        <div ref={pop} className="usage-pop" role="dialog" aria-label="사용량" style={at}>
          <div className="usage-head">
            <span>사용량</span>
            <button className="icon" title="다시 불러오기" aria-label="다시 불러오기" data-spin={loading || undefined} disabled={loading} onClick={load}>
              <RefreshIcon />
            </button>
          </div>
          {error && <p className="usage-note">{error}</p>}
          {!report && !error && <p className="usage-note">불러오는 중…</p>}
          {sources.map(([name, agent, src]) => (
            <Provider key={name} name={name} agent={agent} src={src} unfolded={unfolded === name} onToggle={() => setUnfolded(unfolded === name ? null : name)} />
          ))}
        </div>,
        document.body,
      )}
    </>
  );
}

/** Which usage an agent's CLI has: its key in the report and its name; none for a CLI the report doesn't cover. */
const USAGE_OF: Record<string, { key: keyof UsageReport; name: string }> = { "claude-code": { key: "claude", name: "Claude" }, codex: { key: "codex", name: "Codex" } };

/**
 * 사용량 on an agent's message box (docs/features/ui/usage.md): the same gauge, for this agent's CLI alone, every
 * window with when it resets. The popup opens from the button's bottom right corner (over it, right edges in line),
 * as the box's other popups. Nothing for an agent whose CLI has no usage to read.
 */
export function AgentUsage({ agent }: { agent: string }) {
  const of = USAGE_OF[agent];
  return of ? <OwnUsage of={of} /> : null;
}

function OwnUsage({ of }: { of: { key: keyof UsageReport; name: string } }) {
  const { open, report, error, loading, at, btn, pop, load, toggle } = useUsagePop((r) => ({ right: window.innerWidth - r.right, bottom: window.innerHeight - r.top + 6 }));
  const src = report?.[of.key];
  return (
    <>
      <button ref={btn} className="composer-btn plain" title="사용량" aria-label="사용량" aria-expanded={open} onClick={toggle}>
        <Gauge size={16} />
      </button>
      {open && at && createPortal(
        <div ref={pop} className="usage-pop" data-own role="dialog" aria-label={`${of.name} 사용량`} style={at}>
          <div className="usage-head">
            <span>
              사용량 · {of.name}
              {src?.plan && <span className="usage-plan">{src.plan}</span>}
              {src?.as_of && <span className="usage-plan" title="마지막 Codex 대화 때 값">{ago(src.as_of)}</span>}
            </span>
            <button className="icon" title="다시 불러오기" aria-label="다시 불러오기" data-spin={loading || undefined} disabled={loading} onClick={load}>
              <RefreshIcon />
            </button>
          </div>
          {error && <p className="usage-note">{error}</p>}
          {!report && !error && <p className="usage-note">불러오는 중…</p>}
          {src && !src.windows.length && <p className="usage-note">{src.note}</p>}
          {src && src.windows.length > 0 && (
            <div className="usage-rows">
              {src.windows.map((w) => <Row key={w.label} w={w} />)}
            </div>
          )}
        </div>,
        document.body,
      )}
    </>
  );
}

/** One CLI: a head (logo, name, plan or how old Codex's numbers are, ›) and its 5시간·7일 bars side by side; unfolded, every window in full. */
function Provider({ name, agent, src, unfolded, onToggle }: { name: string; agent: string; src: UsageSource; unfolded: boolean; onToggle: () => void }) {
  const has = src.windows.length > 0;
  const short = src.windows.filter((w) => w.label === "5시간" || w.label === "7일");
  return (
    <section className="usage-sec" data-open={unfolded || undefined}>
      <button className="usage-name" disabled={!has} aria-expanded={has ? unfolded : undefined} onClick={onToggle}>
        <span className="usage-who">
          <AgentLogo id={agent} size={16} />
          {name}
          {src.plan && <span className="usage-plan">{src.plan}</span>}
          {src.as_of && <span className="usage-plan" title="마지막 Codex 대화 때 값">{ago(src.as_of)}</span>}
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
