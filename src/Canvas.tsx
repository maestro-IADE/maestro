import { useCallback, useEffect, useMemo, useRef, useState, type CSSProperties, type ReactNode } from "react";
import {
  BaseEdge,
  type EdgeProps,
  Panel,
  ReactFlow,
  ConnectionMode,
  MarkerType,
  SelectionMode,
  addEdge,
  useEdgesState,
  useNodesState,
  useReactFlow,
  useStore as useFlowStore,
  type Connection,
  type EdgeChange,
  type IsValidConnection,
  type NodeChange,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import { useShallow } from "zustand/react/shallow";
import { ipc, type DocFile } from "./ipc";
import { PromptNode } from "./nodes/PromptNode";
import { AgentBar, AgentNode, BarButton, CanvasBackground, barOpen as openBars, COMPACT_SIZE, GRID, TERM_SIZES, clampAgentSize } from "./nodes/AgentNode";
import { CARD_COMPACT, CARD_SIZE, WorktreeCard } from "./nodes/WorktreeCard";
import { MD_SIZE, MarkdownBlock } from "./nodes/MarkdownBlock";
import { ArrowEdge, BLOCK_SIZE, STACK_GAP, StructBlock, StructEditor, ZONE_CHIP, ZONE_METRICS, ZONE_SIZE, ZONE_Z, ZoneFrame, useDropHost, useZoneFold } from "./nodes/StructNodes";
import { addPhase, boxOf, centreOf, commonPhased, currentOf, dropInto, fitZone, foldedInto, goPhase, growZones, hiddenByPhase, hostAt, isStack, layStacks, lineEnds, lineHiddenByPhase, phaseLabel, phasesOf, removePhase, renamePhase, seatIn, setPhaseMode, settleZones, setWhen, stackOnto, stackedInto, toggleWhen, wrapZone, type Held } from "./zones";
import { LibraryPanel, MD_MIME, PlaybookPanel } from "./LibraryPanel";
import { listen } from "@tauri-apps/api/event";
import { applyAsk, applyAskSet, applyDrawing, applyErase, applyPhaseMove, applyShow, tidyCanvas, type Drawing } from "./drawing";
import { VIEW_MIME, VIEW_SIZE, ViewBlock, viewKind } from "./nodes/ViewBlock";
import { QUESTION_WIDTH, QuestionBlock, QuestionEditor } from "./nodes/QuestionBlock";
import { useStore, type Worktree } from "./store";
import { ViewMap } from "./ViewMap";
import { isLightTheme, motionMs, useSettings } from "./settings";
import { ArrowRight, Bot, BookOpen, Check, ChevronRight, ChevronUp, Focus, RotateCcw, Workflow, CircleHelp, FileText, Frame, ListCollapse, Play, Maximize2, MessageCircleQuestion, MessageSquarePlus, Minimize2, Minus, Network, Palette, PencilRuler, Plus, Scan, Shrink, Spline, Square, SquarePlus, Trash2, Waypoints, Pencil, X } from "./icons";
import { openMenu, pointOf, type MenuItem } from "./ui/Menu";
import { allOf, colorVar, relationOfLine, resolve, useRelFile, type Behavior } from "./relations";
import { COLORS, COLOR_NAMES, type RelColor } from "./relationModel";
import { LineCtx, LineEditor, LineLabel, RelationPanel } from "./relationsUi";
import { ask } from "./ui/Confirm";
import { CONNECTION_LINE, edgePath, useEdgeShape } from "./edgePath";
import { commonParent, flowOfLayout, sidesFor, type Flow } from "./lineSides";
import { boundsOf, changed, inView, planFor, toward, viewFor, worthMoving, type Rect as FocusRect } from "./canvasFocus";
import { holdMs, stepsOf } from "./canvasGuide";
import { flowsFrom, record, retitle, without, type Flow as Shown } from "./canvasFlows";
import { gate, useGate, type Queued, type Signal, type Visual } from "./canvasGate";
import { MAKE_PLAY, pickBook, pickedBook, playbooks, usePicked } from "./playbookPick";
import { visualOf } from "./visualSteps";
import { named, pictureOf } from "./chatVisuals";
import type { Mini } from "./canvasMini";
import type { CanvasEdge, CanvasNode, CanvasStateSchema, LineNote, Picked, QuestionNode, StructNode, ZoneNode } from "./types";

const nodeTypes = { prompt_input_primitive: PromptNode, pty_terminal_node: AgentNode, worktree_card: WorktreeCard, markdown_block: MarkdownBlock, struct_block: StructBlock, zone: ZoneFrame, question_block: QuestionBlock, view_block: ViewBlock };

/**
 * A line that does something: a hierarchy, reference or prompt line, each an instance of its built-in relation (or of
 * one that builds on it). Its look comes from the canvas (`shownEdges`), its label stages from `LineLabel`. While its
 * worker works (and motion is on) a small dot runs along it: a state, so the line's own pattern never changes.
 */
function LinkEdge({ id, sourceX, sourceY, targetX, targetY, sourcePosition, targetPosition, markerEnd, markerStart, style, selected, data }: EdgeProps<CanvasEdge>) {
  const shape = useEdgeShape();
  const [path, x, y] = edgePath(shape, { sourceX, sourceY, sourcePosition, targetX, targetY, targetPosition });
  const [hover, setHover] = useState(false);
  const child = data?.kind === "orchestrates" ? data.child : undefined;
  const working = useStore((s) => !!child && s.worktrees.some((w) => w.status === "RUNNING" && w.path.toLowerCase() === child.toLowerCase()));
  const motion = useSettings((s) => motionMs(s) > 0);
  return (
    // hovered (on the line or its label): `line-hot` lifts it over every block and line (styles.css)
    <g className={hover ? "line-hot" : undefined} onMouseEnter={() => setHover(true)} onMouseLeave={() => setHover(false)}>
      {/* wide invisible hit area: a thin line is hard to click */}
      <BaseEdge path={path} markerEnd={markerEnd} markerStart={markerStart} style={style} interactionWidth={36} />
      {working && motion && (
        <circle r={2.5} className="edge-dot">
          <animateMotion dur="1.6s" repeatCount="indefinite" path={path} />
        </circle>
      )}
      <LineLabel id={id} x={x} y={y} data={data} selected={selected} hover={hover} onHover={setHover} cutTitle="연결 끊기" />
    </g>
  );
}
const edgeTypes = { link: LinkEdge, arrow: ArrowEdge };
// every line points from its source to its target: a small head of fixed size (screen space, so it never grows with
// the line on hover or selection), drawn in the line colour (`defaultMarkerColor`, --mae-edge)
export const ARROW = { type: MarkerType.ArrowClosed, width: 16, height: 16, markerUnits: "userSpaceOnUse" };
/** The colour of every line's head (styles.css --mae-edge). */
export const EDGE_COLOR = "var(--mae-edge)";
export const MIN_ZOOM = 0.15;
/** how long the view stays on what is new before it widens to the structure around it (ms, after the move itself) */
const FOLLOW_DWELL = 1200;
/** how long the edge of something just put on the canvas stays lit (ms; styles.css `mae-mark` fades it out at the end) */
const MARK_MS = 3000;
const MULTI_SELECT = ["Shift", "Control", "Meta"];
// the playbook picked on each canvas (by `root|scope`), kept while the app runs
// the relation new lines are drawn as on each canvas (by `root|scope`), kept while the app runs (relations.md "펜")
const pens = new Map<string, string>();
/** A line's own part (relation, label, body), or null when it has none. */
const noteOf = (e: Pick<CanvasEdge, "data">): LineNote | null => {
  const { relation, label, body } = e.data ?? {};
  return relation || label || body ? { relation, label, body } : null;
};
const MAX_ZOOM = 2;
const snap = (v: number) => Math.round(v / GRID) * GRID;

// Older canvases may hold free-form sizes/positions; pull them onto the grid and size presets.
/** A length rounded to whole grid cells, at least `min` cells. */
const cells = (px: number, min: number) => Math.max(min, Math.round(px / GRID)) * GRID;
const normalize = (n: CanvasNode): CanvasNode => {
  const position = { x: snap(n.position.x), y: snap(n.position.y) };
  if (n.type === "pty_terminal_node") {
    // a dragged size is kept (within limits); older canvases only have a preset
    if (n.data.compact) return { ...n, position, ...COMPACT_SIZE, deletable: false };
    const preset = TERM_SIZES[n.data.size ?? "M"];
    return { ...n, position, ...clampAgentSize(n.width ?? preset.width, n.height ?? preset.height), deletable: false };
  }
  if (n.type === "worktree_card") return { ...n, position, ...(n.data.compact ? CARD_COMPACT : CARD_SIZE), deletable: false };
  if (n.type === "markdown_block") return { ...n, position, ...MD_SIZE };
  // a resized structure block or area keeps its size, rounded onto whole grid cells
  if (n.type === "struct_block") return { ...n, position, width: cells(n.width ?? BLOCK_SIZE.width, 4), height: cells(n.height ?? BLOCK_SIZE.height, 2) };
  // a folded area is its chip (the size it opens back to is in its data)
  if (n.type === "zone" && n.data.folded) return { ...n, position, ...ZONE_CHIP, zIndex: ZONE_Z };
  if (n.type === "zone") return { ...n, position, width: cells(n.width ?? ZONE_SIZE.width, 8), height: cells(n.height ?? ZONE_SIZE.height, 5), zIndex: ZONE_Z };
  // a view block keeps the size it was dragged to
  if (n.type === "view_block") return { ...n, position, width: cells(n.width ?? VIEW_SIZE.width, 8), height: cells(n.height ?? VIEW_SIZE.height, 6) };
  return { ...n, position, width: undefined, height: undefined };
};

const folderName = (path: string) => path.split(/[\\/]/).pop() ?? path;
/** What an area's steps or states are called in menus: 단계 (a timeline) or 상태. */
const phaseWord = (zone: CanvasNode) => ((zone.data as { mode?: string }).mode === "state" ? "상태" : "단계");
/** Where a block in an area with steps or states exists, as shown in the area's list: "2단계", "모든 단계"; "" in a plain area. */
const whenLabel = (zone: CanvasNode, block: CanvasNode) => {
  const ps = phasesOf(zone);
  if (!ps.length) return "";
  const mine = ps.filter((p) => (block.data as { when?: string[] }).when?.includes(p.id));
  return mine.length ? mine.map((p) => phaseLabel(zone, p.id)).join(", ") : `모든 ${phaseWord(zone)}`;
};

/** The `?` at a floating toolbar's end: how to use this canvas, one line per thing, shown while it's open. */
export function Guide({ lines }: { lines: string[] }) {
  const [open, setOpen] = useState(false);
  return (
    <span className="guide">
      <button className="ct-icon" title="사용법" aria-label="사용법" aria-expanded={open} data-on={open || undefined} onClick={() => setOpen(!open)}>
        <CircleHelp />
      </button>
      {open && (
        <span className="guide-bubble small" onClick={() => setOpen(false)}>
          {lines.map((l) => <span key={l}>{l}</span>)}
        </span>
      )}
    </span>
  );
}

/**
 * The vertical tool strip on the canvases' top left (and the project map's), with a fold button at its foot: folded,
 * only that button is left (settings `toolsShut`, one switch for every canvas, kept across restarts).
 */
export function ToolStrip({ children }: { children: ReactNode }) {
  const shut = useSettings((s) => s.toolsShut);
  const set = useSettings((s) => s.set);
  return (
    <div className="canvas-tools vertical" data-shut={shut || undefined}>
      {/* first, so it is under the pointer both ways: a press opens, the next press there folds */}
      <button className="ct-icon" title={shut ? "도구 막대 펴기" : "도구 막대 접기"} aria-label={shut ? "도구 막대 펴기" : "도구 막대 접기"} aria-expanded={!shut} onClick={() => set({ toolsShut: !shut })}>
        {/* folded: the tools themselves (not the sidebar's panel icon); open: fold upwards */}
        {shut ? <PencilRuler /> : <ChevronUp />}
      </button>
      {!shut && (
        <>
          <span className="canvas-tools-sep" />
          {children}
        </>
      )}
    </div>
  );
}

/** What every canvas's `?` ends with: where the controls that moved into menus are (docs/prd stage 11). */
export const MENU_LINES = ["카드는 ⋯나 우클릭, 에이전트 블록은 머리의 ⋯로 메뉴를 열어요", "프로젝트 분할은 프로젝트 메뉴 → 분할로 열기"];

type Size = { width: number; height: number };

/**
 * Block positions as a tree: the Maestro on top, every worker below its parent, siblings side by side.
 * Worktrees nobody leads hang under the Maestro too (without a line), so the canvas reads top-down.
 */
export function treeLayout(worktrees: Worktree[], size: (w: Worktree) => Size = () => CARD_SIZE): Map<string, { x: number; y: number }> {
  const GAP_X = 2 * GRID, GAP_Y = 4 * GRID;
  const key = (p: string) => p.toLowerCase();
  const byPath = new Map(worktrees.map((w) => [key(w.path), w]));
  const led = new Set(worktrees.flatMap((w) => w.workers.map(key)));
  const maestro = worktrees.find((w) => w.isMaestro);
  const loose = worktrees.filter((w) => !w.isMaestro && !led.has(key(w.path)));

  type T = { w: Worktree; kids: T[]; kidsWidth: number; width: number };
  const seen = new Set<string>();
  const build = (w: Worktree): T => {
    seen.add(key(w.path));
    const own = w.workers.map((p) => byPath.get(key(p))).filter((c): c is Worktree => !!c);
    const kids: T[] = [];
    for (const c of w === maestro ? [...own, ...loose] : own) if (!seen.has(key(c.path))) kids.push(build(c));
    const kidsWidth = kids.reduce((s, k) => s + k.width, 0) + GAP_X * Math.max(0, kids.length - 1);
    return { w, kids, kidsWidth, width: Math.max(size(w).width, kidsWidth) };
  };
  const trees: T[] = [];
  // the backend keeps links a tree; the second loop only catches what a broken file would leave unreachable
  for (const w of maestro ? [maestro, ...worktrees] : [...loose, ...worktrees]) if (!seen.has(key(w.path))) trees.push(build(w));

  const out = new Map<string, { x: number; y: number }>();
  const place = (t: T, x: number, y: number) => {
    const s = size(t.w);
    out.set(t.w.path, { x: snap(x + (t.width - s.width) / 2), y });
    let cx = x + (t.width - t.kidsWidth) / 2;
    for (const k of t.kids) {
      place(k, cx, y + s.height + GAP_Y);
      cx += k.width + GAP_X;
    }
  };
  let x = 0;
  for (const t of trees) {
    place(t, x, 0);
    x += t.width + GAP_X;
  }
  return out;
}

type Placed = { position: { x: number; y: number }; width?: number; height?: number; measured?: { width?: number; height?: number } };
/**
 * Which sides of two cards a line leaves and enters by (lineSides.ts): the ones facing each other, or, with a
 * `flow`, the ones the layout runs by (a tree: out of the bottom, in at the top, however far to the side).
 */
function facingSides(a?: Placed, b?: Placed, flow?: Flow): [string, string] {
  const box = (n?: Placed) => n && { x: n.position.x, y: n.position.y, w: n.width ?? n.measured?.width ?? 0, h: n.height ?? n.measured?.height ?? 0 };
  return sidesFor(box(a), box(b), flow);
}

/**
 * Bottom-left zoom bar: − zoom% + | fit, and (agent canvases) 연결점 자동 정리. Clicking the percentage resets to
 * 100%. (Settings: the top bar.)
 */
export function CanvasControls({ onTidy, onArrange, onBack }: { onTidy?: () => void; onArrange?: () => void; onBack?: () => void } = {}) {
  const zoom = useFlowStore((s) => s.transform[2]);
  const { zoomIn, zoomOut, zoomTo, fitView, getNodes, getEdges } = useReactFlow();
  // 선택한 것 보기: the view filled with what is picked (a picked line counts as its two ends)
  const picked = useFlowStore((s) => s.nodes.some((n) => n.selected) || s.edges.some((e) => e.selected));
  const fitPicked = () => {
    const ids = new Set([...getNodes().filter((n) => n.selected).map((n) => n.id), ...getEdges().filter((e) => e.selected).flatMap((e) => [e.source, e.target])]);
    if (ids.size) void fitView({ nodes: [...ids].map((id) => ({ id })), padding: 0.1, maxZoom: MAX_ZOOM, duration });
  };
  const duration = motionMs(useSettings.getState());
  return (
    <Panel position="bottom-left" className="cc-row">
      <div className="cc">
        <button title="축소" aria-label="축소" onClick={() => zoomOut({ duration })}><Minus /></button>
        <button className="cc-zoom" title="100%로" onClick={() => zoomTo(1, { duration })}>{Math.round(zoom * 100)}%</button>
        <button title="확대" aria-label="확대" onClick={() => zoomIn({ duration })}><Plus /></button>
        <span className="cc-sep" />
        <button title="전체 보기" aria-label="전체 보기" onClick={() => fitView({ padding: 0.15, maxZoom: 1, duration })}>
          <Scan />
        </button>
        <button title={picked ? "선택한 것 보기 · 고른 것이 화면에 꽉 차게 맞춰요" : "선택한 것 보기 · 먼저 블록이나 선을 골라요"} aria-label="선택한 것 보기" disabled={!picked} onClick={fitPicked}>
          <Focus />
        </button>
        {onTidy && (
          <button title="연결점 자동 정리 · 지금 블록 자리에 맞게 모든 선의 연결점을 다시 골라요" aria-label="연결점 자동 정리" onClick={onTidy}>
            <Waypoints />
          </button>
        )}
        {onArrange && (
          <button title="배치 정리 · 영역 안의 블록을 줄 맞추고 크기를 맞추고, 그린 것들을 빈틈 없이 다시 놓아요 (되돌리기: Ctrl+Z)" aria-label="배치 정리" onClick={onArrange}>
            <Workflow />
          </button>
        )}
        {onBack && (
          <button title="보던 곳으로 · 화면이 저절로 옮겨지기 전에 보던 자리로 돌아가요" aria-label="보던 곳으로" onClick={onBack}>
            <RotateCcw />
          </button>
        )}
      </div>
    </Panel>
  );
}

/**
 * The working canvases (the project map, ProjectMap.tsx, only shows the structure):
 * - Maestro canvas (`scope` = the Maestro): its agent block on top, a summary card per worktree below,
 *   hierarchy and reference lines, Markdown blocks. Links are made and cut here.
 * - worktree canvas (`scope` = a worktree): that worktree's agent block plus free prompt blocks
 * Each saves its own layout.
 */
export function Canvas({ root, scope, onNewAgent, onFull, bare }: {
  root: string;
  scope: string;
  onNewAgent: (opts?: { parentPath?: string }) => void;
  onFull?: () => void; // this agent in full screen instead of its canvas
  /** a canvas with no agent behind it (the developer tests' blank one, DevTools.tsx): no chat box at the bottom */
  bare?: boolean;
}) {
  const [nodes, setNodes, onNodesChange] = useNodesState<CanvasNode>([]);
  // 플레이리스트 (canvas-flows.md): what the agent showed here with its lines, kept with the canvas as plays; the list at the top right
  const [flows, setFlows] = useState<Shown[]>([]);
  const [flowsOpen, setFlowsOpen] = useState(false);
  const [naming, setNaming] = useState<string | null>(null); // the play whose name is being edited (double click)
  const [edges, setEdges, onEdgesChange] = useEdgesState<CanvasEdge>([]);
  const { getViewport, setViewport, screenToFlowPosition, fitView } = useReactFlow();
  const needsFit = useRef(false); // no saved viewport yet → frame the blocks once they exist
  const untreed = useRef(false); // no saved layout, or one from before the tree layout
  // the agents' area (zones.md) was made on this canvas once: it is never made again (the user may have removed it)
  const teamMade = useRef(false);
  const hub = useStore((s) => s.worktrees.some((w) => w.root === root && w.isMaestro && w.path === scope));
  const worktrees = useStore(useShallow((s) => s.worktrees.filter((w) => w.root === root && (hub || w.path === scope))));
  const select = useStore((s) => s.select);
  const setError = useStore((s) => s.setError);
  // the project's relations (relations.ts): every line here is drawn as one of them
  const relFile = useRelFile(root);
  const rels = useMemo(() => allOf(relFile), [relFile]);
  const light = useSettings((s) => isLightTheme(s.theme)); // a custom palette can turn light without its id changing
  // bumped on every layout load, so the worktree sync below runs after *each* load. Otherwise a reload
  // that lands after the sync (HMR, a slow read) would overwrite freshly added cards with the old layout.
  const [loadGen, setLoadGen] = useState(0);
  const loaded = loadGen > 0;
  const [viewTick, setViewTick] = useState(0);
  const { setCenter, getZoom, getNode } = useReactFlow<CanvasNode, CanvasEdge>();
  const justAdded = useRef<{ x: number; y: number } | null>(null);
  const wtKey = worktrees.map((w) => `${w.path}|${w.branch}`).join("\n");
  const canvasKey = hub ? "overview" : folderName(scope); // "overview": the old project canvas, now the Maestro's
  // the Maestro (and a worktree on its own canvas) is a full agent block; workers are summary cards
  const kind = (w: Worktree) => (!hub || w.isMaestro ? "pty_terminal_node" : "worktree_card");
  const mirrors = (n: CanvasNode) => n.type === "worktree_card" || n.type === "pty_terminal_node";
  const pathOf = (n: CanvasNode) => (n.data as { worktreePath: string }).worktreePath;

  useEffect(() => {
    ipc.loadCanvas(root, canvasKey).then((saved) => {
      zoneSeen.current = new Map(); // (areas come as they were saved: nothing is fitted again on opening)
      // a file that isn't a canvas any more (cut short, edited by hand) opens as an empty canvas with a note, rather
      // than leaving this canvas blank and unsaved for good
      let state: CanvasStateSchema | null = null;
      try {
        state = saved ? JSON.parse(saved) : null;
        if (state && (!Array.isArray(state.nodes) || !Array.isArray(state.edges))) throw new Error("blocks or lines missing");
      } catch (err) {
        state = null;
        setError(`캔버스 파일을 읽지 못해서 빈 캔버스로 열었어요 (${String(err)})`);
      }
      pinned.current = new Map(Object.entries(state?.handles ?? {}));
      for (const e of state?.edges ?? []) if (e.sourceHandle && e.targetHandle && !pinned.current.has(e.id)) pinned.current.set(e.id, [e.sourceHandle, e.targetHandle]);
      notes.current = new Map(Object.entries(state?.lines ?? {}));
      for (const e of state?.edges ?? []) {
        const n = noteOf(e);
        if (n && !notes.current.has(e.id)) notes.current.set(e.id, n);
      }
      if (state) {
        // (what a stack holds sits in its column, a few pixels apart: not on the grid, and left where it is)
        const saved = state.nodes as CanvasNode[];
        const stacks = new Set(saved.filter((n) => n.type === "zone" && n.data.stack).map((n) => n.id));
        setNodes(saved.map((n) => (n.parentId && stacks.has(n.parentId) ? { ...normalize(n), position: n.position } : normalize(n))));
        setEdges(state.edges.map((e) => (e.data?.kind === "arrow" ? { ...e, type: "arrow", markerEnd: ARROW, data: { ...e.data, editing: false } } : e.data?.kind === "command_pipe" ? { ...e, type: "link", className: "pipe", markerEnd: ARROW } : { ...e, markerEnd: ARROW })));
        setViewport(state.viewport);
      }
      needsFit.current = !state;
      untreed.current = !state?.tree;
      teamMade.current = !!state?.team;
      setFlows(flowsFrom(state?.flows));
      setLoadGen((g) => g + 1);
    }).catch(setError);
  }, [root, canvasKey]);

  const dropped = useRef(new Map<string, CanvasNode>());
  // Keep exactly one mirrored node per worktree (a card here, or the agent block inside a worktree).
  useEffect(() => {
    if (!loaded) return;
    const paths = new Set(worktrees.map((w) => w.path));
    setNodes((ns) => {
      // free blocks (prompt, markdown) stay; a mirrored block stays while its worktree exists and it's the
      // right kind (a canvas saved before the Maestro became an agent block has a card for it)
      const kept = ns.filter((n) => !mirrors(n) || worktrees.some((w) => w.path === pathOf(n) && kind(w) === n.type));
      // a block whose worktree drops out of the list for a moment (a refresh) comes back where it was
      for (const n of ns) if (mirrors(n) && !kept.includes(n)) dropped.current.set(`${n.type}|${pathOf(n)}`, n);
      const have = new Set(kept.filter(mirrors).map(pathOf));
      // where each block is on the canvas (one inside an area has its place relative to that area): a free slot is
      // looked for there
      const onCanvas = (n: CanvasNode): CanvasNode => {
        const b = n.parentId ? boxOf(kept.includes(n) ? kept : [...kept, n], n.id, ZONE_METRICS) : null;
        return b ? { ...n, position: { x: b.x, y: b.y } } : n;
      };
      // first grid slot no block already covers (cards get moved around, so "the next index" may be taken)
      const taken = kept.map(onCanvas);
      // a new card goes into the agents' area when the canvas has one (zones.md): its place there, the area grows
      const team = hub ? kept.find((n) => n.type === "zone" && n.data.team) : undefined;
      const teamAt = team ? boxOf(kept, team.id, ZONE_METRICS) : null;
      const inTeam = (n: CanvasNode): CanvasNode => (team && teamAt ? { ...n, parentId: team.id, position: { x: n.position.x - teamAt.x, y: n.position.y - teamAt.y } } : n);
      let lastAt: { x: number; y: number } | null = null;
      const freeSlot = () => {
        for (let i = 0; ; i++) {
          const p = { x: (i % 3) * (CARD_SIZE.width + 2 * GRID), y: Math.floor(i / 3) * (CARD_SIZE.height + 2 * GRID) };
          const hit = taken.some((n) => {
            const w = n.width ?? CARD_SIZE.width, h = n.height ?? CARD_SIZE.height;
            return p.x < n.position.x + w && n.position.x < p.x + CARD_SIZE.width && p.y < n.position.y + h && n.position.y < p.y + CARD_SIZE.height;
          });
          if (!hit) return p;
        }
      };
      // a new worker's card goes under its Lead (or the block that leads it), on the first free spot to the right
      const slotNear = (w: Worktree) => {
        const lead = worktrees.find((p) => p.workers.some((x) => x.toLowerCase() === w.path.toLowerCase()));
        const at = lead && taken.find((n) => mirrors(n) && pathOf(n) === lead.path);
        if (!at) return freeSlot();
        const y = at.position.y + (at.height ?? CARD_SIZE.height) + 2 * GRID;
        for (let i = 0; i < 40; i++) {
          const p = { x: at.position.x + (i % 8) * (CARD_SIZE.width + 2 * GRID), y: y + Math.floor(i / 8) * (CARD_SIZE.height + 2 * GRID) };
          const hit = taken.some((n) => {
            const nw = n.width ?? CARD_SIZE.width, nh = n.height ?? CARD_SIZE.height;
            return p.x < n.position.x + nw && n.position.x < p.x + CARD_SIZE.width && p.y < n.position.y + nh && n.position.y < p.y + CARD_SIZE.height;
          });
          if (!hit) return p;
        }
        return freeSlot();
      };
      const added = worktrees
        .filter((w) => !have.has(w.path))
        .map((w): CanvasNode => {
          const back = dropped.current.get(`${kind(w)}|${w.path}`);
          if (back) {
            dropped.current.delete(`${kind(w)}|${w.path}`);
            // back where it was, in the area it was in (if that area is still there)
            const home = !back.parentId || kept.some((n) => n.id === back.parentId) ? back : { ...back, parentId: undefined };
            taken.push(onCanvas(home));
            lastAt = taken[taken.length - 1].position;
            return home;
          }
          if (kind(w) === "pty_terminal_node") {
            // on the Maestro canvas it keeps the card_ id, so lines and links treat it like any card
            const block: CanvasNode = { id: hub ? `card_${w.path}` : `term_${w.path}`, type: "pty_terminal_node", position: hub ? freeSlot() : { x: 360, y: 0 }, ...TERM_SIZES.M, deletable: false, data: { worktreePath: w.path, branchName: w.branch } };
            taken.push(block);
            lastAt = block.position;
            return inTeam(block);
          }
          const card: CanvasNode = {
            id: `card_${w.path}`,
            type: "worktree_card",
            position: slotNear(w),
            ...CARD_SIZE,
            deletable: false, // removed only by discarding the worktree
            data: { worktreePath: w.path },
          };
          taken.push(card);
          lastAt = card.position;
          return inTeam(card);
        });
      if (hub && added.length) justAdded.current = lastAt;
      return kept.length === ns.length && !added.length ? ns : [...kept, ...added];
    });
    setEdges((es) => es.filter((e) => !e.target.startsWith("term_") || paths.has(e.target.slice(5))));
    if (needsFit.current && worktrees.length) {
      needsFit.current = false;
      justAdded.current = null;
      requestAnimationFrame(() => fitView({ padding: 0.15, maxZoom: 1 }));
    } else {
      // a worktree created while you're here: bring its new card into view
      requestAnimationFrame(() => {
        const p = justAdded.current;
        justAdded.current = null;
        // not while you're typing in this canvas: the box you're writing in would slide away
        if (p && !typing()) setCenter(p.x + CARD_SIZE.width / 2, p.y + CARD_SIZE.height / 2, { zoom: getZoom(), duration: motionMs(useSettings.getState()) * 1.5 });
      });
    }
  }, [loadGen, wtKey]);

  // The agents' area (zones.md): on the Lead's canvas the Lead's block and the worker cards sit in one area, so the
  // whole team folds away to a chip and moves as one. Made once, around the cards where they are (new and old
  // canvases alike); a card added later goes into it (the sync above), the tree layout lays out inside it. Taken
  // away by the user, it stays away.
  const mirrorIds = nodes.filter(mirrors).map((n) => n.id).join("|");
  useEffect(() => {
    if (!loaded || !hub || teamMade.current || !mirrorIds) return;
    teamMade.current = true;
    const area = { id: `zone_team_${Date.now()}`, type: "zone", position: { x: 0, y: 0 }, zIndex: ZONE_Z, data: { title: "에이전트", team: true } } as CanvasNode;
    setNodes((ns) => (ns.some((n) => n.type === "zone" && n.data.team) ? ns : wrapZone(ns, ns.filter(mirrors).map((n) => n.id), area, ZONE_METRICS)));
  }, [loaded, hub, mirrorIds]);

  // every line on the canvas tells the memory its points (before the autosave below reads it)
  useEffect(() => {
    for (const e of edges) if (e.sourceHandle && e.targetHandle) pinned.current.set(e.id, [e.sourceHandle, e.targetHandle]);
    // a line on the canvas is the truth for its own part (a label cleared is cleared); a line gone for a moment keeps it
    for (const e of edges) {
      const n = noteOf(e);
      if (n) notes.current.set(e.id, n);
      else notes.current.delete(e.id);
    }
  }, [edges]);

  // Debounced autosave (backend writes atomically).
  useEffect(() => {
    if (!loaded) return;
    const t = setTimeout(() => {
      const state: CanvasStateSchema = {
        version: "1.0.0",
        rootPath: root,
        activeSkillId: "",
        activeThemeId: "theme_default_dark",
        viewport: getViewport(),
        tree: 1,
        team: teamMade.current ? 1 : undefined,
        flows: flows.length ? flows : undefined,
        nodes: nodes.map(({ id, type, position, width, height, data, parentId }) => ({ id, type, position, width, height, data, parentId }) as CanvasStateSchema["nodes"][number]),
        // every line, the Lead's too: who leads whom comes from the backend, but the connection points each line
        // was given (drawn, moved by its end, or 연결점 자동 정리) are this canvas's own and come back on open
        edges: edges.map(({ id, source, target, sourceHandle, targetHandle, data }) => ({ id, source, target, sourceHandle, targetHandle, data: data && { ...data, editing: undefined } })),
        handles: Object.fromEntries(pinned.current),
        lines: Object.fromEntries([...notes.current].filter(([id]) => /^(orch|ref)_/.test(id) || edges.some((e) => e.id === id))),
      };
      ipc.saveCanvas(root, canvasKey, JSON.stringify(state, null, 2)).catch(setError);
      // each settled state is an undo step (selection, viewport and connection points alone don't count; the Lead's
      // lines aren't undone either, they come from the backend)
      const key = JSON.stringify([
        state.nodes,
        state.edges.filter((e) => e.data?.kind !== "orchestrates").map(({ sourceHandle: _s, targetHandle: _t, ...e }) => e),
        // a Lead → worker line comes from the backend, but its relation, label and body are this canvas's
        state.edges.filter((e) => e.data?.kind === "orchestrates").map((e) => [e.id, noteOf(e)]),
      ]);
      const h = history.current;
      if (h.at < 0 || h.steps[h.at].key !== key) {
        h.steps = [...h.steps.slice(0, h.at + 1), { key, nodes, edges }].slice(-100);
        h.at = h.steps.length - 1;
      }
    }, 500);
    return () => clearTimeout(t);
  }, [loaded, nodes, edges, viewTick, flows]);

  // Ctrl+Z / Ctrl+Shift+Z (Ctrl+Y): steps back and forth through this canvas's blocks and lines.
  // Links kept by the backend (workers, references) are drawn from it, so they aren't undone here; nor are the
  // connection points lines are attached to (saved, but outside the undo steps).
  const history = useRef<{ steps: { key: string; nodes: CanvasNode[]; edges: CanvasEdge[] }[]; at: number }>({ steps: [], at: -1 });
  const focused = useRef(false); // the last click landed on this canvas (several can be open in splits)
  // references cut by deleting a Markdown block or a reference line (by block or line id): an undo step that brings
  // the block or line back links them again
  const cutRefs = useRef(new Map<string, { path: string; agents: string[] }>());
  // every line's connection points by line id, apart from the lines themselves: the Lead's and reference lines are
  // rebuilt from the agents, which can arrive after the canvas file (or drop out for a moment on a refresh), and a
  // line gone for that moment must come back on the points it had, not on freshly picked ones
  const pinned = useRef(new Map<string, [string, string]>());
  // every line's own part (relation, label, body) by line id, kept apart from the lines the same way: a Lead's or
  // reference line rebuilt from the agents comes back with its label and relation
  const notes = useRef(new Map<string, LineNote>());
  /** The focus is in a text field of this canvas (a chat box, a card's quick instruction, a prompt block). */
  const typing = () => {
    const a = document.activeElement;
    return !!a && !!flowRef.current?.contains(a) && a.matches("input, textarea, [contenteditable]");
  };
  /** The event happened in the canvas itself (not in a dialog a block opened, portalled elsewhere in the page). */
  const inBlock = (e: React.SyntheticEvent) => !!flowRef.current?.contains(e.target as Node);
  useEffect(() => {
    const onDown = (e: PointerEvent) => (focused.current = !!flowRef.current?.contains(e.target as Node));
    const onKey = (e: KeyboardEvent) => {
      if (!focused.current || !(e.ctrlKey || e.metaKey)) return;
      const t = e.target as HTMLElement;
      if (t.closest("input, textarea, [contenteditable], .monaco-editor, .xterm")) return;
      const k = e.key.toLowerCase();
      const dir = k === "z" ? (e.shiftKey ? 1 : -1) : k === "y" ? 1 : 0;
      const h = history.current;
      const step = h.steps[h.at + dir];
      if (!dir || !step) return;
      e.preventDefault();
      h.at += dir;
      // a step taken while a block was held (a pause in the middle of a drag) comes back at rest, or nothing on the
      // canvas would settle (which area holds what) until that block is dragged again
      zoneSeen.current = new Map(); // (the areas come back as they were then: nothing is fitted again)
      setNodes(step.nodes.map((n) => (n.dragging || n.resizing ? { ...n, dragging: false, resizing: false } : n)));
      setLoadGen((g) => g + 1); // the worktree sync puts back an agent block or card the older step lacks
      // the Lead's lines come from the backend: keep the current ones. Connection points aren't undone: a line still
      // on the canvas keeps the points it has now
      setEdges((es) => {
        const now = new Map(es.map((x) => [x.id, x]));
        const back = step.edges
          .filter((x) => x.data?.kind !== "orchestrates")
          .map((x) => {
            const cur = now.get(x.id);
            return cur ? { ...x, sourceHandle: cur.sourceHandle, targetHandle: cur.targetHandle } : x;
          });
        // the Lead's lines stay, with the relation, label and body they had in that step
        const was = new Map(step.edges.filter((x) => x.data?.kind === "orchestrates").map((x) => [x.id, noteOf(x)]));
        const lead = es
          .filter((x) => x.data?.kind === "orchestrates")
          .map((x) => (was.has(x.id) ? { ...x, data: { ...x.data!, relation: undefined, label: undefined, body: undefined, ...was.get(x.id) } } : x));
        return [...lead, ...back];
      });
      // a Markdown block or reference line this step brings back had its references cut: link them again
      const now = new Set([...live.current.nodes.map((n) => n.id), ...live.current.edges.map((x) => x.id)]);
      const back = new Set([...step.nodes.map((n) => n.id), ...step.edges.map((x) => x.id)]);
      for (const [id, cut] of cutRefs.current) {
        if (now.has(id) || !back.has(id)) continue;
        cutRefs.current.delete(id);
        for (const a of cut.agents) act(ipc.setRef(root, a, cut.path, true));
      }
    };
    window.addEventListener("pointerdown", onDown, true);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("pointerdown", onDown, true);
      window.removeEventListener("keydown", onKey);
    };
  }, []);

  // Delete (and Backspace on macOS) removes the selected blocks and lines, through the same path as × and React
  // Flow's own key (onBeforeDelete, onNodesDelete, onEdgesDelete), but only on the canvas the last click landed on,
  // while it shows, and never while typing: with the focus in the sidebar, a file tree or a code tab over the
  // canvas, or in another split, the key leaves the canvas alone.
  const { getNodes, getEdges, deleteElements: removeElements } = useReactFlow<CanvasNode, CanvasEdge>();
  useEffect(() => {
    const mac = navigator.platform.toLowerCase().includes("mac");
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Delete" && !(mac && e.key === "Backspace")) return;
      if (!focused.current || e.defaultPrevented || !flowRef.current?.offsetParent) return;
      const t = e.target as HTMLElement;
      if (t.closest("input, textarea, select, [contenteditable], .monaco-editor, .xterm, .modal")) return;
      const nodes = getNodes().filter((n) => n.selected);
      const edges = getEdges().filter((x) => x.selected);
      if (!nodes.length && !edges.length) return;
      e.preventDefault();
      void removeElements({ nodes, edges });
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  // Ctrl held: the empty canvas shows a hand (a drag pans instead of selecting)
  useEffect(() => {
    const set = (e: KeyboardEvent) => flowRef.current?.toggleAttribute("data-pan", e.ctrlKey);
    const off = () => flowRef.current?.removeAttribute("data-pan");
    window.addEventListener("keydown", set);
    window.addEventListener("keyup", set);
    window.addEventListener("blur", off);
    return () => {
      window.removeEventListener("keydown", set);
      window.removeEventListener("keyup", set);
      window.removeEventListener("blur", off);
    };
  }, []);

  // Ctrl+wheel (and trackpad pinch, which arrives as ctrl+wheel) zooms anywhere, even over a block's
  // scrolling chat; plain wheel scrolls blocks / pans the canvas. Capture phase beats the block and
  // React Flow's own handler, and preventDefault stops WebView2's page zoom.
  const flowRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const el = flowRef.current;
    if (!el) return;
    const onWheel = (e: WheelEvent) => {
      if (!e.ctrlKey) return;
      e.preventDefault();
      e.stopPropagation();
      const { x, y, zoom } = getViewport();
      const delta = e.deltaY * (e.deltaMode === 1 ? 20 : 1);
      const next = Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, zoom * Math.pow(2, -delta * 0.002)));
      const r = el.getBoundingClientRect();
      const px = e.clientX - r.left, py = e.clientY - r.top;
      // keep the point under the cursor fixed
      setViewport({ x: px - ((px - x) * next) / zoom, y: py - ((py - y) * next) / zoom, zoom: next });
    };
    el.addEventListener("wheel", onWheel, { capture: true, passive: false });
    return () => el.removeEventListener("wheel", onWheel, { capture: true });
  }, []);

  const barKey = `${root}|${scope}`;
  const [barOpen, setBarOpenState] = useState(() => openBars.get(barKey) ?? false);
  const setBarOpen = (v: boolean) => {
    openBars.set(barKey, v);
    setBarOpenState(v);
  };
  // what's selected, as this canvas's agent sees it: `maestro selection` reads it, the chat box can send it along
  const titleOf = (id?: string) => {
    const n = id ? nodes.find((x) => x.id === id) : undefined;
    return n && (n.type === "struct_block" || n.type === "zone" || n.type === "markdown_block") ? n.data.title || "이름 없음" : "";
  };
  // each line as a proposition: "B (관계: 라벨)", the relation left out when it's the plain one
  const arrowsFrom = (id: string) =>
    edges
      .filter((e) => e.data?.kind === "arrow" && e.source === id)
      .map((e) => {
        const to = titleOf(e.target);
        const rel = relationOfLine(rels, e);
        const what = [rel.id === "plain" ? "" : rel.name, e.data?.label ?? ""].filter(Boolean).join(": ");
        return to && to + (what ? ` (${what})` : "");
      })
      .filter(Boolean);
  const picked: Picked[] = nodes.flatMap((n): Picked[] => {
    if (!n.selected) return [];
    if (n.type === "struct_block") return [{ kind: "block", title: n.data.title || "이름 없음", body: n.data.body ?? "", area: titleOf(n.parentId) || undefined, to: arrowsFrom(n.id) }];
    if (n.type === "zone") return [{ kind: "area", title: n.data.title || "영역", holds: nodes.filter((c) => c.parentId === n.id).map((c) => titleOf(c.id)).filter(Boolean), to: arrowsFrom(n.id) }];
    if (n.type === "markdown_block") return [{ kind: "document", title: n.data.title, path: n.data.path }];
    if (n.type === "prompt_input_primitive") return [{ kind: "prompt", name: n.data.label }];
    // a question block, with its answer if it has one: how the agent is shown one it isn't waiting for (the user's own)
    if (n.type === "question_block") {
      const a = n.data.answer;
      return [{ kind: "question", title: n.data.title || "질문", body: n.data.body, choices: n.data.choices ?? [], multi: n.data.multi, answer: a && { picked: a.picked, text: a.text }, by: n.data.mine ? "user" : undefined }];
    }
    const w = worktrees.find((w) => w.path === pathOf(n));
    return w && w.path !== scope ? [{ kind: "agent", name: w.name, branch: w.branch, path: w.path }] : [];
  });
  // a picked line: what it joins (any block's name), its relation, label and body
  const nameOf = (id: string) => {
    const n = nodes.find((x) => x.id === id);
    if (!n) return "";
    if (n.type === "prompt_input_primitive") return n.data.label;
    if (mirrors(n)) return worktrees.find((w) => w.path === pathOf(n))?.name ?? "";
    return titleOf(id);
  };
  for (const e of edges) {
    if (!e.selected) continue;
    const rel = relationOfLine(rels, e);
    picked.push({ kind: "line", from: nameOf(e.source), to: nameOf(e.target), relation: rel.name, label: e.data?.label || undefined, body: e.data?.body || undefined });
  }
  const pickedKey = JSON.stringify(picked);
  useEffect(() => void ipc.setCanvasSelection(scope, picked).catch(() => {}), [pickedKey, scope]);
  useEffect(() => () => void ipc.setCanvasSelection(scope, []).catch(() => {}), [scope]);
  const boxing = useRef(false); // a selection box is being dragged
  const promptCount = useRef(0);
  const addPrompt = (at?: { x: number; y: number }) => {
    // where the canvas menu was opened (`at`, in canvas coordinates), else in the middle of what this canvas shows
    // (a split's own pane, not the window's), a step down-right of each prompt block already there so they don't
    // stack; named after the highest number in use, so no two share one
    const prompts = nodes.filter((n) => n.type === "prompt_input_primitive");
    const size = { width: 14 * GRID, height: 8 * GRID };
    const mid = viewMiddle(size);
    const step = (prompts.length % 6) * GRID;
    const p = at ? { x: snap(at.x - size.width / 2), y: snap(at.y - size.height / 2) } : { x: mid.x + step, y: mid.y + step };
    const used = prompts.map((n) => Number(/(\d+)$/.exec((n.data as { label: string }).label)?.[1] ?? 0));
    promptCount.current = Math.max(0, ...used) + 1;
    setNodes((ns) => [
      ...ns.map((x) => (x.selected ? { ...x, selected: false } : x)),
      { id: `prompt_${Date.now()}`, type: "prompt_input_primitive", position: p, selected: true, data: { label: `프롬프트 ${promptCount.current}`, attachedContextPaths: [], focus: true } },
    ]);
  };

  // ---- structure drawing: blocks, areas (React Flow parents) and arrows, all kept with the canvas
  const [editing, setEditing] = useState<string | null>(null);
  const editNode = editing ? (nodes.find((n) => n.id === editing && (n.type === "struct_block" || n.type === "zone")) as StructNode | ZoneNode | undefined) : undefined;
  const { getInternalNode, updateNodeData, updateEdgeData } = useReactFlow<CanvasNode, CanvasEdge>();
  // the middle of what's on screen, for a new block or area
  const viewMiddle = (size: { width: number; height: number }) => {
    const r = flowRef.current?.getBoundingClientRect();
    const c = screenToFlowPosition({ x: (r?.left ?? 0) + (r?.width ?? 800) / 2, y: (r?.top ?? 0) + (r?.height ?? 600) / 2 });
    return { x: snap(c.x - size.width / 2), y: snap(c.y - size.height / 2) };
  };
  /**
   * A new block or area, named right away: in the middle of the view, or centred on `at` (canvas coordinates, where
   * the canvas menu was opened). `parentId`: a block put inside that area (its position kept relative to it).
   */
  const addStruct = (type: "struct_block" | "zone", at?: { x: number; y: number }, parentId?: string) => {
    const id = `${type === "zone" ? "zone" : "block"}_${Date.now()}`;
    const size = type === "zone" ? ZONE_SIZE : BLOCK_SIZE;
    const base = parentId ? absOf(parentId) : { x: 0, y: 0 };
    const position = at ? { x: snap(at.x - base.x - size.width / 2), y: snap(at.y - base.y - size.height / 2) } : viewMiddle(size);
    // put into an area with steps or states: it exists in the one showing (as a block dropped there would)
    const now = currentOf(parentId ? nodes.find((x) => x.id === parentId) : undefined);
    const n = (type === "zone"
      ? { id, type, position, ...size, zIndex: ZONE_Z, data: { title: "" }, selected: true }
      : { id, type, parentId, position, ...size, data: { title: "", ...(now && { when: [now] }) }, selected: true }) as CanvasNode;
    // areas first, so they lie under what's already there; the new item alone is selected
    setNodes((ns) => (type === "zone" ? [n, ...ns.map((x) => ({ ...x, selected: false }))] : [...ns.map((x) => ({ ...x, selected: false })), n]));
    setEditing(id); // name it right away
  };
  // a question block (question-block.md) is usually an agent's, but until it is answered the user can change it, and
  // can put one there (`mine`: nobody is expected to read it until the user points the agent at it; its editor opens
  // right away)
  const [qEditing, setQEditing] = useState<string | null>(null);
  const qEditNode = qEditing ? (nodes.find((n) => n.id === qEditing && n.type === "question_block" && !n.data.answer) as QuestionNode | undefined) : undefined;
  const addQuestion = (at?: { x: number; y: number }) => {
    const qid = `q_${Date.now()}`;
    const size = { width: QUESTION_WIDTH, height: 6 * GRID };
    const position = at ? { x: snap(at.x - size.width / 2), y: snap(at.y - size.height / 2) } : viewMiddle(size);
    const n = { id: `question_${qid}`, type: "question_block", position, selected: true, data: { qid, owner: scope, title: "", mine: true } } as CanvasNode;
    setNodes((ns) => [...ns.map((x) => (x.selected ? { ...x, selected: false } : x)), n]);
    setQEditing(n.id);
  };
  // what can sit in an area: drawn blocks, documents, questions, areas, and the agents (the Lead's block and the
  // worker cards live in the agents' area by default, zones.md)
  const nestable = (n: CanvasNode) => n.type === "struct_block" || n.type === "zone" || n.type === "markdown_block" || n.type === "question_block" || n.type === "view_block" || n.type === "pty_terminal_node" || n.type === "worktree_card";
  const absOf = (id: string) => getInternalNode(id)?.internals.positionAbsolute ?? { x: 0, y: 0 };
  // React Flow draws a parent before its children: parents first, otherwise the order stays
  const parentsFirst = (ns: CanvasNode[]) => {
    const byId = new Map(ns.map((n) => [n.id, n]));
    const depth = (n: CanvasNode) => {
      let d = 0;
      for (let p = n.parentId; p && d < 50; p = byId.get(p)?.parentId) d++;
      return d;
    };
    return [...ns].sort((a, b) => depth(a) - depth(b));
  };
  // Every time things come to rest, each stack is laid out again and each area is brought in line with what it
  // holds (zones.md "크기"): an area whose contents changed (something came in or went out, moved or changed size, or
  // the area went to another step) is fitted to them again; the others only grow if they must. Who holds what is
  // not read from where things lie: that changes only when something is dragged in or out (`settleDrop` below).
  // Nothing settles while something is being dragged or resized (a block dragged past an area's edge can still be
  // on its way out)
  const boxes = nodes
    .filter(nestable)
    .map((n) => {
      // (an area's step or state, and the steps a block exists in, decide what shows: they count as much as a place)
      const d = n.data as { folded?: boolean; at?: string; when?: string[] };
      return `${n.id}>${n.parentId ?? ""}:${n.position.x},${n.position.y},${n.width ?? n.measured?.width},${n.height ?? n.measured?.height}${n.type === "zone" ? `@${d.at ?? ""}${d.folded ? "*" : ""}` : ""}${d.when ? `~${d.when.join(",")}` : ""}`;
    })
    .join("|");
  const moving = nodes.some((n) => n.dragging || n.resizing);
  // what each area held when it was last settled (zones.ts `settleZones`); forgotten when the canvas is loaded or a
  // step is undone, so a state brought back is taken as it is
  const zoneSeen = useRef(new Map<string, Held>());
  useEffect(() => {
    if (!loaded || moving) return;
    // (a stack lays what it holds in a column and is exactly that big; one left with nothing goes: zones.md "묶음")
    const seen = zoneSeen.current;
    const settle = (ns: CanvasNode[]) => settleZones(layStacks(ns, ZONE_METRICS, STACK_GAP), ZONE_METRICS, seen);
    const r = settle(nodes);
    zoneSeen.current = r.seen;
    // (computed here, not in the updater: it remembers what it saw. Should the canvas have changed meanwhile, it is
    // settled as it is by then)
    if (r.nodes !== nodes) setNodes((ns) => (ns === nodes ? r.nodes : settle(ns).nodes));
  }, [loaded, boxes, moving]);
  // a stack that went with its last block takes its lines with it
  const stackIds = nodes.filter((n) => n.type === "zone" && n.data.stack).map((n) => n.id).join("|");
  const hadStacks = useRef<string[]>([]);
  useEffect(() => {
    const now = stackIds ? stackIds.split("|") : [];
    const gone = hadStacks.current.filter((id) => !now.includes(id));
    hadStacks.current = now;
    if (gone.length) setEdges((es) => (es.some((e) => gone.includes(e.source) || gone.includes(e.target)) ? es.filter((e) => !gone.includes(e.source) && !gone.includes(e.target)) : es));
  }, [stackIds]);
  // where the pointer is on the canvas (a touch: where the finger is, or left)
  const flowPoint = (e: { clientX: number; clientY: number } | { changedTouches: ArrayLike<{ clientX: number; clientY: number }> }) => {
    const p = "changedTouches" in e ? e.changedTouches[0] : e;
    return p ? screenToFlowPosition({ x: p.clientX, y: p.clientY }) : null;
  };
  // A block let go on the middle of a block makes a stack of the two (zones.md "묶음"): the pointer is within the
  // inner half of the other one, across and down. Off the middle it only lies on that block, and in an area it is
  // then given a place beside what is there (`dropAndSeat`): areas are as big as what they hold, so nearly every
  // drop into one lands on a block, and only a deliberate one should merge. Structure blocks and documents stack.
  // Over a block that is already in a stack nothing new is made: the pointer is in that stack, so what was dragged
  // joins it and the settling above puts it in its place
  const stackable = (n?: CanvasNode) => n?.type === "struct_block" || n?.type === "markdown_block";
  const stackTarget = (dragged: CanvasNode[], at: { x: number; y: number }) => {
    if (!dragged.some(stackable)) return undefined;
    const all = getNodes();
    const byId = new Map(all.map((x) => [x.id, x]));
    const held = new Set(dragged.map((d) => d.id));
    const on = all.filter((t) => {
      if (!stackable(t) || t.hidden || held.has(t.id) || isStack(t.parentId ? byId.get(t.parentId) : undefined)) return false;
      const p = absOf(t.id), w = t.width ?? t.measured?.width ?? 0, h = t.height ?? t.measured?.height ?? 0;
      return Math.abs(at.x - (p.x + w / 2)) < w / 4 && Math.abs(at.y - (p.y + h / 2)) < h / 4;
    });
    // (several under the pointer: the one drawn last, on top)
    return on[on.length - 1];
  };
  // While something is dragged, the area the pointer is in lights up: it is the one that takes what is dragged when
  // it is let go there. None lit: let go here, it is in no area. `ids`: what is dragged (none for a new block from a
  // panel); null when nothing dragged can sit in an area (a prompt block)
  const hintDrop = (e: Parameters<typeof flowPoint>[0], ids: string[] | null) => {
    const at = ids && flowPoint(e);
    const id = (at && hostAt(getNodes(), ids, at, ZONE_METRICS)?.id) || null;
    if (useDropHost.getState().id !== id) useDropHost.setState({ id });
  };
  // What is let go at `at` goes into the area the pointer is in (zones.ts `dropInto`). In an area it may land on
  // what is there (an area is as big as what it holds, so there is rarely free room, whether it comes in or is
  // moved about inside): then it is put beside all of that, on the side it was let go (zones.ts `seatIn`, eight
  // directions). On free room it stays where it is. The area is fitted around all of it once things settle
  const dropAndSeat = (ns: CanvasNode[], ids: string[], at: { x: number; y: number }) => {
    const next = dropInto(ns, ids, at, ZONE_METRICS, nestable);
    const host = hostAt(next, ids, at, ZONE_METRICS);
    const held = host ? next.filter((n) => ids.includes(n.id) && n.parentId === host.id).map((n) => n.id) : [];
    return host && held.length ? seatIn(next, held, host.id, at, ZONE_METRICS, GRID) : next;
  };
  // Who holds what changes only here, when something dragged is let go (zones.md "소속"): it goes into the area the
  // pointer is in, or out onto the canvas when the pointer is in none. Where the blocks themselves lie doesn't count
  const settleDrop = (e: Parameters<typeof flowPoint>[0], dragged: CanvasNode[]) => {
    useDropHost.setState({ id: null });
    const at = flowPoint(e);
    if (!at) return;
    const target = stackTarget(dragged, at);
    // (with a stack made, what can't stack was still let go inside it: it joins like anything dragged into a stack)
    const stacking = target ? dragged.filter(stackable).map((d) => d.id) : [];
    const rest = dragged.map((d) => d.id).filter((id) => !stacking.includes(id));
    const zone = { id: `stack_${Date.now()}`, type: "zone", position: { x: 0, y: 0 }, zIndex: ZONE_Z, data: { title: "", stack: true } } as CanvasNode;
    setNodes((ns) => dropAndSeat(target ? stackOnto(ns, stacking, target.id, zone, ZONE_METRICS, STACK_GAP) : ns, rest, at));
  };
  const foldArea = useZoneFold();
  // deleting an area keeps what's inside it: React Flow would take the children along, so they move up a level first
  const onBeforeDelete = async ({ nodes: del, edges: delEdges }: { nodes: CanvasNode[]; edges: CanvasEdge[] }) => {
    // a Lead → worker line is a link the backend keeps, and undo can't bring it back: asked first
    const links = delEdges.filter((e) => e.data?.kind === "orchestrates");
    if (links.length) {
      const names = links.map((e) => cardOf(e.target)?.name).filter(Boolean) as string[];
      const ok = await ask({ title: links.length > 1 ? `워커 연결 ${links.length}개 끊기` : `워커 연결 끊기 · ${names[0] ?? ""}`, body: "워커는 그대로 남고 Lead와의 연결만 끊어요. 되돌리기(Ctrl+Z)로 다시 잇지 않아요.", list: names.length > 1 ? names : undefined, confirm: "끊기", danger: true });
      if (!ok) return false;
    }
    const kept = new Set(del.filter((n) => !n.selected && n.parentId).map((n) => n.id));
    if (!kept.size) return true;
    const gone = new Set(del.filter((n) => !kept.has(n.id)).map((n) => n.id));
    setNodes((ns) => {
      const byId = new Map(ns.map((n) => [n.id, n]));
      return parentsFirst(
        ns.map((n) => {
          if (!n.parentId || !gone.has(n.parentId)) return n;
          let p: string | undefined = n.parentId;
          while (p && gone.has(p)) p = byId.get(p)?.parentId;
          const a = absOf(n.id), base = p ? absOf(p) : { x: 0, y: 0 };
          return { ...n, parentId: p, position: { x: a.x - base.x, y: a.y - base.y } };
        }),
      );
    });
    return { nodes: del.filter((n) => gone.has(n.id)), edges: delEdges.filter((e) => e.selected || (!kept.has(e.source) && !kept.has(e.target))) };
  };

  // structure this canvas's agent drew (`maestro draw`): taken when it arrives, and on opening if it came while closed
  const live = useRef({ nodes, edges });
  live.current = { nodes, edges };
  // The view follows what the agent put on the canvas (canvas-controls.md "화면이 따라가기"): first what is new, then,
  // a moment later, the whole structure it belongs to (the areas it is in and everything joined to them by arrows).
  // It doesn't move for what is in view already, nor while the user is typing here or holding something; and once
  // the user moves the view themself, the second step is dropped (`onMoveStart` below)
  const followTimer = useRef(0);
  // What was just put on the canvas has its edge lit for a moment (`marked`), so it shows which things are the new
  // ones. What the view couldn't go to (the user was typing here, or holding something, or has the view following
  // turned off) is kept as `unseen`, for the notice at the top of the canvas that leads there
  const [marked, setMarked] = useState<string[]>([]);
  const markTimer = useRef(0);
  const [unseen, setUnseen] = useState<{ id: string; fresh: boolean }[]>([]);
  const mark = (ids: string[]) => {
    window.clearTimeout(markTimer.current);
    setMarked(ids);
    markTimer.current = window.setTimeout(() => setMarked([]), MARK_MS);
  };
  // (a block not measured yet: about what it will be)
  const sizeGuess = (n: CanvasNode) => {
    const guess = n.type === "question_block" ? { width: QUESTION_WIDTH, height: 10 * GRID } : BLOCK_SIZE;
    return { w: n.width ?? n.measured?.width ?? guess.width, h: n.height ?? n.measured?.height ?? guess.height };
  };
  const paneOf = () => {
    const box = flowRef.current?.getBoundingClientRect();
    return box && box.width >= 80 && box.height >= 80 ? { w: box.width, h: box.height } : null;
  };
  // where the user was looking before the view last went somewhere on its own: 보던 곳으로 goes back there. It is
  // kept until used, and taken anew only once the user has moved the view themself since
  const [backView, setBackView] = useState<{ x: number; y: number; zoom: number } | null>(null);
  const userMoved = useRef(true);
  const goBack = () => {
    if (!backView) return;
    void setViewport(backView, { duration: motionMs(useSettings.getState()) * 1.5 });
    setBackView(null);
    userMoved.current = true;
  };
  /**
   * Moves the view to a box: "moved"; "there" when it is in view already; "held" when the view stays with the user
   * (typing here, holding something, the following turned off in the settings) or the canvas isn't showing.
   * `asked`: the user asked for it, so nothing holds it.
   */
  const lookAt = (r: FocusRect, asked = false) => {
    const pane = paneOf();
    const follows = useSettings.getState().canvasFollow ?? true;
    if (!pane || (!asked && (!follows || typing() || live.current.nodes.some((n) => n.dragging || n.resizing)))) return "held";
    const to = viewFor(r, pane, { padding: 0.15, minZoom: MIN_ZOOM, maxZoom: 1 });
    const now = getViewport();
    if (!worthMoving(now, to, pane, r)) return "there";
    if (userMoved.current) {
      setBackView(now);
      userMoved.current = false;
    }
    void setViewport(to, { duration: motionMs(useSettings.getState()) * 1.5 });
    return "moved";
  };
  // a question the agent waits on: looked at before the rest of what came with it (the agent stands still until it
  // has the answer)
  const asking = (n: CanvasNode) => (n.type === "question_block" && !n.data.answer && !n.data.mine) || (n.type === "zone" && !!n.data.qset);
  /**
   * The view goes to some nodes (what a step brought, changed or names): to them first, then, a moment later, to
   * the whole structure they belong to (`whole`). `marks`: their edges are lit, and when the view is held they
   * are remembered as unseen. `want`: their ids, or how to find them on the canvas as it is by then (by name).
   * Nodes put on the canvas this instant are waited for (the canvas has them a frame later); a name that never
   * turns up is given up on.
   */
  const showNodes = (want: string[] | ((ns: CanvasNode[]) => string[]), o: { fresh: boolean; whole: boolean; marks: boolean }, tries = 12) => {
    window.clearTimeout(followTimer.current);
    requestAnimationFrame(() => {
      const sheet = live.current;
      const ids = typeof want === "function" ? want(sheet.nodes) : want;
      const there = typeof want === "function" ? ids.length > 0 : ids.every((id) => sheet.nodes.some((n) => n.id === id));
      if (!there && tries > 0) return showNodes(want, o, tries - 1);
      const plan = planFor(sheet, ids, sizeGuess, new Set([...foldedInto(sheet.nodes).keys(), ...hiddenByPhase(sheet.nodes)]), asking);
      if (!plan) return;
      if (o.marks) mark(plan.marks);
      if (lookAt(plan.first) === "held") {
        // the view stays with the user: what came is remembered until they have seen it
        if (o.marks) setUnseen((u) => [...u.filter((x) => !plan.marks.includes(x.id)), ...plan.marks.map((id) => ({ id, fresh: o.fresh }))]);
        return;
      }
      // (long enough to see what is new before the view widens)
      if (o.whole && plan.then) followTimer.current = window.setTimeout(() => lookAt(plan.then!), motionMs(useSettings.getState()) * 1.5 + FOLLOW_DWELL);
    });
  };
  // what of it is still out of view (less than half of it shows); read again whenever the view comes to rest
  const farPane = unseen.length ? paneOf() : null;
  const away = farPane
    ? unseen.filter((x) => {
        const r = boundsOf(nodes, [x.id], sizeGuess);
        return !!r && inView(getViewport(), farPane, r) < 0.5;
      })
    : unseen;
  const awayBox = farPane && away.length ? boundsOf(nodes, away.map((x) => x.id), sizeGuess) : null;
  // seen meanwhile (the user went there), or gone: no longer remembered
  useEffect(() => {
    if (farPane && away.length !== unseen.length) setUnseen(away);
  }, [viewTick, unseen, boxes]);
  const awayLabel = () => {
    const n = away.length;
    const asks = away.filter((x) => {
      const node = nodes.find((y) => y.id === x.id);
      return node?.type === "question_block" || (node?.type === "zone" && !!node.data.qset);
    }).length;
    if (asks === n) return n > 1 ? `새 질문 ${n}개` : "새 질문";
    return `${away.every((x) => x.fresh) ? "새로 생긴 것" : away.some((x) => x.fresh) ? "새로 생기거나 바뀐 것" : "바뀐 것"} ${n}개`;
  };
  const showUnseen = () => {
    if (awayBox) lookAt(awayBox, true);
    mark(away.map((x) => x.id));
    setUnseen([]);
  };

  // ---- What the agent puts here comes in step by step (canvas-controls.md "그림 넘기기"), like lines of a dialogue:
  // each step is what came under one line the agent says with it (`--say`), and that line is shown on the canvas
  // while the step is the one to look at. 자동: a step keeps the canvas for the line's time (설정 → 안내 시간), then
  // the next comes in; things nothing was said about come in at once. 눌러서: nothing comes in until the user presses
  // the line on the canvas, or 캔버스에 올리기 in the chat (canvasGate.ts): the same thing. The chat shows each
  // waiting step as a small picture first, and can throw it away instead (chat-visuals.md).
  // What a step brings is on the canvas (and saved) from the moment it is taken, so whatever the agent does next
  // finds it there; until its turn it is only kept out of sight (`veil`). What a step does rather than brings (an
  // area going to another step, the view pointed somewhere, the layout tidied) happens when its turn comes.
  // `items`: the queued things themselves; `mini`: the step's picture for the chat (chat-visuals.md), made as it
  // was taken: what it brings, or else what it points the view at
  // `again`: a step of a flow shown again (다시 보기): it brings nothing, isn't kept as a flow again and doesn't wait in the chat
  type Scene = { id: number; say?: string; nodes: string[]; touched: string[]; edges: string[]; later: Queued[]; items: Queued[]; mini: Mini | null; again?: true };
  const visualOfScene = (sc: Scene): Visual => ({ ...visualOf(`s${sc.id}`, { say: sc.say, items: sc.items }), brings: sc.nodes.length + sc.edges.length > 0, mini: sc.mini });
  const scenes = useRef<Scene[]>([]);
  const sceneSeq = useRef(0);
  const [veil, setVeil] = useState<{ nodes: string[]; edges: string[] }>({ nodes: [], edges: [] });
  const [left, setLeft] = useState(0);
  // the line on the canvas: `ends`, after how long it goes on its own (0: it stays until the next press)
  const [guide, setGuide] = useState<{ key: number; say: string; ends: number } | null>(null);
  const holdUntil = useRef(0), pumpTimer = useRef(0), hideTimer = useRef(0), credits = useRef(0);
  const manual = useSettings((s) => (s.guideMode ?? "auto") === "manual");
  // (a playbook the user named for this turn lets its steps in without a press: canvasGate.ts `loosen`)
  const loose = useGate((s) => !!s.loose[gate.key(scope)]);
  const byHand = () => (useSettings.getState().guideMode ?? "auto") === "manual" && !gate.isLoose(scope);
  const sync = () => {
    setVeil((v) => (scenes.current.length || v.nodes.length || v.edges.length ? { nodes: scenes.current.flatMap((s) => s.nodes), edges: scenes.current.flatMap((s) => s.edges) } : v));
    setLeft(scenes.current.length);
    gate.setWaiting(scope, byHand() ? scenes.current.filter((s) => !s.again).map(visualOfScene) : []);
  };
  /** 다시 보기: a kept flow's steps come up again in order, each with its line, the view going to what it showed. */
  const replay = (f: Shown) => {
    setFlowsOpen(false);
    scenes.current.push(...f.steps.map((st): Scene => ({ id: ++sceneSeq.current, say: st.say, nodes: [], touched: st.ids, edges: [], later: [], items: [], mini: null, again: true })));
    pump();
  };
  /** The next step's turn: it comes into sight, does what it does, the view goes to it and its line is shown. */
  const present = () => {
    const sc = scenes.current.shift();
    if (!sc) return;
    const moves = sc.later.filter((d) => d.phase || d.tidy);
    if (moves.length) setNodes((ns) => moves.reduce((acc, d) => (d.phase ? applyPhaseMove(d.phase, acc) : tidyCanvas(acc, d.tidy?.area ?? undefined)), ns));
    // (by name, on the canvas as it is when the view gets to it: what was drawn a moment ago is there by then)
    const pointed = sc.later.flatMap((d) => (d.focus ? [d.focus.title.trim()] : []));
    const moved = sc.later.flatMap((d) => (d.phase ? [d.phase.area.trim()] : d.tidy?.area ? [d.tidy.area.trim()] : []));
    // where to look: where the agent points; else at what the step brought; else at what it changed
    if (pointed.length) showNodes((ns) => named(ns, pointed), { fresh: false, whole: false, marks: true });
    else if (sc.nodes.length) showNodes(sc.nodes, { fresh: true, whole: true, marks: true });
    else if (sc.touched.length || moved.length) showNodes((ns) => [...sc.touched.filter((id) => ns.some((n) => n.id === id)), ...named(ns, moved)], { fresh: false, whole: !moved.length, marks: !sc.later.some((d) => d.tidy) });
    // a step with a line is kept as a step of a flow: the line, and what it showed (by id to go back there, by name to be read)
    if (sc.say && !sc.again) {
      const ns = live.current.nodes;
      const ids = pointed.length ? named(ns, pointed) : sc.nodes.length ? sc.nodes : [...sc.touched, ...named(ns, moved)];
      const names = ids.flatMap((id) => {
        const t = ((ns.find((n) => n.id === id)?.data as { title?: string } | undefined)?.title ?? "").trim();
        return t ? [t] : [];
      });
      const say = sc.say;
      setFlows((fs) => record(fs, { say, ids, names: names.length ? names : pointed }, Date.now(), `flow_${Date.now().toString(36)}`));
    }
    if (!sc.say) {
      holdUntil.current = 0;
      // (a line that was waiting for this press has had its turn)
      setGuide((g) => (g && !g.ends ? null : g));
      return;
    }
    const hold = holdMs(sc, useSettings.getState().guideSeconds ?? 4);
    const stays = byHand() && scenes.current.length > 0;
    window.clearTimeout(hideTimer.current);
    setGuide({ key: sc.id, say: sc.say, ends: stays ? 0 : hold });
    // (a little after the next step would come in on its own, so one line follows the other without a gap)
    if (!stays) hideTimer.current = window.setTimeout(() => setGuide((g) => (g?.key === sc.id ? null : g)), hold + 200);
    holdUntil.current = Date.now() + hold;
  };
  /** Lets in what may come in now: everything in turn when on its own (each line having its time), what was allowed when by hand. */
  const pump = () => {
    window.clearTimeout(pumpTimer.current);
    while (scenes.current.length) {
      if (byHand()) {
        if (credits.current < 1) break;
        credits.current--;
      } else {
        const wait = holdUntil.current - Date.now();
        if (wait > 0) {
          pumpTimer.current = window.setTimeout(pump, wait);
          break;
        }
      }
      present();
    }
    sync();
  };
  /** The user says "next": the line on the canvas pressed, or 캔버스에 올리기 in the chat. With nothing waiting, the line goes. */
  const advance = () => {
    if (!scenes.current.length) return setGuide(null);
    holdUntil.current = 0;
    present();
    pump();
  };
  /**
   * 버리기 in the chat (chat-visuals.md): a waiting step doesn't come in. What it brought, kept out of sight until
   * now, is taken off the canvas (a thing that was there before and got moved into one of its areas stays, where it
   * shows), and the agent is told. What the step changed on things already there stays changed.
   */
  const discard = (id: string) => {
    const i = scenes.current.findIndex((s) => `s${s.id}` === id);
    if (i < 0) return;
    const [sc] = scenes.current.splice(i, 1);
    const gone = new Set(sc.nodes), lines = new Set(sc.edges);
    setNodes((ns) => {
      const byId = new Map(ns.map((n) => [n.id, n]));
      return ns.flatMap((n) => {
        if (gone.has(n.id)) return [];
        if (!n.parentId || !gone.has(n.parentId)) return [n];
        // out of the area that goes: to the nearest area that stays, or the canvas itself
        let { x, y } = n.position, up = byId.get(n.parentId);
        for (let k = 0; up && gone.has(up.id) && k < 50; k++) {
          x += up.position.x;
          y += up.position.y;
          up = up.parentId ? byId.get(up.parentId) : undefined;
        }
        const { parentId: _old, extent: _kept, ...rest } = n;
        return [{ ...rest, ...(up ? { parentId: up.id } : {}), position: { x, y } } as CanvasNode];
      });
    });
    setEdges((es) => es.filter((e) => !lines.has(e.id) && !gone.has(e.source) && !gone.has(e.target)));
    const v = visualOfScene(sc);
    void ipc.discardCanvasDrawing(scope, v.seqs, v.what).catch(setError);
    sync();
  };
  useEffect(() => {
    gate.mount(scope, true);
    credits.current = gate.takePasses(scope);
    return () => {
      gate.mount(scope, false);
      for (const t of [followTimer, markTimer, pumpTimer, hideTimer]) window.clearTimeout(t.current);
    };
  }, [scope]);
  // the chat while this canvas is open: 캔버스에 올리기 is the same as pressing the line here, 버리기 throws a step away
  const onSignal = useRef<(s: Signal) => void>(() => {});
  onSignal.current = (s) => (s.type === "pass" ? advance() : discard(s.id));
  useEffect(() => gate.listen(scope, (s) => onSignal.current(s)), [scope]);
  // the setting changed while steps wait: on its own they come in now, by hand they wait for the press
  useEffect(() => pump(), [manual, loose]);
  // questions the agent waits on that are out of view: pointed to for as long as they wait (the agent stands still
  // until they are answered), whatever else has happened on the canvas since
  const asksAway = () => {
    const pane = paneOf();
    if (!pane) return null;
    const v = getViewport();
    const ids = shownNodes
      .filter((n) => !n.hidden && n.type === "question_block" && !n.data.answer && !n.data.mine && !veil.nodes.includes(n.id) && !away.some((x) => x.id === n.id))
      .map((n) => n.id)
      .filter((id) => {
        const r = boundsOf(nodes, [id], sizeGuess);
        return !!r && inView(v, pane, r) < 0.5;
      });
    const box = ids.length ? boundsOf(nodes, ids, sizeGuess) : null;
    return box ? { ids, box, deg: toward(v, pane, box) } : null;
  };
  // 배치 정리 (structure-drawing.md): the layout put straight once, then the lines' sides picked again for the new places
  const straighten = (area?: { id: string }) => {
    setNodes((ns) => tidyCanvas(ns, area));
    window.setTimeout(tidy, 80);
  };

  useEffect(() => {
    if (!loaded) return;
    const take = () =>
      ipc.takeCanvasDrawings(scope).then((list) => {
        if (!list.length) return;
        let { nodes: ns, edges: es } = live.current;
        const came: Scene[] = [];
        for (const step of stepsOf(list as Queued[])) {
          const before = { nodes: ns, edges: es };
          const later: Queued[] = [];
          for (const d of step.items) {
            // an area going to another step or state, the view pointed, the layout tidied: when the step's turn comes
            if (d.phase || d.focus || d.tidy) later.push(d);
            // a question (`maestro ask`), several at once, a file shown as a view block (`maestro show`)
            else if (d.ask) ns = applyAsk(d.ask, scope, ns);
            else if (d.ask_set) ns = applyAskSet(d.ask_set, scope, ns);
            else if (d.show) ns = applyShow(d.show, ns);
            else if (d.erase) ({ nodes: ns, edges: es } = applyErase(d.erase, ns, es));
            // a drawing (a line on its own, `maestro say`, brings nothing)
            else if (Array.isArray(d.areas) || Array.isArray(d.blocks) || Array.isArray(d.arrows)) ({ nodes: ns, edges: es } = applyDrawing(d as Drawing, ns, es));
          }
          const c = changed(before, { nodes: ns, edges: es });
          const had = new Set(before.edges.map((e) => e.id));
          const pointed = later.flatMap((d) => (d.focus ? [d.focus.title.trim()] : []));
          // (the picture is only for the chat: it never keeps a drawing from coming in)
          let mini: Mini | null = null;
          try {
            mini = pictureOf({ nodes: ns, edges: es }, c.fresh.length ? c.fresh : named(ns, pointed));
          } catch {
            mini = null;
          }
          came.push({ id: ++sceneSeq.current, say: step.say, nodes: c.fresh, touched: c.touched, edges: es.filter((e) => !had.has(e.id)).map((e) => e.id), later, items: step.items, mini });
        }
        setNodes(ns);
        setEdges(es);
        scenes.current.push(...came);
        pump();
      }, setError);
    void take();
    const off = listen<string>("maestro://draw", (e) => e.payload.toLowerCase() === scope.toLowerCase() && void take());
    return () => void off.then((f) => f());
  }, [loaded, scope]);

  // ---- Maestro canvas lines, drawn from agent settings (the backend is the truth):
  //   parent card → worker card (hierarchy), Markdown block → card (reference)
  const self = worktrees.find((w) => w.path === scope); // this canvas's own agent
  const same = (a: string, b: string) => a.toLowerCase() === b.toLowerCase();
  // the agent a block stands for: a worker card, or an agent block (card_ ids on the Maestro canvas, term_ inside a worktree)
  const idOf = (path: string) => (hub ? `card_${path}` : `term_${path}`);
  const cardOf = (id: string | null) => {
    const n = id ? nodes.find((x) => x.id === id) : undefined;
    return n && mirrors(n) ? worktrees.find((w) => w.path === pathOf(n)) : undefined;
  };
  const mdOf = (id: string | null) => nodes.find((n) => n.id === id && n.type === "markdown_block") as { data: { path: string } } | undefined;
  const reload = () => useStore.getState().refreshWorktrees(root);
  const act = (p: Promise<unknown>) => p.then(reload).catch(setError);
  // what a line between these two nodes would mean, or null if it's not a new, allowed link
  // stacks (zones.md "묶음"): a document in a stack refers to an agent through the stack: its reference line leaves
  // from the stack's frame, and the stack's documents share one line to each agent
  const stackKey = nodes.map((n) => `${n.id}>${n.parentId ?? ""}${n.type === "zone" && n.data.stack ? "=" : ""}`).join("|");
  const stacked = useMemo(() => stackedInto(nodes), [stackKey]);
  // the documents in a stack (however deep)
  const docsIn = (stack: string) => nodes.filter((n) => n.type === "markdown_block" && stacked.get(n.id) === stack) as unknown as { id: string; data: { path: string } }[];
  const stackOf = (id: string | null) => nodes.find((n) => n.id === id && n.type === "zone" && !!n.data.stack && docsIn(n.id).length > 0);
  type Link = { kind: "child"; parent: string; child: string } | { kind: "ref"; card: string; md: string } | { kind: "refs"; card: string; docs: { id: string; path: string }[] };
  const linkFor = (a: string | null, b: string | null): Link | null => {
    const [ca, cb, ma, mb] = [cardOf(a), cardOf(b), mdOf(a), mdOf(b)];
    // a stack of documents and an agent: the agent refers to every document in it (those it doesn't yet)
    const stack = stackOf(a) ?? stackOf(b);
    if (stack) {
      const to = ca ?? cb;
      const docs = to ? docsIn(stack.id).filter((d) => !to.refs.some((r) => same(r, d.data.path))) : [];
      return to && docs.length ? { kind: "refs", card: to.path, docs: docs.map((d) => ({ id: d.id, path: d.data.path })) } : null;
    }
    if (ca && cb) {
      // drag direction is hierarchy direction: from the parent to its worker; never the Lead as a worker, a link
      // that's already there, or one that would make a loop (the worker leading, however far up, the parent)
      if (cb.isMaestro || same(ca.path, cb.path) || ca.workers.some((p) => same(p, cb.path))) return null;
      for (let up = ca, i = 0; up && i < 64; i++) {
        const lead = worktrees.find((w) => w.workers.some((x) => same(x, up!.path)));
        if (lead && same(lead.path, cb.path)) return null;
        up = lead as typeof ca;
      }
      return { kind: "child", parent: ca.path, child: cb.path };
    }
    const card = ca ?? cb, md = ma ?? mb;
    if (card && md && !card.refs.some((r) => same(r, md.data.path))) return { kind: "ref", card: card.path, md: md.data.path };
    return null;
  };
  // A document that joins a stack is referred to by the agents that refer to everything else in it: the stack's one
  // line says "this stack", so it has to hold for all of it. Read when a document's stack changes (not on load).
  const stackDocs = nodes.filter((n) => n.type === "markdown_block" && stacked.has(n.id)).map((n) => `${n.id}>${stacked.get(n.id)}`).join("|");
  const lastStackDocs = useRef<{ gen: number; at: Map<string, string> } | null>(null);
  useEffect(() => {
    if (!loaded) return;
    const at = new Map(stackDocs ? stackDocs.split("|").map((x) => x.split(">") as [string, string]) : []);
    const before = lastStackDocs.current;
    lastStackDocs.current = { gen: loadGen, at };
    if (!before || before.gen !== loadGen) return;
    for (const [id, stack] of at) {
      const me = mdOf(id);
      const others = docsIn(stack).filter((d) => d.id !== id);
      if (before.at.get(id) === stack || !me || !others.length) continue;
      for (const w of worktrees) {
        const refers = (path: string) => w.refs.some((r) => same(r, path));
        if (others.every((d) => refers(d.data.path)) && !refers(me.data.path)) act(ipc.setRef(root, w.path, me.data.path, true));
      }
    }
  }, [loaded, loadGen, stackDocs]);
  const [layoutTick, setLayoutTick] = useState(0);
  // a node where it is on the canvas (its place summed up through the areas it is in), with its size
  const placedAt = (id: string): Placed | undefined => {
    const n = getInternalNode(id);
    return n ? { position: n.internals.positionAbsolute, width: n.measured.width ?? n.width, height: n.measured.height ?? n.height } : undefined;
  };
  // The way a line keeps to (canvas-controls.md "연결점 규약"): a hierarchy line runs down, the Lead above its workers,
  // so every worker is joined from the Lead's bottom; a drawn arrow keeps the way of the layout it was drawn in
  // (`flow`), or of the area both its ends are in when that was drawn as a tree or a flow. Other lines have none
  const flowOf = (e: CanvasEdge): Flow | undefined => {
    if (e.data?.kind === "orchestrates") return "down";
    if (e.data?.kind !== "arrow") return undefined;
    if (e.data.flow) return e.data.flow;
    const holder = commonParent(live.current.nodes, e.source, e.target);
    const z = holder ? live.current.nodes.find((n) => n.id === holder) : undefined;
    return z?.type === "zone" ? flowOfLayout(z.data.layout) : undefined;
  };
  const linkKey = worktrees.map((w) => `${w.workers.join(",")}|${w.refs.join(",")}`).join("\n");
  useEffect(() => {
    if (!loaded) return;
    setEdges((es) => {
      const others = es.filter((e) => e.data?.kind !== "orchestrates" && e.data?.kind !== "reference");
      const line = (id: string, source: string, target: string, kind: "orchestrates" | "reference", child?: string): CanvasEdge => {
        // a line keeps the connection points it already has (fixed: moving a block doesn't move them); only a new
        // line picks the facing sides. 연결점 자동 정리 (`tidy`) and the tree layout pick again for every line.
        const had = es.find((e) => e.id === id);
        const kept = pinned.current.get(id) ?? (had?.sourceHandle && had?.targetHandle ? ([had.sourceHandle, had.targetHandle] as [string, string]) : null);
        const [sourceHandle, targetHandle] = kept ?? facingSides(placedAt(source), placedAt(target), kind === "orchestrates" ? "down" : undefined);
        return { id, type: "link", source, target, sourceHandle, targetHandle, className: kind === "reference" ? "ref" : "orch", markerEnd: ARROW, data: { kind, child, ...notes.current.get(id) } };
      };
      const lines: CanvasEdge[] = [];
      for (const w of worktrees) {
        // hierarchy lines only where the workers are on the canvas (the Maestro's)
        for (const child of hub ? worktrees.filter((c) => w.workers.some((p) => same(p, c.path))) : []) {
          lines.push(line(`orch_${w.path}>${child.path}`, idOf(w.path), idOf(child.path), "orchestrates", child.path));
        }
        for (const md of nodes.filter((n) => n.type === "markdown_block" && w.refs.some((r) => same(r, (n.data as { path: string }).path)))) {
          lines.push(line(`ref_${md.id}>${w.path}`, md.id, idOf(w.path), "reference"));
        }
      }
      return [...others, ...lines];
    });
  }, [loadGen, wtKey, linkKey, layoutTick, nodes.length]);

  // References stay two-way: one added elsewhere (project canvas, the `maestro` CLI) shows up here as a
  // Markdown block linked to this canvas's agent; one removed elsewhere takes its block away (unless
  // another agent on this canvas still refers to it). Blocks dropped but not linked yet are left alone.
  const selfRefs = (self?.refs ?? []).join("\n");
  const lastRefs = useRef<string[] | null>(null);
  useEffect(() => {
    if (!loaded || !self) return;
    const refs = self.refs, prev = lastRefs.current;
    lastRefs.current = refs;
    const gone = (prev ?? []).filter((p) => !refs.some((r) => same(r, p)) && !worktrees.some((w) => w !== self && w.refs.some((r) => same(r, p))));
    const mdPath = (n: CanvasNode) => (n.type === "markdown_block" ? n.data.path : null);
    const missing = refs.filter((r) => !nodes.some((n) => mdPath(n) !== null && same(mdPath(n)!, r)));
    if (!gone.length && !missing.length) return;
    let live = true;
    Promise.all(missing.map((p) => ipc.mdTitle(p).catch(() => p.split("/").pop() ?? p))).then((titles) => {
      if (!live) return;
      setNodes((ns) => {
        const kept = ns.filter((n) => !(mdPath(n) !== null && gone.some((g) => same(g, mdPath(n)!))));
        // stacked down the agent block's left side, below the Markdown blocks already there
        const agent = ns.find((n) => n.id === idOf(self.path));
        const x0 = (agent?.position.x ?? 0) - MD_SIZE.width - 2 * GRID, y0 = agent?.position.y ?? 0;
        const stacked = kept.filter((n) => n.type === "markdown_block" && n.position.x === x0).length;
        const added = missing
          .filter((p) => !kept.some((n) => mdPath(n) !== null && same(mdPath(n)!, p)))
          .map((p, i): CanvasNode => ({
            id: `md_${Date.now()}_${i}`,
            type: "markdown_block",
            position: { x: x0, y: y0 + (stacked + i) * (MD_SIZE.height + GRID) },
            ...MD_SIZE,
            data: { path: p, title: titles[missing.indexOf(p)] },
          }));
        return [...kept, ...added];
      });
    });
    return () => {
      live = false;
    };
  }, [loaded, selfRefs]);

  // Tree layout is the default: a new canvas (or one saved before the tree layout) starts arranged, and so
  // does any change of worktrees or links. Blocks you drag stay where you put them (saved with the
  // canvas) until then. Markdown blocks aren't touched.
  const arrange = (animate = true) => {
    setNodes((ns) => {
      // each block's own size: the Maestro's agent block is bigger than a card
      const sizeOf = (w: Worktree) => {
        const n = ns.find((n) => n.id === `card_${w.path}`);
        return { width: n?.width ?? CARD_SIZE.width, height: n?.height ?? CARD_SIZE.height };
      };
      const pos = treeLayout(worktrees, sizeOf);
      // with the agents' area (zones.md) the tree is laid out inside it, under its name, and the area is drawn
      // around the tree; without one the cards are placed on the canvas itself (out of any area they were in)
      const team = ns.find((n) => n.type === "zone" && n.data.team);
      const moved = ns.map((n) => {
        const p = mirrors(n) && pos.get(pathOf(n));
        if (!p) return n;
        return team ? { ...n, parentId: team.id, position: { x: p.x + ZONE_METRICS.pad, y: p.y + ZONE_METRICS.top } } : { ...n, parentId: undefined, position: p };
      });
      return team ? fitZone(parentsFirst(moved), team.id, ZONE_METRICS, ZONE_SIZE) : moved;
    });
    justAdded.current = null; // the fit below shows the new card; no separate pan
    needsFit.current = false;
    requestAnimationFrame(() => {
      tidy(); // the tree layout moved every card: lines pick their sides again from the new positions
      fitView({ padding: 0.15, maxZoom: 1, duration: animate ? motionMs(useSettings.getState()) * 1.5 : 0 });
    });
  };
  const hierKey = worktrees.map((w) => `${w.path}>${w.workers.join(",")}`).join("\n");
  const baseline = useRef<{ gen: number; hier: string } | null>(null);
  useEffect(() => {
    if (!loaded || !hub || !worktrees.length) return;
    const b = baseline.current;
    baseline.current = { gen: loadGen, hier: hierKey };
    const justLoaded = !b || b.gen !== loadGen;
    // only a canvas never laid out as a tree is arranged on its own (once, on open); after that a change of who
    // leads whom places just the new cards (slotNear) and moves nothing else. A right click on empty canvas → 트리로 다시 배치 does it on demand
    if (!justLoaded || !untreed.current) return;
    untreed.current = false;
    arrange(false);
  }, [loadGen, hierKey]);

  // Markdown dropped from the library or the file tree → a Markdown block where it landed
  // what drops down under the island: the documents, or (on a Maestro's canvas) the playbooks
  const [panel, setPanel] = useState<"blocks" | "docs" | "books" | "rels" | null>(null);
  const toolsShut = useSettings((s) => s.toolsShut); // a folded tool strip shows none of its panels
  const toggle = (p: "blocks" | "docs" | "books" | "rels") => setPanel(panel === p ? null : p);
  // the pen: the relation new lines here are drawn as (기본 관계 until one is picked in the 관계 panel)
  const penKey = `${root}|${scope}`;
  const [pen, setPenState] = useState(() => pens.get(penKey) ?? "plain");
  const setPen = (id: string) => {
    pens.set(penKey, id);
    setPenState(id);
  };
  // a pen whose relation was removed (here or by an agent) is 기본 관계 again
  const penRel = resolve(rels, pen) ?? resolve(rels, "plain")!;
  /** The pen's relation for a new line of behaviour `b`, when it fits that line (else the line's default: none). */
  const penFor = (b: Behavior | null) => (penRel.behavior === b && penRel.id !== (b ?? "plain") ? penRel.id : undefined);
  // 블록: a structure block or an area, from a panel like the others
  const blockItems: Array<{ icon: ReactNode; title: string; sub: string; run: () => void }> = [
    { icon: <Square size={16} />, title: "구조 블록", sub: "이름과 메모가 있는 상자", run: () => addStruct("struct_block") },
    { icon: <Frame size={16} />, title: "영역", sub: "블록을 묶어 담는 칸", run: () => addStruct("zone") },
  ];
  // the playbook the Maestro is told to follow (null = it picks), kept per canvas for the session
  const book = usePicked((s) => pickedBook(s, scope));
  const setBook = (b: DocFile | null) => pickBook(scope, b);
  /** 플레이 만들기: the message box comes up with the playbook that makes a play already picked. */
  const makePlay = () => {
    setFlowsOpen(false);
    void playbooks().then((list) => {
      const b = list.find((f) => f.path.replace(/\\/g, "/").endsWith(`/${MAKE_PLAY}`));
      if (!b) return setError("'플레이 만들기' 플레이북을 찾지 못했어요 (~/.maestro/playbooks/make-play.md)");
      pickBook(scope, b);
      setBarOpen(true);
    }, setError);
  };
  const onDragOver = (e: React.DragEvent) => {
    if (!e.dataTransfer.types.includes(MD_MIME) && !e.dataTransfer.types.includes(VIEW_MIME)) return;
    e.preventDefault();
    e.dataTransfer.dropEffect = "copy";
    // the area the pointer is in takes the new block (as with a block dragged on the canvas): it lights up
    hintDrop(e, []);
  };
  const onDrop = async (e: React.DragEvent) => {
    useDropHost.setState({ id: null });
    // an image or a page from the file tree: a view block where it was dropped (view-block.md)
    const shown = e.dataTransfer.getData(VIEW_MIME);
    if (shown) {
      if ((e.target as Element).closest?.(".composer, .node-prompt, .agent-bar")) return;
      e.preventDefault();
      const { path } = JSON.parse(shown) as { path: string };
      const kind = viewKind(path);
      if (!kind) return;
      const at = screenToFlowPosition({ x: e.clientX, y: e.clientY });
      // (dropped in an area: it is in that area, zones.md "소속")
      const id = `view_${Date.now()}`;
      setNodes((ns) => dropAndSeat([...ns, { id, type: "view_block", position: { x: snap(at.x - VIEW_SIZE.width / 2), y: snap(at.y - VIEW_SIZE.height / 2) }, ...VIEW_SIZE, data: { path, kind } } as CanvasNode], [id], at));
      return;
    }
    const raw = e.dataTransfer.getData(MD_MIME);
    if (!raw) return;
    // dropped on a chat box or a prompt block: that one attaches the file; no stray Markdown block here too
    if ((e.target as Element).closest?.(".composer, .node-prompt, .agent-bar")) return;
    e.preventDefault();
    const file = JSON.parse(raw) as { path: string; title: string };
    const at = screenToFlowPosition({ x: e.clientX, y: e.clientY });
    const title = file.title || (await ipc.mdTitle(file.path).catch(() => file.path.split("/").pop() ?? file.path));
    const id = `md_${Date.now()}`;
    const block = { id, type: "markdown_block", position: { x: snap(at.x - MD_SIZE.width / 2), y: snap(at.y - MD_SIZE.height / 2) }, ...MD_SIZE, data: { path: file.path, title } } as CanvasNode;
    setNodes((ns) => dropAndSeat([...ns, block], [id], at));
  };

  // prompt → agent: a pipe kept with the canvas; everything else is a hierarchy or reference link (agent settings)
  const drawn = (id: string | null) => {
    const t = nodes.find((n) => n.id === id)?.type;
    return t === "struct_block" || t === "zone";
  };
  // an arrow: either end is a structure block or an area
  // (a stack of documents joined to an agent is a reference, not an arrow)
  const isArrow = (c: Connection | { source: string; target: string }) =>
    c.source !== c.target && (drawn(c.source) || drawn(c.target)) && !((stackOf(c.source) && cardOf(c.target)) || (stackOf(c.target) && cardOf(c.source)));
  // a prompt block and an agent (an agent block or a worker card), drawn from either end
  const typeOf = (id: string) => nodes.find((n) => n.id === id)?.type;
  const agentish = (t?: string) => t === "pty_terminal_node" || t === "worktree_card";
  const isPipe = (c: Connection | { source: string; target: string }) =>
    (typeOf(c.source) === "prompt_input_primitive" && agentish(typeOf(c.target))) || (typeOf(c.target) === "prompt_input_primitive" && agentish(typeOf(c.source)));
  const onConnect = useCallback(
    (c: Connection) => {
      if (isArrow(c)) return setEdges((es) => addEdge({ ...c, id: `arrow_${Date.now()}`, type: "arrow", markerEnd: ARROW, data: { kind: "arrow" as const, relation: penFor(null) } }, es));
      if (isPipe(c)) {
        // stored prompt → agent, whichever end the line was drawn from
        const p = typeOf(c.source) === "prompt_input_primitive" ? c : { source: c.target, target: c.source, sourceHandle: c.targetHandle, targetHandle: c.sourceHandle };
        return setEdges((es) => addEdge({ ...p, type: "link", className: "pipe", markerEnd: ARROW, data: { kind: "command_pipe" as const, relation: penFor("command_pipe") } }, es));
      }
      const l = linkFor(c.source, c.target);
      // a line the backend draws (worker, reference) takes the pen's relation through its note, set before it exists
      const noteFor = (id: string, b: Behavior) => {
        const relation = penFor(b);
        if (relation) notes.current.set(id, { ...notes.current.get(id), relation });
      };
      if (l?.kind === "child") {
        const link = () => {
          noteFor(`orch_${l.parent}>${l.child}`, "orchestrates");
          act(ipc.linkWorktrees(root, l.parent, l.child));
        };
        // a worker already led by someone else moves under the new Lead: said, and asked, first
        const before = worktrees.find((w) => !same(w.path, l.parent) && w.workers.some((x) => same(x, l.child)));
        const child = worktrees.find((w) => same(w.path, l.child));
        const parent = worktrees.find((w) => same(w.path, l.parent));
        if (!before) return link();
        void ask({ title: `워커 옮기기 · ${child?.name ?? ""}`, body: `${before.name} 아래에서 ${parent?.name ?? ""} 아래로 옮겨요.`, confirm: "옮기기" }).then((ok) => void (ok && link()));
        return;
      }
      if (l?.kind === "ref") {
        const md = typeOf(c.source) === "markdown_block" ? c.source : c.target;
        noteFor(`ref_${md}>${l.card}`, "reference");
        act(ipc.setRef(root, l.card, l.md, true));
      }
      if (l?.kind === "refs") {
        for (const d of l.docs) {
          noteFor(`ref_${d.id}>${l.card}`, "reference");
          act(ipc.setRef(root, l.card, d.path, true));
        }
      }
    },
    [setEdges, worktrees, nodes, pen, rels],
  );
  const [connecting, setConnecting] = useState(false);
  // this canvas's own agent block is open (not folded): it shows the approvals itself, the chat box needn't
  const selfOpen = nodes.some((n) => n.type === "pty_terminal_node" && pathOf(n) === scope && !n.data.compact);
  // ... and shows: in a folded area (the agents' area folded away) or out of its area's step it can't, and the chat
  // box at the bottom takes the approvals
  const selfShown = () => selfOpen && !nodes.some((n) => n.type === "pty_terminal_node" && pathOf(n) === scope && (into.has(n.id) || outOfPhase.has(n.id)));
  // the line whose end is being dragged to another connection point (onReconnectStart … End)
  const reconnecting = useRef<CanvasEdge | null>(null);
  /** A reconnection that keeps the line's two blocks: its new [source handle, target handle], or null. */
  const sameEnds = (old: { source: string; target: string }, c: Connection): [string | null, string | null] | null => {
    if (c.source === old.source && c.target === old.target) return [c.sourceHandle, c.targetHandle];
    if (c.source === old.target && c.target === old.source) return [c.targetHandle, c.sourceHandle];
    return null;
  };
  /** 연결점 자동 정리: every line takes its sides again from where its two blocks are now: the ones its layout runs
   * by (`flowOf`: a hierarchy and a tree down, a flow across), else the ones facing each other. A prompt block's line
   * keeps its source (the block has a single connection point). */
  const tidy = () =>
    setEdges((es) =>
      es.map((e) => {
        const [s, t] = facingSides(placedAt(e.source), placedAt(e.target), flowOf(e));
        const fromPrompt = getNode(e.source)?.type === "prompt_input_primitive";
        return { ...e, sourceHandle: fromPrompt ? e.sourceHandle : s, targetHandle: t };
      }),
    );
  const edgeShape = useEdgeShape();
  // only a selected line's ends can be dragged: two lines on one connection point would otherwise both offer an end
  // there, and the one not selected could be the one that moves
  // every line wears its relation's look: pattern class, colour (`--rel`, read by styles.css) and heads in that colour
  // folded areas (zones.md): what's inside one is hidden (the blocks stay in `nodes`, saved as they are), a line
  // wholly inside one isn't drawn, and a line to something inside one is drawn to its chip. `foldKey` keeps the map
  // the same object while blocks only move, so the lines aren't all redone on every frame of a drag
  const foldKey = nodes.map((n) => `${n.id}>${n.parentId ?? ""}${n.type === "zone" && n.data.folded ? "*" : ""}${n.type === "zone" && n.data.stack ? "=" : ""}`).join("|");
  const into = useMemo(() => foldedInto(nodes), [foldKey]);
  // steps and states (zones.md): what doesn't exist in the step or state its area shows is hidden the same way, and a
  // line with such an end (or limited to other steps itself) isn't drawn. `phaseKey`: as `foldKey`, so a drag
  // doesn't redo it
  const phaseKey = nodes
    .map((n) => {
      const d = n.data as { when?: string[]; mode?: string; at?: string; phases?: { id: string }[] };
      return `${n.id}>${n.parentId ?? ""}:${d.when?.join(",") ?? ""}${n.type === "zone" && d.mode ? `#${d.mode}@${d.at ?? ""}[${(d.phases ?? []).map((p) => p.id).join(",")}]` : ""}`;
    })
    .join("|");
  const outOfPhase = useMemo(() => hiddenByPhase(nodes), [phaseKey]);
  const shownNodes = useMemo(() => (into.size || outOfPhase.size ? nodes.map((n) => (into.has(n.id) || outOfPhase.has(n.id) ? { ...n, hidden: true } : n)) : nodes), [nodes, into, outOfPhase]);
  const lineOut = (e: CanvasEdge) => lineHiddenByPhase(live.current.nodes, outOfPhase, { source: e.source, target: e.target, when: e.data?.when, whenZone: e.data?.whenZone });
  const shownEdges = useMemo(
    () => {
      const fromStack = new Set<string>();
      return edges.map((e): CanvasEdge => {
        if (lineOut(e)) return { ...e, hidden: true, selected: false };
        const folded = lineEnds(into, e.source, e.target);
        if (!folded) return { ...e, hidden: true, selected: false };
        const stack = e.data?.kind === "reference" && folded.source === e.source ? stacked.get(e.source) : undefined;
        const ends = stack ? { ...folded, source: stack, moved: true } : folded;
        if (stack) {
          // one line per stack and agent: the first document's stands for all of them
          const pair = `${stack}>${ends.target}`;
          if (fromStack.has(pair)) return { ...e, hidden: true, selected: false };
          fromStack.add(pair);
        }
        const rel = relationOfLine(rels, e);
        const color = colorVar(rel.color);
        const head = { ...ARROW, color };
        const shown: CanvasEdge = {
          ...e,
          ...(e.selected ? {} : { reconnectable: false }),
          className: [e.className, `rel-${rel.pattern}`].filter(Boolean).join(" "),
          style: { ...e.style, "--rel": color } as CSSProperties,
          markerEnd: rel.head === "none" ? undefined : head,
          markerStart: rel.head === "both" ? head : undefined,
        };
        if (!ends.moved) return shown;
        // between the chip and the other end, on the sides facing each other (a prompt block keeps its one point).
        // The chip isn't the line's real end, so the line can't be picked, moved or cut there
        const at = (id: string) => {
          const c = centreOf(live.current.nodes, id, ZONE_METRICS);
          return c ? { position: c } : undefined;
        };
        const [s, t] = facingSides(at(ends.source), at(ends.target));
        const fromPrompt = live.current.nodes.find((n) => n.id === e.source)?.type === "prompt_input_primitive";
        // (a stack's line can still be picked and cut: that cuts the reference of every document in the stack)
        const fixed = stack && !folded.moved ? { reconnectable: false } : { selected: false, selectable: false, reconnectable: false, focusable: false };
        return { ...shown, source: ends.source, target: ends.target, sourceHandle: fromPrompt ? e.sourceHandle : s, targetHandle: t, ...fixed };
      });
    },
    // layoutTick: a chip dragged somewhere else picks its facing sides again
    [edges, rels, into, stacked, outOfPhase, layoutTick],
  );
  /** The folded areas' chips a line is drawn to in place of its real ends; none for a line shown as it is. */
  const chipsOf = (id: string) => {
    const e = edges.find((x) => x.id === id);
    const ends = e && lineEnds(into, e.source, e.target);
    return e && ends?.moved ? [ends.source !== e.source && ends.source, ends.target !== e.target && ends.target].filter((c): c is string => !!c) : [];
  };
  // what a folded area or a step hides doesn't stay picked (nor a line taken to a chip): Delete would cut what
  // can't be seen
  useEffect(() => {
    const off = (e: CanvasEdge) => !!e.selected && (into.has(e.source) || into.has(e.target) || lineOut(e));
    setEdges((es) => (es.some(off) ? es.map((e) => (off(e) ? { ...e, selected: false } : e)) : es));
    if (outOfPhase.size) setNodes((ns) => (ns.some((n) => n.selected && outOfPhase.has(n.id)) ? ns.map((n) => (n.selected && outOfPhase.has(n.id) ? { ...n, selected: false } : n)) : ns));
  }, [into, outOfPhase]);
  // What React Flow is given isn't the state: `shownNodes` marks what a folded area hides, `shownEdges` dresses the
  // lines and moves the ends of those that go into a folded area. A change made through React Flow (updateNodeData,
  // updateEdgeData, an area folding) hands the shown thing back: the hidden mark and the chip's ends come off again
  // here, or a block would stay hidden for good and a line would stay tied to the chip
  const onNodesChangeShown = (cs: NodeChange<CanvasNode>[]) =>
    onNodesChange(cs.map((c) => ((c.type === "replace" || c.type === "add") && c.item.hidden ? { ...c, item: { ...c.item, hidden: undefined } } : c)));
  const onEdgesChangeShown = (cs: EdgeChange<CanvasEdge>[]) => {
    const real = new Map(live.current.edges.map((e) => [e.id, e]));
    onEdgesChange(
      // a selection box takes blocks only: lines are picked by clicking them
      cs.filter((c) => !(boxing.current && c.type === "select")).map((c): EdgeChange<CanvasEdge> => {
        if (c.type !== "replace") return c;
        const r = real.get(c.id);
        if (!r || (!c.item.hidden && c.item.source === r.source && c.item.target === r.target)) return c;
        const { hidden: _h, selectable: _s, focusable: _f, ...item } = c.item;
        return { ...c, item: { ...item, source: r.source, target: r.target, sourceHandle: r.sourceHandle, targetHandle: r.targetHandle, reconnectable: r.reconnectable, selected: r.selected } };
      }),
    );
  };
  // how many of this canvas's lines each relation has (shown in the 관계 panel)
  const relCounts = useMemo(() => {
    const m = new Map<string, number>();
    for (const e of edges) {
      const id = relationOfLine(rels, e).id;
      m.set(id, (m.get(id) ?? 0) + 1);
    }
    return m;
  }, [edges, rels]);
  // the line whose editor is open (double click, or the pencil on a picked line)
  const [lineEditing, setLineEditing] = useState<string | null>(null);
  /** Opens a line's editor, unless the line is drawn to a folded area's chip (it is edited once the area is open). */
  const openLine = (id: string) => void (chipsOf(id).length || setLineEditing(id));
  const editLine = lineEditing ? edges.find((e) => e.id === lineEditing) : undefined;
  // its line went away (cut elsewhere, unlinked by an agent): the editor closes and doesn't pop up when it returns
  useEffect(() => void (lineEditing && !editLine && setLineEditing(null)), [lineEditing, editLine]);
  /** 블록으로 꺼내기: A → B becomes A → [block] → B, the block named by the line's label (or relation) holding its body. */
  const extract = (id: string) => {
    const e = edges.find((x) => x.id === id);
    if (!e || e.data?.kind !== "arrow") return;
    const mid = (nid: string) => {
      const n = getInternalNode(nid);
      const p = n?.internals.positionAbsolute ?? { x: 0, y: 0 };
      return { x: p.x + (n?.measured.width ?? 0) / 2, y: p.y + (n?.measured.height ?? 0) / 2 };
    };
    const a = mid(e.source), b = mid(e.target);
    const c = { x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 };
    const rel = relationOfLine(rels, e);
    // both ends in the same area: the block goes in it too (placed relative to it), so it moves with the area
    const parentId = getNode(e.source)?.parentId === getNode(e.target)?.parentId ? getNode(e.source)?.parentId : undefined;
    const base = parentId ? (getInternalNode(parentId)?.internals.positionAbsolute ?? { x: 0, y: 0 }) : { x: 0, y: 0 };
    const block = {
      id: `block_${Date.now()}`,
      type: "struct_block",
      parentId,
      position: { x: snap(c.x - base.x - BLOCK_SIZE.width / 2), y: snap(c.y - base.y - BLOCK_SIZE.height / 2) },
      ...BLOCK_SIZE,
      selected: true,
      data: { title: e.data?.label || rel.name, body: e.data?.body },
    } as CanvasNode;
    const at = (q: { x: number; y: number }) => ({ position: q });
    const [s1, t1] = facingSides(at(a), at(c), e.data?.flow);
    const [s2, t2] = facingSides(at(c), at(b), e.data?.flow);
    const data = { kind: "arrow" as const, relation: e.data?.relation, flow: e.data?.flow };
    setNodes((ns) => [...ns.map((n) => (n.selected ? { ...n, selected: false } : n)), block]);
    setEdges((es) => [
      ...es.filter((x) => x.id !== id),
      { id: `arrow_${Date.now()}_a`, type: "arrow", source: e.source, target: block.id, sourceHandle: s1, targetHandle: t1, markerEnd: ARROW, data },
      { id: `arrow_${Date.now()}_b`, type: "arrow", source: block.id, target: e.target, sourceHandle: s2, targetHandle: t2, markerEnd: ARROW, data },
    ]);
  };
  /**
   * A block that can be 선으로 합치기: one drawn line in, one out, nothing else attached, and the two lines bare (no
   * label or body of their own, which the merged line would have to drop) and of one relation.
   */
  const mergeable = (nid: string) => {
    const ins = edges.filter((e) => e.data?.kind === "arrow" && e.target === nid);
    const outs = edges.filter((e) => e.data?.kind === "arrow" && e.source === nid);
    const others = edges.some((e) => e.data?.kind !== "arrow" && (e.source === nid || e.target === nid));
    if (ins.length !== 1 || outs.length !== 1 || others || ins[0].source === outs[0].target) return null;
    const bare = (e: CanvasEdge) => !e.data?.label?.trim() && !e.data?.body?.trim();
    if (!bare(ins[0]) || !bare(outs[0]) || relationOfLine(rels, ins[0]).id !== relationOfLine(rels, outs[0]).id) return null;
    return { into: ins[0], out: outs[0] };
  };
  /** 선으로 합치기: the block becomes a line again, its name the label and its notes the body. */
  const merge = (nid: string) => {
    const m = mergeable(nid);
    const n = nodes.find((x) => x.id === nid);
    if (!m || n?.type !== "struct_block") return;
    setEditing(null);
    setNodes((ns) => ns.filter((x) => x.id !== nid));
    setEdges((es) => [
      ...es.filter((x) => x.id !== m.into.id && x.id !== m.out.id),
      {
        id: `arrow_${Date.now()}`,
        type: "arrow",
        source: m.into.source,
        target: m.out.target,
        sourceHandle: m.into.sourceHandle,
        targetHandle: m.out.targetHandle,
        markerEnd: ARROW,
        selected: true,
        data: { kind: "arrow", relation: m.into.data?.relation, label: n.data.title || undefined, body: n.data.body || undefined },
      },
    ]);
  };
  // ---- right click (docs/features/canvas/canvas-controls.md "우클릭 메뉴"): empty canvas, a drawn block, a line.
  // Worker cards and agent blocks have their own menus (useAgentMenu, the header's ⋯) and are left alone.
  const deleteSelected = () => void removeElements({ nodes: getNodes().filter((n) => n.selected), edges: getEdges().filter((x) => x.selected) });
  /** A right click on something not picked picks it alone (as a click would); on something picked, keeps the pick.
   * Returns how many things the menu acts on. */
  const pickFor = (kind: "node" | "edge", id: string) => {
    const hit = kind === "node" ? nodes.find((n) => n.id === id) : edges.find((x) => x.id === id);
    if (hit?.selected) return nodes.filter((n) => n.selected).length + edges.filter((x) => x.selected).length;
    const pick = <T extends { id: string; selected?: boolean }>(xs: T[], k: "node" | "edge") =>
      xs.map((x) => {
        const on = k === kind && x.id === id;
        return !!x.selected === on ? x : { ...x, selected: on };
      });
    setNodes((ns) => pick(ns, "node"));
    setEdges((es) => pick(es, "edge"));
    return 1;
  };
  const del = (count: number, one: string): MenuItem => ({ label: count > 1 ? `선택한 ${count}개 지우기` : one, icon: <Trash2 size={14} />, shortcut: "Delete", danger: true, onSelect: deleteSelected });
  const onPaneMenu = (e: React.MouseEvent | MouseEvent) => {
    e.preventDefault();
    setPanel(null);
    const at = screenToFlowPosition({ x: e.clientX, y: e.clientY });
    openMenu(pointOf(e), [
      { label: "구조 블록 추가", icon: <Square size={14} />, onSelect: () => addStruct("struct_block", at) },
      { label: "영역 추가", icon: <Frame size={14} />, onSelect: () => addStruct("zone", at) },
      { label: "프롬프트 블록 추가", icon: <MessageSquarePlus size={14} />, onSelect: () => addPrompt(at) },
      { label: "질문 블록 추가", icon: <MessageCircleQuestion size={14} />, onSelect: () => addQuestion(at) },
      ...(hub ? [{ label: "새 워커…", icon: <Bot size={14} />, onSelect: () => onNewAgent({ parentPath: scope }) }] : []),
      "sep",
      { label: "전체 보기", icon: <Scan size={14} />, onSelect: () => fitView({ padding: 0.15, maxZoom: 1, duration: motionMs(useSettings.getState()) }) },
      { label: "연결점 자동 정리", icon: <Waypoints size={14} />, onSelect: tidy },
      { label: "배치 정리", icon: <Workflow size={14} />, onSelect: () => straighten() },
      ...(hub ? [{ label: "트리로 다시 배치", icon: <Network size={14} />, onSelect: () => arrange() }] : []),
    ]);
  };
  const onNodeMenu = (e: React.MouseEvent, n: CanvasNode) => {
    // a field or a text in the block keeps the system menu (copy, paste)
    if ((e.target as Element).closest("input, textarea, select, [contenteditable], .xterm, .monaco-editor")) return;
    if (n.type !== "struct_block" && n.type !== "zone" && n.type !== "markdown_block" && n.type !== "prompt_input_primitive" && n.type !== "question_block") return;
    e.preventDefault();
    setPanel(null);
    const count = pickFor("node", n.id);
    const at = screenToFlowPosition({ x: e.clientX, y: e.clientY });
    const items: MenuItem[] = [];
    if (count === 1 && n.type === "struct_block") {
      items.push({ label: "이름·메모 편집…", icon: <Pencil size={14} />, shortcut: "더블클릭", onSelect: () => setEditing(n.id) });
      if (mergeable(n.id)) items.push({ label: "선으로 합치기", icon: <Spline size={14} />, onSelect: () => merge(n.id) });
    }
    if (count === 1 && n.type === "zone") {
      // its editor: its name and colour, its steps or states, the list of what it holds. A folded area's double click
      // opens the area instead
      items.push({ label: n.data.stack ? "묶음 편집…" : "영역 편집…", icon: <Pencil size={14} />, shortcut: n.data.folded ? undefined : "더블클릭", onSelect: () => setEditing(n.id) });
      if (n.data.folded) items.push({ label: "펼치기", icon: <Maximize2 size={14} />, shortcut: "더블클릭", onSelect: () => foldArea(n.id, false) });
      else {
        items.push({ label: "여기에 구조 블록 추가", icon: <Square size={14} />, onSelect: () => addStruct("struct_block", at, n.id) });
        // (a stack is always as big as what it holds)
        if (!n.data.stack) items.push({ label: "내용에 맞추기", icon: <Shrink size={14} />, onSelect: () => setNodes((ns) => fitZone(ns, n.id, ZONE_METRICS, ZONE_SIZE)) });
        // (a stack lays itself out, the agents' area has its tree, and what an area with steps holds may share a place)
        if (!n.data.stack && !n.data.team && !n.data.mode) items.push({ label: "이 영역 배치 정리", icon: <Workflow size={14} />, onSelect: () => straighten({ id: n.id }) });
        items.push({ label: "작게 접기", icon: <Minimize2 size={14} />, onSelect: () => foldArea(n.id, true) });
      }
    }
    if (count === 1 && n.type === "question_block" && !n.data.answer) {
      items.push({ label: "질문 편집…", icon: <Pencil size={14} />, shortcut: "더블클릭", onSelect: () => setQEditing(n.id) });
    }
    // in an area with steps or states: which of them this block (and the others picked in the same area) exists in
    const host = n.parentId ? nodes.find((x) => x.id === n.parentId) : undefined;
    const hostPhases = phasesOf(host);
    if (host && hostPhases.length) {
      const ids = nodes.filter((x) => x.parentId === host.id && (x.id === n.id || (count > 1 && x.selected))).map((x) => x.id);
      const mine = (n.data as { when?: string[] }).when?.filter((w) => hostPhases.some((p) => p.id === w));
      const word = phaseWord(host);
      const where = pointOf(e);
      items.push({
        label: ids.length > 1 ? `보이는 ${word}… (${ids.length}개)` : `보이는 ${word}…`,
        icon: <ListCollapse size={14} />,
        onSelect: () =>
          openMenu(where, [
            { label: `모든 ${word}에 보이기`, icon: mine?.length ? undefined : <Check size={14} />, onSelect: () => setNodes((ns) => setWhen(ns, ids, undefined)) },
            "sep",
            ...hostPhases.map((p) => ({ label: phaseLabel(host, p.id), icon: mine?.includes(p.id) ? <Check size={14} /> : undefined, onSelect: () => setNodes((ns) => setWhen(ns, ids, toggleWhen(mine, hostPhases, p.id))) })),
          ]),
      });
    }
    // a border colour for the picked blocks and areas (all of them at once: the same colour says they belong together)
    const paint = nodes.filter((x) => (x.id === n.id || (count > 1 && x.selected)) && (x.type === "struct_block" || x.type === "zone")).map((x) => x.id);
    if (paint.length) {
      const color = (c?: RelColor) => setNodes((ns) => ns.map((x) => (paint.includes(x.id) && (x.type === "struct_block" || x.type === "zone") ? ({ ...x, data: { ...x.data, color: c } } as CanvasNode) : x)));
      const dot = (c?: RelColor) => <i className="menu-dot" style={c ? { background: colorVar(c) } : undefined} data-none={!c || undefined} />;
      const at = pointOf(e);
      items.push({
        label: paint.length > 1 ? `테두리 색… (${paint.length}개)` : "테두리 색…",
        icon: <Palette size={14} />,
        onSelect: () => openMenu(at, [{ label: "색 없음", icon: dot(), onSelect: () => color(undefined) }, ...COLORS.map((c) => ({ label: COLOR_NAMES[c], icon: dot(c), onSelect: () => color(c) }))]),
      });
    }
    if (items.length) items.push("sep");
    // a Markdown block only refers to its file: taking it off the canvas leaves the file alone
    // (a stack taken away leaves its blocks where they are, as an area does)
    items.push(del(count, n.type === "markdown_block" ? "캔버스에서 빼기" : n.type === "zone" && n.data.stack ? "묶음 풀기" : "지우기"));
    openMenu(pointOf(e), items);
  };
  const onEdgeMenu = (e: React.MouseEvent, x: CanvasEdge) => {
    e.preventDefault();
    setPanel(null);
    // a line drawn to a folded area's chip isn't at its real end there (zones.md): it can't be edited or cut from
    // here, only its area opened
    const chips = chipsOf(x.id);
    if (chips.length) return openMenu(pointOf(e), [{ label: "영역 펼치기", icon: <Maximize2 size={14} />, onSelect: () => chips.forEach((id) => foldArea(id, false)) }]);
    const count = pickFor("edge", x.id);
    const items: MenuItem[] = [];
    if (count === 1) {
      items.push({ label: "선 편집…", icon: <Pencil size={14} />, shortcut: "더블클릭", onSelect: () => setLineEditing(x.id) });
      if (x.data?.kind === "arrow") items.push({ label: "블록으로 꺼내기", icon: <Square size={14} />, onSelect: () => extract(x.id) });
      // both ends in an area with steps or states: the line can be limited to some of them (a state diagram's lines
      // change while its blocks stay)
      const zid = commonPhased(nodes, x.source, x.target);
      const zone = zid ? nodes.find((z) => z.id === zid) : undefined;
      const zonePhases = phasesOf(zone);
      if (zone && zonePhases.length && x.data?.kind === "arrow") {
        const mine = x.data.whenZone === zid ? x.data.when?.filter((w) => zonePhases.some((p) => p.id === w)) : undefined;
        const word = phaseWord(zone);
        const where = pointOf(e);
        const limit = (when?: string[]) => setEdges((es) => es.map((l) => (l.id === x.id && l.data ? { ...l, data: { ...l.data, when, whenZone: when ? zid : undefined } } : l)));
        items.push({
          label: `보이는 ${word}…`,
          icon: <ListCollapse size={14} />,
          onSelect: () =>
            openMenu(where, [
              { label: `모든 ${word}에 보이기`, icon: mine?.length ? undefined : <Check size={14} />, onSelect: () => limit(undefined) },
              "sep",
              ...zonePhases.map((p) => ({ label: phaseLabel(zone, p.id), icon: mine?.includes(p.id) ? <Check size={14} /> : undefined, onSelect: () => limit(toggleWhen(mine, zonePhases, p.id)) })),
            ]),
        });
      }
      items.push("sep");
    }
    // a Lead → worker line is a link: cutting it is asked first (onBeforeDelete)
    items.push(del(count, x.data?.kind === "orchestrates" ? "워커 연결 끊기…" : "선 지우기"));
    openMenu(pointOf(e), items);
  };
  const lineCtx = useMemo(() => ({ rels, editLine: openLine, extract }), [rels, edges, into]);
  const isValidConnection: IsValidConnection = (c) => {
    const r = reconnecting.current;
    if (r) return !!sameEnds(r, c as Connection); // moving a line's end: only to the same two blocks
    return isArrow(c) || isPipe(c) || !!linkFor(c.source, c.target);
  };

  return (
    <LineCtx.Provider value={lineCtx}>
    <ReactFlow
      nodes={shownNodes}
      edges={shownEdges}
      nodeTypes={nodeTypes}
      edgeTypes={edgeTypes}
      defaultMarkerColor={EDGE_COLOR}
      connectionLineType={CONNECTION_LINE[edgeShape]}
      // while a line is being drawn every connection point shows (styles.css `.react-flow.connecting`)
      className={[connecting && "connecting", barOpen && "bar-open"].filter(Boolean).join(" ") || undefined}
      onConnectStart={() => setConnecting(true)}
      onNodesChange={onNodesChangeShown}
      onEdgesChange={onEdgesChangeShown}
      onSelectionStart={() => (boxing.current = true)}
      onSelectionEnd={() => (boxing.current = false)}
      onConnect={onConnect}
      // a selected line's ends can be dragged to another connection point of the same two blocks (not to other
      // blocks: that would change who leads whom; cut the line and draw a new one instead)
      reconnectRadius={8}
      onReconnectStart={(_, e) => (reconnecting.current = e)}
      onReconnectEnd={() => (reconnecting.current = null)}
      onReconnect={(old, c) => {
        const handles = sameEnds(old, c);
        if (!handles) return setError("다른 블록에 이으려면 선을 끊고 새로 이어요");
        setEdges((es) => es.map((e) => (e.id === old.id ? { ...e, sourceHandle: handles[0], targetHandle: handles[1] } : e)));
      }}
      isValidConnection={isValidConnection}
      connectionMode={ConnectionMode.Loose}
      onDragOver={onDragOver}
      onDrop={onDrop}
      // a line from a card dropped on empty canvas → a new worktree as that card's worker
      onConnectEnd={(e, cs) => {
        setConnecting(false);
        const from = cardOf(cs.fromNode?.id ?? null);
        // only a line let go over empty canvas means "a new worker": not over a block, a toolbar, the minimap or the
        // chat box, and not a line that barely left its point (under 24px)
        const p = "changedTouches" in e ? e.changedTouches[0] : e;
        const under = p && document.elementFromPoint(p.clientX, p.clientY);
        const onPane = !!under?.closest(".react-flow__pane") && !under.closest(".react-flow__node, .react-flow__panel");
        const start = cs.from;
        const far = !!start && !!cs.to && Math.hypot(cs.to.x - start.x, cs.to.y - start.y) >= GRID;
        if (from && !cs.toNode && onPane && far) onNewAgent({ parentPath: from.path });
      }}
      onEdgesDelete={(gone) => {
        for (const e of gone) {
          // a line cut on purpose forgets its label and relation: linked again later, it starts plain (an undo that
          // brings a reference line back brings them back with it)
          notes.current.delete(e.id);
          const card = cardOf(e.target);
          const parent = cardOf(e.source);
          if (e.data?.kind === "orchestrates" && card && parent) act(ipc.unlinkWorktrees(root, parent.path, card.path));
          // (a stack's line is drawn from the stack: the document it stands for is the real line's end)
          const src = edges.find((x) => x.id === e.id)?.source ?? e.source;
          const md = mdOf(src);
          if (e.data?.kind === "reference" && card && md) {
            // a document in a stack: the one line stood for every document in it, so all of them stop referring
            const st = stacked.get(src);
            const docs = st ? docsIn(st).filter((d) => card.refs.some((r) => same(r, d.data.path))) : [];
            for (const d of docs.length ? docs : [{ id: src, data: md.data }]) {
              cutRefs.current.set(`ref_${d.id}>${card.path}`, { path: d.data.path, agents: [card.path] });
              act(ipc.setRef(root, card.path, d.data.path, false));
            }
          }
        }
      }}
      onNodesDelete={(gone) => {
        // the last block of a Markdown file left this canvas → the agents here no longer refer to it
        for (const n of gone.filter((g) => g.type === "markdown_block")) {
          const path = (n.data as { path: string }).path;
          const left = nodes.some((o) => o.id !== n.id && o.type === "markdown_block" && same((o.data as { path: string }).path, path));
          if (left) continue;
          const agents = worktrees.filter((w) => w.refs.some((r) => same(r, path)));
          cutRefs.current.set(n.id, { path, agents: agents.map((w) => w.path) });
          for (const w of agents) act(ipc.setRef(root, w.path, path, false));
        }
      }}
      // what is dragged goes into the area the pointer is in when it is let go (that area is lit meanwhile). React
      // Flow calls these two for a dragged block and for a dragged selection box alike (with everything dragged), so
      // the selection's own handlers are not used: the drop would be settled twice
      onNodeDrag={(e, _n, dragged) => hintDrop(e, dragged.some(nestable) ? dragged.map((d) => d.id) : null)}
      onNodeDragStop={(e, _n, dragged) => {
        settleDrop(e, dragged);
        setLayoutTick((t) => t + 1);
      }}
      onBeforeDelete={onBeforeDelete}
      onEdgeDoubleClick={(_, e) => openLine(e.id)}
      // a click on empty canvas closes the tool strip's open panel (문서 · 관계 · 플레이북), as its × does
      onPaneClick={() => setPanel(null)}
      onPaneContextMenu={onPaneMenu}
      onNodeContextMenu={onNodeMenu}
      onEdgeContextMenu={onEdgeMenu}
      onNodeClick={(e, n) => inBlock(e) && n.type === "pty_terminal_node" && n.data.compact && pathOf(n) === scope && setBarOpen(!barOpen)}
      onNodeDoubleClick={(e, n) => {
        // a double click on a button, a field or a link in the block, or inside a dialog it opened (portalled out of
        // the canvas but still bubbling through the block), means that, not the block's own double click
        if (!inBlock(e) || (e.target as Element).closest("button, a, input, textarea, select, [role=button]")) return;
        if (n.type === "worktree_card") select(n.data.worktreePath, true);
        else if (n.type === "zone" && n.data.folded) foldArea(n.id, false);
        else if (n.type === "struct_block" || n.type === "zone") setEditing(n.id);
        else if (n.type === "question_block" && !n.data.answer) setQEditing(n.id);
        // this canvas's agent block, by its header or folded chip (not inside the chat, where a double click selects a word)
        else if (n.type === "pty_terminal_node" && pathOf(n) === scope && (e.target as HTMLElement).closest(".node-agent > header, .agent-chip")) onFull?.();
      }}
      zoomOnDoubleClick={false}
      // (the user moving the view, or picking something up, takes over from the view following a drawing)
      onMoveStart={(e) => {
        if (!e) return;
        window.clearTimeout(followTimer.current);
        userMoved.current = true;
      }}
      onNodeDragStart={() => window.clearTimeout(followTimer.current)}
      onMoveEnd={() => setViewTick((t) => t + 1)}
      snapToGrid
      snapGrid={[GRID, GRID]}
      ref={flowRef}
      minZoom={MIN_ZOOM}
      maxZoom={MAX_ZOOM}
      zoomOnScroll={false}
      zoomOnPinch={false}
      zoomActivationKeyCode={null}
      panOnScroll
      // left drag on empty canvas draws a selection box, Shift/Ctrl-click adds to it; the middle button, or a
      // left drag with Ctrl held, pans
      panOnDrag={[1]}
      panActivationKeyCode="Control"
      selectionOnDrag
      selectionMode={SelectionMode.Partial}
      multiSelectionKeyCode={MULTI_SELECT}
      // the middle button would otherwise start the browser's autoscroll
      onMouseDownCapture={(e) => e.button === 1 && e.preventDefault()}
      // Delete is handled by the canvas itself (only while it has the focus and shows): see the keydown effect
      deleteKeyCode={null}
      // arrow keys scroll the chat and move the sidebar, never the clicked block
      disableKeyboardA11y
      colorMode={light ? "light" : "dark"}
      proOptions={{ hideAttribution: true }}
    >
      <CanvasBackground />
      <CanvasControls onTidy={tidy} onArrange={() => straighten()} onBack={backView ? goBack : undefined} />
      <ViewMap nodes={shownNodes} />
      {/* the edge of what the agent just put here, lit for a moment (a ring around the node, whatever kind it is) */}
      {marked.length > 0 && (
        <style>{`${marked.map((id) => `.react-flow__node[data-id="${CSS.escape(id)}"]::after`).join(", ")} { content: ""; position: absolute; inset: -6px; border: 2px solid var(--mae-accent); border-radius: 18px; pointer-events: none; animation: mae-mark ${MARK_MS}ms ease-out forwards; }`}</style>
      )}
      {/* what a step brings stays out of sight until its turn (it is on the canvas already: see `scenes`) */}
      {(veil.nodes.length > 0 || veil.edges.length > 0) && (
        <style>{`${[...veil.nodes.map((id) => `.react-flow__node[data-id="${CSS.escape(id)}"]`), ...veil.edges.flatMap((id) => [`.react-flow__edge[data-id="${CSS.escape(id)}"]`, `.line-label[data-edge="${CSS.escape(id)}"]`])].join(", ")} { opacity: 0 !important; pointer-events: none !important; }`}</style>
      )}
      {/* over everything on the canvas: the line the agent says with what is on screen, and the ways to what isn't */}
      <Panel position="top-center" className="canvas-top">
        {(guide || (manual && left > 0)) && (
          <button
            key={guide?.key ?? "waiting"}
            className="canvas-guide"
            data-waiting={!guide || undefined}
            style={guide?.ends ? { animation: `mae-guide ${guide.ends + 200}ms ease forwards` } : undefined}
            title={left > 0 ? "누르면 다음으로 넘어가요 (채팅의 '캔버스에 올리기'와 같아요)" : "누르면 닫혀요"}
            onClick={advance}
          >
            <span>{guide?.say ?? `에이전트가 보여 줄 것이 ${left}개 있어요`}</span>
            {left > 0 && (
              <b>
                {guide ? "다음" : "보기"}
                <ChevronRight size={14} />
                {left > 1 && <i>{left}</i>}
              </b>
            )}
          </button>
        )}
        {/* what the agent put on the canvas while the view stayed with the user, and is out of view: the way there */}
        {awayBox && farPane && (
          <div className="canvas-tools canvas-unseen" role="status">
            <button title="화면 밖에 있어요 · 누르면 그리로 가요" onClick={showUnseen}>
              <ArrowRight size={14} style={{ transform: `rotate(${toward(getViewport(), farPane, awayBox)}deg)` }} />
              {awayLabel()}
            </button>
            <button className="ct-icon" title="알림 닫기" aria-label="알림 닫기" onClick={() => setUnseen([])}>
              <X size={14} />
            </button>
          </div>
        )}
        {(() => {
          const asks = asksAway();
          return (
            asks && (
              <div className="canvas-tools canvas-unseen" data-ask role="status">
                <button
                  title="에이전트가 답을 기다리는 질문이 화면 밖에 있어요 · 누르면 그리로 가요"
                  onClick={() => {
                    lookAt(asks.box, true);
                    mark(asks.ids);
                  }}
                >
                  <ArrowRight size={14} style={{ transform: `rotate(${asks.deg}deg)` }} />
                  {asks.ids.length > 1 ? `답을 기다리는 질문 ${asks.ids.length}개` : "답을 기다리는 질문"}
                </button>
              </div>
            )
          );
        })()}
      </Panel>
      {/* 플레이리스트: what the agent showed here, kept as plays to look at again; always there (empty: 플레이 만들기) */}
      {(
        <Panel position="top-right" className="canvas-flows">
          <div className="canvas-tools">
            <button className="ct-icon" title="플레이리스트" aria-label="플레이리스트" aria-expanded={flowsOpen} data-on={flowsOpen || undefined} onClick={() => setFlowsOpen(!flowsOpen)}>
              <ListCollapse size={16} />
            </button>
          </div>
          {flowsOpen && (
            <div className="library canvas-flows-list nowheel nodrag">
              {/* no head of its own (the button above names it and closes it): a quiet line that says how many there
                  are, or that there are none, with + to make one */}
              <div className="library-body canvas-flows-body">
              <div className="canvas-flows-top">
                <span>{flows.length ? `플레이 ${flows.length}` : "플레이 없음"}</span>
                {!bare && <button className="icon" title="플레이 만들기" aria-label="플레이 만들기" onClick={makePlay}><Plus size={14} /></button>}
              </div>
              {[...flows].reverse().map((f) => (
                <div key={f.id} className="canvas-flow">
                  {/* the name wraps (two lines at most) with the count under it; a double click edits it */}
                  <div className="canvas-flow-text" onDoubleClick={() => setNaming(f.id)}>
                    {naming === f.id ? (
                      <input
                        autoFocus
                        className="canvas-flow-title"
                        aria-label="플레이 이름"
                        defaultValue={f.title}
                        onFocus={(e) => e.currentTarget.select()}
                        onBlur={(e) => {
                          if (e.target.value.trim() !== f.title) setFlows((fs) => retitle(fs, f.id, e.target.value));
                          setNaming(null);
                        }}
                        onKeyDown={(e) => {
                          if (e.key === "Escape") e.currentTarget.value = f.title;
                          if (e.key === "Enter" || e.key === "Escape") e.currentTarget.blur();
                        }}
                      />
                    ) : (
                      <span className="canvas-flow-name" title={f.title}>{f.title}</span>
                    )}
                    <span className="canvas-flow-meta">{f.summary ?? `${f.steps.length} step${f.steps.length === 1 ? "" : "s"}`}</span>
                  </div>
                  <button className="icon" title="다시 보기" aria-label="다시 보기" onClick={() => replay(f)}><Play size={14} /></button>
                  <button className="icon" title="지우기" aria-label="플레이 지우기" onClick={() => setFlows((fs) => without(fs, f.id))}><X size={14} /></button>
                </div>
              ))}
              </div>
            </div>
          )}
        </Panel>
      )}
      {/* this canvas's own agent, spoken to from the bottom: the logo button (or its folded block) opens and closes it */}
      {!bare && (
        <Panel position="bottom-center" className="agent-bar-panel">
          {barOpen && <AgentBar key={scope} cards={!selfShown()} path={scope} picked={picked} onUnpick={() => setNodes((ns) => ns.map((n) => (n.selected ? { ...n, selected: false } : n)))} onClose={() => setBarOpen(false)} onFull={onFull} />}
          <BarButton path={scope} open={barOpen} onClick={() => setBarOpen(!barOpen)} />
        </Panel>
      )}
      <Panel position="top-left" className="canvas-dock">
        {/* a vertical strip of icons on the left (Illustrator-like), each named by its tooltip; what it opens drops to
            its right. The Maestro's canvas: 워커 · 프롬프트 · 질문 │ 블록 · 관계 │ 문서 · 플레이북 │ ?; any other agent's has no
            워커 or 플레이북 (no name here: the tab above already says which canvas this is) */}
        <ToolStrip>
          {hub && (
            <button className="ct-icon" title="새 워커 · 이 Lead 아래에 에이전트를 만들어요" aria-label="새 워커" onClick={() => onNewAgent({ parentPath: scope })}>
              <Bot size={16} />
            </button>
          )}
          <button className="ct-icon" title="프롬프트 블록 · 선으로 이은 에이전트(워커 카드 포함)에게 같은 지시를 보내요" aria-label="프롬프트 블록" onClick={() => addPrompt()}>
            <MessageSquarePlus size={16} />
          </button>
          <button className="ct-icon" title="질문 블록 · 에이전트가 묻는 질문과 같은 블록을 직접 놓아요. 에이전트가 알아서 읽지는 않아요 (블록을 고르고 입력창으로 말하면 읽어요)" aria-label="질문 블록" onClick={() => addQuestion()}>
            <MessageCircleQuestion size={16} />
          </button>
          <span className="canvas-tools-sep" />
          <button className="ct-icon" title="블록 · 구조 블록이나 영역을 더해요" aria-label="블록" data-on={panel === "blocks" || undefined} onClick={() => toggle("blocks")}>
            <SquarePlus size={16} />
          </button>
          <button
            className="ct-icon"
            title={`관계 · 선의 종류를 선언하고, 새로 긋는 선의 관계(펜)를 골라요${penRel.id !== "plain" ? ` · 펜: ${penRel.name}` : ""}`}
            aria-label="관계"
            data-on={panel === "rels" || undefined}
            onClick={() => toggle("rels")}
          >
            <Spline size={16} />
            {/* the pen, when it isn't 기본 관계: a dot in its colour */}
            {penRel.id !== "plain" && <i className="ct-dot" style={{ background: colorVar(penRel.color) }} />}
          </button>
          <span className="canvas-tools-sep" />
          <button className="ct-icon" title="문서 · 끌어와 참조 블록으로 놓아요 (파일은 복사하지 않아요)" aria-label="문서" data-on={panel === "docs" || undefined} onClick={() => toggle("docs")}>
            <FileText size={16} />
          </button>
          {hub && (
            <button
              className="ct-icon"
              title={`플레이북 · Lead가 따를 것 (고르지 않으면 스스로 골라요)${book ? ` · 지금: ${book.title}` : ""}`}
              aria-label="플레이북"
              data-on={panel === "books" || undefined}
              onClick={() => toggle("books")}
            >
              <BookOpen size={16} />
              {book && <i className="ct-dot" />}
            </button>
          )}
          <span className="canvas-tools-sep" />
          <Guide
            key={canvasKey}
            lines={[
              "블록의 점을 다른 블록으로 끌어 이어요 · 선택한 선의 끝을 끌면 연결점을 옮겨요",
              hub ? "에이전트 → 워커, 마크다운 → 에이전트(참조), 프롬프트 ↔ 에이전트" : "마크다운 → 에이전트(참조), 프롬프트 ↔ 에이전트",
              "에이전트에서 끈 선을 빈 곳에 놓으면 새 워커를 만들어요",
              "선을 누르고 ×나 Delete로 끊어요",
              "에이전트 블록은 오른쪽 아래 모서리, 구조 블록과 영역은 선택한 뒤 테두리를 끌어 크기를 바꿔요",
              "구조 블록은 더블클릭해서 이름과 메모를 고쳐요 · 영역은 더블클릭하면 이름, 단계·상태, 든 것을 봐요",
              "영역에 단계나 상태를 주면 든 것이 그때만 보여요: 머리의 ◀ ▶(단계)나 목록(상태)으로 바꾸고, 블록 우클릭 → 보이는 단계…로 정해요",
              "블록은 끌어서 영역에 넣고 끌어서 빼요: 놓을 때 마우스가 있는 영역에 들어가요 (끄는 동안 그 영역이 밝아져요 · 영역을 옮기거나 늘려서 덮는 것으로는 들어가지 않아요)",
              "영역은 안이 바뀌면 든 것에 맞게 다시 맞춰져요 (블록이 들어오거나 나가거나 움직이거나, 단계를 넘길 때) · 영역 우클릭 → 작게 접기 (접힌 영역은 더블클릭으로 펼쳐요)",
              "영역 안에서 블록 위에 걸쳐 놓으면 놓은 쪽(여덟 방향) 옆에 자리를 잡아 줘요 · 빈 자리에 놓으면 그 자리에 있어요",
              "블록을 다른 블록의 한가운데에 놓으면 묶음이 돼요 · 묶음 안에서 위아래로 끌어 순서를 바꾸고, 밖으로 끌어 빼요",
              "구조 블록과 영역은 우클릭 → 테두리 색으로 색을 줘요 (여럿을 고르고 하면 한 번에) · 같은 색으로 묶음을 나타내요",
              "선은 관계의 하나예요: 누르면 관계를 바꾸고, 더블클릭하면 라벨과 본문을 적어요 · 관계 도구에서 선의 종류를 선언하고 펜을 골라요",
              "이 에이전트의 머리를 더블클릭하면 전체 화면 · 접힌 블록을 누르면 아래 입력창이 열려요",
              ...(hub ? ["워커 카드를 더블클릭하면 그 에이전트로 들어가요 · 빈 곳 우클릭 → 트리로 다시 배치"] : []),
              ...(hub ? ["Lead와 워커는 '에이전트' 영역에 들어 있어요: 영역을 접으면 팀이 칩 하나로 줄고, 대화와 승인은 아래 입력창에서 해요"] : []),
              "빈 곳을 우클릭하면 그 자리에 블록을 더하고, 블록이나 선을 우클릭하면 편집·지우기 메뉴가 나와요",
              "에이전트가 묻는 질문은 노란 테두리의 질문 블록으로 와요: 보기를 고르거나 답을 적어 보내요 · 답하기 전에는 더블클릭으로 고쳐요",
              ...MENU_LINES,
            ]}
          />
        </ToolStrip>
        {!toolsShut && panel === "blocks" && (
          <div className="library nowheel nodrag">
            <div className="library-head">
              <b>블록</b>
              <span className="spacer" />
              <button className="icon" title="닫기" aria-label="닫기" onClick={() => setPanel(null)}><X size={16} /></button>
            </div>
            <div className="library-body book-list" role="menu" aria-label="더할 블록">
              {blockItems.map((it) => (
                <div
                  key={it.title}
                  className="book-item"
                  role="menuitem"
                  title={it.sub}
                  onClick={() => {
                    setPanel(null);
                    it.run();
                  }}
                >
                  <span className="book-check muted">{it.icon}</span>
                  <span className="book-text">
                    <b className="ellipsis">{it.title}</b>
                    <span>{it.sub}</span>
                  </span>
                  <span />
                </div>
              ))}
            </div>
          </div>
        )}
        {/* the documents drop down under the toolbar, every category folded */}
        {!toolsShut && panel === "docs" && <LibraryPanel onClose={() => setPanel(null)} />}
        {!toolsShut && panel === "rels" && <RelationPanel root={root} pen={penRel.id} onPen={setPen} onClose={() => setPanel(null)} counts={relCounts} />}
        {!toolsShut && panel === "books" && (
          <PlaybookPanel
            value={book}
            onPick={(b) => {
              setBook(b);
              if (!barOpen) setBarOpen(true); // the pick is used from the chat box: bring it up
            }}
            onClose={() => setPanel(null)}
          />
        )}
        {/* an agent's question the user changes is marked: its answer then tells the agent what was really answered */}
        {qEditNode && <QuestionEditor data={qEditNode.data} onChange={(d) => updateNodeData(qEditNode.id, qEditNode.data.mine ? d : { ...d, edited: true })} onClose={() => setQEditing(null)} />}
        {editNode && (
          <StructEditor
            node={editNode}
            onChange={(d) => updateNodeData(editNode.id, d)}
            onClose={() => setEditing(null)}
            onMerge={mergeable(editNode.id) ? () => merge(editNode.id) : undefined}
            // an area's editor lists what it holds (zones.md): a row pressed picks that block on the canvas (a folded
            // area opens first, or the block couldn't be seen)
            held={editNode.type === "zone" ? nodes.filter((c) => c.parentId === editNode.id).map((c) => ({ id: c.id, type: c.type ?? "", name: (c.type === "question_block" ? c.data.title : titleOf(c.id)) || "이름 없음", when: whenLabel(editNode, c) })) : undefined}
            // its steps or states, changed in the editor (zones.ts); a new one gets its id here
            onPhases={
              editNode.type === "zone"
                ? (o) => {
                    const fresh = { id: `p_${Date.now()}`, name: "" };
                    setNodes((ns) =>
                      o.op === "mode" ? setPhaseMode(ns, editNode.id, o.mode, fresh)
                      : o.op === "add" ? addPhase(ns, editNode.id, fresh)
                      : o.op === "rename" ? renamePhase(ns, editNode.id, o.id, o.name)
                      : o.op === "remove" ? removePhase(ns, editNode.id, o.id)
                      : goPhase(ns, editNode.id, o.id),
                    );
                  }
                : undefined
            }
            onPick={(id) => {
              if (editNode.type === "zone" && editNode.data.folded) foldArea(editNode.id, false);
              setNodes((ns) => ns.map((n) => (!!n.selected === (n.id === id) ? n : { ...n, selected: n.id === id })));
              setEditing(null);
            }}
            onFit={editNode.type === "zone" && !editNode.data.folded ? () => setNodes((ns) => fitZone(ns, editNode.id, ZONE_METRICS, ZONE_SIZE)) : undefined}
          />
        )}
        {editLine && <LineEditor edge={editLine} onChange={(d) => updateEdgeData(editLine.id, d)} onClose={() => setLineEditing(null)} />}
      </Panel>
    </ReactFlow>
    </LineCtx.Provider>
  );
}
