import { useState } from "react";
import { createPortal } from "react-dom";
import { useReactFlow, type NodeProps } from "@xyflow/react";
import { isImeComposing } from "../ime";
import { ipc } from "../ipc";
import { Check, MessageCircleQuestion, X } from "../icons";
import { GRID } from "./AgentNode";
import type { CanvasNode, QuestionData, QuestionNode } from "../types";

export const QUESTION_WIDTH = 13 * GRID;

const when = (ms: number) => new Date(ms).toLocaleString(undefined, { month: "numeric", day: "numeric", hour: "2-digit", minute: "2-digit" });

/**
 * A question on the canvas (docs/features/canvas/question-block.md): asked by the canvas's agent (`maestro ask`), or
 * made by the user (`mine`: from the 블록 panel or a right click, its question and choices set in `QuestionEditor`).
 * Multiple choice: one pick (or several with `multi`), plus a free-text 기타 with `other`; no choices: free text.
 * 보내기 sends the answer to the agent (`ipc.answerQuestion`) and keeps it on the block, read-only: the choices not
 * picked stay, dimmed, as the decision's record.
 */
export function QuestionBlock({ id, data }: NodeProps<QuestionNode>) {
  const { updateNodeData } = useReactFlow<CanvasNode>();
  const choices = data.choices ?? [];
  const answered = data.answer;
  const [picked, setPicked] = useState<string[]>([]);
  const [text, setText] = useState("");
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState("");
  const free = !choices.length;
  const ready = !!picked.length || !!text.trim();

  const pick = (c: string) => setPicked((p) => (data.multi ? (p.includes(c) ? p.filter((x) => x !== c) : [...p, c]) : p[0] === c ? [] : [c]));
  const send = async () => {
    if (!ready || busy || answered) return;
    setBusy(true);
    try {
      // picked in the order the choices are listed, not the order they were pressed
      const list = choices.filter((c) => picked.includes(c));
      await ipc.answerQuestion(data.owner, { id: data.qid, title: data.title, made: !!data.mine, body: data.body ?? "", choices }, list, text);
      updateNodeData(id, { answer: { picked: list, text: text.trim() || undefined, at: Date.now() } });
      setErr("");
    } catch (e) {
      setErr(String(e));
    }
    setBusy(false);
  };

  const on = (c: string) => (answered ? answered.picked.includes(c) : picked.includes(c));
  return (
    // the warn border: the agent waits for this answer (a question the user made waits for nobody)
    <div className="node node-question" data-open={(!answered && !data.mine) || undefined} style={{ width: QUESTION_WIDTH }}>
      <header>
        <MessageCircleQuestion size={14} className="muted" />
        <b className="ellipsis">{data.mine ? "내 질문" : "질문"}</b>
        <span className="spacer" />
        {answered ? (
          <span className="q-state done" title={when(answered.at)}><Check size={12} /> 답함</span>
        ) : data.mine ? (
          <span className="q-state mine">에이전트에게 보내요</span>
        ) : (
          <span className="q-state">답을 기다려요</span>
        )}
      </header>
      <div className="q-body">
        {data.title ? <p className="q-title">{data.title}</p> : <p className="q-note">더블클릭해 질문과 보기를 적어요</p>}
        {data.body && <p className="q-note">{data.body}</p>}
        {!free && (
          <div className="q-choices" role={data.multi ? "group" : "radiogroup"} aria-label="보기">
            {choices.map((c) => (
              <button
                key={c}
                className="q-choice nodrag"
                role={data.multi ? "checkbox" : "radio"}
                aria-checked={on(c)}
                data-on={on(c) || undefined}
                data-dim={(answered && !on(c)) || undefined}
                disabled={!!answered || busy}
                onClick={() => pick(c)}
              >
                <span className="q-mark" data-multi={data.multi || undefined} />
                <span>{c}</span>
              </button>
            ))}
          </div>
        )}
        {answered
          ? answered.text && <p className="q-answer">{free ? answered.text : `기타: ${answered.text}`}</p>
          : (free || data.other) && (
              <textarea
                className="nodrag nowheel"
                rows={free ? 3 : 1}
                readOnly={busy}
                value={text}
                placeholder={free ? "답 입력 · Enter 보내기 · Shift+Enter 줄바꿈" : "기타 (적으면 함께 보내요)"}
                onChange={(e) => setText(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter" && !e.shiftKey && !isImeComposing(e)) {
                    e.preventDefault();
                    void send();
                  }
                }}
              />
            )}
      </div>
      {!answered && (
        <footer>
          {err ? <span className="err">{err}</span> : <span className="muted">{data.multi ? "여러 개 고를 수 있어요" : ""}</span>}
          <button className="primary nodrag" disabled={busy || !ready || !data.title.trim()} onClick={() => void send()}>{busy ? "보내는 중…" : "보내기"}</button>
        </footer>
      )}
    </div>
  );
}

