// 설정 → 프로젝트 → 워크트리 준비: the "복사할 파일" field as text (comma separated) and back
// (docs/features/worktrees/worktree-setup.md). Pure; `scripts/prep-check.ts` checks it in `npm test`.

/** The patterns typed in the field: split on commas, trimmed, empty ones dropped, each once. */
export function parseCopyFiles(text: string): string[] {
  const out: string[] = [];
  for (const p of text.split(",").map((s) => s.trim())) if (p && !out.includes(p)) out.push(p);
  return out;
}

/** The saved patterns as the field shows them. */
export const copyFilesText = (list: string[]) => list.join(", ");

/** Whether the field says something other than what is saved (a blur without an edit writes nothing). */
export const copyFilesChanged = (text: string, saved: string[]) => parseCopyFiles(text).join("\n") !== saved.join("\n");
