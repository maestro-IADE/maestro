import { useState } from "react";
import { isImeComposing } from "../ime";
import { ipc } from "../ipc";
import { useStore } from "../store";
import { X } from "../icons";
import type { ChatAsk } from "../chatAsks";

/**
 * A choice an agent put in the chat (`maestro ask --in chat`, docs/features/agents/chat-choices.md): what it asks,
 * its options with a number each, and 직접 입력 as the last number, always there, for an answer that isn't among
 * them. One option to pick: pressing it answers. Several (`multi`): they are ticked, then sent. The answer goes to
 * the agent as a note and the card leaves; × takes it away without an answer (the user would rather just write).
 */
export function ChoiceCard({ path, ask }: { path: string; ask: ChatAsk }) {
  const [picked, setPicked] = useState<string[]>([]);
  const [typing, setTyping] = useState(!ask.choices.length);
  const [text, setText] = useState("");
  const [busy, setBusy] = useState(false);
  const send = (choices: string[], own: string) => {
    if (busy || (!choices.length && !own.trim())) return;
    setBusy(true);
    ipc
      .answerChatAsk(path, ask.id, choices, own.trim())
      .catch((e) => useStore.getState().setError(String(e)))
      .finally(() => setBusy(false));
  };
  const pick = (c: string) => (ask.multi ? setPicked((p) => (p.includes(c) ? p.filter((x) => x !== c) : [...p, c])) : send([c], ""));
  const own = ask.choices.length + 1;
  return (
    <div className="approval choice-card">
      <div className="approval-head">
        <span className="approval-badge">{ask.multi ? "선택 · 여러 개" : "선택"}</span>
        <b>{ask.title}</b>
        <span className="spacer" />
        <button className="icon" title="닫기 · 답하지 않고 치워요 (그냥 글로 답할 때)" aria-label="선택지 닫기" disabled={busy} onClick={() => void ipc.answerChatAsk(path, ask.id, [], "").catch(() => {})}>
          <X size={14} />
        </button>
      </div>
      {ask.body && <div className="choice-body">{ask.body}</div>}
      <div className="choice-list" role={ask.multi ? "group" : "list"}>
        {ask.choices.map((c, i) => (
          <button key={c} className="choice-item" disabled={busy} data-on={picked.includes(c) || undefined} aria-pressed={ask.multi ? picked.includes(c) : undefined} onClick={() => pick(c)}>
            <i>{i + 1}</i>
            <span>{c}</span>
          </button>
        ))}
        {ask.choices.length > 0 && (
          <button className="choice-item" disabled={busy} data-on={typing || undefined} aria-expanded={typing} onClick={() => setTyping(!typing)}>
            <i>{own}</i>
            <span>직접 입력</span>
          </button>
        )}
      </div>
      {typing && (
        <textarea
          className="choice-own nowheel"
          rows={2}
          autoFocus={ask.choices.length > 0}
          placeholder="보기에 없는 답을 적어요 (Enter로 보내고, Shift+Enter로 줄을 바꿔요)"
          value={text}
          disabled={busy}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={(e) => {
            if (e.key !== "Enter" || e.shiftKey || isImeComposing(e)) return;
            e.preventDefault();
            send(picked, text);
          }}
        />
      )}
      {(ask.multi || typing) && (
        <div className="approval-actions">
          <button className="primary" disabled={busy || (!picked.length && !text.trim())} onClick={() => send(picked, typing ? text : "")}>
            보내기
          </button>
          {ask.multi && picked.length > 0 && <span className="muted small">{picked.length}개 골랐어요</span>}
        </div>
      )}
    </div>
  );
}
