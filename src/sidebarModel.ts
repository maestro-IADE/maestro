// The sidebar's pure parts (docs/features/ui/side-panels.md): the order and indent of a project's agents, what a
// session row's second line says, and the order Ctrl+Shift+↑/↓ walks. No React, no store, no Tauri, so
// scripts/sidebar-check.ts can test it. Only `import type` here (Node runs the tests without a bundler).
import type { Worktree } from "./store";
import type { AttentionItem } from "./attention";
import type { Repo } from "./types";

// The same folder, whatever the case or slashes: the rule of `samePath` (attention.ts, which the app imports). Copied,
// not imported: Node runs these modules for the tests without a bundler, and one can't import the other.
const norm = (p: string) => p.replace(/\\/g, "/").replace(/\/+$/, "").toLowerCase();
const samePath = (a: string, b: string) => norm(a) === norm(b);

/** How deep a row is drawn: deeper workers stay at this indent (their tooltip says how deep they really are). */
export const MAX_DEPTH = 3;

/** The row parts the model reads (a Worktree has them all). */
type RowWt = Pick<Worktree, "path" | "isMaestro" | "workers">;

export type TreeRow<W extends RowWt = Worktree> = {
  w: W;
  /** how many leads up it really is (0: nobody leads it) */
  depth: number;
  /** the indent it is drawn at: `depth`, capped at MAX_DEPTH */
  shown: number;
  /** under the 독립 에이전트 subhead: nobody leads it while the project has a Maestro (or under such an agent) */
  loose: boolean;
};

/**
 * A project's agents in hierarchy order, as the canvas arrows draw it: the Maestro first with its workers nested
 * below (depth first), then the agents nobody leads (`loose`, marked only when there is a Maestro) with theirs.
 */
export function treeRows<W extends RowWt>(wts: W[]): TreeRow<W>[] {
  const key = norm;
  const byPath = new Map(wts.map((w) => [key(w.path), w]));
  const led = new Set(wts.flatMap((w) => w.workers.map(key)));
  const rows: TreeRow<W>[] = [];
  const seen = new Set<string>();
  let loose = false;
  const walk = (w: W, depth: number) => {
    if (seen.has(key(w.path))) return;
    seen.add(key(w.path));
    rows.push({ w, depth, shown: Math.min(depth, MAX_DEPTH), loose });
    for (const p of w.workers) {
      const c = byPath.get(key(p));
      if (c) walk(c, depth + 1);
    }
  };
  wts.filter((w) => w.isMaestro).forEach((w) => walk(w, 0));
  loose = rows.length > 0;
  wts.filter((w) => !led.has(key(w.path))).forEach((w) => walk(w, 0));
  wts.forEach((w) => walk(w, 0)); // only reachable through a broken (cyclic) link file
  return rows;
}

/**
 * What a session row's second line shows. In priority order: no agent attached (an outside worktree) > a tool
 * approval ("승인 대기 · <도구>") > a terminal question > the Maestro's merge requests ("병합 요청 N") > the normal
 * line (agent logo, branch, PR; the Maestro adds "워커 N").
 */
export type RowMeta = {
  line2: "normal" | "approval" | "tui" | "merge" | "noAgent";
  /** approval: the tool it asks about */
  tool?: string;
  /** the merge requests this Maestro asked for (0 for others) */
  mergeCount: number;
  /** the Maestro's workers (0 for others) */
  workerCount: number;
};

/**
 * The row's meta from the 확인할 일 list (attention.ts `attentionItems`): an item belongs to the row of its `path`
 * (the asking agent; the Maestro for a merge request). PRs don't change the second line (it shows the PR already).
 */
export function rowMeta(wt: Pick<Worktree, "path" | "isMaestro" | "workers" | "adapterId">, items: AttentionItem[]): RowMeta {
  const mine = items.filter((i) => samePath(i.path, wt.path));
  const mergeCount = wt.isMaestro ? mine.filter((i) => i.kind === "merge").length : 0;
  const workerCount = wt.isMaestro ? wt.workers.length : 0;
  const base = { mergeCount, workerCount };
  if (!wt.adapterId) return { line2: "noAgent", ...base };
  const approval = mine.find((i) => i.kind === "approval");
  if (approval) return { line2: "approval", tool: approval.tool || undefined, ...base };
  if (mine.some((i) => i.kind === "tui")) return { line2: "tui", ...base };
  if (mergeCount > 0) return { line2: "merge", ...base };
  return { line2: "normal", ...base };
}

/** The second line's words when it stands for what waits (or the missing agent); null for the normal line. */
export function reasonText(m: RowMeta): string | null {
  switch (m.line2) {
    case "approval":
      return m.tool ? `승인 대기 · ${m.tool}` : "승인 대기";
    case "tui":
      return "터미널 질문";
    case "merge":
      return `병합 요청 ${m.mergeCount}`;
    case "noAgent":
      return "에이전트 연결 필요";
    default:
      return null;
  }
}

/**
 * Every agent in sidebar order (projects as listed, each one's rows in tree order), leaving out folded projects:
 * the order Ctrl+Shift+↑/↓ walks.
 */
export function flatAgentOrder(repos: Pick<Repo, "root">[], worktrees: Array<RowWt & { root: string }>, folded: string[]): string[] {
  return repos
    .filter((r) => !folded.includes(r.root))
    .flatMap((r) => treeRows(worktrees.filter((w) => w.root === r.root)).map((x) => x.w.path));
}

/**
 * The agent one step from `current` in `order` (-1 up, 1 down), or null at either end. With no current agent (or one
 * not in the order: its project is folded) down starts at the first and up at the last.
 */
export function stepPath(order: string[], current: string | null, dir: -1 | 1): string | null {
  if (!order.length) return null;
  const i = current ? order.findIndex((p) => samePath(p, current)) : -1;
  if (i < 0) return dir > 0 ? order[0] : order[order.length - 1];
  return order[i + dir] ?? null;
}
