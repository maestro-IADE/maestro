import type { CanvasEdge } from "./types";

/*
 * Relations, the model (docs/features/canvas/relations.md): pure, so scripts/relations-check.ts runs it under Node.
 * The store lives in relations.ts.
 *
 * Relations (docs/features/canvas/relations.md): the kinds of line a project draws. A relation is declared once
 * (name, meaning, look) and every line of that kind is an instance of it (with its own label and body). Four are
 * built in: three that do something (who leads whom, prompts, references) and the default for a line drawn between
 * structure items. The look is a small fixed grammar (3 patterns, 3 heads, the line colour or 6 tints); the meaning is free.
 */

export type Pattern = "solid" | "dashed" | "dotted";
export type Head = "one" | "both" | "none";
export const PATTERNS: [Pattern, string][] = [["solid", "실선"], ["dashed", "긴 점선"], ["dotted", "짧은 점선"]];
export const HEADS: [Head, string][] = [["one", "→"], ["both", "↔"], ["none", "—"]];
/** the tints a relation can take (styles.css --mae-rel-*), besides the line colour itself */
export const COLORS = ["blue", "green", "amber", "red", "violet", "teal"] as const;
export type RelColor = (typeof COLORS)[number];
export const COLOR_NAMES: Record<RelColor, string> = { blue: "파랑", green: "초록", amber: "호박", red: "빨강", violet: "보라", teal: "청록" };

/** As stored (relations.json): fields left empty come from `extends`, then the defaults. */
export type Relation = { id: string; name: string; description?: string; pattern?: Pattern; head?: Head; color?: RelColor; extends?: string };
export type RelFile = { relations: Relation[]; builtin: Record<string, RelColor> };

/** What a built-in relation does: the line is a real link, made and cut through the app. */
export type Behavior = "orchestrates" | "command_pipe" | "reference";
export const BUILTIN: (Relation & { builtin: true })[] = [
  { id: "plain", name: "기본 관계", pattern: "solid", head: "one", description: "구조 블록·영역 사이에 그냥 그은 선", builtin: true },
  { id: "orchestrates", name: "지휘", pattern: "solid", head: "one", description: "Lead → 워커: 그 워커를 이끌어요", builtin: true },
  { id: "command_pipe", name: "지시", pattern: "solid", head: "one", description: "프롬프트 블록 → 에이전트: 그 프롬프트를 보내요", builtin: true },
  { id: "reference", name: "참조", pattern: "dashed", head: "one", description: "문서 → 에이전트: 그 문서를 읽혀요", builtin: true },
];
const BEHAVIORS: Behavior[] = ["orchestrates", "command_pipe", "reference"];

/** A relation with every look field filled (through its parents), its behaviour (the root's) and its parent chain. */
export type Resolved = Required<Pick<Relation, "id" | "name" | "pattern" | "head">> & {
  description?: string;
  color?: RelColor;
  builtin: boolean;
  behavior: Behavior | null;
  extends?: string;
};

/** All relations of a project, built-in first. */
export const allOf = (f: RelFile | undefined): Relation[] => [
  ...BUILTIN.map((b) => ({ ...b, color: f?.builtin[b.id] })),
  ...(f?.relations ?? []),
];

export function resolve(list: Relation[], id: string): Resolved | null {
  const byId = new Map(list.map((r) => [r.id, r]));
  const r = byId.get(id);
  if (!r) return null;
  const out: Resolved = { id: r.id, name: r.name, description: r.description, pattern: "solid", head: "one", builtin: BUILTIN.some((b) => b.id === r.id), behavior: null, extends: r.extends };
  let pattern: Pattern | undefined, head: Head | undefined, color: RelColor | undefined;
  // walk up: each field takes the nearest one set; the root decides the behaviour
  for (let at: Relation | undefined = r, k = 0; at && k < 32; at = at.extends ? byId.get(at.extends) : undefined, k++) {
    pattern ??= at.pattern;
    head ??= at.head;
    color ??= at.color;
    if (BEHAVIORS.includes(at.id as Behavior)) out.behavior = at.id as Behavior;
  }
  return { ...out, pattern: pattern ?? "solid", head: head ?? "one", color };
}

/** The behaviour a line has by what it connects (its kind), or null for a drawn line. */
export const behaviorOf = (e: Pick<CanvasEdge, "data">): Behavior | null => {
  const k = e.data?.kind;
  return k === "orchestrates" || k === "command_pipe" || k === "reference" ? k : null;
};

/**
 * The relation a line is drawn as: its own, when that one still exists and fits the line (a line that does something
 * takes only that behaviour's relations; a drawn line only ones without behaviour); otherwise the line's default
 * (the built-in of its behaviour, or 기본 관계). A deleted relation's lines fall back here, label and body kept.
 */
export function relationOfLine(list: Relation[], e: Pick<CanvasEdge, "data">): Resolved {
  const b = behaviorOf(e);
  const own = e.data?.relation ? resolve(list, e.data.relation) : null;
  if (own && own.behavior === b) return own;
  const id = b ?? "plain";
  return resolve(list, id) ?? resolve(BUILTIN, id)!;
}

/** The relations a line can be switched to (same behaviour, or none for a drawn line). */
export const choicesFor = (list: Relation[], e: Pick<CanvasEdge, "data">) => {
  const b = behaviorOf(e);
  return list.map((r) => resolve(list, r.id)!).filter((r) => r.behavior === b);
};

/** The CSS colour of a relation (styles.css --mae-rel-*), or the line colour. */
export const colorVar = (c?: RelColor) => (c ? `var(--mae-rel-${c})` : "var(--mae-edge)");

/** Another relation that looks the same (pattern, head and colour), if any: worth saying when declaring. */
export function lookalike(list: Relation[], r: Relation): Resolved | null {
  const me = resolve([...list.filter((x) => x.id !== r.id), r], r.id);
  if (!me) return null;
  for (const x of list) {
    if (x.id === r.id) continue;
    const o = resolve(list, x.id);
    if (o && o.pattern === me.pattern && o.head === me.head && o.color === me.color && o.behavior === me.behavior) return o;
  }
  return null;
}

