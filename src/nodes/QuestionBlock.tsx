import { useState } from "react";
import { createPortal } from "react-dom";
import { useReactFlow, useStore as useFlowStore, type NodeProps } from "@xyflow/react";
import { isImeComposing } from "../ime";
import { ipc } from "../ipc";
import { ArrowRight, Check, MessageCircleQuestion, X } from "../icons";
import { GRID } from "./AgentNode";
import { goPhase } from "../zones";
import type { CanvasNode, QuestionData, QuestionNode, ZoneNode } from "../types";

export const QUESTION_WIDTH = 13 * GRID;

const when = (ms: number) => new Date(ms).toLocaleString(undefined, { month: "numeric", day: "numeric", hour: "2-digit", minute: "2-digit" });

type Draft = { picked: string[]; text?: string };
const filled = (d?: Draft) => !!d && (!!d.picked.length || !!d.text?.trim());

/**
 * A question on the canvas (docs/features/canvas/question-block.md): the block an agent asks the user with
 * (`maestro ask`). Multiple choice: one pick (or several with `multi`), plus a free-text 기타 with `other`; no
 * choices: free text. The arrow at the bottom sends the answer to the agent (`ipc.answerQuestion`) and keeps it on the
 * block, read-only: the choices not picked stay, dimmed, as the decision's record.
 *
 * One block, two ways it comes to be. The usual one: an agent asks, the user answers, the agent uses the answer and
 * takes the block away (`maestro erase --question`). The other: the user puts one on the canvas (`mine`). That one
 * starts with nobody expected to read it: its answer stays on the block, and the agent sees it only when the user
 * points at it (picks it, which hands it over through the chat box and `maestro selection`). Until a block is
 * answered its question and choices can be edited (`QuestionEditor`; an agent's question changed this way is
 * `edited`, and its answer then carries the question as answered).
 *
 * In a set (`maestro ask --file`: several questions in one area, a step each) the arrow means "next": the answer is
 * kept on the block (`draft`, still changeable) and the area moves to the next question still open. On the last open
 * one the arrow sends them all, in one note (`ipc.answerQuestions`). A question alone in an area with steps moves the
 * area on a step once answered.
 */
