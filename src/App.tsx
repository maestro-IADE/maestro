import { useEffect, useState } from "react";
import { NewAgentDialog } from "./NewAgentDialog";
import { RightPanel } from "./RightPanel";
import { Sidebar } from "./Sidebar";
import { LogoLoading } from "./LogoMotion";
import { useStore } from "./store";
import { UpdateNotice } from "./UpdateNotice";
import { useShortcuts } from "./ui/useShortcuts";
import { PanelLeft, PanelRight } from "./icons";
import { Brand, NavButtons, SettingsButton, useAgentEvents, useAwayReport, WindowControls, WindowLayers, Workspace } from "./Workspace";
import type { Repo } from "./types";

// Per-viewer panel visibility; storage may be unavailable, so default to shown.
function usePanelPref(key: string) {
  const [on, setOn] = useState(() => {
    try {
      return localStorage.getItem(key) !== "0";
    } catch {
      return true;
    }
  });
  const set = (v: boolean) => {
    setOn(v);
    try {
      localStorage.setItem(key, v ? "1" : "0");
    } catch {}
  };
  return [on, set] as const;
}

// Per-viewer side panel width (px), clamped; same storage caveats as above.
function usePanelWidth(key: string, def: number, min: number, max: number) {
  const clamp = (v: number) => Math.round(Math.min(max, Math.max(min, v)));
  const [w, setW] = useState(() => {
    try {
      const v = Number(localStorage.getItem(key));
      return v ? clamp(v) : def;
    } catch {
      return def;
    }
  });
  const set = (v: number) => {
    setW(clamp(v));
    try {
      localStorage.setItem(key, String(clamp(v)));
    } catch {}
  };
  return [w, set, def] as const;
}

/** Drag handle between a side panel and the stage; double-click restores the default width. */
function Resizer({ side, width, set, def, onDrag }: { side: "left" | "right"; width: number; set: (v: number) => void; def: number; onDrag: (on: boolean) => void }) {
  const dir = side === "left" ? 1 : -1;
  return (
    <div
      className="resizer"
      data-side={side}
      role="separator"
      aria-orientation="vertical"
      aria-valuenow={width}
      title="드래그해서 너비 조절 · 더블클릭하면 기본값"
      onDoubleClick={() => set(def)}
      onPointerDown={(e) => {
        if (e.button !== 0) return;
        const el = e.currentTarget;
        const x0 = e.clientX;
        el.setPointerCapture(e.pointerId);
        onDrag(true);
        el.onpointermove = (m) => set(width + dir * (m.clientX - x0));
        el.onpointerup = el.onpointercancel = () => {
          el.onpointermove = el.onpointerup = el.onpointercancel = null;
          onDrag(false);
        };
      }}
    />
  );
}

// Orca-style shell: repos/agents sidebar | agent canvas | files & changes panel.
export default function App() {
  const { repos, loadRepos, setError } = useStore();
  // new worktree / worker / Maestro dialog
  const [dialog, setDialog] = useState<{ repo: Repo; maestro?: boolean; parentPath?: string } | null>(null);
  const setDialogRepo = (repo: Repo | null) => setDialog(repo && { repo });
  const [showLeft, setShowLeft] = usePanelPref("maestro.showLeft");
  const [showRight, setShowRight] = usePanelPref("maestro.showRight");
  const [leftW, setLeftW, leftDef] = usePanelWidth("maestro.leftWidth", 260, 180, 480);
  const [rightW, setRightW, rightDef] = usePanelWidth("maestro.rightWidth", 300, 220, 560);
  const [resizing, setResizing] = useState(false);
  // every project on screen (any split, any window) counts as in use, not just the focused one
  useAwayReport();
  useAgentEvents();
  useShortcuts();
  // until the repo list is read, "add a repo" would be wrong
  const ready = useStore((s) => s.ready);

  useEffect(() => {
    // the shared workspace first: it decides which group shows what
    useStore.getState().connect().then(loadRepos).catch(setError);
    // no agent CLI at all → nothing works yet; open settings, where they can be installed
    useStore.getState().loadCli().then((c) => !c.agents.some((a) => a.installed) && useStore.getState().openSettings({ section: "agents" })).catch(() => {});
  }, []);

  return (
    <div
      className="app"
      data-hide-left={!showLeft || undefined}
      data-hide-right={!showRight || undefined}
      data-resizing={resizing || undefined}
      style={{ "--mae-sidebar-w": `${leftW}px`, "--mae-right-w": `${rightW}px` } as React.CSSProperties}
    >
      {/* replaces the native title bar (decorations: false). Only the brand and the blank middle drag the window
          (drag regions don't inherit, and on a button they would eat its click); a double click there maximizes */}
      <header className="topbar">
        <Brand />
        <button className="icon tb-icon" data-on={showLeft || undefined} title="왼쪽 패널" aria-label="왼쪽 패널" aria-pressed={showLeft} onClick={() => setShowLeft(!showLeft)}>
          <PanelLeft />
        </button>
        <NavButtons />
        <span className="spacer" data-tauri-drag-region />
        {/* 확인할 일 (attention): filled in a later step */}
        <span className="tb-attention" />
        <UpdateNotice />
        <SettingsButton />
        <button className="icon tb-icon" data-on={showRight || undefined} title="오른쪽 패널" aria-label="오른쪽 패널" aria-pressed={showRight} onClick={() => setShowRight(!showRight)}>
          <PanelRight />
        </button>
        <WindowControls />
      </header>
      <div className="shell">
        <Sidebar onNewWorktree={setDialogRepo} />
        <Resizer side="left" width={leftW} set={setLeftW} def={leftDef} onDrag={setResizing} />
        <main className="stage">
          {!ready ? (
            <LogoLoading text="저장소와 워크트리를 읽는 중…" />
          ) : repos.length ? (
            // per tab group: nothing selected → the project map; an agent (the Maestro too) → its full view or its canvas
            <Workspace
              inWindow={false}
              dialogs={{
                // for the group pressed (it took the focus on mousedown)
                onNewAgent: (o) => {
                  const repo = repos.find((r) => r.root === useStore.getState().activeRoot);
                  if (repo) setDialog({ repo, ...o });
                },
              }}
            />
          ) : (
            <div className="empty">
              <p>왼쪽에서 Git 저장소를 추가하세요</p>
              <p className="muted">에이전트마다 ~/.maestro/projects/ 아래에 격리된 워크트리가 만들어지고, 원본 폴더에는 파일이 생기지 않아요.</p>
            </div>
          )}
        </main>
        <Resizer side="right" width={rightW} set={setRightW} def={rightDef} onDrag={setResizing} />
        <RightPanel />
      </div>
      {dialog && <NewAgentDialog {...dialog} onClose={() => setDialog(null)} />}
      <WindowLayers />
    </div>
  );
}
