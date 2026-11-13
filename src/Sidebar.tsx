import { useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { useShallow } from "zustand/react/shallow";
import { ipc } from "./ipc";
import { BookOpen, ChevronRight, ChevronsDownUp, ChevronsUpDown, Columns2, Copy, Ellipsis, ExternalLink, FileText, FolderPlus, GitBranch, MessageSquare, Plus, Settings, Settings2, SquareTerminal, Trash2, X } from "./icons";
import { AgentLogo } from "./agents";
import { StatusGlyph } from "./status";
import { ask } from "./ui/Confirm";
import { keysOf, tip } from "./shortcuts";
import { openMenu, pointOf, type MenuAt, type MenuItem } from "./ui/Menu";
import { selectAttention, SYSTEM_ROOT, usePr, useStore, type NewAgentTarget, type SystemView, type Worktree } from "./store";
import { useSettings } from "./settings";
import { useDiscard } from "./discard";
import { copyText, openInSplit, openInWindow, showAgentAs } from "./agentActions";
import { effectiveView, viewSwitch } from "./settingsMigrate";
import { flatAgentOrder, reasonText, rowMeta, stepPath, treeRows, type TreeRow } from "./sidebarModel";
import { ciOf, countByRoot, isBlocking, samePath } from "./attention";
import type { PullRequest, Repo } from "./types";
import { ciLabel } from "./nodes/PrCard";

// which projects are folded (their roots, kept across restarts); the old switch folded the whole list at once
const FOLD_KEY = "maestro.foldedProjects";
const OLD_SHUT_KEY = "maestro.projectsShut";

/** The folded projects' roots, as last saved (also read by Ctrl+Shift+↑/↓, which skips them). */
export function readFolded(): string[] {
  try {
    const v = JSON.parse(localStorage.getItem(FOLD_KEY) ?? "[]");
    return Array.isArray(v) ? v.filter((x): x is string => typeof x === "string") : [];
  } catch {
    return [];
  }
}

/** The folded projects and a setter that saves them (only roots still in the list). */
function useFolded(repos: Repo[], ready: boolean) {
  const [folded, setFoldedState] = useState(readFolded);
  const save = (next: string[]) => {
    const kept = next.filter((root) => repos.some((r) => r.root === root));
    setFoldedState(kept);
    try {
      localStorage.setItem(FOLD_KEY, JSON.stringify(kept));
    } catch {}
  };
  // once the list is read: the old "projects folded" switch becomes every project folded, then it's gone
  useEffect(() => {
    if (!ready) return;
    try {
      const old = localStorage.getItem(OLD_SHUT_KEY);
      if (old === null) return;
      localStorage.removeItem(OLD_SHUT_KEY);
      if (old === "1" && localStorage.getItem(FOLD_KEY) === null) save(repos.map((r) => r.root));
    } catch {}
  }, [ready]);
  return [folded, save] as const;
}

/**
 * 목록에서 제거: asks first (the app's confirmation), then takes the project off the list; the files and worktrees on
 * disk stay. True once it's gone (a failure shows as a toast). The project menu and settings → 프로젝트 use it.
 */
export async function removeFromList(r: Repo): Promise<boolean> {
  const ok = await ask({ title: `'${r.name}'을 목록에서 제거할까요?`, body: "디스크의 파일과 워크트리는 지우지 않아요", confirm: "목록에서 제거", danger: true });
  if (!ok) return false;
  const s = useStore.getState();
  try {
    await s.removeRepo(r.root);
    return true;
  } catch (e) {
    s.setError(e);
    return false;
  }
}

/** Adds a project folder: picked in the OS dialog; a folder that isn't a git repo with a commit yet is set up first, if the user agrees. */
export async function addProject() {
  const { addRepo, setError } = useStore.getState();
  try {
    const dir = await open({ directory: true, title: "프로젝트 폴더 선택" });
    if (typeof dir !== "string") return;
    // worktrees need a git repo with at least one commit: offer to set that up rather than refuse the folder
    const state = await ipc.repoState(dir);
    if (state !== "ready") {
      const ok = await ask(
        state === "none"
          ? { title: "Git 저장소가 아니에요", body: "git init을 하고 지금 있는 파일로 첫 커밋을 만들어 프로젝트로 추가해요", list: [dir], confirm: "Git 저장소로 만들고 추가" }
          : { title: "아직 커밋이 없어요", body: "워커가 갈라져 나올 첫 커밋이 필요해요. 지금 있는 파일로 첫 커밋을 만들어 프로젝트로 추가해요.", list: [dir], confirm: "첫 커밋 만들고 추가" },
      );
      if (!ok) return;
      await ipc.initRepo(dir);
    }
    await addRepo(dir);
  } catch (e) {
    setError(e);
  }
}

/** The new agent dialog for a project (its map comes up behind it, where the new agent appears). */
function newAgent(root: string, mode: NewAgentTarget["mode"] = "agent") {
  const s = useStore.getState();
  s.setActive(root);
  s.openNewAgent({ root, mode });
}

/**
 * Ctrl+Shift+↑/↓ (shortcuts.ts): opens the agent before or after the one on screen, in sidebar order, skipping
 * folded projects; it stops at either end.
 */
export function stepAgent(dir: -1 | 1) {
  const s = useStore.getState();
  const next = stepPath(flatAgentOrder(s.repos, s.worktrees, readFolded()), s.selectedPath, dir);
  if (next) s.select(next);
}

/** The session rows and project heads of the list, in screen order: ↑/↓ move the focus between them. */
const LIST_ITEMS = ".repo-head, .sess-row";
function onListKey(e: React.KeyboardEvent<HTMLElement>) {
  if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
  if (e.ctrlKey || e.metaKey || e.altKey || e.shiftKey) return;
  const t = e.target;
  if (!(t instanceof HTMLElement) || !t.matches(LIST_ITEMS)) return;
  const items = [...e.currentTarget.querySelectorAll<HTMLElement>(LIST_ITEMS)];
  const next = items[items.indexOf(t) + (e.key === "ArrowDown" ? 1 : -1)];
  e.preventDefault();
  next?.focus();
}

/** Shift+F10 and the menu key: the item's menu, under it (no pointer to open it at). */
const isMenuKey = (e: React.KeyboardEvent) => (e.shiftKey && e.key === "F10") || e.key === "ContextMenu";

/**
 * The left sidebar: a 32px head (프로젝트, fold all, add a project), the projects with their agents (scrolls), and a
 * foot with the shared 문서 and 플레이북 (docs/features/ui/side-panels.md).
 */
export function Sidebar() {
  const ready = useStore((s) => s.ready);
  const repos = useStore((s) => s.repos);
  const [folded, setFolded] = useFolded(repos, ready);
  // the board the focused group shows, while it's on the system project (its row is lit)
  const sys = useStore((s) => (s.activeRoot === SYSTEM_ROOT ? s.systemView : undefined));
  const allFolded = repos.length > 0 && repos.every((r) => folded.includes(r.root));
  const toggle = (root: string) => setFolded(folded.includes(root) ? folded.filter((x) => x !== root) : [...folded, root]);

  const removeProject = async (r: Repo) => {
    if (await removeFromList(r)) setFolded(folded.filter((x) => x !== r.root));
  };

  // the project menu (⋯ and right click): making things first, the removal last and red
  const projectMenu = (r: Repo, at: MenuAt, owner: Element) => {
    const s = useStore.getState();
    const maestro = s.worktrees.find((w) => w.root === r.root && w.isMaestro);
    openMenu(
      at,
      [
        { label: "새 에이전트", icon: <Plus size={14} />, onSelect: () => newAgent(r.root) },
        maestro
          ? { label: "Lead 열기", onSelect: () => useStore.getState().select(maestro.path, true) }
          : { label: "Lead 만들기", onSelect: () => newAgent(r.root, "maestro") },
        { label: "분할로 열기", icon: <Columns2 size={14} />, onSelect: () => useStore.getState().openProject(r.root) },
        { label: "프로젝트 설정…", icon: <Settings size={14} />, onSelect: () => useStore.getState().openSettings({ section: "projects", root: r.root }) },
        { label: "폴더 경로 복사", icon: <Copy size={14} />, onSelect: () => copyText(r.root) },
        "sep",
        { label: "목록에서 제거…", icon: <X size={14} />, danger: true, onSelect: () => void removeProject(r) },
      ],
      { owner },
    );
  };

  const navRow = (view: SystemView, label: string, icon: React.ReactNode, hint: string) => (
    <button className="sb-nav" data-on={sys === view || undefined} title={hint} onClick={() => useStore.getState().openSystem(view)}>
      {icon}
      {label}
    </button>
  );

  return (
    <aside className="sidebar">
      {/* what every project shares, above the projects (as Linear keeps Inbox and My issues above its teams) */}
      <nav className="sb-top">
        {navRow("docs", "문서", <FileText />, "모든 프로젝트가 같이 쓰는 문서 (페르소나, 지침, PRD, 디자인, RAG)")}
        {navRow("playbooks", "플레이북", <BookOpen />, "Lead가 워커들을 부리는 절차")}
      </nav>
      <div className="sb-head">
        <span className="sb-title">프로젝트</span>
        <button
          className="icon sb-head-btn"
          title={allFolded ? "모두 펴기" : "모두 접기"}
          aria-label={allFolded ? "모두 펴기" : "모두 접기"}
          disabled={!repos.length}
          onClick={() => setFolded(allFolded ? [] : repos.map((r) => r.root))}
        >
          {allFolded ? <ChevronsUpDown size={14} /> : <ChevronsDownUp size={14} />}
        </button>
        <button className="icon sb-head-btn" title="프로젝트 추가" aria-label="프로젝트 추가" onClick={() => void addProject()}>
          <FolderPlus size={14} />
        </button>
      </div>
      <div className="sb-body" onKeyDown={onListKey}>
        {repos.map((r) => (
          <ProjectGroup key={r.root} repo={r} folded={folded.includes(r.root)} onFold={() => toggle(r.root)} onMenu={(at, owner) => projectMenu(r, at, owner)} />
        ))}
        {ready && !repos.length && (
          <div className="sb-empty">
            <p className="muted small">프로젝트 폴더를 추가하면 에이전트마다 따로 된 작업 폴더에서 일해요</p>
            <button className="sb-empty-btn" onClick={() => void addProject()}>
              <FolderPlus size={14} />
              프로젝트 추가
            </button>
          </div>
        )}
      </div>
      <nav className="sb-foot">
        {/* settings sit at the bottom left, where other apps keep them; the top bar has its gear only while this
            sidebar is hidden (App.tsx) */}
        <button className="sb-nav" title={tip("settings")} onClick={() => useStore.getState().openSettings()}>
          <Settings />
          설정
        </button>
      </nav>
    </aside>
  );
}

/**
 * What keeps a project's agents waiting for the user (확인할 일 that block: tool approvals, terminal questions, merge
 * requests; not PRs), and how many agents work.
 */
function useProjectSummary(root: string) {
  const waiting = useStore((s) => countByRoot(selectAttention(s).filter(isBlocking))[root] ?? 0);
  const running = useStore((s) => s.worktrees.filter((w) => w.root === root && w.status === "RUNNING").length);
  return { waiting, running };
}

/**
 * One project: its 28px head (the chevron folds it, anywhere else opens its map at once; on hover `+` and ⋯ cover
 * the folded summary) and, unfolded, its agents.
 */
function ProjectGroup({ repo: r, folded, onFold, onMenu }: { repo: Repo; folded: boolean; onFold: () => void; onMenu: (at: MenuAt, owner: Element) => void }) {
  const active = useStore((s) => s.activeRoot === r.root);
  const selectedPath = useStore((s) => s.selectedPath);
  const allWts = useStore((s) => s.worktrees);
  const wts = allWts.filter((w) => w.root === r.root);
  const { waiting, running } = useProjectSummary(r.root);
  const compact = useSettings((s) => s.sidebarDensity === "one");
  const { setActive } = useStore.getState();
  const summary = !folded ? null : waiting > 0 ? (
    <span className="repo-sum" data-warn>승인 대기 {waiting}</span>
  ) : running > 0 ? (
    <span className="repo-sum">작업 중 {running}</span>
  ) : null;

  return (
    <section className="repo" data-active={active || undefined}>
      <div
        className="repo-head"
        role="button"
        tabIndex={0}
        aria-expanded={!folded}
        title={`${r.name} · ${r.defaultBranch} · ${r.root}`}
        onClick={() => setActive(r.root)}
        onKeyDown={(e) => {
          if (e.target !== e.currentTarget) return;
          if (isMenuKey(e)) {
            e.preventDefault();
            return onMenu(e.currentTarget, e.currentTarget);
          }
          if (e.key !== "Enter" && e.key !== " ") return;
          e.preventDefault();
          setActive(r.root);
        }}
        onContextMenu={(e) => {
          e.preventDefault();
          // the menu key and Shift+F10 come with no pointer: under the head then
          onMenu(e.clientX || e.clientY ? pointOf(e) : e.currentTarget, e.currentTarget);
        }}
      >
        <button
          type="button"
          className="repo-caret"
          data-open={!folded || undefined}
          title={folded ? "펴기" : "접기"}
          aria-label={folded ? "펴기" : "접기"}
          onClick={(e) => {
            e.stopPropagation();
            onFold();
          }}
        >
          <ChevronRight size={12} />
        </button>
        <span className="repo-name ellipsis">{r.name}</span>
        {summary}
        <span className="repo-actions">
          <button
            type="button"
            className="icon"
            title="새 에이전트"
            aria-label="새 에이전트"
            onClick={(e) => {
              e.stopPropagation();
              newAgent(r.root);
            }}
          >
            <Plus size={14} />
          </button>
          <button
            type="button"
            className="icon"
            title="프로젝트 메뉴"
            aria-label="프로젝트 메뉴"
            aria-haspopup="menu"
            onClick={(e) => {
              e.stopPropagation();
              onMenu(e.currentTarget, e.currentTarget.closest(".repo-head")!);
            }}
          >
            <Ellipsis size={14} />
          </button>
        </span>
      </div>
      {!folded && (
        <div className="sess-list" role="group" aria-label={`${r.name} 에이전트`}>
          {treeRows(wts).map((row, i, rows) => (
            <SessionRowSlot key={row.w.path} row={row} subhead={row.loose && !rows[i - 1]?.loose} compact={compact} selected={!!selectedPath && samePath(row.w.path, selectedPath)} />
          ))}
          {!wts.length && (
            <button type="button" className="wt-empty" onClick={() => newAgent(r.root)}>
              <Plus size={14} />
              새 에이전트
            </button>
          )}
        </div>
      )}
    </section>
  );
}

/** A row, after the 독립 에이전트 subhead when it is the first agent nobody leads (only in a project with a Maestro). */
function SessionRowSlot({ row, subhead, compact, selected }: { row: TreeRow; subhead: boolean; compact: boolean; selected: boolean }) {
  return (
    <>
      {subhead && <div className="sess-subhead">독립 에이전트</div>}
      <SessionRow row={row} compact={compact} selected={selected} />
    </>
  );
}

type PrBadge = { text: string; num: string; tone?: "err" | "muted"; title: string };

/** A PR in a row's second line: "#41", "#41 CI 실패" (red) or "#41 병합됨" (muted); a closed one shows nothing. */
function prBadge(p: PullRequest | undefined): PrBadge | null {
  if (!p || p.state === "CLOSED") return null;
  const num = `#${p.number}`;
  const title = `PR ${num} · ${p.state === "MERGED" ? "병합됨" : `CI ${ciLabel(p)}`}${p.error ? ` · 오류: ${p.error}` : ""}`;
  if (p.state === "MERGED") return { text: `${num} 병합됨`, num, tone: "muted", title };
  return ciOf(p) === "failure" ? { text: `${num} CI 실패`, num, tone: "err", title } : { text: num, num, title };
}

/**
 * One agent in the sidebar (docs/features/ui/side-panels.md "세션 행"): 44px, two lines (or 28px, one line, in the
 * compact density). The status lane (a click marks it read / unread), the name (the Lead's in the full text colour: no badge, it leads by place, first in its project with its
 * workers hanging under it, and by "워커 N" on its second line), then the
 * agent logo · branch · PR, or what waits for the user in their place. A click previews it, a double click keeps its
 * tab; ⋯ (on hover) and a right click open its menu. Deeper than three levels it stays at the third indent.
 */
/** One depth step of a session row's box (px), and where a row's status lane centre sits from its box's left edge
 * (8 padding + half the 16px lane): a child's box starts 4px right of its parent's lane, where the guide runs. */
const INDENT = 20;
const LANE_CENTER = 16;

function SessionRow({ row, compact, selected }: { row: TreeRow; compact: boolean; selected: boolean }) {
  const w = row.w;
  // the second line's reason comes from 확인할 일 (attention.ts), like the top bar and the window title
  const meta = useStore(useShallow((s) => rowMeta(w, selectAttention(s))));
  const pr = prBadge(usePr(w.path));
  const { discard, busy, dialog } = useDiscard(w);
  const ref = useRef<HTMLDivElement>(null);
  const noAgent = meta.line2 === "noAgent";
  const waiting = meta.line2 === "approval" || meta.line2 === "tui" || meta.line2 === "merge";
  const reason = busy ? (busy === "check" ? "확인 중…" : "지우는 중…") : reasonText(meta);
  const workers = meta.workerCount > 0 ? `워커 ${meta.workerCount}` : null;
  const line2Text = reason ?? [w.branch, workers, pr?.text].filter(Boolean).join(" · ");
  const deep = row.depth > row.shown ? `${row.depth}단계 아래 워커` : null;
  const head = compact ? `${w.name}\n${line2Text}` : w.name !== w.branch ? `${w.name}\n브랜치 ${w.branch}` : w.name;
  const title = [head, deep, w.path].filter(Boolean).join("\n");

  // brought into view when it becomes the agent on screen (Ctrl+Shift+↑/↓, a tab, the last place on start)
  useEffect(() => {
    if (selected) ref.current?.scrollIntoView({ block: "nearest" });
  }, [selected]);

  const { select, markUnread } = useStore.getState();
  // the lane: read ↔ unread; while it waits or works there is nothing to mark, so it opens the agent like the row
  const onLane = noAgent ? undefined : () => (waiting || w.status === "RUNNING" ? select(w.path) : markUnread(w.path, !w.unread));

  const menu = (at: MenuAt, owner: Element) => {
    const s = useStore.getState();
    // 터미널로 보기 / 채팅으로 돌아가기: this agent's view for now, from the one it shows (docs/features/agents/chat-view.md)
    const global = useSettings.getState().agentView;
    const flip = viewSwitch(effectiveView(w.path, s.viewOverride, global), global);
    const items: MenuItem[] = noAgent
      ? [
          { label: "열기", onSelect: () => s.select(w.path) },
          // its tab shows how to attach an agent (AttachAgent)
          { label: "에이전트 연결…", icon: <Plus size={14} />, onSelect: () => s.select(w.path, true) },
          { label: "폴더 경로 복사", icon: <Copy size={14} />, onSelect: () => copyText(w.path) },
        ]
      : [
          { label: "열기", onSelect: () => s.select(w.path) },
          { label: "고정 탭으로 열기", onSelect: () => s.select(w.path, true) },
          { label: "분할로 열기", icon: <Columns2 size={14} />, onSelect: () => openInSplit(w) },
          { label: "새 창으로 열기", icon: <ExternalLink size={14} />, onSelect: () => openInWindow(w) },
          "sep",
          { label: "에이전트 설정…", icon: <Settings2 size={14} />, shortcut: keysOf("rename"), onSelect: () => s.openAgentSettings({ path: w.path }) },
          { label: "워커 추가", icon: <Plus size={14} />, onSelect: () => s.openNewAgent({ root: w.root, mode: "worker", parentPath: w.path }) },
          { label: flip.menu, icon: flip.to === "cli" ? <SquareTerminal size={14} /> : <MessageSquare size={14} />, onSelect: () => showAgentAs(w.path, flip.to) },
          { label: w.unread ? "읽음으로 표시" : "안 읽음으로 표시", onSelect: () => s.markUnread(w.path, !w.unread) },
          { label: "브랜치 이름 복사", icon: <GitBranch size={14} />, onSelect: () => copyText(w.branch) },
          { label: "폴더 경로 복사", icon: <Copy size={14} />, onSelect: () => copyText(w.path) },
          "sep",
          { label: w.isMaestro ? "Lead 없애기…" : "폐기…", icon: <Trash2 size={14} />, danger: true, disabled: !!busy, onSelect: () => void discard() },
        ];
    openMenu(at, items, { owner, minWidth: 220 });
  };

  // the discard dialog sits beside the row, not in it: a portal's events still bubble through the React tree
  return (
    <>
      <div
        ref={ref}
        className="sess-row"
        role="button"
        // F2 (shortcuts.ts "rename") opens this agent's settings at its name
        data-agent-path={noAgent ? undefined : w.path}
        tabIndex={selected ? 0 : -1}
        data-compact={compact || undefined}
        data-selected={selected || undefined}
        data-lead={w.isMaestro || undefined}
        data-unread={(w.unread && !noAgent) || undefined}
        aria-current={selected || undefined}
        aria-busy={!!busy || undefined}
        // the row's own box steps in with its depth, so its hover and selection start just right of the guide it
        // hangs from and every guide runs on unbroken beside the boxes (not under them)
        style={{ marginLeft: row.shown * INDENT }}
        title={title}
        // a click previews the agent in the reusable tab, a double click keeps its tab (VS Code); it opens in the
        // split showing its project (select finds or switches one)
        onClick={() => select(w.path)}
        onDoubleClick={() => select(w.path, true)}
        onKeyDown={(e) => {
          if (e.target !== e.currentTarget) return;
          if (isMenuKey(e)) {
            e.preventDefault();
            return menu(e.currentTarget, e.currentTarget);
          }
          if (e.key !== "Enter") return;
          e.preventDefault();
          select(w.path);
        }}
        onContextMenu={(e) => {
          e.preventDefault();
          menu(e.clientX || e.clientY ? pointOf(e) : e.currentTarget, e.currentTarget);
        }}
      >
        {/* one guide per level, under the lane of the agent it hangs from: left of this row's box, outside it */}
        {Array.from({ length: row.shown }, (_, k) => (
          <span key={k} className="sess-guide" style={{ left: LANE_CENTER + (k - row.shown) * INDENT }} aria-hidden />
        ))}
        <span className="sess-lane">
          <StatusGlyph status={w.status} waiting={waiting} unread={w.unread} noAgent={noAgent} onToggleRead={onLane} />
        </span>
        <span className="sess-text">
          <span className="sess-line1">
            <span className="sess-name ellipsis">{w.name}</span>
            {compact && !reason && pr && <span className="sess-pr-num" data-tone={pr.tone} title={pr.title}>{pr.num}</span>}
          </span>
          {!compact && (
            <span className="sess-line2">
              {reason ? (
                <span className="sess-reason ellipsis" data-warn={(waiting && !busy) || undefined}>{reason}</span>
              ) : (
                <>
                  <AgentLogo id={w.adapterId} size={12} />
                  <span className="sess-branch ellipsis">{w.branch}</span>
                  {workers && <span className="sess-meta">· {workers}</span>}
                  {pr && <span className="sess-meta sess-pr" data-tone={pr.tone} title={pr.title}>· {pr.text}</span>}
                </>
              )}
            </span>
          )}
        </span>
        <span className="sess-more">
          <button
            type="button"
            className="icon"
            title="에이전트 메뉴"
            aria-label="에이전트 메뉴"
            aria-haspopup="menu"
            onClick={(e) => {
              e.stopPropagation();
              menu(e.currentTarget, e.currentTarget.closest(".sess-row")!);
            }}
            onDoubleClick={(e) => e.stopPropagation()}
          >
            <Ellipsis size={14} />
          </button>
        </span>
      </div>
      {dialog}
    </>
  );
}
