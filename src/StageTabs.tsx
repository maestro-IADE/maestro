import { useEffect, useRef, useState } from "react";
import { AgentLogo } from "./agents";
import { AgentGlyph } from "./status";
import { ask, askDiscardEdits } from "./ui/Confirm";
import { X } from "./icons";
import { activeTab, fileKey, groupView, isActiveTab, navTarget, SYSTEM_ROOT, useStore, type Group, type StageTab } from "./store";
import { dropOn, TAB_MIME, type TabDrag } from "./Workspace";

// outlines after Lucide (ISC): "file", "folder" (the project), "chevron-left/right", "columns-2", "external-link"
const FILE_ICON = "M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8zM14 2v6h6";
const PROJECT_ICON = "M4 20h16a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.93a2 2 0 0 1-1.66-.9l-.82-1.2A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13c0 1.1.9 2 2 2Z";
const SPLIT_ICON = "M5 3h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2zM12 3v18";
// "layout-grid" (the system project: Maestro's documents and playbooks)
const SYSTEM_ICON = "M3 3h7v7H3zM14 3h7v7h-7zM14 14h7v7h-7zM3 14h7v7H3z";
const WINDOW_ICON = "M15 3h6v6M10 14 21 3M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6";

function Icon({ d, size = 14 }: { d: string; size?: number }) {
  return (
    <svg className="tree-icon" width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
      <path d={d} />
    </svg>
  );
}

/** Whether a screen point is outside this window (where a dropped tab opens a window of its own). */
const outsideWindow = (x: number, y: number) =>
  x < window.screenX || y < window.screenY || x > window.screenX + window.outerWidth || y > window.screenY + window.outerHeight;

/**
 * One group's tab bar: the project's own tab first (always there), then each worktree gone into and each file
 * opened. A single click opens the preview tab (italic, reused by the next one); editing, sending or a double
 * click keeps it. A tab drags like in VS Code: onto another group's tab bar or centre to join it, onto a group's
 * edge to split that way, outside the window to open a window of its own. The project tab drags the whole group
 * (all its tabs) the same ways. On the right: split, pop the group out,
 * and (on the last group) back / forward through the places visited.
 */
