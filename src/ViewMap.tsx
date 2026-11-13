import { useState } from "react";
import { Panel, useReactFlow, useStore } from "@xyflow/react";
import type { CanvasNode } from "./types";

const SPAN = 2; // the map covers the visible area ×2 on each axis
type Box = { x: number; y: number; w: number; h: number };

/**
 * Minimap locked to the current view: the view is framed in the middle with its surroundings around it.
 * LoL-style: press to put the camera there, hold and drag to sweep it. The map stays still while held.
 */
export function ViewMap({ nodes }: { nodes: CanvasNode[] }) {
  const [tx, ty, zoom] = useStore((s) => s.transform);
  const w = useStore((s) => s.width);
  const h = useStore((s) => s.height);
  const { setViewport, getInternalNode } = useReactFlow();
  const [held, setHeld] = useState<Box | null>(null);
  if (!w || !h) return null;

  // visible rect and map box, in flow coordinates
  const view = { x: -tx / zoom, y: -ty / zoom, w: w / zoom, h: h / zoom };
  const box = held ?? { x: view.x - (view.w * (SPAN - 1)) / 2, y: view.y - (view.h * (SPAN - 1)) / 2, w: view.w * SPAN, h: view.h * SPAN };

  const centerAt = (e: React.PointerEvent<SVGSVGElement>, b: Box) => {
    const r = e.currentTarget.getBoundingClientRect();
    const fx = b.x + ((e.clientX - r.left) / r.width) * b.w;
    const fy = b.y + ((e.clientY - r.top) / r.height) * b.h;
    setViewport({ x: w / 2 - fx * zoom, y: h / 2 - fy * zoom, zoom });
  };

  return (
    <Panel position="bottom-right">
      <svg
        className="viewmap nodrag nopan nowheel"
        data-held={held ? "" : undefined}
        // the canvas's shape, fitted inside 140 × 100 (a tall or wide split would otherwise stretch it)
        style={{ "--vm-w": `${Math.min(140, (100 * w) / (h || 1))}px`, "--vm-h": `${Math.min(100, (140 * h) / (w || 1))}px` } as React.CSSProperties}
        viewBox={`${box.x} ${box.y} ${box.w} ${box.h}`}
        onPointerDown={(e) => {
          if (e.button !== 0) return;
          e.currentTarget.setPointerCapture(e.pointerId);
          setHeld(box);
          centerAt(e, box);
        }}
        onPointerMove={(e) => held && centerAt(e, held)}
        onPointerUp={() => setHeld(null)}
        onPointerCancel={() => setHeld(null)}
      >
        {nodes.map((n) => {
          // a block inside an area has its position relative to the area: the map needs where it really is
          const at = getInternalNode(n.id)?.internals.positionAbsolute ?? n.position;
          const kind = n.type === "prompt_input_primitive" ? "vm-prompt" : n.type === "zone" ? "vm-zone" : n.type === "struct_block" || n.type === "markdown_block" ? "vm-note" : "vm-term";
          return (
          <rect
            key={n.id}
            className={kind}
            x={at.x}
            y={at.y}
            width={n.width ?? n.measured?.width ?? 0}
            height={n.height ?? n.measured?.height ?? 0}
            rx={8 / zoom}
          />
          );
        })}
        <rect className="vm-frame" x={view.x} y={view.y} width={view.w} height={view.h} vectorEffect="non-scaling-stroke" />
      </svg>
    </Panel>
  );
}
