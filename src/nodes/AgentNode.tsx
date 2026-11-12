import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { isImeComposing } from "../ime";
import { LogoMotion, Spinner } from "../LogoMotion";
import { CaretIcon, DocIcon, PersonaIcon, X } from "../icons";
import { Background, BackgroundVariant, Handle, NodeResizeControl, Position, useReactFlow, useStore as useFlowStore, type NodeProps } from "@xyflow/react";
import { motionMs, useSettings } from "../settings";
import { listen } from "@tauri-apps/api/event";
import Markdown, { type Components } from "react-markdown";
import remarkGfm from "remark-gfm";
import { ipc, type DocFile } from "../ipc";
import { DRAG_MIME } from "../RightPanel";
import { AgentGlyph } from "../status";
import { XTerm } from "./XTerm";
import { AttachAgent } from "./AttachAgent";
import { AgentLogo, agentDef, agentLabel } from "../agents";
import { ask } from "../ui/Confirm";
import { useDiscard } from "../discard";
import { ApprovalCards } from "./ApprovalCards";
import { ToolPreviewView } from "./ToolDiff";
import { toolPreview } from "../toolPreview";
import { PrChip } from "./PrCard";
import { matchCommands, type SlashCmd } from "../slashCommands";
import { useStore, useTuiWait, useWaiting, useWorktree, type Worktree } from "../store";
import type { AgentEventPayload, AgentView, Picked, RuntimeEvent, SwitchResult, SwitchWhen, TerminalFlowNode, TerminalSize } from "../types";
import { addDelta, fold, lastText, type Item } from "../chatFold";

const LOD_ZOOM = 0.6;
export const GRID = 24;

/** The canvas pattern picked in 설정 > 스타일 (both canvases), on the snap grid. */
export function CanvasBackground() {
  const bg = useSettings((s) => s.canvasBg) ?? "dots";
  const gap = useSettings((s) => s.canvasGap) ?? GRID;
  const alpha = useSettings((s) => s.canvasAlpha) ?? 100;
  if (bg === "none") return null;
  // dots keep React Flow's own dot colour, the lines the theme's border colour; both faded by `alpha`
  const base = bg === "dots" ? "var(--xy-background-pattern-dots-color-default, #91919a)" : "var(--mae-border)";
  return (
    <Background
      gap={gap}
      variant={bg === "dots" ? BackgroundVariant.Dots : bg === "lines" ? BackgroundVariant.Lines : BackgroundVariant.Cross}
      size={bg === "cross" ? 7 : bg === "dots" ? 1 : undefined}
      lineWidth={bg === "lines" ? 0.6 : 1}
      color={`color-mix(in srgb, ${base} ${alpha}%, transparent)`}
    />
  );
}
// All multiples of GRID so nodes stay on the grid.
export const TERM_SIZES: Record<TerminalSize, { width: number; height: number }> = {
  S: { width: 480, height: 408 },
  M: { width: 648, height: 552 },
  L: { width: 864, height: 720 },
};

/** An agent block folded to a chip: it's just there; selecting it brings up its chat box (AgentBar). */
export const COMPACT_SIZE = { width: 14 * GRID, height: 3.5 * GRID };

/** How far an agent block can be resized (grid multiples): small enough to stay readable, not past a screen. */
export const AGENT_LIMITS = { minWidth: 18 * GRID, minHeight: 14 * GRID, maxWidth: 50 * GRID, maxHeight: 40 * GRID };
export const clampAgentSize = (width: number, height: number) => ({
  width: Math.min(AGENT_LIMITS.maxWidth, Math.max(AGENT_LIMITS.minWidth, Math.round(width / GRID) * GRID)),
  height: Math.min(AGENT_LIMITS.maxHeight, Math.max(AGENT_LIMITS.minHeight, Math.round(height / GRID) * GRID)),
});

/**
 * An agent block on an agent canvas: the agent panel, resized by dragging its bottom-right corner, with
 * link points on every side (workers, references, prompt blocks; the canvas connects in loose mode).
 */
export function AgentNode({ id, data }: NodeProps<TerminalFlowNode>) {
  const lod = useFlowStore((s) => s.transform[2] < LOD_ZOOM);
  const { updateNode } = useReactFlow();
  // the window comes back at the size it had
  const fold = useFold(id, (compact, n) =>
    compact
      ? { size: COMPACT_SIZE, data: { ...data, compact, full: { width: n.width ?? TERM_SIZES.M.width, height: n.height ?? TERM_SIZES.M.height } } }
      : { size: data.full ?? TERM_SIZES.M, data: { ...data, compact } },
  );
  return (
    <>
      {/* outside the panel so they sit on the block's edges, not its header */}
      {([["top", Position.Top], ["right", Position.Right], ["bottom", Position.Bottom], ["left", Position.Left]] as const).map(([side, pos]) => (
        <Handle key={side} id={side} type="source" position={pos} className="wt-handle" />
      ))}
      {data.compact ? (
        <AgentChip path={data.worktreePath} onExpand={() => fold(false)} />
      ) : (
        <>
          <NodeResizeControl position="bottom-right" className="agent-resize" {...AGENT_LIMITS} />
          <AgentPanel
            className="node node-agent"
            path={data.worktreePath}
            view={data.view ?? "chat"}
            // from the node as it is then: a switch may finish after the block was folded or resized
            onView={(v) => updateNode(id, (n) => ({ data: { ...n.data, view: v } }))}
            lod={lod}
            extra={
              <button className="icon nodrag" title="작게 접기 (선택하면 캔버스 아래에 입력창이 떠요)" aria-label="작게 접기" onClick={() => fold(true)}>
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
                  <path d="M4 14h6v6M20 10h-6V4M14 10l7-7M3 21l7-7" />
                </svg>
              </button>
            }
          />
        </>
      )}
    </>
  );
}

