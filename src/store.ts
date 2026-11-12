import { create } from "zustand";
import { useShallow } from "zustand/react/shallow";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { ipc } from "./ipc";
import { useSettings } from "./settings";
import { insertAt, leaves, withSizes, without, type Layout, type Side } from "./layout";
import { withToast, type Toast } from "./ui/toastList";
import { attentionItems, blockingCount, samePath, waitsAt, type AttentionItem } from "./attention";
import type { AgentRuntime, AgentStatus, AgentView, Approval, CliStatus, FallbackReason, MergeMode, MergeRequest, PrPollPayload, PullRequest, Repo, TuiWait, WorktreeInfo } from "./types";

export type Worktree = {
  root: string;
  path: string;
  branch: string;
  /** user-given name, falling back to the branch */
  name: string;
  description?: string;
  adapterId?: string;
  /** the permission mode it runs with (agents.tsx `PERMISSION_NAMES`); null without an agent */
  permissionMode: string | null;
  /** the model it starts with (`--model`); undefined = the CLI's own default */
  model?: string;
  isMaestro: boolean;
  workers: string[];
  refs: string[];
  external: boolean;
  personas: string[];
  baseBranch: string | null;
  /** Maestro only: the project's merge mode (PR mode, R6) */
  mergeMode: MergeMode | null;
  docs: string[];
  status: AgentStatus;
  unread: boolean; // finished a turn while not selected (Orca-style attention dot)
  /** what runs its conversation now (null: its agent is off) and why the chat runs on the terminal */
  runtime: AgentRuntime | null;
  runtimeNote: FallbackReason | null;
  /** a chat ↔ CLI switch waiting for the turn to end ("agent://switch"), to this view */
  switchPending: AgentView | null;
};

/**
 * A tab above the stage (besides the project's own, always first): a worktree went into, or a file opened.
 * `preview` (VS Code's preview tab): opened by a single click, replaced by the next one; editing, sending,
 * double-clicking it or what it shows makes it stay.
 */
export type SystemView = "docs" | "playbooks";
/**
 * The "project" a group shows Maestro's own documents and playbooks under: it opens like any project (its own
 * project tab, the group's other tabs parked until you come back) and files opened from it become its tabs.
 */
export const SYSTEM_ROOT = "maestro:system";
export type StageTab = ({ kind: "wt"; path: string; layout?: "full" | "canvas" } | { kind: "file"; base: string; rel: string; diff: boolean }) & { preview?: boolean };
type Editor = { base: string; rel: string; diff: boolean };
/**
 * A tab group: one project (`root`), its tabs, and what it shows (`path`: the agent under the stage, null = the
 * project; `editor`: the file over it). Groups sit in a window's split layout; layouts are not a project's.
 */
export type Group = { id: string; root: string | null; tabs: StageTab[]; path: string | null; editor: Editor | null };
/** A project's tabs while no group shows it, so going back to it brings them back. */
type Parked = Pick<Group, "tabs" | "path" | "editor">;
type Win = { layout: Layout; focus: string };
/**
 * The workspace of every window, kept by the backend (workspace.rs) and shown by each window: windows (their
 * layout and focused group), the groups, and the projects' remembered tabs. A tab moving between windows is one
 * change to it.
 */
export type WS = { windows: Record<string, Win>; groups: Record<string, Group>; parked: Record<string, Parked> };
/** A place in the tab history (back / forward): the project, the agent under the stage, the file over it. */
type Nav = { root: string | null; path: string | null; editor: Editor | null };
export type { Layout, Side };
/**
 * Where the settings dialog opens (settings/SettingsDialog.tsx): a section (general / agents / projects / look /
 * about; an unknown id opens general), and an agent's row (its catalog id, opened at 관리) or a project's card (its
 * root) to scroll to and outline for a moment. Opening again while it's open goes to the new place.
 */
export type SettingsTarget = { section?: string; agent?: string; root?: string };
/**
 * The new agent dialog of this window: for which project, and what it makes (the entry point decides): an agent, a
 * worker of `parentPath`, or the project's Maestro.
 */
export type NewAgentTarget = { root: string; mode: "agent" | "worker" | "maestro"; parentPath?: string };
/**
 * The agent settings dialog of this window (AgentSettingsDialog.tsx): for which agent, and which field takes the
 * focus first (F2: the name; a header chip: its own field).
 */
export type AgentSettingsTarget = { path: string; focus?: "name" | "permission" | "model" };
/** a folder as a key: the same whatever the case or slashes */
const pathKey = (p: string) => p.replace(/\\/g, "/").replace(/\/+$/, "").toLowerCase();
let toastSeq = 0;
export const fileKey = (base: string, rel: string) => `${base}|${rel}`;
export const newGroupId = () => Math.random().toString(36).slice(2, 10);

/** This webview is a pop-out window (`?ws=1`, from the backend's pool), not the main window. */
export const IS_WINDOW = new URLSearchParams(location.search).has("ws");
/** This window's name in the workspace */
export const ME = IS_WINDOW ? getCurrentWindow().label : "main";

