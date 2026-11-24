import { useMemo, useRef, useState } from "react";
import { attentionItems } from "./attention";
import { notifyCases, type NotifyCase } from "./devNotify";
import { askText, coalesce, live, notices, type OsNotify } from "./osNotify";
import { useSettings } from "./settings";
import { Spinner } from "./LogoMotion";
import { deliver, reason, type Delivery } from "./ui/useOsNotify";
import { Check, CircleAlert, Pause, Play, RotateCcw, StepForward, X } from "./icons";

// OS 알림 시험 (docs/features/ui/dev-tools.md): a stand-in team's changes (devNotify.ts) put through the app's own
// rules and sent as real OS notifications, with no agent and no model.

type Note = { title: string; body: string };
/** A case run: what became of it, what it sent, and the level it ran at. */
type Result = { how: Delivery; out: Note[]; level: OsNotify };

/** The project the stand-in team is in: every notification says it, so none is taken for a real agent's. */
const PROJECT = "알림 시험";
/** 5초 뒤: time to put the app behind another window, as it is when the app sends one. */
const LATER_S = 5;
/** Between cases run to the end: time to read one notification before the next. */
const GAP_MS = 2500;
const LEVELS: Array<[OsNotify, string]> = [["all", "모두"], ["waiting", "기다릴 때만"], ["off", "끔"]];

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
// what useOsNotify's flush sends for this change: the notices, those still true, folded as one burst
const sendsFor = (c: NotifyCase, level: OsNotify): Note[] => coalesce(live(notices(c.before, c.after, level, reason), c.after, level));
const clock = () => new Date().toLocaleTimeString("ko-KR", { hour12: false });

/** What a case's result line says, at the level it ran at. */
function said(r: Result, c: NotifyCase): string {
  if (r.how === "focused") return `앱이 앞에 있어서 보내지 않았어요 (앱이 하는 그대로예요). ${LATER_S}초 안에 다른 창으로 가 보세요`;
  if (r.how === "denied") return "OS가 알림을 막았어요";
  if (r.how === "sent") return `OS에 넘겼어요 (${r.out.length}개). 화면 구석이나 알림 센터에서 확인해 보세요`;
  if (r.level === "off") return "\"끔\"이라 보내지 않았어요";
  return sendsFor(c, "all").length ? "\"기다릴 때만\"이라 보내지 않았어요" : "보낼 알림이 없어요 (맞아요)";
}

/**
 * The test screen: the cases in order (끝까지 실행, 한 단계, 처음부터, as the other tests), each with what it would
 * send at the test's level. The level is the test's own, starting from 설정 → 일반 → OS 알림, which it never
 * changes. 바로 sends at once, even with the app in front; 5초 뒤 waits and then sends only if no Maestro window has
 * focus, as the app does.
 */