/**
 * Folding a block to its small version and back: it shrinks to / grows from its own centre (not snapped to the
 * grid, which would pull it off centre), moving at the motion setting's pace.
 */
export function useFold(
  id: string,
  next: (compact: boolean, n: { width?: number; height?: number }) => { size: { width: number; height: number }; data: Record<string, unknown> },
) {
  const { updateNode } = useReactFlow();
  return (compact: boolean) => {
    updateNode(id, (n) => {
      const { size, data } = next(compact, n);
      const w = n.width ?? size.width, h = n.height ?? size.height;
      const position = { x: n.position.x + (w - size.width) / 2, y: n.position.y + (h - size.height) / 2 };
      return { ...size, position, className: "folding", data };
    });
    // the transition class only while it moves, or dragging the block would lag behind the pointer
    setTimeout(() => updateNode(id, { className: undefined }), motionMs(useSettings.getState()) + 50);
  };
}

/** The folded agent block: who it is and what it's doing, nothing more. */
function AgentChip({ path, onExpand }: { path: string; onExpand: () => void }) {
  const wt = useWorktree(path);
  const waiting = useWaiting(path);
  if (!wt) return null;
  return (
    <div className="node agent-chip" data-waiting={waiting || undefined} title={`${wt.name}\n브랜치 ${wt.branch}\n누르면 캔버스 아래 입력창을 열고 닫아요`}>
      <div className="agent-chip-row">
        <AgentGlyph w={wt} />
        <AgentLogo id={wt.adapterId} size={18} />
        <b className="ellipsis">{wt.name}</b>
        <PrChip path={path} />
        {waiting && <span className="agent-chip-wait small">승인 대기</span>}
        <button className="icon nodrag" title="창으로 펼치기" aria-label="창으로 펼치기" onClick={onExpand}>
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
            <path d="M15 3h6v6M9 21H3v-6M21 3l-7 7M3 21l7-7" />
          </svg>
        </button>
      </div>
      <RefTags wt={wt} />
    </div>
  );
}

/** A folded block's personas and referenced documents as tags, two lines at most. */
export function RefTags({ wt }: { wt: { personas: string[]; docs: string[] } }) {
  if (!wt.personas.length && !wt.docs.length) return null;
  return (
    <div className="ref-tags" title={[...wt.personas.map((p) => `페르소나 · ${p}`), ...wt.docs.map((d) => `문서 · ${d}`)].join("\n")}>
      {wt.personas.map((p) => <span key={`p${p}`} className="persona-badge"><PersonaIcon />{p}</span>)}
      {wt.docs.map((d) => <span key={`d${d}`} className="doc-badge"><DocIcon />{d}</span>)}
    </div>
  );
}

/**
 * The chat box under a canvas, remembered for the session per canvas (open or not) and per agent (how much
 * conversation shows), so going to full screen and back finds it as it was.
 */
export const barOpen = new Map<string, boolean>();
const peeks = new Map<string, number>();
const PEEK_MIN = 160; // the conversation opened by its button (or a double click on the grip): a few lines

const arrows = (to: string[]) => (to.length ? ` → ${to.join(", ")}` : "");
const pickedLine = (p: Picked) =>
  p.kind === "agent"
    ? `에이전트 ${p.name} (브랜치 ${p.branch}, ${p.path})`
    : p.kind === "document"
      ? `문서 ${p.title} (${p.path})`
      : p.kind === "block"
        ? `블록 ${p.title}${p.area ? ` [영역 ${p.area}]` : ""}${arrows(p.to)}${p.body ? `\n  ${p.body.trim().split("\n").join("\n  ")}` : ""}`
        : p.kind === "area"
          ? `영역 ${p.title}: ${p.holds.join(", ") || "비어 있음"}${arrows(p.to)}`
          : `프롬프트 블록 ${p.name}`;
const pickedName = (p: Picked) => ("title" in p ? p.title : p.name);

/** The button under the canvas that opens / closes the chat box (AgentBar): the logo, playing its loop on hover. */
export function BarButton({ open, onClick }: { open: boolean; onClick: () => void }) {
  const [hover, setHover] = useState(false);
  return (
    <button
      className="bar-button"
      data-on={open || undefined}
      title={open ? "입력창 닫기" : "이 캔버스의 에이전트에게 보내기"}
      aria-label={open ? "입력창 닫기" : "이 캔버스의 에이전트에게 보내기"}
      aria-expanded={open}
      onClick={onClick}
      onMouseEnter={() => setHover(true)}
      onMouseLeave={() => setHover(false)}
    >
      {/* remounted when the hover ends, so it rests in the logo's own shape */}
      <LogoMotion key={hover ? "moving" : "still"} height={30} moving={hover} />
    </button>
  );
}

