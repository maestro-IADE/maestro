// OS 알림 (docs/features/ui/os-notifications.md): what to tell the user while no Maestro window has focus. An agent
// starts waiting (a new approval, terminal question or merge request: the 확인할 일 that keep it waiting; or a question
// it put to the user, in its chat or on its canvas), or it is really done (its turn ended, it reports to no Lead that
// can hear it, none of its workers still runs, nothing that came in this turn, of it or of its workers, still waits on
// the user). One notice per agent per change; notices that come close together go out as one summary.
// Pure: no React, no store, no Tauri, so scripts/os-notify-check.ts can test it. Only `import type` here (Node runs
// the tests without a bundler).
import type { AttentionItem } from "./attention";
import type { AgentStatus } from "./types";
import type { Queued } from "./visualSteps";

/** 설정 → 일반 → OS 알림: waits and finished turns, waits only, or nothing. */
export type OsNotify = "all" | "waiting" | "off";

/** An agent as the notifier compares it. `workers`: its workers' paths (the project hierarchy). */
export type NotifyAgent = { path: string; name: string; root: string; status: AgentStatus; isMaestro: boolean; workers: string[] };

/**
 * A question an agent put to the user (`maestro ask`): a choice in its chat (`chat`) or a question or question set on
 * its canvas. `text`: the notice's words for it (`askText`).
 */
export type Asked = { id: string; path: string; text: string; chat: boolean };

/**
 * One look at the store: what waits, the questions still asked, the agents, and the projects' names. `held`: the ids
 * of the waits (blocking items, questions) that came in since their agent last began a turn (`heldAfter`): they keep
 * its turn's end (and its Lead's) from being really done; an older one doesn't (the user may have left it and gone on).
 */
export type NotifySnap = {
  items: AttentionItem[];
  asks: Asked[];
  agents: NotifyAgent[];
  projects: Array<{ root: string; name: string }>;
  held: string[];
};

/**
 * Something worth a notification: an agent began to wait, or (level "all") it is done. `ids`: the items and questions
 * a wait tells of (none for a finished turn), so it is dropped once those are answered even if older ones still wait.
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
/** The summary of a burst: how many agents wait and how many are done (each agent counted once). */
export const summaryText = (waits: number, dones: number) =>
  !dones ? `에이전트 ${waits}개가 기다려요` : !waits ? `에이전트 ${dones}개가 작업을 끝냈어요` : `기다림 ${waits} · 끝남 ${dones}`;
/** A question's words in a notice. */
export const askText = (title: string) => `질문 · ${title.trim()}`;

// the rule of `samePath` (attention.ts), copied: one pure module can't import another at run time
const norm = (p: string) => p.replace(/\\/g, "/").replace(/\/+$/, "").toLowerCase();

/**
 * The questions in `found` (read for one agent) that are news: never met before in this window (`known`: every id met,
 * heard or not; this adds them) and read because the agent just asked (`heard`: its `maestro://asks` or
 * `maestro://draw` event). Read on showing a chat or opening a canvas, they had been waiting already: only remembered,
 * so never news later either. A question read from the backend's queue and later taken onto the canvas is met once.
 */
export function newAsks<T extends { id: string }>(found: readonly T[], known: Set<string>, heard: boolean): T[] {
  const news: T[] = [];
  for (const a of found) {
    if (known.has(a.id)) continue;
    known.add(a.id);
    news.push(a);
  }
  return heard ? news : [];
}

/**
 * The questions in what an agent queued for its canvas (`maestro ask`: `ask`, `ask --file`: `ask_set`), as the
 * notifier keeps them (without `path`). A set says its name, or how many questions it has.
 */
export function asksIn(items: readonly Queued[]): Array<{ id: string; text: string; chat: false }> {
  return items.flatMap((d) => {
    if (d.ask?.id) return [{ id: d.ask.id, text: askText(d.ask.title ?? ""), chat: false as const }];
    if (d.ask_set?.id) {
      const name = d.ask_set.title?.trim();
      return [{ id: d.ask_set.id, text: name ? askText(name) : `질문 ${d.ask_set.questions?.length ?? 0}개`, chat: false as const }];
    }
    return [];
  });
}

