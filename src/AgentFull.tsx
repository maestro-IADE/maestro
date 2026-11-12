import { AgentPanel } from "./nodes/AgentNode";
import { useSettings } from "./settings";

/**
 * An agent filling its tab (full screen), like the Claude Code app. Its header leaves out the name and state the
 * tab already shows (`inTab`). Full screen ↔ canvas is the tab bar's toggle (StageTabs).
 */
export function AgentFull({ path }: { path: string }) {
  const { fullView, set } = useSettings();
  return <AgentPanel className="agent-full" path={path} view={fullView} onView={(v) => set({ fullView: v })} inTab />;
}