/** The chat box along the canvas bottom, for the canvas's own agent (open until its button closes it). */
export function AgentBar({ path, picked, onUnpick, onClose, onFull, book, onBook }: {
  path: string;
  picked: Picked[];
  onUnpick: () => void;
  onClose: () => void;
  onFull?: () => void;
  /** the playbook picked in the canvas's 플레이북 panel; sent along before each message (none = the Maestro picks) */
  book?: DocFile | null;
  onBook?: (b: DocFile | null) => void;
}) {
  const wt = useWorktree(path);
  // the terminal's own question is answered in the full view, on its CLI
  const openCli =
    onFull &&
    (() => {
      useSettings.getState().set({ fullView: "cli" });
      onFull();
    });
  // the conversation peeking above the box: dragged up to show more, down to hide; kept per agent
  const [peek, setPeekState] = useState(() => peeks.get(path) ?? 0);
  const setPeek = (h: number) => {
    peeks.set(path, h);
    setPeekState(h);
  };
  const events = useTranscript(wt?.root ?? "", path);
  const items = useMemo(() => fold(events), [events]);
  const grip = (e: React.PointerEvent) => {
    if (e.button !== 0) return;
    const el = e.currentTarget as HTMLElement;
    el.setPointerCapture(e.pointerId);
    const y0 = e.clientY, h0 = peek, max = window.innerHeight * 0.6;
    const move = (m: PointerEvent) => setPeekState(Math.max(0, Math.min(max, h0 + y0 - m.clientY)));
    const up = (u: PointerEvent) => {
      el.removeEventListener("pointermove", move);
      const h = Math.max(0, Math.min(max, h0 + y0 - u.clientY));
      setPeek(h < 48 ? 0 : h); // a short pull snaps shut
    };
    el.addEventListener("pointermove", move);
    el.addEventListener("pointerup", up, { once: true });
  };
  // the canvas selection goes along with each message; its × clears the selection on the canvas too
  const along = picked.length > 0;
  const prefix = [
    book && `[플레이북: ${book.title}] ${book.path} 를 읽고 이 절차를 따라.`,
    along && `[캔버스에서 선택한 블록]\n${picked.map((p) => `- ${pickedLine(p)}`).join("\n")}`,
  ].filter(Boolean).join("\n\n");
  const { boot, retry, trust } = useAgentBoot(wt?.root ?? "", path, !!wt?.adapterId);
  if (!wt) return null;
  return (
    <div className="agent-bar nodrag nowheel nopan">
      <div
        className="agent-bar-grip"
        title="끌어 올리면 대화가 보여요 · 더블클릭으로 열고 닫기"
        onPointerDown={grip}
        onDoubleClick={() => setPeek(peek ? 0 : PEEK_MIN)}
      />
      {peek > 0 && (
        <div className="agent-bar-peek" style={{ height: peek }}>
          <Messages items={items} running={wt.status === "RUNNING"} />
        </div>
      )}
      <div className="agent-bar-head">
        <AgentGlyph w={wt} />
        <b className="ellipsis" title={`${wt.name}\n브랜치 ${wt.branch}`}>{wt.name}</b>
        <AgentTag wt={wt} />
        <PrChip path={wt.path} />
        <span className="spacer" />
        <button
          className="icon agent-bar-x"
          title={peek ? "대화 접기" : "대화 펼치기"}
          aria-label={peek ? "대화 접기" : "대화 펼치기"}
          aria-expanded={peek > 0}
          onClick={() => setPeek(peek ? 0 : PEEK_MIN)}
        >
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
            <path d={peek ? "m6 9 6 6 6-6" : "m18 15-6-6-6 6"} />
          </svg>
        </button>
        {onFull && (
          <button className="icon agent-bar-x" title="전체 화면으로 보기" aria-label="전체 화면으로 보기" onClick={onFull}>
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
              <path d="M15 3h6v6M9 21H3v-6M21 3l-7 7M3 21l7-7" />
            </svg>
          </button>
        )}
        <button className="icon agent-bar-x" title="닫기" aria-label="입력창 닫기" onClick={onClose}>
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" aria-hidden>
            <path d="M18 6 6 18M6 6l12 12" />
          </svg>
        </button>
      </div>
      {(along || book) && (
        <div className="agent-bar-picked">
          {book && (
            <span className="picked-chip" title={`${book.title}\n${book.path}`}>
              플레이북 · {book.title}
              <button className="icon" title="자동으로 (마에스트로가 골라요)" aria-label="플레이북 지정 해제" onClick={() => onBook?.(null)}><X size={12} /></button>
            </span>
          )}
          {along && (
            <>
              <span className="picked-chip" title={picked.map(pickedLine).join("\n")}>
                선택 {picked.length}개
                <button className="icon" title="선택 해제" aria-label="선택 해제" onClick={onUnpick}><X size={12} /></button>
              </span>
              <span className="muted small ellipsis">{picked.map(pickedName).join(", ")}</span>
            </>
          )}
        </div>
      )}
      <ApprovalCards path={path} onOpenCli={openCli} />
      <TuiWaitBand path={path} onOpenCli={openCli} />
      {!wt.adapterId ? (
        <AttachAgent wt={wt} />
      ) : boot === "ready" ? (
        <Composer root={wt.root} path={path} running={wt.status === "RUNNING"} acp={wt.runtime === "acp"} prefix={prefix || undefined} />
      ) : boot === "trust" ? (
        <div className="agent-bar-note small">처음 여는 폴더라 신뢰 확인이 필요해요 <button onClick={trust}>신뢰하고 시작</button></div>
      ) : typeof boot === "object" ? (
        <div className="agent-bar-note small err">{boot.error} <button onClick={retry}>다시 시도</button></div>
      ) : (
        <div className="agent-bar-note small muted"><Spinner /> {agentLabel(wt.adapterId)} 준비 중…</div>
      )}
    </div>
  );
}

