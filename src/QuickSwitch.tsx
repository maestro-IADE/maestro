import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { AgentLogo, agentLabel } from "./agents";
import { AgentGlyph } from "./status";
import { addProject } from "./Sidebar";
import { flatAgentOrder } from "./sidebarModel";
import { moveHot, rankQuick, type QuickItem } from "./quickModel";
import { keysOf, matchShortcut } from "./shortcuts";
import { isImeComposing } from "./ime";
import { selectAttention, useAttentionCount, useStore, type Worktree } from "./store";
import { nextItem } from "./attention";
import { useSettings } from "./settings";
import { Bell, BookOpen, FileText, Folder, FolderPlus, MaestroIcon, MessageCircleQuestion, MessageSquare, PanelLeft, PanelRight, Plus, Search, Settings, SquareTerminal } from "./icons";

// 빠른 이동 (Ctrl+K, docs/features/ui/quick-switch.md): a palette over the agents, the projects and the app's actions,
// built from the store. Which rows match what was typed is quickModel.ts (pure, tested by scripts/quick-check.ts).

/** The side panels of the main window (a pop-out window has none). */
export type PanelToggles = { left: boolean; right: boolean; setLeft: (on: boolean) => void; setRight: (on: boolean) => void };

type Entry = QuickItem & {
  /** what the row draws before the label (actions and projects; an agent draws its glyph) */
  icon?: ReactNode;
  /** muted text after the label (an agent's project, a count) */
  meta?: string;
  /** a keycap on the right (actions with a shortcut) */
  keys?: string;
  w?: Worktree;
  run: () => void;
};

// the rows drawn at most: more than this many matches means the query is too short to pick from anyway
const LIMIT = 100;

/** The palette of this window, while the store says it's open (Ctrl+K). Mounted once per window, in WindowLayers. */
export function QuickSwitch({ panels }: { panels?: PanelToggles }) {
  const open = useStore((s) => s.quickOpen);
  if (!open) return null;
  return createPortal(<QuickPanel panels={panels} onClose={() => useStore.getState().setQuickOpen(false)} />, document.body);
}

