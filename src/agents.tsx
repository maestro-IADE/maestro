import { useEffect, useState } from "react";
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

/** A permission mode as the UI names it: the full name, the agent header chip's short one, and what it means. */
export type Permission = { id: string; label: string; short: string; hint: string; warn?: boolean };

/**
 * Every permission mode by its id (docs/features/ui/terms.md "권한 모드"): its name and the short one an agent
 * header's chip shows. `warn`: drawn in the warning colour (모두 허용).
 */
export const PERMISSION_NAMES: Record<string, Omit<Permission, "id">> = {
  default: { label: "매번 묻기", short: "매번 묻기", hint: "파일 수정과 명령 실행을 모두 승인 카드로 물어봐요" },
  acceptEdits: { label: "파일 수정 자동 허용", short: "수정 자동", hint: "파일 수정은 묻지 않고 허용해요. 명령 실행 등 다른 도구는 승인 카드로 물어봐요" },
  auto: { label: "자동 판단", short: "자동 판단", hint: "에이전트가 안전하다고 판단한 작업은 묻지 않아요" },
  bypassPermissions: { label: "모두 허용 (주의)", short: "모두 허용", hint: "묻지 않고 모든 도구를 실행해요. 작업 폴더 밖 파일도 바뀔 수 있어요", warn: true },
  plan: { label: "계획만", short: "계획만", hint: "파일을 바꾸지 않고 계획만 세워요" },
};

/** A mode by its id; one this build doesn't name shows as its id. */
export const permissionOf = (id: string): Permission => ({ id, ...(PERMISSION_NAMES[id] ?? { label: id, short: id, hint: "" }) });

/** Every permission mode, in picker order (an agent takes only some of them: `permissionsFor`). */
export const PERMISSIONS: Permission[] = Object.keys(PERMISSION_NAMES).map(permissionOf);

/**
 * The modes agent `id` takes, in its catalog order (catalog.rs `permission_modes`): Antigravity has no 매번 묻기.
 * An agent the catalog doesn't know gets the whole list (the backend refuses what it can't take).
 */
export const permissionsFor = (id?: string): Permission[] => {
  const modes = agentDef(id)?.permissionModes;
  return modes?.length ? modes.map(permissionOf) : PERMISSIONS;
};

/** `wanted` if agent `id` takes it, else 파일 수정 자동 허용, else its first mode (a default that doesn't fit the agent). */
export const permissionFor = (id: string | undefined, wanted: string): string => {
  const list = permissionsFor(id);
  return (list.find((p) => p.id === wanted) ?? list.find((p) => p.id === "acceptEdits") ?? list[0])?.id ?? wanted;
};

/** The agent new agents start with: the settings' default when it can be used, else ("auto") the first usable one. */
export const defaultAgentOf = (defaultAgent: string, cli: CliStatus | null, disabled: string[]) =>
  agentUsable(defaultAgent, cli, disabled) ? defaultAgent : (AGENTS.find((a) => agentUsable(a.id, cli, disabled))?.id ?? AGENTS[0]?.id ?? "");

/**
 * Why agent `id` can't be picked for a new agent ("" when it can): not launchable yet, switched off, or its CLI
 * missing or too old. A login that looks missing doesn't block (see `agentUsable`).
 */
export const agentBlocker = (id: string, cli: CliStatus | null, disabled: string[]): "" | "준비 중" | "꺼 둠" | "설치 필요" | "버전 낮음" => {
  if (!agentLaunchable(id)) return "준비 중";
  if (disabled.includes(id)) return "꺼 둠";
  const st = cli?.agents.find((c) => c.id === id)?.status;
  return st === "missing" ? "설치 필요" : st === "tooOld" ? "버전 낮음" : "";
};

/** (value, label) models an agent offers (catalog: Claude's aliases, or Antigravity's live list). null while loading. */
export function useModels(id: string) {
  const [models, setModels] = useState<Array<[string, string]> | null>(null);
  useEffect(() => {
    setModels(null);
    if (!agentDef(id)) return setModels([]);
    let live = true;
    ipc.agentModels(id).then((m) => live && setModels(m.map((x) => [x.id, x.label]))).catch(() => live && setModels([]));
    return () => {
      live = false;
    };
  }, [id]);
  return models;
}

/**
 * The repo's branches (local, then remote-only). `pickable`: for checking a branch out, so branches already
 * checked out somewhere are disabled (git refuses them). A value the list doesn't have still shows.
 */
export function BranchSelect({ branches, value, onChange, pickable, disabled, className }: {
  branches: Branch[] | null;
  value: string;
  onChange: (name: string) => void;
  pickable?: boolean;
  disabled?: boolean;
  className?: string;
}) {
  const list = branches ?? [];
  return (
    <select className={className} value={value} disabled={!branches || disabled} onChange={(e) => onChange(e.target.value)}>
      {!list.some((b) => b.name === value) && <option value={value}>{branches ? value : "브랜치 읽는 중…"}</option>}
      {list.map((b) => (
        <option key={b.name} value={b.name} disabled={pickable && b.inUse}>
          {b.name}{b.remote ? " · 원격" : ""}{b.inUse ? " · 사용 중" : ""}
        </option>
      ))}
    </select>
  );
}