export function NotifyTest({ onExit }: { onExit: () => void }) {
  const [level, setLevel] = useState<OsNotify>(() => useSettings.getState().osNotify);
  const cases = useMemo(() => notifyCases(PROJECT, attentionItems, askText), []);
  const [later, setLater] = useState(false);
  const [results, setResults] = useState<Result[]>([]);
  const [busy, setBusy] = useState<"all" | "one" | null>(null);
  const [halting, setHalting] = useState(false);
  const [count, setCount] = useState(0);
  const [log, setLog] = useState<Array<Note & { at: string }>>([]);
  // bumped when the test starts over: a run from before then stops without leaving a result
  const gen = useRef(0);
  const halt = useRef(false);

  const restart = () => {
    gen.current++;
    halt.current = false;
    setBusy(null);
    setHalting(false);
    setCount(0);
    setResults([]);
    setLog([]);
  };

  const go = async (all: boolean) => {
    if (busy) return;
    const mine = gen.current;
    halt.current = false;
    setBusy(all ? "all" : "one");
    for (let i = results.length; i < cases.length; i++) {
      const out = sendsFor(cases[i], level);
      if (later && out.length) {
        for (let s = LATER_S; s > 0; s--) {
          setCount(s);
          await sleep(1000);
          if (gen.current !== mine) return;
        }
        setCount(0);
      }
      const how = await deliver(out, later);
      if (gen.current !== mine) return;
      if (how === "sent") setLog((l) => [...out.map((n) => ({ ...n, at: clock() })), ...l]);
      setResults((rs) => [...rs.slice(0, i), { how, out, level }]);
      if (halt.current || !all || i === cases.length - 1) break;
      if (how === "sent") await sleep(GAP_MS);
      if (gen.current !== mine) return;
      if (halt.current) break;
    }
    setBusy(null);
    setHalting(false);
  };

  const done = results.length;
  const tally = (h: Delivery) => results.filter((r) => r.how === h).length;
  const summary = done
    ? [`넘김 ${tally("sent")}`, `없음 ${tally("none")}`, tally("focused") && `건너뜀 ${tally("focused")}`, tally("denied") && `막힘 ${tally("denied")}`].filter(Boolean).join(" · ")
    : `${cases.length}단계`;
  return (
    <div className="dev-screen">
      <aside className="dev-side">
        <header className="dev-head">
          <b>OS 알림 시험</b>
          <span className="spacer" />
          <button className="small" title="시험을 끝내요" onClick={onExit}>
            <X size={14} /> 종료
          </button>
        </header>
        <p className="dev-about">
          가짜 에이전트들(프로젝트 "{PROJECT}")에게 일이 생긴 것처럼 해서, 앱이 실제로 쓰는 규칙(<code>osNotify.ts</code>)이 만든 알림을 진짜 OS 알림으로 보내요. AI는 쓰지 않아요. 앱은 원래 창이 뒤에 있을 때만 보내요: "5초 뒤"를 고르고 다른 창으로 가면 그대로 시험돼요. 떴는지는 눈으로 봐야 해요(앱은 OS에 넘긴 것까지만 알아요). 개발 빌드의 알림은 보낸 이가 Windows PowerShell로 보여요.
        </p>
        <div className="dev-controls">
          <span className="muted">보내는 때</span>
          <button data-on={!later || undefined} aria-pressed={!later} disabled={!!busy} title="바로 보내요 (앱이 앞에 있어도)" onClick={() => setLater(false)}>
            바로
          </button>
          <button data-on={later || undefined} aria-pressed={later} disabled={!!busy} title={`${LATER_S}초 뒤, 앱이 뒤에 있을 때만 보내요 (앱이 실제로 보내는 방식)`} onClick={() => setLater(true)}>
            {LATER_S}초 뒤
          </button>
        </div>
        <div className="dev-controls">
          <span className="muted">OS 알림</span>
          {LEVELS.map(([v, label]) => (
            <button key={v} data-on={level === v || undefined} aria-pressed={level === v} disabled={!!busy} title="이 시험에서만 써요 (설정은 그대로예요)" onClick={() => setLevel(v)}>
              {label}
            </button>
          ))}
        </div>
        <div className="dev-controls">
          {busy === "all" ? (
            <button
              className="primary"
              disabled={halting}
              title="지금 보내는 것까지만 하고 멈춰요"
              onClick={() => {
                halt.current = true;
                setHalting(true);
              }}
            >
              <Pause size={14} /> {halting ? "멈추는 중…" : "멈춤"}
            </button>
          ) : (
            <button className="primary" disabled={!!busy || done >= cases.length} title="남은 경우를 차례로 보내요" onClick={() => void go(true)}>
              <Play size={14} /> 끝까지 실행
            </button>
          )}
          <button disabled={!!busy || done >= cases.length} title="다음 경우 하나만 보내요" onClick={() => void go(false)}>
            <StepForward size={14} /> 한 단계
          </button>
          <button disabled={!done && !busy} title="처음부터 다시 해요" onClick={restart}>
            <RotateCcw size={14} /> 처음부터
          </button>
          <span className="spacer" />
          <span className={tally("denied") ? "err" : "muted"}>{summary}</span>
        </div>
        <ol className="dev-steps">
          {cases.map((c, i) => {
            const r = results[i];
            const running = !!busy && i === done;
            const out = r?.out ?? sendsFor(c, level);
            // handed to the OS or rightly nothing: done; held back by focus: not tried; refused: wrong
            const state = r ? (r.how === "denied" ? "bad" : r.how === "focused" ? undefined : "ok") : running ? "run" : undefined;
            return (
              <li key={c.title} data-state={state}>
                <div className="dev-step-head">
                  <span className="dev-mark">{state === "ok" ? <Check size={14} /> : state === "bad" ? <CircleAlert size={14} /> : running ? <Spinner /> : i + 1}</span>
                  <b>{c.title}</b>
                  {running && count > 0 && <span className="muted">{count}초 뒤</span>}
                </div>
                <p className="dev-see">{c.see}</p>
                {out.length ? (
                  out.map((n, j) => (
                    <code key={j} className="dev-cmd">
                      {n.title}
                      {"\n"}
                      {n.body}
                    </code>
                  ))
                ) : (
                  <code className="dev-cmd muted">(보낼 알림 없음)</code>
                )}
                {r && <p className={r.how === "denied" ? "dev-see err" : "dev-see"}>{said(r, c)}</p>}
              </li>
            );
          })}
        </ol>
      </aside>
      <div className="dev-canvas dev-log">
        <b>OS에 넘긴 알림</b>
        {log.length ? (
          <ol>
            {log.map((n, i) => (
              <li key={log.length - i}>
                <span className="muted">{n.at}</span>
                <b>{n.title}</b>
                <span>{n.body}</span>
              </li>
            ))}
          </ol>
        ) : (
          <p className="muted">보낸 알림이 여기에 쌓여요. 화면에 뜬 것과 비교해 보세요.</p>
        )}
      </div>
    </div>
  );
}