export function StageTabs({ group, focused, showNav, inWindow }: { group: Group; focused: boolean; showNav: boolean; inWindow: boolean }) {
  const root = group.root;
  const repo = useStore((s) => s.repos.find((r) => r.root === root));
  const worktrees = useStore((s) => s.worktrees);
  const dirtyFiles = useStore((s) => s.dirtyFiles);
  const canBack = useStore((s) => navTarget(s, -1) >= 0);
  const canForward = useStore((s) => navTarget(s, 1) >= 0);
  const [menu, setMenu] = useState<{ x: number; y: number; tab: StageTab } | null>(null);
  // where a tab dragged over the bar would go: before tab `n` of the group (its length = at the end); null = not over it
  const [dropAt, setDropAt] = useState<number | null>(null);
  const indexAt = (x: number) => {
    const tabs = [...(list.current?.querySelectorAll<HTMLElement>(".stage-tab[data-index]") ?? [])];
    const over = tabs.find((el) => {
      const r = el.getBoundingClientRect();
      return x < r.left + r.width / 2;
    });
    return over ? Number(over.dataset.index) : group.tabs.length;
  };
  // where the pointer last was while dragging a tab (dragend itself may report 0, 0 once outside the window)
  const lastPos = useRef({ x: 0, y: 0 });
  // more tabs than fit (after they've shrunk to their minimum): arrows scroll the row, and say which way there's more
  const list = useRef<HTMLDivElement>(null);
  const [more, setMore] = useState({ left: false, right: false });
  const measure = () => {
    const el = list.current;
    if (el) setMore({ left: el.scrollLeft > 1, right: el.scrollLeft + el.clientWidth < el.scrollWidth - 1 });
  };
  useEffect(() => {
    const el = list.current;
    if (!el) return;
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    return () => ro.disconnect();
  }, [!!list.current]);
  // the open tab is kept in view (opening one at the far end, going back / forward)
  useEffect(() => {
    list.current?.querySelector<HTMLElement>(".stage-tab[data-on]")?.scrollIntoView({ block: "nearest", inline: "nearest" });
    measure();
  }, [group.tabs.length, group.path, group.editor?.base, group.editor?.rel]);
  const scrollBy = (dir: -1 | 1) => list.current?.scrollBy({ left: dir * Math.max(160, (list.current.clientWidth * 2) / 3), behavior: "smooth" });
  const { select, openEditor, closeTab, pinTab, goNav, dropTab, splitEmpty, popOutTab, popOutGroup } = useStore.getState();
  const view = groupView(group);
  const onScreen = activeTab(view);
  // the system project's tab reads like a project's: its own name and icon
  const home = root === SYSTEM_ROOT ? { name: "시스템", root: "Maestro 문서와 플레이북 (~/.maestro)" } : repo;

  useEffect(() => {
    if (!menu) return;
    const off = () => setMenu(null);
    window.addEventListener("mousedown", off);
    window.addEventListener("blur", off);
    return () => {
      window.removeEventListener("mousedown", off);
      window.removeEventListener("blur", off);
    };
  }, [menu]);
  // the group's mousedown already focused it, so select / openEditor act on this group
  const go = (t: StageTab | null) => {
    if (t?.kind === "file") return openEditor({ base: t.base, rel: t.rel, diff: t.diff }, !t.preview);
    select(t?.path ?? null, !t?.preview);
  };
  const close = async (t: StageTab) => {
    if (t.kind === "file" && dirtyFiles.includes(fileKey(t.base, t.rel)) && !(await askDiscardEdits(t.rel))) return;
    closeTab(t, group.id);
  };
  // the project's own tab takes the group's tabs with it: say how many (and which have unsaved edits) first.
  // As the only group its tabs are remembered instead, so it only asks about unsaved edits.
  const closeHome = async () => {
    // its tabs are remembered (they come back when the project is opened again); only unsaved edits would be lost
    if (!home) return;
    const unsaved = group.tabs.flatMap((t) => (t.kind === "file" && dirtyFiles.includes(fileKey(t.base, t.rel)) ? [t.rel] : []));
    if (
      unsaved.length &&
      !(await ask({
        title: `'${home.name}' 탭을 닫을까요?`,
        body: `탭 ${group.tabs.length}개는 기억해 두었다가 다시 열면 되살려요.\n저장하지 않은 파일 ${unsaved.length}개의 편집은 사라져요.`,
        list: unsaved,
        confirm: "닫기",
        danger: true,
      }))
    )
      return;
    useStore.getState().closeProject(group.id);
  };
  const split = (t: StageTab | null, side: "right" | "bottom") => (t && root ? dropTab(t, root, group.id, group.id, side, true) : splitEmpty(group.id, side));
  // dragging a tab (or the project tab, carrying its group): where it is, and a window of its own if let go outside
  const dragProps = (data: TabDrag, outside: (at: { x: number; y: number }) => void) => ({
    draggable: true,
    onDragStart: (e: React.DragEvent) => {
      e.dataTransfer.setData(TAB_MIME, JSON.stringify(data));
      e.dataTransfer.effectAllowed = "move";
    },
    onDrag: (e: React.DragEvent) => void ((e.screenX || e.screenY) && (lastPos.current = { x: e.screenX, y: e.screenY })),
    // dropped where nothing took it, outside this window: a window of its own, right there
    onDragEnd: (e: React.DragEvent) => {
      if (e.dataTransfer.dropEffect !== "none") return;
      const at = e.screenX || e.screenY ? { x: e.screenX, y: e.screenY } : lastPos.current;
      if (outsideWindow(at.x, at.y)) outside(at);
    },
  });

  return (
    <div className="stage-tabs" data-focused={focused || undefined}>
      {(more.left || more.right) && (
        <button className="tab-scroll" title="왼쪽 탭" aria-label="왼쪽 탭으로" disabled={!more.left} onClick={() => scrollBy(-1)}>
          <Icon d="m15 18-6-6 6-6" size={14} />
        </button>
      )}
      <div
        ref={list}
        className="stage-tab-list"
        role="tablist"
        onScroll={measure}
        // the mouse wheel scrolls the row sideways, as in VS Code
        onWheel={(e) => {
          if (e.deltaY && list.current) list.current.scrollLeft += e.deltaY;
        }}
        // a tab dropped on the bar joins this group
        onDragOver={(e) => {
          if (!e.dataTransfer.types.includes(TAB_MIME)) return;
          e.preventDefault();
          e.stopPropagation();
          setDropAt(indexAt(e.clientX));
        }}
        onDragLeave={(e) => !e.currentTarget.contains(e.relatedTarget as Node) && setDropAt(null)}
        onDrop={(e) => {
          const at = indexAt(e.clientX);
          setDropAt(null);
          const raw = e.dataTransfer.getData(TAB_MIME);
          if (!raw) return;
          e.preventDefault();
          e.stopPropagation();
          // a tab lands at that spot in the row (from this bar: just moves along it)
          dropOn(raw, group.id, "center", at);
        }}
      >
        {home && root && <div
          className="stage-tab home"
          role="tab"
          aria-selected={!onScreen}
          data-on={!onScreen || undefined}
          title={`${home.root}\n끌면 이 묶음의 탭이 모두 함께 옮겨져요`}
          onClick={() => go(null)}
          onAuxClick={(e) => e.button === 1 && closeHome()}
          {...dragProps({ group: group.id }, (at) => void popOutGroup(group.id, at))}
        >
          <Icon d={root === SYSTEM_ROOT ? SYSTEM_ICON : PROJECT_ICON} />
          <span className="ellipsis">{home.name}</span>
          <button
            className="stage-tab-x"
            title="프로젝트 닫기 (탭들은 기억해 뒀다가 다시 열면 되살려요)"
            aria-label={`${home.name} 닫기`}
            onClick={(e) => {
              e.stopPropagation();
              closeHome();
            }}
          >
            <X size={12} />
          </button>
        </div>}
        {group.tabs.map((t, n) => {
          const on = isActiveTab(t, view);
          const wt = t.kind === "wt" ? worktrees.find((w) => w.path.toLowerCase() === t.path.toLowerCase()) : undefined;
          if (t.kind === "wt" && !wt) return null; // its worktree is gone (or not read yet)
          const name = t.kind === "wt" ? wt!.name : t.rel.split("/").pop()!;
          const dirty = t.kind === "file" && dirtyFiles.includes(fileKey(t.base, t.rel));
          return (
            <div
              key={t.kind === "wt" ? t.path : fileKey(t.base, t.rel)}
              className="stage-tab"
              role="tab"
              aria-selected={on}
              data-on={on || undefined}
              data-preview={t.preview || undefined}
              data-index={n}
              data-insert={dropAt === n ? "before" : dropAt === group.tabs.length && n === group.tabs.length - 1 ? "after" : undefined}
              title={`${t.kind === "wt" ? `${wt!.name}\n브랜치 ${wt!.branch}` : `${t.rel}${t.diff ? " (변경 비교)" : ""}`}${t.preview ? "\n미리보기: 더블클릭하면 탭을 고정해요" : ""}`}
              {...(root ? dragProps({ tab: t, root, from: group.id }, (at) => void popOutTab(t, group.id, at)) : {})}
              onClick={() => go(t)}
              onDoubleClick={() => pinTab(t, group.id)}
              // middle click closes, as in browsers and editors
              onAuxClick={(e) => e.button === 1 && close(t)}
              onContextMenu={(e) => {
                e.preventDefault();
                setMenu({ x: e.clientX, y: e.clientY, tab: t });
              }}
            >
              {wt ? (
                <>
                  <AgentGlyph w={wt} />
                  <AgentLogo id={wt.adapterId} size={14} />
                </>
              ) : (
                <Icon d={FILE_ICON} />
              )}
              <span className="ellipsis">{name}</span>
              {t.kind === "file" && t.diff && <span className="muted small">비교</span>}
              <button
                className="stage-tab-x"
                data-dirty={dirty || undefined}
                title={dirty ? "저장하지 않음 · 닫기" : "닫기"}
                aria-label={`${name} 닫기`}
                onClick={(e) => {
                  e.stopPropagation();
                  close(t);
                }}
              >
                <X size={12} />
              </button>
            </div>
          );
        })}
      </div>
      {(more.left || more.right) && (
        <button className="tab-scroll" title="오른쪽 탭" aria-label="오른쪽 탭으로" disabled={!more.right} onClick={() => scrollBy(1)}>
          <Icon d="m9 18 6-6-6-6" size={14} />
        </button>
      )}
      <div className="stage-nav">
        <button className="icon" title="오른쪽으로 분할 (지금 탭을 옆에 하나 더)" aria-label="오른쪽으로 분할" onClick={() => split(onScreen, "right")}>
          <Icon d={SPLIT_ICON} size={14} />
        </button>
        {!inWindow && (
          <button className="icon" title="이 탭 묶음을 새 창으로" aria-label="새 창으로" disabled={!group.tabs.length} onClick={() => popOutGroup(group.id)}>
            <Icon d={WINDOW_ICON} size={14} />
          </button>
        )}
        {showNav && (
          <>
            <span className="stage-nav-sep" />
            <button className="icon" title="뒤로" aria-label="뒤로" disabled={!canBack} onClick={() => goNav(-1)}>
              <Icon d="m15 18-6-6 6-6" size={16} />
            </button>
            <button className="icon" title="앞으로" aria-label="앞으로" disabled={!canForward} onClick={() => goNav(1)}>
              <Icon d="m9 18 6-6-6-6" size={16} />
            </button>
          </>
        )}
      </div>
      {menu && (
        <div className="ctx-menu" style={{ left: menu.x, top: menu.y }} onMouseDown={(e) => e.stopPropagation()}>
          <button onClick={() => (split(menu.tab, "right"), setMenu(null))}>오른쪽으로 분할</button>
          <button onClick={() => (split(menu.tab, "bottom"), setMenu(null))}>아래로 분할</button>
          <button onClick={() => (popOutTab(menu.tab, group.id), setMenu(null))}>새 창으로 열기</button>
          {menu.tab.preview && <button onClick={() => (pinTab(menu.tab, group.id), setMenu(null))}>탭 고정</button>}
          <hr />
          <button onClick={() => (close(menu.tab), setMenu(null))}>닫기</button>
        </div>
      )}
    </div>
  );
}