type Store = {
  /** the repo list has been read once (before that, "no repos" would be a lie) */
  ready: boolean;
  /** repos whose worktrees have been read at least once (or failed to read) */
  scanned: string[];
  repos: Repo[];
  worktrees: Worktree[];
  /** shows `e` as an error toast (top centre, ui/Toasts.tsx); an empty value closes every toast */
  setError: (e: unknown) => void;
  /** the error toasts on screen in this window, oldest first */
  toasts: Toast[];
  pushToast: (text: string) => void;
  dropToast: (id: number) => void;
  /** the settings dialog of this window: open (at that section) or not */
  settings: SettingsTarget | null;
  openSettings: (target?: SettingsTarget) => void;
  closeSettings: () => void;
  /** the new agent dialog of this window: open (for that project and mode) or not */
  newAgent: NewAgentTarget | null;
  openNewAgent: (target: NewAgentTarget) => void;
  closeNewAgent: () => void;
  /** the agent settings dialog of this window: open (for that agent) or not */
  agentSettings: AgentSettingsTarget | null;
  openAgentSettings: (target: AgentSettingsTarget) => void;
  closeAgentSettings: () => void;
  /**
   * The new agent dialog's 할 일, per agent (by `pathKey`), waiting for that agent's chat composer to open: put in its
   * box, never sent (docs/features/agents/chat-view.md "첫 글"). Taken once (`takeSeed`).
   */
  composerSeed: Record<string, string>;
  seedComposer: (path: string, text: string) => void;
  /** the seed waiting for this agent's composer, if any (read only; `dropSeed` once it's in the box) */
  peekSeed: (path: string) => string | undefined;
  dropSeed: (path: string) => void;
  loadRepos: () => Promise<void>;
  addRepo: (path: string) => Promise<void>;
  removeRepo: (root: string) => Promise<void>;
  refreshWorktrees: (root: string) => Promise<void>;
  upsert: (root: string, info: WorktreeInfo) => void;
  onAgentState: (path: string, status: AgentStatus) => void;
  /** "agent://runtime", `agent_ready`, a switch's result: what runs it now (ends a pending switch) */
  setRuntime: (path: string, runtime: AgentRuntime, note: FallbackReason | null) => void;
  /** "agent://switch" (a view), or the wait over (null) */
  setSwitchPending: (path: string, view: AgentView | null) => void;
  remove: (path: string) => void;
  /** marks an agent read or unread by hand (the sidebar's status lane and row menu); opening it reads it too */
  markUnread: (path: string, unread: boolean) => void;
  cli: CliStatus | null; // which agent CLIs are installed
  loadCli: (recheck?: boolean) => Promise<CliStatus>; // recheck: "다시 확인" (lifts a blocked chat runtime)

  /** the shared workspace as this window shows it (its own pending changes applied) */
  ws: WS;
  /** joins the backend's workspace; the main window creates its own entry the first time */
  connect: () => Promise<void>;
  /** this window's layout and focused group (null until it's been given one: a warm pop-out window) */
  layout: Layout | null;
  focus: string | null;
  groups: Record<string, Group>;
  /** the focused group's project, worktree and file (mirrored, read-only: change them through the actions) */
  activeRoot: string | null;
  selectedPath: string | null;
  editor: Editor | null;
  /** a project, on its own tab: in the group already showing it, else in the focused group */
  setActive: (root: string) => void;
  /** a project in a split of its own beside the focused group (or the group already showing it) */
  openProject: (root: string) => void;
  /** goes to a worktree (null = the project) in the group of its project; pin = a lasting tab, not the preview */
  select: (path: string | null, pin?: boolean) => void;
  /** a file over the stage (in the group of its project); null takes it off the stage, its tab stays */
  openEditor: (e: Editor | null, pin?: boolean) => void;
  /** the shares of a split's children in this window (the split named by its groups, joined by "+") */
  setSplitSizes: (key: string, sizes: number[]) => void;
  /** full screen or canvas for one worktree tab */
  setTabLayout: (groupId: string, path: string, layout: "full" | "canvas") => void;
  focusGroup: (id: string) => void;
  /** a preview tab becomes a lasting one (in the focused group, or `groupId`) */
  pinTab: (tab: StageTab, groupId?: string) => void;
  /**
   * Closes a tab (of the focused group, or `groupId`); closing the one on screen moves to its neighbour (or the
   * project). A group left without tabs closes too, unless it's the main window's last one.
   */
  closeTab: (tab: StageTab, groupId?: string) => void;
  /**
   * Closes every tab of group `groupId` but `keep` (null: all of them, the project shows) and shows `keep`. The
   * group itself stays, even with no tab left. Unsaved edits of the closed files are dropped (ask first).
   */
  closeOthers: (groupId: string, keep: StageTab | null) => void;
  /**
   * A tab dropped on group `target` (in any window): in the centre it joins that group (if it's the same
   * project), on an edge a new group splits off that way. `from` (in any window) loses it unless `copy`.
   */
  dropTab: (tab: StageTab, root: string, from: string, target: string, zone: Side | "center", copy?: boolean, index?: number) => void;
  /** a whole group dropped on `target` (its project tab dragged): merged in the centre, else moved beside it */
  moveGroup: (id: string, target: string, zone: Side | "center") => void;
  /** a new group beside `target` showing the project's own tab */
  splitEmpty: (target: string, side: Side) => void;
  /** the project's own tab closed: the group's tabs go with it, and the group (the last one in main remembers them) */
  closeProject: (groupId: string) => void;
  /** a tab, or a whole group, into a window of its own (opened at `at`, screen px, if given) */
  popOutTab: (tab: StageTab, from: string, at?: { x: number; y: number }) => Promise<void>;
  popOutGroup: (id: string, at?: { x: number; y: number }) => Promise<void>;
  /** this pop-out window's groups back into the main window, beside its focused group */
  dockHere: () => void;
  /** places visited in this window, for the tab bar's back / forward */
  nav: { list: Nav[]; i: number };
  goNav: (dir: -1 | 1) => void;
  /** files with unsaved edits (fileKey), so their tab shows ● and asks before closing */
  dirtyFiles: string[];
  setFileDirty: (key: string, dirty: boolean) => void;
  /** which board the system project shows (documents or playbooks) */
  systemView: SystemView;
  /** the system project on that board, opened like a project (setActive) */
  openSystem: (view: SystemView) => void;
  approvals: Approval[]; // permission prompts waiting for an answer, from the agents' hooks
  addApproval: (a: Approval) => void;
  dropApproval: (id: number) => void;
  /** agents whose terminal waits for an answer only the CLI view can give; the composer is locked meanwhile */
  tuiWaits: TuiWait[];
  /** sets (or, null, clears) the wait of the agent in `path` */
  setTuiWait: (path: string, w: TuiWait | null) => void;
  /** merge requests waiting for the user, shown on the asking Maestro's blocks (not tool approvals) */
  merges: MergeRequest[];
  /** adds or updates one; a newer request of the same worker replaces the older */
  addMerge: (m: MergeRequest) => void;
  dropMerge: (id: number) => void;
  /** PR mode: the workers' pull requests (one per worker) */
  prs: PullRequest[];
  addPr: (p: PullRequest) => void;
  dropPr: (workerPath: string) => void;
  /** PR status polling paused per host (rate limit, login); a pausedUntil of 0 lifts it */
  prPoll: Record<string, PrPollPayload>;
  /** the PR card notice the user hid with ×, per worker (lowercased path): stays hidden while the card remounts */
  seenNotices: Record<string, string>;
  seeNotice: (workerPath: string, notice: string) => void;
  setPrPoll: (p: PrPollPayload) => void;
};

