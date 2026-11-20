// What the user changed on a canvas while a play went again (docs/features/canvas/canvas-flows.md "플레이 중 수정"):
// the canvas's content (blocks, areas, documents, lines with their relations) taken when the play starts, compared
// when it ends. Where things sit and how big they are is not content. Pure, so scripts/focus-check.ts runs it in Node.

/**
 * A node or a line as far as its content goes: what kind, its name, a signature of what it says, and that in words
 * (`text`: for the agent's note, so it can tell what it was and put it back).
 */
export type Part = { kind: "블록" | "영역" | "문서" | "관계"; name: string; sig: string; text: string; doc?: string };
export type Snapshot = Record<string, Part>;
/**
 * One change: which node or line (`id`), what, by name, and how. `was` / `now`: its content before and after in
 * words. `doc`: the Markdown document it is or is joined to.
 */
export type Edit = { id: string; kind: Part["kind"]; name: string; how: "추가" | "삭제" | "수정"; was?: string; now?: string; doc?: string };

type N = { id: string; type?: string; parentId?: string; data: Record<string, unknown> };
type E = { id: string; source: string; target: string; data?: Record<string, unknown> };

const text = (v: unknown) => (typeof v === "string" ? v : "");
const short = (s: string, n = 80) => {
  const one = s.replace(/\s+/g, " ").trim();
  return one.length > n ? `${one.slice(0, n - 1)}…` : one;
};

