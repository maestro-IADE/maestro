import { useEffect, useRef, useState } from "react";
import { NodeResizer, type NodeProps } from "@xyflow/react";
import { ipc } from "../ipc";
import { ImageIcon, RefreshIcon } from "../icons";
import { GRID } from "./AgentNode";
import type { ViewNode } from "../types";

export const VIEW_SIZE = { width: 16 * GRID, height: 12 * GRID };
/** dataTransfer type of a file the canvas can show as a view block (RightPanel's file tree sets it): `{ path }`. */
export const VIEW_MIME = "application/x-maestro-view";
/** What a view block shows, by the file's name: an image, a page, or nothing it can show. */
export const viewKind = (path: string): "image" | "html" | null => (/\.(png|jpe?g|gif|webp|svg)$/i.test(path) ? "image" : /\.html?$/i.test(path) ? "html" : null);

/** How often a view block looks at its file's modified time (while the window shows). */
const POLL_MS = 2500;

/**
 * A view block (docs/features/canvas/view-block.md): a file shown on the canvas as it is, for what the drawing blocks
 * can't draw (a chart, a screenshot, a page an agent made). An agent puts one with `maestro show --file …`; the user
 * drags an image or HTML file from the file tree. An image is drawn to fit; a page runs in a frame that can't reach
 * the app (scripts only: no same origin, no top navigation, no pop-ups, no forms).
 *
 * The block holds the file's path, not its contents: it reads the file when it appears, again when the file's
 * modified time changes (looked at every `POLL_MS`, and when the window gets the focus back), when `maestro show`
 * names the file again (`data.at`), and on its reload button. So an agent that writes the chart again doesn't have
 * to show it again.
 */
export function ViewBlock({ data, selected }: NodeProps<ViewNode>) {
  const [file, setFile] = useState<{ kind: string; src: string; mtime: number } | null>(null);
  const [err, setErr] = useState("");
  const [busy, setBusy] = useState(false);
  const seen = useRef(0);
  const name = data.path.split("/").pop() ?? data.path;

  const load = () => {
    setBusy(true);
    ipc.viewFile(data.path).then(
      (f) => {
        seen.current = f.mtime;
        setFile({ kind: f.kind, src: f.kind === "html" ? f.data : `data:${f.mime};base64,${f.data}`, mtime: f.mtime });
        setErr("");
        setBusy(false);
      },
      (e) => {
        setErr(String(e));
        setBusy(false);
      },
    );
  };
  // on appearing, and when `maestro show` names the file again
  useEffect(load, [data.path, data.at]);
  // the file written again: read again (a file that went away keeps what was last shown, and says so)
  useEffect(() => {
    const look = () => {
      if (document.hidden) return;
      void ipc.viewMtime(data.path).then((m) => {
        if (m && m !== seen.current) load();
        else if (!m && seen.current) setErr("파일이 없어졌어요 (마지막으로 읽은 모습이에요)");
      });
    };
    const t = window.setInterval(look, POLL_MS);
    window.addEventListener("focus", look);
    return () => {
      window.clearInterval(t);
      window.removeEventListener("focus", look);
    };
  }, [data.path]);

  return (
    <div className="node node-view" title={data.path}>
      <NodeResizer isVisible={selected} minWidth={8 * GRID} minHeight={6 * GRID} lineClassName="struct-resize-line" handleClassName="struct-resize-handle" />
      <header>
        <ImageIcon size={14} className="muted" />
        <b className="ellipsis">{data.title?.trim() || name}</b>
        {data.title?.trim() && <span className="muted small ellipsis">{name}</span>}
        <span className="spacer" />
        <button className="icon nodrag" title="다시 읽기 · 파일이 바뀌면 스스로 다시 읽어요" aria-label="다시 읽기" aria-busy={busy || undefined} onClick={load}>
          <RefreshIcon />
        </button>
      </header>
      {/* `nodrag nowheel`: inside the picture or page the pointer is its own (the block moves by its head) */}
      <div className="view-body nodrag nowheel">
        {file?.kind === "image" && <img src={file.src} alt={data.title?.trim() || name} draggable={false} />}
        {file?.kind === "html" && <iframe title={data.title?.trim() || name} sandbox="allow-scripts" referrerPolicy="no-referrer" srcDoc={file.src} />}
        {!file && !err && <p className="muted">읽는 중…</p>}
        {err && <p className={file ? "view-note" : "err"}>{err}</p>}
      </div>
    </div>
  );
}
