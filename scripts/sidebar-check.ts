// The sidebar's pure parts (src/sidebarModel.ts): row order and indent, the second line's priority, and the order
// Ctrl+Shift+↑/↓ walks. Run: `npm test`.
import { deepStrictEqual, equal } from "node:assert/strict";
import { agentKey, blockRepos, childCount, flatAgentOrder, sidebarBlocks, MAX_DEPTH, reasonText, rowMeta, stepPath, subtree, treeRows, visibleRows } from "../src/sidebarModel.ts";
import { attentionItems } from "../src/attention.ts";
import type { Approval, MergeRequest, TuiWait } from "../src/types.ts";

// what waits for the user, as the store holds it, turned into the 확인할 일 list the rows read
type WaitCtx = { approvals: Approval[]; tuiWaits: TuiWait[]; trustWaits?: string[]; merges: MergeRequest[] };
const items = (c: WaitCtx) => attentionItems({ trustWaits: [], ...c, prs: [], worktrees: [], repos: [] });

type W = { root: string; path: string; isMaestro: boolean; workers: string[]; adapterId?: string; external?: boolean };
const wt = (path: string, o: Partial<W> = {}): W => ({ root: "R", path, isMaestro: false, workers: [], adapterId: "claude-code", ...o });
const none: WaitCtx = { approvals: [], tuiWaits: [], merges: [] };
const approval = (worktreePath: string, toolName = "Bash") => ({ id: 1, worktreePath, toolName, toolInput: {}, canAlways: false }) as Approval;
const tui = (worktreePath: string) => ({ worktreePath, source: "cli" }) as TuiWait;
const merge = (maestroPath: string, workerPath: string, id = 1) => ({ id, root: "R", maestroPath, workerPath, workerName: workerPath, branch: "b", base: "main", fileCount: 1, verified: null }) as MergeRequest;

// a: the hierarchy (Maestro → w1 → w2 → w3 → w4), an agent nobody leads with a worker, listed out of order
const wts = [
  wt("C:\\p\\solo"),
  wt("C:\\p\\w3", { workers: ["C:\\p\\w4"] }),
  wt("C:\\p\\w1", { workers: ["c:/p/w2/"] }), // another spelling of the same folder
  wt("C:\\p\\w4"),
  wt("C:\\p\\solo-kid"),
  wt("C:\\p", { isMaestro: true, workers: ["C:\\p\\w1"] }),
  wt("C:\\p\\w2", { workers: ["C:\\p\\w3"] }),
];
wts[0].workers = ["C:\\p\\solo-kid"];
const rows = treeRows(wts);
deepStrictEqual(rows.map((r) => r.w.path), ["C:\\p", "C:\\p\\w1", "C:\\p\\w2", "C:\\p\\w3", "C:\\p\\w4", "C:\\p\\solo", "C:\\p\\solo-kid"], "the Maestro first, its workers depth first, then the rest");
deepStrictEqual(rows.map((r) => r.depth), [0, 1, 2, 3, 4, 0, 1]);
equal(MAX_DEPTH, 3);
deepStrictEqual(rows.map((r) => r.shown), [0, 1, 2, 3, 3, 0, 1], "drawn no deeper than three levels");
deepStrictEqual(rows.map((r) => r.loose), [false, false, false, false, false, true, true], "independent agents (and their workers) after the Maestro's tree");

// a2: an agent's subtree, its own workers (the count chip), and folding one away
deepStrictEqual(subtree(rows, 1).map((r) => r.w.path), ["C:\\p\\w2", "C:\\p\\w3", "C:\\p\\w4"], "w1's workers and theirs, not the agents after them");
deepStrictEqual(subtree(rows, 4), [], "the last of a branch has none");
deepStrictEqual(subtree(rows, 5).map((r) => r.w.path), ["C:\\p\\solo-kid"]);
deepStrictEqual(rows.map((_, i) => childCount(rows, i)), [1, 1, 1, 1, 0, 1, 0], "one level down only");
deepStrictEqual(visibleRows(rows, []).length, rows.length, "nothing folded");
deepStrictEqual(visibleRows(rows, [agentKey("C:\\p\\w1\\")]).map((r) => r.w.path), ["C:\\p", "C:\\p\\w1", "C:\\p\\solo", "C:\\p\\solo-kid"], "w1 folded: its whole subtree goes, the next tree stays");
deepStrictEqual(visibleRows(rows, [agentKey("C:\\p\\w3"), agentKey("C:\\p")]).map((r) => r.w.path), ["C:\\p", "C:\\p\\solo", "C:\\p\\solo-kid"], "a fold inside a folded one changes nothing");
deepStrictEqual(visibleRows(rows, [agentKey("C:\\p\\w4")]).length, rows.length, "an agent with no workers folds nothing");

// b: no Maestro → no 독립 에이전트 subhead; a broken (cyclic) link still lists every agent once
const plain = treeRows([wt("a", { workers: ["b"] }), wt("b", { workers: ["a"] }), wt("c")]);
deepStrictEqual(plain.map((r) => r.loose), [false, false, false]);
deepStrictEqual(plain.map((r) => r.w.path).sort(), ["a", "b", "c"]);

