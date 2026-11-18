import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { fitText, type Mini, type MiniBox } from "../canvasMini";
import { ipc } from "../ipc";
import { colorVar } from "../relations";
import type { RelColor } from "../relationModel";

/** The picture is shown no bigger than this (it is drawn smaller to fit, never bigger than the canvas at 100%). */
const MAX = { w: 420, h: 220 };
/** (a picture that is only view blocks, a chart or a page, gets more room: it is what is to be looked at) */
const MAX_VIEWS = { w: 480, h: 340 };
const FONT = 11;
/** The strip at the top of a view block that holds its name (the block's header on the canvas is 32). */
const VIEW_HEAD = 32;
const headOf = (b: MiniBox) => (b.h > 3 * VIEW_HEAD ? VIEW_HEAD : 0);

/**
 * What a view block shows, inside its box in the picture: the image, or the page in a frame that can't reach the app
 * (as on the canvas: scripts only) drawn at the block's own size and scaled down with the picture. It only shows: the
 * pointer goes through it.
 */
function MiniView({ box, mini, scale }: { box: MiniBox; mini: Mini; scale: number }) {
  const view = box.view!;
  const [file, setFile] = useState<{ kind: string; src: string } | null>(null);
  useEffect(() => {
    let live = true;
    setFile(null);
    void ipc.viewFile(view.path).then(
      (f) => live && setFile({ kind: f.kind, src: f.kind === "html" ? f.data : `data:${f.mime};base64,${f.data}` }),
      () => {},
    );
    return () => {
      live = false;
    };
  }, [view.path]);
  if (!file) return null;
  const head = headOf(box);
  const pct = (n: number, of: number) => `${(n / of) * 100}%`;
  return (
    <div className="mini-view" style={{ left: pct(box.x, mini.w), top: pct(box.y + head, mini.h), width: pct(box.w, mini.w), height: pct(box.h - head, mini.h) }}>
      {file.kind === "html" ? (
        <iframe title={box.title} tabIndex={-1} sandbox="allow-scripts" referrerPolicy="no-referrer" srcDoc={file.src} style={{ width: box.w, height: box.h - head, transform: `scale(${scale})` }} />
      ) : (
        <img src={file.src} alt={box.title} draggable={false} />
      )}
    </div>
  );
}

/**
 * A small picture of a piece of a canvas (canvasMini.ts): areas as frames with their name at the top, blocks as
 * boxes with their name in the middle, a question with a mark, lines between them, and a view block with what it
 * shows (its image or page). It only shows: nothing in it can be pressed or moved. Names are left out of boxes too
 * small to hold them.
 */
export function CanvasMini({ mini }: { mini: Mini }) {
  const views = mini.boxes.filter((b) => b.view);
  const max = views.length > 0 && views.length === mini.boxes.length ? MAX_VIEWS : MAX;
  const scale = mini.boxes.length ? Math.min(max.w / mini.w, max.h / mini.h, 1) : 1;
  // the size it really has on screen (a narrow card makes it smaller still): what a page inside it is scaled by
  const wrap = useRef<HTMLDivElement>(null);
  const [shown, setShown] = useState(scale);
  useLayoutEffect(() => {
    const el = wrap.current;
    if (!el || !mini.w) return;
    const read = () => el.clientWidth > 0 && setShown(el.clientWidth / mini.w);
    read();
    const ro = new ResizeObserver(read);
    ro.observe(el);
    return () => ro.disconnect();
  }, [mini.w, scale]);
  if (!mini.boxes.length) return null;
  // (sizes in the picture's own units that come out the same on screen whatever the scale)
  const px = (n: number) => n / scale;
  const label = (b: MiniBox) => {
    const mark = b.kind === "question" ? "? " : "";
    const room = b.w - px(12);
    if (b.w * scale < 36 || b.h * scale < 14 || !b.title) return null;
    const text = fitText(mark + b.title, room, px(FONT));
    if (!text) return null;
    // at the top: an area (what it holds is under it) and a view block (what it shows is under it)
    const top = b.kind === "area" || (!!b.view && headOf(b) > 0);
    const y = top ? b.y + (b.view ? VIEW_HEAD / 2 : px(FONT + 3)) : b.y + b.h / 2;
    return (
      <text x={top ? b.x + px(6) : b.x + b.w / 2} y={y} fontSize={px(FONT)} textAnchor={top ? "start" : "middle"} dominantBaseline={top && !b.view ? "auto" : "central"} data-kind={b.kind}>
        {text}
      </text>
    );
  };
  return (
    <div className="canvas-mini-wrap" ref={wrap} style={{ width: mini.w * scale, aspectRatio: `${mini.w} / ${mini.h}` }}>
      <svg className="canvas-mini" viewBox={`0 0 ${mini.w} ${mini.h}`} role="img" aria-label={`캔버스 그림: ${mini.boxes.map((b) => b.title).filter(Boolean).slice(0, 6).join(", ")}`}>
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
      {views.map((b) => (
        <MiniView key={b.id} box={b} mini={mini} scale={shown} />
      ))}
    </div>
  );
}
