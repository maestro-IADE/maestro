// OS 알림 (src/osNotify.ts): which changes become notices, their words, the setting's levels, what is dropped as no
// longer true, and how a burst folds into one. Run: `npm test`.
import { deepStrictEqual, equal } from "node:assert/strict";
import { askedAfter, asksIn, askText, coalesce, DONE_TEXT, heldAfter, live, loaded, newAsks, notices, summaryText, SUMMARY_TITLE, waitingAsks, type Asked, type Notice, type NotifyAgent, type NotifySnap } from "../src/osNotify.ts";
import { attentionItems } from "../src/attention.ts";
import { reasonText, rowMeta } from "../src/sidebarModel.ts";
import type { AttentionItem } from "../src/attention.ts";
import type { AgentStatus, Approval, MergeRequest, PullRequest, TuiWait } from "../src/types.ts";

// the app's words: what that agent's sidebar row says of what waits
const reason = (a: Pick<NotifyAgent, "path" | "isMaestro">, items: AttentionItem[]) => reasonText(rowMeta({ ...a, adapterId: "asking" }, items));

const approval = (id: number, worktreePath: string, toolName = "Bash") => ({ id, worktreePath, toolName, toolInput: {}, canAlways: false }) as Approval;
const tui = (worktreePath: string) => ({ worktreePath, source: "cli", toolName: null }) as TuiWait;
const merge = (id: number, maestroPath: string, workerPath: string) =>
  ({ id, root: "A", maestroPath, workerPath, workerName: "로그인 고치기", branch: "b", base: "main", fileCount: 3, verified: null, requestedAt: id }) as MergeRequest;
const pr = (workerPath: string) =>
  ({
    root: "A", maestroPath: "C:\\A", workerPath, workerName: "w", number: 7, state: "OPEN", isDraft: false, queued: false, mergeAttempt: null,
    mergeable: "MERGEABLE", mergeStateStatus: "CLEAN", reviewDecision: null, headSha: "h", hidden: false,
    checks: { state: "failure", passed: 0, failed: 1, pending: 0, failedJobs: [], headSha: "h" },
  }) as unknown as PullRequest;

// (no hierarchy here: the workers' rules have their own agents below)
const agents: NotifyAgent[] = [
  { path: "C:\\A", name: "Lead", root: "A", status: "IDLE", isMaestro: true, workers: [] },
  { path: "C:\\w\\a1", name: "로그인 고치기", root: "A", status: "RUNNING", isMaestro: false, workers: [] },
  { path: "C:\\w\\a2", name: "api 테스트", root: "A", status: "RUNNING", isMaestro: false, workers: [] },
  { path: "C:\\x\\solo", name: "혼자", root: "X", status: "RUNNING", isMaestro: false, workers: [] },
];
const projects = [{ root: "A", name: "maestro" }];
// `held`: the waits that came in since their agent last began a turn (the app counts them with `heldAfter`)
type Src = { approvals?: Approval[]; tuiWaits?: TuiWait[]; trustWaits?: string[]; merges?: MergeRequest[]; prs?: PullRequest[]; status?: Record<string, AgentStatus>; asks?: Asked[]; held?: string[]; of?: NotifyAgent[] };
const snap = (o: Src = {}): NotifySnap => {
  const list = (o.of ?? agents).map((a) => ({ ...a, status: o.status?.[a.path] ?? a.status }));
  return {
    items: attentionItems({
      approvals: o.approvals ?? [],
      tuiWaits: o.tuiWaits ?? [],
      trustWaits: o.trustWaits ?? [],
      merges: o.merges ?? [],
      prs: o.prs ?? [],
      worktrees: list,
      repos: projects,
    }),
    asks: o.asks ?? [],
    agents: list,
    projects,
    held: o.held ?? [],
  };
};
/** The snaps one after another as the app sees them: each one's `held` counted from the one before (`heldAfter`). */
const steps = (...list: NotifySnap[]) =>
  list.reduce<NotifySnap[]>((out, s) => [...out, out.length ? { ...s, held: heldAfter(out[out.length - 1].held, out[out.length - 1], s) } : s], []);
const empty = snap();

