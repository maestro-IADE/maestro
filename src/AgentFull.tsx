import { AgentPanel } from "./nodes/AgentNode";

/**
 * An agent filling its tab (full screen), like the Claude Code app. Its header leaves out the name and state the
 * tab already shows (`inTab`). Full screen ↔ canvas is the tab bar's toggle (StageTabs). Chat or terminal is the
 * app's 에이전트 보기 unless this agent has its own for now (`useAgentView`, inside the panel).
 */
export function AgentFull({ path }: { path: string }) {
  return <AgentPanel className="agent-full" path={path} inTab />;
}
