import { useEffect, useRef, useState } from "react";
import { AgentLogo, agentLabel } from "./agents";
import { glyphState, StatusGlyph } from "./status";
import { ask, askDiscardEdits } from "./ui/Confirm";
import { openMenu, pointOf, type MenuAt, type MenuItem } from "./ui/Menu";
import { BookOpen, ChevronLeft, ChevronRight, Columns2, Ellipsis, ExternalLink, File, FileText, Folder, GitCompare, Maximize2, Settings2, Workflow, X } from "./icons";
import { activeTab, barTabs, fileKey, groupView, isActiveTab, openTab, samePath, SYSTEM_ROOT, useStore, useTabLayout, useWaiting, type Group, type StageTab, type Worktree } from "./store";
import { dropOn, TAB_MIME, type TabDrag } from "./Workspace";

/** Whether a screen point is outside this window (where a dropped tab opens a window of its own). */
const outsideWindow = (x: number, y: number) =>
  x < window.screenX || y < window.screenY || x > window.screenX + window.outerWidth || y > window.screenY + window.outerHeight;

/**
 * An agent tab's one leading icon (never two): its state while it asks for a look (승인 대기, 작업 중, 새 응답) or
 * has no agent at all, else the agent's logo, a little faded.
 */
function AgentTabIcon({ w }: { w: Worktree }) {
  const waiting = useWaiting(w.path);
  const noAgent = !w.adapterId;
  const s = glyphState({ status: w.status, waiting, unread: w.unread, noAgent });
  if (s === "idle" || s === "off")
    return (
      <span className="tab-icon tab-logo" title={agentLabel(w.adapterId)}>
        <AgentLogo id={w.adapterId} size={12} />
      </span>
    );
  return <StatusGlyph status={w.status} waiting={waiting} unread={w.unread} noAgent={noAgent} />;
}

/**
 * Full screen ↔ canvas for the agent tab on screen: one icon showing where a click takes you. It changes this tab
 * only; what new tabs open with is the settings' (`agentLayout`, `maestroLayout`).
 */
function LayoutToggle({ group }: { group: Group }) {
  const layout = useTabLayout(group);
  if (!layout || !group.path) return null;
  const full = layout === "full";
  const label = full ? "캔버스로 보기" : "전체 화면으로 보기";
  return (
    <button
      className="icon"
      aria-pressed={full}
      title={label}
      aria-label={label}
      onClick={() => useStore.getState().setTabLayout(group.id, group.path!, full ? "canvas" : "full")}
    >
      {full ? <Workflow size={14} /> : <Maximize2 size={14} />}
    </button>
  );
}

/**
 * One group's tab bar (docs/features/ui/stage-tabs.md): the project's own tab first (always there), then each agent
 * gone into and each file opened. A tab is one leading icon, its name and a close button. A single click opens the
 * preview tab (reused by the next one); editing, sending or a double click keeps it. A tab drags like in VS Code:
 * onto another group's tab bar or centre to join it, onto a group's edge to split that way, outside the window to
 * open a window of its own. The project tab drags the whole group (all its tabs) the same ways. On the right: full
 * screen ↔ canvas (an agent tab), split right, and the group's menu. Back / forward are in the top bar.
 */