// a: an approval comes in → one notice for the asking agent, "agent · project", the row's words
deepStrictEqual(notices(empty, snap({ approvals: [approval(1, "C:\\w\\a1")] }), "all", reason), [
  { path: "C:\\w\\a1", kind: "wait", title: "로그인 고치기 · maestro", body: "승인 대기 · Bash", ids: ["approval:1"] },
]);
// the same at "waiting"; nothing at "off"
equal(notices(empty, snap({ approvals: [approval(1, "C:\\w\\a1")] }), "waiting", reason).length, 1);
deepStrictEqual(notices(empty, snap({ approvals: [approval(1, "C:\\w\\a1")] }), "off", reason), []);

// b: one already there before is not news; a second for the same agent is, as one notice
const one = snap({ approvals: [approval(1, "C:\\w\\a1")] });
deepStrictEqual(notices(one, one, "all", reason), [], "nothing new");
deepStrictEqual(
  notices(empty, snap({ approvals: [approval(1, "C:\\w\\a1"), approval(2, "C:\\w\\a1", "Edit")] }), "all", reason).map((n) => n.body),
  ["승인 대기 · Bash"],
  "two at once for one agent: one notice",
);
deepStrictEqual(notices(one, snap({ approvals: [approval(1, "C:\\w\\a1"), approval(2, "C:\\w\\a1", "Edit")] }), "all", reason).map((n) => n.body), ["승인 대기 · Edit"], "only the new one is said");

// c: a terminal question, a merge request (the Lead's row: "병합 요청 1"), a PR (never: it keeps nobody waiting)
deepStrictEqual(notices(empty, snap({ tuiWaits: [tui("C:\\w\\a2")] }), "all", reason), [
  { path: "C:\\w\\a2", kind: "wait", title: "api 테스트 · maestro", body: "터미널 질문", ids: ["tui:c:/w/a2"] },
]);
deepStrictEqual(notices(empty, snap({ merges: [merge(4, "C:\\A", "C:\\w\\a1")] }), "all", reason), [
  { path: "C:\\A", kind: "wait", title: "Lead · maestro", body: "병합 요청 1", ids: ["merge:4"] },
]);
deepStrictEqual(notices(empty, snap({ prs: [pr("C:\\w\\a1")] }), "all", reason), [], "a PR needing a hand is no notice");
// the folder trust question (a worker made in the background stops there): a wait, in the row's words
deepStrictEqual(notices(empty, snap({ trustWaits: ["c:/w/a2/"] }), "waiting", reason), [
  { path: "c:/w/a2/", kind: "wait", title: "api 테스트 · maestro", body: "신뢰 대기", ids: ["trust:c:/w/a2"] },
]);
// one yes covers the project: every agent's wait ends, so the gathered notices are dropped
const trusting = snap({ trustWaits: ["C:\\A", "C:\\w\\a1"] });
const trustNotes = notices(empty, trusting, "all", reason);
deepStrictEqual(trustNotes.map((n) => [n.title, n.body]), [["Lead · maestro", "신뢰 대기"], ["로그인 고치기 · maestro", "신뢰 대기"]]);
deepStrictEqual(live(trustNotes, trusting, "all"), trustNotes, "still waiting");
deepStrictEqual(live(trustNotes, empty, "all"), [], "accepted meanwhile");

// d: two agents start waiting in one change → a notice each, in list order
deepStrictEqual(
  notices(empty, snap({ approvals: [approval(1, "C:\\w\\a2")], tuiWaits: [tui("C:\\w\\a1")] }), "all", reason).map((n) => n.path),
  ["C:\\w\\a2", "C:\\w\\a1"],
);

// e: a turn ends (RUNNING → IDLE): a notice at "all" only
const ended = snap({ status: { "C:\\w\\a1": "IDLE" } });
deepStrictEqual(notices(empty, ended, "all", reason), [{ path: "C:\\w\\a1", kind: "done", title: "로그인 고치기 · maestro", body: DONE_TEXT, ids: [] }]);
deepStrictEqual(notices(empty, ended, "waiting", reason), [], "기다릴 때만: no finished turns");
deepStrictEqual(notices(empty, snap({ status: { "C:\\w\\a1": "TERMINATED" } }), "all", reason), [], "stopped is not finished");
deepStrictEqual(notices(ended, ended, "all", reason), [], "IDLE → IDLE");
deepStrictEqual(notices(ended, empty, "all", reason), [], "IDLE → RUNNING");
// an agent not there before (just loaded) is no finished turn
deepStrictEqual(notices({ ...empty, agents: [] }, ended, "all", reason), []);
// the same folder spelled another way
deepStrictEqual(
  notices({ ...empty, agents: [{ ...agents[1], path: "c:/w/a1/" }] }, ended, "all", reason).map((n) => n.kind),
  ["done"],
);