/**
 * One agent in its own worktree, as a chat or as its real terminal (same process either way).
 * Used as a canvas block and as the full-screen agent view.
 */
export function AgentPanel({ path, view, onView, lod = false, lead, extra, className }: {
  path: string;
  view: AgentView;
  onView: (v: AgentView) => void;
  lod?: boolean;
  lead?: React.ReactNode; // header, before the status
  extra?: React.ReactNode; // header, before the discard button
  className: string;
}) {
  const wt = useWorktree(path);
  const setError = useStore((s) => s.setError);
  const events = useTranscript(wt?.root ?? "", path);
  const items = useMemo(() => fold(events), [events]);
  // the discard flow (asks, checks for work to lose, removes) is shared with the sidebar row menu
  const { discard, busy, dialog } = useDiscard(wt);
  const sw = useRuntimeSwitch(wt, onView);
  if (!wt) return null;
  const running = wt.status === "RUNNING";
  const toggle = sw.toggle;
  // a switch waiting for the turn to end: started here, or (another window) by the backend's "agent://switch"
  const waiting = sw.state?.phase === "waiting" || !!wt.switchPending;

  return (
    <div className={className} data-view={view}>
      <header>
        {lead}
        <AgentGlyph w={wt} />
        <b className="ellipsis" title={`${wt.name}\n브랜치 ${wt.branch}`}>{wt.name}</b>
        <AgentTag wt={wt} />
        <span className="spacer" />
        {sw.state?.phase === "switching" ? (
          <span className="muted small switch-note"><Spinner /> 전환 중…</span>
        ) : waiting ? (
          <span className="muted small switch-note ellipsis" title="지금 턴이 끝나면 대화를 넘겨요">
            턴이 끝나면 전환돼요 ·{" "}
            <button className="link small" onClick={() => ipc.stop(wt.path).catch(setError)}>지금 중단하고 전환</button>
          </span>
        ) : null}
        <span className="seg" title="보기 방식" aria-busy={!!sw.state || undefined}>
          {(["chat", "cli"] as AgentView[]).map((v) => (
            <button key={v} data-on={v === view || undefined} onClick={() => toggle(v)}>
              {v === "chat" ? "Chat" : "CLI"}
            </button>
          ))}
        </span>
        {extra}
        {busy && <span className="muted small discard-busy">{busy === "check" ? "확인 중…" : "지우는 중…"}</span>}
        <button className="icon danger trash" title={busy ? "폐기 진행 중" : "워크트리 폐기"} aria-label="워크트리 폐기" onClick={discard} disabled={!!busy} aria-busy={!!busy || undefined}>
          {/* trash outline (Lucide "trash-2", ISC) */}
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
            <path d="M3 6h18M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2M19 6l-1 14a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2L5 6M10 11v6M14 11v6" />
          </svg>
        </button>
      </header>
      {!wt.adapterId ? (
        <AttachAgent wt={wt} />
      ) : lod ? (
        <div className="lod">{lastText(items) || <span className="muted">아직 대화가 없어요</span>}</div>
      ) : (
        <>
          {view === "cli" ? (
            <>
              <ApprovalCards path={wt.path} />
              {sw.acp ? (
                // starts on the terminal when nothing runs; while the chat runtime holds the conversation (here or in
                // another place showing the chat) a bar offers to take it over: never on mount
                <XTerm root={wt.root} path={wt.path} start={() => sw.start("cli")} takeOver={() => toggle("cli")} />
              ) : (
                <XTerm root={wt.root} path={wt.path} />
              )}
            </>
          ) : (
            <ChatBody root={wt.root} path={wt.path} items={items} running={running} acp={sw.acp} onOpenCli={() => toggle("cli")} />
          )}
        </>
      )}
      {dialog}
    </div>
  );
}

/**
 * The agent's name tag in a header. Its tooltip says what runs the chat: the chat runtime (ACP), or why it runs on
 * the terminal; a real fallback (not the Settings choice, not the CLI view) also shows a small "· 터미널".
 */
function AgentTag({ wt }: { wt: Worktree }) {
  const note = wt.runtimeNote;
  const fellBack = wt.runtime === "pty" && !!note?.message && note.kind !== "forced";
  const title = wt.runtime === "acp" ? "채팅: Claude Agent(ACP)" : (note?.message ?? undefined);
  return (
    <span className="agent-tag muted small" title={title}>
      <AgentLogo id={wt.adapterId} size={14} />
      {agentLabel(wt.adapterId)}
      {fellBack && <span className="runtime-note">· 터미널</span>}
    </span>
  );
}

type SwitchState = { view: AgentView; phase: "switching" | "waiting" } | null;

