import { Fragment, lazy, Suspense, useEffect, useRef, useState } from "react";
import { ReactFlowProvider } from "@xyflow/react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow, UserAttentionType } from "@tauri-apps/api/window";
import { Canvas } from "./Canvas";
import { ProjectMap } from "./ProjectMap";
import { NewAgentDialog } from "./NewAgentDialog";
import { AgentSettingsDialog } from "./AgentSettingsDialog";
import { ConfirmHost } from "./ui/Confirm";
import { MenuHost } from "./ui/Menu";
import { Toasts } from "./ui/Toasts";
import { useShortcuts } from "./ui/useShortcuts";
import { SettingsDialog } from "./settings/SettingsDialog";
import { ArrowLeft, ArrowRight, Settings } from "./icons";
import { tip } from "./shortcuts";
import { AgentFull } from "./AgentFull";
import { StageTabs } from "./StageTabs";
import { SystemDashboard } from "./LibraryPanel";
import { LogoLoading, LogoMotion, Spinner } from "./LogoMotion";
import { useSettings } from "./settings";
import { leaves } from "./layout";
import { activeTab, fileKey, groupView, navTarget, SYSTEM_ROOT, useAttentionCount, useStore, useTabLayout, useVisibleRoots, type Group, type Layout, type Side, type StageTab } from "./store";
import { AttentionButton } from "./nodes/Attention";
import { ipc } from "./ipc";
import type { AgentRuntimePayload, AgentStatePayload, AgentSwitchPayload, Approval, MergeDonePayload, MergeRequest, PrPollPayload, PullRequest, TuiWait } from "./types";

// Monaco is several MB: loaded on the first file opened, not at startup
const CodeView = lazy(() => import("./CodeView"));

const win = getCurrentWindow();
/**
 * dataTransfer type of a dragged tab (`{ tab, root, from }`) or of a dragged project tab, which carries its whole
 * group (`{ group }`). Ids are all a drop needs: the workspace (and so every window) knows the rest.
 */
export const TAB_MIME = "application/x-maestro-tab";
export type TabDrag = { tab: StageTab; root: string; from: string } | { group: string };

/** Agent state, runtime, approvals and worktree changes from the backend; every window listens to all of them. */
export function useAgentEvents() {
  useEffect(() => {
    const { addApproval, dropApproval, setTuiWait, addMerge, dropMerge, addPr, dropPr, setPrPoll, onAgentState, setRuntime, setSwitchPending, setError } = useStore.getState();
    let gone = false;
    ipc.listApprovals().then((list) => list.forEach(addApproval)).catch(() => {});
    // the agent's terminal waits for an answer there (a card handed to the CLI or out of time, a picker on screen), or
    // no longer. The backend says so only on a change, and a wait that comes back here after it was cleared locks the
    // composer for good: so the list is asked for only once the listener is up, and a path with an event since then
    // keeps that newer news over the list's
    const waitNews = new Set<string>();
    const tuiWaitKey = (p: string) => p.replace(/\\/g, "/").replace(/\/$/, "").toLowerCase();
    const tuiWaits = listen<{ worktreePath: string; wait: TuiWait | null }>("agent://tui-wait", (e) => {
      waitNews.add(tuiWaitKey(e.payload.worktreePath));
      setTuiWait(e.payload.worktreePath, e.payload.wait);
    });
    tuiWaits
      .then(() => ipc.listTuiWaits())
      .then((list) => {
        for (const w of list) if (!gone && !waitNews.has(tuiWaitKey(w.worktreePath))) setTuiWait(w.worktreePath, w);
      })
      .catch(() => {});
    ipc.listMerges().then((list) => list.forEach(addMerge)).catch(() => {});
    ipc.listPrs().then((list) => list.forEach(addPr)).catch(() => {});
    const uns = [
      listen<AgentStatePayload>("agent://state", (e) => onAgentState(e.payload.worktreePath, e.payload.state.kind)),
      // started, switched (chat ↔ CLI) or fell back to the terminal; a switch waiting for the turn to end
      listen<AgentRuntimePayload>("agent://runtime", (e) => setRuntime(e.payload.worktreePath, e.payload.runtime, e.payload.note)),
      listen<AgentSwitchPayload>("agent://switch", (e) => setSwitchPending(e.payload.worktreePath, e.payload.pending)),
      listen<Approval>("agent://approval", (e) => addApproval(e.payload)),
      listen<{ id: number }>("agent://approval-done", (e) => dropApproval(e.payload.id)),
      tuiWaits,
      // a conductor's merge request, and its end (answered, replaced by a newer one, or its worker discarded)
      listen<MergeRequest>("maestro://merge", (e) => addMerge(e.payload)),
      listen<MergeDonePayload>("maestro://merge-done", (e) => dropMerge(e.payload.id)),
      // PR mode: a pull request made, found, polled or merged; dropped (its worker discarded, or 닫기); polling paused
      listen<PullRequest>("maestro://pr", (e) => addPr(e.payload)),
      listen<{ workerPath: string }>("maestro://pr-done", (e) => dropPr(e.payload.workerPath)),
      listen<PrPollPayload>("maestro://pr-poll", (e) => setPrPoll(e.payload)),
      // an agent ran `maestro worktree create`
      listen<string>("maestro://worktrees", (e) => useStore.getState().refreshWorktrees(e.payload).catch(setError)),
    ];
    return () => {
      gone = true;
      uns.forEach((un) => void un.then((f) => f()));
    };
  }, []);
}