// f: a turn that ends on a question is told once, as the wait
deepStrictEqual(
  notices(empty, snap({ status: { "C:\\w\\a1": "IDLE" }, tuiWaits: [tui("C:\\w\\a1")] }), "all", reason).map((n) => n.kind),
  ["wait"],
);

// g: a project not in the list: the agent's name alone; no words from the row: the item's own line
deepStrictEqual(notices(empty, snap({ status: { "C:\\x\\solo": "IDLE" } }), "all", reason).map((n) => n.title), ["혼자"]);
deepStrictEqual(notices(empty, snap({ approvals: [approval(1, "C:\\w\\a1")] }), "all", () => null).map((n) => n.body), ["Bash"]);

// live: what is still true when the burst goes out
const wait = (path: string, ids = ["approval:1"]): Notice => ({ path, kind: "wait", title: path, body: "승인 대기", ids });
const done = (path: string): Notice => ({ path, kind: "done", title: path, body: DONE_TEXT, ids: [] });
deepStrictEqual(live([wait("C:\\w\\a1")], one, "all"), [wait("C:\\w\\a1")], "still waiting");
deepStrictEqual(live([wait("C:\\w\\a1")], empty, "all"), [], "answered meanwhile");
// a Lead with an older merge request: a new approval answered meanwhile is dropped, the merge doesn't keep it
const leadMerge = snap({ merges: [merge(4, "C:\\A", "C:\\w\\a1")] });
const leadAsks = notices(leadMerge, snap({ merges: [merge(4, "C:\\A", "C:\\w\\a1")], approvals: [approval(9, "C:\\A")] }), "all", reason);
deepStrictEqual(leadAsks.map((n) => [n.title, n.body, n.ids]), [["Lead · maestro", "승인 대기 · Bash", ["approval:9"]]]);
deepStrictEqual(live(leadAsks, leadMerge, "all"), [], "only the old merge request is left");
deepStrictEqual(live(leadAsks, snap({ merges: [merge(4, "C:\\A", "C:\\w\\a1")], approvals: [approval(9, "C:\\A")] }), "all"), leadAsks);
// two new items in one notice: one still there keeps it
deepStrictEqual(live([wait("C:\\w\\a1", ["approval:2", "approval:1"])], one, "all").length, 1);
deepStrictEqual(live([wait("C:\\w\\a1")], snap({ prs: [pr("C:\\w\\a1")] }), "all"), [], "a PR doesn't keep it waiting");
deepStrictEqual(live([done("C:\\w\\a1")], ended, "all"), [done("C:\\w\\a1")], "still idle");
deepStrictEqual(live([done("C:\\w\\a1")], empty, "all"), [], "started another turn");
deepStrictEqual(live([done("C:\\w\\a1"), wait("C:\\w\\a1")], snap({ status: { "C:\\w\\a1": "IDLE" }, approvals: [approval(1, "C:\\w\\a1")] }), "waiting"), [wait("C:\\w\\a1")], "the level turned down meanwhile");
deepStrictEqual(live([wait("C:\\w\\a1")], one, "off"), []);

