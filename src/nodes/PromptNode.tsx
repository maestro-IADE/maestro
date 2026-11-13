import { useState } from "react";
import { isImeComposing } from "../ime";
import { Handle, Position, useEdges, useNodes, useReactFlow, type NodeProps } from "@xyflow/react";
import { ipc } from "../ipc";
import { X } from "../icons";
import { DRAG_MIME } from "../RightPanel";
import { useStore } from "../store";
import type { CanvasNode, PromptFlowNode } from "../types";

export function PromptNode({ id, data }: NodeProps<PromptFlowNode>) {
  const { updateNodeData } = useReactFlow<CanvasNode>();
  const edges = useEdges();
  const nodes = useNodes<CanvasNode>();
  const worktrees = useStore((s) => s.worktrees);
  const [text, setText] = useState("");
  const [err, setErr] = useState("");
  const [over, setOver] = useState(false);
  const attached = data.attachedContextPaths ?? [];

  // command_pipe edges are the source of truth for bindings (PRD 4.2)
  const targets = edges
    .filter((e) => e.source === id)
    .map((e) => nodes.find((n) => n.id === e.target))
    // an agent block or a worker card (on the Lead canvas the workers are cards)
    .flatMap((n) => (n?.type === "pty_terminal_node" || n?.type === "worktree_card" ? worktrees.filter((w) => w.path === n.data.worktreePath) : []));

  const [busy, setBusy] = useState(false);
  const send = async () => {
    if (!text.trim() || busy) return;
    if (!targets.length) return setErr("에이전트에 선을 이어 주세요");
    setBusy(true);
    try {
      // stopped agents are resumed by the backend on send
      await Promise.all(targets.map((w) => ipc.send(w.root, w.path, text, attached)));
      updateNodeData(id, { lastSentPrompt: text });
      setText("");
      setErr("");
    } catch (e) {
      setErr(String(e));
    }
    setBusy(false);
  };

  return (
    <div
      className="node node-prompt"
      data-over={over || undefined}
      onDragOver={(e) => {
        if (!e.dataTransfer.types.includes(DRAG_MIME)) return;
        e.preventDefault();
        setOver(true);
      }}
      onDragLeave={(e) => !e.currentTarget.contains(e.relatedTarget as Node) && setOver(false)} // not when crossing into a child
      onDrop={(e) => {
        setOver(false);
        e.stopPropagation(); // the file is this block's: the canvas doesn't also place it
        const rel = e.dataTransfer.getData(DRAG_MIME);
        if (rel && !attached.includes(rel)) updateNodeData(id, { attachedContextPaths: [...attached, rel] });
      }}
    >
      <header>
        <b className="ellipsis" title={data.label}>{data.label}</b>
        <span className="spacer" />
        <span className="muted small">{targets.length ? `보낼 곳 ${targets.length}` : "연결 없음"}</span>
      </header>
      {!!attached.length && (
        <div className="chips">
          {attached.map((p) => (
            <span key={p} className="chip" title={p}>
              {p.split("/").pop()}
              <button className="nodrag" title="빼기" aria-label={`${p.split("/").pop()} 빼기`} onClick={() => updateNodeData(id, { attachedContextPaths: attached.filter((x) => x !== p) })}><X size={12} /></button>
            </span>
          ))}
        </div>
      )}
      <textarea
        className="nodrag nowheel"
        rows={4}
        autoFocus={!!data.focus}
        readOnly={busy}
        value={text}
        placeholder="지시 입력 · Enter 보내기 · Shift+Enter 줄바꿈 · 파일을 끌어와 첨부"
        onChange={(e) => setText(e.target.value)}
        onKeyDown={(e) => {
          // Enter sends, Shift+Enter breaks the line: the same as every other input on the canvas
          if (e.key === "Enter" && !e.shiftKey && !isImeComposing(e)) {
            e.preventDefault();
            send();
          }
        }}
      />
      <footer>
        {err ? <span className="err">{err}</span> : <span className="muted">{data.lastSentPrompt ? `최근: ${data.lastSentPrompt.slice(0, 40)}` : ""}</span>}
        <button className="primary nodrag" disabled={busy || !text.trim()} onClick={send}>{busy ? "보내는 중…" : "보내기"}</button>
      </footer>
      <Handle type="source" position={Position.Right} className="wt-handle" />
    </div>
  );
}
