// 확인할 일 (docs/features/ui/attention.md): everything that waits for the user, as one list. Tool approvals, terminal
// questions, merge requests and the pull requests that need a hand, sorted by project (sidebar order), kind and
// arrival. The top bar's button and popover, the window title, the folded project heads, the session rows' second
// line, the status glyph's 승인 대기 and Ctrl+J all read it. Pure: no React, no store, no Tauri, so
// scripts/attention-check.ts can test it. Only `import type` here (Node runs the tests without a bundler).
import type { Worktree } from "./store";
import type { Approval, MergeRequest, PullRequest, Repo, TuiWait } from "./types";

/** The same folder, whatever the case or slashes (`C:\a\b` and `c:/a/b/`). */
export const samePath = (a: string, b: string) => normPath(a) === normPath(b);
/** A folder's comparable form: forward slashes, no trailing slash, lower case. */
export const normPath = (p: string) => p.replace(/\\/g, "/").replace(/\/+$/, "").toLowerCase();

export type AttentionKind = "approval" | "tui" | "merge" | "pr";
/** The kinds in the order the list shows them within a project. */
export const KIND_ORDER: readonly AttentionKind[] = ["approval", "tui", "merge", "pr"];

export type AttentionItem = {
  /** stable across updates: "approval:<id>", "tui:<path>", "merge:<id>", "pr:<worker path>" */
  id: string;
  kind: AttentionKind;
  /** the project ("" when its agent isn't known yet) */
  root: string;
  /**
   * The agent it belongs to and that [열기] opens: the asking agent for an approval or a terminal question, the
   * Maestro for a merge request, the Maestro for a PR (its worker once the Maestro is gone).
   */
  path: string;
  /** the name shown on its first line: the asking agent's, or the worker's for a merge request and a PR */
  name: string;
  /** its second line */
  title: string;
  /** more, where there is more: an approval's one-line summary of what the tool wants to do */
  detail?: string;
  /** the tool an approval (or the terminal question it was handed to) is about, when it says */
  tool?: string;
  approvalId?: number;
  mergeId?: number;
  /** a merge request's or PR's worker */
  worker?: string;
  /** how many files a merge request changes */
  files?: number;
  /** a PR whose CI failed (its icon is red) */
  failed?: boolean;
  /** when it came in (ms), where known */
  at?: number;
};

/** What the store holds that the list is made from. */
export type AttentionSource = {
  approvals: Approval[];
  tuiWaits: TuiWait[];
  merges: MergeRequest[];
  prs: PullRequest[];
  worktrees: Array<Pick<Worktree, "root" | "path" | "name">>;
  repos: Array<Pick<Repo, "root">>;
};

/** What a tool wants to do, in one line: its command, file, address or pattern (else its input as JSON). */
export function approvalSummary(a: Pick<Approval, "toolInput">): string {
  const i = a.toolInput ?? {};
  return i.command ?? i.file_path ?? i.url ?? i.pattern ?? i.path ?? JSON.stringify(i);
}

/** The CI state of the head shown: checks recorded for another commit (before a push) count as pending. */
export const ciOf = (p: Pick<PullRequest, "checks" | "headSha">): PullRequest["checks"]["state"] =>
  p.checks.headSha && p.checks.headSha !== p.headSha ? "pending" : p.checks.state;

/**
 * An open PR (not dismissed) that can be merged now: not a draft, not queued, no merge click under way, no conflict,
 * green CI, no change request and nothing blocking it.
 */
export function prMergeable(p: PullRequest): boolean {
  return (
    p.state === "OPEN" &&
    !p.isDraft &&
    !p.queued &&
    !p.mergeAttempt &&
    p.mergeable !== "CONFLICTING" &&
    ciOf(p) === "success" &&
    p.reviewDecision !== "CHANGES_REQUESTED" &&
    p.reviewDecision !== "REVIEW_REQUIRED" &&
    p.mergeStateStatus !== "BLOCKED"
  );
}

/** A PR waits for the user: open (and not dismissed), and its CI failed or it can be merged. */
export function prNeedsHand(p: PullRequest): boolean {
  if (p.hidden || p.state !== "OPEN") return false;
  return ciOf(p) === "failure" || prMergeable(p);
}

