// 확인할 일 (src/attention.ts): what goes in the list, its order, path matching, the counts, which PRs need a hand,
// and where Ctrl+J goes. Run: `npm test`.
import { deepStrictEqual, equal } from "node:assert/strict";
import { attentionItems, blockingCount, ciOf, countByRoot, groupByRoot, nextItem, prNeedsHand, samePath, waitsAt, type AttentionSource } from "../src/attention.ts";
import type { Approval, MergeRequest, PullRequest, TuiWait } from "../src/types.ts";

const approval = (id: number, worktreePath: string, toolName = "Bash", toolInput: any = { command: "npm test" }) =>
  ({ id, worktreePath, toolName, toolInput, canAlways: false }) as Approval;
const tui = (worktreePath: string, toolName: string | null = null) => ({ worktreePath, source: "cli", toolName }) as TuiWait;
const merge = (id: number, root: string, maestroPath: string, workerPath: string, verified: string | null = "npm test 통과") =>
  ({ id, root, maestroPath, workerPath, workerName: `워커 ${id}`, branch: `agent-${id}`, base: "main", fileCount: 7, verified, requestedAt: id }) as MergeRequest;
const pr = (o: Partial<PullRequest> & { checks?: Partial<PullRequest["checks"]> } = {}): PullRequest =>
  ({
    root: "A",
    maestroPath: "C:\\A",
    workerPath: "C:\\w\\a1",
    workerName: "a1",
    number: 12,
    state: "OPEN",
    isDraft: false,
    queued: false,
    mergeAttempt: null,
    mergeable: "MERGEABLE",
    mergeStateStatus: "CLEAN",
    reviewDecision: null,
    headSha: "h1",
    hidden: false,
    ...o,
    checks: { state: "success", passed: 1, failed: 0, pending: 0, failedJobs: [], headSha: "h1", ...o.checks },
  }) as PullRequest;

const repos = [{ root: "A" }, { root: "B" }];
const worktrees = [
  { root: "A", path: "C:\\A", name: "Maestro" },
  { root: "A", path: "C:\\w\\a1", name: "로그인 고치기" },
  { root: "B", path: "C:\\B\\wt\\b1", name: "api 테스트" },
];
const src = (o: Partial<AttentionSource>): AttentionSource => ({ approvals: [], tuiWaits: [], merges: [], prs: [], worktrees, repos, ...o });

// a: each kind is collected, with its name, root and lines
const all = attentionItems(
  src({
    // B's approval came first, but A is first in the sidebar
    approvals: [approval(2, "c:/b/wt/b1/", "Edit", { file_path: "src/a.ts" }), approval(3, "C:\\w\\a1")],
    tuiWaits: [tui("C:\\w\\a1", "Bash")],
    merges: [merge(5, "A", "C:\\A", "C:\\w\\a1")],
    prs: [pr({ checks: { state: "failure", failed: 2 } })],
  }),
);
deepStrictEqual(all.map((i) => i.id), ["approval:3", "tui:c:/w/a1", "merge:5", "pr:c:/w/a1", "approval:2"], "project (sidebar order) → kind → arrival");
deepStrictEqual(all.map((i) => i.root), ["A", "A", "A", "A", "B"]);
equal(all[0].name, "로그인 고치기");
equal(all[0].title, "Bash");
equal(all[0].detail, "npm test", "an approval's one-line summary");
equal(all[0].tool, "Bash");
equal(all[4].root, "B", "a path spelled another way still finds its agent");
equal(all[4].name, "api 테스트");
equal(all[4].detail, "src/a.ts");
equal(all[1].title, "터미널에서 답을 기다려요 · Bash");
equal(all[2].name, "워커 5", "a merge request shows its worker");
equal(all[2].path, "C:\\A", "and opens the Maestro");
equal(all[2].title, "agent-5 → main · 파일 7 · 검증 있음");
equal(attentionItems(src({ merges: [merge(6, "A", "C:\\A", "C:\\w\\a1", null)] }))[0].title, "agent-6 → main · 파일 7 · 검증 없음");
equal(all[3].title, "PR #12 · CI 실패");
equal(all[3].failed, true);
equal(all[3].path, "C:\\A", "a PR opens the Maestro");
equal(attentionItems(src({ prs: [pr({ maestroPath: "" })] }))[0].path, "C:\\w\\a1", "with its Maestro gone, the worker");
equal(attentionItems(src({ prs: [pr()] }))[0].title, "PR #12 · 병합할 수 있어요");