/** The projects on screen in this window count as in use (the backend joins every window's). */
export function useAwayReport() {
  const visibleRoots = useVisibleRoots();
  const awayMinutes = useSettings((s) => s.awayMinutes);
  useEffect(() => void ipc.setAway(visibleRoots, awayMinutes).catch(() => {}), [visibleRoots, awayMinutes]);
}

// the windows' own title (tauri.conf.json, workspace.rs): "(N) " goes before it while N things wait
const TITLE = "Maestro";

/**
 * 확인할 일 outside the window (docs/features/ui/attention.md): the title starts with "(N) " while N things wait
 * (set only when N changes; each window its own). `main`: when more things keep an agent waiting than before and the
 * window isn't focused, the taskbar asks for attention once (Informational: a flash, no focus stealing).
 */
export function useWindowAttention(main: boolean) {
  const { all, blocking } = useAttentionCount();
  const shown = useRef(0);
  const before = useRef(blocking);
  useEffect(() => {
    if (all === shown.current) return;
    shown.current = all;
    void win.setTitle(all ? `(${all}) ${TITLE}` : TITLE).catch(() => {});
  }, [all]);
  useEffect(() => {
    const more = blocking > before.current;
    before.current = blocking;
    if (main && more && !document.hasFocus()) void win.requestUserAttention(UserAttentionType.Informational).catch(() => {});
  }, [blocking]);
}

export function WindowControls() {
  return (
    <div className="win-controls">
      <button title="최소화" onClick={() => win.minimize()}>
        <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden><path d="M0 5h10" stroke="currentColor" /></svg>
      </button>
      <button title="최대화" onClick={() => win.toggleMaximize()}>
        <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden><rect x=".5" y=".5" width="9" height="9" fill="none" stroke="currentColor" /></svg>
      </button>
      <button className="win-close" title="닫기" onClick={() => win.close()}>
        <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden><path d="M0 0l10 10M10 0L0 10" stroke="currentColor" /></svg>
      </button>
    </div>
  );
}

/** The logo and a name (the app's, or in a pop-out window its project's). Part of the drag region. */
export function Brand({ name = "Maestro" }: { name?: string }) {
  return (
    <span className="brand" data-tauri-drag-region title={name}>
      <LogoMotion height={16} />
      <span className="brand-name ellipsis" data-tauri-drag-region>{name}</span>
    </span>
  );
}

/** Back / forward through the places visited in this window (store `nav`), for the top bar. */
export function NavButtons() {
  const canBack = useStore((s) => navTarget(s, -1) >= 0);
  const canForward = useStore((s) => navTarget(s, 1) >= 0);
  const goNav = useStore((s) => s.goNav);
  return (
    <>
      <button className="icon tb-icon" title={tip("back")} aria-label={tip("back")} disabled={!canBack} onClick={() => goNav(-1)}>
        <ArrowLeft />
      </button>
      <button className="icon tb-icon" title={tip("forward")} aria-label={tip("forward")} disabled={!canForward} onClick={() => goNav(1)}>
        <ArrowRight />
      </button>
    </>
  );
}

