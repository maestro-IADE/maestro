import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { ipc, type DocFile } from "./ipc";
import { useStore, type SystemView } from "./store";

/** Drag payload for a Markdown file: JSON {path, title}. The canvas turns a drop into a Markdown block. */
export const MD_MIME = "application/x-maestro-md";

type Folder = { folder: string; persona: boolean; files: DocFile[] };

// outlines after Lucide (ISC): chevron-right, plus, download (import), x, rotate-cw (refresh)
const G = {
  chevron: "m9 18 6-6-6-6",
  plus: "M5 12h14M12 5v14",
  import: "M12 3v12M7 10l5 5 5-5M5 21h14",
  x: "M18 6 6 18M6 6l12 12",
  refresh: "M21 12a9 9 0 1 1-9-9c2.52 0 4.93 1 6.74 2.74L21 8M21 3v5h-5",
};
/** One line icon at a fixed size, so every button in the lists matches (text glyphs differ per font). */
const Glyph = ({ d, size = 16 }: { d: string; size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
    <path d={d} />
  </svg>
);

const nameOf = (f: Folder) => (f.persona ? "페르소나" : f.folder.endsWith("/playbooks") ? "플레이북" : f.folder.split("/").pop() ?? f.folder);

/** Loading and changing one list (documents or playbooks), shared by the canvas panel and the system tab. */
function useDocs(books: boolean) {
  const setError = useStore((s) => s.setError);
  const [folders, setFolders] = useState<Folder[] | null>(null);
  // what is being named: a new document in a folder, or a new category ("")
  const [naming, setNaming] = useState<{ folder: string; text: string } | null>(null);
  const load = () => (books ? ipc.playbookList() : ipc.libraryList()).then(setFolders).catch(setError);
  useEffect(() => void load(), []);

  const commit = () => {
    if (!naming) return;
    const text = naming.text.trim();
    if (!text) return setNaming(null);
    const done = naming.folder ? ipc.docsCreate(naming.folder, text) : ipc.docsCategory(text);
    done.then(() => {
      setNaming(null);
      load();
    }, setError);
  };
  // adds copies of outside files to Maestro's documents; using one afterwards is a reference to the copy
  const importTo = async (folder: string) => {
    const picked = await open({ multiple: true, title: "문서로 가져올 마크다운", filters: [{ name: "Markdown", extensions: ["md"] }] });
    const files = Array.isArray(picked) ? picked : typeof picked === "string" ? [picked] : [];
    if (files.length) ipc.docsImport(folder, files).then(load, setError);
  };
  const remove = (path: string, title: string) => {
    if (confirm(`'${title}' 문서를 지울까요? 이 문서를 참조하던 에이전트에서는 열 수 없게 돼요.`)) ipc.docsDelete(path).then(load, setError);
  };
  const nameBox = (folder: string, placeholder: string) =>
    naming?.folder === folder && (
      <div className="docs-new">
        <input
          autoFocus
          value={naming.text}
          placeholder={placeholder}
          onChange={(e) => setNaming({ folder, text: e.target.value })}
          onKeyDown={(e) => {
            if (e.nativeEvent.isComposing) return;
            if (e.key === "Enter") commit();
            if (e.key === "Escape") setNaming(null);
          }}
          onBlur={() => !naming.text.trim() && setNaming(null)}
        />
      </div>
    );
  return { folders, load, naming, setNaming, importTo, remove, nameBox };
}

const openDoc = (folder: string, path: string) => useStore.getState().openEditor({ base: folder, rel: path.slice(folder.length + 1), diff: false });
// a file dragged onto a canvas becomes a block that refers to it
const dragDoc = (file: DocFile) => ({
  draggable: true,
  onDragEnd: () => delete document.documentElement.dataset.dragging,
  onDragStart: (e: React.DragEvent) => {
    document.documentElement.dataset.dragging = "md"; // lights up the canvas (styles.css)
    e.dataTransfer.setData(MD_MIME, JSON.stringify({ path: file.path, title: file.title }));
    e.dataTransfer.effectAllowed = "copy";
  },
});

/**
 * Maestro's documents (문서): the personas and the shared categories in `~/.maestro/library`, the same for every
 * project. Dragging a file onto a canvas makes a block that refers to it (the file is not copied); a click
 * opens it in the editor. A project's own files come from the file tree instead.
 */
export function DocsList({ kind = "docs", folded = false }: { kind?: "docs" | "playbooks"; folded?: boolean }) {
  const books = kind === "playbooks";
  const { folders, load, naming, setNaming, importTo, remove, nameBox } = useDocs(books);
  // categories flipped from the starting state (all open, or all shut when `folded`)
  const [flipped, setFlipped] = useState<string[]>([]);
  const flip = (folder: string) => setFlipped(flipped.includes(folder) ? flipped.filter((x) => x !== folder) : [...flipped, folder]);

  return (
    <div className="docs-list">
      <div className="docs-tools">
        <span className="muted small">
          {books ? "마에스트로가 워커를 부리는 절차 · 마에스트로가 시작할 때 목록을 받아요" : "모든 프로젝트가 같이 쓰는 문서 · 캔버스로 끌면 참조돼요"}
        </span>
        <span className="spacer" />
        <button className="icon" title="다시 읽기" aria-label="다시 읽기" onClick={load}><Glyph d={G.refresh} /></button>
        {!books && <button className="icon" title="새 분류" aria-label="새 분류" onClick={() => setNaming({ folder: "", text: "" })}><Glyph d={G.plus} /></button>}
      </div>
      {nameBox("", "새 분류 이름 · Enter")}
      {folders?.map((f) => {
        const open = folded === flipped.includes(f.folder);
        return (
          <section key={f.folder} className="docs-cat">
            <div className="docs-cat-row" title={f.folder} onClick={() => flip(f.folder)}>
              <span className="docs-caret" data-open={open || undefined} aria-hidden><Glyph d={G.chevron} size={12} /></span>
              <span className="ellipsis">{nameOf(f)}</span>
              <span className="muted small">{f.files.length}</span>
              <span className="spacer" />
              <button
                className="icon hover-only"
                title={f.persona ? "새 페르소나" : books ? "새 플레이북" : "새 문서"}
                aria-label={f.persona ? "새 페르소나" : books ? "새 플레이북" : "새 문서"}
                onClick={(e) => {
                  e.stopPropagation();
                  if (!open) flip(f.folder);
                  setNaming({ folder: f.folder, text: "" });
                }}
              >
                <Glyph d={G.plus} />
              </button>
              <button
                className="icon hover-only"
                title="마크다운 파일 가져오기 (복사해서 넣어요)"
                aria-label="가져오기"
                onClick={(e) => {
                  e.stopPropagation();
                  void importTo(f.folder);
                }}
              >
                <Glyph d={G.import} />
              </button>
            </div>
            {open && (
              <>
                {nameBox(f.folder, f.persona ? "새 페르소나 이름 · Enter" : books ? "새 플레이북 이름 · Enter" : "새 문서 이름 · Enter")}
                {f.persona && <div className="docs-hint muted small">블록을 에이전트에 이으면 그 역할을 맡아요</div>}
                {!f.files.length && naming?.folder !== f.folder && <div className="docs-hint muted small">비어 있어요</div>}
                {f.files.map((file) => (
                  <div
                    key={file.path}
                    className="docs-file"
                    title={`${file.path}\n클릭하면 편집기로 열어요 · 캔버스로 끌면 참조 블록`}
                    onClick={() => openDoc(f.folder, file.path)}
                    {...dragDoc(file)}
                  >
                    <span className="ellipsis">{file.title}</span>
                    <button
                      className="icon hover-only"
                      title="문서 지우기"
                      aria-label={`${file.title} 지우기`}
                      onClick={(e) => {
                        e.stopPropagation();
                        remove(file.path, file.title);
                      }}
                    >
                      <Glyph d={G.x} />
                    </button>
                  </div>
                ))}
              </>
            )}
          </section>
        );
      })}
    </div>
  );
}

/** The same documents in a panel over an agent canvas (its toolbar's 문서 button). */
export function LibraryPanel({ onClose }: { onClose: () => void }) {
  return (
    <div className="library nowheel nodrag">
      <div className="library-head">
        <b>문서</b>
        <span className="spacer" />
        <button className="icon" title="닫기" aria-label="닫기" onClick={onClose}><Glyph d={G.x} /></button>
      </div>
      <div className="library-body">
        <DocsList folded />
      </div>
    </div>
  );
}

const SHUT_KEY = "maestro.systemShut";
const readShut = (): string[] => {
  try {
    return JSON.parse(localStorage.getItem(SHUT_KEY) ?? "[]");
  } catch {
    return [];
  }
};

/** One list on the system board: each folder a foldable group of cards (title and first line). */
function Board({ books, tabs }: { books: boolean; tabs: React.ReactNode }) {
  const { folders, load, naming, setNaming, importTo, remove, nameBox } = useDocs(books);
  // folded groups (by folder), kept across restarts
  const [shut, setShutState] = useState(readShut);
  const toggle = (folder: string) => {
    const next = shut.includes(folder) ? shut.filter((x) => x !== folder) : [...shut, folder];
    setShutState(next);
    try {
      localStorage.setItem(SHUT_KEY, JSON.stringify(next));
    } catch {}
  };
  const noun = (f: Folder) => (f.persona ? "페르소나" : books ? "플레이북" : "문서");
  return (
    <div className="system">
      <header className="system-head">
        {tabs}
        <span className="spacer" />
        {!books && <button className="sys-btn" onClick={() => setNaming({ folder: "", text: "" })}><Glyph d={G.plus} size={14} />새 분류</button>}
        <button className="icon" title="다시 읽기" aria-label="다시 읽기" onClick={load}><Glyph d={G.refresh} /></button>
      </header>
      <div className="system-body">
        {nameBox("", "새 분류 이름 · Enter")}
        {folders?.map((f) => {
          const open = !shut.includes(f.folder);
          return (
            <section key={f.folder} className="sys-group" data-open={open || undefined}>
              <div className="sys-group-head" title={f.folder} onClick={() => toggle(f.folder)}>
                <span className="docs-caret" data-open={open || undefined} aria-hidden><Glyph d={G.chevron} size={12} /></span>
                <b>{nameOf(f)}</b>
                <span className="sys-count">{f.files.length}</span>
                <span className="spacer" />
                <button
                  className="icon"
                  title={`새 ${noun(f)}`}
                  aria-label={`새 ${noun(f)}`}
                  onClick={(e) => {
                    e.stopPropagation();
                    if (!open) toggle(f.folder);
                    setNaming({ folder: f.folder, text: "" });
                  }}
                >
                  <Glyph d={G.plus} />
                </button>
                <button
                  className="icon"
                  title="마크다운 가져오기 (복사)"
                  aria-label="가져오기"
                  onClick={(e) => {
                    e.stopPropagation();
                    void importTo(f.folder);
                  }}
                >
                  <Glyph d={G.import} />
                </button>
              </div>
              {open && (
                <>
                  {nameBox(f.folder, `새 ${noun(f)} 이름 · Enter`)}
                  <div className="sys-cards">
                    {f.files.map((file) => (
                      <div
                        key={file.path}
                        className="sys-card"
                        title={`${file.path}\n누르면 탭으로 열려요 · 캔버스로 끌면 참조`}
                        onClick={() => openDoc(f.folder, file.path)}
                        {...dragDoc(file)}
                      >
                        <b className="ellipsis">{file.title}</b>
                        <p>{file.summary}</p>
                        <button
                          className="icon hover-only"
                          title="지우기"
                          aria-label={`${file.title} 지우기`}
                          onClick={(e) => {
                            e.stopPropagation();
                            remove(file.path, file.title);
                          }}
                        >
                          <Glyph d={G.x} />
                        </button>
                      </div>
                    ))}
                    {!f.files.length && naming?.folder !== f.folder && <div className="sys-empty muted small">비어 있어요</div>}
                  </div>
                </>
              )}
            </section>
          );
        })}
      </div>
    </div>
  );
}

/** The system project's board: Maestro's own documents or playbooks (`view`, picked in the sidebar or here) as cards. */
export function SystemDashboard({ view, onView }: { view: SystemView; onView: (v: SystemView) => void }) {
  const tabs = (
    <div className="seg" role="tablist">
      <button role="tab" aria-selected={view === "docs"} data-on={view === "docs" || undefined} onClick={() => onView("docs")}>문서</button>
      <button role="tab" aria-selected={view === "playbooks"} data-on={view === "playbooks" || undefined} onClick={() => onView("playbooks")}>플레이북</button>
    </div>
  );
  return <Board key={view} books={view === "playbooks"} tabs={tabs} />;
}

const CHECK = "M20 6 9 17l-5-5";

/**
 * The 플레이북 panel under a Maestro canvas's island: 자동 (the Maestro picks a playbook itself) first and set
 * apart, then each playbook. The one picked goes along with every message from the chat box below.
 */
export function PlaybookPanel({ value, onPick, onClose }: { value: DocFile | null; onPick: (b: DocFile | null) => void; onClose: () => void }) {
  const setError = useStore((s) => s.setError);
  const [books, setBooks] = useState<Array<{ folder: string; file: DocFile }> | null>(null);
  useEffect(() => void ipc.playbookList().then((fs) => setBooks(fs.flatMap((f) => f.files.map((file) => ({ folder: f.folder, file })))), setError), []);
  const item = (on: boolean, title: string, sub: string, pick: () => void, extra?: React.ReactNode, auto = false) => (
    <div className={`book-item${auto ? " auto" : ""}`} role="option" aria-selected={on} data-on={on || undefined} title={sub} onClick={pick}>
      <span className="book-check">{on && <Glyph d={CHECK} size={16} />}</span>
      <span className="book-text">
        <b className="ellipsis">{title}</b>
        <span>{sub}</span>
      </span>
      {extra ?? <span />}
    </div>
  );
  return (
    <div className="library nowheel nodrag">
      <div className="library-head">
        <b>플레이북</b>
        <span className="spacer" />
        <button className="icon" title="닫기" aria-label="닫기" onClick={onClose}><Glyph d={G.x} /></button>
      </div>
      <div className="library-body book-list" role="listbox" aria-label="따를 플레이북">
        {item(!value, "자동", "마에스트로가 요청에 맞는 플레이북을 골라요", () => onPick(null), undefined, true)}
        <div className="book-sep" />
        {books?.map(({ folder, file }) =>
          item(value?.path === file.path, file.title, file.summary, () => onPick(file), (
            <button
              className="icon book-open"
              title="편집기로 열기"
              aria-label={`${file.title} 열기`}
              onClick={(e) => {
                e.stopPropagation();
                openDoc(folder, file.path);
              }}
            >
              <Glyph d="M15 3h6v6M10 14 21 3M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6" size={14} />
            </button>
          )),
        )}
        {books && !books.length && <div className="docs-hint muted small">플레이북이 없어요</div>}
      </div>
    </div>
  );
}
