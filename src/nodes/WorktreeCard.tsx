import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { isImeComposing } from "../ime";
import { Spinner } from "../LogoMotion";
import { MaestroIcon, PersonaIcon } from "../icons";
import { Handle, Position, useReactFlow, type NodeProps } from "@xyflow/react";
import { ipc } from "../ipc";
import { AgentGlyph, glyphState, statusText } from "../status";
import { useMergeAsked, usePr, useStore, useWaiting, useWorktree } from "../store";
import type { WorktreeCardNode } from "../types";
import { fold, lastText, RefTags, useAgentBoot, useFold, useTranscript } from "./AgentNode";
import { AgentLogo, agentLabel } from "../agents";
import { useSettings } from "../settings";
import { AttachAgent } from "./AttachAgent";
import { PrChip } from "./PrCard";
import { ciOf } from "../attention";

// 13 × 11 grid cells
export const CARD_SIZE = { width: 312, height: 264 };
/** folded: just the header, the persona / document tags (two lines, when there are any) and the prompt box */
export const CARD_COMPACT = { width: 312, height: 84 };

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
  const mergeAsked = useMergeAsked(data.worktreePath);
  const pr = usePr(data.worktreePath);
  const events = useTranscript(wt?.root ?? "", data.worktreePath);
  const last = useMemo(() => lastText(fold(events)), [events]);
  const [changed, setChanged] = useState<number | null>(null);
  const [summary, setSummary] = useState<string | null>(null);
  const running = wt?.status === "RUNNING";
  // a card stands for a live agent: start its CLI and wait for it, like the chat view does
  const { boot, retry, trust } = useAgentBoot(wt?.root ?? "", data.worktreePath, !!wt?.adapterId);
  const ready = boot === "ready";

  // refresh the change count whenever a turn starts or ends
  useEffect(() => {
    ipc.gitStatus(data.worktreePath).then((f) => setChanged(f.length)).catch(() => setChanged(null));
  }, [data.worktreePath, wt?.status]);

  // Haiku summary of the latest answer. The backend keeps one per answer, so a visit shows the cached one
  // at once and Haiku runs only when an answer has no summary yet (right after a turn ends).
  const [summarizing, setSummarizing] = useState(false);
  const { assistantAgent, assistantModel } = useSettings();
  // only what the transcript holds asks for a summary: live text deltas (chat runtime) keep coming ~every 50ms and
  // can outlast the turn's end (Claude writes end_turn before the answer line), and their retire changes the count too
  const durable = useMemo(() => events.filter((e) => e.type !== "delta").length, [events]);
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

  if (!wt) return null;
  return (
    <div ref={box} className="node wt-card" data-compact={data.compact || undefined} data-waiting={waiting || undefined} data-maestro={wt.isMaestro || undefined}>
      {/* connection points on every side: lines go Maestro ↔ worker (Canvas uses loose connection mode) */}
      {(["top", "right", "bottom", "left"] as const).map((side) => (
        <Handle key={side} id={side} type="source" position={SIDES[side]} className="wt-handle" />
      ))}
      <header>
        <AgentGlyph w={wt} />
        {wt.isMaestro && <span className="maestro-crown" title="이 프로젝트의 마에스트로"><MaestroIcon /></span>}
        <b className="ellipsis" title={`${wt.name}\n${wt.isMaestro ? `프로젝트 폴더 · ${wt.branch}` : `브랜치 ${wt.branch}`}`}>{wt.name}</b>
        {wt.isMaestro && <span className="muted small">워커 {wt.workers.length}</span>}
        <span className="spacer" />
        {wt.external && <span className="ext-badge" title="Maestro 밖에서 만든 워크트리 (폐기해도 폴더는 그대로)">외부</span>}
        {mergeAsked && <span className="merge-chip" title="마에스트로가 이 브랜치의 병합을 요청했어요. 마에스트로 블록의 카드에서 승인하거나 거부해요">병합 요청</span>}
        <PrChip path={data.worktreePath} />
        <span className="agent-tag muted small"><AgentLogo id={wt.adapterId} size={14} />{agentLabel(wt.adapterId)}</span>
        {waiting && data.compact && <span className="wt-card-waiting small">승인 대기</span>}
        <button
          className="icon nodrag wt-card-fold"
          title={data.compact ? "펼치기" : "작게 접기"}
          aria-label={data.compact ? "펼치기" : "작게 접기"}
          onClick={() => setFolded(!data.compact)}
          onDoubleClick={(e) => e.stopPropagation()}
        >
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
            <path d={data.compact ? "M15 3h6v6M9 21H3v-6M21 3l-7 7M3 21l7-7" : "M4 14h6v6M20 10h-6V4M14 10l7-7M3 21l7-7"} />
          </svg>
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
          ) : boot === "starting" ? (
            <span className="muted"><Spinner /> {agentLabel(wt.adapterId)} 준비 중…</span>
          ) : boot === "trust" ? (
            <span>
              처음 여는 폴더라 신뢰 확인이 필요해요.{" "}
              <button className="nodrag" onClick={trust}>신뢰하고 시작</button>
            </span>
          ) : typeof boot === "object" ? (
            <span className="err">
              {boot.error} <button className="nodrag" onClick={retry}>다시 시도</button>
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
      {wt.adapterId && <QuickPrompt root={wt.root} path={wt.path} ready={ready} />}
    </div>
  );
}

/** One-line prompt straight to this worktree's agent, without opening it. */
function QuickPrompt({ root, path, ready }: { root: string; path: string; ready: boolean }) {
  const [text, setText] = useState("");
  const [busy, setBusy] = useState(false);
  const setError = useStore((s) => s.setError);
  const send = async () => {
    if (!text.trim() || busy) return;
    setBusy(true);
    try {
      await ipc.send(root, path, text);
      setText("");
    } catch (e) {
      setError(e);
    }
    setBusy(false);
  };
  return (
    <div className="wt-card-prompt nodrag" onDoubleClick={(e) => e.stopPropagation()}>
      <input
        value={text}
        disabled={busy || !ready}
        placeholder={!ready ? "CLI 준비 중…" : busy ? "보내는 중…" : "바로 지시하기 · Enter"}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && !isImeComposing(e)) send();
        }}
      />
      <button className="primary" disabled={!text.trim() || busy || !ready} onClick={send}>↑</button>
    </div>
  );
}