export function StageTabs({ group, focused, inWindow }: { group: Group; focused: boolean; inWindow: boolean }) {
  const root = group.root;
  const repo = useStore((s) => s.repos.find((r) => r.root === root));
  const worktrees = useStore((s) => s.worktrees);
  const dirtyFiles = useStore((s) => s.dirtyFiles);
  const systemView = useStore((s) => s.systemView);
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
  const { closeTab, pinTab, dropTab, splitEmpty, popOutTab, popOutGroup } = useStore.getState();
  const view = groupView(group);
  const onScreen = activeTab(view);
  // the system project's tab is named after the board it shows (문서 / 플레이북)
  const home = root === SYSTEM_ROOT ? { name: systemView === "playbooks" ? "플레이북" : "문서", root: "Maestro 문서와 플레이북 (~/.maestro)" } : repo;
  // the tabs as the bar shows them (an agent tab whose worktree is gone is left out)
  const shown = barTabs(group, { repos: repo ? [repo] : [], worktrees }).filter((t): t is StageTab => t !== null);

  const close = async (t: StageTab) => {
    if (t.kind === "file" && dirtyFiles.includes(fileKey(t.base, t.rel)) && !(await askDiscardEdits(t.rel))) return;
    closeTab(t, group.id);
  };
  // every tab but `keep` (null: every tab, the project stays): asks first only about unsaved edits
  const closeOthers = async (keep: StageTab | null) => {
    const unsaved = group.tabs.flatMap((t) => (t.kind === "file" && !(keep && isSame(t, keep)) && dirtyFiles.includes(fileKey(t.base, t.rel)) ? [t.rel] : []));
    if (
      unsaved.length &&
      !(await ask({ title: "다른 탭을 닫을까요?", body: `저장하지 않은 파일 ${unsaved.length}개의 편집은 사라져요`, list: unsaved, confirm: "닫기", danger: true }))
    )
      return;
    useStore.getState().closeOthers(group.id, keep);
  };
  // the project's own tab takes the group's tabs with it. Its tabs are remembered (they come back when the project
  // is opened again), so it only asks about unsaved edits.
  const closeHome = async () => {
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
  const others = (keep: StageTab | null) => shown.some((t) => !keep || !isSame(t, keep));

  // the group's menu (⋯ on the right, and a right click on the project tab)
  const groupMenu = (at: MenuAt, owner?: Element) => {
    const items: MenuItem[] = [
      { label: "오른쪽으로 분할", icon: <Columns2 size={14} />, onSelect: () => split(onScreen, "right") },
      { label: "아래로 분할", onSelect: () => split(onScreen, "bottom") },
      ...(!inWindow ? [{ label: "이 묶음을 새 창으로", icon: <ExternalLink size={14} />, disabled: !group.tabs.length, onSelect: () => void popOutGroup(group.id) }] : []),
      "sep",
      { label: "다른 탭 모두 닫기", disabled: !others(onScreen), onSelect: () => void closeOthers(onScreen) },
    ];
    openMenu(at, items, { owner });
  };
  const tabMenu = (at: MenuAt, t: StageTab, owner: Element) => {
    const items: MenuItem[] = [
      { label: "오른쪽으로 분할", icon: <Columns2 size={14} />, onSelect: () => split(t, "right") },
      { label: "아래로 분할", onSelect: () => split(t, "bottom") },
      { label: "새 창으로 열기", icon: <ExternalLink size={14} />, onSelect: () => void popOutTab(t, group.id).catch(useStore.getState().setError) },
      ...(t.preview ? [{ label: "탭 고정", onSelect: () => pinTab(t, group.id) }] : []),
      // an agent's tab (one with an agent attached): its name, description, permission and model
      ...(t.kind === "wt" && useStore.getState().worktrees.some((w) => w.adapterId && samePath(w.path, t.path))
        ? [{ label: "에이전트 설정…", icon: <Settings2 size={14} />, onSelect: () => useStore.getState().openAgentSettings({ path: t.path }) }]
        : []),
      "sep",
      { label: "닫기", icon: <X size={14} />, onSelect: () => void close(t) },
      { label: "다른 탭 닫기", disabled: !others(t), onSelect: () => void closeOthers(t) },
    ];
    openMenu(at, items, { owner });
  };

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
    <div className="stage-tabs" data-focused={focused || undefined} data-lone={!shown.length || undefined}>
      {(more.left || more.right) && (
        <button className="tab-scroll" title="왼쪽 탭" aria-label="왼쪽 탭으로" disabled={!more.left} onClick={() => scrollBy(-1)}>
          <ChevronLeft size={14} />
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
          onClick={() => openTab(null)}
          onAuxClick={(e) => e.button === 1 && closeHome()}
          onContextMenu={(e) => {
            e.preventDefault();
            groupMenu(pointOf(e), e.currentTarget);
          }}
          {...dragProps({ group: group.id }, (at) => void popOutGroup(group.id, at))}
        >
          <span className="tab-icon" aria-hidden>
            {root !== SYSTEM_ROOT ? <Folder size={14} /> : systemView === "playbooks" ? <BookOpen size={14} /> : <FileText size={14} />}
          </span>
          <span className="tab-label ellipsis">{home.name}</span>
          <button
            className="stage-tab-x"
            title="프로젝트 닫기 · 탭들은 기억해 뒀다가 다시 열면 되살려요"
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
          const what = t.kind === "wt" ? `${wt!.name}\n브랜치 ${wt!.branch}` : `${t.rel}${t.diff ? "\n변경 비교" : ""}`;
          return (
            <div
              key={t.kind === "wt" ? t.path : fileKey(t.base, t.rel)}
              className="stage-tab"
              role="tab"
              aria-selected={on}
              data-on={on || undefined}
              data-preview={t.preview || undefined}
              data-unread={(!!wt?.unread && !on) || undefined}
              data-index={n}
              data-insert={dropAt === n ? "before" : dropAt === group.tabs.length && n === group.tabs.length - 1 ? "after" : undefined}
              title={`${what}${t.preview ? "\n미리보기 탭 · 더블클릭하면 고정" : ""}`}
              {...(root ? dragProps({ tab: t, root, from: group.id }, (at) => void popOutTab(t, group.id, at)) : {})}
              onClick={() => openTab(t)}
              onDoubleClick={() => pinTab(t, group.id)}
              // middle click closes, as in browsers and editors
              onAuxClick={(e) => e.button === 1 && close(t)}
              onContextMenu={(e) => {
                e.preventDefault();
                tabMenu(pointOf(e), t, e.currentTarget);
              }}
            >
              {wt ? (
                <AgentTabIcon w={wt} />
              ) : (
                <span className="tab-icon" aria-hidden>
                  <File size={14} />
                </span>
              )}
              <span className="tab-label ellipsis">{name}</span>
              {t.kind === "file" && t.diff && (
                <span className="tab-diff" title="변경 비교" aria-label="변경 비교">
                  <GitCompare size={12} />
                </span>
              )}
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
          <ChevronRight size={14} />
        </button>
      )}
      <div className="tab-actions">
        {onScreen?.kind === "wt" && <LayoutToggle group={group} />}
        <button className="icon" title="오른쪽으로 분할" aria-label="오른쪽으로 분할" onClick={() => split(onScreen, "right")}>
          <Columns2 size={14} />
        </button>
        <button className="icon" title="탭 묶음 메뉴" aria-label="탭 묶음 메뉴" onClick={(e) => groupMenu(e.currentTarget)}>
          <Ellipsis size={14} />
        </button>
      </div>
    </div>
  );
}

const isSame = (a: StageTab, b: StageTab) =>
  a.kind === "wt" ? b.kind === "wt" && a.path.toLowerCase() === b.path.toLowerCase() : b.kind === "file" && a.base.toLowerCase() === b.base.toLowerCase() && a.rel === b.rel;
