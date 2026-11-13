import { useEffect, useState } from "react";
import { Spinner } from "./LogoMotion";
import { DocIcon, PersonaIcon, Plus } from "./icons";
import { createPortal } from "react-dom";
import { ipc } from "./ipc";
import { Handle, Panel, Position, ReactFlow, useEdgesState, useNodesState, useReactFlow, type Edge, type Node, type NodeProps } from "@xyflow/react";
import { useShallow } from "zustand/react/shallow";
import { ARROW, CanvasControls, EDGE_COLOR, Guide, MENU_LINES, MIN_ZOOM, treeLayout } from "./Canvas";
import { EDGE_TYPE, useEdgeShape } from "./edgePath";
import { CanvasBackground } from "./nodes/AgentNode";
import { AgentLogo, agentLabel } from "./agents";
import { AgentGlyph, WorkerDots } from "./status";
import { useStore, useWorktree, type Worktree } from "./store";
import { isLightTheme, useSettings } from "./settings";

// 11 × 7 grid cells
const STRUCT_SIZE = { width: 14 * 24, height: 7 * 24 }; // 14 × 7 grid cells: as wide as a worker card
type StructNode = Node<{ worktreePath: string }, "structure_card">;
type Folder = Awaited<ReturnType<typeof ipc.libraryList>>[number];
const SHOWN = 3; // reference chips on a card before "+N"

/** One agent on the project map: who it is and what it refers to. Double-click goes in. */
function StructureCard({ data }: NodeProps<StructNode>) {
  const wt = useWorktree(data.worktreePath);
  const repoDefault = useStore((s) => s.repos.find((r) => r.root === wt?.root)?.defaultBranch);
  const [picking, setPicking] = useState(false);
  if (!wt) return null;
  const chips = [...wt.personas.map((title) => ({ title, persona: true })), ...wt.docs.map((title) => ({ title, persona: false }))];
  return (
    <div className="node struct-card" data-maestro={wt.isMaestro || undefined} title="더블클릭해서 들어가기">
      <Handle type="target" id="top" position={Position.Top} className="struct-handle" isConnectable={false} />
      <header>
        <AgentGlyph w={wt} />
        <b className="ellipsis" title={wt.name}>{wt.name}</b>
        <span className="spacer" />
        {wt.external && <span className="ext-badge">외부</span>}
        <span title={agentLabel(wt.adapterId)}><AgentLogo id={wt.adapterId} size={14} /></span>
      </header>
      <div className="struct-desc small">
        {wt.description || <span className="muted">{wt.isMaestro ? "워커에게 일을 나누는 Lead" : "설명 없음"}</span>}
      </div>
      <div className="struct-refs">
        {chips.slice(0, SHOWN).map((c) => (
          <span key={c.title} className={c.persona ? "persona-badge" : "doc-badge"} title={c.persona ? "페르소나" : "참조 문서"}>
            {c.persona ? <PersonaIcon /> : <DocIcon />}
            {c.title}
          </span>
        ))}
        <button
          className="ref-more nodrag"
          title="참조 문서 전체 보기 · 선택"
          onClick={() => setPicking(true)}
          onDoubleClick={(e) => e.stopPropagation()}
        >
          {chips.length > SHOWN ? `+${chips.length - SHOWN}` : "+"}
        </button>
      </div>
      {picking && createPortal(<RefPicker wt={wt} onClose={() => setPicking(false)} />, document.body)}
      <div className="struct-foot muted small">
        <span className="ellipsis" title={wt.isMaestro ? undefined : wt.branch}>
          {wt.isMaestro ? `기준 ${wt.baseBranch ?? repoDefault}${wt.mergeMode === "pr" ? " · PR" : ""}` : wt.branch}
          {wt.workers.length ? ` · 워커 ${wt.workers.length}` : ""}
        </span>
        {/* the Maestro's workers at a glance: their rows aren't on the map */}
        {wt.isMaestro && wt.workers.length > 0 && <WorkerDots paths={wt.workers} />}
      </div>
      <Handle type="source" id="bottom" position={Position.Bottom} className="struct-handle" isConnectable={false} />
    </div>
  );
}

