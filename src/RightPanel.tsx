import { useEffect, useRef, useState } from "react";
import { ipc } from "./ipc";
import { SYSTEM_ROOT, useStore } from "./store";
import { MD_MIME } from "./LibraryPanel";
import { CaretIcon, Files, FileText, Folder, GitBranch, GitCompare, ListCollapse, RefreshIcon } from "./icons";
import { changeCode, parseRightTab, RIGHT_TAB_KEY, splitPath, type RightTab } from "./rightPanelModel";
import type { AgentStatus } from "./types";

export const DRAG_MIME = "application/x-maestro-path";

type FileChange = { code: string; path: string };

const POLL_MS = 4000;

function readTab(): RightTab {
  try {
    return parseRightTab(localStorage.getItem(RIGHT_TAB_KEY));
  } catch {
    return "files";
  }
}

/**
 * The selected agent's files and git changes (the project folder when no agent is open). The tab is remembered, and
 * the changes are read here (not only in their tab) so the head can show how many there are.
 */
export function RightPanel({ visible }: { visible: boolean }) {
  const { repos, activeRoot, worktrees, selectedPath } = useStore();
  const wt = worktrees.find((w) => w.path === selectedPath && w.root === activeRoot);
  const repo = repos.find((r) => r.root === activeRoot);
  const [tab, setTabState] = useState<RightTab>(readTab);
  const setTab = (t: RightTab) => {
    setTabState(t);
    try {
      localStorage.setItem(RIGHT_TAB_KEY, t);
    } catch {}
  };
  // "모두 접기": a new key remounts the tree with every folder shut
  const [treeKey, setTreeKey] = useState(0);
  const base = wt?.path ?? repo?.root ?? null;
  const changes = useChanges(repo ? base : null, wt?.status, visible, tab === "changes");

  if (activeRoot === SYSTEM_ROOT)
    return (
      <aside className="rightpanel">
        <div className="rp-head" />
        <div className="rp-empty">문서와 플레이북에는 파일 트리가 없어요</div>
      </aside>
    );
  if (!repo || !base)
    return (
      <aside className="rightpanel">
        <div className="rp-head" />
      </aside>
    );
  const count = changes.files?.length ?? 0;

  return (
    <aside className="rightpanel">
      <div className="rp-head" role="tablist" aria-label="오른쪽 패널">
        <button className="rp-tab" role="tab" aria-selected={tab === "files"} onClick={() => setTab("files")}>
          <Files size={14} />
          파일
        </button>
        <button className="rp-tab" role="tab" aria-selected={tab === "changes"} title={changes.files ? `바뀐 파일 ${count}개` : undefined} onClick={() => setTab("changes")}>
          <GitCompare size={14} />
          변경
          {count > 0 && <span className="rp-count">{count}</span>}
        </button>
        <span className="spacer" />
        {tab === "files" ? (
          <button className="icon rp-act" title="모두 접기" aria-label="모두 접기" onClick={() => setTreeKey((k) => k + 1)}>
            <ListCollapse size={14} />
          </button>
        ) : (
          <button className="icon rp-act" title="새로고침" aria-label="새로고침" onClick={() => changes.refresh(true)}>
            <RefreshIcon />
          </button>
        )}
      </div>
      <div className="rp-source" title={base}>
        {wt ? <GitBranch size={12} /> : <Folder size={12} />}
        <span className="ellipsis">{wt ? wt.branch : `${repo.name} · 원본`}</span>
      </div>
      {/* both stay mounted, so the open folders survive a look at the changes */}
      <div className="rp-body" hidden={tab !== "files"}>
        <Tree key={`${base}#${treeKey}`} base={base} rel="" depth={0} />
      </div>
      <div className="rp-body" hidden={tab !== "changes"}>
        <Changes path={base} files={changes.files} err={changes.err} />
      </div>
    </aside>
  );
}