const fromInfo = (root: string, w: WorktreeInfo, prev?: Worktree): Worktree => ({
  root,
  path: w.path,
  branch: w.branch,
  name: w.name || w.branch,
  description: w.description ?? undefined,
  adapterId: w.adapterId ?? undefined,
  permissionMode: w.permissionMode ?? null,
  model: w.model ?? undefined,
  isMaestro: w.isMaestro,
  workers: w.workers,
  refs: w.refs,
  external: w.external,
  personas: w.personas,
  baseBranch: w.baseBranch,
  mergeMode: w.mergeMode ?? null,
  docs: w.docs,
  status: w.state.kind,
  unread: prev?.unread ?? false,
  runtime: w.runtime ?? null,
  runtimeNote: w.runtimeNote ?? null,
  switchPending: prev?.switchPending ?? null,
});

const same = (a: string, b: string | null) => !!b && a.toLowerCase() === b.toLowerCase();

const sameTab = (a: StageTab, b: StageTab) =>
  a.kind === "wt" ? b.kind === "wt" && same(a.path, b.path) : b.kind === "file" && same(a.base, b.base) && a.rel === b.rel;

// ---------- tabs within a group ----------

/**
 * A group's tabs after opening `tab`. Already open: it stays where it is (a file's edit/diff mode is updated),
 * and stays a preview only if this open is one too. New: pinned at the end, or as the preview, which takes the
 * old preview's place.
 */
const withTab = (list: StageTab[], tab: StageTab, pin: boolean): StageTab[] => {
  const i = list.findIndex((t) => sameTab(t, tab));
  // (a worktree tab keeps its own full screen / canvas choice)
  if (i >= 0) return list.map((t, j) => (j === i ? { ...tab, ...(t.kind === "wt" && t.layout ? { layout: t.layout } : {}), preview: !!t.preview && !pin } : t));
  const next = { ...tab, preview: !pin };
  const p = pin ? -1 : list.findIndex((t) => t.preview);
  return p < 0 ? [...list, next] : list.map((t, j) => (j === p ? next : t));
};

/** What a group shows once `tab` opens in it. */
const showing = (g: Group, tab: StageTab): Group =>
  tab.kind === "file" ? { ...g, editor: { base: tab.base, rel: tab.rel, diff: tab.diff } } : { ...g, path: tab.path, editor: null };

/** `g` without `tab`; if it was on screen, its neighbour (or the project) is shown instead. */
const withoutTab = (g: Group, tab: StageTab): Group => {
  const i = g.tabs.findIndex((t) => sameTab(t, tab));
  if (i < 0) return g;
  const tabs = g.tabs.filter((_, j) => j !== i);
  if (!isActiveTab(tab, groupView(g))) return { ...g, tabs };
  const next = tabs[i - 1] ?? tabs[i];
  return next ? showing({ ...g, tabs }, next) : { ...g, tabs, path: null, editor: null };
};

/** The tab on screen: the open file's, else the selected worktree's; null = the project's own. */
export const activeTab = (s: { editor: Editor | null; selectedPath: string | null }): StageTab | null =>
  s.editor ? { kind: "file", ...s.editor } : s.selectedPath ? { kind: "wt", path: s.selectedPath } : null;
export const isActiveTab = (t: StageTab, s: { editor: Editor | null; selectedPath: string | null }) => {
  const a = activeTab(s);
  return !!a && sameTab(t, a);
};
export const groupView = (g: Group) => ({ editor: g.editor, selectedPath: g.path });

/**
 * The tabs a group's tab bar shows, in order: its project's own (null), when it has a project, then each tab whose
 * worktree is still there (Ctrl+1…9 counts these).
 */
export const barTabs = (g: Group, s: Pick<Store, "repos" | "worktrees">): Array<StageTab | null> => [
  ...(g.root && (g.root === SYSTEM_ROOT || s.repos.some((r) => r.root === g.root)) ? [null] : []),
  ...g.tabs.filter((t) => t.kind !== "wt" || s.worktrees.some((w) => same(w.path, t.path))),
];

/** Goes to a tab of the focused group (null: its project's own), keeping it as lasting or preview as it is. */
export function openTab(t: StageTab | null) {
  const s = useStore.getState();
  if (t?.kind === "file") return s.openEditor({ base: t.base, rel: t.rel, diff: t.diff }, !t.preview);
  s.select(t?.path ?? null, !t?.preview);
}

/** The first screen of a new agent tab, by its role (settings `maestroLayout` / `agentLayout`). */
export const roleLayout = (isMaestro: boolean) => {
  const p = useSettings.getState();
  return isMaestro ? p.maestroLayout : p.agentLayout;
};

// ---------- the workspace (pure: re-run as is when the backend refuses a commit) ----------

const emptyGroup = (id: string, root: string | null = null): Group => ({ id, root, tabs: [], path: null, editor: null });
const putGroup = (ws: WS, g: Group): WS => ({ ...ws, groups: { ...ws.groups, [g.id]: g } });
const putWin = (ws: WS, label: string, w: Win): WS => ({ ...ws, windows: { ...ws.windows, [label]: w } });
/** the window whose layout holds group `id` */
const winOf = (ws: WS, id: string) => Object.keys(ws.windows).find((l) => leaves(ws.windows[l].layout).includes(id));
const focusIn = (ws: WS, id: string): WS => {
  const l = winOf(ws, id);
  return l ? putWin(ws, l, { ...ws.windows[l], focus: id }) : ws;
};
const mine = (ws: WS) => ws.windows[ME] as Win | undefined;
const focused = (ws: WS) => {
  const w = mine(ws);
  return w ? ws.groups[w.focus] : undefined;
};
const onFocused = (ws: WS, fn: (g: Group) => Group): WS => {
  const f = focused(ws);
  return f ? putGroup(ws, fn(f)) : ws;
};

// the tabs of the last project tab closed while it was main's only group (closeProject)
const LAST_CLOSED_KEY = "maestro.lastClosedProject";
const lastClosed = (): Record<string, Parked> => {
  try {
    const v = JSON.parse(localStorage.getItem(LAST_CLOSED_KEY) ?? "null") as (Parked & { root: string }) | null;
    return v?.root ? { [v.root]: { tabs: v.tabs ?? [], path: v.path ?? null, editor: v.editor ?? null } } : {};
  } catch {
    return {};
  }
};
/** `parked` without a project's remembered tabs, which are returned (and leave storage, if it was the last closed). */
function unpark(parked: Record<string, Parked>, root: string): [Record<string, Parked>, Parked] {
  const rest = { ...parked };
  const back = rest[root] ?? { tabs: [], path: null, editor: null };
  delete rest[root];
  try {
    if (JSON.parse(localStorage.getItem(LAST_CLOSED_KEY) ?? "null")?.root === root) localStorage.removeItem(LAST_CLOSED_KEY);
  } catch {}
  return [rest, back];
}

