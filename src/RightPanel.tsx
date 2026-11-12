import { useEffect, useState } from "react";
import { ipc } from "./ipc";
import { useStore } from "./store";
import { MD_MIME } from "./LibraryPanel";
import { CaretIcon, RefreshIcon } from "./icons";

export const DRAG_MIME = "application/x-maestro-path";

export function RightPanel() {
  const { repos, activeRoot, worktrees, selectedPath } = useStore();
  const wt = worktrees.find((w) => w.path === selectedPath && w.root === activeRoot);
  const repo = repos.find((r) => r.root === activeRoot);
  const [tab, setTab] = useState<"files" | "changes">("files");
  if (!repo) return <aside className="rightpanel" />;
  const base = wt?.path ?? repo.root;

  return (
    <aside className="rightpanel">
      <div className="rp-head">
        <div className="tabs">
          <button data-on={tab === "files" || undefined} onClick={() => setTab("files")}>파일</button>
          <button data-on={tab === "changes" || undefined} onClick={() => setTab("changes")}>변경사항</button>
        </div>
        <div className="muted small ellipsis" title={base}>{wt ? wt.branch : `${repo.name} (원본)`}</div>
      </div>
      <div className="rp-body">
        {tab === "files" ? <Tree key={base} base={base} rel="" depth={0} /> : <Changes key={base} path={base} status={wt?.status} />}
      </div>
    </aside>
  );
}

function Tree({ base, rel, depth }: { base: string; rel: string; depth: number }) {
  const [items, setItems] = useState<Array<{ name: string; isDir: boolean }> | null>(null);
  const [err, setErr] = useState("");
  useEffect(() => {
    ipc.listDir(rel ? `${base}/${rel}` : base).then(setItems).catch((e) => setErr(String(e)));
  }, [base, rel]);
  if (err) return <div className="err small" style={{ paddingLeft: 12 + depth * 12 }}>{err}</div>;
  if (!items) return null;
  return (
    <>
      {items.map((it) => (
        <TreeItem key={it.name} base={base} rel={rel ? `${rel}/${it.name}` : it.name} name={it.name} isDir={it.isDir} depth={depth} />
      ))}
    </>
  );
}

function TreeItem({ base, rel, name, isDir, depth }: { base: string; rel: string; name: string; isDir: boolean; depth: number }) {
  const [open, setOpen] = useState(false);
  const editor = useStore((s) => s.editor);
  const openEditor = useStore((s) => s.openEditor);
  return (
    <>
      <div
        className="tree-row"
        style={{ paddingLeft: 12 + depth * 12 }}
        draggable
        data-selected={(!isDir && editor?.base === base && editor.rel === rel) || undefined}
        title={rel}
        // while a file is dragged, the places that take it light up (styles.css: data-dragging)
        onDragEnd={() => delete document.documentElement.dataset.dragging}
        onDragStart={(e) => {
          document.documentElement.dataset.dragging = /\.md$/i.test(rel) ? "file md" : "file";
          e.dataTransfer.setData(DRAG_MIME, rel);
          // a Markdown file can also become a canvas block (title is read on drop)
          if (/\.md$/i.test(rel)) e.dataTransfer.setData(MD_MIME, JSON.stringify({ path: `${base.replace(/\\/g, "/")}/${rel}`, title: "" }));
          e.dataTransfer.effectAllowed = "copy";
          // the browser snapshots the drag image right away, so the chip only has to exist for this frame
          const chip = document.createElement("div");
          chip.className = "drag-chip";
          chip.innerHTML = `<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="${TREE_ICONS[isDir ? "folder" : "file"]}"/></svg>`;
          chip.append(name);
          document.body.append(chip);
          e.dataTransfer.setDragImage(chip, 14, 14);
          requestAnimationFrame(() => chip.remove());
        }}
        // a click previews the file in the reusable tab, a double click keeps its tab (VS Code)
        onClick={() => (isDir ? setOpen(!open) : openEditor({ base, rel, diff: false }))}
        onDoubleClick={() => !isDir && openEditor({ base, rel, diff: false }, true)}
      >
        <span className="tree-caret caret" data-open={open || undefined}>{isDir && <CaretIcon />}</span>
        <TreeIcon kind={!isDir ? "file" : open ? "open" : "folder"} />
        <span className={isDir ? "tree-dir" : ""}>{name}</span>
      </div>
      {isDir && open && <Tree base={base} rel={rel} depth={depth + 1} />}
    </>
  );
}

// outlines after Lucide "file", "folder", "folder-open" (ISC)
const TREE_ICONS = {
  file: "M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8zM14 2v6h6",
  folder: "M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z",
  open: "m6 14 1.5-2.9A2 2 0 0 1 9.24 10H20a2 2 0 0 1 1.94 2.5l-1.54 6a2 2 0 0 1-1.95 1.5H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h3.9a2 2 0 0 1 1.69.9l.81 1.2a2 2 0 0 0 1.67.9H18a2 2 0 0 1 2 2v2",
};

function TreeIcon({ kind }: { kind: keyof typeof TREE_ICONS }) {
  return (
    <svg className="tree-icon" data-kind={kind} width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
      <path d={TREE_ICONS[kind]} />
    </svg>
  );
}

function Changes({ path, status }: { path: string; status?: string }) {
  const [files, setFiles] = useState<Array<{ code: string; path: string }>>([]);
  const [err, setErr] = useState("");
  const editor = useStore((s) => s.editor);
  const openEditor = useStore((s) => s.openEditor);

  const refresh = () =>
    ipc.gitStatus(path).then((f) => {
      setFiles(f);
      setErr("");
    }).catch((e) => setErr(String(e)));

  // re-read whenever the agent goes quiet, i.e. probably finished an edit
  useEffect(() => {
    refresh();
  }, [path, status]);

  return (
    <div className="changes">
      <div className="changes-head">
        <span className="muted small">{files.length}개 파일 변경</span>
        <span className="spacer" />
        <button className="icon" title="새로고침" aria-label="새로고침" onClick={refresh}><RefreshIcon /></button>
      </div>
      {err && <div className="err small wrap">{err}</div>}
      {files.map((f) => (
        <div
          key={f.path}
          className="change-row"
          data-selected={(editor?.base === path && editor.rel === f.path) || undefined}
          title="눌러서 HEAD와 비교"
          onClick={() => openEditor({ base: path, rel: f.path, diff: true })}
          onDoubleClick={() => openEditor({ base: path, rel: f.path, diff: true }, true)}
        >
          <span className="code" data-code={f.code.trim()[0]}>{f.code.trim() || "?"}</span>
          <span className="ellipsis" title={f.path}>{f.path}</span>
        </div>
      ))}
    </div>
  );
}