// c: what the second line says, in priority order
const m = wt("C:\\p", { isMaestro: true, workers: ["C:\\p\\w1", "C:\\p\\w2"] });
const ctxAll: WaitCtx = { approvals: [approval("c:/p", "Edit")], tuiWaits: [tui("C:\\p\\")], merges: [merge("C:\\p", "C:\\p\\w1"), merge("C:/p", "C:\\p\\w2", 2)] };
deepStrictEqual(rowMeta(m, items(ctxAll)), { line2: "approval", tool: "Edit", mergeCount: 2 }, "an approval comes first");
equal(rowMeta(m, items({ ...ctxAll, approvals: [] })).line2, "tui", "then a terminal question");
equal(rowMeta(m, items({ ...ctxAll, approvals: [], trustWaits: ["c:/p/"] })).line2, "tui", "a terminal question over the trust question");
equal(rowMeta(m, items({ ...ctxAll, approvals: [], tuiWaits: [], trustWaits: ["c:/p/"] })).line2, "trust", "then the trust question");
equal(rowMeta(m, items({ ...ctxAll, approvals: [], tuiWaits: [] })).line2, "merge", "then the Maestro's merge requests");
equal(rowMeta(wt("C:\\p\\w1"), items({ ...none, trustWaits: ["C:/p/w2"] })).line2, "normal", "another agent's trust question isn't this row's");
deepStrictEqual(rowMeta(m, items(none)), { line2: "normal", mergeCount: 0 });
equal(rowMeta(wt("C:\\p\\w1"), items({ ...none, merges: [merge("C:\\p", "C:\\p\\w1")] })).line2, "normal", "a worker's own merge request is the Maestro's to show");
equal(rowMeta(wt("x", { external: true, adapterId: undefined }), items({ ...none, approvals: [approval("x")] })).line2, "noAgent", "an outside worktree with no agent");
equal(rowMeta(wt("x"), items({ ...none, approvals: [approval("x", "")] })).tool, undefined);

equal(reasonText(rowMeta(m, items(ctxAll))), "승인 대기 · Edit");
equal(reasonText({ line2: "approval", mergeCount: 0 }), "승인 대기");
equal(reasonText({ line2: "tui", mergeCount: 0 }), "터미널 질문");
equal(reasonText({ line2: "trust", mergeCount: 0 }), "신뢰 대기");
equal(reasonText({ line2: "merge", mergeCount: 3 }), "병합 요청 3");
equal(reasonText({ line2: "noAgent", mergeCount: 0 }), "에이전트 연결 필요");
equal(reasonText({ line2: "normal", mergeCount: 0 }), null);

// d: Ctrl+Shift+↑/↓ walks projects in list order, each in tree order, and skips folded projects
const repos = [{ root: "A" }, { root: "B" }, { root: "C" }];
const all = [wt("a2", { root: "A" }), wt("a1", { root: "A", isMaestro: true, workers: ["a2"] }), wt("b1", { root: "B" }), wt("c1", { root: "C" })];
deepStrictEqual(flatAgentOrder(repos, all, []), ["a1", "a2", "b1", "c1"]);
deepStrictEqual(flatAgentOrder(repos, all, ["B"]), ["a1", "a2", "c1"], "a folded project is skipped");
deepStrictEqual(flatAgentOrder(repos, all, [], [agentKey("A1")]), ["a1", "b1", "c1"], "workers folded under an agent are skipped");
const order = flatAgentOrder(repos, all, []);
equal(stepPath(order, "a2", 1), "b1");
equal(stepPath(order, "A2", -1), "a1", "paths compare without case");
equal(stepPath(order, "c1", 1), null, "stops at the end");
equal(stepPath(order, "a1", -1), null, "stops at the start");
equal(stepPath(order, null, 1), "a1", "nothing open: down starts at the first");
equal(stepPath(order, "gone", -1), "c1", "not in the order: up starts at the last");
equal(stepPath([], null, 1), null);

// e: groups gather their projects where the first one is listed; empty groups last; an unknown group stands alone
const gA = { root: "C:\\work" };
const gB = { root: "D:/team" };
const rr = [
  { root: "x", name: "x" },
  { root: "C:/work/a", group: "c:/work/" },
  { root: "y", group: null },
  { root: "C:/work/b", group: "C:\\work" },
  { root: "z", group: "E:/gone" },
];
const blocks = sidebarBlocks(rr, [gA, gB]);
deepStrictEqual(
  blocks.map((b) => (b.kind === "repo" ? b.repo.root : `[${b.group.root}: ${b.repos.map((r) => r.root).join(", ")}]`)),
  ["x", "[C:\\work: C:/work/a, C:/work/b]", "y", "z", "[D:/team: ]"],
);
deepStrictEqual(blockRepos(blocks).map((r) => r.root), ["x", "C:/work/a", "C:/work/b", "y", "z"], "the projects in screen order");
deepStrictEqual(sidebarBlocks([], []), []);

console.log("ok sidebar");
