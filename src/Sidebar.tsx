import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { ipc } from "./ipc";
import { BookOpen, ChevronRight, ChevronsDownUp, ChevronsUpDown, Columns2, Copy, Ellipsis, FileText, FolderPlus, MaestroIcon, Plus, Settings, X } from "./icons";
import { AgentLogo } from "./agents";
import { AgentGlyph } from "./status";
import { ask } from "./ui/Confirm";
import { openMenu, pointOf, type MenuAt } from "./ui/Menu";
import { samePath, SYSTEM_ROOT, usePr, useStore, type NewAgentTarget, type SystemView, type Worktree } from "./store";
import type { Repo } from "./types";
import { ciLabel, ciState } from "./nodes/PrCard";

// the glyph says which state an agent is in (status.tsx); only a waiting approval gets words
const STATUS_LABEL: Record<string, string> = {};

/**
 * A repo's worktrees in hierarchy order, as the canvas arrows draw it: the Maestro first with its workers
 * nested below, then the worktrees nobody leads (`loose`, only marked when there is a Maestro).
 */
function treeRows(wts: Worktree[]) {
  const key = (p: string) => p.toLowerCase();
  const byPath = new Map(wts.map((w) => [key(w.path), w]));
  const led = new Set(wts.flatMap((w) => w.workers.map(key)));
  const rows: Array<{ w: Worktree; depth: number; loose: boolean }> = [];
  const seen = new Set<string>();
  let loose = false;
  const walk = (w: Worktree, depth: number) => {
    if (seen.has(key(w.path))) return;
    seen.add(key(w.path));
    rows.push({ w, depth, loose });
    for (const p of w.workers) {
      const c = byPath.get(key(p));
      if (c) walk(c, depth + 1);
    }
  };
  wts.filter((w) => w.isMaestro).forEach((w) => walk(w, 0));
  loose = rows.length > 0;
  wts.filter((w) => !led.has(key(w.path))).forEach((w) => walk(w, 0));
  wts.forEach((w) => walk(w, 0)); // only reachable through a broken (cyclic) link file
  return rows;
}

// which projects are folded (their roots, kept across restarts); the old switch folded the whole list at once
const FOLD_KEY = "maestro.foldedProjects";
const OLD_SHUT_KEY = "maestro.projectsShut";

function readFolded(): string[] {
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

const copyText = (text: string) => void navigator.clipboard.writeText(text).catch(useStore.getState().setError);

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
    const ok = await ask({ title: `'${r.name}'을 목록에서 제거할까요?`, body: "디스크의 파일과 워크트리는 지우지 않아요", confirm: "목록에서 제거", danger: true });
    if (!ok) return;
    const s = useStore.getState();
    s.removeRepo(r.root)
      .then(() => setFolded(folded.filter((x) => x !== r.root)))
      .catch(s.setError);
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
          ? { label: "마에스트로 열기", icon: <MaestroIcon />, onSelect: () => useStore.getState().select(maestro.path, true) }
          : { label: "마에스트로 만들기", icon: <MaestroIcon />, onSelect: () => newAgent(r.root, "maestro") },
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
      <div className="sb-body">
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
        {navRow("docs", "문서", <FileText />, "모든 프로젝트가 같이 쓰는 문서 (페르소나, 지침, PRD, 디자인, RAG)")}
        {navRow("playbooks", "플레이북", <BookOpen />, "마에스트로가 워커들을 부리는 절차")}
      </nav>
    </aside>
  );
}

/** What waits for the user in a project (tool approvals, terminal questions, merge requests), and how many agents work. */
function useProjectSummary(root: string) {
  const waiting = useStore((s) => {
    const paths = s.worktrees.filter((w) => w.root === root).map((w) => w.path);
    const here = (p: string) => paths.some((q) => samePath(q, p));
    return s.approvals.filter((a) => here(a.worktreePath)).length + s.tuiWaits.filter((w) => here(w.worktreePath)).length + s.merges.filter((m) => here(m.maestroPath)).length;
  });
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
  const { setActive, select } = useStore.getState();
  const approvals = useStore((s) => s.approvals);
  const tuiWaits = useStore((s) => s.tuiWaits);
  const merges = useStore((s) => s.merges);
  const waitingPaths = new Set([...approvals.map((a) => a.worktreePath.toLowerCase()), ...tuiWaits.map((w) => w.worktreePath.toLowerCase()), ...merges.map((m) => m.maestroPath.toLowerCase())]);
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
          if (e.target !== e.currentTarget || (e.key !== "Enter" && e.key !== " ")) return;
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
        <>
          {treeRows(wts).map(({ w, depth, loose }, i, rows) => (
            <div key={w.path}>
              {loose && !rows[i - 1]?.loose && <div className="wt-group muted small">상위 없는 워크트리</div>}
              <div
                className="wt-row"
                data-maestro={w.isMaestro || undefined}
                data-selected={w.path === selectedPath || undefined}
                style={{ paddingLeft: 22 + depth * 16 }}
                title={w.path}
                // a click previews the worktree in the reusable tab, a double click keeps its tab (VS Code)
                // it opens in the split showing its project (select finds or switches one)
                onClick={() => select(w.path)}
                onDoubleClick={() => select(w.path, true)}
              >
                {depth > 0 && <span className="wt-elbow" aria-hidden />}
                <AgentGlyph w={w} />
                <AgentLogo id={w.adapterId} size={14} />
                {w.isMaestro && <span className="maestro-crown" title="이 프로젝트의 마에스트로"><MaestroIcon /></span>}
                <span className="wt-branch" title={w.name !== w.branch ? `브랜치 ${w.branch}` : undefined}>{w.name}</span>
                <span className="spacer" />
                <SidebarPr path={w.path} />
                <span className="muted small">
                  {waitingPaths.has(w.path.toLowerCase()) ? "승인 대기" : STATUS_LABEL[w.status]}
                </span>
              </div>
            </div>
          ))}
          {!wts.length && (
            <button type="button" className="wt-empty" onClick={() => newAgent(r.root)}>
              <Plus size={14} />
              새 에이전트
            </button>
          )}
        </>
      )}
    </section>
  );
}

/** A worker's PR on its sidebar row: "PR #N" with a dot for its CI, or "CI 실패". */
function SidebarPr({ path }: { path: string }) {
  const p = usePr(path);
  if (!p || p.state !== "OPEN") return null;
  return (
    <span className="sidebar-pr small" data-ci={ciState(p)} title={`PR #${p.number} · CI ${ciLabel(p)}${p.error ? ` · 오류: ${p.error}` : ""}`}>
      {ciState(p) === "failure" ? "CI 실패" : `PR #${p.number}`}
    </span>
  );
}
