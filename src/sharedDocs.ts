import { ask } from "./ui/Confirm";

/** What a file on the 문서·런북 boards is (docs/features/orchestration/library-and-personas.md). */
export type SharedKind = "runbook" | "persona" | "doc";

/** A file of Maestro's own (`~/.maestro`: the runbooks, the personas, the 문서 categories) by its path; null for any other. */
export function sharedKind(path: string): SharedKind | null {
  const p = path.replace(/\\/g, "/").toLowerCase();
  if (p.includes("/.maestro/playbooks/")) return "runbook";
  if (p.includes("/.maestro/personas/")) return "persona";
  if (p.includes("/.maestro/library/")) return "doc";
  return null;
}

const WHAT: Record<SharedKind, { title: string; body: string }> = {
  runbook: { title: "모든 프로젝트의 Lead가 따르는 런북이에요", body: "고친 절차는 Lead가 다음에 이 런북을 읽을 때부터 따라요" },
  persona: { title: "모든 프로젝트가 같이 쓰는 페르소나예요", body: "이 역할을 맡은 에이전트는 다음 시작부터 고친 내용을 따라요" },
  doc: { title: "모든 프로젝트가 같이 쓰는 문서예요", body: "이 문서를 참조하는 에이전트는 모두 고친 내용을 읽어요" },
};

// files already warned about while the app runs (lower-cased paths)
const warned = new Set<string>();

/**
 * Before editing a shared runbook, persona or document: who reads it, asked once per file while the app runs.
 * `builtin`: a file Maestro ships, which 기본값으로 can put back.
 */
export async function okToEdit(path: string, kind: SharedKind, builtin: boolean): Promise<boolean> {
  const key = path.replace(/\\/g, "/").toLowerCase();
  if (warned.has(key)) return true;
  const w = WHAT[kind];
  const ok = await ask({ title: w.title, body: builtin ? `${w.body}\n기본 파일이라 기본값으로 되돌릴 수 있어요` : w.body, confirm: "편집" });
  if (ok) warned.add(key);
  return ok;
}

const CHANGED = "maestro:docs-changed";
/** A 문서·런북 file was saved or put back in this window: the lists read again. */
export const docsChanged = () => void window.dispatchEvent(new Event(CHANGED));
/** Calls `f` whenever a 문서·런북 file changes in this window; returns the unsubscribe. */
export function onDocsChanged(f: () => void) {
  window.addEventListener(CHANGED, f);
  return () => window.removeEventListener(CHANGED, f);
}