export function QuestionBlock({ id, data, parentId }: NodeProps<QuestionNode>) {
  const { updateNodeData, getNodes, setNodes } = useReactFlow<CanvasNode>();
  const choices = data.choices ?? [];
  const answered = data.answer;
  const [picked, setPicked] = useState<string[]>(data.draft?.picked ?? []);
  const [text, setText] = useState(data.draft?.text ?? "");
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState("");
  const free = !choices.length;
  const ready = !!picked.length || !!text.trim();

  // where it sits: in its set's area ("set", its number, how many, how many of the others are still open), or alone
  // in an area with steps that has a step after this one ("step")
  const place = useFlowStore((s) => {
    const zone = parentId ? (s.nodeLookup.get(parentId) as ZoneNode | undefined) : undefined;
    if (!zone || zone.type !== "zone" || !zone.data.mode) return "";
    const phases = zone.data.phases ?? [];
    if (data.set && zone.data.qset === data.set) {
      const open = (s.nodes as CanvasNode[]).filter((n) => n.type === "question_block" && n.id !== id && n.parentId === parentId && n.data.set === data.set && !filled(n.data.draft)).length;
      return JSON.stringify(["set", phases.findIndex((p) => data.when?.includes(p.id)) + 1, phases.length, open]);
    }
    const at = Math.max(0, phases.findIndex((p) => p.id === zone.data.at));
    return zone.data.mode === "step" && data.when?.length && at < phases.length - 1 ? JSON.stringify(["step", 0, 0, 0]) : "";
  });
  const [kind, nth, of, open] = (place ? JSON.parse(place) : ["", 0, 0, 0]) as [string, number, number, number];
  const inSet = kind === "set" && !answered;
  // picked in the order the choices are listed, not the order they were pressed
  const draftOf = (p = picked, t = text): Draft => ({ picked: choices.filter((c) => p.includes(c)), text: t.trim() || undefined });
  // in a set the answer waits on the block until all are sent: kept as it is picked, so walking the steps loses nothing
  const keep = (p: string[], t: string) => {
    if (inSet) updateNodeData(id, { draft: draftOf(p, t) });
  };

  const pick = (c: string) => {
    const next = data.multi ? (picked.includes(c) ? picked.filter((x) => x !== c) : [...picked, c]) : picked[0] === c ? [] : [c];
    setPicked(next);
    keep(next, text);
  };
  const sendSet = async () => {
    const zone = getNodes().find((n) => n.id === parentId);
    const phases = zone?.type === "zone" ? (zone.data.phases ?? []) : [];
    const nthOf = (q: QuestionNode) => phases.findIndex((p) => q.data.when?.includes(p.id));
    const all = getNodes()
      .filter((n): n is QuestionNode => n.type === "question_block" && n.parentId === parentId && n.data.set === data.set)
      .sort((a, b) => nthOf(a) - nthOf(b));
    const drafts = all.map((q) => (q.id === id ? draftOf() : q.data.draft));
    const me = all.findIndex((q) => q.id === id);
    const left = all.map((_, i) => i).filter((i) => i !== me && !filled(drafts[i]));
    if (left.length) {
      // more to answer: this one is kept, and the next one still open shows (after this one, else from the start)
      const to = all[left.find((i) => i > me) ?? left[0]];
      const step = phases[nthOf(to)]?.id;
      updateNodeData(id, { draft: draftOf() });
      if (step && parentId) setNodes((ns) => goPhase(ns, parentId, step));
      return;
    }
    await ipc.answerQuestions(
      data.owner,
      data.set!,
      all.map((q, i) => ({ title: q.data.title, edited: !!q.data.edited, body: q.data.body ?? "", choices: q.data.choices ?? [], picked: drafts[i]!.picked, text: drafts[i]!.text ?? "" })),
    );
    const at = Date.now();
    const answers = new Map(all.map((q, i) => [q.id, { ...drafts[i]!, at }]));
    setNodes((ns) => ns.map((n) => (answers.has(n.id) && n.type === "question_block" ? { ...n, data: { ...n.data, answer: answers.get(n.id), draft: undefined } } : n)));
  };
  const send = async () => {
    if (!ready || busy || answered) return;
    setBusy(true);
    try {
      if (inSet) await sendSet();
      else {
        const d = draftOf();
        // nobody asked a question the user drew: nothing to send
        if (!data.mine) await ipc.answerQuestion(data.owner, { id: data.qid, title: data.title, edited: !!data.edited, body: data.body ?? "", choices }, d.picked, text);
        updateNodeData(id, { answer: { ...d, at: Date.now() } });
        // alone in an area with steps: answered, the area moves on a step
        if (kind === "step" && parentId) setNodes((ns) => goPhase(ns, parentId, "next"));
      }
      setErr("");
    } catch (e) {
      setErr(String(e));
    }
    setBusy(false);
  };

  const on = (c: string) => (answered ? answered.picked.includes(c) : picked.includes(c));
  const last = inSet && !open;
  const sendTitle = inSet
    ? last
      ? `모두 보내기 · 답 ${of}개를 한 번에 보내요`
      : "다음 질문 · 답은 모두 보낼 때까지 고칠 수 있어요"
    : data.mine
      ? "답 남기기 · 직접 놓은 질문은 에이전트가 알아서 읽지 않아요 (블록을 고르고 입력창으로 말하면 읽어요)"
      : free || data.other
        ? "보내기 (Enter)"
        : "보내기";
  const hint = inSet ? `${nth} / ${of}${last ? " · 누르면 모두 보내요" : ""}` : data.multi ? "여러 개 고를 수 있어요" : "";
  return (
    // the warn border: the agent waits for this answer (a question the user drew waits for nobody)
    <div className="node node-question" data-open={(!answered && !data.mine) || undefined} style={{ width: QUESTION_WIDTH }}>
      <header>
        <MessageCircleQuestion size={14} className="muted" />
        <b className="ellipsis">질문</b>
        <span className="spacer" />
        {answered ? (
          <span className="q-state done" title={when(answered.at)}><Check size={12} /> 답함</span>
        ) : data.mine ? (
          <span className="q-state mine" title="직접 놓은 질문이에요. 에이전트가 알아서 읽지는 않아요. 보이려면 이 블록을 고른 채 아래 입력창으로 말해요">직접 놓음</span>
        ) : inSet && filled(data.draft) ? (
          <span className="q-state mine" title="답을 적어 뒀어요. 모두 보낼 때까지 고칠 수 있어요">적어 둠</span>
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
                placeholder={free ? `답 입력 · Enter ${inSet ? (last ? "모두 보내기" : "다음") : data.mine ? "남기기" : "보내기"} · Shift+Enter 줄바꿈` : data.mine ? "기타" : "기타 (적으면 함께 보내요)"}
                onChange={(e) => setText(e.target.value)}
                onBlur={() => keep(picked, text)}
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
          {err ? <span className="err">{err}</span> : <span className="muted">{hint}</span>}
          {/* an arrow, not the word 보내기 (the composer's round send button): in a set the same button means "next" */}
          <button
            className="composer-btn send nodrag"
            disabled={busy || !ready || !data.title.trim()}
            title={busy ? "보내는 중…" : sendTitle}
            aria-label={inSet ? (last ? "모두 보내기" : "다음 질문") : data.mine ? "답 남기기" : "보내기"}
            aria-busy={busy || undefined}
            onClick={() => void send()}
          >
            <ArrowRight size={16} />
          </button>
        </footer>
      )}
    </div>
  );
}

/** The choices typed one per line: trimmed, blank lines and repeats dropped, at most `CHOICES_MAX`. */
export const parseChoices = (raw: string) => [...new Set(raw.split(/\r?\n/).map((c) => c.trim()).filter(Boolean))].slice(0, CHOICES_MAX);
export const CHOICES_MAX = 8;

/**
 * A question's own editor: its question, notes and choices (one per line; none = free text), and whether several can
 * be picked or a 기타 written. For any question not answered yet, the agent's too: opened by a double click (and on
 * drawing one); changes apply as you type, like the structure block's editor.
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
          <textarea rows={2} value={data.body ?? ""} placeholder="질문 아래에 작게 보이는 설명" onChange={(e) => onChange({ body: e.target.value || undefined })} />
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
