import { fitText, type Mini, type MiniBox } from "../canvasMini";
import { colorVar } from "../relations";
import type { RelColor } from "../relationModel";

/** The picture is shown no bigger than this (it is drawn smaller to fit, never bigger than the canvas at 100%). */
const MAX = { w: 420, h: 220 };
const FONT = 11;

/**
 * A small picture of a piece of a canvas (canvasMini.ts): areas as frames with their name at the top, blocks as
 * boxes with their name in the middle, a question with a mark, lines between them. It only shows: nothing in it can
 * be pressed or moved. Names are left out of boxes too small to hold them.
 */
export function CanvasMini({ mini }: { mini: Mini }) {
  if (!mini.boxes.length) return null;
  const scale = Math.min(MAX.w / mini.w, MAX.h / mini.h, 1);
  // (sizes in the picture's own units that come out the same on screen whatever the scale)
  const px = (n: number) => n / scale;
  const label = (b: MiniBox) => {
    const mark = b.kind === "question" ? "? " : "";
    const room = b.w - px(12);
    if (b.w * scale < 36 || b.h * scale < 14 || !b.title) return null;
    const text = fitText(mark + b.title, room, px(FONT));
    if (!text) return null;
    const top = b.kind === "area";
    return (
      <text x={top ? b.x + px(6) : b.x + b.w / 2} y={top ? b.y + px(FONT + 3) : b.y + b.h / 2} fontSize={px(FONT)} textAnchor={top ? "start" : "middle"} dominantBaseline={top ? "auto" : "central"} data-kind={b.kind}>
        {text}
      </text>
    );
  };
  return (
    <svg className="canvas-mini" viewBox={`0 0 ${mini.w} ${mini.h}`} width={mini.w * scale} height={mini.h * scale} role="img" aria-label={`캔버스 그림: ${mini.boxes.map((b) => b.title).filter(Boolean).slice(0, 6).join(", ")}`}>
      <defs>
        <marker id="canvas-mini-head" viewBox="0 0 8 8" refX="7" refY="4" markerWidth="7" markerHeight="7" orient="auto-start-reverse">
          <path d="M0 0 L8 4 L0 8 z" />
        </marker>
      </defs>
      {mini.boxes.map((b) => (
        <g key={b.id}>
          <rect x={b.x} y={b.y} width={b.w} height={b.h} rx={px(b.kind === "area" ? 6 : 4)} data-kind={b.kind} style={b.color ? { stroke: colorVar(b.color as RelColor) } : undefined} vectorEffect="non-scaling-stroke" />
          {label(b)}
        </g>
      ))}
      {mini.lines.map((l, i) => (
        <line key={i} x1={l.x1} y1={l.y1} x2={l.x2} y2={l.y2} markerEnd="url(#canvas-mini-head)" vectorEffect="non-scaling-stroke" />
      ))}
    </svg>
  );
}