/**
 * The Chat / CLI toggle of an agent with a chat runtime (catalog `runtimes` has "acp"). While its agent runs, the
 * conversation moves to the view's runtime first (`switch_runtime`: the chat on ACP when it can, the CLI on the
 * terminal, the same session), then the view changes; a running turn or an open approval makes it wait for the turn
 * to end; a background shell, or an old process that won't stop (offered as a new branch of the conversation), asks
 * first. Nothing running: the view changes at once and the new view starts the agent on its runtime as it mounts
 * (`start_view`, from the chat's `useAgentBoot` and the CLI's `XTerm start`), showing a start error in place. A mount
 * never takes a running conversation over: the same worktree may show the other view elsewhere (a pop-out, the full
 * view, a canvas block coming back from far zoom), and only a click here (or the CLI's "CLI로 전환") moves it.
 * Other agents just change the view: both views show the same terminal.
 */
function useRuntimeSwitch(wt: Worktree | undefined, onView: (v: AgentView) => void) {
  const setError = useStore((s) => s.setError);
  const setRuntime = useStore((s) => s.setRuntime);
  const setSwitchPending = useStore((s) => s.setSwitchPending);
  const [state, setState] = useState<SwitchState>(null);
  const last = useRef(0);
  const view = useRef(onView);
  view.current = onView;
  // a switch that ends after this panel went away (the turn it waited for ended later, another worktree is in the
  // full view now) changes no view and asks nothing: the backend already remembered the view for the next start
  useEffect(() => () => void ++last.current, []);
  const acp = !!agentDef(wt?.adapterId)?.runtimes.includes("acp");
  const root = wt?.root ?? "", path = wt?.path ?? "";

  /**
   * One switch call (default: after a running turn). "done" records what runs it now; any other end but a newer
   * switch's ends the "턴이 끝나면 전환돼요" the backend's "agent://switch" put up.
   */
  const claim = (v: AgentView, when: SwitchWhen = "afterTurn"): Promise<SwitchResult> =>
    ipc.switchRuntime(root, path, v, when).then(
      (r) => {
        if (r.status === "done") setRuntime(path, r.runtime, r.note);
        else if (r.status !== "superseded") setSwitchPending(path, null);
        return r;
      },
      (e) => {
        setSwitchPending(path, null);
        throw e;
      },
    );

  const toggle = async (v: AgentView) => {
    // nothing running: the mounting view starts it (no wait here for a cold start)
    if (!wt || !acp || !wt.runtime) {
      // an older switch still on its way gives up its view and spinner to this click
      ++last.current;
      setState(null);
      return onView(v);
    }
    // a newer click wins (the backend answers the older one "superseded"); the same view again cancels a wait
    const my = ++last.current;
    const mine = () => my === last.current;
    setState({ view: v, phase: "switching" });
    try {
      let when: SwitchWhen = "ifIdle";
      for (;;) {
        const r = await claim(v, when);
        if (!mine() || r.status === "superseded") return;
        if (r.status === "done") return view.current(v);
        if (r.status === "stuck") {
          const fork = await ask({ title: "전환하지 못했어요", body: "이전 프로세스가 끝나지 않았어요. 대화를 새 갈래로 열 수 있어요.", confirm: "새 갈래로 열기" });
          if (!fork || !mine()) return;
          const f = await ipc.forkSession(root, path, v);
          if (f.status === "done" && mine()) {
            setRuntime(path, f.runtime, f.note);
            view.current(v);
          }
          return;
        }
        if (r.busy === "shell") {
          const go = await ask({ title: "백그라운드 셸이 멈춰요", body: "지금 전환하면 돌고 있는 백그라운드 셸이 멈춰요", confirm: "그래도 전환", danger: true });
          if (!go || !mine()) return;
          when = "interrupt";
        } else if (when === "ifIdle") {
          setState({ view: v, phase: "waiting" });
          when = "afterTurn";
        } else return; // waited as long as the backend does: leave it as it is
      }
    } catch (e) {
      if (mine()) setError(e);
    } finally {
      if (mine()) {
        setState(null);
        setSwitchPending(path, null);
      }
    }
  };
  /** a view mounting: starts the agent on its runtime when it's off, else just says what runs it */
  const start = (v: AgentView): Promise<SwitchResult> =>
    ipc.startView(root, path, v).then((r) => {
      setRuntime(path, r.runtime, r.note);
      return r;
    });

  return { acp, state, toggle, start };
}

export function useTranscript(root: string, path: string) {
  const [events, setEvents] = useState<RuntimeEvent[]>([]);
  useEffect(() => {
    if (!root) return;
    // Load history, then append live events; seq skips ones the history already had. A newer epoch (the agent
    // restarted, or its stream moved to another conversation) reloads it all.
    let epoch = -1, seq = -1, loading = true;
    const queued: AgentEventPayload[] = [];
    let disposed = false;
    const load = () => {
      loading = true;
      ipc.transcript(root, path)
        .then((t) => {
          if (disposed) return;
          epoch = t.epoch;
          seq = t.seq;
          setEvents(t.events);
          loading = false;
          queued.splice(0).forEach(add);
        })
        .catch(() => {});
    };
    const add = (p: AgentEventPayload) => {
      if (loading) return void queued.push(p);
      if (p.epoch < epoch) return; // from a stream that has since started over
      if (p.epoch > epoch) {
        queued.push(p);
        return load();
      }
      // live-only: streamed text (the chat runtime) draws until its durable item lands and retires it with an
      // empty delta (`addDelta` drops its deltas then); approvals, exit and reset draw nothing here
      if (p.seq === null) {
        const ev = p.event;
        if (ev.type === "delta") setEvents((es) => addDelta(es, ev));
        return;
      }
      if (p.seq <= seq) return;
      seq = p.seq;
      const ev = p.event;
      setEvents((es) => [...es, ev]);
    };
    const un = listen<AgentEventPayload>("agent://event", (e) => {
      if (e.payload.worktreePath.toLowerCase() !== path.toLowerCase()) return;
      add(e.payload);
    });
    un.then(load);
    return () => {
      disposed = true;
      un.then((f) => f());
    };
  }, [root, path]);
  return events;
}