/** The top bar's settings button: opens this window's settings dialog. */
export function SettingsButton() {
  return (
    <button className="icon tb-icon" title={tip("settings")} aria-label={tip("settings")} onClick={() => useStore.getState().openSettings()}>
      <Settings />
    </button>
  );
}

/**
 * The window's own layers, outside any group: the new agent and agent settings dialogs, the settings dialog (after
 * them, so a link from one of them, like "설정에서 설치", opens over it and closing it goes back), menus,
 * confirmations, error toasts.
 */
export function WindowLayers() {
  const settings = useStore((s) => s.settings);
  const closeSettings = useStore((s) => s.closeSettings);
  return (
    <>
      <NewAgentHost />
      <AgentSettingsHost />
      {settings && <SettingsDialog target={settings} onClose={closeSettings} />}
      <MenuHost />
      <ConfirmHost />
      <Toasts />
    </>
  );
}

/** The new agent dialog this window's store asks for (`openNewAgent`), for a project still in the list. */
function NewAgentHost() {
  const t = useStore((s) => s.newAgent);
  const repo = useStore((s) => (s.newAgent ? s.repos.find((r) => r.root === s.newAgent!.root) : undefined));
  const close = useStore((s) => s.closeNewAgent);
  if (!t || !repo) return null;
  return <NewAgentDialog key={`${t.root}|${t.mode}|${t.parentPath ?? ""}`} repo={repo} mode={t.mode} parentPath={t.parentPath} onClose={close} />;
}

/** The agent settings dialog this window's store asks for (`openAgentSettings`). */
function AgentSettingsHost() {
  const t = useStore((s) => s.agentSettings);
  const close = useStore((s) => s.closeAgentSettings);
  if (!t) return null;
  return <AgentSettingsDialog key={`${t.path}|${t.focus ?? ""}`} target={t} onClose={close} />;
}

// ---------- a tab group on screen ----------

/** Which part of a group the pointer is over: within a quarter of an edge splits that way, else the centre. */
function zoneAt(e: React.DragEvent, el: HTMLElement): Side | "center" {
  const r = el.getBoundingClientRect();
  const x = (e.clientX - r.left) / r.width;
  const y = (e.clientY - r.top) / r.height;
  const edges: Array<[Side, number]> = [["left", x], ["right", 1 - x], ["top", y], ["bottom", 1 - y]];
  const [side, d] = edges.reduce((a, b) => (b[1] < a[1] ? b : a));
  return d < 0.25 ? side : "center";
}

/** A tab or a whole group dropped on `target`, from this window or another (the workspace is shared). */
export function dropOn(raw: string, target: string, zone: Side | "center", index?: number) {
  const d = JSON.parse(raw) as TabDrag;
  const s = useStore.getState();
  if ("group" in d) s.moveGroup(d.group, target, zone);
  else s.dropTab(d.tab, d.root, d.from, target, zone, false, index);
}

/**
 * One tab group: its tab bar and what its open tab shows, for its own project. Its own React Flow provider, so
 * two canvases don't share a viewport. Pressing anywhere in it makes it the focused group. A tab (or a project
 * tab, with its whole group) dragged over it shows where it would land: an edge splits that way, the centre
 * joins this group.
 */