/**
 * The focus on a group of this window showing `root`: the focused one if it does, else another that does, else
 * the focused group switches to that project (its own tabs are parked until it comes back).
 */
function routeTo(ws: WS, root: string | null): WS {
  const w = mine(ws);
  const f = focused(ws);
  if (!w || !f || !root || f.root === root) return ws;
  const other = leaves(w.layout).find((id) => ws.groups[id].root === root);
  if (other) return putWin(ws, ME, { ...w, focus: other });
  const away = f.root ? { ...ws.parked, [f.root]: { tabs: f.tabs, path: f.path, editor: f.editor } } : ws.parked;
  const [parked, back] = unpark(away, root);
  return { ...putGroup(ws, { ...f, root, ...back }), parked };
}

/**
 * Group `id` taken out of its window's layout (kept in `groups`): the focus moves to its neighbour; a pop-out
 * window left with nothing goes, the main window gets a fresh empty group (`spare`).
 */
function detach(ws: WS, id: string, spare: string): WS {
  const label = winOf(ws, id);
  if (!label) return ws;
  const w = ws.windows[label];
  const order = leaves(w.layout);
  const layout = without(w.layout, id);
  if (!layout) {
    if (label === "main") return putWin(putGroup(ws, emptyGroup(spare)), label, { layout: { group: spare }, focus: spare });
    const windows = { ...ws.windows };
    delete windows[label];
    return { ...ws, windows };
  }
  const i = order.indexOf(id);
  return putWin(ws, label, { layout, focus: w.focus === id ? (order[i - 1] ?? order[i + 1]) : w.focus });
}
function removeGroup(ws: WS, id: string, spare: string): WS {
  const next = detach(ws, id, spare);
  const groups = { ...next.groups };
  delete groups[id];
  return { ...next, groups };
}
/** A group with no tabs left closes, unless it's the main window's last one (that one shows its project). */
const closesWhenEmpty = (ws: WS, id: string) => {
  const l = winOf(ws, id);
  return !!l && (l !== "main" || leaves(ws.windows[l].layout).length > 1);
};

/** Group `from` without `tab` (moved or copied elsewhere); emptied, it closes. */
function leaveTab(ws: WS, from: string, tab: StageTab, spare: string): WS {
  const g = ws.groups[from];
  if (!g) return ws;
  const left = withoutTab(g, tab);
  return !left.tabs.length && closesWhenEmpty(ws, from) ? removeGroup(ws, from, spare) : putGroup(ws, left);
}

/** `tab` dropped on `target`: joins it in the centre (same project), else a new group `id` beside it. */
/** `list` with `tab` (already in it or not) placed before position `index` of the list as it was. */
const placeAt = (list: StageTab[], tab: StageTab, index: number): StageTab[] => {
  const i = list.findIndex((t) => sameTab(t, tab));
  const rest = list.filter((t) => !sameTab(t, tab));
  const at = i >= 0 && i < index ? index - 1 : index;
  return [...rest.slice(0, at), { ...(i >= 0 ? list[i] : tab), preview: false }, ...rest.slice(at)];
};

function dropTabWS(ws: WS, tab: StageTab, root: string, from: string, target: string, zone: Side | "center", copy: boolean, id: string, spare: string, index?: number): WS {
  const t = ws.groups[target];
  if (!t) return ws;
  // dragged along its own tab bar: just a new place in the row
  if (from === target && zone === "center") return index == null ? ws : putGroup(ws, { ...t, tabs: placeAt(t.tabs, tab, index) });
  let next: WS;
  if (zone === "center" && t.root === root) {
    const tabs = index == null ? withTab(t.tabs, tab, true) : placeAt(t.tabs, tab, index);
    next = focusIn(putGroup(ws, showing({ ...t, tabs }, tab)), target);
  } else {
    // another project's group can't hold this tab: it splits off beside it instead
    const l = winOf(ws, target)!;
    next = putGroup(ws, showing({ ...emptyGroup(id, root), tabs: [{ ...tab, preview: false }] }, tab));
    next = putWin(next, l, { layout: insertAt(next.windows[l].layout, target, id, zone === "center" ? "right" : zone), focus: id });
  }
  return copy ? next : leaveTab(next, from, tab, spare);
}

/** Group `id` dropped on `target`: merged into it in the centre (same project), else moved beside it. */
function moveGroupWS(ws: WS, id: string, target: string, zone: Side | "center", spare: string): WS {
  const g = ws.groups[id];
  const t = ws.groups[target];
  if (!g || !t || id === target) return ws;
  if (zone === "center" && g.root === t.root) {
    const merged = { ...t, tabs: g.tabs.reduce((acc, x) => withTab(acc, x, !x.preview), t.tabs), path: g.path ?? t.path, editor: g.editor };
    return focusIn(removeGroup(putGroup(ws, merged), id, spare), target);
  }
  const next = detach(ws, id, spare);
  const l = winOf(next, target);
  if (!l) return ws;
  return putWin(next, l, { layout: insertAt(next.windows[l].layout, target, id, zone === "center" ? "right" : zone), focus: id });
}

// ---------- syncing with the backend ----------

/**
 * This window's view: the backend's last confirmed workspace with its own pending changes on top. Changes are
 * committed one at a time against the confirmed version; refused (another window committed first), the same
 * change is redone on the newer state. Changes are pure functions with their ids made up front, so redoing one
 * gives the same result.
 */
let confirmed: WS = { windows: {}, groups: {}, parked: {} };
let confirmedV = -1;
const queue: Array<(ws: WS) => WS> = [];
let inflight = false;
let latest: { state: WS; version: number } | null = null;

const view = (ws: WS) => {
  const w = ws.windows[ME];
  const f = w ? ws.groups[w.focus] : undefined;
  return { ws, layout: w?.layout ?? null, focus: w?.focus ?? null, groups: ws.groups, activeRoot: f?.root ?? null, selectedPath: f?.path ?? null, editor: f?.editor ?? null };
};
const render = () => useStore.setState(view(queue.reduce((w, f) => f(w), confirmed)));

async function flush() {
  if (inflight) return;
  inflight = true;
  try {
    while (queue.length) {
      const next = queue[0](confirmed);
      if (next === confirmed) {
        queue.shift();
        continue;
      }
      const r = await ipc.wsCommit(next, confirmedV);
      if (r.ok) {
        confirmed = next;
        confirmedV = r.version;
        queue.shift();
      } else {
        confirmed = r.state!;
        confirmedV = r.version;
      }
      // a broadcast that came in meanwhile and is newer still (another window's) wins
      if (latest && latest.version > confirmedV) ({ state: confirmed, version: confirmedV } = latest);
      latest = null;
      render();
    }
  } catch (e) {
    queue.length = 0;
    render();
    useStore.getState().setError(e);
  } finally {
    inflight = false;
  }
}
const apply = (fn: (ws: WS) => WS) => {
  queue.push(fn);
  render();
  void flush();
};