// ---------- events → chat items (chatFold.ts) ----------

export { fold, lastText };

const toolTarget = (input: any) =>
  input?.file_path ?? input?.command ?? input?.pattern ?? input?.path ?? input?.url ?? input?.description ?? "";

// ---------- views ----------

/** error: why it couldn't start (a missing executable, a login, an exit, a timeout), as the backend words it */
export type Boot = "starting" | "trust" | "ready" | { error: string };

/**
 * Starts the agent (if needed) and tracks it until it takes input. Every agent view waits on this. `view`: the
 * chat view of an agent with a chat runtime starts it on ACP when it's off (`start_view`). A running conversation,
 * an idle one on the terminal too, stays where it is until the user clicks Chat; the chat works on the terminal too.
 */
export function useAgentBoot(root: string, path: string, enabled = true, view?: AgentView) {
  const [boot, setBoot] = useState<Boot>("starting");
  const setRuntime = useStore((s) => s.setRuntime);
  const check = () => {
    setBoot("starting");
    // starts it on this view's runtime when it's off; never takes a running conversation over (the toggle does)
    const claim = view ? ipc.startView(root, path, view).catch(() => null) : Promise.resolve(null);
    claim
      .then(() => ipc.agentReady(root, path))
      .then((r) => {
        setRuntime(path, r.runtime, r.runtimeNote);
        setBoot(r.failure ? { error: r.failure.message } : r.trustPrompt ? "trust" : "ready");
      })
      .catch((e) => setBoot({ error: String(e) }));
  };
  // no agent on this worktree yet (found in the repo): nothing to start until one is attached
  useEffect(() => void (enabled && root && check()), [root, path, enabled]);
  const trust = () => ipc.acceptTrust(path).then(check, (e) => setBoot({ error: String(e) }));
  return { boot, retry: check, trust };
}

/** Chat view, shown only once the agent takes input: input pasted earlier would be lost. */
function ChatBody({ root, path, items, running, acp, onOpenCli }: {
  root: string;
  path: string;
  items: Item[];
  running: boolean;
  /** the agent has a chat runtime: started on it when nothing runs yet (a running conversation stays where it is) */
  acp: boolean;
  onOpenCli: () => void;
}) {
  const { boot, retry: check, trust } = useAgentBoot(root, path, true, acp ? "chat" : undefined);
  const wt = useWorktree(path);
  const label = agentLabel(wt?.adapterId);

  if (boot === "ready")
    return (
      <>
        <Messages items={items} running={running} />
        <ApprovalCards path={path} onOpenCli={onOpenCli} />
        <TuiWaitBand path={path} onOpenCli={onOpenCli} />
        <Composer root={root} path={path} running={running} acp={wt?.runtime === "acp"} />
      </>
    );
  return (
    <div className="boot">
      {boot === "starting" ? (
        <>
          <Spinner />
          <span className="muted">{label} 준비 중…</span>
        </>
      ) : boot === "trust" ? (
        <>
          <b>이 폴더를 신뢰할까요?</b>
          <span className="muted small">{label}이(가) 이 워크트리를 처음 열어서, 폴더 안의 파일과 설정을 믿어도 되는지 물어요.</span>
          <div className="boot-actions">
            <button className="primary" onClick={trust}>신뢰하고 시작</button>
            <button onClick={onOpenCli}>CLI에서 보기</button>
          </div>
        </>
      ) : (
        <>
          <span className="err wrap">{boot.error}</span>
          <div className="boot-actions">
            <button onClick={check}>다시 시도</button>
            <button onClick={onOpenCli}>CLI에서 보기</button>
          </div>
        </>
      )}
    </div>
  );
}

// Follow new output while the user is at the bottom; leave them alone once they scroll up.
function useStickyScroll(items: Item[], running: boolean) {
  const ref = useRef<HTMLDivElement>(null);
  const stick = useRef(true);
  useLayoutEffect(() => {
    if (stick.current && ref.current) ref.current.scrollTop = ref.current.scrollHeight;
  }, [items, running]);
  const onScroll = (e: React.UIEvent<HTMLDivElement>) => {
    const el = e.currentTarget;
    stick.current = el.scrollHeight - el.scrollTop - el.clientHeight < 40;
  };
  return { ref, onScroll };
}

function Messages({ items, running }: { items: Item[]; running: boolean }) {
  return (
    <div className="messages nodrag nowheel" {...useStickyScroll(items, running)}>
      {!items.length && <div className="muted empty-chat">무엇을 할지 아래에 입력하세요</div>}
      {items.map((it, i) => {
        switch (it.kind) {
          case "user":
            return (
              <div key={i} className="msg-user">
                <div className="bubble">{it.text}</div>
                {!!it.files.length && <div className="muted small">첨부: {it.files.join(", ")}</div>}
              </div>
            );
          case "text":
            return <RichText key={i} text={it.text} />;
          case "tool":
            return <ToolRow key={i} it={it} />;
          case "note":
            return <div key={i} className={`msg-note small${it.error ? " err" : " muted"}`}>{it.text}</div>;
        }
      })}
      {running && (
        <div className="thinking muted small">
          <Spinner /> 작업 중…
        </div>
      )}
    </div>
  );
}

