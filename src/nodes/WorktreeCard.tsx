import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { isImeComposing } from "../ime";
import { Spinner } from "../LogoMotion";
import { Ellipsis, Maximize2, Minimize2, PersonaIcon } from "../icons";
import { useAgentMenu } from "../agentMenu";
import { Handle, Position, useReactFlow, type NodeProps } from "@xyflow/react";
import { ipc } from "../ipc";
import { AgentGlyph, glyphState, statusText } from "../status";
import { selectAttention, usePr, useStore, useWaiting, useWorktree } from "../store";
import type { WorktreeCardNode } from "../types";
import { GRID, RefTags, useAgentBoot, useChat, useFold } from "./AgentNode";
import { lastText } from "../chatFold";
import { AgentLogo, agentLabel } from "../agents";
import { useSettings } from "../settings";
import { AttachAgent } from "./AttachAgent";
import { PrChip } from "./PrCard";
import { cardItem, ciOf } from "../attention";
import { AttentionRow, MergeDiff, openAgent } from "./Attention";

// 13 × 11 grid cells
export const CARD_SIZE = { width: 14 * GRID, height: 11 * GRID };
/** folded: just the header, the persona / document tags (two lines, when there are any) and the prompt box */
export const CARD_COMPACT = { width: 14 * GRID, height: 4 * GRID };

const SIDES = { top: Position.Top, right: Position.Right, bottom: Position.Bottom, left: Position.Left };

/** Project canvas: what one worktree is up to, with a quick prompt box. Double-click opens it. */
export function WorktreeCard({ id, data }: NodeProps<WorktreeCardNode>) {
  const wt = useWorktree(data.worktreePath);
  const tags = !!wt && wt.personas.length + wt.docs.length > 0;
  const setFolded = useFold(id, (compact) => ({ size: compact ? CARD_COMPACT : CARD_SIZE, data: { ...data, compact } }));
  const { updateNode } = useReactFlow();
  // folded, the card is as tall as its content (one header line, or two with tags): the block follows it
  const box = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    const el = box.current;
    if (!data.compact || !el) return;
    const fit = () => updateNode(id, (n) => (n.height === el.offsetHeight ? {} : { height: el.offsetHeight }));
    fit();
    const ro = new ResizeObserver(fit);
    ro.observe(el);
    return () => ro.disconnect();
  }, [data.compact, tags]);
  const waiting = useWaiting(data.worktreePath);
  // what waits on this worker (its approval, its terminal question, its branch's merge request): answered right here,
  // in the prompt box's place (the same height, so the card doesn't grow into its neighbours)
  const item = useStore((s) => cardItem(selectAttention(s), data.worktreePath));
  const [diff, setDiff] = useState<number | null>(null);
  const pr = usePr(data.worktreePath);
  const chat = useChat(wt?.root ?? "", data.worktreePath);
  const last = useMemo(() => lastText(chat), [chat]);
  const [changed, setChanged] = useState<number | null>(null);
  const [summary, setSummary] = useState<string | null>(null);
  const running = wt?.status === "RUNNING";
  // a card only shows its agent: opening the Lead canvas starts none of them (sending does, in the backend). The
  // start-up states below show only once a send ran into one (the folder trust question, a launch failure)
  const [woken, setWoken] = useState(false);
  const { boot, retry, trust } = useAgentBoot(wt?.root ?? "", data.worktreePath, !!wt?.adapterId && woken);
  const ready = !woken || boot === "ready";
  const shown = woken ? boot : "ready";

  // refresh the change count whenever a turn starts or ends
  useEffect(() => {
    ipc.gitStatus(data.worktreePath).then((f) => setChanged(f.length)).catch(() => setChanged(null));
  }, [data.worktreePath, wt?.status]);

  // Haiku summary of the latest answer. The backend keeps one per answer, so a visit shows the cached one
  // at once and Haiku runs only when an answer has no summary yet (right after a turn ends).
  const [summarizing, setSummarizing] = useState(false);
  const { assistantAgent, assistantModel } = useSettings();
  // only what the transcript holds asks for a summary: live text deltas (chat runtime) keep coming ~every 50ms and
  // can outlast the turn's end (Claude writes end_turn before the answer line): `durable` counts durable events only
  const durable = chat.durable;
  useEffect(() => {
    if (!wt || running || !durable || assistantAgent === "off") return setSummary(null);
    let live = true;
    setSummarizing(true);
    ipc.summarize(wt.root, wt.path, assistantAgent, assistantModel)
      .then((s) => live && setSummary(s))
      .catch(() => {})
      .finally(() => live && setSummarizing(false));
    return () => {
      live = false;
    };
  }, [wt?.root, running, durable, assistantAgent, assistantModel]);

  const agentMenu = useAgentMenu(wt);
  if (!wt) return null;
  return (
    <>
    <div
      ref={box}
      // ⋯ in the head or a right click anywhere on the card: the agent's menu (the sidebar row's)
      onContextMenu={(e) => {
        e.preventDefault();
        agentMenu.open({ x: e.clientX, y: e.clientY }, e.currentTarget);
      }}
      className="node wt-card" data-compact={data.compact || undefined} data-waiting={waiting || !!item || undefined} data-maestro={wt.isMaestro || undefined}>
      {/* connection points on every side: lines go Maestro ↔ worker (Canvas uses loose connection mode) */}
      {(["top", "right", "bottom", "left"] as const).map((side) => (
        <Handle key={side} id={side} type="source" position={SIDES[side]} className="wt-handle" />
      ))}
      <header>
        <AgentGlyph w={wt} />
        <b className="ellipsis" title={`${wt.name}\n${wt.isMaestro ? `프로젝트 폴더 · ${wt.branch}` : `브랜치 ${wt.branch}`}`}>{wt.name}</b>
        {wt.isMaestro && <span className="muted small">워커 {wt.workers.length}</span>}
        <span className="spacer" />
        {wt.external && <span className="ext-badge" title="앱 밖에서 만든 워크트리 (폐기해도 폴더는 그대로)">외부</span>}
        <PrChip path={data.worktreePath} />
        <span className="agent-tag muted small"><AgentLogo id={wt.adapterId} size={14} />{agentLabel(wt.adapterId)}</span>
        <button
          className="icon nodrag wt-card-fold"
          title={data.compact ? "펼치기" : "작게 접기"}
          aria-label={data.compact ? "펼치기" : "작게 접기"}
          onClick={() => setFolded(!data.compact)}
          onDoubleClick={(e) => e.stopPropagation()}
        >
          {data.compact ? <Maximize2 size={14} /> : <Minimize2 size={14} />}
        </button>
        <button
          className="icon nodrag wt-card-fold"
          title="에이전트 메뉴"
          aria-label="에이전트 메뉴"
          onClick={(e) => agentMenu.open(e.currentTarget, e.currentTarget)}
          onDoubleClick={(e) => e.stopPropagation()}
        >
          <Ellipsis size={14} />
        </button>
        {/* folded, the tags sit in the name area, under the name */}
        {data.compact && <RefTags wt={wt} />}
      </header>
      {!data.compact && <div className="wt-card-body">
        {wt.description && <div className="wt-card-desc muted small">{wt.description}</div>}
        {!!wt.personas.length && (
          <div className="wt-card-personas">
            {wt.personas.map((p) => <span key={p} className="persona-badge" title="연결된 페르소나"><PersonaIcon />{p}</span>)}
          </div>
        )}
        <div className="wt-card-meta small">
          <span className={waiting ? "wt-card-waiting" : "muted"}>{statusText(glyphState({ status: wt.status, waiting, unread: wt.unread, noAgent: !wt.adapterId }))}</span>
          <span className="muted">· {changed === null ? "변경 확인 중" : changed ? `변경 ${changed}개 파일` : "변경 없음"}</span>
          {pr?.state === "OPEN" && ciOf(pr) === "failure" && <span className="pr-ci-fail">· CI 실패 {pr.checks.failed}</span>}
        </div>
        <div className="wt-card-last">
          {!wt.adapterId ? (
            <AttachAgent wt={wt} compact />
          ) : shown === "starting" ? (
            <span className="muted"><Spinner /> {agentLabel(wt.adapterId)} 준비 중…</span>
          ) : shown === "trust" ? (
            <span>
              처음 여는 폴더라 신뢰 확인이 필요해요.{" "}
              <button className="nodrag" onClick={trust}>신뢰하고 시작</button>
            </span>
          ) : typeof shown === "object" ? (
            <span className="err">
              {shown.error} <button className="nodrag" onClick={retry}>다시 시도</button>
            </span>
          ) : running ? (
            <span className="muted">{last || "작업 중…"}</span>
          ) : summary ? (
            <span title={last}>{summary}</span>
          ) : summarizing ? (
            <span className="muted"><Spinner /> 답변 요약 중…</span>
          ) : (
            last || <span className="muted">아직 대화가 없어요</span>
          )}
        </div>
      </div>}
      {item && (
        <div className="wt-card-attn" onDoubleClick={(e) => e.stopPropagation()}>
          {/* [열기] opens this worker (the merge request's Maestro is the canvas it's on) */}
          <AttentionRow item={item} size="compact" onOpen={() => openAgent(wt.path)} onDiff={setDiff} />
        </div>
      )}
      {/* kept mounted under the line, so a half-typed instruction survives an approval */}
      {wt.adapterId && <QuickPrompt root={wt.root} path={wt.path} ready={ready} hidden={!!item} onBlocked={() => setWoken(true)} />}
      {diff != null && <MergeDiff id={diff} onClose={() => setDiff(null)} />}
    </div>
    {agentMenu.dialog}
    </>
  );
}