/**
 * `git status` of one folder, read again when the agent's state changes (a turn ended, probably after an edit), when
 * the window gets the focus, and every 4 s while the changes tab is on screen and the agent is working. A poll is
 * skipped while the last read hasn't come back; any other read asked for meanwhile runs once when it does.
 * Nothing is read while the panel is hidden; showing it reads once.
 */
function useChanges(path: string | null, status: AgentStatus | undefined, visible: boolean, onTab: boolean) {
  const [files, setFiles] = useState<FileChange[] | null>(null);
  const [err, setErr] = useState("");
  const at = useRef(path);
  at.current = path;
  const inflight = useRef<string | null>(null);
  const again = useRef(false);

  const refresh = (queue: boolean) => {
    const p = at.current;
    if (!p) return;
    if (inflight.current === p) {
      if (queue) again.current = true;
      return;
    }
    inflight.current = p;
    again.current = false;
    ipc.gitStatus(p)
      .then((f) => {
        if (at.current !== p) return;
        setFiles(f);
        setErr("");
      })
      .catch((e) => at.current === p && setErr(String(e)))
      .finally(() => {
        if (inflight.current !== p) return;
        inflight.current = null;
        if (again.current && at.current === p) refresh(false);
      });
  };
  const latest = useRef(refresh);
  latest.current = refresh;

  // a different folder: forget the old list before the new one comes
  useEffect(() => {
    setFiles(null);
    setErr("");
  }, [path]);
  useEffect(() => {
    if (visible) latest.current(true);
  }, [path, status, visible]);
  useEffect(() => {
    if (!visible) return;
    const onFocus = () => latest.current(true);
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, [visible]);
  // one timer, only while all three hold: the panel shows, the changes tab is open, the agent is working
  const poll = visible && onTab && status === "RUNNING" && !!path;
  useEffect(() => {
    if (!poll) return;
    const t = setInterval(() => latest.current(false), POLL_MS);
    return () => clearInterval(t);
  }, [poll, path]);

  return { files, err, refresh: (queue: boolean) => latest.current(queue) };
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
        style={{ paddingLeft: 8 + depth * 12 }}
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
        <span className="tree-caret caret" data-open={open || undefined}>{isDir && <CaretIcon size={12} />}</span>
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

/** The change rows: the status letter, the file name, then its folder; 비교 and 열기 cover the row's end on hover. */
function Changes({ path, files, err }: { path: string; files: FileChange[] | null; err: string }) {
  const editor = useStore((s) => s.editor);
  const openEditor = useStore((s) => s.openEditor);

  return (
    <div className="changes">
      {err && <div className="err small wrap changes-err">{err}</div>}
      {files && !files.length && !err && <div className="rp-empty">바뀐 파일이 없어요</div>}
      {files?.map((f) => {
        const c = changeCode(f.code);
        const { name, dir } = splitPath(f.path);
        const diff = (pin = false) => openEditor({ base: path, rel: f.path, diff: true }, pin);
        return (
          <div
            key={f.path}
            className="change-row"
            data-selected={(editor?.base === path && editor.rel === f.path) || undefined}
            title={`${f.path}\n눌러서 HEAD와 비교`}
            onClick={() => diff()}
            onDoubleClick={() => diff(true)}
          >
            <span className="chg-code" data-kind={c.kind} title={c.label}>{c.letter}</span>
            <span className="chg-name ellipsis">{name}</span>
            {dir && <span className="chg-dir ellipsis">{dir}</span>}
            <span className="row-acts" onDoubleClick={(e) => e.stopPropagation()}>
              <button
                className="icon"
                title="비교"
                aria-label="비교"
                onClick={(e) => {
                  e.stopPropagation();
                  diff();
                }}
              >
                <GitCompare size={14} />
              </button>
              {c.kind !== "del" && (
                <button
                  className="icon"
                  title="열기"
                  aria-label="열기"
                  onClick={(e) => {
                    e.stopPropagation();
                    openEditor({ base: path, rel: f.path, diff: false });
                  }}
                >
                  <FileText size={14} />
                </button>
              )}
            </span>
          </div>
        );
      })}
    </div>
  );
}
