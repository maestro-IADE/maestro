// The new agent and agent settings dialogs' pure parts (NewAgentDialog.tsx, AgentSettingsDialog.tsx). No React, no
// store, no Tauri: scripts/agent-form-check.ts tests them. Docs: docs/features/agents/agent-settings.md,
// docs/features/worktrees/create-worktree.md.
import type { Branch, UpdateAgentReq } from "./types";

/** The first `agent-N` (from `from` up) that no branch of the repo has yet, local or remote. */
export const nextAgentBranch = (branches: Branch[] | null, from: number): string => {
  let n = Math.max(1, Math.floor(from));
  while (branches?.some((b) => b.name === `agent-${n}`)) n++;
  return `agent-${n}`;
};

/** A new branch name the repo already has (local, or remote-only): git would refuse it, or it would shadow one. */
export const branchTaken = (branches: Branch[] | null, name: string): boolean => {
  const n = name.trim();
  return !!n && !!branches?.some((b) => b.name === n);
};

/** The name a new agent takes from its 할 일 when it has none: the first non-empty line, at most 40 characters. */
export const nameFromTask = (task: string): string => {
  const line = task.split(/\r?\n/).map((l) => l.trim()).find(Boolean) ?? "";
  return line.length > 40 ? `${line.slice(0, 39).trimEnd()}…` : line;
};

/** What the agent settings dialog edits, as text (an empty name: the branch's; an empty model: the CLI's default). */
/**
 * The base branch a new Maestro is made with: "" (the repo's default, read again by the backend and not checked)
 * while the field still holds the default the dialog filled in. That default can be one `create_maestro` would
 * refuse as not a local branch: "HEAD" in a detached checkout with no origin, or the origin's default branch when
 * it isn't checked out here.
 */
export const maestroBaseArg = (base: string, repoDefault: string): string => {
  const b = base.trim();
  return b === repoDefault.trim() ? "" : b;
};

export type AgentFields = { name: string; description: string; permissionMode: string; model: string; baseBranch: string };
export type AgentPatch = Omit<UpdateAgentReq, "projectRoot" | "worktreePath">;

/**
 * The `update_agent` patch from what the agent had (`was`) to the dialog's fields (`now`): only what changed
 * (the backend checks only that). The Maestro's name is fixed and only it has a base branch. `restart`: a running
 * agent takes it only on a restart (the permission mode, the model, the Maestro's base branch in its brief).
 */
export function agentPatch(was: AgentFields, now: AgentFields, maestro: boolean): { patch: AgentPatch; changed: boolean; restart: boolean } {
  const patch: AgentPatch = {};
  if (!maestro && now.name.trim() !== was.name.trim()) patch.name = now.name.trim();
  if (now.description.trim() !== was.description.trim()) patch.description = now.description.trim();
  if (now.permissionMode !== was.permissionMode) patch.permissionMode = now.permissionMode;
  if (now.model !== was.model) patch.model = now.model;
  if (maestro && now.baseBranch !== was.baseBranch) patch.baseBranch = now.baseBranch;
  const restart = patch.permissionMode !== undefined || patch.model !== undefined || patch.baseBranch !== undefined;
  return { patch, changed: Object.keys(patch).length > 0, restart };
}
