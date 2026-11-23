// OS 알림 (src/osNotify.ts): which changes become notices, their words, the setting's levels, what is dropped as no
// longer true, and how a burst folds into one. Run: `npm test`.
import { deepStrictEqual, equal } from "node:assert/strict";
import { coalesce, DONE_TEXT, live, notices, summaryText, SUMMARY_TITLE, type Notice, type NotifyAgent, type NotifySnap } from "../src/osNotify.ts";
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

const agents: NotifyAgent[] = [
  { path: "C:\\A", name: "Lead", root: "A", status: "IDLE", isMaestro: true },
  { path: "C:\\w\\a1", name: "로그인 고치기", root: "A", status: "RUNNING", isMaestro: false },
  { path: "C:\\w\\a2", name: "api 테스트", root: "A", status: "RUNNING", isMaestro: false },
  { path: "C:\\x\\solo", name: "혼자", root: "X", status: "RUNNING", isMaestro: false },
];
const projects = [{ root: "A", name: "maestro" }];
type Src = { approvals?: Approval[]; tuiWaits?: TuiWait[]; merges?: MergeRequest[]; prs?: PullRequest[]; status?: Record<string, AgentStatus> };
const snap = (o: Src = {}): NotifySnap => {
  const list = agents.map((a) => ({ ...a, status: o.status?.[a.path] ?? a.status }));
  return {
    items: attentionItems({ approvals: o.approvals ?? [], tuiWaits: o.tuiWaits ?? [], merges: o.merges ?? [], prs: o.prs ?? [], worktrees: list, repos: projects }),
    agents: list,
    projects,
  };
};
const empty = snap();

// a: an approval comes in → one notice for the asking agent, "agent · project", the row's words
deepStrictEqual(notices(empty, snap({ approvals: [approval(1, "C:\\w\\a1")] }), "all", reason), [
  { path: "C:\\w\\a1", kind: "wait", title: "로그인 고치기 · maestro", body: "승인 대기 · Bash" },
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
  { path: "C:\\w\\a2", kind: "wait", title: "api 테스트 · maestro", body: "터미널 질문" },
]);
deepStrictEqual(notices(empty, snap({ merges: [merge(4, "C:\\A", "C:\\w\\a1")] }), "all", reason), [
  { path: "C:\\A", kind: "wait", title: "Lead · maestro", body: "병합 요청 1" },
]);
deepStrictEqual(notices(empty, snap({ prs: [pr("C:\\w\\a1")] }), "all", reason), [], "a PR needing a hand is no notice");

// d: two agents start waiting in one change → a notice each, in list order
deepStrictEqual(
  notices(empty, snap({ approvals: [approval(1, "C:\\w\\a2")], tuiWaits: [tui("C:\\w\\a1")] }), "all", reason).map((n) => n.path),
  ["C:\\w\\a2", "C:\\w\\a1"],
);

// e: a turn ends (RUNNING → IDLE): a notice at "all" only
const ended = snap({ status: { "C:\\w\\a1": "IDLE" } });
deepStrictEqual(notices(empty, ended, "all", reason), [{ path: "C:\\w\\a1", kind: "done", title: "로그인 고치기 · maestro", body: DONE_TEXT }]);
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
const wait = (path: string): Notice => ({ path, kind: "wait", title: path, body: "승인 대기" });
const done = (path: string): Notice => ({ path, kind: "done", title: path, body: DONE_TEXT });
deepStrictEqual(live([wait("C:\\w\\a1")], one, "all"), [wait("C:\\w\\a1")], "still waiting");
deepStrictEqual(live([wait("C:\\w\\a1")], empty, "all"), [], "answered meanwhile");
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
equal(summaryText(3), "에이전트 3개가 기다려요");

console.log("os-notify-check: ok");