/** The choices typed one per line: trimmed, blank lines and repeats dropped, at most `CHOICES_MAX`. */
export const parseChoices = (raw: string) => [...new Set(raw.split(/\r?\n/).map((c) => c.trim()).filter(Boolean))].slice(0, CHOICES_MAX);
export const CHOICES_MAX = 8;

/**
 * A question the user made: its question, notes and choices (one per line; none = free text), and whether several
 * can be picked or a 기타 written. Opened on making it and by a double click until it is answered; changes apply as
 * you type, like the structure block's editor.
 */
export function QuestionEditor({ data, onChange, onClose }: { data: QuestionData; onChange: (d: Partial<QuestionData>) => void; onClose: () => void }) {
  const [raw, setRaw] = useState((data.choices ?? []).join(String.fromCharCode(10)));
  const choices = parseChoices(raw);
  const over = new Set(raw.split(/\r?\n/).map((c) => c.trim()).filter(Boolean)).size > CHOICES_MAX;
  return createPortal(
    <div className="modal-backdrop" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div className="modal struct-editor q-editor nokey" tabIndex={-1} onKeyDown={(e) => e.key === "Escape" && onClose()}>
        <div className="struct-editor-head">
          <input
            className="struct-title-input"
            autoFocus
            value={data.title}
            placeholder="질문 (예: 어떤 DB로 갈까요?)"
            onChange={(e) => onChange({ title: e.target.value })}
            onKeyDown={(e) => e.key === "Enter" && !isImeComposing(e) && onClose()}
          />
          <button className="icon" title="닫기 (Esc)" aria-label="닫기" onClick={onClose}><X /></button>
        </div>
        <label className="q-field">
          <span>설명 <span className="muted">(선택)</span></span>
          <textarea rows={2} value={data.body ?? ""} placeholder="에이전트가 알아야 할 배경" onChange={(e) => onChange({ body: e.target.value || undefined })} />
        </label>
        <label className="q-field">
          <span>보기 <span className="muted">· 한 줄에 하나, 비우면 주관식 (최대 {CHOICES_MAX}개)</span></span>
          <textarea
            rows={4}
            value={raw}
            placeholder={`Postgres
SQLite`}
            onChange={(e) => {
              setRaw(e.target.value);
              const next = parseChoices(e.target.value);
              onChange({ choices: next.length ? next : undefined, ...(next.length ? {} : { multi: undefined, other: undefined }) });
            }}
          />
          {over && <span className="err">{CHOICES_MAX}개까지만 써요</span>}
        </label>
        <div className="q-flags">
          <label><input type="checkbox" disabled={!choices.length} checked={!!data.multi} onChange={(e) => onChange({ multi: e.target.checked || undefined })} /> 여러 개 고르기</label>
          <label><input type="checkbox" disabled={!choices.length} checked={!!data.other} onChange={(e) => onChange({ other: e.target.checked || undefined })} /> 기타 글칸</label>
        </div>
      </div>
    </div>,
    document.body,
  );
}