// where you were (project, agent), so a reload (Ctrl+R, HMR) comes back to it; storage may be unavailable
const LOCATION_KEY = "maestro.location";
const savedLocation = (): { root?: string; path?: string | null } => {
  try {
    return JSON.parse(localStorage.getItem(LOCATION_KEY) ?? "{}");
  } catch {
    return {};
  }
};

const navKey = (n: Nav) => `${n.root}|${n.path}|${n.editor ? fileKey(n.editor.base, n.editor.rel) : ""}`;
/** A history entry can be gone back to while its project, worktree and file's worktree still exist (a closed tab reopens). */
const navValid = (n: Nav, s: Pick<Store, "repos" | "worktrees">) => {
  const known = (p: string) => same(p, n.root) || s.worktrees.some((w) => same(w.path, p));
  return s.repos.some((r) => r.root === n.root) && (!n.path || known(n.path)) && (!n.editor || known(n.editor.base));
};
/** The nearest entry back (-1) or forward (1) that can still be gone to, or -1. */
export const navTarget = (s: Pick<Store, "repos" | "worktrees" | "nav">, dir: -1 | 1) => {
  for (let j = s.nav.i + dir; j >= 0 && j < s.nav.list.length; j += dir) if (navValid(s.nav.list[j], s)) return j;
  return -1;
};

/** The project a worktree or file folder belongs to (a project's own folder is its Maestro's). */
const rootOf = (s: Store, path: string) =>
  s.worktrees.find((w) => same(w.path, path))?.root ?? s.repos.find((r) => same(r.root, path))?.root ?? s.activeRoot;

