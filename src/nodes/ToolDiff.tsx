import { useMemo, useState, type ReactNode } from "react";
import { relPath, toolPreview, type ToolPreview } from "../toolPreview";

const PREFIX = { " ": " ", "+": "+", "-": "-", "@": "" } as const;

/**
 * A file change's preview (toolPreview.ts): path, +/− counts or line count, then the first `rows` lines and
 * "N줄 더 보기". Expanded it is still bounded (MAX_LINES, the box's max-height scroll). hidePath: the caller's own
 * header already shows the path (ToolRow).
 */
export function ToolPreviewView({ p, rows, root, hidePath }: { p: ToolPreview; rows: number; root?: string; hidePath?: boolean }) {
  const [all, setAll] = useState(false);
  const shown = all ? p.lines : p.lines.slice(0, rows);
  const hidden = p.lines.length - rows;
  return (
    <div className="tool-diff">
      <div className="tool-diff-head">
        {!hidePath && p.path && <span className="ellipsis" title={p.path}>{relPath(p.path, root)}</span>}
        {p.kind === "diff" ? (
          <span className="tool-diff-stat"><span className="merge-add">+{p.adds}</span> <span className="merge-del">−{p.dels}</span></span>
        ) : (
          p.lines.length > 0 && <span className="muted small">{p.total}줄</span>
        )}
        {p.note && <span className="muted small">{p.note}</span>}
      </div>
      {shown.length > 0 && (
        <div className="tool-diff-lines nowheel">
          {shown.map((l, i) => (
            <div key={i} className="dl" data-op={l.op}>{PREFIX[l.op]}{l.text}</div>
          ))}
        </div>
      )}
      {hidden > 0 && (
        <button className="link muted small" onClick={() => setAll(!all)}>{all ? "접기" : `${hidden}줄 더 보기`}</button>
      )}
      {all && p.cut > 0 && <div className="muted small">… {p.cut}줄은 너무 길어 생략했어요</div>}
    </div>
  );
}

/** The preview of a tool call when it changes a file (Edit, MultiEdit, Write, NotebookEdit), else `fallback`. */
export function ToolInputPreview({ name, input, rows, root, fallback }: { name: string; input: any; rows: number; root?: string; fallback: ReactNode }) {
  const p = useMemo(() => toolPreview(name, input), [name, input]);
  return p ? <ToolPreviewView p={p} rows={rows} root={root} /> : <>{fallback}</>;
}