/** Every row the palette can show, before any query: agents in sidebar order, then projects, then actions. */
function useEntries(panels?: PanelToggles): Entry[] {
  const repos = useStore((s) => s.repos);
  const worktrees = useStore((s) => s.worktrees);
  const activeRoot = useStore((s) => s.activeRoot);
  const waiting = useAttentionCount().all;
  const agentView = useSettings((s) => s.agentView);
  const left = panels?.left;
  const right = panels?.right;

  return useMemo(() => {
    const s = () => useStore.getState();
    const repoName = (root: string) => repos.find((r) => r.root === root)?.name ?? "";
    const byPath = new Map(worktrees.map((w) => [w.path, w]));
    // every agent, folded projects included (the palette is how to reach them without unfolding)
    const agents: Entry[] = flatAgentOrder(repos, worktrees, []).flatMap((path) => {
      const w = byPath.get(path);
      if (!w) return [];
      const project = repoName(w.root);
      return [{
        id: `agent:${w.path}`,
        kind: "agent" as const,
        label: w.name,
        keywords: [project, w.branch, agentLabel(w.adapterId), ...(w.isMaestro ? ["Lead"] : [])],
        meta: project,
        w,
        // as a sidebar row's click: a preview tab, its own first screen
        run: () => s().select(w.path),
      }];
    });
    const projects: Entry[] = repos.map((r) => ({
      id: `project:${r.root}`,
      kind: "project",
      label: r.name,
      keywords: [r.defaultBranch, "프로젝트 지도"],
      icon: <Folder size={14} />,
      run: () => s().setActive(r.root),
    }));
    // the project on screen first, for 새 에이전트 and Lead 만들기
    const byActive = [...repos].sort((a, b) => Number(b.root === activeRoot) - Number(a.root === activeRoot));
    const newAgent = (root: string, mode: "agent" | "maestro") => {
      s().setActive(root);
      s().openNewAgent({ root, mode });
    };
    const other = agentView === "chat" ? "cli" : "chat";
    const actions: Entry[] = [
      { id: "settings", kind: "action", label: "설정", keywords: ["settings"], keys: keysOf("settings"), icon: <Settings size={14} />, run: () => s().openSettings() },
      { id: "attention", kind: "action", label: "확인할 일", keywords: ["승인 대기", "병합 요청"], meta: waiting ? `${waiting}개` : undefined, icon: <Bell size={14} />, run: () => s().setAttentionOpen(true) },
      ...(waiting
        ? [{
            id: "nextAttention",
            kind: "action" as const,
            label: "다음 확인할 일",
            keywords: ["승인 대기", "병합 요청"],
            keys: keysOf("nextAttention"),
            icon: <MessageCircleQuestion size={14} />,
            run: () => {
              const st = s();
              const it = nextItem(selectAttention(st), st.selectedPath);
              if (it) st.select(it.path, true);
            },
          }]
        : []),
      ...byActive.map((r) => ({ id: `new:${r.root}`, kind: "action" as const, label: `새 에이전트 · ${r.name}`, keywords: ["new agent"], icon: <Plus size={14} />, run: () => newAgent(r.root, "agent") })),
      ...byActive
        .filter((r) => !worktrees.some((w) => w.root === r.root && w.isMaestro))
        .map((r) => ({ id: `maestro:${r.root}`, kind: "action" as const, label: `Lead 만들기 · ${r.name}`, keywords: ["new maestro"], icon: <MaestroIcon />, run: () => newAgent(r.root, "maestro") })),
      { id: "addProject", kind: "action", label: "프로젝트 추가", keywords: ["폴더", "add project"], icon: <FolderPlus size={14} />, run: () => void addProject() },
      {
        id: "agentView",
        kind: "action",
        // the app's 에이전트 보기 (settings → 일반), not this agent's: every view opened from now on
        label: other === "cli" ? "에이전트 보기를 터미널로" : "에이전트 보기를 채팅으로",
        keywords: ["에이전트 보기 바꾸기", "채팅", "터미널", "chat", "cli"],
        icon: other === "cli" ? <SquareTerminal size={14} /> : <MessageSquare size={14} />,
        run: () => useSettings.getState().set({ agentView: other }),
      },
      ...(panels
        ? [
            { id: "left", kind: "action" as const, label: left ? "왼쪽 패널 숨기기" : "왼쪽 패널 보이기", keywords: ["사이드바"], icon: <PanelLeft size={14} />, run: () => panels.setLeft(!left) },
            { id: "right", kind: "action" as const, label: right ? "오른쪽 패널 숨기기" : "오른쪽 패널 보이기", keywords: ["파일", "변경"], icon: <PanelRight size={14} />, run: () => panels.setRight(!right) },
          ]
        : []),
      { id: "docs", kind: "action", label: "문서", keywords: ["docs"], icon: <FileText size={14} />, run: () => s().openSystem("docs") },
      { id: "playbooks", kind: "action", label: "플레이북", keywords: ["playbook"], icon: <BookOpen size={14} />, run: () => s().openSystem("playbooks") },
    ];
    return [...agents, ...projects, ...actions];
  }, [repos, worktrees, activeRoot, waiting, agentView, !!panels, left, right]);
}