/** One tool call, collapsed. Open: a file change's diff (the approval card's view) or the raw input, then the result. */
function ToolRow({ it }: { it: Extract<Item, { kind: "tool" }> }) {
  const [open, setOpen] = useState(false);
  const preview = useMemo(() => (open ? toolPreview(it.name, it.input) : null), [open, it.name, it.input]);
  const raw = open && !preview ? JSON.stringify(it.input, null, 2) : "";
  return (
    <div className="tool" data-error={it.isError || undefined}>
      <div className="tool-head" onClick={() => setOpen(!open)}>
        <span className="tool-caret caret" data-open={open || undefined}><CaretIcon /></span>
        <b>{it.name}</b>
        <span className="muted ellipsis">{String(toolTarget(it.input))}</span>
        {it.result === undefined && <Spinner />}
      </div>
      {open && preview && <ToolPreviewView p={preview} rows={40} hidePath />}
      {open && (!preview || it.result !== undefined) && (
        <pre className="tool-body">
          {raw}
          {it.result !== undefined && `${raw ? "\n\n" : ""}— 결과 —\n${it.result.length > 4000 ? it.result.slice(0, 4000) + "\n…" : it.result}`}
        </pre>
      )}
    </div>
  );
}

// Links can't navigate: a click would replace the whole app in the webview. Show the URL instead.
const MD_COMPONENTS: Components = {
  a: ({ href, children }) => <span className="md-link" title={href}>{children}</span>,
};

/** Agent replies are Markdown (GitHub flavor: tables, task lists, strikethrough). The editor's preview uses it too. */
export function RichText({ text, className = "msg-text" }: { text: string; className?: string }) {
  return (
    <div className={`${className} md`}>
      <Markdown remarkPlugins={[remarkGfm]} components={MD_COMPONENTS}>{text}</Markdown>
    </div>
  );
}

/**
 * The agent's terminal waits for an answer only the CLI view can give (a permission handed to the CLI or out of time,
 * a picker on screen): says so, and offers the CLI view. The composer is locked meanwhile (`Composer`).
 */
function TuiWaitBand({ path, onOpenCli }: { path: string; onOpenCli?: () => void }) {
  const wait = useTuiWait(path);
  const adapterId = useWorktree(path)?.adapterId;
  if (!wait) return null;
  const why = wait.source === "timeout" ? "승인 요청에 시간 안에 답이 없어서 " : "";
  return (
    <div className="tui-wait-band nodrag" role="status">
      <span className="approval-badge">승인 대기</span>
      <span className="tui-wait-text">
        {why}
        {agentLabel(adapterId)}이(가) 터미널에서 답을 기다려요{wait.toolName ? ` · ${wait.toolName}` : ""}
      </span>
      {onOpenCli && <button className="primary" onClick={onOpenCli}>터미널에서 답하기</button>}
    </div>
  );
}

