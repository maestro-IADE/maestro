import { useEffect, useState } from "react";
import { isImeComposing } from "../ime";
import { createPortal } from "react-dom";
import { BaseEdge, EdgeLabelRenderer, Handle, NodeResizer, Position, useReactFlow, type EdgeProps, type NodeProps } from "@xyflow/react";
import { edgePath, useEdgeShape } from "../edgePath";
import { GRID, RichText } from "./AgentNode";
import { X } from "../icons";
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

/** An arrow between structure items, in the picked line shape (edgePath.ts), with an optional label (double click edits it). */
export function ArrowEdge({ id, sourceX, sourceY, targetX, targetY, sourcePosition, targetPosition, markerEnd, selected, data }: EdgeProps<CanvasEdge>) {
  const [path, x, y] = edgePath(useEdgeShape(), { sourceX, sourceY, sourcePosition, targetX, targetY, targetPosition });
  const { deleteElements, updateEdgeData } = useReactFlow();
  const [text, setText] = useState(data?.label ?? "");
  useEffect(() => setText(data?.label ?? ""), [data?.editing]);
  const done = () => updateEdgeData(id, { label: text.trim() || undefined, editing: false });
  return (
    <>
      <BaseEdge path={path} markerEnd={markerEnd} className="arrow-path" interactionWidth={24} />
      <EdgeLabelRenderer>
        <div className="arrow-label nodrag nopan" style={{ transform: `translate(-50%, -50%) translate(${x}px, ${y}px)` }}>
          {data?.editing ? (
            <input
              autoFocus
              value={text}
              placeholder="설명 · Enter"
              onChange={(e) => setText(e.target.value)}
              onBlur={done}
              onKeyDown={(e) => {
                if (isImeComposing(e)) return;
                if (e.key === "Enter") done();
                if (e.key === "Escape") updateEdgeData(id, { editing: false });
              }}
            />
          ) : (
            data?.label && <span className="arrow-text">{data.label}</span>
          )}
          {selected && !data?.editing && (
            <button className="edge-cut" title="화살표 지우기" aria-label="화살표 지우기" onClick={() => deleteElements({ edges: [{ id }] })}>
              <X size={12} />
            </button>
          )}
        </div>
      </EdgeLabelRenderer>
    </>
  );
}

/**
 * A block's or area's name and (for a block) its Markdown notes, opened by a double click. Changes apply as
 * you type; the canvas saves them like any other change.
 */
export function StructEditor({ node, onChange, onClose }: {
  node: StructNode | ZoneNode;
  onChange: (data: { title?: string; body?: string }) => void;
  onClose: () => void;
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
