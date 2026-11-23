import { useEffect, useRef, useState } from "react";
import { ReactFlowProvider } from "@xyflow/react";
import { listen } from "@tauri-apps/api/event";
import { create } from "zustand";
import { Canvas } from "./Canvas";
import { CHAT_STEPS, DEEP_STEPS, STEPS as ALL_STEPS, typed } from "./devSteps";
import { ApprovalCards } from "./nodes/ApprovalCards";
import { useSettings } from "./settings";
import { ipc } from "./ipc";
import { Spinner } from "./LogoMotion";
import { SYSTEM_ROOT, useStore } from "./store";
import { openMenu } from "./ui/Menu";
import { Check, CircleAlert, MessageSquare, Pause, PencilRuler, Play, RotateCcw, StepForward, X, Frame } from "./icons";

// Developer tools (docs/features/ui/dev-tools.md): the top bar's "dev 모드" badge (a build from `npm run tauri dev`)
// opens a menu of tests that run parts of the app with no agent and no model. A test takes the window over until it
// is ended.

/** The developer test on screen in this window, if any. */
type DevTest = { id: "canvas-cli" | "chat-visuals" | "deep-areas"; root: string };
const useDev = create<{ test: DevTest | null; open: (t: DevTest) => void; close: () => void }>((set) => ({
  test: null,
  open: (test) => set({ test }),
  close: () => set({ test: null }),
}));

/**
 * The top bar's "dev 모드": says this is a development build (so it isn't mistaken for the installed app beside it),
 * and pressed, lists the developer tests. A test runs on the project this window is on.
 */
export function DevBadge() {
  if (!import.meta.env.DEV) return null;
  const start = (id: DevTest["id"]) => {
    const s = useStore.getState();
    const root = s.activeRoot;
    if (!root || root === SYSTEM_ROOT || !s.repos.some((r) => r.root === root)) return s.setError("시험할 프로젝트를 먼저 열어 주세요 (시험 캔버스는 그 프로젝트 아래에 잠깐 만들어져요)");
    useDev.getState().open({ id, root });
  };
  return (
    <button
      className="dev-badge"
      title="npm run tauri dev로 켠 개발 빌드 · 누르면 개발용 시험이 나와요"
      onClick={(e) =>
        openMenu(e.currentTarget, [
          { label: "캔버스 CLI 시험", icon: <PencilRuler size={14} />, onSelect: () => start("canvas-cli") },
          { label: "채팅 창구 시험", icon: <MessageSquare size={14} />, onSelect: () => start("chat-visuals") },
          { label: "깊은 영역 시험", icon: <Frame size={14} />, onSelect: () => start("deep-areas") },
        ])
      }
    >
      dev 모드
    </button>
  );
}

/** The developer test this window is running, over everything under the top bar. */
export function DevScreen() {
  const test = useDev((s) => s.test);
  if (!test) return null;
  return <CanvasCliTest key={`${test.id}|${test.root}`} root={test.root} chat={test.id === "chat-visuals"} deep={test.id === "deep-areas"} onExit={() => useDev.getState().close()} />;
}

type Result = { ok: boolean; code: number; stdout: string; stderr: string; ms: number };
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
const same = (a: string, b: string) => a.replace(/\\/g, "/").toLowerCase() === b.replace(/\\/g, "/").toLowerCase();

/**
 * 캔버스 CLI 시험: the canvas commands of the real `maestro` CLI (draw, ask, erase, canvas, selection, relation), run one
 * after another against a blank canvas, each with what it should do. The canvas belongs to a stand-in caller under
 * project `root` (dev.rs): nothing of it is an agent's, and ending the test takes all of it away. Answering one of the
 * questions it leaves shows the note an agent would get.
 * `chat`: 채팅 창구 시험 (chat-visuals.md) instead: its own commands, and beside the canvas a stand-in for the agent's
 * chat (the cards a chat shows above its input), with the canvas closed at first, as when the agent is on full
 * screen. 추천 그림 is put on 눌러서 for the test (so things wait for a press) and back as it was when it ends.
 */
