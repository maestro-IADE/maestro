// The settings' agent rows (AgentsSection.tsx): one state per agent, the CLI's and its chat runtime's together.
// Pure (types only) so scripts/settings-check.ts can run it under Node.
import type { AcpStatus, CliAgent } from "../types";

export type AgentRowLabel = "확인 중" | "준비 중" | "설치 필요" | "버전 낮음" | "로그인 필요" | "채팅 런타임 필요" | "사용 중" | "꺼짐";
/** ok: in use; warn: something to do before it works as it should; muted: nothing to do (off, checking, not yet) */
export type AgentRowState = { label: AgentRowLabel; tone: "ok" | "warn" | "muted" };

/**
 * Does the chat runtime keep this agent's chat from running on it: set to 자동 (the terminal-only choice doesn't
 * want it) and not ready. Agents without one (`acp` null) never need it.
 */
export const chatRuntimeNeeded = (acp: AcpStatus | null | undefined) => !!acp && acp.mode === "auto" && acp.state !== "ready";

/**
 * One agent's state, first match wins: 확인 중 (no CLI status yet) → 준비 중 (no runtime in this build) → 설치 필요
 * → 버전 낮음 → 로그인 필요 → 채팅 런타임 필요 (Claude Code: the chat runtime isn't ready while 채팅 연결 is 자동)
 * → 사용 중 / 꺼짐 (`on`: usable and switched on).
 */
export function agentRowState(st: CliAgent | undefined, launchable: boolean, on: boolean): AgentRowState {
  if (!st) return { label: "확인 중", tone: "muted" };
  if (!launchable) return { label: "준비 중", tone: "muted" };
  if (st.status === "missing") return { label: "설치 필요", tone: "warn" };
  if (st.status === "tooOld") return { label: "버전 낮음", tone: "warn" };
  if (st.status === "loginRequired") return { label: "로그인 필요", tone: "warn" };
  if (chatRuntimeNeeded(st.acp)) return { label: "채팅 런타임 필요", tone: "warn" };
  return on ? { label: "사용 중", tone: "ok" } : { label: "꺼짐", tone: "muted" };
}

/** What the chat runtime line under an agent's row can do now (one button at most). */
export type AcpAction = "install" | "reinstall" | "recheck" | "needNode" | null;

/**
 * The chat runtime line's button: 설치 when it isn't installed, 다시 설치 when the install is broken, 다시 확인 when
 * it's blocked for this run (failed starts, a crash loop), and none when Node.js is missing or too old (the line
 * says which version) or when nothing is to be done.
 */
export function acpAction(acp: AcpStatus): AcpAction {
  if (acp.state === "nodeMissing" || acp.state === "nodeTooOld") return "needNode";
  if (acp.state === "notInstalled") return "install";
  if (acp.state === "broken") return "reinstall";
  if (acp.state === "blocked") return "recheck";
  return null;
}

/** The chat runtime in a few words for the line under an agent's row: 준비됨, or why not (the backend's message). */
export const acpShort = (acp: AcpStatus) => (acp.state === "ready" ? "준비됨" : acp.message || "준비되지 않았어요");

/** The chat runtime in full, for the row's 관리 part: version, Node.js, the development path. */
export function acpLine(a: AcpStatus) {
  if (a.state !== "ready") return a.message;
  const where = a.dev ? " (개발용 경로)" : "";
  return `준비됨 · 어댑터 ${a.adapterVersion ?? "?"}${where}${a.node ? ` · Node.js ${a.node}` : ""}`;
}

/** Settings sections, in nav order; an id the dialog doesn't know opens the first. */
export const SECTION_IDS = ["general", "agents", "projects", "look", "about"] as const;
export type SectionId = (typeof SECTION_IDS)[number];
export const sectionOf = (id: string | undefined): SectionId => (SECTION_IDS as readonly string[]).includes(id ?? "") ? (id as SectionId) : "general";