/** acp: the chat runtime runs the conversation (no `!` shell lines; unsupported `/` commands say "CLI 보기에서") */
function Composer({ root, path, running, acp = false, prefix }: { root: string; path: string; running: boolean; acp?: boolean; prefix?: string }) {
  // the terminal waits for an answer: anything sent would land in its question (the backend refuses it too); the
  // draft stays
  const tui = !!useTuiWait(path);
  const [text, setText] = useState("");
  const [files, setFiles] = useState<string[]>([]);
  const [over, setOver] = useState(false);
  const setError = useStore((s) => s.setError);
  // the `/` menu and `/model` list are this agent's own (catalog): Antigravity doesn't get Claude's
  const adapterId = useWorktree(path)?.adapterId ?? "";
  const modelCommand = !!agentDef(adapterId)?.features.modelCommand;
  const [cmds, setCmds] = useState<SlashCmd[]>([]);
  const [models, setModels] = useState<SlashCmd[]>([]);
  const [hi, setHi] = useState(0);
  const [menuClosed, setMenuClosed] = useState(false); // Esc hides the menu until the text changes

  // asked again when the runtime changes: under ACP the adapter's own list marks what only the CLI view runs. That
  // list comes after the session is up (and changes as MCP servers connect): "agent://commands" says when.
  useEffect(() => {
    setCmds([]);
    if (!adapterId) return;
    let gone = false;
    const load = () => ipc.slashCommands(path, adapterId).then((c) => !gone && setCmds(c)).catch(() => {});
    load();
    const un = listen<{ worktreePath: string }>("agent://commands", (e) => e.payload.worktreePath.toLowerCase() === path.toLowerCase() && load());
    return () => {
      gone = true;
      un.then((f) => f());
    };
  }, [path, adapterId, acp]);
  useEffect(() => {
    setModels([]);
    if (modelCommand) ipc.agentModels(adapterId).then((m) => setModels(m.map((x) => ({ name: x.id, description: x.description, source: "모델" })))).catch(() => {});
  }, [adapterId, modelCommand]);

  // Like the TUI: `/` at the start opens the command menu, `!` runs a shell command.
  // `/model ` lists models here, since the TUI's own model picker isn't visible from chat.
  const slash = /^\/(\S*)$/.exec(text);
  const modelArg = modelCommand ? /^\/model\s+(\S*)$/.exec(text) : null;
  const matches = menuClosed
    ? []
    : modelArg
      ? matchCommands(models, modelArg[1])
      : slash
        ? matchCommands(cmds, slash[1]).slice(0, 8)
        : [];
  const bash = text.startsWith("!");
  const pick = (c: SlashCmd) => {
    setHi(0);
    if (modelArg) return send(`/model ${c.name}`);
    setText(`/${c.name} `);
  };

  const send = async (t = text) => {
    if (!t.trim() || tui) return;
    if (modelCommand && t.trim() === "/model") return setText("/model "); // open the model list instead of the TUI picker
    try {
      // what's selected on the canvas goes in front of a message (not of a slash command or a shell line)
      await ipc.send(root, path, prefix && !/^[/!]/.test(t) ? `${prefix}\n\n${t}` : t, files);
      useStore.getState().pinTab({ kind: "wt", path }); // worked in, so its tab stays (no longer a preview)
      setText("");
      setFiles([]);
    } catch (e) {
      setError(e);
    }
  };

  return (
    <div
      className="composer"
      data-over={over || undefined}
      data-mode={bash ? "bash" : undefined}
      onDragOver={(e) => {
        if (!e.dataTransfer.types.includes(DRAG_MIME)) return;
        e.preventDefault();
        setOver(true);
      }}
      onDragLeave={(e) => !e.currentTarget.contains(e.relatedTarget as Node) && setOver(false)} // not when crossing into a child
      onDrop={(e) => {
        setOver(false);
        const rel = e.dataTransfer.getData(DRAG_MIME);
        if (rel && !files.includes(rel)) setFiles([...files, rel]);
      }}
    >
      {!!files.length && (
        <div className="chips">
          {files.map((p) => (
            <span key={p} className="chip" title={p}>
              {p.split("/").pop()}
              <button title="빼기" aria-label={`${p.split("/").pop()} 빼기`} onClick={() => setFiles(files.filter((x) => x !== p))}><X size={12} /></button>
            </span>
          ))}
        </div>
      )}
      {!!matches.length && (
        <div className="slash-menu nowheel" role="listbox">
          {matches.map((c, i) => (
            <div
              key={c.name}
              role="option"
              aria-selected={i === hi}
              className="slash-item"
              onMouseEnter={() => setHi(i)}
              onMouseDown={(e) => {
                e.preventDefault(); // keep focus in the textarea
                pick(c);
              }}
            >
              <b>{modelArg ? c.name : `/${c.name}`}</b>
              <span className="ellipsis muted">{c.description}</span>
              <span className="spacer" />
              {c.note ? <span className="slash-note">{c.note}</span> : <span className="muted small">{c.source}</span>}
            </div>
          ))}
        </div>
      )}
      {bash && (
        <div className="bash-hint small">
          {acp ? "채팅에서는 셸 명령(!)을 쓸 수 없어요 · CLI 보기에서 실행" : "! Bash 모드 · 에이전트 터미널에서 이 명령을 바로 실행해요"}
        </div>
      )}
      <textarea
        className="nodrag nowheel"
        rows={2}
        value={text}
        placeholder={
          tui
            ? "CLI 보기에서 먼저 답해 주세요 · 지금 보내면 그 선택지에 들어가요"
            : acp
              ? "메시지 입력 · Enter 전송 · / 명령어 · 셸 명령(!)은 CLI 보기에서"
              : "메시지 입력 · Enter 전송 · / 명령어 · ! 셸 명령"
        }
        onChange={(e) => {
          setText(e.target.value);
          setHi(0);
          setMenuClosed(false);
        }}
        onKeyDown={(e) => {
          // don't act while a Korean syllable is still being composed (ime.ts)
          if (isImeComposing(e)) return;
          if (matches.length) {
            const n = matches.length;
            if (e.key === "ArrowDown" || e.key === "ArrowUp") {
              e.preventDefault();
              setHi((hi + (e.key === "ArrowDown" ? 1 : n - 1)) % n);
              return;
            }
            if (e.key === "Escape") return setMenuClosed(true);
            const cur = matches[Math.min(hi, n - 1)];
            // Tab completes; Enter completes (or, for a model, sends), or sends when the command is already fully typed
            if (e.key === "Tab" || (e.key === "Enter" && !e.shiftKey && (modelArg || text !== `/${cur.name}`))) {
              e.preventDefault();
              return pick(cur);
            }
          }
          if (e.key === "Enter" && !e.shiftKey) {
            e.preventDefault();
            send();
          }
        }}
      />
      <div className="composer-actions">
        <span className="muted small">파일을 끌어와 첨부</span>
        <span className="spacer" />
        {running && <button title={acp ? "지금 작업을 멈춰요" : "지금 작업을 멈춰요 (터미널의 Esc)"} onClick={() => ipc.stop(path).catch(setError)}>중단</button>}
        <button className="primary" disabled={!text.trim() || tui} title={tui ? "터미널의 질문에 먼저 답해 주세요" : undefined} onClick={() => send()}>보내기</button>
      </div>
    </div>
  );
}