/** The content of a canvas (nodes and lines), by id. Lines between agents, and agents' own blocks, are not content. */
export function snapshot(nodes: readonly N[], edges: readonly E[]): Snapshot {
  const out: Snapshot = {};
  const byId = new Map(nodes.map((n) => [n.id, n]));
  const nameOf = (id: string) => text(byId.get(id)?.data.title) || "이름 없음";
  const docOf = (id: string) => {
    const n = byId.get(id);
    return n?.type === "markdown_block" ? text(n.data.path) : undefined;
  };
  const inside = (n: N) => (n.parentId && byId.get(n.parentId) ? ` (영역 '${nameOf(n.parentId)}' 안)` : "");
  for (const n of nodes) {
    const d = n.data;
    const name = text(d.title) || "이름 없음";
    if (n.type === "struct_block")
      out[n.id] = { kind: "블록", name, sig: JSON.stringify([d.title, d.body ?? "", d.color ?? "", n.parentId ?? ""]), text: `'${name}'${text(d.body) ? `: ${short(text(d.body))}` : ""}${inside(n)}` };
    else if (n.type === "zone" && !d.team && !d.qset)
      out[n.id] = { kind: "영역", name, sig: JSON.stringify([d.title, d.color ?? "", d.mode ?? "", d.phases ?? [], d.stack ?? false, n.parentId ?? ""]), text: `'${name}'${inside(n)}` };
    else if (n.type === "markdown_block")
      out[n.id] = { kind: "문서", name: text(d.title) || text(d.path), sig: JSON.stringify([d.path, n.parentId ?? ""]), text: text(d.path), doc: text(d.path) };
  }
  for (const e of edges) {
    const k = e.data?.kind;
    if (k === "orchestrates" || k === "command_pipe") continue;
    if (!out[e.source] && !out[e.target] && !docOf(e.source) && !docOf(e.target)) continue;
    const name = `${nameOf(e.source)} → ${nameOf(e.target)}`;
    const rel = [text(e.data?.relation), text(e.data?.label)].filter(Boolean).join(" · ");
    out[e.id] = {
      kind: "관계",
      name,
      sig: JSON.stringify([e.source, e.target, k ?? "", e.data?.relation ?? "", e.data?.label ?? "", e.data?.body ?? ""]),
      text: `${name}${rel ? ` (${rel})` : ""}${text(e.data?.body) ? `: ${short(text(e.data?.body))}` : ""}`,
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
  const one = (id: string, p: Part, how: Edit["how"], was?: Part, now?: Part): Edit => ({
    id,
    kind: p.kind,
    name: p.name,
    how,
    ...(was ? { was: was.text } : {}),
    ...(now ? { now: now.text } : {}),
    ...(p.doc ? { doc: p.doc } : {}),
  });
  for (const [id, b] of Object.entries(before)) {
    if (skip.has(id)) continue;
    const a = after[id];
    if (!a) out.push(one(id, b, "삭제", b));
    else if (a.sig !== b.sig) out.push(one(id, { ...a, doc: a.doc ?? b.doc }, "수정", b, a));
  }
  for (const [id, a] of Object.entries(after)) if (!before[id] && !skip.has(id)) out.push(one(id, a, "추가", undefined, a));
  return out;
}

/** The Markdown documents the changes touch, and what the play said it explains (`--source`): where to carry them. */
export const targetsOf = (list: readonly Edit[], source: readonly string[] = []): string[] => [...new Set([...source, ...list.flatMap((e) => (e.doc ? [e.doc] : []))])];

/**
 * 되돌리기: the canvas as it is with these changes taken back to how they were when the play started (what was
 * added goes, what was removed comes back, what was changed gets its content back). Anything else, and where things
 * sit, stays as it is now. Lines left without an end go.
 */
export function revertEdits<NN extends N, EE extends E>(list: readonly Edit[], now: { nodes: readonly NN[]; edges: readonly EE[] }, before: { nodes: readonly NN[]; edges: readonly EE[] }): { nodes: NN[]; edges: EE[] } {
  const bn = new Map(before.nodes.map((n) => [n.id, n]));
  const be = new Map(before.edges.map((e) => [e.id, e]));
  const nodeIds = new Set(list.filter((e) => e.kind !== "관계").map((e) => e.id));
  const edgeIds = new Set(list.filter((e) => e.kind === "관계").map((e) => e.id));
  const nodes = now.nodes.flatMap((n): NN[] => {
    if (!nodeIds.has(n.id)) return [n];
    const b = bn.get(n.id);
    if (!b) return [];
    return [b.parentId === n.parentId ? ({ ...n, data: b.data } as NN) : ({ ...n, data: b.data, parentId: b.parentId, position: (b as { position?: unknown }).position } as NN)];
  });
  const had = new Set(nodes.map((n) => n.id));
  for (const b of before.nodes) if (nodeIds.has(b.id) && !had.has(b.id)) nodes.push(b);
  const there = new Set(nodes.map((n) => n.id));
  const edges = now.edges.flatMap((e): EE[] => {
    if (!edgeIds.has(e.id)) return [e];
    const b = be.get(e.id);
    return b ? [b] : [];
  });
  const hadE = new Set(edges.map((e) => e.id));
  for (const b of before.edges) if (edgeIds.has(b.id) && !hadE.has(b.id)) edges.push(b);
  return { nodes, edges: edges.filter((e) => there.has(e.source) && there.has(e.target)) };
}

/**
 * 반영: the note the canvas's agent gets when the user keeps what they changed while watching a play. It carries the
 * changes into what the play explains (`targets`, when there are any), shows the play again under the same title when
 * its flow changed, and may put back a change that doesn't fit (it has each one's before and after).
 */
export function applyNote(title: string, list: readonly Edit[], targets: readonly string[]): string {
  const line = (e: Edit) =>
    `- ${e.kind} ${e.how}: ${e.how === "수정" ? `${e.was} → ${e.now}` : e.how === "삭제" ? e.was : e.now}`;
  return [
    `[maestro] 사용자가 플레이 '${title}'을(를) 보는 동안 캔버스에서 고쳤고, 반영해 달래요:`,
    ...list.map(line),
    "",
    ...(targets.length ? [`1. 이 플레이가 설명하는 원본을 이 변경에 맞게 고쳐요: ${targets.join(", ")}`] : ["1. 이 플레이가 설명하는 원본 파일이 있으면 이 변경에 맞게 고쳐요."]),
    `2. 걸음의 순서나 설명이 달라져야 하면 같은 제목으로 다시 보여 줘요: 첫 걸음 명령에 --play "${title}" (같은 제목이면 그 플레이를 바꿔요).`,
    "3. 맞지 않는 변경이 있으면 캔버스에서 위의 전 모습으로 되돌리고(maestro draw / erase), 무엇을 왜 되돌렸는지 채팅에 말해요.",
    "먼저 `maestro canvas`로 지금 모습을 읽어요.",
  ].join("\n");
}