function GroupView({ group, focused, inWindow }: { group: Group; focused: boolean; inWindow: boolean }) {
  const root = group.root;
  const scope = useStore((s) => s.worktrees.find((w) => w.path === group.path && w.root === root)?.path);
  const loading = useStore((s) => !s.ready || (!!root && !s.scanned.includes(root)));
  // this tab's own full screen / canvas (the tab bar's toggle); a tab from before it had one: its role's first screen
  const agentLayout = useTabLayout(group);
  const [zone, setZone] = useState<Side | "center" | null>(null);
  const fileTabs = group.tabs.filter((t): t is Extract<StageTab, { kind: "file" }> => t.kind === "file");
  const { focusGroup, pinTab, openNewAgent } = useStore.getState();
  const systemView = useStore((s) => s.systemView);
  // a drag that ended anywhere (dropped elsewhere, Esc) takes the drop preview with it
  useEffect(() => {
    if (!zone) return;
    const off = () => setZone(null);
    window.addEventListener("dragend", off);
    window.addEventListener("drop", off);
    return () => {
      window.removeEventListener("dragend", off);
      window.removeEventListener("drop", off);
    };
  }, [!!zone]);

  return (
    <ReactFlowProvider>
      <section
        className="group"
        data-focused={focused || undefined}
        onMouseDownCapture={() => focusGroup(group.id)}
        // a tab coming in: from here on the drop layer below takes the drag (canvas, terminal and editor would eat it)
        onDragEnterCapture={(e) => e.dataTransfer.types.includes(TAB_MIME) && setZone((z) => z ?? "center")}
        onDragLeave={(e) => !e.currentTarget.contains(e.relatedTarget as Node) && setZone(null)}
      >
        <StageTabs group={group} focused={focused} inWindow={inWindow} />
        {/* a double click anywhere in what a preview tab shows keeps that tab (like double-clicking the tab) */}
        <div
          className="stage-body"
          onDoubleClick={() => {
            const t = activeTab(groupView(group));
            if (t) pinTab(t, group.id);
          }}
        >
          {root === SYSTEM_ROOT ? (
            <SystemDashboard view={systemView} onView={(v) => useStore.setState({ systemView: v })} />
          ) : loading || !root ? (
            <div className="empty">
              {!root && !loading ? <p className="muted">왼쪽에서 프로젝트를 고르세요</p> : <LogoLoading text="저장소와 워크트리를 읽는 중…" />}
            </div>
          ) : scope && agentLayout === "full" ? (
            <AgentFull key={scope} path={scope} />
          ) : scope ? (
            <Canvas
              key={`${root}|${scope}`}
              root={root}
              scope={scope}
              onNewAgent={(o) => openNewAgent(o?.parentPath ? { root, mode: "worker", parentPath: o.parentPath } : { root, mode: "agent" })}
              // this tab only: the settings' first screens stay as they are
              onFull={() => useStore.getState().setTabLayout(group.id, scope, "full")}
            />
          ) : (
            <ProjectMap
              key={root}
              root={root}
              onNewAgent={() => openNewAgent({ root, mode: "agent" })}
              onNewMaestro={() => openNewAgent({ root, mode: "maestro" })}
            />
          )}
          {/* over the stage, so the canvas or agent underneath keeps running and keeps its state;
              every open file stays mounted (hidden when not on screen) so its unsaved edits survive tab switches */}
          {fileTabs.length > 0 && (
            <Suspense fallback={group.editor && <div className="code-view empty"><p className="muted"><Spinner /> 편집기 불러오는 중…</p></div>}>
              {fileTabs.map((t) => (
                <CodeView
                  key={fileKey(t.base, t.rel)}
                  groupId={group.id}
                  base={t.base}
                  rel={t.rel}
                  diff={t.diff}
                  active={!!group.editor && group.editor.base === t.base && group.editor.rel === t.rel}
                />
              ))}
            </Suspense>
          )}
          {zone && (
            <div
              className="drop-layer"
              onDragOver={(e) => {
                e.preventDefault();
                e.dataTransfer.dropEffect = "move";
                setZone(zoneAt(e, e.currentTarget));
              }}
              onDrop={(e) => {
                e.preventDefault();
                const where = zoneAt(e, e.currentTarget);
                setZone(null);
                const raw = e.dataTransfer.getData(TAB_MIME);
                if (raw) dropOn(raw, group.id, where);
              }}
            >
              <div className="drop-preview" data-zone={zone} />
            </div>
          )}
        </div>
      </section>
    </ReactFlowProvider>
  );
}

