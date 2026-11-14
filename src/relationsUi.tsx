import { createContext, useContext, useState } from "react";
import { createPortal } from "react-dom";
import { EdgeLabelRenderer, useReactFlow, useStore as useFlowStore } from "@xyflow/react";
import { RichText } from "./nodes/AgentNode";
import { ChevronDown, Pencil, Plus, SquareSplitVertical, X } from "./icons";
import { openMenu } from "./ui/Menu";
import { ask } from "./ui/Confirm";
import { Seg } from "./settings/parts";
import { LINE_LABELS, useSettings } from "./settings";
import { isImeComposing } from "./ime";
import {
  COLORS, COLOR_NAMES, HEADS, PATTERNS, allOf, choicesFor, colorVar, lookalike, newRelationId, relationOfLine, resolve, saveRelations, useRelFile,
  type Head, type Pattern, type RelColor, type Relation, type Resolved,
} from "./relations";
import type { CanvasEdge } from "./types";

/*
 * What relations look like on screen (docs/features/canvas/relations.md): a line's label in four stages, the line
 * editor, the 관계 panel (the project's relations, the pen) with its relation editor, and the canvas legend.
 */

/** What a line on a canvas needs from it: the project's relations, and opening its editor (or pulling it out). */
export const LineCtx = createContext<{ rels: Relation[]; editLine: (id: string) => void; extract: (id: string) => void } | null>(null);
export const useLineCtx = () => useContext(LineCtx);

/** A short sample of a relation's line: its pattern, heads and colour. */
export function Swatch({ r, w = 28 }: { r: Pick<Resolved, "pattern" | "head" | "color">; w?: number }) {
  const color = colorVar(r.color);
  const dash = r.pattern === "dashed" ? "5 3" : r.pattern === "dotted" ? "0.5 3" : undefined;
  return (
    <svg className="rel-swatch" width={w} height={10} viewBox={`0 0 ${w} 10`} aria-hidden>
      <line x1={r.head === "both" ? 5 : 1} y1={5} x2={r.head === "none" ? w - 1 : w - 5} y2={5} stroke={color} strokeWidth={1.5} strokeDasharray={dash} strokeLinecap="round" />
      {r.head !== "none" && <path d={`M${w - 6} 1.5 L${w - 0.5} 5 L${w - 6} 8.5 Z`} fill={color} />}
      {r.head === "both" && <path d="M6 1.5 L0.5 5 L6 8.5 Z" fill={color} />}
    </svg>
  );
}