// coalesce: one per agent, more than one agent → one summary
deepStrictEqual(coalesce([]), []);
deepStrictEqual(coalesce([wait("C:\\w\\a1")]), [{ title: "C:\\w\\a1", body: "승인 대기" }]);
deepStrictEqual(coalesce([done("C:\\w\\a1"), wait("c:/w/a1")]), [{ title: "c:/w/a1", body: "승인 대기" }], "a wait over a finished turn");
deepStrictEqual(coalesce([wait("C:\\w\\a1"), done("C:\\w\\a1")]), [{ title: "C:\\w\\a1", body: "승인 대기" }], "in either order");
deepStrictEqual(coalesce([done("C:\\w\\a1"), done("C:\\w\\a1")]), [{ title: "C:\\w\\a1", body: DONE_TEXT }]);
deepStrictEqual(coalesce([wait("C:\\w\\a1"), done("C:\\w\\a2"), wait("C:\\w\\a2")]), [{ title: SUMMARY_TITLE, body: "에이전트 2개가 기다려요" }]);
// the summary says what they are: all waits, all done, or both counts
deepStrictEqual(coalesce([done("C:\\w\\a1"), done("C:\\w\\a2"), done("C:\\A")]), [{ title: SUMMARY_TITLE, body: "에이전트 3개가 작업을 끝냈어요" }], "all done");
deepStrictEqual(coalesce([wait("C:\\w\\a1"), wait("C:\\A"), done("C:\\w\\a2")]), [{ title: SUMMARY_TITLE, body: "기다림 2 · 끝남 1" }], "mixed");
deepStrictEqual(coalesce([done("C:\\w\\a1"), wait("c:/w/a1"), done("C:\\w\\a2")]), [{ title: SUMMARY_TITLE, body: "기다림 1 · 끝남 1" }], "an agent counts once, as its wait");
equal(summaryText(3, 0), "에이전트 3개가 기다려요");
equal(summaryText(0, 2), "에이전트 2개가 작업을 끝냈어요");
equal(summaryText(2, 1), "기다림 2 · 끝남 1");

