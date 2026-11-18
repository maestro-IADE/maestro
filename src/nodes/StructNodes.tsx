import { useState, type ReactNode } from "react";
import { isImeComposing } from "../ime";
import { createPortal } from "react-dom";
import { create } from "zustand";
import { BaseEdge, Handle, NodeResizer, Position, useReactFlow, useStore as useFlowStore, type EdgeProps, type NodeProps } from "@xyflow/react";
import { LineLabel } from "../relationsUi";
import { edgePath, useEdgeShape } from "../edgePath";
import { GRID, RichText } from "./AgentNode";
import { ChevronLeft, ChevronRight, FileText, Frame, Layers, Maximize2, MessageCircleQuestion, Minimize2, Plus, Shrink, Spline, Square, Trash2, X } from "../icons";
import { motionMs, useSettings } from "../settings";
import { selectAttention, useStore } from "../store";
import { waitsAt } from "../attention";
import { foldZone, goPhase, inside, type Metrics } from "../zones";
import { COLORS, COLOR_NAMES, colorVar, type RelColor } from "../relationModel";
import type { CanvasEdge, CanvasNode, StructNode, ZoneNode } from "../types";

/** A block's or area's border colour (structure-drawing.md): the line colours, as `--bc` for styles.css. */
const bordered = (c?: RelColor) => (c ? ({ "--bc": colorVar(c) } as React.CSSProperties) : undefined);

// structure drawing (FigJam-like): blocks, areas that hold them, arrows between any of them
export const BLOCK_SIZE = { width: 10 * GRID, height: 3 * GRID };
export const ZONE_SIZE = { width: 18 * GRID, height: 12 * GRID };
// areas sit under everything else (a selected one too), so blocks inside stay clickable
export const ZONE_Z = -1000;
/** A folded area: one line (docs/features/canvas/zones.md). */
export const ZONE_CHIP = { width: 10 * GRID, height: 2 * GRID };
/** A stack (zones.md "묶음"): the space between the blocks it holds, kept small so they read as one thing. */
export const STACK_GAP = 8;
/**
 * The numbers the area rules work with (zones.ts): a cell of padding around what an area holds, two on top (its name
 * sits there), the smallest open area, the chip, and the size of a block not measured yet.
 */
export const ZONE_METRICS: Metrics = { grid: GRID, pad: GRID, top: 2 * GRID, min: { width: 8 * GRID, height: 5 * GRID }, chip: ZONE_CHIP, fallback: BLOCK_SIZE };

const SIDES = [Position.Top, Position.Right, Position.Bottom, Position.Left];
const handles = SIDES.map((p) => <Handle key={p} id={p} type="source" position={p} className="wt-handle" />);