// b: arrival order within a kind; an agent nobody knows yet goes last, under no project
const order = attentionItems(src({ approvals: [approval(9, "C:\\w\\a1"), approval(4, "C:\\w\\a1"), approval(1, "D:\\elsewhere")] }));
deepStrictEqual(order.map((i) => i.approvalId), [9, 4, 1]);
equal(order[2].root, "");
equal(order[2].name, "elsewhere");
// a Maestro's own folder is its project's root, even before its agent is listed
equal(attentionItems(src({ worktrees: [], repos: [{ root: "C:\\A" }], approvals: [approval(1, "c:/a/")] }))[0].root, "C:\\A");

// c: paths compare without case or trailing / back slashes
equal(samePath("C:\\a\\b", "c:/a/b/"), true);
equal(samePath("C:\\a\\b", "C:\\a\\bc"), false);
equal(waitsAt(all, "c:/w/a1/"), true);
equal(waitsAt(all, "C:\\w\\none"), false);
equal(waitsAt(attentionItems(src({ prs: [pr()] })), "C:\\A"), false, "a PR doesn't keep anyone waiting");

// d: counts
equal(blockingCount(all), 4, "PRs don't count as blocking");
deepStrictEqual(countByRoot(all), { A: 4, B: 1 });
deepStrictEqual(groupByRoot(all).map((g) => [g.root, g.items.length]), [["A", 4], ["B", 1]]);

// e: which PRs need a hand
equal(prNeedsHand(pr({ checks: { state: "failure" } })), true, "open + CI failed");
equal(prNeedsHand(pr({ checks: { state: "pending" } })), false, "open + CI pending");
equal(prNeedsHand(pr()), true, "open + green + mergeable");
equal(prNeedsHand(pr({ state: "MERGED" })), false, "merged");
equal(prNeedsHand(pr({ state: "MERGED", checks: { state: "failure" } })), false, "merged, even with a red CI");
equal(prNeedsHand(pr({ hidden: true, checks: { state: "failure" } })), false, "dismissed");
equal(prNeedsHand(pr({ isDraft: true })), false, "a draft can't be merged");
equal(prNeedsHand(pr({ mergeable: "CONFLICTING" })), false);
equal(prNeedsHand(pr({ reviewDecision: "REVIEW_REQUIRED" })), false);
equal(prNeedsHand(pr({ mergeAttempt: { method: "merge", seenHead: "h1", at: 1 } })), false, "a merge click under way");
equal(ciOf(pr({ checks: { state: "failure", headSha: "old" } })), "pending", "checks of another commit count as pending");
equal(prNeedsHand(pr({ checks: { state: "failure", headSha: "old" } })), false);

// f: Ctrl+J: the item after the current agent's, round to the start
equal(nextItem([], "x"), undefined);
equal(nextItem(all)?.id, "approval:3", "no agent on screen: the first");
equal(nextItem(all, "C:\\B\\nothing")?.id, "approval:3", "an agent with nothing: the first");
equal(nextItem(all, "c:/w/a1")?.id, "merge:5", "past the current agent's items");
equal(nextItem(all, "C:\\A")?.id, "approval:2", "the Maestro's merge and PR, then the next project");
equal(nextItem(all, "C:\\B\\wt\\b1")?.id, "approval:3", "from the last, round to the start");

console.log("ok attention");
