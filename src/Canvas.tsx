import { useCallback, useEffect, useRef, useState } from "react";
import {
  BaseEdge,
  EdgeLabelRenderer,
  getBezierPath,
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
  type IsValidConnection,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import { useShallow } from "zustand/react/shallow";
import { ipc, type DocFile } from "./ipc";
import { PromptNode } from "./nodes/PromptNode";
import { AgentBar, AgentNode, BarButton, CanvasBackground, barOpen as openBars, COMPACT_SIZE, GRID, TERM_SIZES, clampAgentSize } from "./nodes/AgentNode";
import { CARD_COMPACT, CARD_SIZE, WorktreeCard } from "./nodes/WorktreeCard";
import { MD_SIZE, MarkdownBlock } from "./nodes/MarkdownBlock";
import { ArrowEdge, BLOCK_SIZE, StructBlock, StructEditor, ZONE_SIZE, ZONE_Z, ZoneFrame } from "./nodes/StructNodes";
import { LibraryPanel, MD_MIME, PlaybookPanel } from "./LibraryPanel";
import { listen } from "@tauri-apps/api/event";
import { applyDrawing } from "./drawing";
import { useStore, type Worktree } from "./store";
import { ViewMap } from "./ViewMap";
import { isLightTheme, motionMs, useSettings } from "./settings";
import { BookOpen, ChevronDown, CircleHelp, FileText, Minus, Plus, Scan, X } from "./icons";
import { openMenu } from "./ui/Menu";
import type { CanvasEdge, CanvasNode, CanvasStateSchema, Picked, StructNode, ZoneNode } from "./types";

const nodeTypes = { prompt_input_primitive: PromptNode, pty_terminal_node: AgentNode, worktree_card: WorktreeCard, markdown_block: MarkdownBlock, struct_block: StructBlock, zone: ZoneFrame };

/** A hierarchy or reference line. Selecting it shows × to cut the link (same path as the Delete key). */
function LinkEdge({ id, sourceX, sourceY, targetX, targetY, sourcePosition, targetPosition, markerEnd, style, selected }: EdgeProps) {
  const [path, x, y] = getBezierPath({ sourceX, sourceY, sourcePosition, targetX, targetY, targetPosition });
  const { deleteElements } = useReactFlow();
  return (
    <>
      {/* wide invisible hit area: a thin line is hard to click */}
      <BaseEdge path={path} markerEnd={markerEnd} style={style} interactionWidth={36} />
      {selected && (
        <EdgeLabelRenderer>
          <button
            className="edge-cut nodrag nopan"
            style={{ transform: `translate(-50%, -50%) translate(${x}px, ${y}px)` }}
            title="연결 끊기"
            aria-label="연결 끊기"
            onClick={() => deleteElements({ edges: [{ id }] })}
          >
            <X size={12} />
          </button>
        </EdgeLabelRenderer>
      )}
    </>
  );
}
const edgeTypes = { link: LinkEdge, arrow: ArrowEdge };
// every line points from its source to its target
export const ARROW = { type: MarkerType.ArrowClosed, width: 16, height: 16 };
export const MIN_ZOOM = 0.15;
const MULTI_SELECT = ["Shift", "Control", "Meta"];
// the playbook picked on each canvas (by `root|scope`), kept while the app runs
const pickedBooks = new Map<string, DocFile | null>();
const MAX_ZOOM = 2;
const snap = (v: number) => Math.round(v / GRID) * GRID;

// Older canvases may hold free-form sizes/positions; pull them onto the grid and size presets.
const normalize = (n: CanvasNode): CanvasNode => {
  const position = { x: snap(n.position.x), y: snap(n.position.y) };
  if (n.type === "pty_terminal_node") {
    // a dragged size is kept (within limits); older canvases only have a preset
    if (n.data.compact) return { ...n, position, ...COMPACT_SIZE, deletable: false };
    const preset = TERM_SIZES[n.data.size ?? "M"];
    return { ...n, position, ...clampAgentSize(n.width ?? preset.width, n.height ?? preset.height), deletable: false };
  }
  if (n.type === "worktree_card") return { ...n, position, ...(n.data.compact ? { width: CARD_COMPACT.width, height: n.height ?? CARD_COMPACT.height } : CARD_SIZE), deletable: false };
  if (n.type === "markdown_block") return { ...n, position, ...MD_SIZE };
  if (n.type === "struct_block") return { ...n, position, width: n.width ?? BLOCK_SIZE.width, height: n.height ?? BLOCK_SIZE.height };
  if (n.type === "zone") return { ...n, position, width: n.width ?? ZONE_SIZE.width, height: n.height ?? ZONE_SIZE.height, zIndex: ZONE_Z };
  return { ...n, position, width: undefined, height: undefined };
};

const folderName = (path: string) => path.split(/[\\/]/).pop() ?? path;

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

/** What every canvas's `?` ends with: where the controls that moved into menus are (docs/prd stage 11). */
export const MENU_LINES = ["우클릭과 ⋯로 메뉴를 열어요", "프로젝트 분할은 프로젝트 메뉴 → 분할로 열기"];

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

/** Which sides of two cards face each other, so a line leaves and enters on the near sides. */
function facingSides(a?: { position: { x: number; y: number } }, b?: { position: { x: number; y: number } }): [string, string] {
  if (!a || !b) return ["bottom", "top"];
  const dx = b.position.x - a.position.x, dy = b.position.y - a.position.y;
  if (Math.abs(dy) >= Math.abs(dx)) return dy >= 0 ? ["bottom", "top"] : ["top", "bottom"];
  return dx >= 0 ? ["right", "left"] : ["left", "right"];
}

/** Bottom-left zoom bar: − zoom% + | fit. Clicking the percentage resets to 100%. (Settings: the top bar.) */
export function CanvasControls() {
  const zoom = useFlowStore((s) => s.transform[2]);
  const { zoomIn, zoomOut, zoomTo, fitView } = useReactFlow();
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
export function Canvas({ root, scope, onNewAgent, onFull }: {
  root: string;
  scope: string;
  onNewAgent: (opts?: { parentPath?: string }) => void;
  onFull?: () => void; // this agent in full screen instead of its canvas
}) {
  const [nodes, setNodes, onNodesChange] = useNodesState<CanvasNode>([]);
  const [edges, setEdges, onEdgesChange] = useEdgesState<CanvasEdge>([]);
  const { getViewport, setViewport, screenToFlowPosition, fitView } = useReactFlow();
  const needsFit = useRef(false); // no saved viewport yet → frame the blocks once they exist
  const untreed = useRef(false); // no saved layout, or one from before the tree layout
  const hub = useStore((s) => s.worktrees.some((w) => w.root === root && w.isMaestro && w.path === scope));
  const worktrees = useStore(useShallow((s) => s.worktrees.filter((w) => w.root === root && (hub || w.path === scope))));
  const select = useStore((s) => s.select);
  const setError = useStore((s) => s.setError);
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
      const state: CanvasStateSchema | null = saved ? JSON.parse(saved) : null;
      if (state) {
        setNodes((state.nodes as CanvasNode[]).map(normalize));
        setEdges(state.edges.map((e) => (e.data?.kind === "arrow" ? { ...e, type: "arrow", markerEnd: ARROW, data: { ...e.data, editing: false } } : { ...e, animated: true, markerEnd: ARROW })));
        setViewport(state.viewport);
      }
      needsFit.current = !state;
      untreed.current = !state?.tree;
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
      // first grid slot no block already covers (cards get moved around, so "the next index" may be taken)
      const taken = [...kept];
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
      const added = worktrees
        .filter((w) => !have.has(w.path))
        .map((w): CanvasNode => {
          const back = dropped.current.get(`${kind(w)}|${w.path}`);
          if (back) {
            dropped.current.delete(`${kind(w)}|${w.path}`);
            taken.push(back);
            return back;
          }
          if (kind(w) === "pty_terminal_node") {
            // on the Maestro canvas it keeps the card_ id, so lines and links treat it like any card
            const block: CanvasNode = { id: hub ? `card_${w.path}` : `term_${w.path}`, type: "pty_terminal_node", position: hub ? freeSlot() : { x: 360, y: 0 }, ...TERM_SIZES.M, deletable: false, data: { worktreePath: w.path, branchName: w.branch } };
            taken.push(block);
            return block;
          }
          const card: CanvasNode = {
            id: `card_${w.path}`,
            type: "worktree_card",
            position: freeSlot(),
            ...CARD_SIZE,
            deletable: false, // removed only by discarding the worktree
            data: { worktreePath: w.path },
          };
          taken.push(card);
          return card;
        });
      if (hub && added.length) justAdded.current = added[added.length - 1].position;
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
        if (p) setCenter(p.x + CARD_SIZE.width / 2, p.y + CARD_SIZE.height / 2, { zoom: getZoom(), duration: motionMs(useSettings.getState()) * 1.5 });
      });
    }
  }, [loadGen, wtKey]);

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
        nodes: nodes.map(({ id, type, position, width, height, data, parentId }) => ({ id, type, position, width, height, data, parentId }) as CanvasStateSchema["nodes"][number]),
        // Maestro lines aren't saved here: they're drawn from the Maestro's workers
        edges: edges
          .filter((e) => e.data?.kind !== "orchestrates")
          .map(({ id, source, target, sourceHandle, targetHandle, data }) => ({ id, source, target, sourceHandle, targetHandle, data: data && { ...data, editing: undefined } })),
      };
      ipc.saveCanvas(root, canvasKey, JSON.stringify(state, null, 2)).catch(setError);
      // each settled state is an undo step (selection and viewport alone don't count)
      const key = JSON.stringify([state.nodes, state.edges]);
      const h = history.current;
      if (h.at < 0 || h.steps[h.at].key !== key) {
        h.steps = [...h.steps.slice(0, h.at + 1), { key, nodes, edges }].slice(-100);
        h.at = h.steps.length - 1;
      }
    }, 500);
    return () => clearTimeout(t);
  }, [loaded, nodes, edges, viewTick]);

  // Ctrl+Z / Ctrl+Shift+Z (Ctrl+Y): steps back and forth through this canvas's blocks and lines.
  // Links kept by the backend (workers, references) are drawn from it, so they aren't undone here.
  const history = useRef<{ steps: { key: string; nodes: CanvasNode[]; edges: CanvasEdge[] }[]; at: number }>({ steps: [], at: -1 });
  const focused = useRef(false); // the last click landed on this canvas (several can be open in splits)
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
      setNodes(step.nodes);
      setLoadGen((g) => g + 1); // the worktree sync puts back an agent block or card the older step lacks
      // Maestro lines come from the backend: keep the current ones
      setEdges((es) => [...es.filter((x) => x.data?.kind === "orchestrates"), ...step.edges.filter((x) => x.data?.kind !== "orchestrates")]);
    };
    window.addEventListener("pointerdown", onDown, true);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("pointerdown", onDown, true);
      window.removeEventListener("keydown", onKey);
    };
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
  const arrowsFrom = (id: string) => edges.filter((e) => e.data?.kind === "arrow" && e.source === id).map((e) => titleOf(e.target) + (e.data?.label ? ` (${e.data.label})` : "")).filter(Boolean);
  const picked: Picked[] = nodes.flatMap((n): Picked[] => {
    if (!n.selected) return [];
    if (n.type === "struct_block") return [{ kind: "block", title: n.data.title || "이름 없음", body: n.data.body ?? "", area: titleOf(n.parentId) || undefined, to: arrowsFrom(n.id) }];
    if (n.type === "zone") return [{ kind: "area", title: n.data.title || "영역", holds: nodes.filter((c) => c.parentId === n.id).map((c) => titleOf(c.id)).filter(Boolean), to: arrowsFrom(n.id) }];
    if (n.type === "markdown_block") return [{ kind: "document", title: n.data.title, path: n.data.path }];
    if (n.type === "prompt_input_primitive") return [{ kind: "prompt", name: n.data.label }];
    const w = worktrees.find((w) => w.path === pathOf(n));
    return w && w.path !== scope ? [{ kind: "agent", name: w.name, branch: w.branch, path: w.path }] : [];
  });
  const pickedKey = JSON.stringify(picked);
  useEffect(() => void ipc.setCanvasSelection(scope, picked).catch(() => {}), [pickedKey, scope]);
  useEffect(() => () => void ipc.setCanvasSelection(scope, []).catch(() => {}), [scope]);
  const boxing = useRef(false); // a selection box is being dragged
  const promptCount = useRef(0);
  const addPrompt = () => {
    const s = screenToFlowPosition({ x: 420, y: 160 });
    const p = { x: snap(s.x), y: snap(s.y) };
    promptCount.current = nodes.filter((n) => n.type === "prompt_input_primitive").length + 1;
    setNodes((ns) => [
      ...ns,
      { id: `prompt_${Date.now()}`, type: "prompt_input_primitive", position: p, data: { label: `프롬프트 ${promptCount.current}`, attachedContextPaths: [] } },
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
  const addStruct = (type: "struct_block" | "zone") => {
    const id = `${type === "zone" ? "zone" : "block"}_${Date.now()}`;
    const size = type === "zone" ? ZONE_SIZE : BLOCK_SIZE;
    const n = (type === "zone"
      ? { id, type, position: viewMiddle(size), ...size, zIndex: ZONE_Z, data: { title: "" }, selected: true }
      : { id, type, position: viewMiddle(size), ...size, data: { title: "" }, selected: true }) as CanvasNode;
    // areas first, so they lie under what's already there; the new item alone is selected
    setNodes((ns) => (type === "zone" ? [n, ...ns.map((x) => ({ ...x, selected: false }))] : [...ns.map((x) => ({ ...x, selected: false })), n]));
    setEditing(id); // name it right away
  };
  const nestable = (n: CanvasNode) => n.type === "struct_block" || n.type === "zone" || n.type === "markdown_block";
  const absOf = (id: string) => getInternalNode(id)?.internals.positionAbsolute ?? { x: 0, y: 0 };
  const sizeOf = (n: CanvasNode) => ({ w: n.measured?.width ?? n.width ?? 0, h: n.measured?.height ?? n.height ?? 0 });
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
  // a block or area let go over an area goes inside it (the innermost one under its middle); let go outside, it comes out
  const nest = (dragged: CanvasNode[]) =>
    setNodes((ns) => {
      let next = ns;
      for (const d of dragged.filter(nestable)) {
        const byId = new Map(next.map((n) => [n.id, n]));
        const inside = (z: CanvasNode) => {
          for (let p: string | undefined = z.id; p; p = byId.get(p)?.parentId) if (p === d.id) return true;
          return false;
        };
        const a = absOf(d.id), s = sizeOf(d);
        const cx = a.x + s.w / 2, cy = a.y + s.h / 2;
        const host = next
          .filter((z) => z.type === "zone" && !inside(z))
          .filter((z) => {
            const za = absOf(z.id), zs = sizeOf(z);
            return cx > za.x && cx < za.x + zs.w && cy > za.y && cy < za.y + zs.h;
          })
          .sort((p, q) => sizeOf(p).w * sizeOf(p).h - sizeOf(q).w * sizeOf(q).h)[0];
        if ((host?.id ?? undefined) === (d.parentId ?? undefined)) continue;
        const base = host ? absOf(host.id) : { x: 0, y: 0 };
        next = next.map((n) => (n.id === d.id ? { ...n, parentId: host?.id, position: { x: snap(a.x - base.x), y: snap(a.y - base.y) } } : n));
      }
      return next === ns ? ns : parentsFirst(next);
    });
  // deleting an area keeps what's inside it: React Flow would take the children along, so they move up a level first
  const onBeforeDelete = async ({ nodes: del, edges: delEdges }: { nodes: CanvasNode[]; edges: CanvasEdge[] }) => {
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
  useEffect(() => {
    if (!loaded) return;
    const take = () =>
      ipc.takeCanvasDrawings(scope).then((list) => {
        if (!list.length) return;
        let { nodes: ns, edges: es } = live.current;
        for (const d of list) ({ nodes: ns, edges: es } = applyDrawing(d, ns, es));
        setNodes(ns);
        setEdges(es);
        requestAnimationFrame(() => fitView({ padding: 0.15, maxZoom: 1, duration: motionMs(useSettings.getState()) * 1.5 }));
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
  const linkFor = (a: string | null, b: string | null): { kind: "child"; parent: string; child: string } | { kind: "ref"; card: string; md: string } | null => {
    const [ca, cb, ma, mb] = [cardOf(a), cardOf(b), mdOf(a), mdOf(b)];
    if (ca && cb) {
      // drag direction is hierarchy direction: from the parent to its worker
      if (cb.isMaestro || ca.workers.some((p) => same(p, cb.path))) return null;
      return { kind: "child", parent: ca.path, child: cb.path };
    }
    const card = ca ?? cb, md = ma ?? mb;
    if (card && md && !card.refs.some((r) => same(r, md.data.path))) return { kind: "ref", card: card.path, md: md.data.path };
    return null;
  };
  const [layoutTick, setLayoutTick] = useState(0);
  const linkKey = worktrees.map((w) => `${w.workers.join(",")}|${w.refs.join(",")}`).join("\n");
  useEffect(() => {
    if (!loaded) return;
    setEdges((es) => {
      const others = es.filter((e) => e.data?.kind !== "orchestrates" && e.data?.kind !== "reference");
      const line = (id: string, source: string, target: string, kind: "orchestrates" | "reference"): CanvasEdge => {
        const [sourceHandle, targetHandle] = facingSides(getNode(source), getNode(target));
        return { id, type: "link", source, target, sourceHandle, targetHandle, className: kind === "reference" ? "ref" : "orch", animated: kind === "orchestrates", markerEnd: ARROW, data: { kind } };
      };
      const lines: CanvasEdge[] = [];
      for (const w of worktrees) {
        // hierarchy lines only where the workers are on the canvas (the Maestro's)
        for (const child of hub ? worktrees.filter((c) => w.workers.some((p) => same(p, c.path))) : []) {
          lines.push(line(`orch_${w.path}>${child.path}`, idOf(w.path), idOf(child.path), "orchestrates"));
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
      return ns.map((n) => {
        const p = mirrors(n) && pos.get(pathOf(n));
        return p ? { ...n, position: p } : n;
      });
    });
    justAdded.current = null; // the fit below shows the new card; no separate pan
    needsFit.current = false;
    requestAnimationFrame(() => {
      setLayoutTick((t) => t + 1); // lines pick their sides from the new positions
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
    if (justLoaded ? !untreed.current : b?.hier === hierKey) return;
    untreed.current = false;
    arrange(!justLoaded);
  }, [loadGen, hierKey]);

  // Markdown dropped from the library or the file tree → a Markdown block where it landed
  // what drops down under the island: the documents, or (on a Maestro's canvas) the playbooks
  const [panel, setPanel] = useState<"docs" | "books" | null>(null);
  const toggle = (p: "docs" | "books") => setPanel(panel === p ? null : p);
  // + 블록 ▾: a structure block or an area, from one menu
  const blockMenu = (e: React.MouseEvent<HTMLButtonElement>) =>
    openMenu(e.currentTarget, [
      { label: "구조 블록", onSelect: () => addStruct("struct_block") },
      { label: "영역", onSelect: () => addStruct("zone") },
    ]);
  // the playbook the Maestro is told to follow (null = it picks), kept per canvas for the session
  const [book, setBookState] = useState(() => pickedBooks.get(barKey) ?? null);
  const setBook = (b: DocFile | null) => {
    pickedBooks.set(barKey, b);
    setBookState(b);
  };
  const onDragOver = (e: React.DragEvent) => {
    if (!e.dataTransfer.types.includes(MD_MIME)) return;
    e.preventDefault();
    e.dataTransfer.dropEffect = "copy";
  };
  const onDrop = async (e: React.DragEvent) => {
    const raw = e.dataTransfer.getData(MD_MIME);
    if (!raw) return;
    e.preventDefault();
    const file = JSON.parse(raw) as { path: string; title: string };
    const at = screenToFlowPosition({ x: e.clientX, y: e.clientY });
    const title = file.title || (await ipc.mdTitle(file.path).catch(() => file.path.split("/").pop() ?? file.path));
    setNodes((ns) => [
      ...ns,
      {
        id: `md_${Date.now()}`,
        type: "markdown_block",
        position: { x: snap(at.x - MD_SIZE.width / 2), y: snap(at.y - MD_SIZE.height / 2) },
        ...MD_SIZE,
        data: { path: file.path, title },
      },
    ]);
  };

  // prompt → agent: a pipe kept with the canvas; everything else is a hierarchy or reference link (agent settings)
  const drawn = (id: string | null) => {
    const t = nodes.find((n) => n.id === id)?.type;
    return t === "struct_block" || t === "zone";
  };
  // an arrow: either end is a structure block or an area
  const isArrow = (c: Connection | { source: string; target: string }) => c.source !== c.target && (drawn(c.source) || drawn(c.target));
  const isPipe = (c: Connection | { source: string; target: string }) =>
    nodes.find((n) => n.id === c.source)?.type === "prompt_input_primitive" && nodes.find((n) => n.id === c.target)?.type === "pty_terminal_node";
  const onConnect = useCallback(
    (c: Connection) => {
      if (isArrow(c)) return setEdges((es) => addEdge({ ...c, id: `arrow_${Date.now()}`, type: "arrow", markerEnd: ARROW, data: { kind: "arrow" as const } }, es));
      if (isPipe(c)) return setEdges((es) => addEdge({ ...c, animated: true, markerEnd: ARROW, data: { kind: "command_pipe" as const } }, es));
      const l = linkFor(c.source, c.target);
      if (l?.kind === "child") act(ipc.linkWorktrees(root, l.parent, l.child));
      if (l?.kind === "ref") act(ipc.setRef(root, l.card, l.md, true));
    },
    [setEdges, worktrees, nodes],
  );
  const isValidConnection: IsValidConnection = (c) => isArrow(c) || isPipe(c) || !!linkFor(c.source, c.target);

  return (
    <ReactFlow
      nodes={nodes}
      edges={edges}
      nodeTypes={nodeTypes}
      edgeTypes={edgeTypes}
      onNodesChange={onNodesChange}
      // a selection box takes blocks only: lines are picked by clicking them
      onEdgesChange={(cs) => onEdgesChange(boxing.current ? cs.filter((c) => c.type !== "select") : cs)}
      onSelectionStart={() => (boxing.current = true)}
      onSelectionEnd={() => (boxing.current = false)}
      onConnect={onConnect}
      isValidConnection={isValidConnection}
      connectionMode={ConnectionMode.Loose}
      onDragOver={onDragOver}
      onDrop={onDrop}
      // a line from a card dropped on empty canvas → a new worktree as that card's worker
      onConnectEnd={(_, cs) => {
        const from = cardOf(cs.fromNode?.id ?? null);
        if (from && !cs.toNode) onNewAgent({ parentPath: from.path });
      }}
      onEdgesDelete={(gone) => {
        for (const e of gone) {
          const card = cardOf(e.target);
          const parent = cardOf(e.source);
          if (e.data?.kind === "orchestrates" && card && parent) act(ipc.unlinkWorktrees(root, parent.path, card.path));
          const md = mdOf(e.source);
          if (e.data?.kind === "reference" && card && md) act(ipc.setRef(root, card.path, md.data.path, false));
        }
      }}
      onNodesDelete={(gone) => {
        // the last block of a Markdown file left this canvas → the agents here no longer refer to it
        for (const n of gone.filter((g) => g.type === "markdown_block")) {
          const path = (n.data as { path: string }).path;
          const left = nodes.some((o) => o.id !== n.id && o.type === "markdown_block" && same((o.data as { path: string }).path, path));
          if (left) continue;
          for (const w of worktrees.filter((w) => w.refs.some((r) => same(r, path)))) act(ipc.setRef(root, w.path, path, false));
        }
      }}
      onNodeDragStop={(_, _node, dragged) => {
        setLayoutTick((t) => t + 1);
        nest(dragged);
      }}
      onBeforeDelete={onBeforeDelete}
      onEdgeDoubleClick={(_, e) => e.data?.kind === "arrow" && updateEdgeData(e.id, { editing: true })}
      onNodeClick={(_, n) => n.type === "pty_terminal_node" && n.data.compact && pathOf(n) === scope && setBarOpen(!barOpen)}
      onNodeDoubleClick={(e, n) => {
        if (n.type === "worktree_card") select(n.data.worktreePath, true);
        else if (n.type === "struct_block" || n.type === "zone") setEditing(n.id);
        // this canvas's agent block, by its header or folded chip (not inside the chat, where a double click selects a word)
        else if (n.type === "pty_terminal_node" && pathOf(n) === scope && (e.target as HTMLElement).closest(".node-agent > header, .agent-chip")) onFull?.();
      }}
      zoomOnDoubleClick={false}
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
      deleteKeyCode={["Delete"]}
      colorMode={light ? "light" : "dark"}
      proOptions={{ hideAttribution: true }}
    >
      <CanvasBackground />
      <CanvasControls />
      <ViewMap nodes={nodes} />
      {/* this canvas's own agent, spoken to from the bottom: the logo button (or its folded block) opens and closes it */}
      <Panel position="bottom-center" className="agent-bar-panel">
        {barOpen && <AgentBar key={scope} path={scope} picked={picked} onUnpick={() => setNodes((ns) => ns.map((n) => (n.selected ? { ...n, selected: false } : n)))} onClose={() => setBarOpen(false)} onFull={onFull} book={self?.isMaestro ? book : null} onBook={setBook} />}
        <BarButton path={scope} open={barOpen} onClick={() => setBarOpen(!barOpen)} />
      </Panel>
      <Panel position="top-left">
        {/* the Maestro's canvas: 워커 · 프롬프트 │ 블록 ▾ │ 문서 · 플레이북 │ ?; any other agent's: 프롬프트 │ 블록 ▾ │ 문서 │ ?
            (no name here: the tab above already says which canvas this is; a worker's workers come from its ⋯ menus) */}
        <div className="canvas-tools">
          {hub && (
            <button className="primary" title="새 워커 · 이 Lead 아래에 에이전트를 만들어요" onClick={() => onNewAgent({ parentPath: scope })}>
              <Plus size={14} />워커
            </button>
          )}
          <button title="프롬프트 블록 · 선으로 이은 에이전트에게 같은 지시를 보내요" onClick={addPrompt}>
            <Plus size={14} />프롬프트
          </button>
          <span className="canvas-tools-sep" />
          <button title="구조 블록이나 영역을 더해요" aria-haspopup="menu" onClick={blockMenu}>
            <Plus size={14} />블록<ChevronDown size={12} className="ct-caret" />
          </button>
          <span className="canvas-tools-sep" />
          <button title="문서 · 끌어와 참조 블록으로 놓아요 (파일은 복사하지 않아요)" data-on={panel === "docs" || undefined} onClick={() => toggle("docs")}>
            <FileText size={14} />문서
          </button>
          {hub && (
            <button title="플레이북 · Lead가 따를 것 (고르지 않으면 스스로 골라요)" data-on={panel === "books" || undefined} onClick={() => toggle("books")}>
              <BookOpen size={14} />플레이북
              {book && <span className="ct-sub ellipsis">· {book.title}</span>}
            </button>
          )}
          <span className="canvas-tools-sep" />
          <Guide
            key={canvasKey}
            lines={[
              "블록의 점을 다른 블록으로 끌어 이어요",
              hub ? "에이전트 → 워커, 마크다운 → 에이전트(참조), 프롬프트 → 에이전트" : "마크다운 → 에이전트(참조), 프롬프트 → 에이전트",
              "에이전트에서 끈 선을 빈 곳에 놓으면 새 워커를 만들어요",
              "선을 누르고 X나 Delete로 끊어요",
              "블록 오른쪽 아래 모서리를 끌어 크기를 바꿔요",
              ...(hub ? ["워커 카드를 더블클릭하면 그 에이전트로 들어가요"] : []),
              ...MENU_LINES,
            ]}
          />
        </div>
        {/* the documents drop down under the toolbar, every category folded */}
        {panel === "docs" && <LibraryPanel onClose={() => setPanel(null)} />}
        {panel === "books" && (
          <PlaybookPanel
            value={book}
            onPick={(b) => {
              setBook(b);
              if (!barOpen) setBarOpen(true); // the pick is used from the chat box: bring it up
            }}
            onClose={() => setPanel(null)}
          />
        )}
        {editNode && <StructEditor node={editNode} onChange={(d) => updateNodeData(editNode.id, d)} onClose={() => setEditing(null)} />}
      </Panel>
    </ReactFlow>
  );
}