function CanvasCliTest({ root, onExit, chat, deep }: { root: string; onExit: () => void; chat?: boolean; deep?: boolean }) {
  // (`deep`: 깊은 영역 시험, areas five deep to go into: the 캔버스 CLI 시험's screen with its own list)
  const STEPS = chat ? CHAT_STEPS : deep ? DEEP_STEPS : ALL_STEPS;
  // the canvas on screen (채팅 창구 시험: closed at first, opened from the chat's card or the switch)
  const [canvasOpen, setCanvasOpen] = useState(!chat);
  const guideMode = useSettings((s) => s.guideMode ?? "auto");
  useEffect(() => {
    if (!chat) return;
    const was = useSettings.getState().guideMode ?? "auto";
    useSettings.getState().set({ guideMode: "manual" });
    return () => useSettings.getState().set({ guideMode: was });
  }, [chat]);
  // `run` remounts the canvas: a test started over begins on an empty one
  const [canvas, setCanvas] = useState<{ scope: string; cli: string; run: number } | null>(null);
  const [results, setResults] = useState<Result[]>([]);
  const [busy, setBusy] = useState<"all" | "one" | null>(null);
  const [notes, setNotes] = useState<string[]>([]);
  const [fail, setFail] = useState("");
  const [shown, setShown] = useState<number | null>(null); // the step whose --file JSON is open
  const [halting, setHalting] = useState(false);
  // bumped when the test starts over or ends: a run from before then stops without leaving a result
  const gen = useRef(0);
  // 멈춤: the command in flight has run, so it is still recorded; the run stops after it
  const halt = useRef(false);
  const list = useRef<HTMLOListElement>(null);

  const begin = async () => {
    const mine = ++gen.current;
    setBusy(null);
    setHalting(false);
    setResults([]);
    setNotes([]);
    setFail("");
    // the old canvas goes first and gets a moment: a save of it still on its way would otherwise land after the
    // test's file is cleared, and the new canvas would open on the old drawing
    setCanvas(null);
    setCanvasOpen(!chat);
    await sleep(200);
    try {
      const c = await ipc.devCliBegin(root);
      if (gen.current === mine) setCanvas({ ...c, run: Date.now() });
    } catch (e) {
      if (gen.current === mine) setFail(String(e));
    }
  };
  useEffect(() => {
    void begin();
    const mine = gen.current;
    return () => {
      // (React's development double mount starts the test again at once: that one's `begin` bumps `gen`, and this
      // clean-up is then not the end of the test)
      setTimeout(() => {
        if (gen.current !== mine) return;
        gen.current++;
        void ipc.devCliEnd(root).catch(() => {});
      }, 300);
    };
  }, [root]);
  // an answer given on the test canvas: the note an agent would have got (control.rs sends it here instead)
  const scope = canvas?.scope;
  useEffect(() => {
    if (!scope) return;
    const off = listen<{ owner: string; note: string }>("maestro://dev-note", (e) => same(e.payload.owner, scope) && setNotes((n) => [...n, e.payload.note]));
    return () => void off.then((f) => f());
  }, [scope]);

  const go = async (all: boolean) => {
    if (busy || !canvas) return;
    const mine = gen.current;
    halt.current = false;
    setBusy(all ? "all" : "one");
    for (let i = results.length; i < STEPS.length; i++) {
      const s = STEPS[i];
      let r: Result;
      try {
        // (a step's file: JSON for --file, or a picture or page for `maestro show`, written under its own name)
        const out = await ipc.devCliRun(root, s.args, s.text ?? (s.file ? JSON.stringify(s.file, null, 2) : null), s.text != null ? (s.fileName ?? null) : null);
        r = { ...out, ok: s.fails ? out.code !== 0 : out.code === 0 };
      } catch (e) {
        r = { ok: false, code: -1, stdout: "", stderr: String(e), ms: 0 };
      }
      if (gen.current !== mine) return;
      setResults((rs) => [...rs.slice(0, i), r]);
      requestAnimationFrame(() => list.current?.children[i]?.scrollIntoView({ block: "nearest" }));
      await sleep(s.settle ?? 1500);
      if (gen.current !== mine) return;
      if (halt.current || !all) break;
    }
    setBusy(null);
    setHalting(false);
  };

  const done = results.length;
  const passed = results.filter((r) => r.ok).length;
  return (
    <div className="dev-screen">
      <aside className="dev-side">
        <header className="dev-head">
          <b>{chat ? "채팅 창구 시험" : deep ? "깊은 영역 시험" : "캔버스 CLI 시험"}</b>
          <span className="spacer" />
          <button className="small" title="시험을 끝내요. 시험 캔버스와 시험이 선언한 관계는 지워져요" onClick={onExit}>
            <X size={14} /> 종료
          </button>
        </header>
        {chat ? (
          <p className="dev-about">
            에이전트가 캔버스에 올리려는 것이 채팅에 먼저 보이는지 시험해요. 진짜 <code>maestro</code> CLI로 그리고, AI는 쓰지 않아요. 오른쪽 끝은 채팅 입력창 위에 뜨는 카드 자리(대역)예요. 캔버스는 닫힌 채로 시작해요 (에이전트를 전체 화면으로 볼 때처럼).
          </p>
        ) : (
          <p className="dev-about">
            캔버스에 그리는 <code>maestro</code> 명령을 진짜 CLI로 차례대로 실행해요. AI는 쓰지 않아요. 오른쪽은 이 시험만의 빈 캔버스예요.
          </p>
        )}
        {canvas && <p className="dev-cli muted ellipsis" title={canvas.cli}>CLI: {canvas.cli}</p>}
        {fail && <p className="err dev-fail">{fail}</p>}
        <div className="dev-controls">
          {busy === "all" ? (
            <button
              className="primary"
              disabled={halting}
              title="지금 실행 중인 명령까지만 하고 멈춰요"
              onClick={() => {
                halt.current = true;
                setHalting(true);
              }}
            >
              <Pause size={14} /> {halting ? "멈추는 중…" : "멈춤"}
            </button>
          ) : (
            <button className="primary" disabled={!canvas || !!busy || done >= STEPS.length} title="남은 명령을 끝까지 실행해요" onClick={() => void go(true)}>
              <Play size={14} /> 끝까지 실행
            </button>
          )}
          <button disabled={!canvas || !!busy || done >= STEPS.length} title="다음 명령 하나만 실행해요" onClick={() => void go(false)}>
            <StepForward size={14} /> 한 단계
          </button>
          <button disabled={!done && !fail} title="캔버스를 비우고 처음부터 다시 해요" onClick={() => void begin()}>
            <RotateCcw size={14} /> 처음부터
          </button>
          <span className="spacer" />
          <span className={done && passed < done ? "err" : "muted"}>{done ? `${done}개 중 ${passed}개 통과` : `${STEPS.length}단계`}</span>
        </div>
        <ol className="dev-steps" ref={list}>
          {STEPS.map((s, i) => {
            const r = results[i];
            const running = !!busy && i === done;
            return (
              <li key={i} data-state={r ? (r.ok ? "ok" : "bad") : running ? "run" : undefined}>
                <div className="dev-step-head">
                  <span className="dev-mark">{r ? r.ok ? <Check size={14} /> : <CircleAlert size={14} /> : running ? <Spinner /> : i + 1}</span>
                  <b>{s.title}</b>
                  {s.fails && <span className="muted">거절돼야 통과</span>}
                </div>
                <code className="dev-cmd">{typed(s)}</code>
                {(s.file != null || s.text != null) && (
                  <button className="link dev-file" onClick={() => setShown(shown === i ? null : i)}>
                    {shown === i ? "파일 내용 접기" : `${s.fileName} 내용 보기`}
                  </button>
                )}
                {shown === i && <pre className="dev-out">{s.text ?? JSON.stringify(s.file, null, 2)}</pre>}
                <p className="dev-see">{s.see}</p>
                {r && (
                  <details open={!r.ok}>
                    <summary>{`답 · 종료 코드 ${r.code} · ${r.ms}ms`}</summary>
                    <pre className="dev-out">{(r.stdout + r.stderr).trim() || "(출력 없음)"}</pre>
                  </details>
                )}
              </li>
            );
          })}
        </ol>
        <div className="dev-notes">
          <b>에이전트에게 갈 메모</b>
          {notes.length ? notes.map((n, i) => <pre key={i} className="dev-out">{n}</pre>) : <p className="muted">{chat ? "카드에서 버리거나 선택지에 답하면, 에이전트가 받을 메모가 여기에 보여요." : "남은 질문 블록에 답하면, 에이전트가 받을 메모가 여기에 보여요."}</p>}
        </div>
      </aside>
      <div className="dev-canvas">
        {canvas && canvasOpen && (
          <ReactFlowProvider key={canvas.run}>
            <Canvas root={root} scope={canvas.scope} bare onNewAgent={() => {}} />
          </ReactFlowProvider>
        )}
        {chat && !canvasOpen && <p className="dev-closed muted">캔버스가 닫혀 있어요 (에이전트를 전체 화면으로 보는 중인 셈이에요)</p>}
      </div>
      {chat && (
        <aside className="dev-chat">
          <header className="dev-head">
            <b>채팅 (대역)</b>
          </header>
          <div className="dev-controls">
            <button data-on={canvasOpen || undefined} aria-pressed={canvasOpen} title="에이전트의 캔버스를 열거나 닫아요 (닫으면 채팅만 보는 화면과 같아요)" onClick={() => setCanvasOpen(!canvasOpen)}>
              {canvasOpen ? "캔버스 닫기" : "캔버스 열기"}
            </button>
            <span className="spacer" />
            <span className="muted">추천 그림</span>
            <button data-on={guideMode === "auto" || undefined} aria-pressed={guideMode === "auto"} title="설정 → 캔버스 → 추천 그림: 바로 (시험이 끝나면 원래대로 돌아가요)" onClick={() => useSettings.getState().set({ guideMode: "auto" })}>
              바로
            </button>
            <button data-on={guideMode === "manual" || undefined} aria-pressed={guideMode === "manual"} title="설정 → 캔버스 → 추천 그림: 눌러서 (시험이 끝나면 원래대로 돌아가요)" onClick={() => useSettings.getState().set({ guideMode: "manual" })}>
              눌러서
            </button>
          </div>
          {canvas ? <ApprovalCards path={canvas.scope} root={root} onCanvas={() => setCanvasOpen(true)} /> : null}
          <p className="muted dev-chat-none">에이전트가 캔버스에 올리려는 것과 채팅 선택지가 여기에 카드로 떠요. 아무것도 기다리지 않으면 비어 있어요.</p>
        </aside>
      )}
    </div>
  );
}