export const useStore = create<Store>((set, get) => ({
  ready: false,
  scanned: [],
  repos: [],
  worktrees: [],
  setError: (e) => (e ? get().pushToast(String(e)) : set({ toasts: [] })),
  toasts: [],
  pushToast: (text) => set((s) => ({ toasts: withToast(s.toasts, text, ++toastSeq, Date.now()) })),
  dropToast: (id) => set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) })),
  settings: null,
  openSettings: (target = {}) => set({ settings: target }),
  closeSettings: () => set({ settings: null }),
  newAgent: null,
  openNewAgent: (target) => set({ newAgent: target }),
  closeNewAgent: () => set({ newAgent: null }),
  agentSettings: null,
  openAgentSettings: (target) => set({ agentSettings: target }),
  closeAgentSettings: () => set({ agentSettings: null }),
  composerSeed: {},
  seedComposer: (path, text) => set((s) => ({ composerSeed: { ...s.composerSeed, [pathKey(path)]: text } })),
  peekSeed: (path) => get().composerSeed[pathKey(path)],
  dropSeed: (path) =>
    set((s) => {
      if (!(pathKey(path) in s.composerSeed)) return s;
      const next = { ...s.composerSeed };
      delete next[pathKey(path)];
      return { composerSeed: next };
    }),

  connect: async () => {
    void listen<{ state: WS; version: number }>("workspace://state", (e) => {
      if (e.payload.version <= confirmedV) return;
      if (inflight) latest = e.payload; // applied once the commit in flight is settled
      else {
        confirmed = e.payload.state;
        confirmedV = e.payload.version;
        render();
      }
    });
    const snap = await ipc.wsGet();
    if (snap.version > confirmedV) {
      confirmed = snap.state;
      confirmedV = snap.version;
      render();
    }
    if (!IS_WINDOW) {
      const id = newGroupId();
      apply((ws) => (ws.windows.main ? ws : { ...putWin(putGroup(ws, emptyGroup(id)), "main", { layout: { group: id }, focus: id }), parked: { ...lastClosed(), ...ws.parked } }));
    }
  },

  loadRepos: async () => {
    // unreadable repos.json → show the (empty) app with the error rather than loading forever
    const repos = await ipc.listRepos().catch((e) => (set({ ready: true }), Promise.reject(e)));
    set({ ready: true, repos });
    // the main window starting with no project (a fresh start) goes back to where you were
    if (!IS_WINDOW && !get().activeRoot) {
      const last = savedLocation();
      const back = repos.some((r) => r.root === last.root);
      const root = back ? last.root! : (repos[0]?.root ?? null);
      // the agent is shown once its worktree has loaded (the group looks it up among them)
      const path = back ? (last.path ?? null) : null;
      // through routeTo, so a project closed last time (its tabs remembered) comes back with them
      if (root)
        apply((ws) => {
          if (focused(ws)?.root) return ws;
          const r = routeTo(ws, root);
          return path && !focused(r)?.path ? onFocused(r, (g) => ({ ...g, path, tabs: withTab(g.tabs, { kind: "wt", path }, true) })) : r;
        });
    }
    // one unreadable repo (moved, not a git repo anymore) must not keep the others from loading
    const failed = (await Promise.allSettled(repos.map((r) => get().refreshWorktrees(r.root))))
      .map((res, i) => (res.status === "rejected" ? `${repos[i].name}: ${res.reason}` : ""))
      .filter(Boolean);
    if (failed.length) get().setError(`워크트리를 읽지 못한 저장소가 있어요\n${failed.join("\n")}`);
  },
  addRepo: async (path) => {
    const repo = await ipc.addRepo(path);
    set((s) => ({ repos: s.repos.some((r) => r.root === repo.root) ? s.repos : [...s.repos, repo] }));
    get().setActive(repo.root);
    await get().refreshWorktrees(repo.root);
  },
  removeRepo: async (root) => {
    await ipc.removeRepo(root);
    const repos = get().repos.filter((r) => r.root !== root);
    set((s) => ({ repos, worktrees: s.worktrees.filter((w) => w.root !== root) }));
    const spare = newGroupId();
    // its groups close everywhere; main's last one moves on to another project (or none)
    apply((ws) => {
      let next: WS = { ...ws, parked: { ...ws.parked } };
      delete next.parked[root];
      for (const id of Object.keys(next.groups).filter((id) => next.groups[id].root === root)) next = removeGroup(next, id, spare);
      for (const id of Object.keys(next.groups).filter((id) => next.groups[id].root === root)) next = putGroup(next, { ...next.groups[id], root: repos[0]?.root ?? null });
      return next;
    });
  },
  refreshWorktrees: async (root) => {
    try {
      const list = await ipc.listWorktrees(root);
      set((s) => {
        const prev = new Map(s.worktrees.map((w) => [w.path, w]));
        return { worktrees: [...s.worktrees.filter((w) => w.root !== root), ...list.map((w) => fromInfo(root, w, prev.get(w.path)))] };
      });
    } finally {
      if (!get().scanned.includes(root)) set((s) => ({ scanned: [...s.scanned, root] }));
    }
  },
  upsert: (root, info) =>
    set((s) => {
      const prev = s.worktrees.find((w) => w.path === info.path);
      const next = fromInfo(root, info, prev);
      return { worktrees: prev ? s.worktrees.map((w) => (w.path === info.path ? next : w)) : [...s.worktrees, next] };
    }),
  onAgentState: (path, status) =>
    set((s) => {
      const onScreen = Object.values(s.groups).some((g) => g.path && same(g.path, path));
      // an agent gone off has no terminal waiting any more (the backend says so too)
      const tuiWaits = status === "TERMINATED" ? s.tuiWaits.filter((w) => !samePath(w.worktreePath, path)) : s.tuiWaits;
      return { tuiWaits, worktrees: s.worktrees.map((w) => (!same(w.path, path) ? w : { ...w, status, unread: w.unread || (status !== "RUNNING" && !onScreen) })) };
    }),
  setRuntime: (path, runtime, note) =>
    set((s) => ({ worktrees: s.worktrees.map((w) => (same(w.path, path) ? { ...w, runtime, runtimeNote: note, switchPending: null } : w)) })),
  setSwitchPending: (path, view) =>
    set((s) => ({ worktrees: s.worktrees.map((w) => (same(w.path, path) ? { ...w, switchPending: view } : w)) })),
  remove: (path) => {
    set((s) => ({ worktrees: s.worktrees.filter((w) => w.path !== path) }));
    // the discarded worktree's tab and its files' tabs go with it, in every group of every window and every parked project
    apply((ws) => {
      const gone = (t: StageTab) => same(t.kind === "wt" ? t.path : t.base, path);
      const clean = <T extends Parked>(g: T): T => ({
        ...g,
        tabs: g.tabs.filter((t) => !gone(t)),
        path: g.path && same(g.path, path) ? null : g.path,
        editor: g.editor && same(g.editor.base, path) ? null : g.editor,
      });
      return {
        ...ws,
        groups: Object.fromEntries(Object.entries(ws.groups).map(([id, g]) => [id, clean(g)])),
        parked: Object.fromEntries(Object.entries(ws.parked).map(([r, p]) => [r, clean(p)])),
      };
    });
  },
  markUnread: (path, unread) =>
    set((s) => ({ worktrees: s.worktrees.map((w) => (same(w.path, path) && w.unread !== unread ? { ...w, unread } : w)) })),
  cli: null,
  loadCli: async (recheck = false) => {
    const cli = await ipc.cliStatus(recheck);
    set({ cli });
    return cli;
  },

  ws: confirmed,
  layout: null,
  focus: null,
  groups: {},
  activeRoot: null,
  selectedPath: null,
  editor: null,
  // the open file belongs to the worktree on screen, so going elsewhere takes it off the stage (its tab stays).
  // A project coming back from its remembered tabs opens as it was left instead.
  setActive: (root) => {
    apply((ws) => {
      const f = focused(ws);
      const r = routeTo(ws, root);
      const g = focused(r);
      const restored = !!f && !!g && g.id === f.id && f.root !== root;
      return restored ? r : onFocused(r, (x) => ({ ...x, path: null, editor: null }));
    });
  },
  openProject: (root) => {
    const id = newGroupId();
    apply((ws) => {
      const w = mine(ws);
      if (!w) return ws;
      const shown = leaves(w.layout).find((g) => ws.groups[g].root === root);
      if (shown) return putWin(ws, ME, { ...w, focus: shown });
      const [parked, back] = unpark(ws.parked, root);
      return { ...putWin(putGroup(ws, { id, root, ...back }), ME, { layout: insertAt(w.layout, w.focus, id, "right"), focus: id }), parked };
    });
  },
  select: (path, pin = false) => {
    set((s) => ({ worktrees: s.worktrees.map((w) => (w.path === path ? { ...w, unread: false } : w)) }));
    const root = path ? rootOf(get(), path) : get().activeRoot;
    // a new tab opens on its role's first screen (settings maestroLayout / agentLayout); an open one keeps its own
    const newLayout = roleLayout(!!path && !!get().worktrees.find((w) => same(w.path, path))?.isMaestro);
    apply((ws) =>
      onFocused(routeTo(ws, root), (g) => ({ ...g, path, editor: null, tabs: path ? withTab(g.tabs, { kind: "wt", path, layout: newLayout }, pin) : g.tabs })),
    );
  },
  openEditor: (editor, pin = false) => {
    const root = editor ? rootOf(get(), editor.base) : null;
    apply((ws) =>
      onFocused(editor ? routeTo(ws, root) : ws, (g) => ({ ...g, editor, tabs: editor ? withTab(g.tabs, { kind: "file", ...editor }, pin) : g.tabs })),
    );
  },
  setSplitSizes: (key, sizes) => apply((ws) => (mine(ws) ? putWin(ws, ME, { ...mine(ws)!, layout: withSizes(mine(ws)!.layout, key, sizes) }) : ws)),
  setTabLayout: (groupId, path, layout) =>
    apply((ws) => {
      const g = ws.groups[groupId];
      return g ? putGroup(ws, { ...g, tabs: g.tabs.map((t) => (t.kind === "wt" && same(t.path, path) ? { ...t, layout } : t)) }) : ws;
    }),
  focusGroup: (id) => {
    if (get().focus !== id) apply((ws) => (mine(ws) && winOf(ws, id) === ME ? putWin(ws, ME, { ...mine(ws)!, focus: id }) : ws));
  },
  pinTab: (tab, groupId) => {
    const id = groupId ?? get().focus;
    if (!id) return;
    apply((ws) => {
      const g = ws.groups[id];
      if (!g || !g.tabs.some((t) => t.preview && sameTab(t, tab))) return ws;
      return putGroup(ws, { ...g, tabs: g.tabs.map((t) => (sameTab(t, tab) ? { ...t, preview: false } : t)) });
    });
  },
  closeTab: (tab, groupId) => {
    const id = groupId ?? get().focus;
    if (!id) return;
    if (tab.kind === "file") {
      const k = fileKey(tab.base, tab.rel);
      void ipc.wsDraftSet(k, null);
      set((s) => ({ dirtyFiles: s.dirtyFiles.filter((x) => x !== k) }));
    }
    const spare = newGroupId();
    apply((ws) => (ws.groups[id]?.tabs.some((t) => sameTab(t, tab)) ? leaveTab(ws, id, tab, spare) : ws));
  },
  closeOthers: (groupId, keep) => {
    const g = get().groups[groupId];
    if (!g) return;
    const keys = g.tabs.flatMap((t) => (t.kind === "file" && !(keep && sameTab(t, keep)) ? [fileKey(t.base, t.rel)] : []));
    keys.forEach((k) => void ipc.wsDraftSet(k, null));
    set((s) => ({ dirtyFiles: s.dirtyFiles.filter((k) => !keys.includes(k)) }));
    apply((ws) => {
      const now = ws.groups[groupId];
      if (!now) return ws;
      const kept = keep ? now.tabs.filter((t) => sameTab(t, keep)) : [];
      if (!kept.length) return putGroup(ws, { ...now, tabs: [], path: null, editor: null });
      // a file kept alone: the agent it covered lost its tab, so the project shows under it
      return putGroup(ws, showing({ ...now, tabs: kept, ...(kept[0].kind === "file" ? { path: null } : {}) }, kept[0]));
    });
  },
  dropTab: (tab, root, from, target, zone, copy = false, index) => {
    const id = newGroupId();
    const spare = newGroupId();
    apply((ws) => dropTabWS(ws, tab, root, from, target, zone, copy, id, spare, index));
  },
  moveGroup: (id, target, zone) => {
    const spare = newGroupId();
    apply((ws) => moveGroupWS(ws, id, target, zone, spare));
  },
  splitEmpty: (target, side) => {
    const id = newGroupId();
    apply((ws) => {
      const l = winOf(ws, target);
      if (!l) return ws;
      return putWin(putGroup(ws, emptyGroup(id, ws.groups[target].root)), l, { layout: insertAt(ws.windows[l].layout, target, id, side), focus: id });
    });
  },
  closeProject: (groupId) => {
    const g = get().groups[groupId];
    if (!g) return;
    const keys = g.tabs.flatMap((t) => (t.kind === "file" ? [fileKey(t.base, t.rel)] : []));
    keys.forEach((k) => void ipc.wsDraftSet(k, null));
    set((s) => ({ dirtyFiles: s.dirtyFiles.filter((k) => !keys.includes(k)) }));
    // its tabs are remembered and come back as they were when the project is opened again
    // (the last project closed is also kept across restarts)
    const kept: Parked = { tabs: g.tabs.map((t) => ({ ...t, preview: false })), path: g.path, editor: g.editor };
    if (g.root) {
      try {
        localStorage.setItem(LAST_CLOSED_KEY, JSON.stringify({ root: g.root, ...kept }));
      } catch {}
    }
    const spare = newGroupId();
    apply((ws) => {
      const now = ws.groups[groupId];
      if (!now) return ws;
      const parked = now.root ? { ...ws.parked, [now.root]: kept } : ws.parked;
      // the group goes; main's last one stays, with no project until one is picked in the sidebar
      if (closesWhenEmpty(ws, groupId)) return { ...removeGroup(ws, groupId, spare), parked };
      return { ...putGroup(ws, emptyGroup(groupId)), parked };
    });
  },
  popOutTab: async (tab, from, at) => {
    const root = get().groups[from]?.root;
    if (!root) return;
    const label = await ipc.wsTakeWindow();
    const id = newGroupId();
    const spare = newGroupId();
    apply((ws) => {
      const next = putWin(putGroup(ws, showing({ ...emptyGroup(id, root), tabs: [{ ...tab, preview: false }] }, tab)), label, { layout: { group: id }, focus: id });
      return leaveTab(next, from, tab, spare);
    });
    await ipc.wsShowWindow(label, at?.x, at?.y);
  },
  popOutGroup: async (id, at) => {
    const label = await ipc.wsTakeWindow();
    const spare = newGroupId();
    apply((ws) => (ws.groups[id] ? putWin(detach(ws, id, spare), label, { layout: { group: id }, focus: id }) : ws));
    await ipc.wsShowWindow(label, at?.x, at?.y);
  },
  dockHere: () =>
    apply((ws) => {
      const w = mine(ws);
      const main = ws.windows.main;
      if (!w || !main || ME === "main") return ws;
      let layout = main.layout;
      let after = main.focus;
      for (const id of leaves(w.layout)) {
        layout = insertAt(layout, after, id, "right");
        after = id;
      }
      const windows: Record<string, Win> = { ...ws.windows, main: { layout, focus: after } };
      delete windows[ME];
      return { ...ws, windows };
    }),
  nav: { list: [], i: -1 },
  goNav: (dir) => {
    const s = get();
    const j = navTarget(s, dir);
    if (j < 0) return;
    const n = s.nav.list[j];
    set({ nav: { list: s.nav.list, i: j } });
    // its tab may have been closed or replaced by a newer preview since: it comes back as the preview
    const tab = activeTab({ editor: n.editor, selectedPath: n.path });
    apply((ws) => onFocused(routeTo(ws, n.root), (g) => ({ ...g, path: n.path, editor: n.editor, tabs: tab ? withTab(g.tabs, tab, false) : g.tabs })));
  },
  systemView: "docs",
  openSystem: (view) => {
    set({ systemView: view });
    get().setActive(SYSTEM_ROOT);
  },
  dirtyFiles: [],
  setFileDirty: (key, dirty) =>
    set((s) => (dirty === s.dirtyFiles.includes(key) ? s : { dirtyFiles: dirty ? [...s.dirtyFiles, key] : s.dirtyFiles.filter((k) => k !== key) })),
  approvals: [],
  addApproval: (a) => set((s) => ({ approvals: s.approvals.some((x) => x.id === a.id) ? s.approvals : [...s.approvals, a] })),
  dropApproval: (id) => set((s) => ({ approvals: s.approvals.filter((a) => a.id !== id) })),
  tuiWaits: [],
  setTuiWait: (path, w) => set((s) => ({ tuiWaits: [...s.tuiWaits.filter((x) => !samePath(x.worktreePath, path)), ...(w ? [w] : [])] })),
  merges: [],
  addMerge: (m) => set((s) => ({ merges: [...s.merges.filter((x) => x.id !== m.id && !samePath(x.workerPath, m.workerPath)), m].sort((a, b) => a.id - b.id) })),
  dropMerge: (id) => set((s) => ({ merges: s.merges.filter((m) => m.id !== id) })),
  prs: [],
  addPr: (p) => set((s) => ({ prs: [...s.prs.filter((x) => !samePath(x.workerPath, p.workerPath)), p] })),
  dropPr: (workerPath) => set((s) => ({ prs: s.prs.filter((p) => !samePath(p.workerPath, workerPath)) })),
  prPoll: {},
  seenNotices: {},
  seeNotice: (workerPath, notice) => set((s) => ({ seenNotices: { ...s.seenNotices, [workerPath.toLowerCase()]: notice } })),
  setPrPoll: (p) => set((s) => {
    const next = { ...s.prPoll };
    if (p.pausedUntil > 0) next[p.host] = p;
    else delete next[p.host];
    return { prPoll: next };
  }),
}));

