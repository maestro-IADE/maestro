// Pure helpers for the right panel's change rows and the PR card's remembered merge method (no React, no Tauri:
// scripts/right-panel-check.ts runs them under Node).

export type RightTab = "files" | "changes";
export const RIGHT_TAB_KEY = "maestro.rightTab";

/** A stored tab name, or the files tab for anything else (a missing or unknown value). */
export const parseRightTab = (v: string | null | undefined): RightTab => (v === "changes" ? "changes" : "files");

/** "src/nodes/App.tsx" → name "App.tsx", folder "src/nodes" (git paths use "/"; a trailing slash is dropped). */
export function splitPath(rel: string): { name: string; dir: string } {
  const p = rel.replace(/\\/g, "/").replace(/\/+$/, "");
  const i = p.lastIndexOf("/");
  return i < 0 ? { name: p, dir: "" } : { name: p.slice(i + 1), dir: p.slice(0, i) };
}

/** What the change row's letter box shows: the letter, its colour group and the tooltip word. */
export type ChangeCode = { letter: string; kind: "add" | "mod" | "del" | "conflict"; label: string };

/**
 * A `git status --porcelain=v1` XY code as one letter: untracked "U", added "A", deleted "D", renamed "R", copied
 * "C", type changed "T", modified "M", and "!" for an unmerged (conflicted) file. The index side wins when both
 * sides changed ("AM" → A), as the file is what was added.
 */
export function changeCode(xy: string): ChangeCode {
  const c = xy.trim();
  if (c === "??") return { letter: "U", kind: "add", label: "추적 안 됨" };
  if (c.length === 2 && (c.includes("U") || c === "AA" || c === "DD")) return { letter: "!", kind: "conflict", label: "충돌" };
  switch (c[0]) {
    case "A": return { letter: "A", kind: "add", label: "추가" };
    case "D": return { letter: "D", kind: "del", label: "삭제" };
    case "R": return { letter: "R", kind: "mod", label: "이름 바뀜" };
    case "C": return { letter: "C", kind: "add", label: "복사" };
    case "T": return { letter: "T", kind: "mod", label: "형식 바뀜" };
    case "M": return { letter: "M", kind: "mod", label: "수정" };
    default: return { letter: c[0] ?? "?", kind: "mod", label: "변경" };
  }
}

export type PrMethod = "merge" | "squash";

/** localStorage key of a project's last PR merge method: the root folded like `samePath` (case, slashes). */
export const prMethodKey = (root: string) => `maestro.prMethod.${root.replace(/\\/g, "/").replace(/\/+$/, "").toLowerCase()}`;

/** The remembered method when the repo still allows it, else the first allowed one (merge before squash). */
export function pickPrMethod(saved: string | null | undefined, allowed: { merge: boolean; squash: boolean }): PrMethod {
  if ((saved === "merge" || saved === "squash") && allowed[saved]) return saved;
  if (allowed.merge) return "merge";
  if (allowed.squash) return "squash";
  // neither (a readiness answer that allows nothing): merge, and GitHub's refusal is shown on the card
  return saved === "squash" ? "squash" : "merge";
}