/**
 * The questions still waiting, as far as the notifier can tell: a chat choice while it is in its agent's chat
 * (`chat`: the choices waiting, by `norm`ed path, as chatAsks.ts keeps them; answering or closing takes it away). A
 * canvas question stays until its agent begins another turn (`askedAfter`): the canvas doesn't tell this window when
 * it is answered.
 */
export const waitingAsks = (list: Asked[], chat: Record<string, ReadonlyArray<{ id: string }>>) =>
  list.filter((a) => !a.chat || (chat[norm(a.path)] ?? []).some((c) => c.id === a.id));

/** The agents (by `norm`ed path) that began another turn from `before` to `after`: listed then, not RUNNING, RUNNING now. */
function began(before: NotifySnap, after: NotifySnap): Set<string> {
  const was = new Map(before.agents.map((a) => [norm(a.path), a.status]));
  return new Set(
    after.agents.filter((a) => a.status === "RUNNING" && was.has(norm(a.path)) && was.get(norm(a.path)) !== "RUNNING").map((a) => norm(a.path)),
  );
}

/**
 * The kept questions once the change from `before` to `after` is counted: an agent that began another turn (RUNNING
 * now, something else before) no longer waits on its canvas questions (an answer comes in as a turn). Its chat choices
 * stay: they wait in its chat until answered or closed. The same list when nothing goes.
 */
export function askedAfter(list: Asked[], before: NotifySnap, after: NotifySnap): Asked[] {
  const go = began(before, after);
  if (!go.size) return list;
  const kept = list.filter((a) => a.chat || !go.has(norm(a.path)));
  return kept.length === list.length ? list : kept;
}

/**
 * `held` (NotifySnap) once the change from `before` to `after` is counted: the waits new in `after` (blocking items, not
 * a PR, and questions whose id `before` lacked) join; one that no longer waits leaves, and so does every wait of an
 * agent that began another turn (it came in an earlier turn: a merge request or a chat choice the user left while
 * talking to the agent on doesn't keep its later turns from ending).
 */
export function heldAfter(held: readonly string[], before: NotifySnap, after: NotifySnap): string[] {
  const go = began(before, after);
  const pathOf = new Map([...before.items, ...before.asks].map((w) => [w.id, norm(w.path)]));
  const waits = [...after.items.filter((i) => i.kind !== "pr"), ...after.asks];
  const now = new Set(waits.map((w) => w.id));
  const kept = held.filter((id) => now.has(id) && !go.has(pathOf.get(id) ?? ""));
  return [...kept, ...waits.filter((w) => !pathOf.has(w.id) && !kept.includes(w.id)).map((w) => w.id)];
}

/**
 * Agent `a` is really done in `s`, not just between turns. It isn't when it reports to a Lead that can hear it (its
 * path is in the `workers` of another agent that isn't off: its turn's end goes to that Lead, not to the user; an off
 * Lead keeps the report until it starts, so then the user is told), when one of its workers (or theirs, however deep)
 * still runs (the Lead goes on when their reports come in), or when something that came in this turn (`held`) still
 * waits on the user, of it or of one of those workers (that wait was told already, and the team is stuck on it).
 */
function done(a: NotifyAgent, s: NotifySnap): boolean {
  const k = norm(a.path);
  if (s.agents.some((o) => norm(o.path) !== k && o.status !== "TERMINATED" && o.workers.some((w) => norm(w) === k))) return false;
  const held = new Set(s.held);
  const waits = (p: string) => [...s.items, ...s.asks].some((w) => held.has(w.id) && norm(w.path) === p);
  if (waits(k)) return false;
  const byPath = new Map(s.agents.map((o) => [norm(o.path), o]));
  const seen = new Set([k]);
  const down = [...a.workers];
  for (let w = down.pop(); w !== undefined; w = down.pop()) {
    const kw = norm(w);
    if (seen.has(kw)) continue;
    seen.add(kw);
    const o = byPath.get(kw);
    if (o?.status === "RUNNING" || waits(kw)) return false;
    if (o) down.push(...o.workers);
  }
  return true;
}