// every new place goes into the back / forward history (going back itself lands on an entry, so adds nothing)
useStore.subscribe((s) => {
  if (!s.ready || !s.layout) return;
  const cur: Nav = { root: s.activeRoot, path: s.selectedPath, editor: s.editor };
  const at = s.nav.list[s.nav.i];
  if (at && navKey(at) === navKey(cur)) return;
  const list = [...s.nav.list.slice(0, s.nav.i + 1), cur].slice(-50);
  useStore.setState({ nav: { list, i: list.length - 1 } });
});

useStore.subscribe((s, prev) => {
  if (IS_WINDOW || !s.ready || (s.activeRoot === prev.activeRoot && s.selectedPath === prev.selectedPath)) return;
  try {
    localStorage.setItem(LOCATION_KEY, JSON.stringify({ root: s.activeRoot, path: s.selectedPath }));
  } catch {}
});

/** Every project on screen in this window (any group), e.g. to keep their agents from being stopped as "away". */
export const useVisibleRoots = () =>
  useStore(useShallow((s) => (s.layout ? [...new Set(leaves(s.layout).map((id) => s.groups[id]?.root).filter((r): r is string => !!r))] : [])));

export const useWorktree = (path: string) => useStore((s) => s.worktrees.find((w) => w.path === path));
/**
 * Full screen or canvas for the agent group `g` shows (null: it shows its project): the tab's own choice, else its
 * role's first screen (a tab from before it had one).
 */
