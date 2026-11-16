// A worker's done report (src/report.ts; mirrors canvas::parse_report's tests in src-tauri/src/canvas.rs). Run: `npm test`.
import { deepStrictEqual, equal } from "node:assert/strict";
import { lastAnswer, parseReport, reportSummary, weakCheck } from "../src/report.ts";

// the confirmed block: it ends at the first blank line
const full = parseReport(
  "## 보고\n상태: 완료\n커밋: 3f2a9c1\n검증: cargo test → 통과 42, 실패 0, 건너뜀 0 (종료 0)\n검증: npm run build → 종료 0\n" +
    "미검증: 좁은 창 레이아웃 — 화면을 볼 수 없음\n변경: src/a.ts, src/b.ts\n남은 일: 없음\n\n긴 설명이 이어져요.\n상태: 막힘\n",
)!;
equal(full.status, "완료");
equal(full.commit, "3f2a9c1");
deepStrictEqual(full.verified, [{ text: "cargo test → 통과 42, 실패 0, 건너뜀 0 (종료 0)" }, { text: "npm run build → 종료 0" }]);
deepStrictEqual(full.unverified, ["좁은 창 레이아웃 — 화면을 볼 수 없음"]);
deepStrictEqual(full.changed, ["src/a.ts", "src/b.ts"]);
equal(full.remaining, undefined, "없음 means nothing left");

// the first block; unknown keys and plain lines skipped; first value wins
const first = parseReport("앞말\n  ## 보고  \n상태: 부분\n커밋: 없음\n담당: 나\n그냥 한 줄\n검증: 없음\n변경: 없음\n남은 일: 문서\n남은 일: 두 번째\n# 다음\n\n## 보고\n상태: 막힘\n")!;
equal(first.status, "부분");
equal(first.commit, undefined);
deepStrictEqual(first.verified, [], "검증: 없음 is no verification");
deepStrictEqual(first.changed, []);
equal(first.remaining, "문서");

// missing or broken: no report, never guessed from prose
equal(parseReport("다 됐어요! 테스트도 통과했어요."), null);
equal(parseReport("## 보고\n검증: x → y\n"), null, "no 상태");
equal(parseReport("## 보고\n상태: 완료했어요\n"), null, "상태 is exactly one of the three");
equal(parseReport("### 보고\n상태: 완료\n"), null, "only ## 보고 opens a report");
equal(parseReport("## 보고\n\n상태: 완료\n"), null, "a blank line ends it");

equal(weakCheck("cargo test"), "결과 없음");
equal(weakCheck("cargo test → "), "결과 없음");
equal(weakCheck("pytest → 0 passed in 0.1s"), "테스트 0개");
equal(weakCheck("cargo test → 통과 0, 건너뜀 12"), "테스트 0개");
equal(weakCheck("cargo test → 종료 0"), "종료 코드만");
equal(weakCheck("cargo test → 10 passed"), undefined, "10 is not 0");
equal(weakCheck("cargo test → 통과 10"), undefined);
equal(weakCheck("npm run build → 종료 0"), undefined);

// the card's line
equal(reportSummary(null).tone, "none");
equal(reportSummary(null).text, "보고 없음");
const s = reportSummary(full);
equal(s.text, "완료 · 검증 2 · 미검증 1 · 3f2a9c1");
equal(s.tone, "warn", "something left unverified needs a look (and 완료 with it breaks the rule)");
equal(reportSummary(parseReport("## 보고\n상태: 완료\n검증: cargo test → 12 passed\n")!).tone, "ok");
equal(reportSummary(parseReport("## 보고\n상태: 완료\n")!).text, "완료 · 검증 없음");
equal(reportSummary(parseReport("## 보고\n상태: 완료\n")!).tone, "warn", "완료 without a verification");
equal(reportSummary(parseReport("## 보고\n상태: 부분\n검증: npm test → 0 passed\n")!).text, "부분 · 검증 1 · 통과 아님 1");

// the latest answer, whole; a newer message of the user's means no answer yet
const text = (t: string) => ({ kind: "text", id: t, text: t }) as never;
const user = (t: string) => ({ kind: "user", id: t, via: "prompt", text: t, files: [] }) as never;
const answered = "## 보고\n상태: 완료\n\n첫 답";
equal(lastAnswer({ turns: [[user("a"), text(answered)]], tail: [] }), answered);
equal(lastAnswer({ turns: [[text("옛 답")]], tail: [user("다시 해")] }), null);
equal(lastAnswer({ turns: [], tail: [text("앞"), { kind: "tool", id: "t", name: "Bash", input: {} } as never, text("  ")] }), "앞");
equal(lastAnswer({ turns: [], tail: [] }), null);

console.log("ok report");