// workers: a Lead with two workers (one spelled another way), one of which has a worker of its own
const team: NotifyAgent[] = [
  { path: "C:\\B", name: "Lead", root: "A", status: "RUNNING", isMaestro: true, workers: ["C:\\t\\w1", "c:/t/w2/"] },
  { path: "C:\\t\\w1", name: "화면", root: "A", status: "RUNNING", isMaestro: false, workers: [] },
  { path: "C:\\t\\w2", name: "서버", root: "A", status: "RUNNING", isMaestro: false, workers: ["C:\\t\\w3"] },
  { path: "C:\\t\\w3", name: "DB", root: "A", status: "RUNNING", isMaestro: false, workers: [] },
];
const crew = (status: Record<string, AgentStatus> = {}, o: Src = {}) => snap({ ...o, of: team, status });
const chatAsk = (id: string, path: string, title: string): Asked => ({ id, path, text: askText(title), chat: true });
const canvasAsk = (id: string, path: string, text: string): Asked => ({ id, path, text, chat: false });
// a worker's turn ending goes to its Lead, not to the user (a worker that has workers too)
deepStrictEqual(notices(crew(), crew({ "C:\\t\\w1": "IDLE" }), "all", reason), [], "a worker finished");
deepStrictEqual(notices(crew(), crew({ "C:\\t\\w2": "IDLE", "C:\\t\\w3": "IDLE" }), "all", reason), [], "a worker with workers, and its worker");
// its waits still go out: an approval, a question
deepStrictEqual(notices(crew(), crew({}, { approvals: [approval(5, "C:\\t\\w1")] }), "all", reason).map((n) => [n.title, n.body]), [["화면 · maestro", "승인 대기 · Bash"]]);
deepStrictEqual(
  notices(crew(), crew({ "C:\\t\\w1": "IDLE" }, { asks: [chatAsk("q_9", "C:\\t\\w1", "어느 쪽?")] }), "all", reason).map((n) => [n.title, n.body]),
  [["화면 · maestro", "질문 · 어느 쪽?"]],
);
// a Lead's turn ending while a worker of its (or of theirs) runs is no done
deepStrictEqual(notices(crew(), crew({ "C:\\B": "IDLE" }), "all", reason), [], "its workers still run");
deepStrictEqual(notices(crew(), crew({ "C:\\B": "IDLE", "C:\\t\\w1": "IDLE" }), "all", reason), [], "one still runs (spelled another way)");
deepStrictEqual(notices(crew(), crew({ "C:\\B": "IDLE", "C:\\t\\w1": "IDLE", "C:\\t\\w2": "IDLE" }), "all", reason), [], "a worker's worker still runs");
const quiet: Record<string, AgentStatus> = { "C:\\t\\w1": "IDLE", "C:\\t\\w2": "TERMINATED", "C:\\t\\w3": "IDLE" };
const leadDone = notices(crew(quiet), crew({ ...quiet, "C:\\B": "IDLE" }), "all", reason);
deepStrictEqual(leadDone, [{ path: "C:\\B", kind: "done", title: "Lead · maestro", body: DONE_TEXT, ids: [] }], "none runs: done");
// checked again when it goes out: a worker began to run meanwhile
deepStrictEqual(live(leadDone, crew({ ...quiet, "C:\\B": "IDLE" }), "all"), leadDone);
deepStrictEqual(live(leadDone, crew({ ...quiet, "C:\\B": "IDLE", "C:\\t\\w3": "RUNNING" }), "all"), [], "a worker runs again");
// a worker's finished turn gathered before it was listed as one is dropped when it goes out
deepStrictEqual(live([done("C:\\t\\w1")], crew({ "C:\\t\\w1": "IDLE" }), "all"), []);
// a worker whose Lead is off: the report waits on disk until that Lead starts, so the user hears it
deepStrictEqual(
  notices(crew({ "C:\\B": "TERMINATED" }), crew({ "C:\\B": "TERMINATED", "C:\\t\\w1": "IDLE" }), "all", reason),
  [{ path: "C:\\t\\w1", kind: "done", title: "화면 · maestro", body: DONE_TEXT, ids: [] }],
  "its Lead is off",
);
deepStrictEqual(notices(crew({ "C:\\B": "TERMINATED" }), crew({ "C:\\B": "TERMINATED", "C:\\t\\w3": "IDLE" }), "all", reason), [], "its own Lead (a worker of the off one) is on");
deepStrictEqual(live([done("C:\\t\\w1")], crew({ "C:\\B": "IDLE", "C:\\t\\w1": "IDLE" }), "all"), [], "the Lead started meanwhile");
// a worker's question (chat or canvas) or terminal question from this turn keeps its Lead's end from being done: the
// team waits on the user; when it goes out too
const w1Asks: Src = { asks: [chatAsk("q_7", "C:\\t\\w1", "어느 쪽?")], held: ["q_7"] };
const idleTeam: Record<string, AgentStatus> = { "C:\\t\\w1": "IDLE", "C:\\t\\w2": "IDLE", "C:\\t\\w3": "IDLE" };
deepStrictEqual(notices(crew(idleTeam, w1Asks), crew({ ...idleTeam, "C:\\B": "IDLE" }, w1Asks), "all", reason), [], "a worker's question waits");
deepStrictEqual(live(leadDone, crew({ ...quiet, "C:\\B": "IDLE" }, w1Asks), "all"), [], "a worker asked meanwhile");
const w3Tui: Src = { tuiWaits: [tui("C:\\t\\w3")], held: ["tui:c:/t/w3"] };
deepStrictEqual(notices(crew(idleTeam, w3Tui), crew({ ...idleTeam, "C:\\B": "IDLE" }, w3Tui), "all", reason), [], "a worker's worker waits on its terminal");
const w1Trust: Src = { trustWaits: ["C:\\t\\w1"], held: ["trust:c:/t/w1"] };
deepStrictEqual(notices(crew(idleTeam, w1Trust), crew({ ...idleTeam, "C:\\B": "IDLE" }, w1Trust), "all", reason), [], "a worker stopped at the trust question");
// the worker's question from an earlier turn of its (it began another since) no longer holds the Lead
deepStrictEqual(
  notices(crew(idleTeam, { asks: w1Asks.asks }), crew({ ...idleTeam, "C:\\B": "IDLE" }, { asks: w1Asks.asks }), "all", reason).map((n) => n.kind),
  ["done"],
);