/** The split layout, drawn as nested rows and columns of groups. */
function LayoutView({ node, focus, inWindow }: { node: Layout; focus: string; inWindow: boolean }) {
  const group = useStore((s) => ("group" in node ? s.groups[node.group] : undefined));
  const box = useRef<HTMLDivElement>(null);
  // while the line between two children is dragged: their shares, live; saved to the workspace on release
  const [live, setLive] = useState<number[] | null>(null);
  if ("group" in node) return group ? <GroupView group={group} focused={node.group === focus} inWindow={inWindow} /> : null;
  const sizes = live ?? node.sizes ?? node.children.map(() => 1);
  const row = node.dir === "row";

  const drag = (i: number) => (e: React.PointerEvent) => {
    if (e.button !== 0 || !box.current) return;
    const el = e.currentTarget as HTMLElement;
    el.setPointerCapture(e.pointerId);
    const total = sizes.reduce((a, b) => a + b, 0);
    const span = row ? box.current.clientWidth : box.current.clientHeight;
    const start = row ? e.clientX : e.clientY;
    const pair = sizes[i] + sizes[i + 1];
    const min = total * 0.08; // a group never shrinks below ~8% of the split
    let next = sizes;
    el.onpointermove = (m) => {
      const d = (((row ? m.clientX : m.clientY) - start) / span) * total;
      const a = Math.min(pair - min, Math.max(min, sizes[i] + d));
      next = sizes.map((v, j) => (j === i ? a : j === i + 1 ? pair - a : v));
      setLive(next);
    };
    el.onpointerup = el.onpointercancel = () => {
      el.onpointermove = el.onpointerup = el.onpointercancel = null;
      useStore.getState().setSplitSizes(leaves(node).join("+"), next);
      setLive(null);
    };
  };

  return (
    <div ref={box} className="split" data-dir={node.dir} data-resizing={live ? "" : undefined}>
      {node.children.map((c, i) => (
        <Fragment key={"group" in c ? c.group : leaves(c).join("+")}>
          {i > 0 && <div className="split-gutter" role="separator" aria-orientation={row ? "vertical" : "horizontal"} title="끌어서 비율 조절 · 더블클릭하면 같게" onPointerDown={drag(i - 1)} onDoubleClick={() => useStore.getState().setSplitSizes(leaves(node).join("+"), node.children.map(() => 1))} />}
          <div className="split-cell" style={{ flex: `${sizes[i]} 1 0` }}>
            <LayoutView node={c} focus={focus} inWindow={inWindow} />
          </div>
        </Fragment>
      ))}
    </div>
  );
}

/** This window's tab groups. Back / forward are in the top bar (NavButtons). */
export function Workspace({ inWindow }: { inWindow: boolean }) {
  const layout = useStore((s) => s.layout);
  const focus = useStore((s) => s.focus);
  if (!layout || !focus) return null;
  return (
    <div className="groups">
      <LayoutView node={layout} focus={focus} inWindow={inWindow} />
    </div>
  );
}

// ---------- pop-out windows ----------

/**
 * A pop-out window. The backend keeps one loaded and hidden; popping a tab out gives it an entry in the shared
 * workspace and shows it, so it's on screen at once. Tab groups only (it splits too), no sidebars. Tabs drag
 * between it and the main window; "메인 창으로" sends all of them back as splits. Once nothing in the workspace
 * belongs to it any more (its last tab left, it was docked) it closes.
 */
export function WorkspaceWindow() {
  const focusedRepo = useStore((s) => s.repos.find((r) => r.root === s.activeRoot));
  const has = useStore((s) => !!s.layout);
  const had = useRef(false);
  useAgentEvents();
  useAwayReport();
  useShortcuts();
  useWindowAttention(false);

  useEffect(() => {
    const s = useStore.getState();
    // ready to be handed a group the moment its data is in
    void s.connect().then(() => s.loadRepos()).then(() => ipc.wsWindowReady()).catch(s.setError);
  }, []);
  useEffect(() => {
    if (has) had.current = true;
    else if (had.current) void win.close();
  }, [has]);

  if (!has) return null;
  return (
    <div className="app ws-window">
      {/* the main window's bar, less the panel toggles (no side panels here) and the update (the main window's) */}
      <header className="topbar">
        <Brand name={focusedRepo?.name ?? "Maestro"} />
        <NavButtons />
        <span className="spacer" data-tauri-drag-region />
        <AttentionButton />
        <SettingsButton />
        <button className="tb-text dock-btn" title="이 창의 탭을 모두 메인 창의 분할 화면으로 돌려보내요" onClick={() => useStore.getState().dockHere()}>
          메인 창으로
        </button>
        <WindowControls />
      </header>
      <div className="shell">
        <main className="stage">
          <Workspace inWindow />
        </main>
      </div>
      <WindowLayers />
    </div>
  );
}
