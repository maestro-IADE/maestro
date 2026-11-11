import { useState } from "react";
import { ipc } from "../ipc";
import { AGENTS, AgentLogo, PERMISSIONS, agentUsable } from "../NewAgentDialog";
import { useSettings } from "../settings";
import { useStore, type Worktree } from "../store";

/** A worktree found in the repo (made outside Maestro) has no agent yet: pick one and start it here. */
export function AttachAgent({ wt, compact }: { wt: Worktree; compact?: boolean }) {
  const cli = useStore((s) => s.cli);
  const setError = useStore((s) => s.setError);
  const { defaultAgent, defaultPermission, disabledAgents, agentModels } = useSettings();
  const usable = AGENTS.filter((a) => agentUsable(a.id, cli, disabledAgents));
  const [agent, setAgent] = useState(usable.some((a) => a.id === defaultAgent) ? defaultAgent : (usable[0]?.id ?? AGENTS[0]?.id ?? ""));
  const [perm, setPerm] = useState(defaultPermission);
  const [busy, setBusy] = useState(false);

  const attach = () => {
    setBusy(true);
    ipc.attachAgent({ projectRoot: wt.root, worktreePath: wt.path, adapterId: agent, permissionMode: perm, model: agentModels[agent] ?? "" })
      .then(() => useStore.getState().refreshWorktrees(wt.root))
      .catch((e) => {
        setError(e);
        setBusy(false);
      });
  };

  return (
    <div className={compact ? "attach attach-compact nodrag" : "attach nodrag"}>
      {!compact && <b>이 워크트리에는 아직 에이전트가 없어요</b>}
      <span className="muted small">Maestro 밖에서 만든 워크트리예요. 에이전트를 붙이면 이 폴더에서 바로 실행돼요.</span>
      <div className="attach-row">
        <AgentLogo id={agent} size={16} />
        <select value={agent} onChange={(e) => setAgent(e.target.value)} disabled={busy}>
          {usable.map((a) => <option key={a.id} value={a.id}>{a.name}</option>)}
        </select>
        {!compact && (
          <select value={perm} onChange={(e) => setPerm(e.target.value)} disabled={busy}>
            {PERMISSIONS.map((p) => <option key={p.id} value={p.id}>{p.label}</option>)}
          </select>
        )}
        <button className="primary" disabled={busy || !usable.length} onClick={attach}>{busy ? "시작하는 중…" : "에이전트 붙이기"}</button>
      </div>
    </div>
  );
}
