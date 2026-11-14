import { Handle, Position, type NodeProps } from "@xyflow/react";
import type { MarkdownNode } from "../types";

// 10 × 3 grid cells
export const MD_SIZE = { width: 10 * 24, height: 3 * 24 }; // 10 × 3 grid cells, like a structure block

const SIDES = [Position.Top, Position.Right, Position.Bottom, Position.Left];

/** A Markdown file on the canvas: just its `#` title for now (viewer/editor later). Lines attach on any side. */
export function MarkdownBlock({ data }: NodeProps<MarkdownNode>) {
  const file = data.path.split("/").pop();
  return (
    <div className="node md-block" title={data.path}>
      {SIDES.map((p) => (
        <Handle key={p} id={p} type="source" position={p} className="wt-handle" />
      ))}
      <span className="md-block-icon" aria-hidden>MD</span>
      <div className="md-block-text">
        <b className="ellipsis" title={data.title}>{data.title}</b>
        <span className="muted small ellipsis">{file}</span>
      </div>
    </div>
  );
}
