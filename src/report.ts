// A worker's done report: the `## 보고` block at the start of the answer that ends its turn (shared format,
// confirmed by the playbook rules on 2026-11-16). The same reading as the backend's `canvas::parse_report`
// (src-tauri/src/canvas.rs), checked by scripts/report-check.ts. docs/features/canvas/canvas-reading.md

import type { ChatItem, ChatState } from "./chatFold";

export type Check = { text: string; weak?: string };
export type Report = { status: "완료" | "부분" | "막힘"; commit?: string; verified: Check[]; unverified: string[]; changed: string[]; remaining?: string };

const STATUSES = ["완료", "부분", "막힘"] as const;
const KEYS = ["상태", "커밋", "검증", "미검증", "변경", "남은 일"];

/** `pat` in `hay` as a count of zero: no digit right before it (`0 passed`) or right after it (`통과 0`). */
const zeroCount = (hay: string, pat: string) => {
  for (let i = hay.indexOf(pat); i >= 0; i = hay.indexOf(pat, i + 1)) {
    const near = pat.startsWith("0") ? hay[i - 1] : hay[i + pat.length];
    if (!near || !/[0-9]/.test(near)) return true;
  }
  return false;
};

/** Why a verification doesn't count as a pass: no result after the first `→`, zero tests run, a test run's exit code alone. */
export function weakCheck(v: string): string | undefined {
  const at = v.indexOf("→");
  if (at < 0) return "결과 없음";
  const cmd = v.slice(0, at).trim().toLowerCase(), result = v.slice(at + 1).trim().toLowerCase();
  if (!result) return "결과 없음";
  if (["통과 0", "passed 0", "tests 0", "0 passed", "0 tests", "0 test", "0개 통과", "0개 실행"].some((p) => zeroCount(result, p)) || result.includes("테스트 0개")) return "테스트 0개";
  const testRun = ["test", "테스트", "jest", "spec"].some((t) => cmd.includes(t));
  const withoutExit = ["exit code 0", "exit 0", "종료 코드 0", "종료 0"].reduce((s, p) => s.replaceAll(p, ""), result);
  if (testRun && !/[0-9]/.test(withoutExit) && withoutExit.replace(/^[^\p{L}\p{N}]+|[^\p{L}\p{N}]+$/gu, "").length === 0) return "종료 코드만";
  return undefined;
}

/**
 * The first `## 보고` block, up to the first blank line or heading; null when there's none or its 상태 isn't exactly
 * 완료 / 부분 / 막힘. Unknown keys and lines that aren't `키: 값` are skipped; 검증 and 미검증 gather, the rest take
 * their first value.
 */
export function parseReport(text: string): Report | null {
  const lines = text.split("\n");
  const head = lines.findIndex((l) => l.trim() === "## 보고");
  if (head < 0) return null;
  const r: Omit<Report, "status"> = { verified: [], unverified: [], changed: [] };
  const first = new Map<string, string>();
  for (const raw of lines.slice(head + 1)) {
    const l = raw.trim();
    if (!l || l.startsWith("#")) break;
    const i = l.indexOf(":");
    if (i < 0) continue;
    const k = l.slice(0, i).trim(), v = l.slice(i + 1).trim();
    if (!KEYS.includes(k)) continue;
    if (k === "검증") {
      if (v && v !== "없음") r.verified.push({ text: v, ...(weakCheck(v) && { weak: weakCheck(v) }) });
    } else if (k === "미검증") {
      if (v) r.unverified.push(v);
    } else if (!first.has(k)) first.set(k, v);
  }
  const status = STATUSES.find((s) => s === first.get("상태"));
  if (!status) return null;
  const some = (k: string) => {
    const v = first.get(k);
    return v && v !== "없음" ? v : undefined;
  };
  const commit = some("커밋"), remaining = some("남은 일");
  r.changed = some("변경")?.split(",").map((f) => f.trim()).filter(Boolean) ?? [];
  return { status, ...(commit && { commit }), ...r, ...(remaining && { remaining }) };
}

/**
 * The card's one line about a report, and whether it needs a look: anything but 완료, 완료 without a verification,
 * a verification that doesn't count, something left unverified. No report is its own tone.
 */
export function reportSummary(r: Report | null): { text: string; tone: "ok" | "warn" | "none"; detail: string } {
  if (!r) return { text: "보고 없음", tone: "none", detail: "마지막 답변에 ## 보고 블록이 없거나 형식이 맞지 않아요 (상태: 완료 | 부분 | 막힘)" };
  const weak = r.verified.filter((c) => c.weak).length;
  const parts = [r.status, r.verified.length ? `검증 ${r.verified.length}` : "검증 없음"];
  if (weak) parts.push(`통과 아님 ${weak}`);
  if (r.unverified.length) parts.push(`미검증 ${r.unverified.length}`);
  if (r.commit) parts.push(r.commit);
  const tone = r.status !== "완료" || !r.verified.length || weak || r.unverified.length ? "warn" : "ok";
  const detail = [
    `상태: ${r.status}`,
    ...(r.commit ? [`커밋: ${r.commit}`] : []),
    ...r.verified.map((c) => `검증: ${c.text}${c.weak ? ` (통과 아님: ${c.weak})` : ""}`),
    ...r.unverified.map((u) => `미검증: ${u}`),
    ...(r.changed.length ? [`변경: ${r.changed.join(", ")}`] : []),
    ...(r.remaining ? [`남은 일: ${r.remaining}`] : []),
  ].join("\n");
  return { text: parts.join(" · "), tone, detail };
}

/**
 * The full text of the agent's latest answer (the report sits in it), or null when there's none yet or a message
 * of the user's came after it (a new turn: the old report no longer speaks for it).
 */
export function lastAnswer(s: Pick<ChatState, "turns" | "tail">): string | null {
  const pick = (items: readonly ChatItem[]): string | null | undefined => {
    for (let i = items.length - 1; i >= 0; i--) {
      const it = items[i];
      if (it.kind === "user") return null;
      if (it.kind === "text" && it.text.trim()) return it.text;
    }
    return undefined;
  };
  for (const items of [s.tail, ...[...s.turns].reverse()]) {
    const r = pick(items);
    if (r !== undefined) return r;
  }
  return null;
}
