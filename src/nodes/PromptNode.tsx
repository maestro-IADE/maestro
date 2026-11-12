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
    .flatMap((n) => (n?.type === "pty_terminal_node" ? worktrees.filter((w) => w.path === n.data.worktreePath) : []));

  const send = async () => {
    if (!text.trim()) return;
    if (!targets.length) return setErr("에이전트 블록에 선을 연결하세요");
    try {
      // stopped agents are resumed by the backend on send
      await Promise.all(targets.map((w) => ipc.send(w.root, w.path, text, attached)));
      updateNodeData(id, { lastSentPrompt: text });
      setText("");
      setErr("");
    } catch (e) {
      setErr(String(e));
    }
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
        const rel = e.dataTransfer.getData(DRAG_MIME);
        if (rel && !attached.includes(rel)) updateNodeData(id, { attachedContextPaths: [...attached, rel] });
      }}
    >
      <header>
        <b className="ellipsis" title={data.label}>{data.label}</b>
        <span className="spacer" />
        <span className="muted small">{targets.length > 1 ? `동시 전송 · ${targets.length}` : targets.length ? "단독 전송" : "연결 없음"}</span>
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
        value={text}
        placeholder="지시사항 입력 · Ctrl+Enter 전송 · 파일을 끌어와 첨부"
        onChange={(e) => setText(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && (e.ctrlKey || e.metaKey) && !isImeComposing(e)) {
            e.preventDefault();
            send();
          }
        }}
      />
      <footer>
        {err ? <span className="err">{err}</span> : <span className="muted">{data.lastSentPrompt ? `최근: ${data.lastSentPrompt.slice(0, 40)}` : ""}</span>}
        <button className="primary" onClick={send}>전송</button>
      </footer>
      <Handle type="source" position={Position.Right} />
    </div>
  );
}
