// What the user changed on a canvas while a play went again (docs/features/canvas/canvas-flows.md "플레이 중 수정"):
// the canvas's content (blocks, areas, documents, lines with their relations) taken when the play starts, compared
// when it ends. Where things sit and how big they are is not content. Pure, so scripts/focus-check.ts runs it in Node.

/** A node or a line as far as its content goes: what kind, its name, and a signature of what it says. */
export type Part = { kind: "블록" | "영역" | "문서" | "관계"; name: string; sig: string; doc?: string };
export type Snapshot = Record<string, Part>;
/** One change: what, by name, and how. `doc`: the Markdown document it is or is joined to. */
export type Edit = { kind: Part["kind"]; name: string; how: "추가" | "삭제" | "수정"; doc?: string };

type N = { id: string; type?: string; parentId?: string; data: Record<string, unknown> };
type E = { id: string; source: string; target: string; data?: Record<string, unknown> };

const text = (v: unknown) => (typeof v === "string" ? v : "");

/** The content of a canvas (nodes and lines), by id. Lines between agents, and agents' own blocks, are not content. */
export function snapshot(nodes: readonly N[], edges: readonly E[]): Snapshot {
  const out: Snapshot = {};
  const byId = new Map(nodes.map((n) => [n.id, n]));
  const nameOf = (id: string) => text(byId.get(id)?.data.title) || "이름 없음";
  const docOf = (id: string) => {
    const n = byId.get(id);
    return n?.type === "markdown_block" ? text(n.data.path) : undefined;
  };
  for (const n of nodes) {
    const d = n.data;
    if (n.type === "struct_block") out[n.id] = { kind: "블록", name: text(d.title) || "이름 없음", sig: JSON.stringify([d.title, d.body ?? "", d.color ?? "", n.parentId ?? ""]) };
    else if (n.type === "zone" && !d.team && !d.qset) out[n.id] = { kind: "영역", name: text(d.title) || "이름 없음", sig: JSON.stringify([d.title, d.color ?? "", d.mode ?? "", d.phases ?? [], d.stack ?? false, n.parentId ?? ""]) };
    else if (n.type === "markdown_block") out[n.id] = { kind: "문서", name: text(d.title) || text(d.path), sig: JSON.stringify([d.path, n.parentId ?? ""]), doc: text(d.path) };
  }
  for (const e of edges) {
    const k = e.data?.kind;
    if (k === "orchestrates" || k === "command_pipe") continue;
    if (!out[e.source] && !out[e.target] && !docOf(e.source) && !docOf(e.target)) continue;
    out[e.id] = {
      kind: "관계",
      name: `${nameOf(e.source)} → ${nameOf(e.target)}`,
      sig: JSON.stringify([e.source, e.target, k ?? "", e.data?.relation ?? "", e.data?.label ?? "", e.data?.body ?? ""]),
      doc: docOf(e.source) ?? docOf(e.target),
    };
  }
  // a block joined by a line to a document goes with that document
  for (const e of edges) {
    const doc = docOf(e.source) ?? docOf(e.target);
    for (const id of [e.source, e.target]) if (doc && out[id] && !out[id].doc) out[id] = { ...out[id], doc };
  }
  return out;
}

/** What changed from `before` to `after`, leaving out what is in `skip` (what the agent itself put there meanwhile). */
export function edits(before: Snapshot, after: Snapshot, skip: ReadonlySet<string> = new Set()): Edit[] {
  const out: Edit[] = [];
  const one = (p: Part, how: Edit["how"]): Edit => ({ kind: p.kind, name: p.name, how, ...(p.doc ? { doc: p.doc } : {}) });
  for (const [id, b] of Object.entries(before)) {
    if (skip.has(id)) continue;
    const a = after[id];
    if (!a) out.push(one(b, "삭제"));
    else if (a.sig !== b.sig) out.push(one({ ...a, doc: a.doc ?? b.doc }, "수정"));
  }
  for (const [id, a] of Object.entries(after)) if (!before[id] && !skip.has(id)) out.push(one(a, "추가"));
  return out;
}

/** The Markdown documents the changes touch, and what the play said it explains (`--source`): where to carry them. */
export const targetsOf = (list: readonly Edit[], source: readonly string[] = []): string[] => [...new Set([...source, ...list.flatMap((e) => (e.doc ? [e.doc] : []))])];

/** The note the canvas's agent gets when the user asks for the changes to be carried back to what the play explains. */
export function applyNote(title: string, list: readonly Edit[], targets: readonly string[]): string {
  return [
    `[maestro] 사용자가 플레이 '${title}'을(를) 보는 동안 캔버스에서 고쳤어요:`,
    ...list.map((e) => `- ${e.kind} '${e.name}' ${e.how}`),
    "",
    `이 플레이가 설명하는 원본에 이 변경을 반영해 주세요: ${targets.join(", ")}`,
    "`maestro canvas`로 지금 모습을 읽고, 바뀐 것만 고쳐요. 원본에 맞지 않는 변경이면 고치기 전에 채팅에서 물어요.",
  ].join("\n");
}
