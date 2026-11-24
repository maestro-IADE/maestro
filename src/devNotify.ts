// OS 알림 시험 (docs/features/ui/dev-tools.md, the "dev 모드" menu): the changes an OS notification is made of, played
// on a stand-in team with no agent and no model. Each case is two looks at the store (`before`, `after`); DevTools.tsx
// puts them through the app's own rules (osNotify.ts `notices` → `live` → `coalesce`, as useOsNotify's flush does), so
// what goes out is what the app would send, and sends that as a real OS notification. Pure, and only `import type`
// (Node runs scripts/dev-notify-check.ts without a bundler): the store's list of what waits comes in as `items`.
import type { AttentionItem, AttentionSource } from "./attention";
import type { Asked, NotifyAgent, NotifySnap } from "./osNotify";
import type { AgentStatus, Approval, MergeRequest, TuiWait } from "./types";

/** One case: its name, what it shows (or why nothing goes), and the change it is made of. */
export type NotifyCase = { title: string; see: string; before: NotifySnap; after: NotifySnap };

/** attention.ts `attentionItems`: what waits on the user, from the store's lists. */
export type ItemsOf = (src: AttentionSource) => AttentionItem[];

// the stand-in team: a Lead with two workers, and an agent on its own. Paths no real agent has.
const ROOT = "/__maestro_notify_test__";
const LEAD = `${ROOT}/lead`;
const W1 = `${ROOT}/w1`;
const W2 = `${ROOT}/w2`;
const SOLO = `${ROOT}/solo`;
export const NAMES = { lead: "시험 Lead", w1: "로그인 고치기", w2: "api 테스트", solo: "혼자" } as const;
/** The chat choice case 채팅 선택지 asks (its notice says `askText` of it). */
export const CHAT_QUESTION = "어느 쪽으로 고칠까요?";

type Change = {
  status?: Partial<Record<string, AgentStatus>>;
  approvals?: string[];
  tuiWaits?: string[];
  trustWaits?: string[];
  merges?: number;
  asks?: Asked[];
};

const team = (s: Change["status"] = {}): NotifyAgent[] => [
  { path: LEAD, name: NAMES.lead, root: ROOT, status: s[LEAD] ?? "RUNNING", isMaestro: true, workers: [W1, W2] },
  { path: W1, name: NAMES.w1, root: ROOT, status: s[W1] ?? "RUNNING", isMaestro: false, workers: [] },
  { path: W2, name: NAMES.w2, root: ROOT, status: s[W2] ?? "RUNNING", isMaestro: false, workers: [] },
  { path: SOLO, name: NAMES.solo, root: ROOT, status: s[SOLO] ?? "RUNNING", isMaestro: false, workers: [] },
];

/** One look at the stand-in team, in a project named `project` (the titles say it). */
function snap(project: string, items: ItemsOf, c: Change = {}): NotifySnap {
  const agents = team(c.status);
  const projects = [{ root: ROOT, name: project }];
  const approvals = (c.approvals ?? []).map((path, i) => ({ id: 9000 + i, worktreePath: path, toolName: "Bash", toolInput: {}, canAlways: false }) as unknown as Approval);
  const tuiWaits = (c.tuiWaits ?? []).map((path) => ({ worktreePath: path, source: "cli", toolName: null }) as unknown as TuiWait);
  const merges = Array.from(
    { length: c.merges ?? 0 },
    (_, i) => ({ id: 9100 + i, root: ROOT, maestroPath: LEAD, workerPath: W1, workerName: NAMES.w1, branch: "w1", base: "main", fileCount: 3, verified: null, requestedAt: i }) as unknown as MergeRequest,
  );
  return {
    items: items({ approvals, tuiWaits, trustWaits: c.trustWaits ?? [], merges, prs: [], worktrees: agents as never, repos: projects as never }),
    asks: c.asks ?? [],
    agents,
    projects,
    held: [],
  };
}

/**
 * The cases, their titles naming project `project` (the one the window is on, or a stand-in name). `askText` is
 * osNotify.ts's words for a question.
 */
export function notifyCases(project: string, items: ItemsOf, askText: (title: string) => string): NotifyCase[] {
  const s = (c?: Change) => snap(project, items, c);
  const chat: Asked = { id: "dev-notify-ask", path: SOLO, text: askText(CHAT_QUESTION), chat: true };
  return [
    { title: "승인 기다림", see: "에이전트가 도구를 쓰려고 승인을 기다려요.", before: s(), after: s({ approvals: [SOLO] }) },
    { title: "터미널 질문", see: "터미널로 보는 에이전트가 답을 기다려요.", before: s(), after: s({ tuiWaits: [SOLO] }) },
    { title: "프로젝트 신뢰 기다림", see: "클론하거나 열어 온 프로젝트에서 워커가 신뢰 확인을 기다려요.", before: s(), after: s({ trustWaits: [W1] }) },
    { title: "병합 요청", see: "워커가 병합을 부탁해서 Lead가 기다려요.", before: s(), after: s({ merges: 1 }) },
    { title: "채팅 선택지", see: "에이전트가 채팅에 선택지를 내고 답을 기다려요.", before: s(), after: s({ asks: [chat] }) },
    { title: "작업 끝", see: "혼자 일하던 에이전트의 차례가 끝났어요. 설정이 \"모두\"일 때만 가요.", before: s(), after: s({ status: { [SOLO]: "IDLE" } }) },
    { title: "워커의 작업 끝", see: "워커의 끝은 Lead가 받아요. 알림이 없어야 맞아요.", before: s(), after: s({ status: { [W1]: "IDLE" } }) },
    { title: "워커가 도는 동안 Lead 쉼", see: "Lead는 워커의 보고가 오면 이어 가요. 알림이 없어야 맞아요.", before: s(), after: s({ status: { [LEAD]: "IDLE" } }) },
    { title: "여럿이 한꺼번에", see: "2초 안에 여럿이 생기면 알림 하나로 묶여요.", before: s(), after: s({ approvals: [W1], tuiWaits: [W2], status: { [SOLO]: "IDLE" } }) },
  ];
}