/** First line of the notes, Markdown marks stripped, for the block's face. */
const firstLine = (body = "") =>
  body
    .split("\n")
    .map((l) => l.replace(/^[#>*\-\s`]+/, "").trim())
    .find(Boolean) ?? "";

/** A block: its name, and the first line of its notes (double click opens them). */
export function StructBlock({ data, selected }: NodeProps<StructNode>) {
  const line = firstLine(data.body);
  return (
    <>
      {/* outside the block: its box clips what overflows, the resize handles sit on the edge */}
      <NodeResizer isVisible={selected} minWidth={4 * GRID} minHeight={2 * GRID} lineClassName="struct-resize-line" handleClassName="struct-resize-handle" />
      <div className="node struct-block" data-notes={!!data.body || undefined} data-color={data.color} style={bordered(data.color)}>
        {handles}
        <b className="ellipsis">{data.title || "이름 없음"}</b>
        {line && <span className="struct-line ellipsis">{line}</span>}
      </div>
    </>
  );
}

/**
 * Folding an area to its chip and back (zones.md): what it holds is hidden with it, and nothing hidden stays picked
 * (the blocks by `foldZone`, the lines by the canvas). The area moves at the motion setting's pace, like a folding
 * agent block.
 */
/**
 * The area that takes what is being dragged if it is let go now (the pointer is in it, zones.ts `hostAt`): it lights
 * up, so it shows before letting go whether the block goes in, stays in or comes out. Set by the canvas while dragging.
 */
export const useDropHost = create<{ id: string | null }>(() => ({ id: null }));

export function useZoneFold() {
  const { setNodes, updateNode } = useReactFlow<CanvasNode, CanvasEdge>();
  return (id: string, folded: boolean) => {
    setNodes((ns) => foldZone(ns, id, folded, ZONE_METRICS).map((n) => (n.id === id ? { ...n, className: "folding" } : n)));
    // the transition class only while it moves, or dragging the area would lag behind the pointer
    setTimeout(() => updateNode(id, { className: undefined }), motionMs(useSettings.getState()) + 50);
  };
}

/**
 * What a folded area's chip says it holds, as one string (so the store selector that reads it stays the same until
 * it changes): [blocks, questions still waiting for an answer, the agents in it (their folders)]. Areas inside
 * aren't counted.
 */
const heldBy = (ns: CanvasNode[], id: string) => {
  const kids = inside(ns, id);
  let count = 0, waiting = 0;
  const agents: string[] = [];
  for (const n of ns) {
    if (!kids.has(n.id) || n.type === "zone") continue;
    count++;
    if (n.type === "question_block" && !n.data.answer && !n.data.mine) waiting++;
    if (n.type === "pty_terminal_node" || n.type === "worktree_card") agents.push(n.data.worktreePath);
  }
  return JSON.stringify([count, waiting, agents]);
};

/**
 * An area: a labelled frame; blocks and areas dragged into it go inside (and move with it). It can't be dragged
 * smaller than what it holds, and it folds to a one-line chip that hides what it holds (zones.md): the chip says how
 * many blocks are inside, and in the warn colour when a question in there still waits for an answer.
 */
export function ZoneFrame({ id, data, selected }: NodeProps<ZoneNode>) {
  const fold = useZoneFold();
  const { getNodes, getInternalNode, setNodes } = useReactFlow<CanvasNode>();
  const held = useFlowStore((s) => (data.folded ? heldBy(s.nodes as CanvasNode[], id) : ""));
  const [count, waiting, agents] = (held ? JSON.parse(held) : [0, 0, []]) as [number, number, string[]];
  // agents folded away with it that wait for the user (a tool approval, a merge request): said on the chip, so
  // nothing stalls unseen behind it
  const asking = useStore((s) => (agents.length ? agents.filter((p) => waitsAt(selectAttention(s), p)).length : 0));
  // its steps or states, and the one showing (zones.ts): the control in its head moves it
  const phases = data.mode ? (data.phases ?? []) : [];
  const cur = phases.find((p) => p.id === data.at) ?? phases[0];
  const at = cur ? phases.indexOf(cur) : -1;
  const curName = cur ? (data.mode === "step" ? `${at + 1} / ${phases.length}${cur.name.trim() ? ` · ${cur.name.trim()}` : ""}` : cur.name.trim() || `상태 ${at + 1}`) : "";
  const go = (to: string) => setNodes((ns) => goPhase(ns, id, to));
  const taking = useDropHost((s) => s.id === id);
  if (data.folded) {
    return (
      <>
        {/* outside the chip so they sit on its edges, not clipped by it */}
        {handles}
        <div className="node zone-chip" data-waiting={waiting > 0 || asking > 0 || undefined} data-color={data.color} style={bordered(data.color)}>
          {data.stack ? <Layers size={14} className="muted" /> : <Frame size={14} className="muted" />}
          <b className="ellipsis">{data.title || (data.stack ? "묶음" : "영역")}</b>
          {cur && <span className="zone-count" title={data.mode === "step" ? "지금 단계" : "지금 상태"}>{curName}</span>}
          {asking > 0 && <span className="zone-wait" title="안의 에이전트가 승인이나 답을 기다려요. 펼치거나 아래 입력창, 위 막대의 확인할 일에서 봐요">확인 {asking}</span>}
          {waiting > 0 && <span className="zone-wait" title="안에 답을 기다리는 질문이 있어요">질문 {waiting}</span>}
          <span className="zone-count" title={`블록 ${count}개가 들어 있어요`}>{count}개</span>
          <button className="icon nodrag" title="펼치기" aria-label="펼치기" onClick={() => fold(id, false)}>
            <Maximize2 size={16} />
          </button>
        </div>
      </>
    );
  }
  // a side stops where it would cut a block inside: an area is never smaller than what it holds (a side that is
  // already past a block, on an older canvas, can still move out)
  const holds = (next: { x: number; y: number; width: number; height: number }) => {
    const me = getInternalNode(id);
    if (!me) return true;
    const a = me.internals.positionAbsolute;
    const cur = { x0: a.x, y0: a.y, x1: a.x + (me.measured.width ?? 0), y1: a.y + (me.measured.height ?? 0) };
    // `next` is in the coordinates of the area around this one: bring it onto the canvas
    const x0 = a.x - me.position.x + next.x, y0 = a.y - me.position.y + next.y;
    const x1 = x0 + next.width, y1 = y0 + next.height;
    return !getNodes().some((c) => {
      const k = c.parentId === id && !c.hidden ? getInternalNode(c.id) : undefined;
      if (!k) return false;
      const p = k.internals.positionAbsolute, w = k.measured.width ?? 0, h = k.measured.height ?? 0;
      return (x0 > p.x && x0 > cur.x0) || (y0 > p.y && y0 > cur.y0) || (x1 < p.x + w && x1 < cur.x1) || (y1 < p.y + h && y1 < cur.y1);
    });
  };
  return (
    <div className="zone" data-color={data.color} data-stack={data.stack || undefined} data-drop={taking || undefined} style={bordered(data.color)}>
      {/* a stack is exactly as big as what it holds (zones.ts `layStacks`): no sides to drag */}
      {!data.stack && (
        <NodeResizer
          isVisible={selected}
          minWidth={ZONE_METRICS.min.width}
          minHeight={ZONE_METRICS.min.height}
          shouldResize={(_, next) => holds(next)}
          lineClassName="struct-resize-line"
          handleClassName="struct-resize-handle"
        />
      )}
      {handles}
      <div className="zone-head">
        {data.stack && <Layers size={12} className="muted" />}
        <span className="zone-title" title={data.stack ? "묶음 · 블록을 세로로 줄 세워 하나로 묶어요. 블록을 끌어 넣고 빼고, 위아래로 끌어 순서를 바꿔요" : undefined}>{data.title || (data.stack ? "묶음" : "영역")}</span>
        {/* steps: back and on along the timeline; states: picked by name. Other things move it too (zones.md) */}
        {cur && data.mode === "step" && (
          <span className="zone-phase nodrag">
            <button className="icon" disabled={at <= 0} title="이전 단계" aria-label="이전 단계" onClick={() => go("prev")}>
              <ChevronLeft size={14} />
            </button>
            <span className="zone-phase-at" title={`지금 ${at + 1}단계 (모두 ${phases.length}단계)`}>{curName}</span>
            <button className="icon" disabled={at >= phases.length - 1} title="다음 단계" aria-label="다음 단계" onClick={() => go("next")}>
              <ChevronRight size={14} />
            </button>
          </span>
        )}
        {cur && data.mode === "state" && (
          <select className="zone-phase-select nodrag" value={cur.id} title="이 영역의 상태" aria-label="상태" onChange={(e) => go(e.target.value)}>
            {phases.map((p, k) => (
              <option key={p.id} value={p.id}>{p.name.trim() || `상태 ${k + 1}`}</option>
            ))}
          </select>
        )}
      </div>
      <button className="icon zone-fold nodrag" title="작게 접기 · 안의 블록을 숨기고 한 줄로 줄여요" aria-label="작게 접기" onClick={() => fold(id, true)}>
        <Minimize2 size={16} />
      </button>
    </div>
  );
}

/**
 * A line drawn between structure items: an instance of a relation (relations.ts; 기본 관계 unless it was given
 * another), in the picked line shape (edgePath.ts). Its look comes from the canvas (`shownEdges`: class, `--rel`,
 * heads); its label, hover card, title block and editor from `LineLabel` (relationsUi.tsx).
 */
export function ArrowEdge({ id, sourceX, sourceY, targetX, targetY, sourcePosition, targetPosition, markerEnd, markerStart, style, selected, data }: EdgeProps<CanvasEdge>) {
  const [path, x, y] = edgePath(useEdgeShape(), { sourceX, sourceY, sourcePosition, targetX, targetY, targetPosition });
  const [hover, setHover] = useState(false);
  return (
    // hovered (on the line or its label): `line-hot` lifts it over every block and line (styles.css)
    <g className={hover ? "line-hot" : undefined} onMouseEnter={() => setHover(true)} onMouseLeave={() => setHover(false)}>
      <BaseEdge path={path} markerEnd={markerEnd} markerStart={markerStart} style={style} className="arrow-path" interactionWidth={24} />
      <LineLabel id={id} x={x} y={y} data={data} selected={selected} hover={hover} onHover={setHover} cutTitle="선 지우기" />
    </g>
  );
}

/**
 * What an area's editor lists: a block or area directly in it (its node id and type, and the name it shows).
 * `when`: in an area with steps or states, the ones it exists in, as shown ("2단계", "모든 단계").
 */
export type Held = { id: string; type: string; name: string; when?: string };
/** A change to an area's steps or states, made in its editor (the canvas applies it: zones.ts). */
export type PhaseOp =
  | { op: "mode"; mode?: "step" | "state" }
  | { op: "add" }
  | { op: "rename"; id: string; name: string }
  | { op: "remove"; id: string }
  | { op: "go"; id: string };
const HELD_KINDS: Record<string, { label: string; icon: ReactNode }> = {
  struct_block: { label: "구조 블록", icon: <Square size={14} /> },
  zone: { label: "영역", icon: <Frame size={14} /> },
  markdown_block: { label: "문서", icon: <FileText size={14} /> },
  question_block: { label: "질문", icon: <MessageCircleQuestion size={14} /> },
};

/**
 * A block's or area's name and (for a block) its Markdown notes, opened by a double click. Changes apply as
 * you type; the canvas saves them like any other change. An area's editor also lists what the area holds
 * (zones.md): which blocks belong to it is otherwise not to be seen on the canvas.
 */
export function StructEditor({ node, onChange, onClose, onMerge, held, onPick, onFit, onPhases }: {
  node: StructNode | ZoneNode;
  onChange: (data: { title?: string; body?: string; color?: RelColor }) => void;
  onClose: () => void;
  /** 선으로 합치기 (relations.md): offered for a block with one drawn line in and one out */
  onMerge?: () => void;
  /** an area: the blocks and areas directly in it */
  held?: Held[];
  /** an area: one of them pressed in the list (the canvas picks it) */
  onPick?: (id: string) => void;
  /** an area: 내용에 맞추기 (not offered for a folded one) */
  onFit?: () => void;
  /** an area: its steps or states changed here (given: plain, steps or states; added, named, removed, gone to) */
  onPhases?: (op: PhaseOp) => void;
}) {
  const block = node.type === "struct_block";
  const body = block ? (node.data.body ?? "") : "";
  const [mode, setMode] = useState<"view" | "edit">(body ? "view" : "edit");
  // an area's steps or states (zones.md): `zmode` none = a plain area
  const zmode = node.type === "zone" ? node.data.mode : undefined;
  const zphases = node.type === "zone" && zmode ? (node.data.phases ?? []) : [];
  const zat = node.type === "zone" ? (zphases.find((p) => p.id === node.data.at) ?? zphases[0])?.id : undefined;
  const word = zmode === "state" ? "상태" : "단계";
  // a stack (zones.md "묶음"): a column of blocks, always as big as they are: no steps, nothing to fit
  const stack = node.type === "zone" && !!node.data.stack;
  return createPortal(
    <div className="modal-backdrop" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div
        className={block ? "modal struct-editor nokey" : "modal struct-editor zone-editor nokey"}
        tabIndex={-1}
        // reading notes has no field to focus: the dialog itself takes it (Esc closes, Delete stays here)
        ref={(el) => void (el && !el.contains(document.activeElement) && el.focus())}
        onKeyDown={(e) => e.key === "Escape" && onClose()}
      >
        <div className="struct-editor-head">
          <input
            className="struct-title-input"
            autoFocus={!node.data.title || !block}
            value={node.data.title}
            placeholder={block ? "블록 이름" : stack ? "묶음 이름" : "영역 이름"}
            onChange={(e) => onChange({ title: e.target.value })}
            onKeyDown={(e) => !block && e.key === "Enter" && !isImeComposing(e) && onClose()}
          />
          {/* its border colour: none (the plain border) or one of the line colours */}
          <div className="rel-colors block-colors" role="radiogroup" aria-label="테두리 색">
            <button className="rel-color" role="radio" aria-checked={!node.data.color} data-on={!node.data.color || undefined} title="테두리 색 없음" onClick={() => onChange({ color: undefined })}>
              <i className="block-color-none" />
            </button>
            {COLORS.map((c) => (
              <button key={c} className="rel-color" role="radio" aria-checked={node.data.color === c} data-on={node.data.color === c || undefined} title={`테두리 ${COLOR_NAMES[c]}`} onClick={() => onChange({ color: c })}>
                <i style={{ background: colorVar(c) }} />
              </button>
            ))}
          </div>
          {block && (
            <div className="seg">
              <button data-on={mode === "view" || undefined} onClick={() => setMode("view")}>보기</button>
              <button data-on={mode === "edit" || undefined} onClick={() => setMode("edit")}>편집</button>
            </div>
          )}
          {block && onMerge && (
            <button className="icon" title="선으로 합치기 · 이 블록을 앞뒤 블록을 잇는 선으로 되돌려요 (이름 → 라벨, 내용 → 본문)" aria-label="선으로 합치기" onClick={onMerge}>
              <Spline size={14} />
            </button>
          )}
          <button className="icon" title="닫기 (Esc)" aria-label="닫기" onClick={onClose}><X /></button>
        </div>
        {block &&
          (mode === "edit" ? (
            <textarea
              className="struct-body-input"
              autoFocus={!!node.data.title}
              value={body}
              placeholder={"이 블록이 맡는 일, 입출력, 의존성 같은 내용을 마크다운으로 적어요"}
              onChange={(e) => onChange({ body: e.target.value })}
            />
          ) : (
            <div className="struct-view">{body ? <RichText text={body} className="struct-md" /> : <p className="muted">내용이 없어요. 편집을 눌러 적어요.</p>}</div>
          ))}
        {!block && !stack && onPhases && (
          <div className="zone-held">
            <div className="zone-held-head">
              <span>보이는 때</span>
              <div className="seg" role="radiogroup" aria-label="보이는 때">
                <button role="radio" aria-checked={!zmode} data-on={!zmode || undefined} title="든 것이 늘 보여요" onClick={() => onPhases({ op: "mode" })}>늘</button>
                <button role="radio" aria-checked={zmode === "step"} data-on={zmode === "step" || undefined} title="순서대로 넘기는 타임라인" onClick={() => onPhases({ op: "mode", mode: "step" })}>단계</button>
                <button role="radio" aria-checked={zmode === "state"} data-on={zmode === "state" || undefined} title="이름으로 고르는 상황" onClick={() => onPhases({ op: "mode", mode: "state" })}>상태</button>
              </div>
              <span className="spacer" />
              {zmode && (
                <button className="small" onClick={() => onPhases({ op: "add" })}>
                  <Plus size={14} /> {word} 추가
                </button>
              )}
            </div>
            {zmode ? (
              <ul className="zone-phase-list">
                {zphases.map((p, k) => (
                  <li key={p.id} data-on={p.id === zat || undefined}>
                    <button className="zone-phase-go" title={`이 ${word} 보기`} aria-label={`${k + 1}번째 ${word} 보기`} aria-pressed={p.id === zat} onClick={() => onPhases({ op: "go", id: p.id })}>
                      {k + 1}
                    </button>
                    <input value={p.name} placeholder={zmode === "step" ? `${k + 1}단계 이름 (선택)` : `상태 ${k + 1}`} onChange={(e) => onPhases({ op: "rename", id: p.id, name: e.target.value })} />
                    <button className="icon" disabled={zphases.length < 2} title={`지우기 · 이 ${word}에만 있던 블록은 남아서 모든 ${word}에 보여요`} aria-label={`${word} 지우기`} onClick={() => onPhases({ op: "remove", id: p.id })}>
                      <Trash2 size={14} />
                    </button>
                  </li>
                ))}
              </ul>
            ) : (
              <p className="muted zone-held-none">단계(순서대로 넘기는 타임라인)나 상태(이름으로 고르는 상황)를 주면, 든 것마다 어느 단계·상태에 있을지 정할 수 있어요.</p>
            )}
          </div>
        )}
        {!block && held && (
          <div className="zone-held">
            <div className="zone-held-head">
              <span>든 것 <span className="muted">{held.length}개</span></span>
              <span className="spacer" />
              {onFit && !stack && (
                <button className="small" disabled={!held.length} title="영역을 든 것에 딱 맞게 줄여요" onClick={onFit}>
                  <Shrink size={14} /> 내용에 맞추기
                </button>
              )}
            </div>
            {held.length ? (
              <ul className="zone-held-list">
                {held.map((h) => {
                  const kind = HELD_KINDS[h.type] ?? HELD_KINDS.struct_block;
                  return (
                    <li key={h.id}>
                      <button title="캔버스에서 이 블록을 골라요" onClick={() => onPick?.(h.id)}>
                        {kind.icon}
                        <span className="ellipsis">{h.name}</span>
                        {h.when && <span className="muted">{h.when}</span>}
                        <span className="muted">{kind.label}</span>
                      </button>
                    </li>
                  );
                })}
              </ul>
            ) : (
              <p className="muted zone-held-none">{stack ? "아직 든 것이 없어요." : "아직 든 것이 없어요. 블록을 끌어서 이 영역 안에 놓으면 여기에 나와요."}</p>
            )}
          </div>
        )}
      </div>
    </div>,
    document.body,
  );
}