/** Every item that waits for the user, sorted: project (in `repos` order, unknown last) → kind → arrival. */
export function attentionItems(src: AttentionSource): AttentionItem[] {
  const byPath = new Map(src.worktrees.map((w) => [normPath(w.path), w]));
  const wtOf = (p: string) => (p ? byPath.get(normPath(p)) : undefined);
  // a Maestro works on the project folder itself: a path that is a project's root belongs to it
  const rootOf = (p: string) => wtOf(p)?.root ?? src.repos.find((r) => samePath(r.root, p))?.root ?? "";
  const nameOf = (p: string) => wtOf(p)?.name ?? p.replace(/[\\/]+$/, "").split(/[\\/]/).pop() ?? p;
  const items: AttentionItem[] = [];

  for (const a of src.approvals) {
    items.push({
      id: `approval:${a.id}`,
      kind: "approval",
      root: rootOf(a.worktreePath),
      path: a.worktreePath,
      name: nameOf(a.worktreePath),
      title: a.toolName || "도구",
      detail: approvalSummary(a),
      tool: a.toolName || undefined,
      approvalId: a.id,
    });
  }
  for (const w of src.tuiWaits) {
    items.push({
      id: `tui:${normPath(w.worktreePath)}`,
      kind: "tui",
      root: rootOf(w.worktreePath),
      path: w.worktreePath,
      name: nameOf(w.worktreePath),
      title: w.toolName ? `터미널에서 답을 기다려요 · ${w.toolName}` : "터미널에서 답을 기다려요",
      tool: w.toolName || undefined,
    });
  }
  for (const m of src.merges) {
    items.push({
      id: `merge:${m.id}`,
      kind: "merge",
      root: m.root || rootOf(m.maestroPath),
      path: m.maestroPath,
      name: m.workerName,
      title: `${m.branch} → ${m.base} · 파일 ${m.fileCount} · ${m.verified ? "검증 있음" : "검증 없음"}`,
      mergeId: m.id,
      worker: m.workerPath,
      files: m.fileCount,
      at: m.requestedAt,
    });
  }
  for (const p of src.prs) {
    if (!prNeedsHand(p)) continue;
    const failed = ciOf(p) === "failure";
    // the Maestro shows the PR card; with the Maestro gone the worker's own block does
    const lead = p.maestroPath && wtOf(p.maestroPath) ? p.maestroPath : p.workerPath;
    items.push({
      id: `pr:${normPath(p.workerPath)}`,
      kind: "pr",
      root: p.root || rootOf(lead),
      path: lead,
      name: p.workerName,
      title: `PR #${p.number} · ${failed ? "CI 실패" : "병합할 수 있어요"}`,
      worker: p.workerPath,
      failed,
    });
  }

  const order = (root: string) => {
    const i = src.repos.findIndex((r) => r.root === root);
    return i < 0 ? src.repos.length : i;
  };
  // Array.prototype.sort is stable: within a project and kind, the order they came in
  // (projects not in the list come last, each kept together)
  const byRoot = (a: AttentionItem, b: AttentionItem) => order(a.root) - order(b.root) || (a.root < b.root ? -1 : a.root > b.root ? 1 : 0);
  return items.sort((a, b) => byRoot(a, b) || KIND_ORDER.indexOf(a.kind) - KIND_ORDER.indexOf(b.kind));
}

/** It keeps an agent from going on (an approval, a terminal question, a merge request); a PR doesn't. */
export const isBlocking = (i: AttentionItem) => i.kind !== "pr";

/** How many items keep an agent waiting. */
export const blockingCount = (items: AttentionItem[]) => items.filter(isBlocking).length;

/** How many items each project has. */
export function countByRoot(items: AttentionItem[]): Record<string, number> {
  const out: Record<string, number> = {};
  for (const i of items) out[i.root] = (out[i.root] ?? 0) + 1;
  return out;
}

/** Whether something keeps the agent in `path` waiting (the status glyph's 승인 대기). */
export const waitsAt = (items: AttentionItem[], path: string) => items.some((i) => isBlocking(i) && samePath(i.path, path));

/**
 * The one item a worker card shows in place of its prompt box (docs/prd stage 11): the first that keeps this agent
 * waiting, in list order. Its own tool approval or terminal question, or a merge request of its branch (that one is
 * the Maestro's to answer, but it is about this worker). A PR isn't one: the card's PR chip shows it.
 */
export function cardItem(items: AttentionItem[], path: string): AttentionItem | undefined {
  return items.find((i) => (i.kind === "merge" ? !!i.worker && samePath(i.worker, path) : isBlocking(i) && samePath(i.path, path)));
}

/**
 * The item Ctrl+J goes to: the first one after the current agent's (last) item, round to the start; with the current
 * agent having none (or no current agent), the first item.
 */
export function nextItem(items: AttentionItem[], currentPath?: string | null): AttentionItem | undefined {
  if (!items.length) return undefined;
  let at = -1;
  if (currentPath) items.forEach((it, i) => samePath(it.path, currentPath) && (at = i));
  return items[(at + 1) % items.length];
}

/** The items grouped by project, in list order (for the popover). */
export function groupByRoot(items: AttentionItem[]): Array<{ root: string; items: AttentionItem[] }> {
  const groups: Array<{ root: string; items: AttentionItem[] }> = [];
  for (const it of items) {
    const g = groups[groups.length - 1];
    if (g && g.root === it.root) g.items.push(it);
    else groups.push({ root: it.root, items: [it] });
  }
  return groups;
}
