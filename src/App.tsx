import { useEffect, useState } from "react";
import { RightPanel } from "./RightPanel";
import { addProject, Sidebar } from "./Sidebar";
import { LogoLoading } from "./LogoMotion";
import { useStore } from "./store";
import { useSettings } from "./settings";
import { ipc } from "./ipc";
import { UpdateNotice } from "./UpdateNotice";
import { useShortcuts } from "./ui/useShortcuts";
import { FolderPlus, PanelLeft, PanelRight } from "./icons";
import { NavButtons, SettingsButton, useAgentEvents, useAwayReport, useWindowAttention, WindowControls, WindowLayers, Workspace } from "./Workspace";
import { AttentionButton } from "./nodes/Attention";

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

// Per-viewer side panel width (px), clamped to its range when read and when set (a width saved under an older,
// wider range comes back inside this one); same storage caveats as above.
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
  const [showLeft, setShowLeft] = usePanelPref("maestro.showLeft");
  const [showRight, setShowRight] = usePanelPref("maestro.showRight");
  const [leftW, setLeftW, leftDef] = usePanelWidth("maestro.leftWidth", 280, 220, 480);
  const [rightW, setRightW, rightDef] = usePanelWidth("maestro.rightWidth", 320, 220, 560);
  const [resizing, setResizing] = useState(false);
  // every project on screen (any split, any window) counts as in use, not just the focused one
  useAwayReport();
  useAgentEvents();
  useShortcuts();
  // 확인할 일 outside the window: "(N) " before the title, the taskbar flashing when something new waits
  useWindowAttention(true);
  // until the repo list is read, "add a repo" would be wrong
  const ready = useStore((s) => s.ready);
  // the backend starts agents no view asked for (delegations, resumes, quick prompts) on the app's 에이전트 보기:
  // told at launch and on every change (also one made in a pop-out window, which reaches here through storage)
  const agentView = useSettings((s) => s.agentView);
  useEffect(() => void ipc.setViewDefault(agentView).catch(() => {}), [agentView]);

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
      {/* replaces the native title bar (decorations: false). No logo or name here, only controls; the blank middle
          drags the window (drag regions don't inherit, and on a button they would eat its click); a double click there
          maximizes */}
      <header className="topbar">
        <button className="icon tb-icon" data-on={showLeft || undefined} title="왼쪽 패널" aria-label="왼쪽 패널" aria-pressed={showLeft} onClick={() => setShowLeft(!showLeft)}>
          <PanelLeft />
        </button>
        <NavButtons />
        <span className="spacer" data-tauri-drag-region />
        <AttentionButton />
        <UpdateNotice />
        <SettingsButton />
        <button className="icon tb-icon" data-on={showRight || undefined} title="오른쪽 패널" aria-label="오른쪽 패널" aria-pressed={showRight} onClick={() => setShowRight(!showRight)}>
          <PanelRight />
        </button>
        <WindowControls />
      </header>
      <div className="shell">
        <Sidebar />
        <Resizer side="left" width={leftW} set={setLeftW} def={leftDef} onDrag={setResizing} />
        <main className="stage">
          {!ready ? (
            <LogoLoading text="프로젝트와 에이전트를 읽는 중…" />
          ) : repos.length ? (
            // per tab group: nothing selected → the project map; an agent (the Maestro too) → its full view or its canvas
            <Workspace inWindow={false} />
          ) : (
            <div className="empty stage-empty">
              <p>프로젝트 폴더를 추가하면 에이전트마다 따로 된 작업 폴더에서 일해요</p>
              <p className="muted small">작업 폴더(워크트리)는 ~/.maestro/projects/ 아래에 만들어지고, 원본 폴더에는 파일이 생기지 않아요</p>
              <button className="primary" onClick={() => void addProject()}>
                <FolderPlus size={14} />
                프로젝트 추가
              </button>
            </div>
          )}
        </main>
        <Resizer side="right" width={rightW} set={setRightW} def={rightDef} onDrag={setResizing} />
        <RightPanel visible={showRight} />
      </div>
      <WindowLayers panels={{ left: showLeft, right: showRight, setLeft: setShowLeft, setRight: setShowRight }} />
    </div>
  );
}
