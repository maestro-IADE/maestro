import { useState } from "react";
import { isImeComposing } from "../ime";
import { createPortal } from "react-dom";
import { BaseEdge, Handle, NodeResizer, Position, type EdgeProps, type NodeProps } from "@xyflow/react";
import { LineLabel } from "../relationsUi";
import { edgePath, useEdgeShape } from "../edgePath";
import { GRID, RichText } from "./AgentNode";
import { Spline, X } from "../icons";
import type { CanvasEdge, StructNode, ZoneNode } from "../types";

// structure drawing (FigJam-like): blocks, areas that hold them, arrows between any of them
export const BLOCK_SIZE = { width: 10 * GRID, height: 3 * GRID };
export const ZONE_SIZE = { width: 18 * GRID, height: 12 * GRID };
// areas sit under everything else (a selected one too), so blocks inside stay clickable
export const ZONE_Z = -1000;

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
      <div className="node struct-block" data-notes={!!data.body || undefined}>
        {handles}
        <b className="ellipsis">{data.title || "이름 없음"}</b>
        {line && <span className="struct-line ellipsis">{line}</span>}
      </div>
    </>
  );
}

/** An area: a labelled frame; blocks and areas dragged into it go inside (and move with it). */
export function ZoneFrame({ data, selected }: NodeProps<ZoneNode>) {
  return (
    <div className="zone">
      <NodeResizer isVisible={selected} minWidth={8 * GRID} minHeight={5 * GRID} lineClassName="struct-resize-line" handleClassName="struct-resize-handle" />
      {handles}
      <span className="zone-title">{data.title || "영역"}</span>
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
    <g onMouseEnter={() => setHover(true)} onMouseLeave={() => setHover(false)}>
      <BaseEdge path={path} markerEnd={markerEnd} markerStart={markerStart} style={style} className="arrow-path" interactionWidth={24} />
      <LineLabel id={id} x={x} y={y} data={data} selected={selected} hover={hover} onHover={setHover} cutTitle="선 지우기" />
    </g>
  );
}

/**
 * A block's or area's name and (for a block) its Markdown notes, opened by a double click. Changes apply as
 * you type; the canvas saves them like any other change.
 */
export function StructEditor({ node, onChange, onClose, onMerge }: {
  node: StructNode | ZoneNode;
  onChange: (data: { title?: string; body?: string }) => void;
  onClose: () => void;
  /** 선으로 합치기 (relations.md): offered for a block with one drawn line in and one out */
  onMerge?: () => void;
}) {
  const block = node.type === "struct_block";
  const body = block ? (node.data.body ?? "") : "";
  const [mode, setMode] = useState<"view" | "edit">(body ? "view" : "edit");
  return createPortal(
    <div className="modal-backdrop" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div
        className="modal struct-editor nokey"
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
            placeholder={block ? "블록 이름" : "영역 이름"}
            onChange={(e) => onChange({ title: e.target.value })}
            onKeyDown={(e) => !block && e.key === "Enter" && !isImeComposing(e) && onClose()}
          />
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
      </div>
    </div>,
    document.body,
  );
}
