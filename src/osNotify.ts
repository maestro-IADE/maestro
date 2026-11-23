// OS 알림 (docs/features/ui/os-notifications.md): what to tell the user while no Maestro window has focus. An agent
// starts waiting (a new approval, terminal question or merge request: the 확인할 일 that keep it waiting), or its turn
// ends (RUNNING → IDLE). One notice per agent per change; notices that come close together go out as one summary.
// Pure: no React, no store, no Tauri, so scripts/os-notify-check.ts can test it. Only `import type` here (Node runs
// the tests without a bundler).
import type { AttentionItem } from "./attention";
import type { AgentStatus } from "./types";

/** 설정 → 일반 → OS 알림: waits and finished turns, waits only, or nothing. */
export type OsNotify = "all" | "waiting" | "off";

/** An agent as the notifier compares it. */
export type NotifyAgent = { path: string; name: string; root: string; status: AgentStatus; isMaestro: boolean };

/** One look at the store: what waits, the agents, and the projects' names. */
export type NotifySnap = {
  items: AttentionItem[];
  agents: NotifyAgent[];
  projects: Array<{ root: string; name: string }>;
};

/**
 * Something worth a notification: an agent began to wait, or (level "all") its turn ended. `ids`: the items a wait
 * tells of (none for a finished turn), so it is dropped once those are answered even if older ones still wait.
 */
export type Notice = { path: string; kind: "wait" | "done"; title: string; body: string; ids: string[] };

/** The store while the app starts: the project list, each project's agents, and what already waited (the backend's lists). */
export type Startup = { ready: boolean; listed: boolean; repos: Array<{ root: string }>; scanned: string[] };

/**
 * The launch (or a reload) has loaded everything: until then what comes in had been waiting already (a merge request
 * kept across restarts, an approval still open) and is no news. The worktrees too, so the titles name the Lead.
 */
export const loaded = (s: Startup) => s.ready && s.listed && s.repos.every((r) => s.scanned.includes(r.root));

/** How long notices gather before they go out: more than one agent in that time becomes one summary. */
export const BURST_MS = 2000;
export const DONE_TEXT = "작업이 끝났어요";
export const SUMMARY_TITLE = "Maestro";
export const summaryText = (n: number) => `에이전트 ${n}개가 기다려요`;

// the rule of `samePath` (attention.ts), copied: one pure module can't import another at run time
const norm = (p: string) => p.replace(/\\/g, "/").replace(/\/+$/, "").toLowerCase();

/**
 * The notices for the change from `before` to `after`. A wait: blocking items (not a PR) whose id `before` lacked,
 * one notice per agent they belong to (`item.path`: the asking agent, the Lead for a merge request), its body what
 * that agent's sidebar row would say of those new ones alone (`reason`, sidebarModel's `rowMeta` + `reasonText` in the
 * app; the first item's own line when it says nothing), so "병합 요청 N" counts the new requests. A finished turn
 * (only at "all"): an agent RUNNING before and IDLE now, and not already told of for a wait. Titles are
 * "agent · project".
 */
export function notices(
  before: NotifySnap,
  after: NotifySnap,
  level: OsNotify,
  reason: (agent: Pick<NotifyAgent, "path" | "isMaestro">, items: AttentionItem[]) => string | null,
): Notice[] {
  if (level === "off") return [];
  const agentOf = (p: string) => after.agents.find((a) => norm(a.path) === norm(p));
  const projectOf = (root: string) => after.projects.find((r) => norm(r.root) === norm(root))?.name;
  const title = (name: string, root: string) => {
    const project = root ? projectOf(root) : undefined;
    return project ? `${name} · ${project}` : name;
  };

  const seen = new Set(before.items.map((i) => i.id));
  const fresh = new Map<string, AttentionItem[]>();
  for (const i of after.items) {
    if (i.kind === "pr" || seen.has(i.id)) continue;
    const k = norm(i.path);
    fresh.set(k, [...(fresh.get(k) ?? []), i]);
  }

  const out: Notice[] = [];
  for (const items of fresh.values()) {
    const first = items[0];
    const agent = agentOf(first.path);
    // a merge request belongs to the Lead that asked, known or not
    const who = agent ?? { path: first.path, isMaestro: items.some((i) => i.kind === "merge") };
    out.push({
      path: first.path,
      kind: "wait",
      title: title(agent?.name ?? first.name, agent?.root ?? first.root),
      body: reason(who, items) ?? first.title,
      ids: items.map((i) => i.id),
    });
  }
  if (level !== "all") return out;

  const was = new Map(before.agents.map((a) => [norm(a.path), a.status]));
  for (const a of after.agents) {
    const k = norm(a.path);
    if (was.get(k) !== "RUNNING" || a.status !== "IDLE" || fresh.has(k)) continue;
    out.push({ path: a.path, kind: "done", title: title(a.name, a.root), body: DONE_TEXT, ids: [] });
  }
  return out;
}

/**
 * The gathered notices still true when they go out, at the level set then: a wait one of whose own items is still
 * there (not answered meanwhile; an older item of that agent doesn't count), a finished turn whose agent hasn't
 * started another (a Lead goes on as soon as a worker's report comes in).
 */
export function live(pending: Notice[], now: NotifySnap, level: OsNotify): Notice[] {
  if (level === "off") return [];
  return pending.filter((n) =>
    n.kind === "wait"
      ? now.items.some((i) => n.ids.includes(i.id))
      : level === "all" && now.agents.some((a) => norm(a.path) === norm(n.path) && a.status === "IDLE"),
  );
}

/**
 * What goes out for the notices gathered in one burst: one per agent (a wait over a finished turn, else the later),
 * and once that leaves more than one agent, a single summary instead.
 */
export function coalesce(pending: Notice[]): Array<{ title: string; body: string }> {
  const byAgent = new Map<string, Notice>();
  for (const n of pending) {
    const k = norm(n.path);
    const had = byAgent.get(k);
    if (!had || n.kind === "wait" || had.kind === "done") byAgent.set(k, n);
  }
  const list = [...byAgent.values()];
  if (list.length > 1) return [{ title: SUMMARY_TITLE, body: summaryText(list.length) }];
  return list.map(({ title, body }) => ({ title, body }));
}
