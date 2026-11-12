import { ipc } from "./ipc";
import type { AgentDef, Branch, CliAgentStatus, CliStatus } from "./types";

// The agent catalog and the small pieces every screen that names or picks an agent shares (moved out of
// NewAgentDialog.tsx so the sidebar, settings, cards and tabs don't import a dialog).

// Brand marks: Claude and OpenAI from simple-icons, Antigravity from the icon its installer ships.
const LOGOS = import.meta.glob<string>("./assets/agents/*", { eager: true, import: "default" });

/** The backend's agent catalog (catalog.rs), loaded once before the first render (main.tsx). */
export let AGENTS: AgentDef[] = [];
export async function loadAgents() {
  AGENTS = await ipc.agentCatalog();
}

export const agentDef = (id?: string) => AGENTS.find((a) => a.id === id);

/** Can run as a worktree agent now (has a runtime); Codex doesn't yet. */
export const agentLaunchable = (id?: string) => !!agentDef(id)?.runtimes.length;

export const agentLabel = (id?: string) => agentDef(id)?.name ?? id ?? "";

export const AgentLogo = ({ id, size = 16 }: { id?: string; size?: number }) => {
  const a = agentDef(id);
  const src = a && LOGOS[`./assets/agents/${a.logo}`];
  return src ? <img className="agent-logo" src={src} width={size} height={size} alt="" /> : null;
};

/** 미설치 / 버전 낮음 / 로그인 필요 / 준비됨 */
export const STATUS_LABEL: Record<CliAgentStatus, string> = { missing: "설치 필요", tooOld: "버전 낮음", loginRequired: "로그인 필요", ready: "준비됨" };

/**
 * Can be picked for a new worktree: launchable, installed in a version that works, and not switched off in
 * settings. A login that looks missing doesn't block (it's read from files and can be wrong); the start says so.
 */
export const agentUsable = (id: string, cli: CliStatus | null, disabled: string[]) => {
  const st = cli?.agents.find((c) => c.id === id)?.status;
  return agentLaunchable(id) && (st === "ready" || st === "loginRequired") && !disabled.includes(id);
};

/** Permission modes a new agent can start with (docs/features/ui/terms.md "권한 모드"). */
export const PERMISSIONS = [
  { id: "acceptEdits", label: "파일 수정만 자동 허용", hint: "파일 수정은 묻지 않고 허용해요. 명령 실행 등 다른 도구는 승인 카드로 물어봐요." },
  { id: "auto", label: "자동 판단", hint: "Claude가 안전하다고 판단한 작업을 자동으로 허용해요" },
  { id: "bypassPermissions", label: "모두 허용 (주의)", hint: "확인 없이 모든 명령을 실행해요. 격리된 워크트리라도 컴퓨터 전체에 영향을 줄 수 있어요." },
  { id: "plan", label: "계획만", hint: "파일을 바꾸지 않고 계획만 세워요" },
];

/**
 * The repo's branches (local, then remote-only). `pickable`: for checking a branch out, so branches already
 * checked out somewhere are disabled (git refuses them). A value the list doesn't have still shows.
 */
export function BranchSelect({ branches, value, onChange, pickable, className }: {
  branches: Branch[] | null;
  value: string;
  onChange: (name: string) => void;
  pickable?: boolean;
  className?: string;
}) {
  const list = branches ?? [];
  return (
    <select className={className} value={value} disabled={!branches} onChange={(e) => onChange(e.target.value)}>
      {!list.some((b) => b.name === value) && <option value={value}>{branches ? value : "브랜치 읽는 중…"}</option>}
      {list.map((b) => (
        <option key={b.name} value={b.name} disabled={pickable && b.inUse}>
          {b.name}{b.remote ? " · 원격" : ""}{b.inUse ? " · 사용 중" : ""}
        </option>
      ))}
    </select>
  );
}
