// OS 알림 시험's cases (src/devNotify.ts): what each would send through the app's rules (src/osNotify.ts), at each
// setting, so the developer test keeps showing what it says it shows. Run: `npm test`.
import { deepStrictEqual, equal } from "node:assert/strict";
import { CHAT_QUESTION, NAMES, notifyCases } from "../src/devNotify.ts";
import { attentionItems, type AttentionItem } from "../src/attention.ts";
import { askText, coalesce, DONE_TEXT, live, notices, summaryText, SUMMARY_TITLE, type NotifyAgent, type OsNotify } from "../src/osNotify.ts";
import { reasonText, rowMeta } from "../src/sidebarModel.ts";

// the app's words (useOsNotify's `reason`) and its pipeline (useOsNotify's flush)
const reason = (a: Pick<NotifyAgent, "path" | "isMaestro">, items: AttentionItem[]) => reasonText(rowMeta({ ...a, adapterId: "asking" }, items));
const P = "maestro";
const cases = notifyCases(P, attentionItems, askText);
const sends = (title: string, level: OsNotify) => {
  const c = cases.find((x) => x.title === title);
  if (!c) throw new Error(`no case ${title}`);
  return coalesce(live(notices(c.before, c.after, level, reason), c.after, level));
};
const one = (who: string, body: string) => [{ title: `${who} · ${P}`, body }];

equal(cases.length, 8);
equal(new Set(cases.map((c) => c.title)).size, cases.length, "titles are unique");

// "모두": every wait and the real end; nothing for a worker's end or a Lead whose workers still run; a burst folds
deepStrictEqual(sends("승인 기다림", "all"), one(NAMES.solo, "승인 대기 · Bash"));
deepStrictEqual(sends("터미널 질문", "all"), one(NAMES.solo, "터미널 질문"));
deepStrictEqual(sends("병합 요청", "all"), one(NAMES.lead, "병합 요청 1"));
deepStrictEqual(sends("채팅 선택지", "all"), one(NAMES.solo, `질문 · ${CHAT_QUESTION}`));
deepStrictEqual(sends("작업 끝", "all"), one(NAMES.solo, DONE_TEXT));
deepStrictEqual(sends("워커의 작업 끝", "all"), []);
deepStrictEqual(sends("워커가 도는 동안 Lead 쉼", "all"), []);
deepStrictEqual(sends("여럿이 한꺼번에", "all"), [{ title: SUMMARY_TITLE, body: summaryText(2, 1) }]);
equal(summaryText(2, 1), "기다림 2 · 끝남 1");

// "기다릴 때만": the waits alone (the burst is two waits then); "끔": nothing at all
deepStrictEqual(sends("승인 기다림", "waiting"), one(NAMES.solo, "승인 대기 · Bash"));
deepStrictEqual(sends("작업 끝", "waiting"), []);
deepStrictEqual(sends("여럿이 한꺼번에", "waiting"), [{ title: SUMMARY_TITLE, body: "에이전트 2개가 기다려요" }]);
for (const c of cases) deepStrictEqual(sends(c.title, "off"), [], c.title);

console.log("ok dev notify");