/** One-line prompt straight to this worktree's agent, without opening it. */
function QuickPrompt({ root, path, ready, hidden, onBlocked }: { root: string; path: string; ready: boolean; hidden?: boolean; onBlocked: () => void }) {
  const [text, setText] = useState("");
  const [busy, setBusy] = useState(false);
  const setError = useStore((s) => s.setError);
  const field = useRef<HTMLInputElement>(null);
  const send = async () => {
    if (!text.trim() || busy) return;
    setBusy(true);
    try {
      await ipc.send(root, path, text);
      setText("");
    } catch (e) {
      // the agent couldn't take it (the folder trust question, a failed start): the card shows why, with its button
      if (/신뢰|시작|실행/.test(String(e))) onBlocked();
      else setError(e);
    }
    setBusy(false);
    field.current?.focus(); // the next instruction goes right in
  };
  return (
    <div className="wt-card-prompt nodrag" hidden={hidden} onDoubleClick={(e) => e.stopPropagation()}>
      <input
        ref={field}
        value={text}
        // read-only (not disabled) while sending, so the field keeps the focus for the next instruction
        readOnly={busy}
        disabled={!ready}
        placeholder={!ready ? "준비 중…" : busy ? "보내는 중…" : "바로 지시하기 · Enter"}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && !isImeComposing(e)) send();
        }}
      />
      <button className="primary" disabled={!text.trim() || busy || !ready} onClick={send}>보내기</button>
    </div>
  );
}