// questions the agent asks the user (a choice in its chat, a question on its canvas) are waits: "질문 · …"
const asking = snap({ asks: [chatAsk("q_1", "C:\\w\\a1", "어느 DB?")] });
deepStrictEqual(notices(empty, asking, "waiting", reason), [
  { path: "C:\\w\\a1", kind: "wait", title: "로그인 고치기 · maestro", body: "질문 · 어느 DB?", ids: ["q_1"] },
]);
deepStrictEqual(notices(asking, asking, "all", reason), [], "asked before");
deepStrictEqual(notices(empty, asking, "off", reason), []);
deepStrictEqual(notices(empty, snap({ asks: [chatAsk("q_2", "C:\\dev\\stand-in", "시험")] }), "all", reason), [], "an agent the app doesn't list");
// two at once: one notice, the first one's words; with a new approval: the approval's words, both ids
deepStrictEqual(
  notices(empty, snap({ asks: [canvasAsk("qs_1", "C:\\w\\a2", "질문 3개"), chatAsk("q_3", "c:/w/a2", "B?")] }), "all", reason).map((n) => [n.title, n.body, n.ids]),
  [["api 테스트 · maestro", "질문 3개", ["qs_1", "q_3"]]],
);
deepStrictEqual(
  notices(empty, snap({ approvals: [approval(1, "C:\\w\\a1")], asks: [chatAsk("q_1", "C:\\w\\a1", "어느 DB?")] }), "all", reason).map((n) => [n.body, n.ids]),
  [["승인 대기 · Bash", ["approval:1", "q_1"]]],
);
// a turn that ends on a question is told once, as the wait, whether it ends with the question or later
const askedEnded = snap({ status: { "C:\\w\\a1": "IDLE" }, asks: [chatAsk("q_1", "C:\\w\\a1", "어느 DB?")], held: ["q_1"] });
deepStrictEqual(notices(empty, askedEnded, "all", reason).map((n) => n.kind), ["wait"]);
deepStrictEqual(notices(asking, askedEnded, "all", reason), [], "its question still waits: not done");
// only for the turn it was asked in: a chat choice left open while the user talks on doesn't keep the next ends quiet
const leftOpen = steps(
  empty,
  asking,
  snap({ status: { "C:\\w\\a1": "IDLE" }, asks: asking.asks }),
  snap({ asks: asking.asks }),
  snap({ status: { "C:\\w\\a1": "IDLE" }, asks: asking.asks }),
);
deepStrictEqual(leftOpen.map((s) => s.held), [[], ["q_1"], ["q_1"], [], []]);
deepStrictEqual(notices(leftOpen[1], leftOpen[2], "all", reason), [], "the turn it was asked in");
deepStrictEqual(notices(leftOpen[3], leftOpen[4], "all", reason).map((n) => [n.kind, n.body]), [["done", DONE_TEXT]], "a later turn");
deepStrictEqual(leftOpen[4].asks.map((a) => a.id), ["q_1"], "(the choice itself still waits)");
// the same for a blocking item: a merge request the Lead made this turn holds its end; once it began another, not
const m4 = [merge(4, "C:\\A", "C:\\w\\a1")];
const leadTurns = steps(
  snap({ status: { "C:\\A": "RUNNING" } }),
  snap({ status: { "C:\\A": "RUNNING" }, merges: m4 }),
  snap({ status: { "C:\\A": "IDLE" }, merges: m4 }),
  snap({ status: { "C:\\A": "RUNNING" }, merges: m4 }),
  snap({ status: { "C:\\A": "IDLE" }, merges: m4 }),
);
deepStrictEqual(notices(leadTurns[0], leadTurns[1], "all", reason).map((n) => n.body), ["병합 요청 1"]);
deepStrictEqual(notices(leadTurns[1], leadTurns[2], "all", reason), [], "its merge request waits: not done");
deepStrictEqual(live([done("C:\\A")], leadTurns[2], "all"), []);
deepStrictEqual(notices(leadTurns[3], leadTurns[4], "all", reason).map((n) => n.kind), ["done"], "left while the user talks on");
// live: answered (or closed) meanwhile → dropped; a finished turn whose agent asked meanwhile → dropped
const askNotice = notices(empty, asking, "all", reason);
deepStrictEqual(live(askNotice, asking, "all"), askNotice);
deepStrictEqual(live(askNotice, empty, "all"), [], "answered meanwhile");
deepStrictEqual(live([done("C:\\w\\a1")], askedEnded, "all"), [], "it asked meanwhile");