/**
 * Every Markdown file this agent can refer to (the project's library and the personas), ticked where it
 * already does. Ticking one links it: the agent's canvas gets a Markdown block with a reference line.
 */
function RefPicker({ wt, onClose }: { wt: Worktree; onClose: () => void }) {
  const setError = useStore((s) => s.setError);
  const [folders, setFolders] = useState<Folder[] | null>(null);
  useEffect(() => void ipc.libraryList().then(setFolders).catch(setError), []);
  const on = (path: string) => wt.refs.some((r) => r.toLowerCase() === path.toLowerCase());
  const toggle = (path: string) =>
    ipc.setRef(wt.root, wt.path, path, !on(path)).then(() => useStore.getState().refreshWorktrees(wt.root)).catch(setError);
  // references made from the file tree, outside the library folders
  const listed = new Set((folders ?? []).flatMap((f) => f.files.map((x) => x.path.toLowerCase())));
  const others = wt.refs.filter((r) => !listed.has(r.toLowerCase()));
  const row = (path: string, title: string, sub: string) => (
    <label key={path} className="ref-row" title={path}>
      <input type="checkbox" checked={on(path)} onChange={() => toggle(path)} />
      <span className="ellipsis">{title}</span>
      <span className="muted small ellipsis">{sub}</span>
    </label>
  );
  return (
    <div className="modal-backdrop" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div className="modal ref-picker" onKeyDown={(e) => e.key === "Escape" && onClose()}>
        <h3>참조 문서 · {wt.name}</h3>
        <p className="muted small">고르면 이 에이전트의 캔버스에 마크다운 블록이 생기고 참조로 연결돼요. 페르소나는 그 역할을 맡게 해요.</p>
        <div className="ref-list">
          {!folders && <span className="muted"><Spinner /> 읽는 중…</span>}
          {folders?.map((f) => (
            <div key={f.folder}>
              <div className="ref-folder muted small">{f.persona ? "페르소나" : f.folder.split(/[\\/]/).pop()}</div>
              {f.files.map((x) => row(x.path, x.title, x.path.slice(f.folder.replace(/\\/g, "/").length + 1)))}
              {!f.files.length && <div className="muted small ref-row">마크다운 없음</div>}
            </div>
          ))}
          {others.length > 0 && (
            <div>
              <div className="ref-folder muted small">프로젝트 파일 (파일 트리에서 끌어온 것)</div>
              {others.map((p) => row(p, p.split("/").pop() ?? p, p))}
            </div>
          )}
          {folders && folders.every((f) => !f.files.length) && <p className="muted small">문서는 왼쪽 사이드바의 문서 탭에서 만들거나 가져와요</p>}
        </div>
        <div className="modal-actions">
          <button className="primary" autoFocus onClick={onClose}>닫기</button>
        </div>
      </div>
    </div>
  );
}

const nodeTypes = { structure_card: StructureCard };

/**
 * The project canvas: only the structure, always laid out as a tree (Maestro on top). Nothing is edited
 * here; links and references are made on the Maestro canvas. Double-click a card to go into it.
 */