/**
 * The notices for the change from `before` to `after`. A wait: blocking items (not a PR) whose id `before` lacked,
 * and questions (`asks`) whose id `before` lacked, of an agent the app lists (the developer tests' stand-in is none);
 * one notice per agent they belong to (`item.path`: the asking agent, the Lead for a merge request). Its body is what
 * that agent's sidebar row would say of the new items alone (`reason`, sidebarModel's `rowMeta` + `reasonText` in the
 * app; the first item's own line when it says nothing), so "병합 요청 N" counts the new requests; with only new
 * questions, the first one's words ("질문 · …"). A finished turn (only at "all"): an agent RUNNING before and IDLE
 * now, not already told of for a wait, and really done (`done`). Titles are "agent · project".
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
  const heard = new Set(before.asks.map((a) => a.id));
  const asked = new Map<string, Asked[]>();
  for (const a of after.asks) {
    if (heard.has(a.id) || !agentOf(a.path)) continue;
    const k = norm(a.path);
    asked.set(k, [...(asked.get(k) ?? []), a]);
  }

  const out: Notice[] = [];
  for (const [k, items] of fresh) {
    const first = items[0];
    const agent = agentOf(first.path);
    // a merge request belongs to the Lead that asked, known or not
    const who = agent ?? { path: first.path, isMaestro: items.some((i) => i.kind === "merge") };
    out.push({
      path: first.path,
      kind: "wait",
      title: title(agent?.name ?? first.name, agent?.root ?? first.root),
      body: reason(who, items) ?? first.title,
      ids: [...items.map((i) => i.id), ...(asked.get(k) ?? []).map((a) => a.id)],
    });
  }
  for (const [k, asks] of asked) {
    const agent = agentOf(asks[0].path);
    if (fresh.has(k) || !agent) continue;
    out.push({ path: agent.path, kind: "wait", title: title(agent.name, agent.root), body: asks[0].text, ids: asks.map((a) => a.id) });
  }
  if (level !== "all") return out;

  const was = new Map(before.agents.map((a) => [norm(a.path), a.status]));
  for (const a of after.agents) {
    const k = norm(a.path);
    if (was.get(k) !== "RUNNING" || a.status !== "IDLE" || fresh.has(k) || asked.has(k) || !done(a, after)) continue;
    out.push({ path: a.path, kind: "done", title: title(a.name, a.root), body: DONE_TEXT, ids: [] });
  }
  return out;
}

/**
 * The gathered notices still true when they go out, at the level set then: a wait one of whose own items or questions
 * is still there (not answered meanwhile; an older item of that agent doesn't count), a finished turn whose agent
 * hasn't started another (a Lead goes on as soon as a worker's report comes in) and is still really done (no worker of
 * it began to run, nothing of it or of its workers began to wait meanwhile).
 */
export function live(pending: Notice[], now: NotifySnap, level: OsNotify): Notice[] {
  if (level === "off") return [];
  return pending.filter((n) => {
    if (n.kind === "wait") return now.items.some((i) => n.ids.includes(i.id)) || now.asks.some((a) => n.ids.includes(a.id));
    const a = now.agents.find((x) => norm(x.path) === norm(n.path));
    return level === "all" && !!a && a.status === "IDLE" && done(a, now);
  });
}

/**
 * What goes out for the notices gathered in one burst: one per agent (a wait over a finished turn, else the later),
 * and once that leaves more than one agent, a single summary instead that says how many wait and how many are done.
 */
export function coalesce(pending: Notice[]): Array<{ title: string; body: string }> {
  const byAgent = new Map<string, Notice>();
  for (const n of pending) {
    const k = norm(n.path);
    const had = byAgent.get(k);
    if (!had || n.kind === "wait" || had.kind === "done") byAgent.set(k, n);
  }
  const list = [...byAgent.values()];
  if (list.length > 1) {
    const waits = list.filter((n) => n.kind === "wait").length;
    return [{ title: SUMMARY_TITLE, body: summaryText(waits, list.length - waits) }];
  }
  return list.map(({ title, body }) => ({ title, body }));
}