// asksIn: the questions in what an agent queued for its canvas (a set by its name, or how many)
const q = (id: string, title: string) => ({ id, title, choices: [], multi: false, other: false });
deepStrictEqual(
  asksIn([{ ask: q("q_1", " 어느 DB? ") }, { say: "설명" }, { blocks: [] }, { ask_set: { id: "qs_2", title: "시작 전에", questions: [q("q_2", "a"), q("q_3", "b")] } }, { ask_set: { id: "qs_4", questions: [q("q_5", "a"), q("q_6", "b"), q("q_7", "c")] } }]),
  [
    { id: "q_1", text: "질문 · 어느 DB?", chat: false },
    { id: "qs_2", text: "질문 · 시작 전에", chat: false },
    { id: "qs_4", text: "질문 3개", chat: false },
  ],
);
// waitingAsks: a chat choice while it is in its agent's chat, a canvas question until askedAfter lets it go
const heard = [chatAsk("q_1", "C:\\w\\a1", "A?"), canvasAsk("q_2", "C:\\w\\a1", "질문 · B?"), chatAsk("q_3", "C:\\w\\a2", "C?")];
deepStrictEqual(waitingAsks(heard, { "c:/w/a1": [{ id: "q_1" }] }).map((a) => a.id), ["q_1", "q_2"]);
deepStrictEqual(waitingAsks(heard, {}).map((a) => a.id), ["q_2"], "answered or closed");
// askedAfter: an agent that began another turn no longer waits on its canvas questions; its chat choices stay
const idleA1 = snap({ status: { "C:\\w\\a1": "IDLE" } });
deepStrictEqual(askedAfter(heard, idleA1, empty).map((a) => a.id), ["q_1", "q_3"]);
equal(askedAfter(heard, empty, empty), heard, "no turn began: the same list");
equal(askedAfter(heard, empty, idleA1), heard, "a turn ended");
equal(askedAfter(heard, { ...empty, agents: [] }, empty), heard, "an agent just listed began nothing");
equal(askedAfter([heard[0]], idleA1, empty)[0], heard[0]);
// heldAfter: new waits (not a PR) join; answered ones leave; an agent that began another turn lets its own go, and a
// wait that comes with that start is of the new turn
deepStrictEqual(
  heldAfter([], empty, snap({ approvals: [approval(1, "C:\\w\\a1")], prs: [pr("C:\\w\\a2")], asks: [chatAsk("q_1", "C:\\w\\a2", "A?")] })),
  ["approval:1", "q_1"],
);
deepStrictEqual(heldAfter([], one, one), [], "there before: not of this turn");
deepStrictEqual(heldAfter(["approval:1"], one, empty), [], "answered");
deepStrictEqual(
  heldAfter(
    ["approval:1", "tui:c:/w/a1"],
    snap({ status: { "C:\\w\\a1": "IDLE" }, approvals: [approval(1, "C:\\w\\a2")], tuiWaits: [tui("C:\\w\\a1")] }),
    snap({ approvals: [approval(1, "C:\\w\\a2"), approval(2, "C:\\w\\a1")], tuiWaits: [tui("C:\\w\\a1")] }),
  ),
  ["approval:1", "approval:2"],
);

// newAsks: what reading questions in is news (asked.ts `noteAsks`)
const met = new Set<string>();
const qa = { id: "q_1", text: "질문 · A?", chat: true };
const qb = { id: "q_2", text: "질문 · B?", chat: true };
deepStrictEqual(newAsks([qa], met, true), [qa], "heard: news");
deepStrictEqual(newAsks([qa], met, true), [], "heard again: no news");
deepStrictEqual(newAsks([qb], met, false), [], "read on showing a chat or opening a canvas: no news");
deepStrictEqual(newAsks([qb], met, true), [], "nor when heard later");
const set = asksIn([{ ask_set: { id: "qs_9", title: "시작 전에", questions: [q("q_8", "a")] } }]);
deepStrictEqual(newAsks(set, met, true).map((a) => a.id), ["qs_9"], "peeked from the backend's queue");
deepStrictEqual(newAsks(set, met, true), [], "then taken onto the canvas: once");
deepStrictEqual(newAsks([{ id: "q_10" }, { id: "q_10" }], met, true), [{ id: "q_10" }], "twice in one read: once");

// loaded: the launch is read in (projects, each one's agents, what already waited); until then nothing is news
const start = { ready: true, listed: true, repos: [{ root: "A" }, { root: "X" }], scanned: ["A", "X"] };
equal(loaded(start), true);
equal(loaded({ ...start, ready: false, repos: [], scanned: [] }), false, "project list not read");
equal(loaded({ ...start, listed: false }), false, "waits not asked for yet");
equal(loaded({ ...start, scanned: ["A"] }), false, "a project's agents not read");
equal(loaded({ ...start, scanned: ["X", "gone", "A"] }), true, "a removed project's root left over");
equal(loaded({ ...start, repos: [], scanned: [] }), true, "no projects");

console.log("os-notify-check: ok");