export function ProjectMap({ root, onNewAgent, onNewMaestro }: {
  root: string;
  onNewAgent: () => void;
  onNewMaestro: () => void;
}) {
  const worktrees = useStore(useShallow((s) => s.worktrees.filter((w) => w.root === root)));
  const repoName = useStore((s) => s.repos.find((r) => r.root === root)?.name ?? "");
  const select = useStore((s) => s.select);
  const light = useSettings((s) => isLightTheme(s.theme)); // a custom palette can turn light without its id changing
  const { fitView } = useReactFlow();
  const [nodes, setNodes, onNodesChange] = useNodesState<StructNode>([]);
  const [edges, setEdges] = useEdgesState<Edge>([]);
  const maestro = worktrees.find((w) => w.isMaestro);

  // relaid only when the structure changes; each card reads its own details from the store
  const shape = worktrees.map((w) => `${w.path}>${w.workers.join(",")}`).join("\n");
  useEffect(() => {
    const pos = treeLayout(worktrees, () => STRUCT_SIZE);
    setNodes(worktrees.map((w) => ({ id: `s_${w.path}`, type: "structure_card", position: pos.get(w.path)!, ...STRUCT_SIZE, data: { worktreePath: w.path } })));
    const byPath = new Map(worktrees.map((w) => [w.path.toLowerCase(), w.path]));
    setEdges(
      worktrees.flatMap((w) =>
        w.workers.flatMap((c) => {
          const child = byPath.get(c.toLowerCase());
          return child ? [{ id: `${w.path}>${child}`, source: `s_${w.path}`, target: `s_${child}`, sourceHandle: "bottom", targetHandle: "top", className: "orch", markerEnd: ARROW }] : [];
        }),
      ),
    );
    requestAnimationFrame(() => fitView({ padding: 0.2, maxZoom: 1 }));
  }, [shape]);

  const edgeShape = useEdgeShape();
  return (
    <ReactFlow
      nodes={nodes}
      // the map's lines take the picked shape too (React Flow's own edge types)
      edges={edges.map((e) => ({ ...e, type: EDGE_TYPE[edgeShape] }))}
      nodeTypes={nodeTypes}
      // read-only lines: no hover or pointer (styles.css `.map-flow`), heads in the line colour
      className="map-flow"
      defaultMarkerColor={EDGE_COLOR}
      onNodesChange={onNodesChange}
      nodesDraggable={false}
      nodesConnectable={false}
      elementsSelectable={false}
      // a click handler, even an empty one, is what makes React Flow let the pointer reach a card that can't be
      // selected or dragged: without it the double click, the reference chip and the tooltips never fire
      onNodeClick={() => {}}
      onNodeDoubleClick={(_, n) => select(n.data.worktreePath, true)}
      disableKeyboardA11y
      zoomOnDoubleClick={false}
      minZoom={MIN_ZOOM}
      zoomOnScroll={false}
      panOnScroll
      panOnDrag={[0, 1]}
      onMouseDownCapture={(e) => e.button === 1 && e.preventDefault()}
      zoomActivationKeyCode="Control"
      colorMode={light ? "light" : "dark"}
      proOptions={{ hideAttribution: true }}
    >
      <CanvasBackground />
      <CanvasControls />
      <Panel position="top-left">
        {/* 새 에이전트 · Lead 열기 (or 만들기) │ ? */}
        <div className="canvas-tools">
          <button className="primary" title="새 에이전트 · 따로 된 작업 폴더와 브랜치에서 일해요" onClick={onNewAgent}>
            <Plus size={14} />새 에이전트
          </button>
          {maestro ? (
            // its own first view (the canvas by default: settings → 일반 → 열 때 화면)
            <button title="Lead 열기 · 워커들이 보이는 캔버스" onClick={() => select(maestro.path, true)}>Lead 열기</button>
          ) : (
            <button title="Lead 만들기 · 프로젝트 폴더에서 일하며 워커에게 일을 나눠요" onClick={onNewMaestro}>Lead 만들기</button>
          )}
          <span className="canvas-tools-sep" />
          <Guide
            key={root}
            lines={["프로젝트 구조만 보여 주는 지도예요", "카드를 더블클릭하면 그 에이전트로 들어가요", "연결과 참조는 Lead 캔버스에서 바꿔요", ...MENU_LINES]}
          />
        </div>
      </Panel>
      {!worktrees.length && (
        <div className="canvas-empty-center">
          <div className="map-empty">
            <b>아직 에이전트가 없어요</b>
            <p>에이전트마다 따로 된 작업 폴더(워크트리)와 브랜치에서 일해요</p>
            <div className="map-empty-actions">
              <button className="primary" onClick={onNewAgent}><Plus size={14} />새 에이전트</button>
              <button onClick={onNewMaestro}>Lead 만들기</button>
            </div>
          </div>
        </div>
      )}
    </ReactFlow>
  );
}