function QuickPanel({ panels, onClose }: { panels?: PanelToggles; onClose: () => void }) {
  const entries = useEntries(panels);
  const [q, setQ] = useState("");
  const [hot, setHot] = useState(0);
  const rows = useMemo(() => rankQuick(entries, q, LIMIT), [entries, q]);
  const box = useRef<HTMLDivElement>(null);
  const input = useRef<HTMLInputElement>(null);
  const list = useRef<HTMLDivElement>(null);
  // a picked row leaves the focus to what it opens; Esc or a press outside hands it back to where it was
  const picked = useRef(false);
  const cur = hot >= 0 && hot < rows.length ? hot : rows.length ? 0 : -1;

  useEffect(() => {
    const before = document.activeElement as HTMLElement | null;
    input.current?.focus();
    // something outside taking the focus (a terminal focusing itself) hands it back to the input
    const onFocus = (e: FocusEvent) => {
      if (!(e.target instanceof Node && box.current?.contains(e.target))) input.current?.focus();
    };
    document.addEventListener("focusin", onFocus, true);
    return () => {
      document.removeEventListener("focusin", onFocus, true);
      if (!picked.current && before?.isConnected) before.focus({ preventScroll: true });
    };
  }, []);
  // the highlighted row stays in view as ↑/↓ move it
  useEffect(() => void list.current?.querySelector("[data-hot]")?.scrollIntoView({ block: "nearest" }), [cur]);

  const run = (e: Entry | undefined) => {
    if (!e) return;
    picked.current = true;
    onClose();
    e.run();
  };

  const onKey = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (isImeComposing(e)) return;
    const sc = matchShortcut(e.nativeEvent);
    if (sc?.id === "quickSwitch") {
      // Ctrl+K again: closes it
      e.preventDefault();
      e.stopPropagation();
      onClose();
      return;
    }
    // Ctrl+, goes on to the window (settings): the palette makes way for it
    if (sc?.id === "settings") {
      picked.current = true;
      onClose();
      return;
    }
    const nav = { ArrowDown: 1, ArrowUp: -1 } as const;
    if (e.key in nav) {
      setHot(moveHot(cur, rows.length, nav[e.key as keyof typeof nav]));
    } else if (e.key === "Enter") {
      run(rows[cur]);
    } else if (e.key === "Escape") {
      onClose();
    } else if (e.key === "Tab") {
      // the focus stays in the input
    } else return;
    e.preventDefault();
    e.stopPropagation();
  };

  return (
    <div className="modal-backdrop quick-backdrop" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div ref={box} className="quick" role="dialog" aria-modal="true" aria-label="빠른 이동">
        <label className="quick-input">
          <Search />
          <input
            ref={input}
            value={q}
            placeholder="에이전트, 프로젝트, 동작 찾기"
            spellCheck={false}
            autoComplete="off"
            role="combobox"
            aria-expanded="true"
            aria-controls="quick-list"
            aria-activedescendant={cur >= 0 ? `quick-${cur}` : undefined}
            onChange={(e) => {
              setQ(e.target.value);
              setHot(0);
            }}
            onKeyDown={onKey}
          />
        </label>
        <div ref={list} id="quick-list" className="quick-list" role="listbox" aria-label="빠른 이동 결과">
          {rows.map((r, i) => (
            <div
              key={r.id}
              id={`quick-${i}`}
              role="option"
              aria-selected={i === cur}
              className="quick-row"
              data-kind={r.kind}
              data-hot={i === cur || undefined}
              // the input keeps the focus (typing goes on); the pointer only moves the highlight once it moves
              onMouseDown={(e) => e.preventDefault()}
              onMouseMove={() => i !== cur && setHot(i)}
              onClick={() => run(r)}
            >
              <span className="quick-lead">{r.w ? <AgentGlyph w={r.w} /> : r.icon}</span>
              <span className="quick-label ellipsis">{r.label}</span>
              {r.meta && <span className="quick-meta ellipsis">{r.meta}</span>}
              {r.w && <span className="quick-logo"><AgentLogo id={r.w.adapterId} size={12} /></span>}
              {r.keys && <kbd className="keycap">{r.keys}</kbd>}
            </div>
          ))}
          {!rows.length && <div className="quick-empty">맞는 항목이 없어요</div>}
        </div>
        <div className="quick-foot">Enter 열기 · Esc 닫기 · ↑↓ 이동</div>
      </div>
    </div>
  );
}