/** The first `n` lines of a body, Markdown marks stripped. */
const firstLines = (body: string, n: number) =>
  body
    .split("\n")
    .map((l) => l.replace(/^[#>*\-\s`]+/, "").trim())
    .filter(Boolean)
    .slice(0, n)
    .join(" ");

/** A line's relation picker: a menu of the relations this line can be (same behaviour). */
function pickRelation(at: Element, rels: Relation[], e: Pick<CanvasEdge, "data">, onPick: (id: string | undefined) => void) {
  const fallback = relationOfLine([], e).id; // the line's default: its built-in, or 기본 관계
  openMenu(
    at,
    choicesFor(rels, e).map((c) => ({ label: c.name, icon: <Swatch r={c} w={20} />, onSelect: () => onPick(c.id === fallback ? undefined : c.id) })),
  );
}

/**
 * A line's label, in four stages: at rest its short label (and ≡ when it has a body), as the 선 라벨 setting says;
 * hovered, a card with its relation, label and the start of its body; picked, a title block with its relation
 * (switchable), edit, pull-out (drawn lines) and cut; double-clicked, the line editor (the canvas opens it).
 */
export function LineLabel({ id, x, y, data, selected, hover, onHover, cutTitle }: {
  id: string;
  x: number;
  y: number;
  data: CanvasEdge["data"];
  selected?: boolean;
  hover: boolean;
  onHover: (on: boolean) => void;
  cutTitle: string;
}) {
  const ctx = useLineCtx();
  const { deleteElements, updateEdgeData } = useReactFlow();
  const always = useSettings((s) => (s.lineLabels ?? "always") === "always");
  const near = useFlowStore((s) => s.transform[2] >= 0.6);
  const rels = ctx?.rels ?? [];
  const rel = relationOfLine(rels, { data });
  const label = data?.label?.trim();
  const body = data?.body?.trim();
  const face = !selected && always && near && (label || body);
  const card = hover && !selected;
  if (!selected && !face && !card) return null;
  return (
    <EdgeLabelRenderer>
      <div
        className="line-label nodrag nopan"
        style={{ transform: `translate(-50%, -50%) translate(${x}px, ${y}px)` }}
        onMouseEnter={() => onHover(true)}
        onMouseLeave={() => onHover(false)}
        onDoubleClick={() => ctx?.editLine(id)}
      >
        {selected ? (
          <div className="line-card">
            <button
              className="line-rel"
              title="관계 바꾸기"
              onClick={(ev) => pickRelation(ev.currentTarget, rels, { data }, (relation) => updateEdgeData(id, { relation }))}
            >
              <Swatch r={rel} w={20} />
              <span className="ellipsis">{rel.name}</span>
              <ChevronDown size={12} />
            </button>
            {label && <b className="line-card-label ellipsis" title={label}>{label}</b>}
            <button className="icon" title="편집 · 라벨과 본문 (더블클릭)" aria-label="선 편집" onClick={() => ctx?.editLine(id)}>
              <Pencil size={13} />
            </button>
            {data?.kind === "arrow" && (
              <button className="icon" title="블록으로 꺼내기 · 이 관계를 블록으로 바꿔 사이에 놓아요" aria-label="블록으로 꺼내기" onClick={() => ctx?.extract(id)}>
                <SquareSplitVertical size={13} />
              </button>
            )}
            <button className="icon" title={cutTitle} aria-label={cutTitle} onClick={() => deleteElements({ edges: [{ id }] })}>
              <X size={12} />
            </button>
          </div>
        ) : (
          face && (
            <span className="line-tag" data-body={!!body || undefined}>
              {label && <span className="ellipsis">{label}</span>}
              {body && <span className="line-more" aria-label="본문 있음">≡</span>}
            </span>
          )
        )}
        {card && (
          <div className="line-hover">
            <div className="line-hover-head">
              <Swatch r={rel} w={20} />
              <span>{rel.name}</span>
            </div>
            {label && <b>{label}</b>}
            {body ? <p>{firstLines(body, 2)}</p> : !label && rel.description && <p className="muted">{rel.description}</p>}
          </div>
        )}
      </div>
    </EdgeLabelRenderer>
  );
}

/** The line editor (double click): its label, relation and Markdown body. Changes apply as you type. */
export function LineEditor({ edge, onChange, onClose }: {
  edge: CanvasEdge;
  onChange: (data: { label?: string; body?: string; relation?: string }) => void;
  onClose: () => void;
}) {
  const ctx = useLineCtx();
  const rels = ctx?.rels ?? [];
  const rel = relationOfLine(rels, edge);
  const body = edge.data?.body ?? "";
  const [mode, setMode] = useState<"view" | "edit">(body ? "view" : "edit");
  return createPortal(
    <div className="modal-backdrop" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div className="modal struct-editor line-editor nokey" tabIndex={-1} ref={(el) => void (el && !el.contains(document.activeElement) && el.focus())} onKeyDown={(e) => e.key === "Escape" && onClose()}>
        <div className="struct-editor-head">
          <input
            className="struct-title-input"
            autoFocus={!edge.data?.label}
            value={edge.data?.label ?? ""}
            placeholder="라벨 · 짧은 동사구 (예: 원인이다)"
            onChange={(e) => onChange({ label: e.target.value || undefined })}
            onKeyDown={(e) => e.key === "Enter" && !isImeComposing(e) && setMode("edit")}
          />
          <button className="line-rel" title="관계 바꾸기" onClick={(ev) => pickRelation(ev.currentTarget, rels, edge, (relation) => onChange({ relation }))}>
            <Swatch r={rel} w={20} />
            <span className="ellipsis">{rel.name}</span>
            <ChevronDown size={12} />
          </button>
          <div className="seg">
            <button data-on={mode === "view" || undefined} onClick={() => setMode("view")}>보기</button>
            <button data-on={mode === "edit" || undefined} onClick={() => setMode("edit")}>편집</button>
          </div>
          <button className="icon" title="닫기 (Esc)" aria-label="닫기" onClick={onClose}><X /></button>
        </div>
        {mode === "edit" ? (
          <textarea
            className="struct-body-input"
            autoFocus={!!edge.data?.label}
            value={body}
            placeholder="이 관계에 대한 긴 설명을 마크다운으로 적어요 (왜, 어떤 조건에서, 근거)"
            onChange={(e) => onChange({ body: e.target.value || undefined })}
          />
        ) : (
          <div className="struct-view">{body ? <RichText text={body} className="struct-md" /> : <p className="muted">본문이 없어요. 편집을 눌러 적어요.</p>}</div>
        )}
      </div>
    </div>,
    document.body,
  );
}

/**
 * The 관계 panel (canvas toolbar): the project's relations, built-in first. A row picks the pen (the relation new
 * lines on this canvas are drawn as); its pencil opens the relation editor; `+ 새 관계` declares one.
 */
export function RelationPanel({ root, pen, onPen, onClose }: { root: string; pen: string; onPen: (id: string) => void; onClose: () => void }) {
  const file = useRelFile(root);
  const list = allOf(file);
  const [editing, setEditing] = useState<Relation | "new" | null>(null);
  const row = (r: Relation) => {
    const x = resolve(list, r.id)!;
    const parent = x.extends ? resolve(list, x.extends) : null;
    return (
      <div key={r.id} className="rel-item" data-on={pen === r.id || undefined} title={x.description} onClick={() => onPen(r.id)}>
        <Swatch r={x} />
        <span className="rel-text">
          <b className="ellipsis">{x.name}{x.builtin && <span className="rel-tag">내장</span>}</b>
          <span className="ellipsis">{parent ? `${parent.name}을(를) 이어요 · ` : ""}{x.description || "설명 없음"}</span>
        </span>
        <button className="icon" title="관계 편집" aria-label={`${x.name} 편집`} onClick={(e) => (e.stopPropagation(), setEditing(r))}>
          <Pencil size={13} />
        </button>
      </div>
    );
  };
  return (
    <div className="library rel-panel nowheel nodrag">
      <div className="library-head">
        <b>관계</b>
        <span className="muted small">누르면 새 선이 그 관계로 그려져요</span>
        <span className="spacer" />
        <button className="icon" title="새 관계" aria-label="새 관계" onClick={() => setEditing("new")}><Plus size={14} /></button>
        <button className="icon" title="닫기" aria-label="닫기" onClick={onClose}><X size={14} /></button>
      </div>
      <div className="library-body rel-list">
        {list.filter((r) => resolve(list, r.id)!.builtin).map(row)}
        <div className="book-sep" />
        {file.relations.length ? file.relations.map(row) : <p className="muted small rel-empty">아직 선언한 관계가 없어요. + 로 만들거나, 그냥 그은 선에서 바꿔요.</p>}
      </div>
      {editing && <RelationEditor root={root} relation={editing === "new" ? null : editing} onClose={() => setEditing(null)} onSaved={(id) => onPen(id)} />}
    </div>
  );
}

/** Declares a relation or changes one. A built-in one only takes a colour. */
function RelationEditor({ root, relation, onClose, onSaved }: { root: string; relation: Relation | null; onClose: () => void; onSaved: (id: string) => void }) {
  const file = useRelFile(root);
  const list = allOf(file);
  const builtin = !!relation && list.some((r) => r.id === relation.id && resolve(list, r.id)!.builtin);
  const [draft, setDraft] = useState<Relation>(relation ?? { id: newRelationId(file), name: "" });
  const [error, setError] = useState<string | null>(null);
  const set = (p: Partial<Relation>) => setDraft((d) => ({ ...d, ...p }));
  const look = resolve([...list.filter((r) => r.id !== draft.id), draft], draft.id);
  const twin = draft.name.trim() ? lookalike(list, draft) : null;
  // a relation can't build on itself or on one that builds on it
  const below = (id: string) => {
    for (let at = list.find((r) => r.id === id), k = 0; at && k < 32; at = list.find((r) => r.id === at!.extends), k++) if (at.id === draft.id) return true;
    return false;
  };
  const parents = list.filter((r) => r.id !== draft.id && !below(r.id));
  const save = async () => {
    const next = builtin
      ? { ...file, builtin: Object.fromEntries(Object.entries({ ...file.builtin, [draft.id]: draft.color }).filter(([, c]) => c)) as Record<string, RelColor> }
      : { ...file, relations: file.relations.some((r) => r.id === draft.id) ? file.relations.map((r) => (r.id === draft.id ? clean(draft) : r)) : [...file.relations, clean(draft)] };
    try {
      await saveRelations(root, next);
      onSaved(draft.id);
      onClose();
    } catch (e) {
      setError(String(e));
    }
  };
  const remove = async () => {
    const ok = await ask({ title: `관계 지우기 · ${draft.name}`, body: "이 관계로 그린 선은 지워지지 않고 기본 관계로 돌아가요(라벨과 본문은 남아요). 이 관계를 잇던 관계는 부모를 잃어요.", confirm: "지우기", danger: true });
    if (!ok) return;
    try {
      await saveRelations(root, { ...file, relations: file.relations.filter((r) => r.id !== draft.id).map((r) => (r.extends === draft.id ? { ...r, extends: undefined } : r)) });
      onClose();
    } catch (e) {
      setError(String(e));
    }
  };
  return createPortal(
    <div className="modal-backdrop" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div className="modal rel-editor nokey" onKeyDown={(e) => e.key === "Escape" && onClose()}>
        <div className="struct-editor-head">
          <input
            className="struct-title-input"
            autoFocus={!builtin}
            readOnly={builtin}
            value={draft.name}
            placeholder="관계 이름 (예: 원인, 포함, 가정)"
            onChange={(e) => set({ name: e.target.value })}
          />
          <button className="icon" title="닫기 (Esc)" aria-label="닫기" onClick={onClose}><X /></button>
        </div>
        <label className="rel-field">
          <span>뜻</span>
          <textarea readOnly={builtin} rows={2} value={draft.description ?? ""} placeholder="이 선이 무엇을 말하는지 (범례와 에이전트에게 보여요)" onChange={(e) => set({ description: e.target.value || undefined })} />
        </label>
        {!builtin && (
          <>
            <div className="rel-field">
              <span>패턴</span>
              <Seg<Pattern | "">
                value={draft.pattern ?? ""}
                options={[...(draft.extends ? [["", "이어받기"] as const] : []), ...PATTERNS.map(([v, l]) => [v, <span className="rel-opt"><Swatch r={{ pattern: v, head: "none" }} w={24} />{l}</span>] as const)]}
                onChange={(v) => set({ pattern: v || undefined })}
              />
            </div>
            <div className="rel-field">
              <span>머리</span>
              <Seg<Head | "">
                value={draft.head ?? ""}
                options={[...(draft.extends ? [["", "이어받기"] as const] : []), ...HEADS.map(([v, l]) => [v, l] as const)]}
                onChange={(v) => set({ head: v || undefined })}
              />
            </div>
          </>
        )}
        <div className="rel-field">
          <span>색</span>
          <div className="rel-colors">
            <button className="rel-color" data-on={!draft.color || undefined} title="선 색 (기본)" onClick={() => set({ color: undefined })}><i style={{ background: colorVar() }} /></button>
            {COLORS.map((c) => (
              <button key={c} className="rel-color" data-on={draft.color === c || undefined} title={COLOR_NAMES[c]} onClick={() => set({ color: c })}><i style={{ background: colorVar(c) }} /></button>
            ))}
          </div>
        </div>
        {!builtin && (
          <label className="rel-field">
            <span>잇기</span>
            <select value={draft.extends ?? ""} onChange={(e) => set({ extends: e.target.value || undefined })}>
              <option value="">없음</option>
              {parents.map((p) => (
                <option key={p.id} value={p.id}>{p.name}</option>
              ))}
            </select>
          </label>
        )}
        <div className="rel-preview">
          {look && <Swatch r={look} w={64} />}
          <span className="muted small">
            {builtin ? "내장 관계는 이름과 동작이 정해져 있어 색만 바꿔요." : look?.behavior ? `${resolve(list, look.behavior)?.name} 동작을 이어받아요: 그 선에만 쓸 수 있어요.` : "그림의 뜻만 가진 관계예요."}
            {twin && ` · '${twin.name}'와(과) 모양이 같아요.`}
          </span>
        </div>
        {error && <p className="err small">{error}</p>}
        <div className="modal-actions">
          {!builtin && relation && <button className="danger" onClick={remove}>지우기</button>}
          <span className="spacer" />
          <button onClick={onClose}>취소</button>
          <button className="primary" disabled={!draft.name.trim()} onClick={save}>저장</button>
        </div>
      </div>
    </div>,
    document.body,
  );
}

/** A relation without empty strings (the file keeps only what's set). */
const clean = (r: Relation): Relation => {
  const out: Relation = { id: r.id, name: r.name.trim() };
  if (r.description?.trim()) out.description = r.description.trim();
  if (r.pattern) out.pattern = r.pattern;
  if (r.head) out.head = r.head;
  if (r.color) out.color = r.color;
  if (r.extends) out.extends = r.extends;
  return out;
};

const legendShut = new Set<string>();

/** The canvas legend (top right): the relations its lines use, and when line labels show. Foldable, per canvas. */
export function Legend({ used, canvasKey }: { used: Resolved[]; canvasKey: string }) {
  const [shut, setShut] = useState(() => legendShut.has(canvasKey));
  const s = useSettings();
  if (!used.length) return null;
  const fold = () => {
    if (shut) legendShut.delete(canvasKey);
    else legendShut.add(canvasKey);
    setShut(!shut);
  };
  return (
    <div className="legend nowheel nodrag">
      <button className="legend-head" aria-expanded={!shut} onClick={fold}>
        <b>범례</b>
        <span className="muted small">{used.length}</span>
        <span className="spacer" />
        <ChevronDown size={12} className="legend-caret" data-open={!shut || undefined} />
      </button>
      {!shut && (
        <>
          <div className="legend-list">
            {used.map((r) => (
              <div key={r.id} className="legend-row" title={r.description}>
                <Swatch r={r} w={24} />
                <span className="ellipsis">{r.name}</span>
              </div>
            ))}
          </div>
          <div className="legend-foot" title={LINE_LABELS[s.lineLabels ?? "always"].hint}>
            <span className="muted small">라벨</span>
            <Seg value={s.lineLabels ?? "always"} options={Object.entries(LINE_LABELS).map(([id, l]) => [id as keyof typeof LINE_LABELS, l.label] as const)} onChange={(v) => s.set({ lineLabels: v })} />
          </div>
        </>
      )}
    </div>
  );
}
