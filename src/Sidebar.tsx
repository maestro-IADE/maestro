import { useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { ipc } from "./ipc";
import { GearIcon } from "./SettingsDialog";
import { MaestroIcon } from "./icons";
import { AgentLogo } from "./NewAgentDialog";
import { SYSTEM_ROOT, usePr, useStore, useWaiting, type SystemView, type Worktree } from "./store";
import type { Repo } from "./types";
import { ciLabel, ciState } from "./nodes/PrCard";

// the dot says which state an agent is in (it pulses while working); only a waiting approval gets words
const STATUS_LABEL: Record<string, string> = {};

export function StatusDot({ w }: { w: Worktree }) {
  const waiting = useWaiting(w.path);
  return <span className="dot" data-status={w.status} data-unread={w.unread || undefined} data-waiting={waiting || undefined} title={waiting ? "승인 대기" : undefined} />;
}

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

const SHUT_KEY = "maestro.projectsShut";

// outlines after Lucide (ISC): "file-text" (문서), "book-open" (플레이북)
const DOCS_ICON = "M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8zM14 2v6h6M16 13H8M16 17H8M10 9H8";
const BOOKS_ICON = "M2 3h6a4 4 0 0 1 4 4v14a3 3 0 0 0-3-3H2zM22 3h-6a4 4 0 0 0-4 4v14a3 3 0 0 1 3-3h7z";

export function Sidebar({ onNewWorktree, onOpenSettings }: { onNewWorktree: (repo: Repo) => void; onOpenSettings: () => void }) {
  const { ready, repos, activeRoot, worktrees, selectedPath, addRepo, removeRepo, setActive, openProject, select, setError, approvals, tuiWaits, merges } = useStore();
  const waitingPaths = new Set([...approvals.map((a) => a.worktreePath.toLowerCase()), ...tuiWaits.map((w) => w.worktreePath.toLowerCase()), ...merges.map((m) => m.maestroPath.toLowerCase())]);
  const clickTimer = useRef<ReturnType<typeof setTimeout>>(undefined);
  // the board the focused group shows, while it's on the system project (its row is lit)
  const sys = useStore((s) => (s.activeRoot === SYSTEM_ROOT ? s.systemView : undefined));
  // the project list folded away (kept across restarts)
  const [shut, setShutState] = useState(() => {
    try {
      return localStorage.getItem(SHUT_KEY) === "1";
    } catch {
      return false;
    }
  });
  const setShut = (v: boolean) => {
    setShutState(v);
    try {
      localStorage.setItem(SHUT_KEY, v ? "1" : "0");
    } catch {}
  };
  const navRow = (view: SystemView, label: string, icon: string, hint: string) => (
    <button className="sb-nav" data-on={sys === view || undefined} title={hint} onClick={() => useStore.getState().openSystem(view)}>
      <svg width={16} height={16} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
        <path d={icon} />
      </svg>
      {label}
    </button>
  );

  const pickRepo = async () => {
    try {
      const dir = await open({ directory: true, title: "프로젝트 폴더 선택" });
      if (typeof dir !== "string") return;
      // worktrees need a git repo with at least one commit: offer to set that up rather than refuse the folder
      const state = await ipc.repoState(dir);
      if (state !== "ready") {
        const ask =
          state === "none"
            ? `'${dir}'는 Git 저장소가 아니에요.\n\ngit init을 하고 지금 있는 파일로 첫 커밋을 만들어 프로젝트로 추가할까요?`
            : `'${dir}' 저장소에 아직 커밋이 없어요. 워커가 갈라져 나올 첫 커밋이 필요해요.\n\n지금 있는 파일로 첫 커밋을 만들까요?`;
        if (!confirm(ask)) return;
        await ipc.initRepo(dir);
      }
      await addRepo(dir);
    } catch (e) {
      setError(e);
    }
  };

  return (
    <aside className="sidebar">
      <nav className="sb-navs">
        {navRow("docs", "문서", DOCS_ICON, "모든 프로젝트가 같이 쓰는 문서 (페르소나, 지침, PRD, 디자인, RAG) · 시스템 탭에 열려요")}
        {navRow("playbooks", "플레이북", BOOKS_ICON, "마에스트로가 워커들을 부리는 절차 · 시스템 탭에 열려요")}
      </nav>
      <div className="sb-section" onClick={() => setShut(!shut)} title={shut ? "펼치기" : "접기"}>
        <span>프로젝트</span>
        <span className="docs-caret" data-open={!shut || undefined} aria-hidden>
          <svg width={14} height={14} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
            <path d="m9 18 6-6-6-6" />
          </svg>
        </span>
        <span className="spacer" />
        <button
          className="icon"
          title="저장소 추가"
          aria-label="저장소 추가"
          onClick={(e) => {
            e.stopPropagation();
            void pickRepo();
          }}
        >
          +
        </button>
      </div>
      {shut ? (
        <div className="sb-body" />
      ) : (
      <div className="sb-body">
        {repos.map((r) => {
          const wts = worktrees.filter((w) => w.root === r.root);
          return (
            <section key={r.root} className="repo" data-active={r.root === activeRoot || undefined}>
              {/* click: the project in the focused split (or the one showing it); double click: a split of its own */}
              {/* the click waits a moment: switching the focused split first would leave the double click nothing to split */}
              <div
                className="repo-row"
                onClick={(e) => {
                  if (e.detail > 1) return;
                  clearTimeout(clickTimer.current);
                  clickTimer.current = setTimeout(() => setActive(r.root), 220);
                }}
                onDoubleClick={() => {
                  clearTimeout(clickTimer.current);
                  openProject(r.root);
                }}
                title={`${r.root}\n더블클릭하면 분할 화면으로 열어요`}
              >
                <span className="repo-name">{r.name}</span>
                <span className="muted">{r.defaultBranch}</span>
                <span className="spacer" />
                <button
                  className="icon hover-only"
                  title="저장소 목록에서 제거 (파일은 그대로)"
                  onClick={(e) => {
                    e.stopPropagation();
                    if (confirm(`'${r.name}'을 목록에서 제거할까요? 디스크의 파일과 워크트리는 지우지 않습니다.`)) removeRepo(r.root).catch(setError);
                  }}
                >
                  ×
                </button>
                <button
                  className="icon"
                  title="새 에이전트 블록"
                  onClick={(e) => {
                    e.stopPropagation();
                    setActive(r.root);
                    onNewWorktree(r);
                  }}
                >
                  +
                </button>
              </div>
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
                    <StatusDot w={w} />
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
                <div className="wt-empty muted small" onClick={() => onNewWorktree(r)}>
                  + 에이전트 블록 만들기
                </div>
              )}
            </section>
          );
        })}
        {ready && !repos.length && (
          <div className="sb-empty">
            <p className="muted">로컬 Git 저장소를 추가하면 에이전트 블록마다 격리된 워크트리에서 에이전트를 실행할 수 있어요</p>
            <button onClick={pickRepo}>저장소 추가</button>
          </div>
        )}
      </div>
      )}
      <div className="sb-foot">
        <button className="sb-settings" onClick={onOpenSettings}>
          <GearIcon />
          설정
        </button>
      </div>
    </aside>
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