export function useTabLayout(g: Group): "full" | "canvas" | null {
  const own = g.path ? g.tabs.find((t): t is Extract<StageTab, { kind: "wt" }> => t.kind === "wt" && same(t.path, g.path))?.layout : undefined;
  const maestro = useStore((s) => !!g.path && !!s.worktrees.find((w) => same(w.path, g.path))?.isMaestro);
  const role = useSettings((p) => (maestro ? p.maestroLayout : p.agentLayout));
  return g.path ? (own ?? role) : null;
}
export const useApprovals = (path: string) => useStore(useShallow((s) => s.approvals.filter((a) => same(a.worktreePath, path))));
/** the same folder, whatever the case or slashes (pure, in attention.ts) */
export { samePath };
/** merge requests this agent (a Maestro) asked for */
export const useMerges = (path: string) => useStore(useShallow((s) => s.merges.filter((m) => samePath(m.maestroPath, path))));
/** what this agent's terminal waits for, while it does */
export const useTuiWait = (path: string) => useStore((s) => s.tuiWaits.find((w) => samePath(w.worktreePath, path)));
// 확인할 일, made once per change of what it reads (attention.ts): every selector below shares the same array. The
// worktrees change on every status update, so only what the list reads of them (root, path, name) is compared, and
// that key is built once per worktrees array: every row, glyph and project head calls this on every store update,
// and all but the first call of an update find the same array and skip the string work.
const NO_ATTENTION: AttentionItem[] = [];
let attnIn: { approvals: unknown; tuiWaits: unknown; merges: unknown; prs: unknown; repos: unknown; wts: string } | null = null;
let attnOut: AttentionItem[] = NO_ATTENTION;
let wtsOf: { list: unknown; key: string } | null = null;
function worktreesKey(list: Worktree[]): string {
  if (wtsOf?.list !== list) wtsOf = { list, key: list.map((w) => `${w.root}\n${w.path}\n${w.name}`).join("\n\n") };
  return wtsOf.key;
}
/** Everything waiting for the user, sorted (project → kind → arrival): the same array until something in it changes. */
export function selectAttention(s: Pick<Store, "approvals" | "tuiWaits" | "merges" | "prs" | "repos" | "worktrees">): AttentionItem[] {
  // nothing to show: no need to look at the worktrees at all
  if (!s.approvals.length && !s.tuiWaits.length && !s.merges.length && !s.prs.length) return NO_ATTENTION;
  const wts = worktreesKey(s.worktrees);
  const i = attnIn;
  if (i && i.approvals === s.approvals && i.tuiWaits === s.tuiWaits && i.merges === s.merges && i.prs === s.prs && i.repos === s.repos && i.wts === wts) return attnOut;
  attnIn = { approvals: s.approvals, tuiWaits: s.tuiWaits, merges: s.merges, prs: s.prs, repos: s.repos, wts };
  attnOut = attentionItems(s);
  return attnOut;
}
/** 확인할 일: the list (the top bar's popover, the sidebar, Ctrl+J) */
export const useAttention = () => useStore(selectAttention);
/** how many things wait for the user (the top bar's count, the window title) and how many keep an agent waiting */
export const useAttentionCount = () =>
  useStore(
    useShallow((s) => {
      const items = selectAttention(s);
      return { all: items.length, blocking: blockingCount(items) };
    }),
  );
/** something keeps this agent waiting for the user: a tool approval (in a card or in the terminal), or a merge request it asked for */
export const useWaiting = (path: string) => useStore((s) => waitsAt(selectAttention(s), path));
/** this worker's branch has a merge request waiting */
export const useMergeAsked = (path: string) => useStore((s) => s.merges.some((m) => samePath(m.workerPath, path)));
/** this worker's pull request (PR mode), unless dismissed */
export const usePr = (path: string) => useStore((s) => s.prs.find((p) => !p.hidden && samePath(p.workerPath, path)));
/**
 * The PR cards on an agent's block: the pull requests of the workers this Maestro asked for, and on a worker's own
 * block its PR once the Maestro that asked was discarded (`maestroPath` empty), so the PR stays reachable (merge,
 * base update, failed log, dismiss) with no note going to a later Maestro.
 */
export const usePrs = (path: string) =>
  useStore(useShallow((s) => s.prs.filter((p) => !p.hidden && (samePath(p.maestroPath, path) || (!p.maestroPath && samePath(p.workerPath, path))))));
